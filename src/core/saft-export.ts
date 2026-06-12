import { createHash } from "node:crypto";
import { mkdirSync, statSync, writeFileSync } from "node:fs";
import { basename, join, relative } from "node:path";
import type { Database } from "bun:sqlite";
import { insertAuditLog } from "./actor";
import { isValidIsoDate as looksLikeIsoDate } from "./dates";
import { formatAmount, roundDkk, sumDkk } from "./money";

type CanonicalJson = null | boolean | number | string | CanonicalJson[] | { [key: string]: CanonicalJson };

type ExportedFileMeta = {
  path: string;
  sha256: string;
  sizeBytes: number;
};

type AccountRow = {
  accountNo: string;
  name: string;
  type: string;
  normalBalance: string;
  defaultVatCode: string | null;
};

type JournalLineRow = {
  accountNo: string;
  accountName: string;
  debitAmount: number;
  creditAmount: number;
  vatCode: string | null;
  text: string | null;
};

type JournalEntryRow = {
  id: number;
  entryNo: string;
  transactionDate: string;
  registrationDatetime: string;
  text: string;
  currency: string;
  amountDkk: number | null;
  documentId: number | null;
  lines: JournalLineRow[];
};

type SalesInvoiceRow = {
  documentNo: string;
  invoiceNo: string;
  issueDate: string;
  dueDate: string | null;
  currency: string;
  sellerName: string | null;
  sellerVatOrCvr: string | null;
  buyerName: string | null;
  buyerVatOrCvr: string | null;
  netAmount: number | null;
  vatAmount: number | null;
  grossAmount: number | null;
  payloadJson: string | null;
};

type CompanyRow = {
  id: number;
  name: string;
  country: string;
  currency: string;
  cvr: string | null;
  fiscalYearStartMonth: number;
  fiscalYearLabelStrategy: string;
};

type InvoiceLinePayload = {
  description?: string;
  quantity?: number;
  unitPriceExVat?: number;
  lineTotalExVat?: number;
};

export type ExportSaftPackageInput = {
  periodStart: string;
  periodEnd: string;
  outputDir: string;
  generatedAt?: string;
};

export type ExportSaftPackageResult = {
  ok: boolean;
  exportDir?: string;
  manifestPath?: string;
  saftXmlPath?: string;
  generatedAt?: string;
  periodStart?: string;
  periodEnd?: string;
  journalEntryCount?: number;
  salesInvoiceCount?: number;
  purchaseInvoiceCount?: number;
  customerCount?: number;
  supplierCount?: number;
  appliedRules: string[];
  errors: string[];
};

const RULE_ID = "DK-BOOKKEEPING-SAFT-EXPORT-001";

// ===== Third deterministic slice: customer + supplier master files =====
// Built on top of slice 2 (ledger + sales + purchases + VAT summary +
// document references). Slice 3 adds the MasterFiles/Customers and
// MasterFiles/Suppliers sections from the company's own customer/vendor
// stamdata, so a SAF-T reader can resolve a TaxRegistrationNumber on a
// purchase or sales invoice back to a master record. The profile identifier
// is bumped to v3 and stays explicit so the export is never mistaken for a
// complete/generic SAF-T implementation.
const PROFILE_ID = "rentemester-dk-saft-v3-ledger-sales-purchases-masterfiles";
const AUDIT_FILE_VERSION = "3.0";
const OUT_OF_SCOPE = [
  "bank_statement_transport",
  "payments_collections",
  "fixed_assets",
  "stock_movements",
  "official_schema_submission",
] as const;

type PurchaseInvoiceRow = {
  documentNo: string;
  invoiceNo: string | null;
  invoiceDate: string | null;
  supplierName: string | null;
  supplierVatOrCvr: string | null;
  currency: string;
  netAmount: number | null;
  vatAmount: number | null;
  grossAmount: number | null;
};

type VatSummaryRow = {
  taxCode: string;
  debitAmount: number;
  creditAmount: number;
  lineCount: number;
};

