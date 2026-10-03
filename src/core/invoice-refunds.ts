import { selectBankTransaction } from "./bank-transaction-selection";
import { TransactionRejectionError, decodeTransactionRejection } from "./transaction-rejection";
import type { Database } from "bun:sqlite";
import { postJournalEntry, type JournalPostResult } from "./ledger";
import { getInvoiceStatus } from "./invoice-payments";
import { insertAuditLog } from "./actor";
import { compareDkk, roundDkk } from "./money";
import { validateInvoiceJournalEvidence } from "./invoice-journal-evidence";
import {
  calculateInvoiceReceivableCarryingBalance,
  resolveInvoiceReceivableAccount,
  resolveSettlementBankAccount,
} from "./invoice-fx-receivable";

const RULE_ID = "DK-INVOICE-REFUND-001";

export type RefundInvoiceToBankInput = {
  invoiceDocumentId: number;
  bankTransactionId?: number;
  bankTransactionReference?: string;
  refundDate?: string;
  amount?: number;
  bankAccountNo?: string;
  receivableAccountNo?: string;
  createdBy?: string;
  createdByProgram?: string;
};

export type RefundInvoiceToBankResult = JournalPostResult & {
  refundId?: number;
  invoiceNumber?: string;
  remainingCreditBalance?: number;
};



