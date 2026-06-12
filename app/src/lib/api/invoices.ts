import type {
  InvoicesResponse,
  RecurringInvoiceGenerationResult,
  RecurringInvoiceTemplateCreatedResult,
  RecurringInvoiceTemplateInput,
  RecurringInvoicesResponse,
} from "../types";
import { ApiError, request } from "./_shared";

export const invoicesApi = {
  invoices: (slug: string, year?: string) =>
    request<InvoicesResponse>(
      `/api/companies/${encodeURIComponent(slug)}/invoices${
        year ? `?year=${encodeURIComponent(year)}` : ""
      }`,
    ).then((r) => r.invoices),

  /**
   * URL of an issued invoice's PDF — opened directly in a new browser tab so
   * the owner can download or forward it. URL builder rather than a fetch:
   * the server serves the file inline (#378).
   */
  invoicePdfUrl: (slug: string, id: number) =>
    `/api/companies/${encodeURIComponent(slug)}/invoices/${id}/pdf`,

  /** Recurring-invoice templates + their past generations for a company. */
  recurringInvoices: (slug: string) =>
    request<RecurringInvoicesResponse>(
      `/api/companies/${encodeURIComponent(slug)}/recurring-invoices`,
    ).then((r) => r.recurringInvoices),

  /**
   * Creates a recurring-invoice template (#386). The cockpit hands the server
   * a minimal `{name, interval, firstIssueDate, paymentTermsDays,
   * deliveryPeriodMode, notes, vatRatePercent, currency, customerId, buyer,
   * lines}`; the server runs `computeInvoiceAmounts` + `resolveInvoiceMasterData`
   * and then `createRecurringInvoiceTemplate` — the same core path the CLI's
   * `recurring-invoice create` uses. Write-irreversible (a template that
   * cannot be edited or deleted, only retired), so the body carries
   * `confirm: true` to match the surrounding write-routes.
   */
  createRecurringInvoiceTemplate: (
    slug: string,
    input: RecurringInvoiceTemplateInput,
  ) =>
    request<{ ok: true; template: RecurringInvoiceTemplateCreatedResult }>(
      `/api/companies/${encodeURIComponent(slug)}/recurring-invoices`,
      {
        method: "POST",
        body: JSON.stringify({ ...input, confirm: true }),
      },
    ).then((r) => r.template),

  /**
   * Generates the next invoice from a template for the given `asOfDate`. The
   * core is idempotent: a second call for the same period returns the existing
   * generation with `created: false`. Write-irreversible (issues an invoice
   * document), so the body carries `confirm: true`.
   */
  generateRecurringInvoice: (slug: string, templateId: number, asOfDate: string) =>
    request<{ ok: true; generation: RecurringInvoiceGenerationResult }>(
      `/api/companies/${encodeURIComponent(slug)}/recurring-invoices/${templateId}/generate`,
      {
        method: "POST",
        body: JSON.stringify({ asOfDate, confirm: true }),
      },
    ).then((r) => r.generation),

  /**
   * Retires (deactivates) a recurring-invoice template (#435). Templates are
   * append-only by schema, so this flips `active` from 1 -> 0 and audit-logs
   * the change; the cockpit hides the generate button on retired templates.
   * The trigger forbids reactivation — an owner who needs to change terms
   * creates a new template that supersedes the retired one.
   */
  retireRecurringInvoiceTemplate: (
    slug: string,
    templateId: number,
    reason?: string,
  ) =>
    request<{ ok: true; template: { id: number; retired: boolean } }>(
      `/api/companies/${encodeURIComponent(slug)}/recurring-invoices/${templateId}/retire`,
      {
        method: "POST",
        body: JSON.stringify({
          confirm: true,
          ...(reason ? { reason } : {}),
        }),
      },
    ).then((r) => r.template),

  /**
   * #440 — forhåndsviser en faktura uden at udstede. Same body as
   * `issueInvoice`, but the server runs `previewIssuedInvoicePdf` instead of
   * `issueInvoice`: no sequence draw, no `documents` row, no `audit_log`
   * entry. The response is the raw PDF (Content-Type application/pdf) so the
   * cockpit can open it in a new tab via `URL.createObjectURL`. Errors come
   * back as the regular `{ok:false,error}` envelope (400 for validation,
   * 409 for unknown customer/master-data, 404 for unknown company).
   */
  previewInvoice: async (
    slug: string,
    input: InvoiceIssueInput,
  ): Promise<Blob> => {
    const body = {
      issueDate: input.issueDate,
      lines: input.lines,
      ...(input.vatRatePercent !== undefined
        ? { vatRatePercent: input.vatRatePercent }
        : {}),
      ...(input.customerId ? { customerId: input.customerId } : {}),
      ...(input.invoiceNumber ? { invoiceNumber: input.invoiceNumber } : {}),
      ...(input.dueDate ? { dueDate: input.dueDate } : {}),
      ...(input.currency ? { currency: input.currency } : {}),
      ...(input.seller ? { seller: input.seller } : {}),
      ...(input.buyer ? { buyer: input.buyer } : {}),
    };
    let res: Response;
    try {
      res = await fetch(
        `/api/companies/${encodeURIComponent(slug)}/invoices/preview`,
        {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify(body),
        },
      );
    } catch {
      throw new ApiError(
        "network",
        "Kunne ikke nå serveren. Kører `rentemester serve`?",
        0,
      );
    }
    if (!res.ok) {
      let message = `HTTP ${res.status}`;
      let code = "internal";
      try {
        // #368: unified envelope — `errors[0]` is the message, `code` is the
        // top-level enum. Mirrors `_shared.ts`/`accountant.ts`; the old
        // `{ error: { code, message } }` shape is dead and swallowed errors.
        const errBody = (await res.json()) as {
          errors?: unknown;
          code?: unknown;
        };
        if (Array.isArray(errBody.errors) && errBody.errors.length > 0) {
          message = String(errBody.errors[0]);
        }
        if (typeof errBody.code === "string") code = errBody.code;
      } catch {}
      throw new ApiError(code, message, res.status);
    }
    return await res.blob();
  },

  /**
   * Issues a sales invoice (#213, slice 4). The human enters the customer and
   * the line items; the server COMPUTES every line total, net, VAT and gross
   * via the same core path the CLI's `invoice create` uses — the human never
   * does invoice arithmetic. Issuing is a kladde (no journal entry yet), so no
   * `confirm` is required.
   */
  issueInvoice: (slug: string, input: InvoiceIssueInput) =>
    request<{ ok: true; invoice: InvoiceIssueSummary }>(
      `/api/companies/${encodeURIComponent(slug)}/invoices/issue`,
      {
        method: "POST",
        body: JSON.stringify({
          issueDate: input.issueDate,
          lines: input.lines,
          ...(input.vatRatePercent !== undefined
            ? { vatRatePercent: input.vatRatePercent }
            : {}),
          ...(input.customerId ? { customerId: input.customerId } : {}),
          ...(input.invoiceNumber ? { invoiceNumber: input.invoiceNumber } : {}),
          ...(input.dueDate ? { dueDate: input.dueDate } : {}),
          ...(input.currency ? { currency: input.currency } : {}),
          ...(input.seller ? { seller: input.seller } : {}),
          ...(input.buyer ? { buyer: input.buyer } : {}),
        }),
      },
    ).then((r) => r.invoice),

  /**
   * Settles an issued invoice against a bank payment (#213, slice 4). The
   * human identifies the bank receipt by its transaction id or reference.
   * Write-irreversible, so the body carries `confirm: true`.
   */
  settleInvoice: (slug: string, input: InvoiceSettleInput) =>
    request<{ ok: true; settlement: InvoiceSettleSummary }>(
      `/api/companies/${encodeURIComponent(slug)}/invoices/settle`,
      {
        method: "POST",
        body: JSON.stringify({
          invoiceDocumentId: input.invoiceDocumentId,
          ...(input.bankTransactionId
            ? { bankTransactionId: input.bankTransactionId }
            : {}),
          ...(input.bankTransactionReference
            ? { bankTransactionReference: input.bankTransactionReference }
            : {}),
          ...(input.paymentDate ? { paymentDate: input.paymentDate } : {}),
          ...(input.amount !== undefined ? { amount: input.amount } : {}),
          confirm: true,
        }),
      },
    ).then((r) => r.settlement),

  /**
   * Issues a credit note for an already-issued sales invoice (#412). The
   * human supplies the source invoice + a mandatory reason; Rentemester
   * computes the credit amount from the original gross unless `grossAmount`
   * is given. Write-irreversible (it inserts a credit-note document AND
   * appends a reversal journal entry), so the body carries `confirm: true`.
   */
  creditInvoice: (slug: string, input: InvoiceCreditNoteInput) =>
    request<{ ok: true; creditNote: InvoiceCreditNoteSummary }>(
      `/api/companies/${encodeURIComponent(slug)}/invoices/credit-note`,
      {
        method: "POST",
        body: JSON.stringify({
          invoiceDocumentId: input.invoiceDocumentId,
          issueDate: input.issueDate,
          reason: input.reason,
          ...(input.grossAmount !== undefined
            ? { grossAmount: input.grossAmount }
            : {}),
          ...(input.creditNoteNumber
            ? { creditNoteNumber: input.creditNoteNumber }
            : {}),
          confirm: true,
        }),
      },
    ).then((r) => r.creditNote),

  /**
   * #429 — sends an issued invoice to the customer's e-mail with the PDF
   * attached, from the cockpit. Third caller of the SAME `sendInvoiceEmail`
   * core function the CLI's `invoice send` command and the MCP tool
   * `invoice_send_email` use, so the cockpit and the terminal produce a
   * byte-identical MIME message and `email_send_log` row.
   *
   * SMTP CONFIG (host/port/fromAddress + optional username/password) is read
   * server-side from `config/smtp.json` in the company directory — credentials
   * never enter the request body. Idempotent: an identical send collapses onto
   * the existing send-log row instead of re-transmitting. Write-irreversible
   * (an `email_send_log` row + an `audit_log` entry), so the body carries
   * `confirm: true`.
   */
  sendInvoiceByEmail: (slug: string, input: InvoiceSendEmailInput) =>
    request<{ ok: true; delivery: InvoiceSendEmailSummary }>(
      `/api/companies/${encodeURIComponent(slug)}/invoices/send-email`,
      {
        method: "POST",
        body: JSON.stringify({
          invoiceDocumentId: input.invoiceDocumentId,
          to: input.to,
          ...(input.kind ? { kind: input.kind } : {}),
          confirm: true,
        }),
      },
    ).then((r) => r.delivery),

  /**
   * #434 — sends a payment reminder (betalingspaamindelse) for an overdue
   * issued invoice. Single endpoint that combines three core calls so the
   * cockpit's "Send rykker" action is a one-click write:
   *   1. `registerInvoiceReminder` — records the reminder + statutory fee
   *      (max 100 kr/reminder, max 3 reminders per rentel. § 9b),
   *   2. (optional, when `bookFee` is true) `postInvoiceReminderToLedger` —
   *      journals the fee against the customer receivable,
   *   3. `sendInvoiceEmail` with `kind: 'reminder'` — same SMTP transport as
   *      "Send på mail", so the message is byte-identical to a CLI send.
   *
   * Write-irreversible (it inserts an `invoice_reminders` row, may append a
   * journal entry, and always appends an `email_send_log` + `audit_log`
   * row), so the body carries `confirm: true`.
   */
  sendInvoiceReminder: (slug: string, input: InvoiceSendReminderInput) =>
    request<{ ok: true; reminder: InvoiceSendReminderSummary }>(
      `/api/companies/${encodeURIComponent(slug)}/invoices/send-reminder`,
      {
        method: "POST",
        body: JSON.stringify({
          invoiceDocumentId: input.invoiceDocumentId,
          to: input.to,
          bookFee: input.bookFee,
          ...(input.feeAmount !== undefined ? { feeAmount: input.feeAmount } : {}),
          confirm: true,
        }),
      },
    ).then((r) => r.reminder),

  /**
   * #428 — sends an issued invoice as an e-faktura (NemHandel/PEPPOL) via
   * the cockpit. Third caller of the SAME `submitPublicEInvoicePeppol` core
   * function the CLI / MCP use; the server loads its access-point config
   * from `RENTEMESTER_PEPPOL_ACCESS_POINT` so credentials never enter the
   * body. Write-irreversible (it appends a `peppol_submissions` row + an
   * `audit_log` entry), so the body carries `confirm: true`.
   */
  sendInvoiceAsEInvoice: (slug: string, input: InvoiceSendEInvoiceInput) =>
    request<{ ok: true; submission: InvoiceSendEInvoiceSummary }>(
      `/api/companies/${encodeURIComponent(slug)}/invoices/send-public`,
      {
        method: "POST",
        body: JSON.stringify({
          invoiceDocumentId: input.invoiceDocumentId,
          confirm: true,
        }),
      },
    ).then((r) => r.submission),
};