type CustomerRow = {
  customerId: number;
  name: string;
  vatOrCvr: string | null;
  address: string | null;
  email: string | null;
  phone: string | null;
  defaultCurrency: string;
  paymentTermsDays: number;
};

type SupplierRow = {
  supplierId: number;
  name: string;
  vatOrCvr: string | null;
  address: string | null;
  email: string | null;
  phone: string | null;
  defaultExpenseAccount: string | null;
  defaultVatTreatment: string | null;
};

function resolveIsoDateTime(value?: string) {
  if (!value) return null;
  const parsed = Date.parse(value);
  if (Number.isNaN(parsed)) return null;
  return new Date(parsed).toISOString();
}

function normalizeExportTimestamp(periodEnd: string) {
  return `${periodEnd}T23:59:59.000Z`;
}

function packageName(periodStart: string, periodEnd: string, generatedAt: string) {
  return `saft-export-${periodStart}_${periodEnd}_${generatedAt.replace(/[-:]/g, "").replace(/\.\d{3}Z$/, "Z")}`;
}

function packageRelativePath(exportDir: string, path: string) {
  return relative(exportDir, path).replace(/\\/g, "/");
}

function sha256Text(text: string) {
  return createHash("sha256").update(text).digest("hex");
}

function canonicalize(value: unknown): CanonicalJson {
  if (value === null || typeof value === "boolean" || typeof value === "number" || typeof value === "string") return value;
  if (Array.isArray(value)) return value.map((entry) => canonicalize(entry));
  if (typeof value === "object") {
    const entries = Object.entries(value as Record<string, unknown>)
      .filter(([, entry]) => entry !== undefined)
      .sort(([a], [b]) => a.localeCompare(b));
    return Object.fromEntries(entries.map(([key, entry]) => [key, canonicalize(entry)]));
  }
  return String(value);
}

function stringifyCanonicalJson(value: unknown) {
  return `${JSON.stringify(canonicalize(value), null, 2)}\n`;
}

function recordWrittenText(exportDir: string, path: string, text: string, outputs: ExportedFileMeta[]) {
  writeFileSync(path, text);
  outputs.push({
    path: packageRelativePath(exportDir, path),
    sha256: sha256Text(text),
    sizeBytes: statSync(path).size,
  });
}

function escapeXml(value: string | number | null | undefined) {
  return String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&apos;");
}

function xmlTag(name: string, value: string | number | null | undefined, indent = "") {
  if (value == null || value === "") return "";
  return `${indent}<${name}>${escapeXml(value)}</${name}>`;
}

function money(value: number | null | undefined) {
  return formatAmount(value);
}

function fetchCompany(db: Database): CompanyRow | null {
  const row = db.query(
    `SELECT id, name, country, currency, cvr, fiscal_year_start_month, fiscal_year_label_strategy
     FROM companies ORDER BY id ASC LIMIT 1`
  ).get() as any;
  if (!row) return null;
  return {
    id: row.id,
    name: row.name,
    country: row.country,
    currency: row.currency,
    cvr: row.cvr ?? null,
    fiscalYearStartMonth: row.fiscal_year_start_month,
    fiscalYearLabelStrategy: row.fiscal_year_label_strategy,
  };
}

function fetchAccounts(db: Database): AccountRow[] {
  return (db.query(
    `SELECT account_no, name, type, normal_balance, default_vat_code
     FROM accounts ORDER BY account_no ASC`
  ).all() as any[]).map((row) => ({
    accountNo: row.account_no,
    name: row.name,
    type: row.type,
    normalBalance: row.normal_balance,
    defaultVatCode: row.default_vat_code ?? null,
  }));
}

