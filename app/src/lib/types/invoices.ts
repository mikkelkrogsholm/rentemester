// Invoices / Fakturaer + recurring-invoice wire types.
//
// All money fields below are kroner (DKK with decimals) — use `formatKroner`.

import type { DataCoverage, FiscalYearEntry, StatementCompany } from "./common";

// --- invoices / Fakturaer (GET .../invoices?year=) — cockpit-redesign it. 5 --

export type InvoiceStatus =
  | "open"
  | "paid"
  | "credited"
  | "refunded"
  | "overpaid"
  | "written_off"
  | "overdue";

/**
 * Cockpit-facing PEPPOL/e-faktura status (#428) — `null` when the invoice
 * has never been attempted as an e-faktura. `queued` has an accepted document
 * id and permits status-only polling; `failed` is a terminal remote result and
 * `uncertain` means the POST outcome cannot safely be retried. Neither may be
 * redelivered. `retryable` failed before delivery; `in_progress` is reserved by
 * another sender; `acknowledged` is delivered.
 */
export type InvoicePeppolStatus = {
  status: "queued" | "failed" | "uncertain" | "retryable" | "in_progress" | "acknowledged";
  submissionReference: string;
  transmissionId: string | null;
  acknowledgedAt: string | null;
};

export type CompanyInvoiceRow = {
  documentId: number;
  invoiceNo: string;
  invoiceDate: string | null;
  customerName: string | null;
  partyId?: string | null;
  /**
   * Customer's e-mail when set on the kontaktkort (#429). The cockpit row
   * offers "Send på mail" only when this is present so the dialog can
   * prefill the recipient without a second round-trip.
   */
  customerEmail: string | null;
  /**
   * Buyer's EAN-number (13 digits) when set on the invoice payload. The
   * cockpit row offers "Send e-faktura" only when this is present.
   */
  buyerEanNumber: string | null;
  /** True when the buyer is marked as a public recipient. */
  buyerPublicRecipient: boolean;
  /** Latest PEPPOL submission/transmission, or `null` when never sent. */
  peppolStatus: InvoicePeppolStatus | null;
  /**
   * Timestamp (ISO-8601) of the most recent `email_send_log` row for this
   * invoice (#429), or `null` when the invoice has never been emailed from
   * the cockpit. Surfaced so the row can show "Sendt {dato}" beside the
   * settlement status.
   */
  lastEmailedAt: string | null;
  /**
   * Timestamp (ISO-8601) of the most recently registered payment reminder
   * (#434), or `null` when no reminder has been registered yet. Surfaced so
   * the row can show "{n}. rykker registreret {dato}" under the status flag
   * and the "Send rykker" action knows whether further reminders are still
   * allowed.
   */
  lastReminderAt: string | null;
  /**
   * Count of reminders that have been registered against the invoice (#434).
   * 0 when no reminder has been sent. The cockpit hides the "Send rykker"
   * action once this reaches the statutory cap of 3 (rentel. § 9b).
   */
  lastReminderSequence: number;
  /** Gross amount inc. VAT, kroner. */
  grossAmount: number;
  /** Still-outstanding balance on the invoice, kroner. */
  openBalance: number;
  currency: string;
  status: InvoiceStatus;
  effectiveDueDate: string | null;
  overdueDays: number;
};

export type CompanyInvoices = {
  slug: string;
  selectedYear: string;
  archived: boolean;
  company: StatementCompany;
  fiscalYears: FiscalYearEntry[];
  periodStart: string;
  periodEnd: string;
  invoices: CompanyInvoiceRow[];
  totalGross: number;
  totalOpen: number;
  overdueCount: number;
  coverage: DataCoverage;
};

export type InvoicesResponse = {
  ok: true;
  invoices: CompanyInvoices;
};

/** Source-evidenced opening debtors.  Kept separate from `CompanyInvoices`: a
 * native issued invoice is never an import continuation or a duplicate row. */
