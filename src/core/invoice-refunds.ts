import type { Database } from "bun:sqlite";
import { postJournalEntry, type JournalPostResult } from "./ledger";
import { getInvoiceStatus } from "./invoice-payments";
import { insertAuditLog } from "./actor";
import { roundDkk } from "./money";

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


function getOutgoingRefundBankTransaction(db: Database, input: RefundInvoiceToBankInput) {
  if (input.bankTransactionId === undefined && !input.bankTransactionReference) {
    return { error: "bankTransactionId or bankTransactionReference is required" };
  }
  const bank = (input.bankTransactionId !== undefined
    ? db.query(`SELECT id, transaction_date, amount, text, reference FROM bank_transactions WHERE id = ?`).get(input.bankTransactionId)
    : db.query(`SELECT id, transaction_date, amount, text, reference FROM bank_transactions WHERE reference = ? ORDER BY id DESC LIMIT 1`).get(input.bankTransactionReference)) as { id: number; transaction_date: string; amount: number; text: string; reference: string | null } | null;
  if (!bank) {
    return { error: input.bankTransactionId !== undefined ? `bank transaction ${input.bankTransactionId} does not exist` : `no bank transaction found with reference ${input.bankTransactionReference}` };
  }
  return { bank };
}

export function refundInvoiceToBank(db: Database, input: RefundInvoiceToBankInput): RefundInvoiceToBankResult {
  if (!Number.isInteger(input.invoiceDocumentId) || input.invoiceDocumentId <= 0) {
    return { ok: false, appliedRules: [RULE_ID], errors: ["invoiceDocumentId must be a positive integer"] };
  }
  if (input.bankTransactionId !== undefined && (!Number.isInteger(input.bankTransactionId) || input.bankTransactionId <= 0)) {
    return { ok: false, appliedRules: [RULE_ID], errors: ["bankTransactionId must be a positive integer when present"] };
  }

  const selected = getOutgoingRefundBankTransaction(db, input);
  if (selected.error) return { ok: false, appliedRules: [RULE_ID], errors: [selected.error] };
  const bank = selected.bank!;
  if (Number(bank.amount) >= 0) return { ok: false, appliedRules: [RULE_ID], errors: [`bank transaction ${bank.id} is not an outgoing customer refund`] };

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

  const existingJournal = db.query(`SELECT id FROM journal_entries WHERE source_bank_transaction_id = ? LIMIT 1`).get(bank.id) as { id: number } | null;
  if (existingJournal) return { ok: false, appliedRules: [RULE_ID], errors: [`bank transaction ${bank.id} is already linked to journal entry ${existingJournal.id}`] };

  if (db.query(`SELECT id FROM invoice_refunds WHERE bank_transaction_id = ? LIMIT 1`).get(bank.id)) {
    return { ok: false, appliedRules: [RULE_ID], errors: [`bank transaction ${bank.id} is already applied to an invoice refund`] };
  }

  const status = getInvoiceStatus(db, input.invoiceDocumentId);
  if (!status.ok) return { ok: false, appliedRules: [RULE_ID], errors: status.errors };
  const creditBalance = roundDkk(Math.max(0, -(status.openBalance ?? 0)));
  if (creditBalance <= 0) return { ok: false, appliedRules: [RULE_ID], errors: [`invoice ${invoice.invoice_no} has no refundable credit balance`] };

  const amount = roundDkk(input.amount ?? Math.abs(Number(bank.amount)));
  if (amount > creditBalance) return { ok: false, appliedRules: [RULE_ID], errors: [`refund amount ${amount} exceeds refundable credit balance ${creditBalance}`] };
  const refundDate = input.refundDate ?? bank.transaction_date;

  try {
    const result = db.transaction(() => {
      const refund = db.query(
        `INSERT INTO invoice_refunds (invoice_document_id, bank_transaction_id, refund_date, amount, currency, note)
         VALUES (?, ?, ?, ?, 'DKK', ?)
         RETURNING id`
      ).get(input.invoiceDocumentId, bank.id, refundDate, amount, `Customer refund from transaction ${bank.id}`) as { id: number };

      insertAuditLog(db, {
        eventType: "invoice_refund_apply",
        entityType: "invoice_refund",
        entityId: refund.id,
        message: `Applied refund ${amount} to invoice ${invoice.invoice_no}`,
        createdBy: input.createdBy,
        createdByProgram: input.createdByProgram,
      });

      const journal = postJournalEntry(db, {
        transactionDate: refundDate,
        text: `Customer refund for invoice ${invoice.invoice_no}`,
        sourceBankTransactionId: bank.id,
        documentId: input.invoiceDocumentId,
        createdBy: input.createdBy,
        createdByProgram: input.createdByProgram,
        lines: [
          { accountNo: input.receivableAccountNo ?? "1100", debitAmount: amount, text: `Refund clearing ${invoice.invoice_no}` },
          { accountNo: input.bankAccountNo ?? "2000", creditAmount: amount, text: `Bank refund ${invoice.invoice_no}` },
        ],
      });
      if (!journal.ok) throw new Error(JSON.stringify({ appliedRules: journal.appliedRules, errors: journal.errors }));

      const after = getInvoiceStatus(db, input.invoiceDocumentId);
      if (!after.ok) throw new Error(JSON.stringify({ errors: after.errors }));
      return {
        ...journal,
        refundId: refund.id,
        invoiceNumber: invoice.invoice_no,
        remainingCreditBalance: roundDkk(Math.max(0, -(after.openBalance ?? 0))),
        appliedRules: [...new Set([RULE_ID, ...(journal.appliedRules ?? [])])],
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