export function refundInvoiceToBank(db: Database, input: RefundInvoiceToBankInput): RefundInvoiceToBankResult {
  if (!Number.isInteger(input.invoiceDocumentId) || input.invoiceDocumentId <= 0) {
    return { ok: false, appliedRules: [RULE_ID], errors: ["invoiceDocumentId must be a positive integer"] };
  }
  if (input.bankTransactionId !== undefined && (!Number.isInteger(input.bankTransactionId) || input.bankTransactionId <= 0)) {
    return { ok: false, appliedRules: [RULE_ID], errors: ["bankTransactionId must be a positive integer when present"] };
  }

  const selected = selectBankTransaction(db, input);
  if (selected.error) return { ok: false, appliedRules: [RULE_ID], errors: [selected.error] };
  const bank = selected.bank!;
  const bankAmount = Number(bank.amount);
  if (!Number.isFinite(bankAmount)) return { ok: false, appliedRules: [RULE_ID], errors: [`bank transaction ${bank.id} amount is not a finite number`] };
  if (bankAmount >= 0) return { ok: false, appliedRules: [RULE_ID], errors: [`bank transaction ${bank.id} is not an outgoing customer refund`] };

  const invoice = db.query(`SELECT id, invoice_no, document_type, currency FROM documents WHERE id = ?`).get(input.invoiceDocumentId) as { id: number; invoice_no: string; document_type: string; currency: string | null } | null;
  if (!invoice) return { ok: false, appliedRules: [RULE_ID], errors: [`invoice document ${input.invoiceDocumentId} does not exist`] };
  if (invoice.document_type !== "issued_invoice") return { ok: false, appliedRules: [RULE_ID], errors: [`document ${input.invoiceDocumentId} is not an issued invoice`] };
  // The refund path posts a plain DKK 1100 line and caps the refund against a
  // foreign-unit credit balance read as DKK — it is not currency-aware, so for a
  // foreign-currency invoice it would strand the rate difference on the
  // receivable. Refuse until the refund path handles FX explicitly (mirrors the
  // combined-settlement guard in invoice-settlement.ts; adversarial re-review #3).
  if ((invoice.currency ?? "DKK").trim().toUpperCase() !== "DKK") {
    return { ok: false, appliedRules: [RULE_ID], errors: [`refusion af faktura ${invoice.invoice_no} i fremmed valuta understøttes ikke endnu — refusionsstien er ikke valuta-bevidst (foreign-currency invoice refunds are not supported)`] };
  }

  const existingJournal = db.query(`SELECT journal_entry_id AS id FROM bank_journal_reconciliations WHERE bank_transaction_id = ? LIMIT 1`).get(bank.id) as { id: number } | null;
  if (existingJournal) return { ok: false, appliedRules: [RULE_ID], errors: [`bank transaction ${bank.id} is already linked to journal entry ${existingJournal.id}`] };

  if (db.query(`SELECT id FROM invoice_refunds WHERE bank_transaction_id = ? LIMIT 1`).get(bank.id)) {
    return { ok: false, appliedRules: [RULE_ID], errors: [`bank transaction ${bank.id} is already applied to an invoice refund`] };
  }

  const status = getInvoiceStatus(db, input.invoiceDocumentId);
  if (!status.ok) return { ok: false, appliedRules: [RULE_ID], errors: status.errors };
  const creditBalance = roundDkk(Math.max(0, -(status.openBalance ?? 0)));
  if (creditBalance <= 0) return { ok: false, appliedRules: [RULE_ID], errors: [`invoice ${invoice.invoice_no} has no refundable credit balance`] };

  const amount = roundDkk(input.amount ?? Math.abs(bankAmount));
  if (!(amount > 0)) return { ok: false, appliedRules: [RULE_ID], errors: ["refund amount must be positive"] };
  if (compareDkk(amount, Math.abs(bankAmount)) !== 0) {
    return { ok: false, appliedRules: [RULE_ID], errors: [`refund amount ${amount} must equal outgoing bank transaction ${bank.id} amount ${Math.abs(bankAmount)}`] };
  }
  if (amount > creditBalance) return { ok: false, appliedRules: [RULE_ID], errors: [`refund amount ${amount} exceeds refundable credit balance ${creditBalance}`] };
  const refundDate = input.refundDate ?? bank.transaction_date;
  try {
    const result = db.transaction(() => {
      const lockedStatus = getInvoiceStatus(db, input.invoiceDocumentId);
      if (!lockedStatus.ok) throw new TransactionRejectionError({ appliedRules: [RULE_ID], errors: lockedStatus.errors });
      const lockedCreditBalance = roundDkk(Math.max(0, -Number(lockedStatus.openBalance ?? 0)));
      if (!(lockedCreditBalance > 0)) {
        throw new TransactionRejectionError({ appliedRules: [RULE_ID], errors: [`invoice ${invoice.invoice_no} has no refundable credit balance`] });
      }
      if (compareDkk(amount, lockedCreditBalance) > 0) {
        throw new TransactionRejectionError({ appliedRules: [RULE_ID], errors: [`refund amount ${amount} exceeds refundable credit balance ${lockedCreditBalance}`] });
      }
      const receivable = resolveInvoiceReceivableAccount(db, {
        invoiceDocumentId: input.invoiceDocumentId,
      });
      if (!receivable.ok) throw new TransactionRejectionError({ appliedRules: [RULE_ID], errors: [receivable.error] });
      if (input.receivableAccountNo && input.receivableAccountNo !== receivable.accountNo) {
        throw new TransactionRejectionError({
          appliedRules: [RULE_ID],
          errors: [`invoice ${invoice.invoice_no} must refund its booked receivable account ${receivable.accountNo}, not ${input.receivableAccountNo}`],
        });
      }
      const carryingBalance = calculateInvoiceReceivableCarryingBalance(db, {
        invoiceDocumentId: input.invoiceDocumentId,
        invoiceNumber: invoice.invoice_no,
        receivableAccountNo: receivable.accountNo,
      });
      if (compareDkk(carryingBalance, Number(lockedStatus.openBalance ?? 0)) !== 0) {
        throw new TransactionRejectionError({
          appliedRules: [RULE_ID],
          errors: [`invoice ${invoice.invoice_no} domain balance ${roundDkk(Number(lockedStatus.openBalance ?? 0))} DKK does not match receivable ${receivable.accountNo} carrying balance ${carryingBalance} DKK`],
        });
      }
      const bankAccount = resolveSettlementBankAccount(db, {
        bankTransactionId: bank.id,
        requestedAccountNo: input.bankAccountNo,
      });
      if (!bankAccount.ok) throw new TransactionRejectionError({ appliedRules: [RULE_ID], errors: [bankAccount.error] });
      if (bankAccount.accountNo === receivable.accountNo) {
        throw new TransactionRejectionError({ appliedRules: [RULE_ID], errors: [`bank ledger ${bankAccount.accountNo} cannot also be the invoice receivable account`] });
      }

      const journal = postJournalEntry(db, {
        transactionDate: refundDate,
        text: `Customer refund for invoice ${invoice.invoice_no}`,
        sourceBankTransactionId: bank.id,
        documentId: input.invoiceDocumentId,
        createdBy: input.createdBy,
        createdByProgram: input.createdByProgram,
        lines: [
          { accountNo: receivable.accountNo, debitAmount: amount, text: `Refund clearing ${invoice.invoice_no}` },
          { accountNo: bankAccount.accountNo, creditAmount: amount, text: `Bank refund ${invoice.invoice_no}` },
        ],
      });
      if (!journal.ok || journal.entryId == null) {
        throw new TransactionRejectionError({ appliedRules: journal.appliedRules, errors: journal.errors.length > 0 ? journal.errors : ["refund journal posting returned no entry id"] });
      }

      const evidence = validateInvoiceJournalEvidence(db, {
        invoiceDocumentId: input.invoiceDocumentId,
        candidates: [{
          kind: "refund",
          invoiceDocumentId: input.invoiceDocumentId,
          bankTransactionId: bank.id,
          journalEntryId: journal.entryId,
          effectiveDate: refundDate,
          amount,
          currency: "DKK",
        }],
      });
      if (!evidence.ok) throw new TransactionRejectionError({ appliedRules: [RULE_ID], errors: evidence.errors });

      const refund = db.query(
        `INSERT INTO invoice_refunds (invoice_document_id, bank_transaction_id, journal_entry_id, refund_date, amount, currency, note)
         VALUES (?, ?, ?, ?, ?, 'DKK', ?)
         RETURNING id`
      ).get(input.invoiceDocumentId, bank.id, journal.entryId, refundDate, amount, `Customer refund from transaction ${bank.id}`) as { id: number };

      insertAuditLog(db, {
        eventType: "invoice_refund_apply",
        entityType: "invoice_refund",
        entityId: refund.id,
        message: `Applied refund ${amount} to invoice ${invoice.invoice_no}`,
        createdBy: input.createdBy,
        createdByProgram: input.createdByProgram,
      });

      const after = getInvoiceStatus(db, input.invoiceDocumentId);
      if (!after.ok) throw new TransactionRejectionError({ errors: after.errors });
      return {
        ...journal,
        refundId: refund.id,
        invoiceNumber: invoice.invoice_no,
        remainingCreditBalance: roundDkk(Math.max(0, -(after.openBalance ?? 0))),
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