/** A single invoice line as the human enters it — Rentemester computes totals. */
export type InvoiceLineInput = {
  description: string;
  quantity: number;
  unitPriceExVat: number;
};

/** An invoice party (seller / buyer) — all fields optional. */
export type InvoicePartyInput = {
  name?: string;
  address?: string;
  vatOrCvr?: string;
};

/** Input for `api.issueInvoice`. */
export type InvoiceIssueInput = {
  issueDate: string;
  lines: InvoiceLineInput[];
  vatRatePercent?: number;
  customerId?: number;
  invoiceNumber?: string;
  dueDate?: string;
  currency?: string;
  seller?: InvoicePartyInput;
  buyer?: InvoicePartyInput;
};

/** The issue result the server echoes back — every amount Rentemester computed. */
export type InvoiceIssueSummary = {
  documentId: number | null;
  invoiceNumber: string | null;
  netAmount: number;
  vatRate: number;
  vatAmount: number;
  grossAmount: number;
  lines: Array<InvoiceLineInput & { lineTotalExVat: number }>;
};

/** Input for `api.settleInvoice`. */
export type InvoiceSettleInput = {
  invoiceDocumentId: number;
  bankTransactionId?: number;
  bankTransactionReference?: string;
  paymentDate?: string;
  amount?: number;
};

