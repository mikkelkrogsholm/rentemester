import { documentAttr, escapeDocumentText } from "../design/document-html";
import { PageHeader, Table, PageFooter } from "../design/pdfcn/components";
import { renderDocumentPdf } from "../design/pdf-render";
import type { Database } from "bun:sqlite";
import { createHash } from "node:crypto";
import { mkdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { insertAuditLog } from "./actor";
import { promoteTempFileExclusive, removeIfExists, writeTempFileFor } from "./atomic-file";
import { snapshotRegisteredDocumentEvidence } from "./document-storage";
import { strengthenGdprErasureAliasesForIdentity } from "./gdpr";
import type { InvoicePayload } from "./invoice";
import { formatAmount } from "./money";
import { companyPaths } from "./paths";

const RULE_ID = "DK-INVOICE-ISSUE-001";
const PDF_DOCUMENT_TYPE = "issued_invoice_pdf";

export type RenderIssuedInvoicePdfInput = {
  invoiceDocumentId: number;
};

export type RenderIssuedInvoicePdfResult = {
  ok: boolean;
  renderDocumentId?: number;
  invoiceNumber?: string;
  storedPath?: string;
  sha256?: string;
  appliedRules: string[];
  errors: string[];
};

/**
 * Payment instructions printed on the invoice so the customer knows where to
 * send the money. Sourced from the ledger's `bank_accounts` data; every field
 * is optional so partially-configured companies still produce a valid PDF.
 */
export type InvoicePaymentDetails = {
  bankName?: string | null;
  /** Danish 4-digit bank registration number (registreringsnummer). */
  registrationNo?: string | null;
  /** Danish bank account number (kontonummer). */
  accountNo?: string | null;
  iban?: string | null;
  /** Optional SWIFT/BIC for foreign transfers. */
  bic?: string | null;
  accountOwner?: string | null;
  customerNo?: string | null;
};

/**
 * The full payload `buildIssuedInvoicePdf` accepts: a validated `InvoicePayload`
 * plus the issued-invoice metadata and the optional rendering extras (payment
 * details + a lightweight text logo). Keeping these extras here — rather than in
 * the core `InvoicePayload` — keeps the validator unaware of presentation data.
 */
export type IssuedInvoicePdfPayload = InvoicePayload & {
  invoiceNumber?: string;
  issuedAt?: string;
  status?: string;
  /** Payment instructions; injected from ledger bank-account data on render. */
  payment?: InvoicePaymentDetails;
  /**
   * Lightweight brand mark. A short text string is rendered as a styled word
   * mark in the header. This is a deliberate minimal seam: an image-based logo
   * can be added later without changing the call sites.
   */
  logoText?: string | null;
};

function sha256(buffer: Uint8Array) {
  return createHash("sha256").update(buffer).digest("hex");
}

function compact(value: string | null | undefined) {
  const trimmed = value?.trim();
  return trimmed ? trimmed : null;
}

// ---------------------------------------------------------------------------
// Danish number formatting (#225)
//
// A Danish invoice must show amounts the Danish way — thousands grouped with a
// period and the decimal separator a comma: `1.000,00`, not `1000.00`. The
// shared `money.ts` `formatAmount`/`formatDkk` emit the locale-free
// "1234.56" form for JSON/ledger stability; here, on the customer-facing PDF,
// we re-group that canonical string into Danish presentation. The arithmetic
// still runs through `money.ts`, so this is a pure presentation transform.
// ---------------------------------------------------------------------------

/** Re-group a canonical `formatAmount` string ("-1234.56") into Danish
 *  presentation ("-1.234,56"). Returns null for null input. */
function toDanishNumber(canonical: string | null): string | null {
  if (canonical == null) return null;
  const negative = canonical.startsWith("-");
  const unsigned = negative ? canonical.slice(1) : canonical;
  const [whole = "0", fraction = "00"] = unsigned.split(".");
  const grouped = whole.replace(/\B(?=(\d{3})+(?!\d))/g, ".");
  return `${negative ? "-" : ""}${grouped},${fraction}`;
}

/** Format a monetary amount in Danish number format ("1.000,00"), no currency
 *  suffix. Returns null for null/undefined/empty/NaN. */
function formatDanishAmount(value: number | string | null | undefined): string | null {
  return toDanishNumber(formatAmount(value));
}

/** Format a monetary amount in Danish number format with a currency suffix
 *  ("1.000,00 DKK"). Returns null for invalid input. */
function formatDanishDkk(value: number | string | null | undefined, currency = "DKK"): string | null {
  const amount = formatDanishAmount(value);
  if (amount == null) return null;
  return `${amount} ${currency.trim().toUpperCase()}`;
}

function amountLabel(value: unknown, currency = "DKK") {
  if (value == null || value === "") return null;
  const amount = typeof value === "number" ? value : Number(value);
  if (!Number.isFinite(amount)) return null;
  return formatDanishDkk(amount, currency);
}

type LineItem = { description: string; quantity: string | null; unitPrice: string | null; lineTotal: string | null };

function buildLineItems(payload: IssuedInvoicePdfPayload): LineItem[] {
  return (payload.lines ?? []).map((line) => ({
    description: `${compact(line.description) ?? ""}${line.taxClassification ? ` [${line.taxClassification}]` : ""}`,
    quantity: line.quantity == null ? "" : String(line.quantity),
    unitPrice: line.unitPriceExVat == null ? "" : (formatDanishAmount(line.unitPriceExVat) ?? ""),
    lineTotal: line.lineTotalExVat == null ? "" : (formatDanishAmount(line.lineTotalExVat) ?? ""),
  }));
}

function paymentLines(payment: InvoicePaymentDetails | undefined): string[] {
  if (!payment) return [];
  const lines: string[] = [];
  const bank = compact(payment.bankName);
  if (bank) lines.push(bank);
  const reg = compact(payment.registrationNo);
  const acct = compact(payment.accountNo);
  if (reg && acct) lines.push(`Indenlandsk: Reg.nr. ${reg}  Kontonr. ${acct}`);
  else if (acct) lines.push(`Kontonr. ${acct}`);
  else if (reg) lines.push(`Reg.nr. ${reg}`);
  const iban = compact(payment.iban);
  if (iban) lines.push(`International: IBAN: ${iban}`);
  const bic = compact(payment.bic);
  if (bic) lines.push(`SWIFT/BIC: ${bic}`);
  const owner = compact(payment.accountOwner);
  if (owner) lines.push(`Kontoejer: ${owner}`);
  const customerNo = compact(payment.customerNo);
  if (customerNo) lines.push(`Bank-kundenr.: ${customerNo}`);
  return lines;
}

export function buildIssuedInvoicePdf(payload: IssuedInvoicePdfPayload): Buffer {
  const currency = (payload.currency ?? "DKK").trim().toUpperCase();
  const invoiceNumber = compact(payload.invoiceNumber) ?? "";
  const title = `Faktura ${invoiceNumber}`.trim();
  const pairs: Array<[string, string]> = [];
  if (invoiceNumber) pairs.push(["Fakturanr.", invoiceNumber]);
  if (compact(payload.issueDate)) pairs.push(["Fakturadato", payload.issueDate!.trim()]);
  if (compact(payload.dueDate)) pairs.push(["Forfaldsdato", payload.dueDate!.trim()]);
  if (compact(payload.deliveryDate)) pairs.push(["Leveringsdato", payload.deliveryDate!.trim()]);
  else if (compact(payload.deliveryPeriodStart) && compact(payload.deliveryPeriodEnd)) pairs.push(["Leveringsperiode", `${payload.deliveryPeriodStart!.trim()} – ${payload.deliveryPeriodEnd!.trim()}`]);
  pairs.push(["Valuta", currency]);
  const paragraph = (text: string) => `<p ${documentAttr("pdfParagraph")}>${escapeDocumentText(text)}</p>`;
  const party = (label: string, value?: { name?: string; address?: string; vatOrCvr?: string }) => `<section ${documentAttr("pdfParty")}><h2 ${documentAttr("pdfLabel")}>${label}</h2>${paragraph(compact(value?.name) ?? "—")}${compact(value?.address) ? paragraph(value!.address!.trim()) : ""}${compact(value?.vatOrCvr) ? paragraph(`CVR/SE: ${value!.vatOrCvr!.trim()}`) : ""}</section>`;
  const items = buildLineItems(payload);
  const totals: Array<[string, string, boolean]> = [];
  const net = amountLabel(payload.totals?.netAmount, currency);
  const vat = amountLabel(payload.totals?.vatAmount, currency);
  const gross = amountLabel(payload.totals?.grossAmount, currency);
  if (net) totals.push(["Netto", net, false]);
  if (vat) {
    const rate = payload.totals?.vatRate;
    const percent = rate == null ? null : Number(rate) * 100;
    totals.push([percent != null && Number.isFinite(percent) ? `Moms (${percent.toFixed(Number.isInteger(percent) ? 0 : 2)}%)` : "Moms", vat, false]);
  }
  if (gross) totals.push(["Total", gross, true]);
  if (currency !== "DKK" && payload.totals?.fxRateToDkk != null) totals.push(["Valutakurs til DKK", Number(payload.totals.fxRateToDkk).toFixed(6), false]);
  const grossDkk = amountLabel(payload.totals?.grossAmountDkk, "DKK");
  if (currency !== "DKK" && grossDkk) totals.push(["Total i DKK", grossDkk, true]);
  const payment = paymentLines(payload.payment);
  const html = `<main ${documentAttr("pdfBody")}>${PageHeader({ title: compact(payload.logoText) ?? compact(payload.seller?.name) ?? "Faktura", rightText: "FAKTURA", rightSubText: invoiceNumber })}<section ${documentAttr("pdfSection")}>${pairs.map(([label, value]) => paragraph(`${label}: ${value}`)).join("")}</section><div ${documentAttr("pdfParties")}>${party("SÆLGER", payload.seller)}${party("KØBER", payload.buyer)}</div>${Table({ headers: ["Beskrivelse", "Antal", "Stykpris", "Beløb"], rows: items.map(item => [item.description || "—", item.quantity ?? "", item.unitPrice ?? "", item.lineTotal ?? ""]), numericColumns: [1,2,3], zebraStripe: true })}<table ${documentAttr("pdfTotals")}><tbody>${totals.map(([label, amount, strong]) => `<tr ${documentAttr("pdfRow")}><td ${documentAttr(strong ? "pdfTotalLabel" : "pdfCell")}>${escapeDocumentText(label)}</td><td ${documentAttr(strong ? "pdfTotal" : "pdfNumber")}>${escapeDocumentText(amount)}</td></tr>`).join("")}</tbody></table>${payment.length ? `<section ${documentAttr("pdfSection")}><h2 ${documentAttr("pdfLabel")}>BETALING</h2>${payment.map(paragraph).join("")}${invoiceNumber ? paragraph(`Anfør fakturanr. ${invoiceNumber} ved betaling.`) : ""}</section>` : ""}${compact(payload.reverseChargeNote) ? `<section ${documentAttr("pdfSection")}><h2 ${documentAttr("pdfLabel")}>NOTE</h2>${paragraph(payload.reverseChargeNote!.trim())}</section>` : ""}</main>`;
  return renderDocumentPdf({ html, title, date: compact(payload.issueDate) ?? "1970-01-01", footer: PageFooter(title) });
}

/**
 * Resolve the company's payment instructions from the ledger's `bank_accounts`
 * table. Deterministic: prefers the lowest-id active account whose currency
 * matches the invoice, then any lowest-id active account. Returns `undefined`
 * when no account is configured so the PDF simply omits the payment block.
 */
function resolvePaymentDetails(db: Database, currency: string): InvoicePaymentDetails | undefined {
  let rows: Array<{
    bank_name: string | null;
    registration_no: string | null;
    account_no: string | null;
    iban: string | null;
    bic: string | null;
    account_owner: string | null;
    customer_no: string | null;
    currency: string | null;
  }> = [];
  try {
    rows = db
      .query(
        `SELECT bank_name, registration_no, account_no, iban, bic, account_owner, customer_no, currency
           FROM bank_accounts
          WHERE active = 1
          ORDER BY id ASC`,
      )
      .all() as typeof rows;
  } catch {
    // The table may not exist in very old ledgers; payment block is optional.
    return undefined;
  }
  if (rows.length === 0) return undefined;
  const wanted = currency.trim().toUpperCase();
  const match =
    rows.find((row) => (row.currency ?? "").trim().toUpperCase() === wanted) ?? rows[0];
  const details: InvoicePaymentDetails = {
    bankName: match.bank_name,
    registrationNo: match.registration_no,
    accountNo: match.account_no,
    iban: match.iban,
    bic: match.bic,
    accountOwner: match.account_owner,
    customerNo: match.customer_no,
  };
  // Only return a block if at least one field carries real payment information.
  if (!compact(details.bankName) && !compact(details.registrationNo) && !compact(details.accountNo) && !compact(details.iban) && !compact(details.bic) && !compact(details.accountOwner) && !compact(details.customerNo)) {
    return undefined;
  }
  return details;
}

export function renderIssuedInvoicePdf(db: Database, companyRoot: string, input: RenderIssuedInvoicePdfInput): RenderIssuedInvoicePdfResult {
  const invoice = db.query(
    `SELECT id, invoice_no, invoice_date, payload_json, status, retain_until
     FROM documents WHERE id = ? AND document_type = 'issued_invoice'`
  ).get(input.invoiceDocumentId) as {
    id: number;
    invoice_no: string | null;
    invoice_date: string | null;
    payload_json: string | null;
    status: string | null;
    retain_until: string | null;
  } | null;

  if (!invoice) return { ok: false, appliedRules: [RULE_ID], errors: [`invoice document ${input.invoiceDocumentId} does not exist or is not an issued invoice`] };
  if (!invoice.payload_json) return { ok: false, appliedRules: [RULE_ID], errors: [`invoice ${invoice.invoice_no ?? input.invoiceDocumentId} is missing payload_json`] };

  // A normal issued invoice has its sequential number in the immutable row.
  // Do not parse the render payload merely to serve an existing PDF: the PDF
  // remains independently verifiable evidence even if unrelated legacy JSON
  // has become unreadable.
  let payload: IssuedInvoicePdfPayload | undefined;
  let invoiceNumber = compact(invoice.invoice_no);
  if (!invoiceNumber) {
    try {
      payload = JSON.parse(invoice.payload_json) as IssuedInvoicePdfPayload;
      invoiceNumber = compact(payload.invoiceNumber);
    } catch {
      return { ok: false, appliedRules: [RULE_ID], errors: [`invoice ${input.invoiceDocumentId} has invalid payload_json`] };
    }
  }
  if (!invoiceNumber) return { ok: false, appliedRules: [RULE_ID], errors: ["issued invoice is missing invoice number"] };

  const paths = companyPaths(companyRoot);
  const storedPath = join(paths.invoicesIssued, `${invoiceNumber}.pdf`);

  // An issued PDF is accounting evidence, not a view derived from today's
  // master data.  In particular, a later bank-account or payment-term change
  // must never replace the bytes the customer received.  Look it up before
  // building anything: the successful path below is deliberately read-only.
  const existing = db.query(
    `SELECT id, sha256_hash, stored_path, payload_json FROM documents
       WHERE document_type = ? AND invoice_no = ?
       ORDER BY id ASC`,
  ).all(PDF_DOCUMENT_TYPE, invoiceNumber) as Array<{ id: number; sha256_hash: string; stored_path: string | null; payload_json: string | null }>;

  if (existing.length > 1) {
    return {
      ok: false,
      appliedRules: [RULE_ID],
      errors: [`issued invoice PDF evidence for ${invoiceNumber} is ambiguous; refusing to render or replace it`],
    };
  }

  if (existing.length === 1) {
    const evidence = existing[0]!;
    if (evidence.payload_json !== invoice.payload_json) {
      return {
        ok: false,
        appliedRules: [RULE_ID],
        errors: [`issued invoice PDF evidence for ${invoiceNumber} is not bound to the immutable invoice snapshot; refusing to repair it`],
      };
    }
    try {
      const existing = snapshotRegisteredDocumentEvidence(companyRoot, {
        storedPath: evidence.stored_path ?? "",
        expectedSha256: evidence.sha256_hash,
        documentType: PDF_DOCUMENT_TYPE,
      });
      if (!existing.bytes.subarray(0, 5).equals(Buffer.from("%PDF-"))) {
        return {
          ok: false,
          appliedRules: [RULE_ID],
          errors: [`issued invoice PDF evidence for ${invoiceNumber} is missing or fails integrity verification; refusing to repair it`],
        };
      }
      return {
        ok: true,
        renderDocumentId: evidence.id,
        invoiceNumber,
        storedPath,
        sha256: evidence.sha256_hash,
        appliedRules: [RULE_ID],
        errors: [],
      };
    } catch {
      return {
        ok: false,
        appliedRules: [RULE_ID],
        errors: [`issued invoice PDF evidence for ${invoiceNumber} is missing or fails integrity verification; refusing to repair it`],
      };
    }
  }

  // Older ledgers can contain an issued invoice without a PDF evidence row.
  // That is the only case in which this command may create one.  It publishes
  // to an absent path exclusively and never replaces an existing file.
  mkdirSync(paths.invoicesIssued, { recursive: true });
  if (!payload) {
    try {
      payload = JSON.parse(invoice.payload_json) as IssuedInvoicePdfPayload;
    } catch {
      return { ok: false, appliedRules: [RULE_ID], errors: [`invoice ${invoiceNumber} has invalid payload_json`] };
    }
  }
  // Payment details from the payload win; otherwise pull them from the ledger.
  const currency = (payload.currency ?? "DKK").trim().toUpperCase();
  const payment = payload.payment ?? resolvePaymentDetails(db, currency);
  const bytes = buildIssuedInvoicePdf({
    ...payload,
    invoiceNumber,
    status: payload.status ?? invoice.status ?? "issued",
    ...(payment ? { payment } : {}),
  });
  const hash = sha256(bytes);
  let tempPath: string | undefined;

  try {
    const result = db.transaction(() => {
      const concurrentEvidence = db.query(
        `SELECT id FROM documents WHERE document_type = ? AND invoice_no = ? ORDER BY id ASC`,
      ).all(PDF_DOCUMENT_TYPE, invoiceNumber) as Array<{ id: number }>;
      if (concurrentEvidence.length > 0) {
        throw new Error(`issued invoice PDF evidence for ${invoiceNumber} appeared concurrently; retry to verify it`);
      }

      tempPath = writeTempFileFor(storedPath, bytes);

      const inserted = db.query(
        `INSERT INTO documents (
          document_no, source, original_filename, stored_path, mime_type, sha256_hash,
          supplier_name, invoice_no, invoice_date, amount_inc_vat, currency, status,
          document_type, sender_name, sender_address, sender_vat_cvr,
          recipient_name, recipient_address, recipient_vat_cvr, vat_amount, payload_json, retain_until
        ) VALUES (?, 'rentemester', ?, ?, 'application/pdf', ?, ?, ?, ?, ?, ?, 'issued', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        RETURNING id`
      ).get(
        `${invoiceNumber}-pdf`,
        `${invoiceNumber}.pdf`,
        storedPath,
        hash,
        payload.seller?.name ?? null,
        invoiceNumber,
        invoice.invoice_date ?? payload.issueDate ?? null,
        payload.totals?.grossAmount ?? null,
        payload.currency ?? "DKK",
        PDF_DOCUMENT_TYPE,
        payload.seller?.name ?? null,
        payload.seller?.address ?? null,
        payload.seller?.vatOrCvr ?? null,
        payload.buyer?.name ?? null,
        payload.buyer?.address ?? null,
        payload.buyer?.vatOrCvr ?? null,
        payload.totals?.vatAmount ?? null,
        invoice.payload_json,
        invoice.retain_until ?? null,
      ) as { id: number };

      strengthenGdprErasureAliasesForIdentity(db, {
        name: payload.seller?.name,
        cvr: payload.seller?.vatOrCvr,
      });
      strengthenGdprErasureAliasesForIdentity(db, {
        name: payload.buyer?.name,
        cvr: payload.buyer?.vatOrCvr,
      });

      insertAuditLog(db, {
        eventType: "invoice_render_pdf",
        entityType: "document",
        entityId: inserted.id,
        message: `Rendered invoice PDF ${invoiceNumber}`,
      });

      return { renderDocumentId: inserted.id };
    }).immediate();

    promoteTempFileExclusive(tempPath!, storedPath);
    return { ok: true, renderDocumentId: result.renderDocumentId, invoiceNumber, storedPath, sha256: hash, appliedRules: [RULE_ID], errors: [] };
  } catch (error) {
    if (tempPath) removeIfExists(tempPath);
    return { ok: false, appliedRules: [RULE_ID], errors: [String(error)] };
  }
}

export function readIssuedInvoicePdfText(path: string) {
  return readFileSync(path, "latin1");
}
