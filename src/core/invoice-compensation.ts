import type { Database } from "bun:sqlite";
import { postJournalEntry, type JournalPostResult } from "./ledger";
import { getInvoiceStatus } from "./invoice-payments";
import { insertAuditLog } from "./actor";
import { isValidIsoDate as looksLikeIsoDate } from "./dates";
import { roundDkk } from "./money";

const RULE_ID = "DK-INVOICE-LATE-COMPENSATION-001";
const REGISTER_RULE_ID = "DK-INVOICE-LATE-COMPENSATION-REGISTER-001";
const BOOKKEEPING_RULE_ID = "DK-INVOICE-LATE-COMPENSATION-BOOKKEEPING-001";

export type CalculateInvoiceLateCompensationInput = {
  invoiceDocumentId: number;
  asOfDate: string;
  compensationAmountDkk?: number;
};

export type CalculateInvoiceLateCompensationResult = {
  ok: boolean;
  invoiceDocumentId?: number;
  invoiceNumber?: string;
  asOfDate?: string;
  effectiveDueDate?: string;
  overdueDays?: number;
  principalOpenBalance?: number;
  isCommercialTransaction?: boolean;
  eligible?: boolean;
  compensationAmountDkk?: number;
  reason?: string;
  appliedRules: string[];
  errors: string[];
};

export type RegisterInvoiceLateCompensationInput = {
  invoiceDocumentId: number;
  asOfDate: string;
  compensationAmountDkk?: number;
  note?: string;
  // Actor attribution for the registration audit_log row (the post step already
  // threads these; the register step must too, or the row leaks the OS user).
  createdBy?: string;
  createdByProgram?: string;
};

export type RegisterInvoiceLateCompensationResult = CalculateInvoiceLateCompensationResult & {
  claimId?: number;
  claimDate?: string;
  claimOpenBalance?: number;
};

export type PostInvoiceLateCompensationToLedgerInput = {
  invoiceDocumentId: number;
  transactionDate?: string;
  receivableAccountNo?: string;
  compensationIncomeAccountNo?: string;
  createdBy?: string;
  createdByProgram?: string;
};

export type PostInvoiceLateCompensationToLedgerResult = JournalPostResult & {
  claimId?: number;
  invoiceDocumentId?: number;
  invoiceNumber?: string;
  compensationAmountDkk?: number;
  claimOpenBalance?: number;
};

const STATUTORY_COMPENSATION_DKK = 310;
const STATUTORY_COMPENSATION_START_DATE = "2013-03-01";

/**
 * A commercial buyer identifier is a Danish CVR (8 digits, optionally DK-prefixed)
 * or an EU-style VAT number (two-letter country code followed by 2-12 alphanumerics).
 * A free-text string that merely happens to be non-empty is not proof of a
 * commercial transaction.
 */
function looksLikeCommercialVatOrCvr(value: unknown): boolean {
  if (typeof value !== "string") return false;
  const normalized = value.trim().toUpperCase().replace(/\s+/g, "");
  if (!normalized) return false;
  if (/^(DK)?\d{8}$/.test(normalized)) return true;
  return /^[A-Z]{2}[A-Z0-9]{2,12}$/.test(normalized);
}

