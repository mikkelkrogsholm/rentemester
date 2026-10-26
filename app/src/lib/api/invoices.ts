import type { InvoicesResponse, ImportedReceivablesResponse, RecurringInvoiceGenerationResult, RecurringInvoicesResponse, RecurringInvoiceTemplateCreatedResult, RecurringInvoiceTemplateInput } from "../types";
import { type ReadRequestOptions, request, requestBlob } from "./_shared";
/** Preview and issue must serialize the exact same invoice facts. */
function invoiceRequestPayload(input: InvoiceIssueInput) {
  return {
    issueDate: input.issueDate,
    ...(input.deliveryDate !== undefined ? {
      deliveryDate: input.deliveryDate
    } : {}),
    ...(input.deliveryPeriodStart !== undefined ? {
      deliveryPeriodStart: input.deliveryPeriodStart
    } : {}),
    ...(input.deliveryPeriodEnd !== undefined ? {
      deliveryPeriodEnd: input.deliveryPeriodEnd
    } : {}),
    ...(input.unitCode !== undefined ? {
      unitCode: input.unitCode
    } : {}),
    ...(input.invoiceType !== undefined ? {
      invoiceType: input.invoiceType
    } : {}),
    ...(input.totals !== undefined ? {
      totals: input.totals
    } : {}),
    lines: input.lines,
    ...(input.vatTreatment !== undefined ? {
      vatTreatment: input.vatTreatment
    } : {}),
    ...(input.reverseChargeBasis !== undefined ? {
      reverseChargeBasis: input.reverseChargeBasis
    } : {}),
    ...(input.reverseChargeNote !== undefined ? {
      reverseChargeNote: input.reverseChargeNote
    } : {}),
    ...(input.vatRatePercent !== undefined ? {
      vatRatePercent: input.vatRatePercent
    } : {}),
    ...(input.customerId ? {
      customerId: input.customerId
    } : {}),
    ...(input.invoiceNumber ? {
      invoiceNumber: input.invoiceNumber
    } : {}),
    ...(input.dueDate ? {
      dueDate: input.dueDate
    } : {}),
    ...(input.currency ? {
      currency: input.currency
    } : {}),
    ...(input.fxRateToDkk !== undefined ? {
      fxRateToDkk: input.fxRateToDkk
    } : {}),
    ...(input.seller ? {
      seller: input.seller
    } : {}),
    ...(input.buyer ? {
      buyer: input.buyer
    } : {})
  };
}
export const invoicesApi = {
  writeOffNativeInvoiceBadDebt: (slug: string, body: NativeInvoiceBadDebtInput) => request<{
    ok: true;
    writeOff: NativeInvoiceBadDebtReceipt;
  }>(`/api/companies/${encodeURIComponent(slug)}/invoices/bad-debt/write-off`, {
    method: "POST",
    body: JSON.stringify(body)
  }).then(r => r.writeOff),
  calculateNativeInvoiceInterest: (slug: string, body: NativeInvoiceInterestCalculate) => request<{
    ok: true;
    interest: NativeInvoiceInterestResult;
  }>(`/api/companies/${encodeURIComponent(slug)}/invoices/interest/calculate`, {
    method: "POST",
    body: JSON.stringify(body),
    readOnly: true
  }).then(r => r.interest),
  registerNativeInvoiceInterest: (slug: string, body: NativeInvoiceInterestCalculate & {
    note?: string;
    confirm: true;
  }) => request(`/api/companies/${encodeURIComponent(slug)}/invoices/interest/register`, {
    method: "POST",
    body: JSON.stringify(body)
  }),
  postNativeInvoiceInterest: (slug: string, body: NativeInvoiceInterestPost) => request(`/api/companies/${encodeURIComponent(slug)}/invoices/interest/post`, {
    method: "POST",
    body: JSON.stringify(body)
  }),
  calculateNativeInvoiceInterestCorrection: (slug: string, body: {
    invoiceDocumentId: number;
  }) => request<{
    ok: true;
    interestCorrection: NativeInvoiceInterestCorrection;
  }>(`/api/companies/${encodeURIComponent(slug)}/invoices/interest/correction`, {
    method: "POST",
    body: JSON.stringify(body),
    readOnly: true
  }).then(r => r.interestCorrection),
  postNativeInvoiceInterestCorrection: (slug: string, body: NativeInvoiceInterestCorrectionPost) => request(`/api/companies/${encodeURIComponent(slug)}/invoices/interest/post-correction`, {
    method: "POST",
    body: JSON.stringify(body)
  }),
  calculateNativeInvoiceCompensation: (slug: string, body: NativeInvoiceCompensationCalculate) => request<{
    ok: true;
    compensation: NativeInvoiceCompensationResult;
  }>(`/api/companies/${encodeURIComponent(slug)}/invoices/compensation/calculate`, {
    method: "POST",
    body: JSON.stringify(body),
    readOnly: true
  }).then(r => r.compensation),
  registerNativeInvoiceCompensation: (slug: string, body: NativeInvoiceCompensationCalculate & {
    note?: string;
    confirm: true;
  }) => request<{
    ok: true;
    compensation: NativeInvoiceCompensationResult;
  }>(`/api/companies/${encodeURIComponent(slug)}/invoices/compensation/register`, {
    method: "POST",
    body: JSON.stringify(body)
  }).then(r => r.compensation),
  postNativeInvoiceCompensation: (slug: string, body: {
    invoiceDocumentId: number;
    transactionDate?: string;
    receivableAccountNo: string;
    compensationIncomeAccountNo: string;
    confirm: true;
  }) => request<{
    ok: true;
    compensation: NativeInvoiceCompensationResult;
  }>(`/api/companies/${encodeURIComponent(slug)}/invoices/compensation/post`, {
    method: "POST",
    body: JSON.stringify(body)
  }).then(r => r.compensation),
  publicEInvoiceConfig: (slug: string, options?: ReadRequestOptions) => request<{
    ok: true;
    publicEInvoice: PublicEInvoiceConfig;
  }>(`/api/companies/${encodeURIComponent(slug)}/invoices/public-config`, options).then(r => r.publicEInvoice),
  registerPublicEInvoice: (slug: string) => request<{
    ok: true;
    registration: {
      registered: true;
      outbound: true;
      environment: "test" | "production";
    };
  }>(`/api/companies/${encodeURIComponent(slug)}/invoices/public-register`, {
    method: "POST",
    body: JSON.stringify({
      confirm: true
    })
  }).then(r => r.registration),
  invoiceReminderClaims: (slug: string, id: number) => request<{
    ok: true;
    reminderClaims: NativeReminderClaimStatus;
  }>(`/api/companies/${encodeURIComponent(slug)}/invoices/${id}/reminder-claims`).then(r => r.reminderClaims),
  registerNativeReminder: (slug: string, body: {
    invoiceDocumentId: number;
    reminderDate: string;
    feeAmount: number;
    note?: string;
    confirm: true;
  }) => request(`/api/companies/${encodeURIComponent(slug)}/invoices/reminders/register`, {
    method: "POST",
    body: JSON.stringify(body)
  }),
  postNativeReminder: (slug: string, body: {
    invoiceDocumentId: number;
    reminderId: number;
    transactionDate?: string;
    receivableAccountNo: string;
    reminderIncomeAccountNo: string;
    confirm: true;
  }) => request(`/api/companies/${encodeURIComponent(slug)}/invoices/reminders/post`, {
    method: "POST",
    body: JSON.stringify(body)
  }),
  settleNativeInvoiceClaims: (slug: string, body: {
    invoiceDocumentId: number;
    bankTransactionId: number;
    confirm: true;
  }) => request(`/api/companies/${encodeURIComponent(slug)}/invoices/claims/settle`, {
    method: "POST",
    body: JSON.stringify(body)
  }),
  refundInvoice: (slug: string, invoiceDocumentId: number, bankTransactionReference: string) => request<{
    ok: true;
    refund: {
      entryId: number;
      refundId: number;
      invoiceNumber: string;
      remainingCreditBalance: number;
    };
  }>(`/api/companies/${encodeURIComponent(slug)}/invoices/refund`, {
    method: "POST",
    body: JSON.stringify({
      invoiceDocumentId,
      bankTransactionReference,
      confirm: true
    })
  }).then(r => r.refund),
  postNativeInvoice: (slug: string, invoiceDocumentId: number, revenueAccountNo: string) => request<{
    ok: true;
    posting: {
      entryId: number;
      entryNo: string;
    };
  }>(`/api/companies/${encodeURIComponent(slug)}/invoices/post`, {
    method: "POST",
    body: JSON.stringify({
      confirm: true,
      invoiceDocumentId,
      revenueAccountNo
    })
  }).then(r => r.posting),
  invoices: (slug: string, year?: string, options?: ReadRequestOptions) => request<InvoicesResponse>(`/api/companies/${encodeURIComponent(slug)}/invoices${year ? `?year=${encodeURIComponent(year)}` : ""}`, options).then(r => r.invoices),
  importedReceivables: (slug: string, asOf: string, options?: ReadRequestOptions) => request<ImportedReceivablesResponse>(`/api/companies/${encodeURIComponent(slug)}/imported-receivables?asOf=${encodeURIComponent(asOf)}`, options).then(r => r.importedReceivables),
  planImportedReceivableSettlement: (slug: string, input: {
    scheduleHash: string;
    externalInvoiceId: string;
    bankTransactionId: number;
  }) => request<{
    ok: true;
    settlement: {
      ok: boolean;
      plan?: {
        planHash: string;
      };
    };
  }>(`/api/companies/${encodeURIComponent(slug)}/imported-receivables/settlement/plan`, {
    method: "POST",
    body: JSON.stringify(input)
  }).then(r => r.settlement),
  applyImportedReceivableSettlement: (slug: string, input: {
    scheduleHash: string;
    externalInvoiceId: string;
    bankTransactionId: number;
    planHash: string;
    idempotencyKey: string;
  }) => request<{
    ok: true;
    settlement: {
      ok: boolean;
      errors?: string[];
    };
  }>(`/api/companies/${encodeURIComponent(slug)}/imported-receivables/settlement/apply`, {
    method: "POST",
    body: JSON.stringify({
      ...input,
      confirm: true
    })
  }).then(r => r.settlement),
  /**
   * URL of an issued invoice's PDF — opened directly in a new browser tab so
   * the owner can download or forward it. URL builder rather than a fetch:
   * the server serves the file inline (#378).
   */
  invoicePdfUrl: (slug: string, id: number) => `/api/companies/${encodeURIComponent(slug)}/invoices/${id}/pdf`,
  /** Recurring-invoice templates + their past generations for a company. */
  recurringInvoices: (slug: string, options?: ReadRequestOptions) => request<RecurringInvoicesResponse>(`/api/companies/${encodeURIComponent(slug)}/recurring-invoices`, options).then(r => r.recurringInvoices),
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
  createRecurringInvoiceTemplate: (slug: string, input: RecurringInvoiceTemplateInput) => request<{
    ok: true;
    template: RecurringInvoiceTemplateCreatedResult;
  }>(`/api/companies/${encodeURIComponent(slug)}/recurring-invoices`, {
    method: "POST",
    body: JSON.stringify({
      ...input,
      confirm: true
    })
  }).then(r => r.template),
  /**
   * Generates the next invoice from a template for the given `asOfDate`. The
   * core is idempotent: a second call for the same period returns the existing
   * generation with `created: false`. Write-irreversible (issues an invoice
   * document), so the body carries `confirm: true`.
   */
  generateRecurringInvoice: (slug: string, templateId: number, asOfDate: string) => request<{
    ok: true;
    generation: RecurringInvoiceGenerationResult;
  }>(`/api/companies/${encodeURIComponent(slug)}/recurring-invoices/${templateId}/generate`, {
    method: "POST",
    body: JSON.stringify({
      asOfDate,
      confirm: true
    })
  }).then(r => r.generation),
  /**
   * Retires (deactivates) a recurring-invoice template (#435). Templates are
   * append-only by schema, so this flips `active` from 1 -> 0 and audit-logs
   * the change; the cockpit hides the generate button on retired templates.
   * The trigger forbids reactivation — an owner who needs to change terms
   * creates a new template that supersedes the retired one.
   */
  retireRecurringInvoiceTemplate: (slug: string, templateId: number, reason?: string) => request<{
    ok: true;
    template: {
      id: number;
      retired: boolean;
    };
  }>(`/api/companies/${encodeURIComponent(slug)}/recurring-invoices/${templateId}/retire`, {
    method: "POST",
    body: JSON.stringify({
      confirm: true,
      ...(reason ? {
        reason
      } : {})
    })
  }).then(r => r.template),
  /**
   * #440 — forhåndsviser en faktura uden at udstede. Same body as
   * `issueInvoice`, but the server runs `previewIssuedInvoicePdf` instead of
   * `issueInvoice`: no sequence draw, no `documents` row, no `audit_log`
   * entry. The response is the raw PDF (Content-Type application/pdf) so the
   * cockpit can open it in a new tab via `URL.createObjectURL`. Errors come
   * back as the regular `{ok:false,error}` envelope (400 for validation,
   * 409 for unknown customer/master-data, 404 for unknown company).
   */
  previewInvoice: async (slug: string, input: InvoiceIssueInput): Promise<Blob> => {
    const response = await requestBlob(`/api/companies/${encodeURIComponent(slug)}/invoices/preview`, {
      method: "POST",
      body: JSON.stringify(invoiceRequestPayload(input)),
      readOnly: true
    });
    return response.blob;
  },
  /**
   * Issues a sales invoice (#213, slice 4). The human enters the customer and
   * the line items; the server COMPUTES every line total, net, VAT and gross
   * via the same core path the CLI's `invoice create` uses — the human never
   * does invoice arithmetic. Issuing is a kladde (no journal entry yet), so no
   * `confirm` is required.
   */
  issueInvoice: (slug: string, input: InvoiceIssueInput) => request<{
    ok: true;
    invoice: InvoiceIssueSummary;
  }>(`/api/companies/${encodeURIComponent(slug)}/invoices/issue`, {
    method: "POST",
    body: JSON.stringify(invoiceRequestPayload(input))
  }).then(r => r.invoice),
  /**
   * Settles an issued invoice against a bank payment (#213, slice 4). The
   * human identifies the bank receipt by its transaction id or reference.
   * Write-irreversible, so the body carries `confirm: true`.
   */
  settleInvoice: (slug: string, input: InvoiceSettleInput) => request<{
    ok: true;
    settlement: InvoiceSettleSummary;
  }>(`/api/companies/${encodeURIComponent(slug)}/invoices/settle`, {
    method: "POST",
    body: JSON.stringify({
      invoiceDocumentId: input.invoiceDocumentId,
      ...(input.bankTransactionId ? {
        bankTransactionId: input.bankTransactionId
      } : {}),
      ...(input.bankTransactionReference ? {
        bankTransactionReference: input.bankTransactionReference
      } : {}),
      ...(input.paymentDate ? {
        paymentDate: input.paymentDate
      } : {}),
      ...(input.amount !== undefined ? {
        amount: input.amount
      } : {}),
      confirm: true
    })
  }).then(r => r.settlement),
  /**
   * Issues a credit note for an already-issued sales invoice (#412). The
   * human supplies the source invoice + a mandatory reason; Rentemester
   * computes the credit amount from the original gross unless `grossAmount`
   * is given. Write-irreversible (it inserts a credit-note document AND
   * appends a reversal journal entry), so the body carries `confirm: true`.
   */
  creditInvoice: (slug: string, input: InvoiceCreditNoteInput) => request<{
    ok: true;
    creditNote: InvoiceCreditNoteSummary;
  }>(`/api/companies/${encodeURIComponent(slug)}/invoices/credit-note`, {
    method: "POST",
    body: JSON.stringify({
      invoiceDocumentId: input.invoiceDocumentId,
      issueDate: input.issueDate,
      reason: input.reason,
      ...(input.grossAmount !== undefined ? {
        grossAmount: input.grossAmount
      } : {}),
      ...(input.creditNoteNumber ? {
        creditNoteNumber: input.creditNoteNumber
      } : {}),
      confirm: true
    })
  }).then(r => r.creditNote),
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
  sendInvoiceByEmail: (slug: string, input: InvoiceSendEmailInput) => request<{
    ok: true;
    delivery: InvoiceSendEmailSummary;
  }>(`/api/companies/${encodeURIComponent(slug)}/invoices/send-email`, {
    method: "POST",
    body: JSON.stringify({
      invoiceDocumentId: input.invoiceDocumentId,
      to: input.to,
      ...(input.kind ? {
        kind: input.kind
      } : {}),
      confirm: true
    })
  }).then(r => r.delivery),
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
  sendInvoiceReminder: (slug: string, input: InvoiceSendReminderInput) => request<{
    ok: true;
    reminder: InvoiceSendReminderSummary;
  }>(`/api/companies/${encodeURIComponent(slug)}/invoices/send-reminder`, {
    method: "POST",
    body: JSON.stringify({
      invoiceDocumentId: input.invoiceDocumentId,
      to: input.to,
      bookFee: input.bookFee,
      ...(input.feeAmount !== undefined ? {
        feeAmount: input.feeAmount
      } : {}),
      confirm: true
    })
  }).then(r => r.reminder),
  /**
   * Sends through the selected company's local DigiSense binding. The browser
   * never supplies a company key, access-point identity or credentials.
   */
  sendInvoiceAsEInvoice: (slug: string, input: InvoiceSendEInvoiceInput) => request<{
    ok: true;
    submission: InvoiceSendEInvoiceSummary;
  }>(`/api/companies/${encodeURIComponent(slug)}/invoices/send-public`, {
    method: "POST",
    body: JSON.stringify({
      invoiceDocumentId: input.invoiceDocumentId,
      confirm: true
    })
  }).then(r => r.submission),
  /** Observes a queued DigiSense delivery without ever redelivering it. */
  refreshEInvoiceStatus: (slug: string, input: InvoiceSendEInvoiceInput) => request<{
    ok: true;
    submission: InvoiceSendEInvoiceSummary;
  }>(`/api/companies/${encodeURIComponent(slug)}/invoices/send-public/status`, {
    method: "POST",
    body: JSON.stringify({
      invoiceDocumentId: input.invoiceDocumentId,
      confirm: true
    })
  }).then(r => r.submission)
};
/** A single invoice line as the human enters it — Rentemester computes totals. */
export type InvoiceLineInput = {
  description: string;
  quantity: number;
  unitPriceExVat: number;
  unitCode?: string;
  taxClassification?: InvoiceTaxClassification;
  vatRate?: number;
  reverseChargeBasis?: InvoiceReverseChargeBasis;
};
/** An invoice party (seller / buyer) — all fields optional. */
export type InvoicePartyInput = {
  name?: string;
  address?: string;
  vatOrCvr?: string;
  eanNumber?: string;
  publicRecipient?: boolean;
  buyerReference?: string;
  orderReference?: string;
};
export type InvoiceTaxClassification = "taxable" | "exempt" | "reverse_charge";
export type ForeignReverseChargeBasis = "EU_B2B_SERVICES" | "DK_MOMSLOVEN_§46_STK_1_NR_3" | "EU_MOMSDIREKTIV_ART_196" | "EU_MOMSDIREKTIV_ART_199";
export type InvoiceReverseChargeBasis = DomesticReverseChargeBasis | ForeignReverseChargeBasis;
export type DomesticReverseChargeBasis = "DK_MOMSLOVEN_§46_STK_1_NR_6" | "DK_MOMSLOVEN_§46_STK_1_NR_7";
/** Input for `api.issueInvoice`. */
type InvoiceIssueCommon = {
  issueDate: string;
  deliveryDate?: string;
  deliveryPeriodStart?: string;
  deliveryPeriodEnd?: string;
  unitCode?: string;
  vatRatePercent?: number;
  vatTreatment?: "standard" | "domestic_reverse_charge" | "foreign_reverse_charge";
  reverseChargeBasis?: InvoiceReverseChargeBasis;
  reverseChargeNote?: string;
  customerId?: number;
  invoiceNumber?: string;
  dueDate?: string;
  currency?: string;
  fxRateToDkk?: number;
  seller?: InvoicePartyInput;
  buyer?: InvoicePartyInput;
};
export type InvoiceIssueInput = InvoiceIssueCommon & ({
  invoiceType?: "full";
  lines: InvoiceLineInput[];
  totals?: never;
} | {
  invoiceType: "simplified";
  lines: Array<{
    description: string;
  }>;
  totals: {
    grossAmount: number;
    vatRate: 0.25;
    vatComputationBasis: "VAT_20_OF_GROSS";
  };
});
/** The issue result the server echoes back — every amount Rentemester computed. */
export type InvoiceIssueSummary = {
  deliveryDate?: string;
  deliveryPeriodStart?: string;
  deliveryPeriodEnd?: string;
  unitCode?: string;
  invoiceType?: "full" | "simplified";
  currency?: string;
  fxRateToDkk?: number;
  netAmountDkk?: number;
  vatAmountDkk?: number;
  grossAmountDkk?: number;
  documentId: number | null;
  invoiceNumber: string | null;
  netAmount: number;
  vatRate?: number | null;
  vatAmount?: number | null;
  vatTreatment?: "standard" | "domestic_reverse_charge" | "foreign_reverse_charge";
  reverseChargeBasis?: InvoiceReverseChargeBasis | null;
  reverseChargeNote?: string | null;
  grossAmount: number;
  lines: Array<Partial<InvoiceLineInput> & {
    description: string;
    lineTotalExVat?: number;
    vatBase?: number;
    vatAmount?: number;
  }>;
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
  status: "prepared" | "queued" | "acknowledged" | "failed" | "uncertain" | "legacy" | null;
  redeliveryBlocked?: boolean;
  legacy?: boolean;
  providerStatus?: string | null;
  duplicate: boolean;
  transmissionId?: string | null;
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
export type NativeReminderClaimStatus = {
  invoiceDocumentId: number;
  invoiceNumber: string;
  asOfDate: string;
  dueDate: string;
  principalOpenBalance: number;
  principalCurrency: string;
  claimCurrency: "DKK";
  claimOpenBalance: number | null;
  totalBadDebtWrittenOff: number;
  totalBadDebtWrittenOffDkk: number;
  badDebtWriteOffs: NativeInvoiceBadDebtWriteOff[];
  totalCompensationDkk: number;
  totalInterestDkk: number;
  totalInterestCorrectionsDkk: number;
  pendingUnpostedInterest: boolean;
  registeredClaimBalanceDkk: number;
  pendingUnpostedCompensation: boolean;
  compensations: Array<{
    claimId: number;
    claimDate: string;
    amountDkk: number;
    journalEntryId: number | null;
  }>;
  totalReminderFees: number;
  totalClaimPayments: number;
  ledgerBackedClaimBalance: number;
  pendingUnpostedReminders: boolean;
  settlementAvailable: boolean;
  reminders: Array<{
    reminderId: number;
    reminderDate: string;
    feeAmount: number;
    journalEntryId: number | null;
  }>;
  claimPayments: Array<{
    claimPaymentId: number;
    paymentDate: string;
    amount: number;
    bankTransactionId: number;
    journalEntryId: number;
  }>;
  accounts: Array<{
    accountNo: string;
    name: string;
    type: string;
    normalBalance: string;
  }>;
  incomingBankTransactions: Array<{
    id: number;
    date: string;
    text: string;
    amount: number;
    reference: string | null;
    bankAccountNo: string | null;
    sourceHash: string | null;
    transactionHash: string | null;
  }>;
  rules: {
    ruleId: string;
    maximumFeeAmount: number;
    maximumReminders: number;
    minimumDaysBetweenReminders: number;
  };
  delivery: {
    available: false;
    sentByThisFlow: false;
    priorDeliveryStatus: "unavailable";
    note: string;
  };
};
/** Safe operator configuration; no credentials or provider company key. */
export type PublicEInvoiceConfig = {
  configured: boolean;
  environment: "test" | "production" | null;
  registered: boolean;
  outbound: boolean;
};
export type NativeInvoiceCompensationCalculate = {
  invoiceDocumentId: number;
  asOfDate: string;
  compensationAmountDkk?: number;
};
export type NativeInvoiceCompensationResult = {
  invoiceDocumentId: number;
  invoiceNumber: string;
  asOfDate: string;
  effectiveDueDate: string;
  overdueDays: number;
  principalOpenBalance: number;
  principalCurrency: string;
  eligible: boolean;
  isCommercialTransaction: boolean;
  compensationAmountDkk: number;
  reason: string | null;
  appliedRules: string[];
  errors: string[];
  claimId: number | null;
  claimDate: string | null;
  registeredAmountDkk: number | null;
  postedJournalEntryId: number | null;
  postedJournalEntryNo: string | null;
};
export type NativeInvoiceInterestCalculate = {
  invoiceDocumentId: number;
  asOfDate: string;
  referenceRatePercent?: number;
};
export type NativeInvoiceInterestPost = {
  invoiceDocumentId: number;
  claimId: number;
  transactionDate: string;
  receivableAccountNo: string;
  interestIncomeAccountNo: string;
  confirm: true;
};
export type NativeInvoiceInterestCorrectionPost = {
  invoiceDocumentId: number;
  transactionDate: string;
  reason: string;
  confirm: true;
};
export type NativeInvoiceInterestResult = {
  invoiceDocumentId: number;
  invoiceNumber: string;
  principalCurrency: string;
  asOfDate: string;
  effectiveDueDate: string;
  overdueDays: number;
  interestFromDate: string;
  claimableDays: number;
  principalOpenBalance: number;
  referenceRatePercent: number;
  annualInterestRatePercent: number;
  referenceRateSource: string;
  accruedInterestAmount: number;
  priorClaimedInterest: number;
  totalInterestToDate: number;
  overClaimedInterest: number;
  warnings: string[];
  errors?: string[];
  claims: Array<{
    id: number;
    claimDate: string;
    referenceRatePercent: number;
    annualInterestRatePercent: number;
    referenceRateSource: string;
    interestAmount: number;
    postedJournalEntryId: number | null;
    postedJournalEntryNo: string | null;
  }>;
};
export type NativeInvoiceInterestCorrection = {
  invoiceDocumentId: number;
  invoiceNumber: string;
  hasProposal: boolean;
  postedInterest: number;
  lawfulInterest: number;
  alreadyCorrected: number;
  overClaimedAmount: number;
  throughDate: string | null;
  claimOverclaims: Array<{
    claimId: number;
    claimDate: string;
    journalEntryId: number;
    overClaimedAmount: number;
  }>;
  canPost: boolean;
  reason: string | null;
  requiresRefund: boolean;
  proposedLines: Array<{
    accountNo: string;
    debitAmount: number;
    creditAmount: number;
  }>;
};
export type NativeInvoiceBadDebtInput = {
  invoiceDocumentId: number;
  writeOffDate: string;
  grossAmount?: number;
  expenseAccountNo: string;
  note?: string;
  confirm: true;
};
export type NativeInvoiceBadDebtWriteOff = {
  writeOffId: number;
  writeOffDate: string;
  grossAmount: number;
  netAmount: number;
  vatAmount: number;
  grossAmountDkk: number;
  netAmountDkk: number;
  vatAmountDkk: number;
  journalEntryId: number;
  journalEntryNo: string;
};
export type NativeInvoiceBadDebtReceipt = {
  writeOffId: number;
  journalEntryId: number;
  journalEntryNo: string;
  invoiceDocumentId: number;
  invoiceNumber: string;
  principalCurrency: string;
  claimCurrency: "DKK";
  grossAmount: number;
  netAmount: number;
  vatAmount: number;
  grossAmountDkk: number;
  netAmountDkk: number;
  vatAmountDkk: number;
  remainingPrincipal: number;
  remainingPrincipalDkk: number;
  remainingClaimDkk: number;
};