/** The settlement result the server echoes back. */
export type InvoiceSettleSummary = {
  entryId: number | null;
  paymentId: number | null;
  principalAmount: number;
  claimAmount: number;
  invoiceNumber: string | null;
  openBalance: number | null;
};

/** Input for `api.creditInvoice` (#412). */
export type InvoiceCreditNoteInput = {
  invoiceDocumentId: number;
  issueDate: string;
  reason: string;
  /** Optional partial credit. When omitted, the remaining gross is credited. */
  grossAmount?: number;
  creditNoteNumber?: string;
};

/** Input for `api.sendInvoiceAsEInvoice` (#428). */
export type InvoiceSendEInvoiceInput = {
  invoiceDocumentId: number;
};

/** The PEPPOL submission result the server echoes back (#428). */
export type InvoiceSendEInvoiceSummary = {
  invoiceNumber: string | null;
  submissionReference: string | null;
  status: "prepared" | "acknowledged" | null;
  duplicate: boolean;
  envelopeSha256: string | null;
  oioublSha256: string | null;
};

/** Input for `api.sendInvoiceByEmail` (#429). */
export type InvoiceSendEmailInput = {
  invoiceDocumentId: number;
  /** Recipient email address — prefilled from the customer but editable. */
  to: string;
  /** What to send (#429): the invoice itself or a payment reminder. */
  kind?: "invoice" | "reminder";
};