export type ImportedReceivableRow = {
  source: "imported";
  externalInvoiceId: string;
  customerExternalId: string | null;
  customerName: string | null;
  invoiceDate: string;
  dueDate: string | null;
  grossAmount: number;
  paidAmount: number;
  openBalance: number;
  controlAccountNo: string;
  sourceRecognitionRef: string;
  sourceDocumentHash: string;
  scheduleHash: string;
  archiveBoundary: string;
};

export type ImportedReceivables = {
  ok: true;
  asOfDate: string;
  boundary: string;
  count: number;
  totalOpen: number;
  rows: ImportedReceivableRow[];
  errors: string[];
};

export type ImportedReceivablesResponse = { ok: true; importedReceivables: ImportedReceivables };

/** One previously generated invoice for a recurring-invoice template. */
export type RecurringInvoiceGenerationRow = {
  id: number;
  periodIndex: number;
  invoiceNumber: string;
  issueDate: string;
  documentId: number;
  deliveryPeriodStart: string | null;
  deliveryPeriodEnd: string | null;
};

/** One recurring-invoice template plus the invoices it has already issued. */
export type RecurringInvoiceTemplateRow = {
  id: number;
  name: string;
  interval: "weekly" | "monthly" | "quarterly" | "yearly";
  /** Present for v3 templates; absent legacy API payloads mean 1/manual. */
  intervalCount?: number;
  deliveryChannel?: "manual" | "email" | "digisense";
  firstIssueDate: string;
  nextIssueDate: string;
  paymentTermsDays: number;
  deliveryPeriodMode: "issue_month" | "interval_window" | "none";
  notes: string | null;
  active: boolean;
  createdAt: string;
  generations: RecurringInvoiceGenerationRow[];
};

/** Public alias used by the create modal (#386). */
export type RecurringInterval = "weekly" | "monthly" | "quarterly" | "yearly";
export type RecurringDeliveryChannel = "manual" | "email" | "digisense";
export type DeliveryPeriodMode = "issue_month" | "interval_window" | "none";

/**
 * Minimal create-template payload the cockpit POSTs (#386). The server
 * computes line totals + net/moms/brutto via `computeInvoiceAmounts` and runs
 * the same `createRecurringInvoiceTemplate` core function the CLI calls —
 * the cockpit never hand-builds an `InvoicePayload`.
 */
export type RecurringInvoiceTemplateInput = {
  name: string;
  interval: RecurringInterval;
  intervalCount?: number;
  deliveryChannel?: RecurringDeliveryChannel;
  firstIssueDate: string;
  paymentTermsDays: number;
  deliveryPeriodMode?: DeliveryPeriodMode;
  notes?: string;
  vatRatePercent: number;
  currency?: string;
  /** When set, server back-fills the buyer from stored customer master-data. */
  customerId?: number;
  buyer?: { name?: string; address?: string; vatOrCvr?: string };
  lines: Array<{
    description: string;
    quantity: number;
    unitPriceExVat: number;
  }>;
};

/** Server's echo of a successful create. */
export type RecurringInvoiceTemplateCreatedResult = {
  templateId: number;
  name: string;
  interval: RecurringInterval;
  intervalCount: number;
  deliveryChannel: RecurringDeliveryChannel;
  firstIssueDate: string;
};

export type CompanyRecurringInvoices = {
  slug: string;
  templates: RecurringInvoiceTemplateRow[];
};

export type RecurringInvoicesResponse = {
  ok: true;
  recurringInvoices: CompanyRecurringInvoices;
};

/** The generate-from-template result the server echoes back. */
export type RecurringInvoiceGenerationResult = {
  /** True for a freshly-issued invoice, false for an idempotent re-run. */
  created: boolean;
  templateId: number | null;
  periodIndex: number | null;
  documentId: number | null;
  invoiceNumber: string | null;
  issueDate: string | null;
  dueDate: string | null;
  deliveryPeriodStart: string | null;
  deliveryPeriodEnd: string | null;
};
