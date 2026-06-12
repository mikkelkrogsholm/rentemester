// Leverandørfaktura-arbejdsbordet (#340) — read-side data for the cockpit's
// payables view.
//
// The view shows the kreditorliste produced by `core/payables.ts#buildPayablesList`
// plus the picker rows the "Registrér leverandørfaktura"-modal needs to register
// an existing purchase document (bilag) as a payable: the bookable expense
// accounts, the unregistered purchase documents, and the registered vendors.
//
// Every figure here is computed by an existing core function — this module only
// opens the right ledger, calls core, and shapes the JSON. It never mutates a
// ledger.
//
// All money fields are kroner (DKK with decimals).
import { existsSync } from "node:fs";
import { companyPaths } from "../../core/paths";
import { openDb, migrate } from "../../core/db";
import {
  buildPayablesList,
  type PayableAgingBucket,
  type PayableStatus,
  type PayableQueryStatus,
  type PayablesListRow,
} from "../../core/payables";
import {
  companyRootForSlug,
  findWorkspaceCompany,
} from "../../core/workspace";
import { ApiError } from "../errors";
import {
  buildCompanyFiscalYears,
  requireCompanyDbPath,
  roundKroner,
  statementCompanyBlock,
  type FiscalYearEntry,
} from "./shared";
import { getCompanySettings } from "../../core/company";

/** One payable row, shaped for the cockpit table. Money in kroner. */
export type CompanyPayableRow = {
  payableId: number;
  documentId: number;
  billNo: string | null;
  billDate: string;
  dueDate: string;
  supplierName: string | null;
  vendorId: number | null;
  grossAmount: number;
  currency: string;
  paidAmount: number;
  openBalance: number;
  status: PayableStatus;
  isOverdue: boolean;
  overdueDays: number;
  agingBucket: PayableAgingBucket;
};

/** One purchase document not yet registered as a payable (modal picker row). */
export type UnregisteredPurchaseDocumentRow = {
  id: number;
  documentNo: string | null;
  invoiceNo: string | null;
  invoiceDate: string | null;
  supplierName: string | null;
  amountIncVat: number | null;
  vatAmount: number | null;
  currency: string;
};

/** One expense account that can back a payable's debit line (modal picker). */
export type PayableExpenseAccountOption = {
  accountNo: string;
  name: string;
  defaultVatCode: string | null;
};

/** One registered vendor, surfaced for the modal's optional vendor link. */
export type PayableVendorOption = {
  id: number;
  name: string;
  defaultExpenseAccount: string | null;
  defaultVatTreatment: string | null;
};

/** Aggregated cockpit payload backing `GET /api/companies/:slug/payables`. */
export type CompanyPayables = {
  slug: string;
  asOfDate: string;
  /** Status filter that was effectively applied to the list. */
  status: PayableQueryStatus;
  company: ReturnType<typeof statementCompanyBlock>;
  fiscalYears: FiscalYearEntry[];
  rows: CompanyPayableRow[];
  count: number;
  totalOpenBalance: number;
  overdueOpenBalance: number;
  notYetDueOpenBalance: number;
  /** Picker rows for the "Registrér leverandørfaktura"-modal. */
  unregisteredDocuments: UnregisteredPurchaseDocumentRow[];
  expenseAccounts: PayableExpenseAccountOption[];
  vendors: PayableVendorOption[];
};

function normalizeStatus(raw: string | null | undefined): PayableQueryStatus {
  if (raw === "open" || raw === "paid" || raw === "overdue" || raw === "all") {
    return raw;
  }
  // Default surfaces the action-needed list — open payables, sorted with the
  // most overdue first — exactly what the cockpit owner wants to see.
  return "open";
}

/**
 * Builds the cockpit payable workbench payload. Reuses
 * `core/payables.ts#buildPayablesList` for the kreditorliste; adds the modal
 * picker rows (unregistered purchase documents, expense accounts, vendors) so
 * the "Registrér leverandørfaktura"-modal can open without a second round-trip.
 */