/** Input for `api.sendInvoiceReminder` (#434). */
export type InvoiceSendReminderInput = {
  invoiceDocumentId: number;
  /** Recipient email address — prefilled from the customer but editable. */
  to: string;
  /**
   * Whether to also book the statutory reminder fee (max 100 kr) against the
   * receivable account. When false the reminder is only registered + emailed.
   */
  bookFee: boolean;
  /** Optional override of the default 100 kr reminder fee. */
  feeAmount?: number;
};

/** The reminder-send result the server echoes back (#434). */
export type InvoiceSendReminderSummary = {
  invoiceNumber: string | null;
  recipient: string | null;
  /** 1, 2 or 3 — which reminder in the statutory series this one was. */
  reminderSequence: number | null;
  /** Fee amount registered (kroner). */
  feeAmount: number | null;
  /** True when the fee was also booked to the receivable. */
  feeBooked: boolean;
  /** Journal entry number when the fee was booked, else null. */
  journalEntryNo: string | null;
  /** Message-id of the reminder e-mail. */
  messageId: string | null;
  /** True when this is a re-send that collapsed onto an existing send-log row. */
  duplicate: boolean;
};

/** The email-delivery result the server echoes back (#429). */
export type InvoiceSendEmailSummary = {
  invoiceNumber: string | null;
  recipient: string | null;
  subject: string | null;
  messageId: string | null;
  duplicate: boolean;
};

/** The credit-note result the server echoes back. */
export type InvoiceCreditNoteSummary = {
  documentId: number | null;
  creditNoteNumber: string | null;
  originalInvoiceNumber: string | null;
  journalEntryId: number | null;
  journalEntryNo: string | null;
};
