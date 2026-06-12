import type { Database } from "bun:sqlite";
import { postJournalEntry } from "./ledger";
import { insertAuditLog } from "./actor";
import { isValidIsoDate as looksLikeIsoDate, addDays, diffDays } from "./dates";
import { addDkk, roundDkk, subtractDkk, sumDkk } from "./money";

export type ApplyInvoicePaymentInput = {
  invoiceDocumentId: number;
  paymentDate: string;
  amount: number;
  bankTransactionId?: number;
  journalEntryId?: number;
  bankAccountNo?: string;
  receivableAccountNo?: string;
  createdBy?: string;
  createdByProgram?: string;
  note?: string;
};

export type ApplyInvoicePaymentResult = {
  ok: boolean;
  paymentId?: number;
  journalEntryId?: number;
  invoiceDocumentId?: number;
  invoiceNumber?: string;
  openBalance?: number;
  appliedRules: string[];
  errors: string[];
};

export type InvoiceStatusResult = {
  ok: boolean;
  invoiceDocumentId?: number;
  invoiceNumber?: string;
  grossAmount?: number;
  creditedAmount?: number;
  paidAmount?: number;
  openBalance?: number;
  claimOpenBalance?: number;
  asOfDate?: string;
  dueDate?: string;
  effectiveDueDate?: string;
  isOverdue?: boolean;
  overdueDays?: number;
  status?: "open" | "paid" | "credited" | "refunded" | "overpaid" | "written_off";
  payments?: Array<{
    paymentId: number;
    paymentDate: string;
    amount: number;
    bankTransactionId: number | null;
    journalEntryId: number | null;
    note: string | null;
  }>;
  creditNotes?: Array<{
    documentId: number;
    creditNoteNumber: string;
    amount: number;
    issueDate: string | null;
  }>;
  refunds?: Array<{
    refundId: number;
    refundDate: string;
    amount: number;
    bankTransactionId: number | null;
    note: string | null;
  }>;
  claimPayments?: Array<{
    claimPaymentId: number;
    paymentDate: string;
    amount: number;
    bankTransactionId: number | null;
    note: string | null;
  }>;
  badDebtWriteOffs?: Array<{
    writeOffId: number;
    writeOffDate: string;
    grossAmount: number;
    netAmount: number;
    vatAmount: number;
    journalEntryId: number;
    note: string | null;
  }>;
  reminders?: Array<{
    reminderId: number;
    reminderDate: string;
    feeAmount: number;
    note: string | null;
    journalEntryId: number | null;
  }>;
  compensationClaims?: Array<{
    claimId: number;
    claimDate: string;
    amountDkk: number;
    note: string | null;
    journalEntryId: number | null;
  }>;
  interestClaims?: Array<{
    claimId: number;
    claimDate: string;
    amountDkk: number;
    referenceRatePercent: number;
    annualInterestRatePercent: number;
    overdueDays: number;
    note: string | null;
    journalEntryId: number | null;
  }>;
  interestCorrections?: Array<{
    correctionId: number;
    correctionDate: string;
    amountDkk: number;
    journalEntryId: number;
  }>;
  totalReminderFees?: number;
  totalCompensationClaims?: number;
  totalInterestClaims?: number;
  totalInterestCorrections?: number;
  totalClaimPayments?: number;
  totalBadDebtWrittenOff?: number;
  errors: string[];
};

const RULE_ID = "DK-INVOICE-PAYMENT-001";
const CORRECTION_BALANCE_RULE_ID = "DK-INVOICE-CORRECTION-BALANCE-001";
const DUE_DATE_RULE_ID = "DK-INVOICE-DUE-DATE-001";
const FX_REALISED_RULE_ID = "DK-INVOICE-FX-REALISED-001";

// Realised exchange gain/loss accounts (seedAccounts): the receivable is
// relieved at the invoice-date rate, and the difference vs the DKK actually
// received lands here.
const FX_GAIN_ACCOUNT_NO = "1020"; // Valutakursgevinst (income, credit)
const FX_LOSS_ACCOUNT_NO = "3320"; // Valutakurstab (expense, debit)