export function buildCompanyPayables(
  workspaceRoot: string,
  slug: string,
  rawStatus?: string | null,
  rawAsOf?: string | null,
): CompanyPayables {
  const entry = findWorkspaceCompany(workspaceRoot, slug);
  if (!entry) {
    throw ApiError.notFound(`ingen virksomhed med slug '${slug}' findes i workspacet`);
  }
  const dbPath = requireCompanyDbPath(workspaceRoot, slug);
  if (!existsSync(dbPath)) {
    throw ApiError.notFound(`virksomheden '${slug}' har ingen ledger`);
  }
  const status = normalizeStatus(rawStatus);
  const asOfDate = typeof rawAsOf === "string" && /^\d{4}-\d{2}-\d{2}$/.test(rawAsOf)
    ? rawAsOf
    : undefined;
  const db = openDb(dbPath);
  try {
    migrate(db);
    const settings = getCompanySettings(db);
    const companyBlock = statementCompanyBlock(settings);
    const fiscalYears = buildCompanyFiscalYears(workspaceRoot, slug).years;
    const list = buildPayablesList(db, {
      status,
      asOfDate,
    });
    if (!list.ok) {
      throw ApiError.badRequest(
        list.errors[0] ?? "kunne ikke bygge kreditorliste",
      );
    }

    const rows: CompanyPayableRow[] = list.rows.map((r: PayablesListRow) => ({
      payableId: r.payableId,
      documentId: r.documentId,
      billNo: r.billNo,
      billDate: r.billDate,
      dueDate: r.dueDate,
      supplierName: r.supplierName,
      vendorId: r.vendorId,
      grossAmount: roundKroner(r.grossAmount),
      currency: r.currency,
      paidAmount: roundKroner(r.paidAmount),
      openBalance: roundKroner(r.openBalance),
      status: r.status,
      isOverdue: r.isOverdue,
      overdueDays: r.overdueDays,
      agingBucket: r.agingBucket,
    }));

    const unregisteredDocuments: UnregisteredPurchaseDocumentRow[] = (db
      .query(
        `SELECT d.id, d.document_no, d.invoice_no, d.invoice_date,
                d.supplier_name, d.amount_inc_vat, d.vat_amount, d.currency
           FROM documents d
           LEFT JOIN payables p ON p.document_id = d.id
          WHERE d.document_type IN ('purchase_sale', 'cash_register_receipt')
            AND p.id IS NULL
            AND UPPER(COALESCE(d.currency, 'DKK')) = 'DKK'
          ORDER BY COALESCE(d.invoice_date, '') DESC, d.id DESC
          LIMIT 200`,
      )
      .all() as Array<{
      id: number;
      document_no: string | null;
      invoice_no: string | null;
      invoice_date: string | null;
      supplier_name: string | null;
      amount_inc_vat: number | null;
      vat_amount: number | null;
      currency: string;
    }>).map((r) => ({
      id: r.id,
      documentNo: r.document_no,
      invoiceNo: r.invoice_no,
      invoiceDate: r.invoice_date,
      supplierName: r.supplier_name,
      amountIncVat:
        r.amount_inc_vat === null ? null : roundKroner(r.amount_inc_vat),
      vatAmount: r.vat_amount === null ? null : roundKroner(r.vat_amount),
      currency: r.currency,
    }));

    const expenseAccounts: PayableExpenseAccountOption[] = (db
      .query(
        `SELECT account_no, name, default_vat_code
           FROM accounts
          WHERE type = 'expense' AND active = 1
          ORDER BY account_no ASC`,
      )
      .all() as Array<{
      account_no: string;
      name: string;
      default_vat_code: string | null;
    }>).map((r) => ({
      accountNo: r.account_no,
      name: r.name,
      defaultVatCode: r.default_vat_code,
    }));

    let vendors: PayableVendorOption[] = [];
    try {
      vendors = (db
        .query(
          `SELECT id, name, default_expense_account, default_vat_treatment
             FROM vendors
            WHERE archived = 0
            ORDER BY name COLLATE NOCASE ASC`,
        )
        .all() as Array<{
        id: number;
        name: string;
        default_expense_account: string | null;
        default_vat_treatment: string | null;
      }>).map((r) => ({
        id: r.id,
        name: r.name,
        defaultExpenseAccount: r.default_expense_account,
        defaultVatTreatment: r.default_vat_treatment,
      }));
    } catch {
      // Older fixtures without a vendors table — degrade to empty list.
      vendors = [];
    }

    return {
      slug: entry.slug,
      asOfDate: list.asOfDate,
      status: list.status,
      company: companyBlock,
      fiscalYears,
      rows,
      count: list.count,
      totalOpenBalance: roundKroner(list.totalOpenBalance),
      overdueOpenBalance: roundKroner(list.overdueOpenBalance),
      notYetDueOpenBalance: roundKroner(list.notYetDueOpenBalance),
      unregisteredDocuments,
      expenseAccounts,
      vendors,
    };
  } finally {
    db.close();
  }
}