function fetchJournalEntries(db: Database, periodStart: string, periodEnd: string): JournalEntryRow[] {
  const entries = db.query(
    `SELECT id, entry_no, transaction_date, registration_datetime, text, currency, amount_dkk, document_id
     FROM journal_entries
     WHERE transaction_date BETWEEN ? AND ?
     ORDER BY transaction_date ASC, id ASC`
  ).all(periodStart, periodEnd) as any[];

  const linesStmt = db.query(
    `SELECT a.account_no, a.name AS account_name, jl.debit_amount, jl.credit_amount, jl.vat_code, jl.text
     FROM journal_lines jl
     JOIN accounts a ON a.id = jl.account_id
     WHERE jl.journal_entry_id = ?
     ORDER BY jl.id ASC`
  );

  return entries.map((entry) => ({
    id: entry.id,
    entryNo: entry.entry_no,
    transactionDate: entry.transaction_date,
    registrationDatetime: entry.registration_datetime,
    text: entry.text,
    currency: entry.currency,
    amountDkk: entry.amount_dkk == null ? null : Number(entry.amount_dkk),
    documentId: entry.document_id ?? null,
    lines: (linesStmt.all(entry.id) as any[]).map((line) => ({
      accountNo: line.account_no,
      accountName: line.account_name,
      debitAmount: Number(line.debit_amount),
      creditAmount: Number(line.credit_amount),
      vatCode: line.vat_code ?? null,
      text: line.text ?? null,
    })),
  }));
}

function fetchSalesInvoices(db: Database, periodStart: string, periodEnd: string): SalesInvoiceRow[] {
  return (db.query(
    `SELECT document_no, invoice_no, invoice_date, currency, sender_name, sender_vat_cvr,
            recipient_name, recipient_vat_cvr, amount_inc_vat, vat_amount, payload_json
     FROM documents
     WHERE document_type = 'issued_invoice'
       AND invoice_date BETWEEN ? AND ?
     ORDER BY invoice_date ASC, id ASC`
  ).all(periodStart, periodEnd) as any[]).map((row) => {
    const payload = row.payload_json ? JSON.parse(row.payload_json) as { dueDate?: string; totals?: { netAmount?: number; grossAmount?: number }; lines?: InvoiceLinePayload[] } : null;
    return {
      documentNo: row.document_no,
      invoiceNo: row.invoice_no,
      issueDate: row.invoice_date,
      dueDate: payload?.dueDate ?? null,
      currency: row.currency ?? "DKK",
      sellerName: row.sender_name ?? null,
      sellerVatOrCvr: row.sender_vat_cvr ?? null,
      buyerName: row.recipient_name ?? null,
      buyerVatOrCvr: row.recipient_vat_cvr ?? null,
      netAmount: payload?.totals?.netAmount == null ? null : Number(payload.totals.netAmount),
      vatAmount: row.vat_amount == null ? null : Number(row.vat_amount),
      grossAmount: payload?.totals?.grossAmount == null ? (row.amount_inc_vat == null ? null : Number(row.amount_inc_vat)) : Number(payload.totals.grossAmount),
      payloadJson: row.payload_json ?? null,
    };
  });
}

// ===== Purchase-side records for the second slice (#127) =====
// Purchase invoices are read directly from deterministic `documents` rows
// (document_type = 'purchase_sale'); no derived or speculative fields are added.
function fetchPurchaseInvoices(db: Database, periodStart: string, periodEnd: string): PurchaseInvoiceRow[] {
  return (db.query(
    `SELECT document_no, invoice_no, invoice_date, supplier_name, sender_vat_cvr,
            currency, amount_inc_vat, vat_amount
     FROM documents
     WHERE document_type = 'purchase_sale'
       AND invoice_date BETWEEN ? AND ?
     ORDER BY invoice_date ASC, document_no ASC, id ASC`
  ).all(periodStart, periodEnd) as any[]).map((row) => {
    const gross = row.amount_inc_vat == null ? null : Number(row.amount_inc_vat);
    const vat = row.vat_amount == null ? null : Number(row.vat_amount);
    return {
      documentNo: row.document_no,
      invoiceNo: row.invoice_no ?? null,
      invoiceDate: row.invoice_date ?? null,
      supplierName: row.supplier_name ?? null,
      supplierVatOrCvr: row.sender_vat_cvr ?? null,
      currency: row.currency ?? "DKK",
      netAmount: gross == null || vat == null ? null : roundDkk(gross - vat),
      vatAmount: vat,
      grossAmount: gross,
    };
  });
}