type JournalLineInput = { accountNo: string; debitAmount?: number; creditAmount?: number; text?: string };

// For a foreign-currency invoice payment, relieve the receivable (1100) at the
// rate the invoice was BOOKED at — never the payment-date rate. The relief for
// this payment is the share of the booked receivable DKK (grossAmountDkk,
// rounded ONCE at booking) that corresponds to the foreign balance this payment
// settles, computed as the DIFFERENCE of two cumulative proportional figures
// (relief on the open balance before vs after the payment). Computing it that
// way makes the per-payment reliefs TELESCOPE: across any split of the foreign
// amount they sum to exactly grossAmountDkk, so 1100 nets to zero with no
// per-partial rounding residue (adversarial re-review #1/#2). The foreign open
// balance already nets credit notes (which relieve 1100 at the invoice rate via
// creditNoteLinesFromOriginalJournal), so their relief is accounted for
// automatically. The difference between the DKK actually received
// (paymentAmountDkk, at the payment-date rate) and the relieved receivable is
// the realised exchange gain/loss line, which makes the entry balance and routes
// the rate drift to the result. (An earlier snap-to-remainder design anchored on
// the gross receivable and reconstructed prior relief from payments only — it
// double-counted credit-note relief and drove 1100 negative; adversarial
// findings #4/#6/#7/#8. Foreign-currency REFUNDS are refused upstream because
// that path is not yet currency-aware — see invoice-refunds.ts.)
function buildForeignPaymentLines(args: {
  payloadJson: string | null;
  invoiceNo: string;
  amountForeign: number;
  grossForeign: number;
  openForeignBefore: number;
  paymentAmountDkk: number;
  bankAccountNo?: string;
  receivableAccountNo?: string;
}): { lines: JournalLineInput[]; fxApplied: boolean } {
  const payload = args.payloadJson ? JSON.parse(args.payloadJson) : null;
  const invoiceRate = Number(payload?.totals?.fxRateToDkk ?? 0);
  const grossAmountDkk = roundDkk(Number(payload?.totals?.grossAmountDkk ?? 0));

  let receivableReliefDkk: number;
  if (args.grossForeign > 0 && grossAmountDkk > 0) {
    const openAfter = subtractDkk(args.openForeignBefore, args.amountForeign);
    const cumulativeReliefBefore = roundDkk((grossAmountDkk * args.openForeignBefore) / args.grossForeign);
    const cumulativeReliefAfter = roundDkk((grossAmountDkk * openAfter) / args.grossForeign);
    receivableReliefDkk = subtractDkk(cumulativeReliefBefore, cumulativeReliefAfter);
  } else {
    receivableReliefDkk = roundDkk(args.amountForeign * invoiceRate);
  }

  const lines: JournalLineInput[] = [
    { accountNo: args.bankAccountNo ?? "2000", debitAmount: args.paymentAmountDkk, text: `Payment receipt ${args.invoiceNo}` },
    { accountNo: args.receivableAccountNo ?? "1100", creditAmount: receivableReliefDkk, text: `Receivable settlement ${args.invoiceNo}` },
  ];
  const fxDelta = roundDkk(subtractDkk(args.paymentAmountDkk, receivableReliefDkk));
  if (fxDelta > 0) {
    lines.push({ accountNo: FX_GAIN_ACCOUNT_NO, creditAmount: fxDelta, text: `Realiseret valutakursgevinst ${args.invoiceNo}` });
  } else if (fxDelta < 0) {
    lines.push({ accountNo: FX_LOSS_ACCOUNT_NO, debitAmount: roundDkk(-fxDelta), text: `Realiseret valutakurstab ${args.invoiceNo}` });
  }
  return { lines, fxApplied: fxDelta !== 0 };
}

function defaultComparisonDate(invoiceDate?: string, effectiveDueDate?: string) {
  return effectiveDueDate ?? invoiceDate ?? "1970-01-01";
}