export function calculateInvoiceLateCompensation(db: Database, input: CalculateInvoiceLateCompensationInput): CalculateInvoiceLateCompensationResult {
  const errors: string[] = [];
  if (!Number.isInteger(input.invoiceDocumentId) || input.invoiceDocumentId <= 0) errors.push("invoiceDocumentId must be a positive integer");
  if (!looksLikeIsoDate(input.asOfDate)) errors.push("asOfDate must be YYYY-MM-DD");
  if (input.compensationAmountDkk !== undefined && (!Number.isFinite(input.compensationAmountDkk) || input.compensationAmountDkk < 0)) errors.push("compensationAmountDkk must be a non-negative number when present");
  if (input.compensationAmountDkk !== undefined && roundDkk(input.compensationAmountDkk) > STATUTORY_COMPENSATION_DKK) {
    errors.push(`compensationAmountDkk must not exceed the statutory amount DKK ${STATUTORY_COMPENSATION_DKK}`);
  }
  if (errors.length > 0) return { ok: false, appliedRules: [RULE_ID], errors };

  const invoice = db.query(`SELECT invoice_no, payload_json, document_type, invoice_date FROM documents WHERE id = ?`).get(input.invoiceDocumentId) as { invoice_no: string; payload_json: string | null; document_type: string; invoice_date: string | null } | null;
  if (!invoice) return { ok: false, appliedRules: [RULE_ID], errors: [`invoice document ${input.invoiceDocumentId} does not exist`] };
  if (invoice.document_type !== "issued_invoice") return { ok: false, appliedRules: [RULE_ID], errors: [`document ${input.invoiceDocumentId} is not an issued invoice`] };

  const status = getInvoiceStatus(db, input.invoiceDocumentId, input.asOfDate);
  if (!status.ok) return { ok: false, appliedRules: [RULE_ID], errors: status.errors };

  const payload = invoice.payload_json ? JSON.parse(invoice.payload_json) : null;
  const isCommercialTransaction = looksLikeCommercialVatOrCvr(payload?.buyer?.vatOrCvr);
  const principalOpenBalance = roundDkk(Number(status.openBalance ?? 0));
  const overdueDays = Number(status.overdueDays ?? 0);
  const compensationAmountDkk = roundDkk(input.compensationAmountDkk ?? STATUTORY_COMPENSATION_DKK);
  const coveredByStatutoryAmount = (invoice.invoice_date ?? input.asOfDate) >= STATUTORY_COMPENSATION_START_DATE;
  const eligible = isCommercialTransaction && principalOpenBalance > 0 && overdueDays > 0 && coveredByStatutoryAmount;

  let reason = "eligible";
  if (!isCommercialTransaction) reason = "buyer.vatOrCvr missing; commercial transaction not proven";
  else if (!(principalOpenBalance > 0)) reason = "invoice has no collectible open balance";
  else if (!(overdueDays > 0)) reason = "invoice is not overdue as of the requested date";
  else if (!coveredByStatutoryAmount) reason = `invoice predates statutory compensation start date ${STATUTORY_COMPENSATION_START_DATE}`;

  return {
    ok: true,
    invoiceDocumentId: input.invoiceDocumentId,
    invoiceNumber: status.invoiceNumber,
    asOfDate: input.asOfDate,
    effectiveDueDate: status.effectiveDueDate,
    overdueDays,
    principalOpenBalance,
    isCommercialTransaction,
    eligible,
    compensationAmountDkk: eligible ? compensationAmountDkk : 0,
    reason,
    appliedRules: [RULE_ID],
    errors: [],
  };
}

export function registerInvoiceLateCompensation(db: Database, input: RegisterInvoiceLateCompensationInput): RegisterInvoiceLateCompensationResult {
  const assessment = calculateInvoiceLateCompensation(db, input);
  if (!assessment.ok) return { ...assessment, appliedRules: [...new Set([...(assessment.appliedRules ?? []), REGISTER_RULE_ID])] };
  if (!assessment.eligible || !(Number(assessment.compensationAmountDkk ?? 0) > 0)) {
    return {
      ...assessment,
      ok: false,
      appliedRules: [...new Set([...(assessment.appliedRules ?? []), REGISTER_RULE_ID])],
      errors: [assessment.reason ?? "invoice is not eligible for compensation registration"],
    };
  }

  const existing = db.query(
    `SELECT id, claim_date, amount_dkk FROM invoice_compensation_claims WHERE invoice_document_id = ? LIMIT 1`
  ).get(input.invoiceDocumentId) as { id: number; claim_date: string; amount_dkk: number } | null;
  if (existing) {
    return {
      ok: false,
      invoiceDocumentId: input.invoiceDocumentId,
      invoiceNumber: assessment.invoiceNumber,
      asOfDate: input.asOfDate,
      effectiveDueDate: assessment.effectiveDueDate,
      overdueDays: assessment.overdueDays,
      principalOpenBalance: assessment.principalOpenBalance,
      isCommercialTransaction: assessment.isCommercialTransaction,
      eligible: assessment.eligible,
      compensationAmountDkk: roundDkk(Number(existing.amount_dkk)),
      reason: `compensation claim already registered on ${existing.claim_date}`,
      appliedRules: [RULE_ID, REGISTER_RULE_ID],
      errors: [`invoice ${input.invoiceDocumentId} already has a registered compensation claim`],
    };
  }

  const inserted = db.query(
    `INSERT INTO invoice_compensation_claims (invoice_document_id, claim_date, amount_dkk, note)
     VALUES (?, ?, ?, ?)
     RETURNING id`
  ).get(input.invoiceDocumentId, input.asOfDate, roundDkk(Number(assessment.compensationAmountDkk)), input.note ?? null) as { id: number };

  insertAuditLog(db, {
    eventType: "invoice_compensation_register",
    entityType: "invoice_compensation_claim",
    entityId: inserted.id,
    message: `Registered compensation claim ${roundDkk(Number(assessment.compensationAmountDkk))} on invoice ${assessment.invoiceNumber}`,
    createdBy: input.createdBy,
    createdByProgram: input.createdByProgram,
  });

  const statusAfter = getInvoiceStatus(db, input.invoiceDocumentId, input.asOfDate);
  return {
    ok: true,
    claimId: inserted.id,
    claimDate: input.asOfDate,
    claimOpenBalance: statusAfter.ok ? statusAfter.claimOpenBalance : undefined,
    invoiceDocumentId: input.invoiceDocumentId,
    invoiceNumber: assessment.invoiceNumber,
    asOfDate: input.asOfDate,
    effectiveDueDate: assessment.effectiveDueDate,
    overdueDays: assessment.overdueDays,
    principalOpenBalance: assessment.principalOpenBalance,
    isCommercialTransaction: assessment.isCommercialTransaction,
    eligible: true,
    compensationAmountDkk: roundDkk(Number(assessment.compensationAmountDkk)),
    reason: "registered",
    appliedRules: [RULE_ID, REGISTER_RULE_ID],
    errors: [],
  };
}