// ===== Customer + supplier master files for the third slice =====
// Both tables are read in full (active + archived), so a SAF-T reader can
// resolve every TaxRegistrationNumber referenced by a sales or purchase
// invoice in the period, not only the still-active parties.
function fetchCustomers(db: Database): CustomerRow[] {
  return (db.query(
    `SELECT id, name, vat_or_cvr, address, email, phone, default_currency, payment_terms_days
     FROM customers ORDER BY id ASC`,
  ).all() as any[]).map((row) => ({
    customerId: Number(row.id),
    name: row.name,
    vatOrCvr: row.vat_or_cvr ?? null,
    address: row.address ?? null,
    email: row.email ?? null,
    phone: row.phone ?? null,
    defaultCurrency: row.default_currency ?? "DKK",
    paymentTermsDays: Number(row.payment_terms_days ?? 30),
  }));
}

function fetchSuppliers(db: Database): SupplierRow[] {
  return (db.query(
    `SELECT id, name, vat_or_cvr, address, email, phone, default_expense_account, default_vat_treatment
     FROM vendors ORDER BY id ASC`,
  ).all() as any[]).map((row) => ({
    supplierId: Number(row.id),
    name: row.name,
    vatOrCvr: row.vat_or_cvr ?? null,
    address: row.address ?? null,
    email: row.email ?? null,
    phone: row.phone ?? null,
    defaultExpenseAccount: row.default_expense_account ?? null,
    defaultVatTreatment: row.default_vat_treatment ?? null,
  }));
}

// ===== VAT summary aggregated from deterministic journal-line tax codes (#127) =====
function summarizeVat(journalEntries: JournalEntryRow[]): VatSummaryRow[] {
  const byCode = new Map<string, VatSummaryRow>();
  for (const entry of journalEntries) {
    for (const line of entry.lines) {
      if (!line.vatCode) continue;
      const existing = byCode.get(line.vatCode) ?? { taxCode: line.vatCode, debitAmount: 0, creditAmount: 0, lineCount: 0 };
      existing.debitAmount = roundDkk(existing.debitAmount + line.debitAmount);
      existing.creditAmount = roundDkk(existing.creditAmount + line.creditAmount);
      existing.lineCount += 1;
      byCode.set(line.vatCode, existing);
    }
  }
  return [...byCode.values()].sort((a, b) => a.taxCode.localeCompare(b.taxCode));
}

function buildReadme(input: { periodStart: string; periodEnd: string; generatedAt: string }) {
  return [
    "Rentemester SAF-T export (third deterministic slice)",
    "",
    "This is a bounded, profile-scoped export. It is NOT a complete or generic",
    "SAF-T implementation and must not be treated as one. See ProfileID below.",
    "",
    `Profile: ${PROFILE_ID}`,
    `AuditFileVersion: ${AUDIT_FILE_VERSION}`,
    `Period: ${input.periodStart}..${input.periodEnd}`,
    `Generated at: ${input.generatedAt}`,
    "",
    "Included:",
    "- Header/company metadata",
    "- Chart of accounts",
    "- General ledger entries with lines and source-document references",
    "- Sales invoices issued in the selected period",
    "- Purchase invoices (purchase_sale documents) dated in the selected period",
    "- VAT summary aggregated from journal-line tax codes",
    "",
    "Out of scope in this slice:",
    ...OUT_OF_SCOPE.map((item) => `- ${item}`),
    "",
  ].join("\n");
}

// ===== Resolve a journal entry's document_id to its document number (#127) =====
// document_id is already part of deterministic core ledger state; this only
// surfaces an explicit, stable reference, never inventing data.
function fetchDocumentNumbers(db: Database, ids: number[]): Map<number, string> {
  const map = new Map<number, string>();
  const stmt = db.query(`SELECT document_no FROM documents WHERE id = ?`);
  for (const id of ids) {
    const row = stmt.get(id) as { document_no: string } | null;
    if (row) map.set(id, row.document_no);
  }
  return map;
}

