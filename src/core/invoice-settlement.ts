import { TransactionRejectionError, decodeTransactionRejection } from "./transaction-rejection";
import type { Database } from "bun:sqlite";
import { applyInvoicePayment, getInvoiceStatus } from "./invoice-payments";
import { postJournalEntry, type JournalPostResult } from "./ledger";
import { insertAuditLog } from "./actor";
import { compareDkk, roundDkk } from "./money";
import { validateInvoiceJournalEvidence } from "./invoice-journal-evidence";
import { asJournalEntryId, type JournalEntryId } from "./ids";
import { allocateClaimReceipt, calculateClaimReceivableBalances } from "./invoice-claim-receivable";
import {
  calculateInvoiceReceivableCarryingBalance,
  resolveInvoiceReceivableAccount,
  resolveSettlementBankAccount,
} from "./invoice-fx-receivable";
import { isValidIsoDate as looksLikeIsoDate } from "./dates";

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
        .get(input.bankTransactionReference ?? "")) as {
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

export function settleInvoiceFromBank(db: Database, input: SettleInvoiceFromBankInput): SettleInvoiceFromBankResult {
  if (!Number.isInteger(input.invoiceDocumentId) || input.invoiceDocumentId <= 0) {
    return { ok: false, appliedRules: [RULE_ID], errors: ["invoiceDocumentId must be a positive integer"] };
  }
  if (input.bankTransactionId !== undefined && (!Number.isInteger(input.bankTransactionId) || input.bankTransactionId <= 0)) {
    return { ok: false, appliedRules: [RULE_ID], errors: ["bankTransactionId must be a positive integer when present"] };
  }
  if (input.paymentDate !== undefined && !looksLikeIsoDate(input.paymentDate)) {
    return { ok: false, appliedRules: [RULE_ID], errors: ["paymentDate must be YYYY-MM-DD when present"] };
  }

  const selected = getIncomingBankTransaction(db, input);
  if (selected.error) return { ok: false, appliedRules: [RULE_ID], errors: [selected.error] };
  const bank = selected.bank!;
  const bankAmount = Number(bank.amount);
  if (!Number.isFinite(bankAmount)) return { ok: false, appliedRules: [RULE_ID], errors: [`bank transaction ${bank.id} amount is not a finite number`] };
  if (bankAmount <= 0) return { ok: false, appliedRules: [RULE_ID], errors: [`bank transaction ${bank.id} is not an incoming customer receipt`] };

  const invoice = db.query(
    `SELECT id, invoice_no, currency FROM documents WHERE id = ? AND document_type = 'issued_invoice'`
  ).get(input.invoiceDocumentId) as { id: number; invoice_no: string; currency: string | null } | null;
  if (!invoice) return { ok: false, appliedRules: [RULE_ID], errors: [`invoice document ${input.invoiceDocumentId} is not an issued invoice`] };

  const existingJournal = db.query(
    `SELECT journal_entry_id AS id FROM bank_journal_reconciliations WHERE bank_transaction_id = ? LIMIT 1`
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

  const amount = roundDkk(input.amount ?? bankAmount);
  if (!(amount > 0)) return { ok: false, appliedRules: [RULE_ID], errors: ["settlement amount must be positive"] };
  if (compareDkk(amount, bankAmount) !== 0) {
    return { ok: false, appliedRules: [RULE_ID], errors: [`settlement amount ${amount} must equal bank transaction ${bank.id} amount ${bankAmount}`] };
  }
  const paymentDate = input.paymentDate ?? bank.transaction_date;
  const claimEvidenceDate = paymentDate < bank.transaction_date ? paymentDate : bank.transaction_date;
  const before = getInvoiceStatus(db, input.invoiceDocumentId);
  if (!before.ok) return { ok: false, appliedRules: [RULE_ID], errors: before.errors };
  const preflightPrincipalOpenBalance = roundDkk(Number(before.openBalance ?? 0));
  const preflightClaimOpenBalance = roundDkk(Number(before.claimOpenBalance ?? 0));
  if (amount > preflightClaimOpenBalance) {
    return { ok: false, appliedRules: [amount > preflightPrincipalOpenBalance && preflightPrincipalOpenBalance > 0 ? COMBINED_RULE_ID : RULE_ID], errors: [`settlement amount ${amount} exceeds invoice claim open balance ${preflightClaimOpenBalance}`] };
  }

  try {
    const result = db.transaction(() => {
      const lockedStatus = getInvoiceStatus(db, input.invoiceDocumentId);
      if (!lockedStatus.ok) throw new TransactionRejectionError({ appliedRules: [RULE_ID], errors: lockedStatus.errors });
      const principalOpenBalance = roundDkk(Number(lockedStatus.openBalance ?? 0));
      const claimOpenBalance = roundDkk(Number(lockedStatus.claimOpenBalance ?? 0));
      const isCombined = amount > principalOpenBalance && principalOpenBalance > 0;
      if (amount > claimOpenBalance) {
        throw new TransactionRejectionError({ appliedRules: [isCombined ? COMBINED_RULE_ID : RULE_ID], errors: [`settlement amount ${amount} exceeds invoice claim open balance ${claimOpenBalance}`] });
      }
      // Combined principal+claim settlement assumes a single currency: it credits
      // 1100 by the whole receipt at the payment-date rate and mixes a foreign
      // principal with DKK-denominated claims (interest/compensation), which both
      // strands the realised exchange difference and confuses units (adversarial
      // #1/#5/#9). Require foreign principal and DKK claims to be settled in
      // separate receipts, each well-defined.
      if (isCombined && invoiceCurrency !== "DKK") {
        throw new TransactionRejectionError({ appliedRules: [COMBINED_RULE_ID], errors: ["kombineret afregning af principal og krav i ét beløb understøttes ikke for fakturaer i fremmed valuta — afregn principal og krav hver for sig (combined principal + claim settlement is not supported for foreign-currency invoices)"] });
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
          throw new TransactionRejectionError({ appliedRules: [COMBINED_RULE_ID], errors: ["combined settlement produced no claim component"] });
        }
        appliedRules.add(COMBINED_RULE_ID);
      }

      let journalEntryId: JournalEntryId | undefined;

      if (claimAmount > 0) {
        const receivable = resolveInvoiceReceivableAccount(db, {
          invoiceDocumentId: input.invoiceDocumentId,
        });
        if (!receivable.ok) throw new TransactionRejectionError({ appliedRules: [COMBINED_RULE_ID], errors: [receivable.error] });
        if (input.receivableAccountNo && input.receivableAccountNo !== receivable.accountNo) {
          throw new TransactionRejectionError({
            appliedRules: [COMBINED_RULE_ID],
            errors: [`invoice ${invoice.invoice_no} must settle its booked receivable account ${receivable.accountNo}, not ${input.receivableAccountNo}`],
          });
        }
        const carryingBalance = calculateInvoiceReceivableCarryingBalance(db, {
          invoiceDocumentId: input.invoiceDocumentId,
          invoiceNumber: invoice.invoice_no,
          receivableAccountNo: receivable.accountNo,
        });
        if (compareDkk(carryingBalance, principalOpenBalance) !== 0) {
          throw new TransactionRejectionError({
            appliedRules: [COMBINED_RULE_ID],
            errors: [`invoice ${invoice.invoice_no} domain principal ${principalOpenBalance} DKK does not match receivable ${receivable.accountNo} carrying balance ${carryingBalance} DKK`],
          });
        }
        const claimBalances = calculateClaimReceivableBalances(db, {
          invoiceDocumentId: input.invoiceDocumentId,
          asOfDate: claimEvidenceDate,
        });
        if (!claimBalances.ok) {
          throw new TransactionRejectionError({
            appliedRules: [COMBINED_RULE_ID],
            errors: [`combined settlement requires all included claims to be ledger-posted first: ${claimBalances.errors.join("; ")}`],
          });
        }
        const domainClaimOnly = roundDkk(claimOpenBalance - principalOpenBalance);
        if (compareDkk(claimBalances.totalDkk, domainClaimOnly) !== 0) {
          throw new TransactionRejectionError({
            appliedRules: [COMBINED_RULE_ID],
            errors: [`invoice ${invoice.invoice_no} domain claim balance ${domainClaimOnly} DKK does not match ledger-backed claim receivables ${claimBalances.totalDkk} DKK`],
          });
        }
        const claimAllocation = allocateClaimReceipt(claimBalances.balances, claimAmount);
        if (!claimAllocation.ok) {
          throw new TransactionRejectionError({ appliedRules: [COMBINED_RULE_ID], errors: [claimAllocation.error] });
        }
        const bankAccount = resolveSettlementBankAccount(db, {
          bankTransactionId: bank.id,
          requestedAccountNo: input.bankAccountNo,
        });
        if (!bankAccount.ok) throw new TransactionRejectionError({ appliedRules: [COMBINED_RULE_ID], errors: [bankAccount.error] });
        const receivableCredits = new Map<string, number>();
        receivableCredits.set(receivable.accountNo, principalAmount);
        for (const credit of claimAllocation.credits) {
          receivableCredits.set(
            credit.accountNo,
            roundDkk((receivableCredits.get(credit.accountNo) ?? 0) + credit.amountDkk),
          );
        }
        if (receivableCredits.has(bankAccount.accountNo)) {
          throw new TransactionRejectionError({ appliedRules: [COMBINED_RULE_ID], errors: [`bank ledger ${bankAccount.accountNo} cannot also be an invoice receivable account`] });
        }
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
            { accountNo: bankAccount.accountNo, debitAmount: journalAmountDkk, text: `Bank receipt ${invoice.invoice_no}` },
            ...[...receivableCredits.entries()].map(([accountNo, creditAmount]) => ({
              accountNo,
              creditAmount,
              text: `Principal and claim settlement ${invoice.invoice_no}`,
            })),
          ],
        });
        if (!journal.ok || journal.entryId == null) throw new TransactionRejectionError({ appliedRules: journal.appliedRules, errors: journal.errors });
        journalEntryId = journal.entryId;
        for (const rule of journal.appliedRules ?? []) appliedRules.add(rule);

        const evidence = validateInvoiceJournalEvidence(db, {
          invoiceDocumentId: input.invoiceDocumentId,
          candidates: [
            {
              kind: "payment",
              invoiceDocumentId: input.invoiceDocumentId,
              bankTransactionId: bank.id,
              journalEntryId,
              effectiveDate: paymentDate,
              amount: principalAmount,
              currency: invoiceCurrency,
            },
            {
              kind: "claim",
              invoiceDocumentId: input.invoiceDocumentId,
              bankTransactionId: bank.id,
              journalEntryId,
              effectiveDate: paymentDate,
              amount: claimAmount,
              currency: invoiceCurrency,
            },
          ],
        });
        if (!evidence.ok) throw new TransactionRejectionError({ appliedRules: [...appliedRules], errors: evidence.errors });

        const payment = db.query(
          `INSERT INTO invoice_payments (invoice_document_id, bank_transaction_id, journal_entry_id, payment_date, amount, currency, note)
           VALUES (?, ?, ?, ?, ?, ?, ?)
           RETURNING id`,
        ).get(
          input.invoiceDocumentId,
          bank.id,
          journalEntryId,
          paymentDate,
          principalAmount,
          invoiceCurrency,
          `Bank settlement from transaction ${bank.id}`,
        ) as { id: number };
        paymentId = payment.id;
        insertAuditLog(db, {
          eventType: "invoice_payment_apply",
          entityType: "invoice_payment",
          entityId: payment.id,
          message: `Applied payment ${principalAmount} to invoice ${invoice.invoice_no}`,
          createdBy: input.createdBy,
          createdByProgram: input.createdByProgram,
        });

        const claimPayment = db.query(
          `INSERT INTO invoice_claim_payments (invoice_document_id, bank_transaction_id, journal_entry_id, payment_date, amount, currency, note)
           VALUES (?, ?, ?, ?, ?, ?, ?)
           RETURNING id`
        ).get(input.invoiceDocumentId, bank.id, journalEntryId, paymentDate, claimAmount, invoiceCurrency, `Combined settlement claim component from transaction ${bank.id}`) as { id: number };
        claimPaymentId = claimPayment.id;
        insertAuditLog(db, {
          eventType: "invoice_claim_payment_apply",
          entityType: "invoice_claim_payment",
          entityId: claimPayment.id,
          message: `Applied claim receipt ${claimAmount} to invoice ${invoice.invoice_no} via combined settlement`,
          createdBy: input.createdBy,
          createdByProgram: input.createdByProgram,
        });
      } else {
        const payment = applyInvoicePayment(db, {
          invoiceDocumentId: input.invoiceDocumentId,
          bankTransactionId: bank.id,
          paymentDate,
          amount: principalAmount,
          bankAccountNo: input.bankAccountNo,
          receivableAccountNo: input.receivableAccountNo,
          createdBy: input.createdBy,
          createdByProgram: input.createdByProgram,
          note: `Bank settlement from transaction ${bank.id}`,
        });
        if (!payment.ok) throw new TransactionRejectionError({ appliedRules: payment.appliedRules, errors: payment.errors });
        paymentId = payment.paymentId;
        journalEntryId = payment.journalEntryId == null
          ? undefined
          : asJournalEntryId(payment.journalEntryId);
        for (const rule of payment.appliedRules ?? []) appliedRules.add(rule);
      }

      const after = getInvoiceStatus(db, input.invoiceDocumentId);
      if (!after.ok) throw new TransactionRejectionError({ errors: after.errors });

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
    }).immediate();
    return result;
  } catch (error) {
    const parsed = decodeTransactionRejection(error);
    return {
      ok: false,
      appliedRules: [...new Set([RULE_ID, ...(parsed?.appliedRules ?? [])])],
      errors: parsed?.errors ?? [String(error)],
    };
  }
}
