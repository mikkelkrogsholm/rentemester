import type { Database } from "bun:sqlite";
import { getInvoiceStatus } from "./invoice-payments";
import { normalizeEanNumber } from "./ean";

export type InvoiceQueryStatus = "open" | "paid" | "credited" | "refunded" | "overpaid" | "written_off" | "overdue" | "all";

export type InvoiceListFilters = {
  status?: InvoiceQueryStatus;
  from?: string;
  to?: string;
  customerCvr?: string;
  customer?: string;
  invoiceNumber?: string;
  query?: string;
  minAmount?: number;
  maxAmount?: number;
  asOfDate?: string;
  minDays?: number;
};

/**
 * E-faktura/PEPPOL-status for an issued invoice, derived from the
 * append-only `peppol_submissions` table populated by
 * `submitPublicEInvoicePeppol` / `transmitPublicEInvoicePeppol` (#128).
 *
 * `null` when the invoice has never been submitted/transmitted as a public
 * e-faktura. When present, `status` is `"prepared"` once a submission
 * envelope has been recorded and `"acknowledged"` once the access point has
 * confirmed receipt. The Cockpit row uses this both to flag "Sendt som
 * e-faktura" and to suppress a duplicate-send button when the invoice has
 * already been transmitted.
 */
export type InvoicePeppolStatus = {
  status: "prepared" | "acknowledged";
  submissionReference: string;
  transmissionId: string | null;
  acknowledgedAt: string | null;
};

export type InvoiceListRow = {
  documentId: number;
  invoiceNumber: string;
  invoiceDate: string | null;
  customerName: string | null;
  customerCvr: string | null;
  /**
   * Buyer's EAN-number when set on the invoice payload, normalised to 13
   * digits. Surfaced here so the Cockpit can render a "Send som e-faktura"
   * action only on rows whose recipient is a public (EAN) buyer — the
   * normalisation lives in `core/ean.ts`.
   */
  buyerEanNumber: string | null;
  /**
   * `true` when the invoice payload marks the buyer as a public-recipient
   * (offentlig kunde), independent of whether an EAN-number was supplied.
   * Used together with `buyerEanNumber` so the cockpit shows the e-faktura
   * action only when both signals are present.
   */
  buyerPublicRecipient: boolean;
  /**
   * The most recent PEPPOL submission/transmission status for this invoice,
   * or `null` when none exists. Derived from the append-only
   * `peppol_submissions` table.
   */
  peppolStatus: InvoicePeppolStatus | null;
  grossAmount: number;
  currency: string;
  openBalance: number;
  claimOpenBalance: number;
  status: "open" | "paid" | "credited" | "refunded" | "overpaid" | "written_off";
  effectiveDueDate: string | null;
  isOverdue: boolean;
  overdueDays: number;
};

export type InvoiceListResult = {
  ok: boolean;
  count: number;
  status: InvoiceQueryStatus;
  asOfDate?: string;
  query?: string;
  rows: InvoiceListRow[];
  errors: string[];
};

function normalizeText(value: unknown) {
  return typeof value === "string" ? value.trim() : "";
}

function includesFolded(haystack: string | null | undefined, needle: string | null | undefined) {
  const left = normalizeText(haystack).toLocaleLowerCase();
  const right = normalizeText(needle).toLocaleLowerCase();
  if (!right) return true;
  return left.includes(right);
}

function normalizeCode(value: string | null | undefined) {
  const trimmed = normalizeText(value);
  return trimmed ? trimmed.toUpperCase() : null;
}

function compareDate(value: string | null | undefined, boundary: string, direction: "from" | "to") {
  if (!value) return false;
  return direction === "from" ? value >= boundary : value <= boundary;
}

function issuedInvoiceDocuments(db: Database) {
  return db.query(
    `SELECT id, invoice_no, invoice_date, amount_inc_vat, currency, payload_json
     FROM documents
     WHERE document_type = 'issued_invoice'
     ORDER BY invoice_date ASC, id ASC`
  ).all() as Array<{
    id: number;
    invoice_no: string;
    invoice_date: string | null;
    amount_inc_vat: number | null;
    currency: string | null;
    payload_json: string | null;
  }>;
}