export function postInvoiceLateCompensationToLedger(db: Database, input: PostInvoiceLateCompensationToLedgerInput): PostInvoiceLateCompensationToLedgerResult {
  if (!Number.isInteger(input.invoiceDocumentId) || input.invoiceDocumentId <= 0) {
    return { ok: false, appliedRules: [BOOKKEEPING_RULE_ID], errors: ["invoiceDocumentId must be a positive integer"] };
  }

  const claim = db.query(
    `SELECT c.id, c.invoice_document_id, c.claim_date, c.amount_dkk, d.invoice_no
     FROM invoice_compensation_claims c
     JOIN documents d ON d.id = c.invoice_document_id
     WHERE c.invoice_document_id = ?`
  ).get(input.invoiceDocumentId) as {
    id: number;
    invoice_document_id: number;
    claim_date: string;
    amount_dkk: number;
    invoice_no: string;
  } | null;

  if (!claim) {
    return { ok: false, appliedRules: [BOOKKEEPING_RULE_ID], errors: [`invoice ${input.invoiceDocumentId} has no registered compensation claim`] };
  }

  const existing = db.query(
    `SELECT p.id, p.journal_entry_id, j.entry_no
     FROM invoice_compensation_postings p
     JOIN journal_entries j ON j.id = p.journal_entry_id
     WHERE p.compensation_claim_id = ?`
  ).get(claim.id) as { id: number; journal_entry_id: number; entry_no: string } | null;

  if (existing) {
    return {
      ok: false,
      claimId: claim.id,
      invoiceDocumentId: claim.invoice_document_id,
      invoiceNumber: claim.invoice_no,
      compensationAmountDkk: roundDkk(Number(claim.amount_dkk)),
      appliedRules: [BOOKKEEPING_RULE_ID],
      errors: [`compensation claim ${claim.id} is already posted in journal entry ${existing.entry_no}`],
    };
  }

  const amount = roundDkk(Number(claim.amount_dkk));
  try {
    return db.transaction(() => {
      const journal = postJournalEntry(db, {
        transactionDate: input.transactionDate ?? claim.claim_date,
        text: `Compensation claim ${claim.invoice_no}`,
        documentId: claim.invoice_document_id,
        createdBy: input.createdBy,
        createdByProgram: input.createdByProgram,
        lines: [
          { accountNo: input.receivableAccountNo ?? "1100", debitAmount: amount, text: `Compensation receivable ${claim.invoice_no}` },
          { accountNo: input.compensationIncomeAccountNo ?? "1010", creditAmount: amount, text: `Compensation income ${claim.invoice_no}` },
        ],
      });
      if (!journal.ok) {
        return { ...journal, claimId: claim.id, invoiceDocumentId: claim.invoice_document_id, invoiceNumber: claim.invoice_no, compensationAmountDkk: amount, appliedRules: [...new Set([...(journal.appliedRules ?? []), BOOKKEEPING_RULE_ID])] };
      }

      db.run(
        `INSERT INTO invoice_compensation_postings (compensation_claim_id, journal_entry_id) VALUES (?, ?)`,
        claim.id,
        journal.entryId,
      );

      insertAuditLog(db, {
        eventType: "invoice_compensation_post",
        entityType: "invoice_compensation_claim",
        entityId: claim.id,
        message: `Posted compensation claim ${amount} for invoice ${claim.invoice_no} in journal entry ${journal.entryNo}`,
        createdBy: input.createdBy,
        createdByProgram: input.createdByProgram,
      });

      const statusAfter = getInvoiceStatus(db, claim.invoice_document_id, input.transactionDate ?? claim.claim_date);
      return {
        ...journal,
        claimId: claim.id,
        invoiceDocumentId: claim.invoice_document_id,
        invoiceNumber: claim.invoice_no,
        compensationAmountDkk: amount,
        claimOpenBalance: statusAfter.ok ? statusAfter.claimOpenBalance : undefined,
        appliedRules: [...new Set([...(journal.appliedRules ?? []), BOOKKEEPING_RULE_ID])],
      };
    })();
  } catch (error) {
    return {
      ok: false,
      claimId: claim.id,
      invoiceDocumentId: claim.invoice_document_id,
      invoiceNumber: claim.invoice_no,
      compensationAmountDkk: amount,
      appliedRules: [BOOKKEEPING_RULE_ID],
      errors: [String(error)],
    };
  }
}