function renderSaftXml(input: {
  company: CompanyRow;
  accounts: AccountRow[];
  customers: CustomerRow[];
  suppliers: SupplierRow[];
  journalEntries: JournalEntryRow[];
  salesInvoices: SalesInvoiceRow[];
  purchaseInvoices: PurchaseInvoiceRow[];
  vatSummary: VatSummaryRow[];
  documentNumbersById: Map<number, string>;
  periodStart: string;
  periodEnd: string;
  generatedAt: string;
  companyRoot: string;
}) {
  const invoiceTotals = sumDkk(input.salesInvoices.map((invoice) => invoice.grossAmount));
  const purchaseTotals = sumDkk(input.purchaseInvoices.map((invoice) => invoice.grossAmount));
  const journalBlocks = input.journalEntries.map((entry) => [
    "      <Transaction>",
    xmlTag("TransactionID", entry.entryNo, "        "),
    xmlTag("TransactionDate", entry.transactionDate, "        "),
    xmlTag("Description", entry.text, "        "),
    // ===== Document reference linking the transaction to its source document (#127) =====
    ...(entry.documentId != null && input.documentNumbersById.has(entry.documentId)
      ? [
          "        <DocumentReference>",
          xmlTag("SourceDocumentID", input.documentNumbersById.get(entry.documentId), "          "),
          "        </DocumentReference>",
        ]
      : []),
    ...entry.lines.flatMap((line) => [
      "        <Line>",
      xmlTag("AccountID", line.accountNo, "          "),
      xmlTag("AccountDescription", line.accountName, "          "),
      xmlTag("DebitAmount", money(line.debitAmount), "          "),
      xmlTag("CreditAmount", money(line.creditAmount), "          "),
      xmlTag("TaxCode", line.vatCode, "          "),
      xmlTag("LineDescription", line.text, "          "),
      "        </Line>",
    ].filter(Boolean)),
    "      </Transaction>",
  ].filter(Boolean).join("\n")).join("\n");

  const invoiceBlocks = input.salesInvoices.map((invoice) => {
    const payload = invoice.payloadJson ? JSON.parse(invoice.payloadJson) as { lines?: InvoiceLinePayload[] } : null;
    const lineBlocks = (payload?.lines ?? []).map((line, index) => [
      "        <Line>",
      xmlTag("LineNumber", index + 1, "          "),
      xmlTag("Description", line.description ?? null, "          "),
      // Quantity is NOT a monetary amount — render it as-is, never through the
      // 2-decimal øre money formatter (which would turn 3 into "3.00" and round
      // a fractional 1.005 to "1.01"). Only a real number is emitted (mirroring
      // the Peppol sibling's `typeof === "number"` guard), so a malformed
      // non-number from a hand-edited payload cannot leak into the export.
      xmlTag("Quantity", typeof line.quantity === "number" ? line.quantity : null, "          "),
      xmlTag("UnitPrice", formatAmount(line.unitPriceExVat), "          "),
      xmlTag("CreditAmount", formatAmount(line.lineTotalExVat), "          "),
      "        </Line>",
    ].filter(Boolean).join("\n")).join("\n");

    return [
      "      <Invoice>",
      xmlTag("InvoiceNo", invoice.invoiceNo, "        "),
      xmlTag("DocumentNo", invoice.documentNo, "        "),
      xmlTag("IssueDate", invoice.issueDate, "        "),
      xmlTag("DueDate", invoice.dueDate, "        "),
      xmlTag("CustomerName", invoice.buyerName, "        "),
      xmlTag("CustomerTaxID", invoice.buyerVatOrCvr, "        "),
      xmlTag("SellerName", invoice.sellerName, "        "),
      xmlTag("SellerTaxID", invoice.sellerVatOrCvr, "        "),
      xmlTag("DocumentCurrencyCode", invoice.currency, "        "),
      xmlTag("NetTotal", money(invoice.netAmount), "        "),
      xmlTag("TaxPayable", money(invoice.vatAmount), "        "),
      xmlTag("GrossTotal", money(invoice.grossAmount), "        "),
      lineBlocks,
      "      </Invoice>",
    ].filter(Boolean).join("\n");
  }).join("\n");

  // ===== Purchase-invoice blocks for the second slice (#127) =====
  const purchaseBlocks = input.purchaseInvoices.map((invoice) => [
    "      <Invoice>",
    xmlTag("InvoiceNo", invoice.invoiceNo, "        "),
    xmlTag("DocumentNo", invoice.documentNo, "        "),
    xmlTag("InvoiceDate", invoice.invoiceDate, "        "),
    xmlTag("SupplierName", invoice.supplierName, "        "),
    xmlTag("SupplierTaxID", invoice.supplierVatOrCvr, "        "),
    xmlTag("DocumentCurrencyCode", invoice.currency, "        "),
    xmlTag("NetTotal", money(invoice.netAmount), "        "),
    xmlTag("TaxPayable", money(invoice.vatAmount), "        "),
    xmlTag("GrossTotal", money(invoice.grossAmount), "        "),
    "      </Invoice>",
  ].filter(Boolean).join("\n")).join("\n");

  // ===== VAT-summary blocks for the second slice (#127) =====
  const vatSummaryBlocks = input.vatSummary.map((row) => [
    "      <TaxCodeDetail>",
    xmlTag("TaxCode", row.taxCode, "        "),
    xmlTag("DebitAmount", money(row.debitAmount), "        "),
    xmlTag("CreditAmount", money(row.creditAmount), "        "),
    xmlTag("NumberOfLines", row.lineCount, "        "),
    "      </TaxCodeDetail>",
  ].filter(Boolean).join("\n")).join("\n");

  return [
    '<?xml version="1.0" encoding="UTF-8"?>',
    `<AuditFile xmlns="urn:rentemester:dk:saft:v1" sourceCompanyRoot="${escapeXml(basename(input.companyRoot))}">`,
    "  <Header>",
    xmlTag("ProfileID", PROFILE_ID, "    "),
    xmlTag("AuditFileVersion", AUDIT_FILE_VERSION, "    "),
    xmlTag("CompanyName", input.company.name, "    "),
    xmlTag("RegistrationNumber", input.company.cvr, "    "),
    xmlTag("TaxRegistrationNumber", input.company.cvr, "    "),
    xmlTag("Country", input.company.country, "    "),
    xmlTag("DefaultCurrencyCode", input.company.currency, "    "),
    xmlTag("FiscalYearStartMonth", input.company.fiscalYearStartMonth, "    "),
    xmlTag("FiscalYearLabelStrategy", input.company.fiscalYearLabelStrategy, "    "),
    xmlTag("SelectionStartDate", input.periodStart, "    "),
    xmlTag("SelectionEndDate", input.periodEnd, "    "),
    xmlTag("DateCreated", input.generatedAt, "    "),
    "  </Header>",
    "  <MasterFiles>",
    "    <GeneralLedgerAccounts>",
    ...input.accounts.map((account) => [
      "      <Account>",
      xmlTag("AccountID", account.accountNo, "        "),
      xmlTag("AccountDescription", account.name, "        "),
      xmlTag("AccountType", account.type, "        "),
      xmlTag("NormalBalance", account.normalBalance, "        "),
      xmlTag("TaxCode", account.defaultVatCode, "        "),
      "      </Account>",
    ].filter(Boolean).join("\n")),
    "    </GeneralLedgerAccounts>",
    "    <Customers>",
    ...input.customers.map((customer) => [
      "      <Customer>",
      xmlTag("CustomerID", customer.customerId, "        "),
      xmlTag("CustomerName", customer.name, "        "),
      xmlTag("CustomerTaxID", customer.vatOrCvr, "        "),
      xmlTag("Address", customer.address, "        "),
      xmlTag("Email", customer.email, "        "),
      xmlTag("Telephone", customer.phone, "        "),
      xmlTag("DefaultCurrencyCode", customer.defaultCurrency, "        "),
      xmlTag("PaymentTermsDays", customer.paymentTermsDays, "        "),
      "      </Customer>",
    ].filter(Boolean).join("\n")),
    "    </Customers>",
    "    <Suppliers>",
    ...input.suppliers.map((supplier) => [
      "      <Supplier>",
      xmlTag("SupplierID", supplier.supplierId, "        "),
      xmlTag("SupplierName", supplier.name, "        "),
      xmlTag("SupplierTaxID", supplier.vatOrCvr, "        "),
      xmlTag("Address", supplier.address, "        "),
      xmlTag("Email", supplier.email, "        "),
      xmlTag("Telephone", supplier.phone, "        "),
      xmlTag("DefaultExpenseAccount", supplier.defaultExpenseAccount, "        "),
      xmlTag("DefaultVatTreatment", supplier.defaultVatTreatment, "        "),
      "      </Supplier>",
    ].filter(Boolean).join("\n")),
    "    </Suppliers>",
    "  </MasterFiles>",
    "  <GeneralLedgerEntries>",
    "    <Journal>",
    xmlTag("JournalID", "GENERAL", "      "),
    xmlTag("Description", "Rentemester general journal", "      "),
    journalBlocks,
    "    </Journal>",
    "  </GeneralLedgerEntries>",
    "  <SourceDocuments>",
    "    <SalesInvoices>",
    xmlTag("NumberOfEntries", input.salesInvoices.length, "      "),
    xmlTag("TotalDebit", "0.00", "      "),
    xmlTag("TotalCredit", formatAmount(invoiceTotals), "      "),
    invoiceBlocks,
    "    </SalesInvoices>",
    // ===== Purchase invoices section for the second slice (#127) =====
    "    <PurchaseInvoices>",
    xmlTag("NumberOfEntries", input.purchaseInvoices.length, "      "),
    xmlTag("TotalDebit", formatAmount(purchaseTotals), "      "),
    xmlTag("TotalCredit", "0.00", "      "),
    purchaseBlocks,
    "    </PurchaseInvoices>",
    "  </SourceDocuments>",
    // ===== VAT-summary (tax table) section for the second slice (#127) =====
    "  <TaxSummary>",
    xmlTag("NumberOfTaxCodes", input.vatSummary.length, "    "),
    vatSummaryBlocks,
    "  </TaxSummary>",
    "</AuditFile>",
    "",
  ].filter(Boolean).join("\n");
}

