import type { Database } from "bun:sqlite";
import { applyInvoicePayment, getInvoiceStatus } from "./invoice-payments";
import { postJournalEntry, type JournalPostResult } from "./ledger";
import { insertAuditLog } from "./actor";
import { roundDkk } from "./money";

const RULE_ID = "DK-INVOICE-SETTLEMENT-001";
const COMBINED_RULE_ID = "DK-INVOICE-COMBINED-SETTLEMENT-001";

export type SettleInvoiceFromBankInput = {
  invoiceDocumentId: number;
  bankTransactionId?: number;
  bankTransactionReference?: string;
  paymentDate?: string;
  amount?: number;
  bankAccountNo?: string;
  receivableAccountNo?: string;
  createdBy?: string;
  createdByProgram?: string;
};

export type SettleInvoiceFromBankResult = JournalPostResult & {
  paymentId?: number;
  claimPaymentId?: number;
  principalAmount?: number;
  claimAmount?: number;
  invoiceNumber?: string;
  openBalance?: number;
  claimOpenBalance?: number;
};


function getIncomingBankTransaction(db: Database, input: SettleInvoiceFromBankInput) {
  if (input.bankTransactionId === undefined && !input.bankTransactionReference) {
    return { error: "bankTransactionId or bankTransactionReference is required" };
  }
  const bank = (input.bankTransactionId !== undefined
    ? db
        .query(
          `SELECT id, transaction_date, amount, currency, amount_dkk, fx_rate_to_dkk, text, reference FROM bank_transactions WHERE id = ?`,
        )
        .get(input.bankTransactionId)
    : db
        .query(
          `SELECT id, transaction_date, amount, currency, amount_dkk, fx_rate_to_dkk, text, reference FROM bank_transactions WHERE reference = ? ORDER BY id DESC LIMIT 1`,
        )
        .get(input.bankTransactionReference)) as {
    id: number;
    transaction_date: string;
    amount: number;
    currency: string | null;
    amount_dkk: number | null;
    fx_rate_to_dkk: number | null;
    text: string;
    reference: string | null;
  } | null;
  if (!bank) {
    return { error: input.bankTransactionId !== undefined ? `bank transaction ${input.bankTransactionId} does not exist` : `no bank transaction found with reference ${input.bankTransactionReference}` };
  }
  return { bank };
}

function countUnpostedClaims(db: Database, invoiceDocumentId: number) {
  const reminders = db.query(
    `SELECT COUNT(*) AS n
     FROM invoice_reminders r
     LEFT JOIN invoice_reminder_postings p ON p.reminder_id = r.id
     WHERE r.invoice_document_id = ? AND p.id IS NULL`
  ).get(invoiceDocumentId) as { n: number };
  const interestClaims = db.query(
    `SELECT COUNT(*) AS n
     FROM invoice_interest_claims c
     LEFT JOIN invoice_interest_postings p ON p.interest_claim_id = c.id
     WHERE c.invoice_document_id = ? AND p.id IS NULL`
  ).get(invoiceDocumentId) as { n: number };
  const compensationClaims = db.query(
    `SELECT COUNT(*) AS n
     FROM invoice_compensation_claims c
     LEFT JOIN invoice_compensation_postings p ON p.compensation_claim_id = c.id
     WHERE c.invoice_document_id = ? AND p.id IS NULL`
  ).get(invoiceDocumentId) as { n: number };
  return (reminders.n ?? 0) + (interestClaims.n ?? 0) + (compensationClaims.n ?? 0);
}