function getIssuedInvoice(db: Database, documentId: number) {
  return db.query(
    `SELECT id, invoice_no, amount_inc_vat, currency, document_type, status, invoice_date, payload_json
     FROM documents WHERE id = ?`
  ).get(documentId) as { id: number; invoice_no: string; amount_inc_vat: number; currency: string; document_type: string; status: string; invoice_date: string | null; payload_json: string | null } | null;
}

export function getInvoiceStatus(db: Database, invoiceDocumentId: number, asOfDate?: string): InvoiceStatusResult {
  const invoice = getIssuedInvoice(db, invoiceDocumentId);
  if (!invoice) return { ok: false, errors: [`invoice document ${invoiceDocumentId} does not exist`] };
  if (invoice.document_type !== "issued_invoice") return { ok: false, errors: [`document ${invoiceDocumentId} is not an issued invoice`] };

  const payload = invoice.payload_json ? JSON.parse(invoice.payload_json) : null;

  const payments = db.query(
    `SELECT p.id, p.payment_date, p.amount, p.bank_transaction_id, p.journal_entry_id, p.note
     FROM invoice_payments p
     JOIN journal_entries j ON j.id = p.journal_entry_id
     WHERE p.invoice_document_id = ?
     ORDER BY p.id ASC`
  ).all(invoiceDocumentId) as Array<{ id: number; payment_date: string; amount: number; bank_transaction_id: number | null; journal_entry_id: number | null; note: string | null }>;

  const creditNotes = db.query(
    `SELECT id, invoice_no, amount_inc_vat, invoice_date
     FROM documents
     WHERE document_type = 'credit_note' AND payment_details = ?
     ORDER BY id ASC`
  ).all(invoice.invoice_no) as Array<{ id: number; invoice_no: string; amount_inc_vat: number | null; invoice_date: string | null }>;

  const refunds = db.query(
    `SELECT id, refund_date, amount, bank_transaction_id, note
     FROM invoice_refunds WHERE invoice_document_id = ? ORDER BY id ASC`
  ).all(invoiceDocumentId) as Array<{ id: number; refund_date: string; amount: number; bank_transaction_id: number | null; note: string | null }>;

  const claimPayments = db.query(
    `SELECT id, payment_date, amount, bank_transaction_id, note
     FROM invoice_claim_payments WHERE invoice_document_id = ? ORDER BY id ASC`
  ).all(invoiceDocumentId) as Array<{ id: number; payment_date: string; amount: number; bank_transaction_id: number | null; note: string | null }>;

  const badDebtWriteOffs = db.query(
    `SELECT id, writeoff_date, gross_amount, net_amount, vat_amount, journal_entry_id, note
     FROM invoice_bad_debt_writeoffs WHERE invoice_document_id = ? ORDER BY id ASC`
  ).all(invoiceDocumentId) as Array<{ id: number; writeoff_date: string; gross_amount: number; net_amount: number; vat_amount: number; journal_entry_id: number; note: string | null }>;

  const reminders = db.query(
    `SELECT r.id, r.reminder_date, r.fee_amount, r.note, p.journal_entry_id
     FROM invoice_reminders r
     LEFT JOIN invoice_reminder_postings p ON p.reminder_id = r.id
     WHERE r.invoice_document_id = ? ORDER BY r.reminder_date ASC, r.id ASC`
  ).all(invoiceDocumentId) as Array<{ id: number; reminder_date: string; fee_amount: number; note: string | null; journal_entry_id: number | null }>;

  const compensationClaims = db.query(
    `SELECT c.id, c.claim_date, c.amount_dkk, c.note, p.journal_entry_id
     FROM invoice_compensation_claims c
     LEFT JOIN invoice_compensation_postings p ON p.compensation_claim_id = c.id
     WHERE c.invoice_document_id = ? ORDER BY c.claim_date ASC, c.id ASC`
  ).all(invoiceDocumentId) as Array<{ id: number; claim_date: string; amount_dkk: number; note: string | null; journal_entry_id: number | null }>;

  const interestClaims = db.query(
    `SELECT c.id, c.claim_date, c.amount_dkk, c.reference_rate_percent, c.annual_interest_rate_percent, c.overdue_days, c.note, p.journal_entry_id
     FROM invoice_interest_claims c
     LEFT JOIN invoice_interest_postings p ON p.interest_claim_id = c.id
     WHERE c.invoice_document_id = ? ORDER BY c.claim_date ASC, c.id ASC`
  ).all(invoiceDocumentId) as Array<{ id: number; claim_date: string; amount_dkk: number; reference_rate_percent: number; annual_interest_rate_percent: number; overdue_days: number; note: string | null; journal_entry_id: number | null }>;

  // Correcting reversals of over-claimed late interest (back-dated balance
  // reductions); they net DOWN the interest-claim balance.
  const interestCorrections = db.query(
    `SELECT id, correction_date, amount_dkk, journal_entry_id
     FROM invoice_interest_corrections
     WHERE invoice_document_id = ? ORDER BY id ASC`
  ).all(invoiceDocumentId) as Array<{ id: number; correction_date: string; amount_dkk: number; journal_entry_id: number }>;

  const grossAmount = roundDkk(Number(invoice.amount_inc_vat ?? 0));
  const creditedAmount = sumDkk(creditNotes.map((c) => Number(c.amount_inc_vat ?? 0)));
  const paidAmount = sumDkk(payments.map((p) => Number(p.amount)));
  const refundedAmount = sumDkk(refunds.map((r) => Number(r.amount)));
  const totalReminderFees = sumDkk(reminders.map((r) => Number(r.fee_amount)));
  const totalCompensationClaims = sumDkk(compensationClaims.map((c) => Number(c.amount_dkk)));
  const totalInterestCorrections = sumDkk(interestCorrections.map((c) => Number(c.amount_dkk)));
  const totalInterestClaims = subtractDkk(sumDkk(interestClaims.map((c) => Number(c.amount_dkk))), totalInterestCorrections);
  const totalClaimPayments = sumDkk(claimPayments.map((p) => Number(p.amount)));
  const totalBadDebtWrittenOff = sumDkk(badDebtWriteOffs.map((w) => Number(w.gross_amount)));
  const openBalance = subtractDkk(addDkk(subtractDkk(grossAmount, creditedAmount, paidAmount), refundedAmount), totalBadDebtWrittenOff);
  const claimOpenBalance = subtractDkk(addDkk(openBalance, totalReminderFees, totalCompensationClaims, totalInterestClaims), totalClaimPayments);
  const dueDate = typeof payload?.dueDate === "string" ? payload.dueDate : undefined;
  const effectiveDueDate = dueDate ?? (invoice.invoice_date ? addDays(invoice.invoice_date, 30) : undefined);
  const comparisonDate = asOfDate ?? defaultComparisonDate(invoice.invoice_date ?? undefined, effectiveDueDate);
  const overdueDays = effectiveDueDate && openBalance > 0 ? Math.max(0, diffDays(effectiveDueDate, comparisonDate)) : 0;
  const isOverdue = overdueDays > 0;
  // Status ladder: a written-off or refunded zero-balance invoice must never be
  // mislabelled "paid". Write-off and refund take precedence over "paid", and
  // "refunded" is decoupled from creditedAmount so a refund without a credit
  // note (e.g. an overpayment returned) is still labelled "refunded".
  const status = openBalance > 0
    ? "open"
    : openBalance < 0
      ? "overpaid"
      : totalBadDebtWrittenOff > 0
        ? "written_off"
        : refundedAmount > 0
          ? "refunded"
          : creditedAmount === grossAmount && paidAmount === 0
            ? "credited"
            : "paid";

  return {
    ok: true,
    invoiceDocumentId,
    invoiceNumber: invoice.invoice_no,
    grossAmount,
    creditedAmount,
    paidAmount,
    openBalance,
    claimOpenBalance,
    asOfDate: comparisonDate,
    dueDate,
    effectiveDueDate,
    isOverdue,
    overdueDays,
    status,
    payments: payments.map((p) => ({ paymentId: p.id, paymentDate: p.payment_date, amount: roundDkk(Number(p.amount)), bankTransactionId: p.bank_transaction_id, journalEntryId: p.journal_entry_id == null ? null : Number(p.journal_entry_id), note: p.note })),
    creditNotes: creditNotes.map((c) => ({ documentId: c.id, creditNoteNumber: c.invoice_no, amount: roundDkk(Number(c.amount_inc_vat ?? 0)), issueDate: c.invoice_date })),
    refunds: refunds.map((r) => ({ refundId: r.id, refundDate: r.refund_date, amount: roundDkk(Number(r.amount)), bankTransactionId: r.bank_transaction_id, note: r.note })),
    claimPayments: claimPayments.map((p) => ({ claimPaymentId: p.id, paymentDate: p.payment_date, amount: roundDkk(Number(p.amount)), bankTransactionId: p.bank_transaction_id, note: p.note })),
    badDebtWriteOffs: badDebtWriteOffs.map((w) => ({ writeOffId: w.id, writeOffDate: w.writeoff_date, grossAmount: roundDkk(Number(w.gross_amount)), netAmount: roundDkk(Number(w.net_amount)), vatAmount: roundDkk(Number(w.vat_amount)), journalEntryId: Number(w.journal_entry_id), note: w.note })),
    reminders: reminders.map((r) => ({ reminderId: r.id, reminderDate: r.reminder_date, feeAmount: roundDkk(Number(r.fee_amount)), note: r.note, journalEntryId: r.journal_entry_id == null ? null : Number(r.journal_entry_id) })),
    compensationClaims: compensationClaims.map((c) => ({ claimId: c.id, claimDate: c.claim_date, amountDkk: roundDkk(Number(c.amount_dkk)), note: c.note, journalEntryId: c.journal_entry_id == null ? null : Number(c.journal_entry_id) })),
    interestClaims: interestClaims.map((c) => ({
      claimId: c.id,
      claimDate: c.claim_date,
      amountDkk: roundDkk(Number(c.amount_dkk)),
      referenceRatePercent: roundDkk(Number(c.reference_rate_percent)),
      annualInterestRatePercent: roundDkk(Number(c.annual_interest_rate_percent)),
      overdueDays: Number(c.overdue_days),
      note: c.note,
      journalEntryId: c.journal_entry_id == null ? null : Number(c.journal_entry_id),
    })),
    interestCorrections: interestCorrections.map((c) => ({ correctionId: c.id, correctionDate: c.correction_date, amountDkk: roundDkk(Number(c.amount_dkk)), journalEntryId: Number(c.journal_entry_id) })),
    totalReminderFees,
    totalCompensationClaims,
    totalInterestClaims,
    totalInterestCorrections,
    totalClaimPayments,
    totalBadDebtWrittenOff,
    errors: [],
  };
}

