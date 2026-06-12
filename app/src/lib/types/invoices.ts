// Invoices / Fakturaer + recurring-invoice wire types.
//
// All money fields below are kroner (DKK with decimals) — use `formatKroner`.

import type { FiscalYearEntry, StatementCompany } from "./common";

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
 * has never been sent as an e-faktura. `prepared` means an envelope has been
 * recorded; `acknowledged` means the access point confirmed receipt.
 */
export type InvoicePeppolStatus = {
  status: "prepared" | "acknowledged";
  submissionReference: string;
  transmissionId: string | null;
  acknowledgedAt: string | null;
};

export type CompanyInvoiceRow = {
  documentId: number;
  invoiceNo: string;
  invoiceDate: string | null;
  customerName: string | null;
  /**
   * Customer's e-mail when set on the kontaktkort (#429). The cockpit row
   * offers "Send på mail" only when this is present so the dialog can
   * prefill the recipient without a second round-trip.
   */
  customerEmail: string | null;
  /**
   * Buyer's EAN-number (13 digits) when set on the invoice payload. The
   * cockpit row offers "Send som e-faktura" only when this is present.
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
   * (#434), or `null` when no reminder has been sent yet. Surfaced so the row
   * can show "{n}. rykker sendt {dato}" under the status flag and the
   * "Send rykker" action knows whether further reminders are still allowed.
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
};

export type InvoicesResponse = {
  ok: true;
  invoices: CompanyInvoices;
};

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
  interval: "monthly" | "quarterly" | "yearly";
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
export type RecurringInterval = "monthly" | "quarterly" | "yearly";
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