export function settleInvoiceFromBank(db: Database, input: SettleInvoiceFromBankInput): SettleInvoiceFromBankResult {
  if (!Number.isInteger(input.invoiceDocumentId) || input.invoiceDocumentId <= 0) {
    return { ok: false, appliedRules: [RULE_ID], errors: ["invoiceDocumentId must be a positive integer"] };
  }
  if (input.bankTransactionId !== undefined && (!Number.isInteger(input.bankTransactionId) || input.bankTransactionId <= 0)) {
    return { ok: false, appliedRules: [RULE_ID], errors: ["bankTransactionId must be a positive integer when present"] };
  }

  const selected = getIncomingBankTransaction(db, input);
  if (selected.error) return { ok: false, appliedRules: [RULE_ID], errors: [selected.error] };
  const bank = selected.bank!;
  if (Number(bank.amount) <= 0) return { ok: false, appliedRules: [RULE_ID], errors: [`bank transaction ${input.bankTransactionId} is not an incoming customer receipt`] };

  const invoice = db.query(
    `SELECT id, invoice_no, currency FROM documents WHERE id = ? AND document_type = 'issued_invoice'`
  ).get(input.invoiceDocumentId) as { id: number; invoice_no: string; currency: string | null } | null;
  if (!invoice) return { ok: false, appliedRules: [RULE_ID], errors: [`invoice document ${input.invoiceDocumentId} is not an issued invoice`] };

  const existingJournal = db.query(
    `SELECT id FROM journal_entries WHERE source_bank_transaction_id = ? LIMIT 1`
  ).get(bank.id) as { id: number } | null;
  if (existingJournal) return { ok: false, appliedRules: [RULE_ID], errors: [`bank transaction ${bank.id} is already linked to journal entry ${existingJournal.id}`] };

  const invoiceCurrency = (invoice.currency ?? "DKK").trim().toUpperCase();
  const bankCurrency = (bank.currency ?? "DKK").trim().toUpperCase();
  if (invoiceCurrency !== bankCurrency) {
    return { ok: false, appliedRules: [RULE_ID], errors: [`bank transaction ${bank.id} currency ${bankCurrency} does not match invoice currency ${invoiceCurrency}`] };
  }
  if (invoiceCurrency !== "DKK" && (!(Number(bank.fx_rate_to_dkk) > 0) || !(Number(bank.amount_dkk) > 0))) {
    return { ok: false, appliedRules: [RULE_ID], errors: [`bank transaction ${bank.id} is missing deterministic DKK conversion metadata`] };
  }

  const amount = roundDkk(input.amount ?? Number(bank.amount));
  const paymentDate = input.paymentDate ?? bank.transaction_date;
  const before = getInvoiceStatus(db, input.invoiceDocumentId);
  if (!before.ok) return { ok: false, appliedRules: [RULE_ID], errors: before.errors };
  const principalOpenBalance = roundDkk(Number(before.openBalance ?? 0));
  const claimOpenBalance = roundDkk(Number(before.claimOpenBalance ?? 0));

  try {
    const result = db.transaction(() => {
      const isCombined = amount > principalOpenBalance && principalOpenBalance > 0;
      if (amount > claimOpenBalance) {
        throw new Error(JSON.stringify({ appliedRules: [isCombined ? COMBINED_RULE_ID : RULE_ID], errors: [`settlement amount ${amount} exceeds invoice claim open balance ${claimOpenBalance}`] }));
      }
      // Combined principal+claim settlement assumes a single currency: it credits
      // 1100 by the whole receipt at the payment-date rate and mixes a foreign
      // principal with DKK-denominated claims (interest/compensation), which both
      // strands the realised exchange difference and confuses units (adversarial
      // #1/#5/#9). Require foreign principal and DKK claims to be settled in
      // separate receipts, each well-defined.
      if (isCombined && invoiceCurrency !== "DKK") {
        throw new Error(JSON.stringify({ appliedRules: [COMBINED_RULE_ID], errors: ["kombineret afregning af principal og krav i ét beløb understøttes ikke for fakturaer i fremmed valuta — afregn principal og krav hver for sig (combined principal + claim settlement is not supported for foreign-currency invoices)"] }));
      }
      if (isCombined && countUnpostedClaims(db, input.invoiceDocumentId) > 0) {
        throw new Error(JSON.stringify({ appliedRules: [COMBINED_RULE_ID], errors: ["combined settlement requires all included claims to be ledger-posted first"] }));
      }

      let paymentId: number | undefined;
      let claimPaymentId: number | undefined;
      let principalAmount = amount;
      let claimAmount = 0;
      const appliedRules = new Set<string>([RULE_ID]);

      if (isCombined) {
        principalAmount = principalOpenBalance;
        claimAmount = roundDkk(amount - principalAmount);
        if (claimAmount <= 0) {
          throw new Error(JSON.stringify({ appliedRules: [COMBINED_RULE_ID], errors: ["combined settlement produced no claim component"] }));
        }
        appliedRules.add(COMBINED_RULE_ID);
      }

      let journalEntryId: number | undefined;

      if (claimAmount > 0) {
        const journalAmountDkk = invoiceCurrency === "DKK" ? amount : roundDkk(Number(bank.amount_dkk ?? 0));
        const journal = postJournalEntry(db, {
          transactionDate: paymentDate,
          text: `Customer payment incl. claims for invoice ${invoice.invoice_no}`,
          sourceBankTransactionId: bank.id,
          documentId: input.invoiceDocumentId,
          currency: invoiceCurrency === "DKK" ? undefined : invoiceCurrency,
          amountForeign: invoiceCurrency === "DKK" ? undefined : amount,
          amountDkk: invoiceCurrency === "DKK" ? undefined : journalAmountDkk,
          fxRateToDkk: invoiceCurrency === "DKK" ? undefined : Number(bank.fx_rate_to_dkk ?? undefined),
          createdBy: input.createdBy,
          createdByProgram: input.createdByProgram,
          lines: [
            { accountNo: input.bankAccountNo ?? "2000", debitAmount: journalAmountDkk, text: `Bank receipt ${invoice.invoice_no}` },
            { accountNo: input.receivableAccountNo ?? "1100", creditAmount: journalAmountDkk, text: `Principal and claim settlement ${invoice.invoice_no}` },
          ],
        });
        if (!journal.ok || journal.entryId == null) throw new Error(JSON.stringify({ appliedRules: journal.appliedRules, errors: journal.errors }));
        journalEntryId = journal.entryId;
        for (const rule of journal.appliedRules ?? []) appliedRules.add(rule);
      }

      const payment = applyInvoicePayment(db, {
        invoiceDocumentId: input.invoiceDocumentId,
        bankTransactionId: bank.id,
        journalEntryId,
        paymentDate,
        amount: principalAmount,
        bankAccountNo: input.bankAccountNo,
        receivableAccountNo: input.receivableAccountNo,
        createdBy: input.createdBy,
        createdByProgram: input.createdByProgram,
        note: `Bank settlement from transaction ${bank.id}`,
      });
      if (!payment.ok) throw new Error(JSON.stringify({ appliedRules: payment.appliedRules, errors: payment.errors }));
      paymentId = payment.paymentId;
      journalEntryId = payment.journalEntryId;
      for (const rule of payment.appliedRules ?? []) appliedRules.add(rule);

      if (claimAmount > 0) {
        const claimPayment = db.query(
          `INSERT INTO invoice_claim_payments (invoice_document_id, bank_transaction_id, payment_date, amount, currency, note)
           VALUES (?, ?, ?, ?, ?, ?)
           RETURNING id`
        ).get(input.invoiceDocumentId, bank.id, paymentDate, claimAmount, invoiceCurrency, `Combined settlement claim component from transaction ${bank.id}`) as { id: number };
        claimPaymentId = claimPayment.id;
        insertAuditLog(db, {
          eventType: "invoice_claim_payment_apply",
          entityType: "invoice_claim_payment",
          entityId: claimPayment.id,
          message: `Applied claim receipt ${claimAmount} to invoice ${invoice.invoice_no} via combined settlement`,
          createdBy: input.createdBy,
          createdByProgram: input.createdByProgram,
        });
      }

      const after = getInvoiceStatus(db, input.invoiceDocumentId);
      if (!after.ok) throw new Error(JSON.stringify({ errors: after.errors }));

      return {
        ok: true,
        entryId: journalEntryId,
        paymentId,
        claimPaymentId,
        principalAmount,
        claimAmount,
        invoiceNumber: invoice.invoice_no,
        openBalance: after.openBalance,
        claimOpenBalance: after.claimOpenBalance,
        appliedRules: [...appliedRules],
        errors: [],
      };
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