export function applyInvoicePayment(db: Database, input: ApplyInvoicePaymentInput): ApplyInvoicePaymentResult {
  const errors: string[] = [];
  if (!Number.isInteger(input.invoiceDocumentId) || input.invoiceDocumentId <= 0) errors.push("invoiceDocumentId must be a positive integer");
  if (!looksLikeIsoDate(input.paymentDate)) errors.push("paymentDate must be YYYY-MM-DD");
  if (!Number.isFinite(input.amount) || input.amount <= 0) errors.push("amount must be a positive number");
  if (input.bankTransactionId !== undefined && (!Number.isInteger(input.bankTransactionId) || input.bankTransactionId <= 0)) errors.push("bankTransactionId must be a positive integer when present");
  if (input.journalEntryId !== undefined && (!Number.isInteger(input.journalEntryId) || input.journalEntryId <= 0)) errors.push("journalEntryId must be a positive integer when present");
  if (errors.length > 0) return { ok: false, appliedRules: [RULE_ID], errors };

  const invoice = getIssuedInvoice(db, input.invoiceDocumentId);
  if (!invoice) return { ok: false, appliedRules: [RULE_ID], errors: [`invoice document ${input.invoiceDocumentId} does not exist`] };
  if (invoice.document_type !== "issued_invoice") return { ok: false, appliedRules: [RULE_ID], errors: [`document ${input.invoiceDocumentId} is not an issued invoice`] };

  const invoiceCurrency = (invoice.currency ?? "DKK").trim().toUpperCase();
  const bank = input.bankTransactionId !== undefined
    ? db.query("SELECT id, amount, currency, amount_dkk, fx_rate_to_dkk FROM bank_transactions WHERE id = ?").get(input.bankTransactionId) as { id: number; amount: number; currency: string | null; amount_dkk: number | null; fx_rate_to_dkk: number | null } | null
    : null;

  if (input.bankTransactionId !== undefined) {
    if (!bank) return { ok: false, appliedRules: [RULE_ID], errors: [`bank transaction ${input.bankTransactionId} does not exist`] };
    const bankCurrency = (bank.currency ?? "DKK").trim().toUpperCase();
    if (bankCurrency !== invoiceCurrency) {
      return { ok: false, appliedRules: [RULE_ID], errors: [`bank transaction ${input.bankTransactionId} currency ${bankCurrency} does not match invoice currency ${invoiceCurrency}`] };
    }
    if (invoiceCurrency !== "DKK" && (!(Number(bank.fx_rate_to_dkk) > 0) || !(Number(bank.amount_dkk) > 0))) {
      return { ok: false, appliedRules: [RULE_ID], errors: [`bank transaction ${input.bankTransactionId} is missing deterministic DKK conversion metadata`] };
    }
    const alreadyLinked = db.query("SELECT id FROM invoice_payments WHERE bank_transaction_id = ? LIMIT 1").get(input.bankTransactionId) as { id: number } | null;
    if (alreadyLinked) return { ok: false, appliedRules: [RULE_ID], errors: [`bank transaction ${input.bankTransactionId} is already applied to an invoice payment`] };
  }

  // A foreign-currency payment must post its OWN journal so the realised
  // exchange gain/loss is booked here (buildForeignPaymentLines). A caller-
  // provided journalEntryId would bypass that and could strand the rate drift on
  // the receivable (adversarial #2), so it is refused for non-DKK invoices.
  if (invoiceCurrency !== "DKK" && input.journalEntryId !== undefined) {
    return { ok: false, appliedRules: [RULE_ID, FX_REALISED_RULE_ID], errors: ["betaling i fremmed valuta kan ikke genbruge en forud-leveret journalEntryId — den realiserede valutakursdifference skal bogføres her (foreign-currency payment cannot reuse a caller-provided journalEntryId)"] };
  }
  if (invoiceCurrency !== "DKK" && !bank) {
    return { ok: false, appliedRules: [RULE_ID], errors: ["non-DKK invoice payments require a bankTransactionId"] };
  }

  const status = getInvoiceStatus(db, input.invoiceDocumentId);
  if (!status.ok) return { ok: false, appliedRules: [RULE_ID], errors: status.errors };
  const openBalance = roundDkk(status.openBalance!);
  const amount = roundDkk(input.amount);
  if (amount > openBalance) {
    return { ok: false, appliedRules: [RULE_ID, CORRECTION_BALANCE_RULE_ID], errors: [`payment amount ${amount} exceeds open invoice balance ${openBalance}`] };
  }

  let fxRealisedApplied = false;
  try {
    const result = db.transaction(() => {
      let journalEntryId = input.journalEntryId;

      if (journalEntryId === undefined) {
        const paymentAmountDkk = invoiceCurrency === "DKK"
          ? amount
          : roundDkk(amount * Number(bank?.fx_rate_to_dkk ?? 0));
        let lines: JournalLineInput[];
        if (invoiceCurrency === "DKK") {
          lines = [
            { accountNo: input.bankAccountNo ?? "2000", debitAmount: paymentAmountDkk, text: `Payment receipt ${invoice.invoice_no}` },
            { accountNo: input.receivableAccountNo ?? "1100", creditAmount: paymentAmountDkk, text: `Receivable settlement ${invoice.invoice_no}` },
          ];
        } else {
          const built = buildForeignPaymentLines({
            payloadJson: invoice.payload_json,
            invoiceNo: invoice.invoice_no,
            amountForeign: amount,
            grossForeign: roundDkk(Number(invoice.amount_inc_vat ?? 0)),
            openForeignBefore: openBalance,
            paymentAmountDkk,
            bankAccountNo: input.bankAccountNo,
            receivableAccountNo: input.receivableAccountNo,
          });
          lines = built.lines;
          fxRealisedApplied = built.fxApplied;
        }
        const journal = postJournalEntry(db, {
          transactionDate: input.paymentDate,
          text: input.bankTransactionId !== undefined ? `Customer payment for invoice ${invoice.invoice_no}` : `Manual invoice payment for invoice ${invoice.invoice_no}`,
          sourceBankTransactionId: input.bankTransactionId,
          documentId: input.invoiceDocumentId,
          currency: invoiceCurrency === "DKK" ? undefined : invoiceCurrency,
          amountForeign: invoiceCurrency === "DKK" ? undefined : amount,
          amountDkk: invoiceCurrency === "DKK" ? undefined : paymentAmountDkk,
          fxRateToDkk: invoiceCurrency === "DKK" ? undefined : Number(bank?.fx_rate_to_dkk ?? undefined),
          createdBy: input.createdBy,
          createdByProgram: input.createdByProgram,
          lines,
        });
        if (!journal.ok || journal.entryId == null) throw new Error(JSON.stringify({ appliedRules: journal.appliedRules, errors: journal.errors }));
        journalEntryId = journal.entryId;
      } else {
        const journal = db.query(
          `SELECT id, document_id, source_bank_transaction_id FROM journal_entries WHERE id = ?`
        ).get(journalEntryId) as { id: number; document_id: number | null; source_bank_transaction_id: number | null } | null;
        if (!journal) {
          throw new Error(JSON.stringify({ appliedRules: [RULE_ID], errors: [`journal entry ${journalEntryId} does not exist`] }));
        }
        if (journal.document_id !== input.invoiceDocumentId) {
          throw new Error(JSON.stringify({ appliedRules: [RULE_ID], errors: [`journal entry ${journalEntryId} is not linked to invoice document ${input.invoiceDocumentId}`] }));
        }
        if ((input.bankTransactionId ?? null) !== (journal.source_bank_transaction_id ?? null)) {
          throw new Error(JSON.stringify({ appliedRules: [RULE_ID], errors: [`journal entry ${journalEntryId} bank link does not match invoice payment bank transaction`] }));
        }
      }

      const paymentId = db.query(
        `INSERT INTO invoice_payments (invoice_document_id, bank_transaction_id, journal_entry_id, payment_date, amount, currency, note)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         RETURNING id`
      ).get(input.invoiceDocumentId, input.bankTransactionId ?? null, journalEntryId, input.paymentDate, amount, invoiceCurrency, input.note ?? null) as { id: number };

      insertAuditLog(db, {
        eventType: "invoice_payment_apply",
        entityType: "invoice_payment",
        entityId: paymentId.id,
        message: `Applied payment ${amount} to invoice ${invoice.invoice_no}`,
        createdBy: input.createdBy,
        createdByProgram: input.createdByProgram,
      });

      const after = getInvoiceStatus(db, input.invoiceDocumentId);
      if (!after.ok) throw new Error(JSON.stringify({ appliedRules: [RULE_ID], errors: after.errors }));

      return {
        ok: true,
        paymentId: paymentId.id,
        journalEntryId,
        invoiceDocumentId: input.invoiceDocumentId,
        invoiceNumber: invoice.invoice_no,
        openBalance: after.openBalance,
        appliedRules: fxRealisedApplied
          ? [RULE_ID, CORRECTION_BALANCE_RULE_ID, FX_REALISED_RULE_ID]
          : [RULE_ID, CORRECTION_BALANCE_RULE_ID],
        errors: [],
      } satisfies ApplyInvoicePaymentResult;
    })();
    return result;
  } catch (error) {
    const parsed = typeof error === "object" && error && "message" in error ? (() => {
      try { return JSON.parse(String((error as any).message)); } catch { return null; }
    })() : null;
    return {
      ok: false,
      appliedRules: [...new Set([RULE_ID, ...((parsed?.appliedRules as string[] | undefined) ?? [])])],
      errors: (parsed?.errors as string[] | undefined) ?? [String(error)],
    };
  }
}