export function buildInvoiceList(db: Database, filters: InvoiceListFilters = {}): InvoiceListResult {
  const status = filters.status ?? "all";
  const rows: InvoiceListRow[] = [];
  const normalizedCustomerCvr = normalizeCode(filters.customerCvr);
  const normalizedInvoiceNumber = normalizeText(filters.invoiceNumber);
  const normalizedQuery = normalizeText(filters.query);
  const minDays = Number.isFinite(filters.minDays) ? Math.max(0, Number(filters.minDays)) : 0;

  for (const doc of issuedInvoiceDocuments(db)) {
    if (filters.from && !compareDate(doc.invoice_date, filters.from, "from")) continue;
    if (filters.to && !compareDate(doc.invoice_date, filters.to, "to")) continue;
    if (normalizedInvoiceNumber && doc.invoice_no !== normalizedInvoiceNumber) continue;

    const payload = doc.payload_json ? JSON.parse(doc.payload_json) : null;
    const customerName = normalizeText(payload?.buyer?.name) || null;
    const customerCvr = normalizeCode(payload?.buyer?.vatOrCvr);
    const buyerEanNumber = normalizeEanNumber(payload?.buyer?.eanNumber);
    const buyerPublicRecipient = payload?.buyer?.publicRecipient === true || Boolean(buyerEanNumber);
    if (normalizedCustomerCvr && customerCvr !== normalizedCustomerCvr) continue;
    if (!includesFolded(customerName, filters.customer)) continue;
    if (normalizedQuery && !includesFolded(doc.invoice_no, normalizedQuery) && !includesFolded(customerName, normalizedQuery)) continue;

    const invoiceStatus = getInvoiceStatus(db, doc.id, filters.asOfDate);
    if (!invoiceStatus.ok) continue;
    if (filters.minAmount !== undefined && Number(invoiceStatus.grossAmount ?? 0) < filters.minAmount) continue;
    if (filters.maxAmount !== undefined && Number(invoiceStatus.grossAmount ?? 0) > filters.maxAmount) continue;

    const peppolStatus = loadLatestPeppolStatus(db, doc.id);

    const row: InvoiceListRow = {
      documentId: doc.id,
      invoiceNumber: doc.invoice_no,
      invoiceDate: doc.invoice_date,
      customerName,
      customerCvr,
      buyerEanNumber,
      buyerPublicRecipient,
      peppolStatus,
      grossAmount: Number(invoiceStatus.grossAmount ?? 0),
      currency: doc.currency ?? "DKK",
      openBalance: Number(invoiceStatus.openBalance ?? 0),
      claimOpenBalance: Number(invoiceStatus.claimOpenBalance ?? 0),
      status: invoiceStatus.status!,
      effectiveDueDate: invoiceStatus.effectiveDueDate ?? null,
      isOverdue: Boolean(invoiceStatus.isOverdue),
      overdueDays: Number(invoiceStatus.overdueDays ?? 0),
    };

    if (status === "overdue") {
      if (!row.isOverdue || row.overdueDays < minDays) continue;
    } else if (status !== "all" && row.status !== status) {
      continue;
    }

    rows.push(row);
  }

  rows.sort((a, b) => {
    if (status === "overdue") {
      if (b.overdueDays !== a.overdueDays) return b.overdueDays - a.overdueDays;
      if (b.openBalance !== a.openBalance) return b.openBalance - a.openBalance;
    }
    const dueA = a.effectiveDueDate ?? "9999-12-31";
    const dueB = b.effectiveDueDate ?? "9999-12-31";
    if (dueA !== dueB) return dueA.localeCompare(dueB);
    return a.invoiceNumber.localeCompare(b.invoiceNumber);
  });

  return {
    ok: true,
    count: rows.length,
    status,
    asOfDate: filters.asOfDate,
    query: normalizedQuery || undefined,
    rows,
    errors: [],
  };
}

/**
 * Returns the most recent PEPPOL submission/transmission row for an invoice,
 * preferring an `acknowledged` row over a `prepared` one (because an
 * acknowledged transmission is the stronger "sent" signal). Returns `null`
 * when the invoice has never been submitted/transmitted as an e-faktura, or
 * when the `peppol_submissions` table is missing (older db not yet
 * migrated). Pure read; no side effects on the ledger.
 */
function loadLatestPeppolStatus(db: Database, invoiceDocumentId: number): InvoicePeppolStatus | null {
  try {
    const row = db
      .query(
        `SELECT status, submission_reference, transmission_id, acknowledged_at
         FROM peppol_submissions
         WHERE invoice_document_id = ?
         ORDER BY CASE status WHEN 'acknowledged' THEN 0 ELSE 1 END, id DESC
         LIMIT 1`,
      )
      .get(invoiceDocumentId) as
      | {
          status: "prepared" | "acknowledged";
          submission_reference: string;
          transmission_id: string | null;
          acknowledged_at: string | null;
        }
      | null;
    if (!row) return null;
    return {
      status: row.status,
      submissionReference: row.submission_reference,
      transmissionId: row.transmission_id,
      acknowledgedAt: row.acknowledged_at,
    };
  } catch {
    // Table not migrated yet (older fixture/db) → treat as "never sent".
    return null;
  }
}

export function findInvoices(db: Database, filters: Omit<InvoiceListFilters, "status" | "minDays"> = {}) {
  return buildInvoiceList(db, { ...filters, status: "all" });
}

export function buildOverdueInvoiceList(db: Database, filters: Omit<InvoiceListFilters, "status"> = {}) {
  return buildInvoiceList(db, { ...filters, status: "overdue" });
}