export function exportSaftPackage(db: Database, companyRoot: string, input: ExportSaftPackageInput): ExportSaftPackageResult {
  const errors: string[] = [];
  if (!looksLikeIsoDate(input.periodStart)) errors.push("periodStart must be YYYY-MM-DD");
  if (!looksLikeIsoDate(input.periodEnd)) errors.push("periodEnd must be YYYY-MM-DD");
  if (typeof input.outputDir !== "string" || input.outputDir.trim().length === 0) errors.push("outputDir is required");
  if (errors.length === 0 && input.periodStart > input.periodEnd) errors.push("periodStart cannot be after periodEnd");
  const generatedAt = resolveIsoDateTime(input.generatedAt) ?? normalizeExportTimestamp(input.periodEnd);
  if (input.generatedAt && !resolveIsoDateTime(input.generatedAt)) errors.push("generatedAt must be a valid ISO-8601 datetime when provided");

  const company = fetchCompany(db);
  if (!company) errors.push("company row is required for SAF-T export");
  if (company && !company.cvr) errors.push("company cvr is required for SAF-T export");

  if (errors.length > 0) return { ok: false, appliedRules: [RULE_ID], errors };

  const accounts = fetchAccounts(db);
  const journalEntries = fetchJournalEntries(db, input.periodStart, input.periodEnd);
  const salesInvoices = fetchSalesInvoices(db, input.periodStart, input.periodEnd);

  // ===== Second slice: purchase records + VAT summary + document references (#127) =====
  const purchaseInvoices = fetchPurchaseInvoices(db, input.periodStart, input.periodEnd);
  // The purchase profile requires explicit, deterministic source fields. Fail
  // clearly (before writing anything) when a purchase document in the period
  // cannot be represented, rather than emitting a silently incomplete record.
  for (const purchase of purchaseInvoices) {
    if (!purchase.supplierVatOrCvr) {
      errors.push(`purchase document ${purchase.documentNo} is missing supplier tax id (sender_vat_cvr) required for the SAF-T purchase profile`);
    }
    if (purchase.grossAmount == null) {
      errors.push(`purchase document ${purchase.documentNo} is missing amount_inc_vat required for the SAF-T purchase profile`);
    }
    if (purchase.vatAmount == null) {
      errors.push(`purchase document ${purchase.documentNo} is missing vat_amount required for the SAF-T purchase profile`);
    }
  }
  if (errors.length > 0) return { ok: false, appliedRules: [RULE_ID], errors };

  const vatSummary = summarizeVat(journalEntries);
  const documentNumbersById = fetchDocumentNumbers(
    db,
    [...new Set(journalEntries.map((entry) => entry.documentId).filter((id): id is number => id != null))],
  );

  // ===== Third slice: customer + supplier master files =====
  const customers = fetchCustomers(db);
  const suppliers = fetchSuppliers(db);

  const exportDir = join(input.outputDir, packageName(input.periodStart, input.periodEnd, generatedAt));
  mkdirSync(exportDir, { recursive: true });

  const outputs: ExportedFileMeta[] = [];
  const saftXmlPath = join(exportDir, "saft.xml");
  const readmePath = join(exportDir, "README.txt");
  const manifestPath = join(exportDir, "manifest.json");

  const xml = renderSaftXml({
    company: company!,
    accounts,
    customers,
    suppliers,
    journalEntries,
    salesInvoices,
    purchaseInvoices,
    vatSummary,
    documentNumbersById,
    periodStart: input.periodStart,
    periodEnd: input.periodEnd,
    generatedAt,
    companyRoot,
  });
  recordWrittenText(exportDir, saftXmlPath, xml, outputs);
  recordWrittenText(exportDir, readmePath, `${buildReadme({ periodStart: input.periodStart, periodEnd: input.periodEnd, generatedAt })}\n`, outputs);

  const manifest = {
    packageType: "saft_export",
    profileId: PROFILE_ID,
    generatedAt,
    periodStart: input.periodStart,
    periodEnd: input.periodEnd,
    sourceCompanyRootName: basename(companyRoot),
    appliedRules: [RULE_ID],
    files: {
      saftXml: "saft.xml",
      readme: "README.txt",
    },
    counts: {
      accounts: accounts.length,
      journalEntries: journalEntries.length,
      salesInvoices: salesInvoices.length,
      // ===== Second slice manifest counts (#127) =====
      purchaseInvoices: purchaseInvoices.length,
      vatSummaryCodes: vatSummary.length,
      documentReferences: documentNumbersById.size,
      // ===== Third slice manifest counts (master files) =====
      customers: customers.length,
      suppliers: suppliers.length,
    },
    outOfScope: [...OUT_OF_SCOPE],
    outputs,
  };
  recordWrittenText(exportDir, manifestPath, stringifyCanonicalJson(manifest), outputs);

  insertAuditLog(db, {
    eventType: "saft_export",
    entityType: "company",
    entityId: company!.id,
    message: `Exported saft_export ${PROFILE_ID} for ${input.periodStart}..${input.periodEnd} to ${packageRelativePath(input.outputDir, exportDir)}`,
  });

  return {
    ok: true,
    exportDir,
    manifestPath,
    saftXmlPath,
    generatedAt,
    periodStart: input.periodStart,
    periodEnd: input.periodEnd,
    journalEntryCount: journalEntries.length,
    salesInvoiceCount: salesInvoices.length,
    purchaseInvoiceCount: purchaseInvoices.length,
    customerCount: customers.length,
    supplierCount: suppliers.length,
    appliedRules: [RULE_ID],
    errors: [],
  };
}
