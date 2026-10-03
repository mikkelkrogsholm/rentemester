import { TransactionRejectionError, decodeTransactionRejection } from "./transaction-rejection";
import type { Database } from "bun:sqlite";
import { getInvoiceStatus } from "./invoice-payments";
import { postJournalEntry, type JournalPostResult } from "./ledger";
import { insertAuditLog } from "./actor";
import { compareDkk, normalizeCurrency, roundDkk } from "./money";
import { validateInvoiceJournalEvidence } from "./invoice-journal-evidence";
import { allocateClaimReceipt, calculateClaimReceivableBalances } from "./invoice-claim-receivable";
import { resolveSettlementBankAccount } from "./invoice-fx-receivable";
import { isValidIsoDate as looksLikeIsoDate } from "./dates";

const RULE_ID = "DK-INVOICE-CLAIM-SETTLEMENT-001";

export type SettleInvoiceClaimsFromBankInput = {
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

export type SettleInvoiceClaimsFromBankResult = JournalPostResult & {
  claimPaymentId?: number;
  invoiceNumber?: string;
  remainingClaimOpenBalance?: number;
};


function getIncomingClaimBankTransaction(db: Database, input: SettleInvoiceClaimsFromBankInput) {
  if (input.bankTransactionId === undefined && !input.bankTransactionReference) {
    return { error: "bankTransactionId or bankTransactionReference is required" };
  }
  const bank = (input.bankTransactionId !== undefined
    ? db.query(`SELECT id, transaction_date, amount, currency, text, reference FROM bank_transactions WHERE id = ?`).get(input.bankTransactionId)
    : db.query(`SELECT id, transaction_date, amount, currency, text, reference FROM bank_transactions WHERE reference = ? ORDER BY id DESC LIMIT 1`).get(input.bankTransactionReference ?? "")) as { id: number; transaction_date: string; amount: number; currency: string | null; text: string; reference: string | null } | null;
  if (!bank) {
    return { error: input.bankTransactionId !== undefined ? `bank transaction ${input.bankTransactionId} does not exist` : `no bank transaction found with reference ${input.bankTransactionReference}` };
  }
  return { bank };
}

export function settleInvoiceClaimsFromBank(db: Database, input: SettleInvoiceClaimsFromBankInput): SettleInvoiceClaimsFromBankResult {
  if (!Number.isInteger(input.invoiceDocumentId) || input.invoiceDocumentId <= 0) {
    return { ok: false, appliedRules: [RULE_ID], errors: ["invoiceDocumentId must be a positive integer"] };
  }
  if (input.bankTransactionId !== undefined && (!Number.isInteger(input.bankTransactionId) || input.bankTransactionId <= 0)) {
    return { ok: false, appliedRules: [RULE_ID], errors: ["bankTransactionId must be a positive integer when present"] };
  }
  if (input.paymentDate !== undefined && !looksLikeIsoDate(input.paymentDate)) {
    return { ok: false, appliedRules: [RULE_ID], errors: ["paymentDate must be YYYY-MM-DD when present"] };
  }

  const selected = getIncomingClaimBankTransaction(db, input);
  if (selected.error) return { ok: false, appliedRules: [RULE_ID], errors: [selected.error] };
  const bank = selected.bank!;
  const bankAmount = Number(bank.amount);
  if (!Number.isFinite(bankAmount)) return { ok: false, appliedRules: [RULE_ID], errors: [`bank transaction ${bank.id} amount is not a finite number`] };
  if (bankAmount <= 0) return { ok: false, appliedRules: [RULE_ID], errors: [`bank transaction ${bank.id} is not an incoming claim receipt`] };
  if (normalizeCurrency(bank.currency) !== "DKK") return { ok: false, appliedRules: [RULE_ID], errors: [`bank transaction ${bank.id} claim receipt must be denominated in DKK`] };

  const invoice = db.query(
    `SELECT id, invoice_no FROM documents WHERE id = ? AND document_type = 'issued_invoice'`
  ).get(input.invoiceDocumentId) as { id: number; invoice_no: string } | null;
  if (!invoice) return { ok: false, appliedRules: [RULE_ID], errors: [`invoice document ${input.invoiceDocumentId} is not an issued invoice`] };

  const existingJournal = db.query(`SELECT journal_entry_id AS id FROM bank_journal_reconciliations WHERE bank_transaction_id = ? LIMIT 1`).get(bank.id) as { id: number } | null;
  if (existingJournal) return { ok: false, appliedRules: [RULE_ID], errors: [`bank transaction ${bank.id} is already linked to journal entry ${existingJournal.id}`] };
  if (db.query(`SELECT id FROM invoice_claim_payments WHERE bank_transaction_id = ? LIMIT 1`).get(bank.id)) {
    return { ok: false, appliedRules: [RULE_ID], errors: [`bank transaction ${bank.id} is already applied to an invoice claim payment`] };
  }
  if (db.query(`SELECT id FROM invoice_payments WHERE bank_transaction_id = ? LIMIT 1`).get(bank.id)) {
    return { ok: false, appliedRules: [RULE_ID], errors: [`bank transaction ${bank.id} is already applied to an invoice principal payment`] };
  }

  const status = getInvoiceStatus(db, input.invoiceDocumentId);
  if (!status.ok) return { ok: false, appliedRules: [RULE_ID], errors: status.errors };
  const principalOpenBalance = roundDkk(Number(status.openBalance ?? 0));
  const claimOpenBalance = roundDkk(Number(status.claimOpenBalance ?? 0));
  if (principalOpenBalance !== 0) return { ok: false, appliedRules: [RULE_ID], errors: [`invoice ${invoice.invoice_no} still has principal open balance ${principalOpenBalance}; settle principal before claim receipts`] };
  if (!(claimOpenBalance > 0)) return { ok: false, appliedRules: [RULE_ID], errors: [`invoice ${invoice.invoice_no} has no outstanding claim balance`] };

  const amount = roundDkk(input.amount ?? bankAmount);
  if (!(amount > 0)) return { ok: false, appliedRules: [RULE_ID], errors: ["claim receipt amount must be positive"] };
  if (compareDkk(amount, bankAmount) !== 0) {
    return { ok: false, appliedRules: [RULE_ID], errors: [`claim receipt amount ${amount} must equal bank transaction ${bank.id} amount ${bankAmount}`] };
  }
  if (amount > claimOpenBalance) return { ok: false, appliedRules: [RULE_ID], errors: [`claim receipt amount ${amount} exceeds claim open balance ${claimOpenBalance}`] };
  const paymentDate = input.paymentDate ?? bank.transaction_date;
  const claimEvidenceDate = paymentDate < bank.transaction_date ? paymentDate : bank.transaction_date;
  try {
    const result = db.transaction(() => {
      const lockedStatus = getInvoiceStatus(db, input.invoiceDocumentId);
      if (!lockedStatus.ok) throw new TransactionRejectionError({ appliedRules: [RULE_ID], errors: lockedStatus.errors });
      const lockedPrincipalOpen = roundDkk(Number(lockedStatus.openBalance ?? 0));
      const lockedClaimOpen = roundDkk(Number(lockedStatus.claimOpenBalance ?? 0));
      if (lockedPrincipalOpen !== 0) {
        throw new TransactionRejectionError({
          appliedRules: [RULE_ID],
          errors: [`invoice ${invoice.invoice_no} still has principal open balance ${lockedPrincipalOpen}; settle principal before claim receipts`],
        });
      }

      const claimBalances = calculateClaimReceivableBalances(db, {
        invoiceDocumentId: input.invoiceDocumentId,
        asOfDate: claimEvidenceDate,
      });
      if (!claimBalances.ok) {
        throw new TransactionRejectionError({ appliedRules: [RULE_ID], errors: claimBalances.errors });
      }
      if (compareDkk(claimBalances.totalDkk, lockedClaimOpen) !== 0) {
        throw new TransactionRejectionError({
          appliedRules: [RULE_ID],
          errors: [`invoice ${invoice.invoice_no} claim balance ${lockedClaimOpen} DKK does not match ledger-backed claim receivables ${claimBalances.totalDkk} DKK`],
        });
      }
      if (compareDkk(amount, claimBalances.totalDkk) > 0) {
        throw new TransactionRejectionError({
          appliedRules: [RULE_ID],
          errors: [`claim receipt amount ${amount} exceeds ledger-backed claim balance ${claimBalances.totalDkk}`],
        });
      }
      const allocation = allocateClaimReceipt(claimBalances.balances, amount);
      if (!allocation.ok) throw new TransactionRejectionError({ appliedRules: [RULE_ID], errors: [allocation.error] });
      if (
        input.receivableAccountNo &&
        allocation.credits.some((credit) => credit.accountNo !== input.receivableAccountNo)
      ) {
        throw new TransactionRejectionError({
          appliedRules: [RULE_ID],
          errors: [`claim receipt must clear its ledger-backed receivable account(s) ${allocation.credits.map((row) => row.accountNo).join(", ")}, not ${input.receivableAccountNo}`],
        });
      }
      const bankAccount = resolveSettlementBankAccount(db, {
        bankTransactionId: bank.id,
        requestedAccountNo: input.bankAccountNo,
      });
      if (!bankAccount.ok) throw new TransactionRejectionError({ appliedRules: [RULE_ID], errors: [bankAccount.error] });
      if (allocation.credits.some((credit) => credit.accountNo === bankAccount.accountNo)) {
        throw new TransactionRejectionError({ appliedRules: [RULE_ID], errors: [`bank ledger ${bankAccount.accountNo} cannot also be the claim receivable account`] });
      }

      const journal = postJournalEntry(db, {
        transactionDate: paymentDate,
        text: `Customer claim payment for invoice ${invoice.invoice_no}`,
        sourceBankTransactionId: bank.id,
        documentId: input.invoiceDocumentId,
        createdBy: input.createdBy,
        createdByProgram: input.createdByProgram,
        lines: [
          { accountNo: bankAccount.accountNo, debitAmount: amount, text: `Bank claim receipt ${invoice.invoice_no}` },
          ...allocation.credits.map((credit) => ({
            accountNo: credit.accountNo,
            creditAmount: credit.amountDkk,
            text: `Claim receivable settlement ${invoice.invoice_no}`,
          })),
        ],
      });
      if (!journal.ok || journal.entryId == null) {
        throw new TransactionRejectionError({ appliedRules: journal.appliedRules, errors: journal.errors.length > 0 ? journal.errors : ["claim settlement journal posting returned no entry id"] });
      }

      const evidence = validateInvoiceJournalEvidence(db, {
        invoiceDocumentId: input.invoiceDocumentId,
        candidates: [{
          kind: "claim",
          invoiceDocumentId: input.invoiceDocumentId,
          bankTransactionId: bank.id,
          journalEntryId: journal.entryId,
          effectiveDate: paymentDate,
          amount,
          currency: "DKK",
        }],
      });
      if (!evidence.ok) throw new TransactionRejectionError({ appliedRules: [RULE_ID], errors: evidence.errors });

      const payment = db.query(
        `INSERT INTO invoice_claim_payments (invoice_document_id, bank_transaction_id, journal_entry_id, payment_date, amount, currency, note)
         VALUES (?, ?, ?, ?, ?, 'DKK', ?)
         RETURNING id`
      ).get(input.invoiceDocumentId, bank.id, journal.entryId, paymentDate, amount, `Claim settlement from transaction ${bank.id}`) as { id: number };

      insertAuditLog(db, {
        eventType: "invoice_claim_payment_apply",
        entityType: "invoice_claim_payment",
        entityId: payment.id,
        message: `Applied claim receipt ${amount} to invoice ${invoice.invoice_no}`,
        createdBy: input.createdBy,
        createdByProgram: input.createdByProgram,
      });

      const after = getInvoiceStatus(db, input.invoiceDocumentId);
      if (!after.ok) throw new TransactionRejectionError({ errors: after.errors });

      return {
        ...journal,
        claimPaymentId: payment.id,
        invoiceNumber: invoice.invoice_no,
        remainingClaimOpenBalance: roundDkk(Number(after.claimOpenBalance ?? 0)),
        appliedRules: [...new Set([RULE_ID, ...(journal.appliedRules ?? [])])],
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
