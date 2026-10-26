import { canonicalJson } from "./canonical-json";
import type { Database } from "bun:sqlite";
import { insertAuditLog, resolveActor, type ResolveActorInput } from "./actor";
import { resolveOpenExceptionsForBankTransaction } from "./exceptions";
import { roundDkk } from "./money";
import { createHash } from "node:crypto";
import type { StablePrincipal } from "./idempotency";
import { resolveAccountRole } from "./account-roles";

export const BANK_JOURNAL_RECONCILIATION_RULE = "DK-BOOKKEEPING-RECONCILIATION-001";

export type BankJournalMatchMethod =
  | "exact-date-amount"
  | "settlement-lag-amount"
  | "source-reference"
  | "manual-review";

export type LinkBankTransactionToJournalInput = ResolveActorInput & {
  bankTransactionId: number;
  journalEntryId: number;
  matchMethod: BankJournalMatchMethod;
  sourceReference?: string;
  note?: string;
};

export function findBankJournalReconciliation(
  db: Database,
  bankTransactionId: number,
): { bankTransactionId: number; journalEntryId: number; journalEntryNo: string; linkKind: string; reconciliationId?: string } | null {
  const row = db.query(
    `SELECT bank_transaction_id, journal_entry_id, journal_entry_no, link_kind, reconciliation_id
       FROM bank_journal_reconciliations
      WHERE bank_transaction_id = ?
      LIMIT 1`,
  ).get(bankTransactionId) as {
    bank_transaction_id: number;
    journal_entry_id: number;
    journal_entry_no: string;
    link_kind: string; reconciliation_id?: string;
  } | null;
  return row ? {
    bankTransactionId: Number(row.bank_transaction_id),
    journalEntryId: Number(row.journal_entry_id),
    journalEntryNo: row.journal_entry_no,
    linkKind: row.link_kind,
    reconciliationId: row.reconciliation_id,
  } : null;
}

const canonical = (value: unknown): string => value === null || typeof value !== "object"
  ? JSON.stringify(value)
  : Array.isArray(value) ? `[${value.map(canonical).join(",")}]`
  : `{${Object.keys(value as object).sort().map((key) => `${JSON.stringify(key)}:${canonical((value as Record<string, unknown>)[key])}`).join(",")}}`;
const planHash = (value: unknown) => createHash("sha256").update(canonical(value)).digest("hex");
const validText = (value: unknown, max = 1000) => typeof value === "string" && value.trim().length > 0 && value.trim().length <= max ? value.trim() : null;

export type BankReconciliationCorrectionPlanInput = { bankTransactionId: number; replacementJournalEntryId: number };
export type ApplyBankReconciliationCorrectionInput = BankReconciliationCorrectionPlanInput & {
  expectedReconciliationId: string; planHash: string; reason: string; actor?: string; principal?: StablePrincipal; confirm: boolean;
};

type CorrectionContext = {
  reconciliationId: string; bankTransactionId: number; currentJournalEntryId: number; currentJournalEntryNo: string;
  replacementJournalEntryId: number; replacementJournalEntryNo: string; replacementJournalHash: string;
  bankAccountNo: string; bankAmountDkk: number; replacementBankMovement: number;
};

function correctionContext(db: Database, input: BankReconciliationCorrectionPlanInput): CorrectionContext | { error: string } {
  if (!Number.isInteger(input.bankTransactionId) || input.bankTransactionId <= 0 || !Number.isInteger(input.replacementJournalEntryId) || input.replacementJournalEntryId <= 0) return { error: "BANK_TRANSACTION_AND_REPLACEMENT_JOURNAL_REQUIRED" };
  // Include a reversed legacy source here: it is precisely the evidence a correction
  // replaces. The public effective view only exposes active replacement journals.
  const current = db.query(`SELECT reconciliation_id, journal_entry_id, journal_entry_no FROM (
      SELECT 'direct:' || je.id AS reconciliation_id, je.id AS journal_entry_id, je.entry_no AS journal_entry_no FROM journal_entries je WHERE je.source_bank_transaction_id=? AND je.reversal_of_entry_id IS NULL
      UNION ALL SELECT 'append-only:' || link.id, link.journal_entry_id, je.entry_no FROM bank_journal_reconciliation_links link JOIN journal_entries je ON je.id=link.journal_entry_id WHERE link.bank_transaction_id=?
      UNION ALL SELECT 'correction:' || event.id, event.replacement_journal_entry_id, je.entry_no FROM bank_reconciliation_correction_events event JOIN journal_entries je ON je.id=event.replacement_journal_entry_id WHERE event.bank_transaction_id=?
    ) candidate WHERE NOT EXISTS (SELECT 1 FROM bank_reconciliation_correction_events event WHERE event.supersedes_reconciliation_id=candidate.reconciliation_id) ORDER BY reconciliation_id LIMIT 2`).all(input.bankTransactionId, input.bankTransactionId, input.bankTransactionId) as Array<{reconciliation_id:string;journal_entry_id:number;journal_entry_no:string}>;
  if (current.length !== 1) return { error: current.length ? "CONFLICTING_CURRENT_RECONCILIATIONS" : "CURRENT_RECONCILIATION_NOT_FOUND" };
  const bank = db.query(`SELECT bt.amount,bt.amount_dkk,bt.currency,ba.ledger_account_no FROM bank_transactions bt LEFT JOIN bank_accounts ba ON ba.id=bt.bank_account_id WHERE bt.id=?`).get(input.bankTransactionId) as any;
  if (!bank) return { error: "BANK_TRANSACTION_NOT_FOUND" };
  const mappedBankAccountNo = bank?.ledger_account_no || (resolveAccountRole(db, "bank").ok ? (resolveAccountRole(db, "bank") as any).accountNo : null);
  if (!mappedBankAccountNo) return { error: "BANK_ACCOUNT_MAPPING_REQUIRED" };
  const old = db.query(`SELECT je.status,je.reversal_of_entry_id,EXISTS(SELECT 1 FROM journal_entries reversal WHERE reversal.reversal_of_entry_id=je.id) AS has_reversal FROM journal_entries je WHERE je.id=?`).get(current[0]!.journal_entry_id) as any;
  if (!old || (old.status === "posted" && old.reversal_of_entry_id == null && !Number(old.has_reversal))) return { error: "ACTIVE_OLD_JOURNAL_MUST_BE_REVERSED" };
  const replacement = db.query(`SELECT je.id,je.entry_no,je.entry_hash,je.status,je.reversal_of_entry_id,je.source_bank_transaction_id,EXISTS(SELECT 1 FROM journal_entries reversal WHERE reversal.reversal_of_entry_id=je.id) AS has_reversal,COALESCE(SUM(CASE WHEN a.account_no=? THEN jl.debit_amount-jl.credit_amount ELSE 0 END),0) AS bank_movement FROM journal_entries je LEFT JOIN journal_lines jl ON jl.journal_entry_id=je.id LEFT JOIN accounts a ON a.id=jl.account_id WHERE je.id=? GROUP BY je.id`).get(mappedBankAccountNo,input.replacementJournalEntryId) as any;
  if (!replacement) return { error: "REPLACEMENT_JOURNAL_NOT_FOUND" };
  if (replacement.status !== "posted" || replacement.reversal_of_entry_id != null || Number(replacement.has_reversal) || replacement.source_bank_transaction_id != null) return { error: "INVALID_REPLACEMENT_JOURNAL" };
  const alreadyUsed = db.query("SELECT 1 FROM bank_journal_reconciliations WHERE journal_entry_id=? LIMIT 1").get(input.replacementJournalEntryId);
  if (alreadyUsed) return { error: "REPLACEMENT_JOURNAL_ALREADY_RECONCILED" };
  const amount = String(bank.currency).trim().toUpperCase() === "DKK" ? Number(bank.amount) : Number(bank.amount_dkk);
  if (!Number.isFinite(amount)) return { error: "BANK_AMOUNT_DKK_REQUIRED" };
  if (roundDkk(Number(replacement.bank_movement)) !== roundDkk(amount)) return { error: "REPLACEMENT_BANK_AMOUNT_OR_ACCOUNT_MISMATCH" };
  return { reconciliationId:current[0]!.reconciliation_id, bankTransactionId:input.bankTransactionId,currentJournalEntryId:Number(current[0]!.journal_entry_id),currentJournalEntryNo:current[0]!.journal_entry_no,replacementJournalEntryId:Number(replacement.id),replacementJournalEntryNo:replacement.entry_no,replacementJournalHash:replacement.entry_hash,bankAccountNo:mappedBankAccountNo,bankAmountDkk:roundDkk(amount),replacementBankMovement:roundDkk(Number(replacement.bank_movement)) };
}

/** Read-only, hash-bound correction proposal. It never changes either journal. */
export function planBankReconciliationCorrection(db: Database, input: BankReconciliationCorrectionPlanInput) {
  const context = correctionContext(db, input); if ("error" in context) return { ok:false as const, errors:[context.error] };
  const plan = { schemaVersion:"rentemester-bank-reconciliation-correction-v1", ...context };
  return { ok:true as const, plan:{ ...plan, planHash:planHash(plan) }, errors:[] as string[] };
}

/** Atomically supersedes exactly one reviewed reconciliation with a replacement journal. */
export function applyBankReconciliationCorrection(db: Database, input: ApplyBankReconciliationCorrectionInput) {
  if (!input.confirm) return { ok:false as const, errors:["CONFIRMATION_REQUIRED"] };
  const actor = validText(input.actor,160), reason=validText(input.reason,1000);
  const principalKind = input.principal?.kind === "user" || input.principal?.kind === "service-account" ? input.principal.kind : null;
  const principalSubjectId = validText(input.principal?.subjectId,160);
  if (!actor || !principalKind || !principalSubjectId) return { ok:false as const, errors:["ACTOR_AND_PRINCIPAL_REQUIRED"] };
  if (!reason) return { ok:false as const, errors:["REASON_REQUIRED"] };
  return db.transaction(() => {
    const planned=planBankReconciliationCorrection(db,input); if(!planned.ok)return planned;
    if(planned.plan.reconciliationId!==input.expectedReconciliationId)return {ok:false as const,errors:["CURRENT_RECONCILIATION_CONFLICT"]};
    if(planned.plan.planHash!==input.planHash)return {ok:false as const,errors:["PLAN_HASH_MISMATCH"]};
    const [supersedesKind, supersedesId] = planned.plan.reconciliationId.split(":", 2);
    const inserted=db.query(`INSERT INTO bank_reconciliation_correction_events(bank_transaction_id,supersedes_kind,supersedes_id,supersedes_reconciliation_id,superseded_journal_entry_id,replacement_journal_entry_id,bank_account_no,bank_amount_dkk,replacement_journal_hash,plan_hash,reason,actor,principal_kind,principal_subject_id,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?) RETURNING id`).get(planned.plan.bankTransactionId,supersedesKind,supersedesId,planned.plan.reconciliationId,planned.plan.currentJournalEntryId,planned.plan.replacementJournalEntryId,planned.plan.bankAccountNo,planned.plan.bankAmountDkk,planned.plan.replacementJournalHash,input.planHash,reason,actor,principalKind,principalSubjectId,new Date().toISOString()) as any;
    insertAuditLog(db,{eventType:"bank_reconciliation_corrected",entityType:"bank_transaction",entityId:String(input.bankTransactionId),message:`Superseded reconciliation ${planned.plan.reconciliationId} with journal ${planned.plan.replacementJournalEntryNo}`,createdBy:actor,createdByProgram:"bank-reconciliation-correction"});
    return {ok:true as const,id:Number(inserted.id),idempotent:false,planHash:input.planHash,reconciliationId:`correction:${inserted.id}`,errors:[] as string[]};
  }).immediate();
}

/**
 * Append-only reconciliation for a bank row whose accounting journal already
 * exists (typically after a verified migration). No journal row or amount is
 * changed. The v6 database guard independently proves that the journal's net
 * movement on the bank account mapped to the row equals the bank amount in DKK.
 */
export function linkBankTransactionToJournal(
  db: Database,
  input: LinkBankTransactionToJournalInput,
) {
  const errors: string[] = [];
  if (!Number.isInteger(input.bankTransactionId) || input.bankTransactionId <= 0) {
    errors.push("bankTransactionId must be a positive integer");
  }
  if (!Number.isInteger(input.journalEntryId) || input.journalEntryId <= 0) {
    errors.push("journalEntryId must be a positive integer");
  }
  const methods: BankJournalMatchMethod[] = ["exact-date-amount", "settlement-lag-amount", "source-reference", "manual-review"];
  if (!methods.includes(input.matchMethod)) errors.push("matchMethod is invalid");
  if (input.sourceReference !== undefined && !input.sourceReference.trim()) errors.push("sourceReference must not be blank when present");
  if (input.note !== undefined && !input.note.trim()) errors.push("note must not be blank when present");
  if (errors.length > 0) return { ok: false as const, appliedRules: [BANK_JOURNAL_RECONCILIATION_RULE], errors };

  const existing = findBankJournalReconciliation(db, input.bankTransactionId);
  if (existing) {
    if (existing.journalEntryId === input.journalEntryId) {
      return { ok: true as const, idempotent: true, ...existing, appliedRules: [BANK_JOURNAL_RECONCILIATION_RULE], errors: [] as string[] };
    }
    return { ok: false as const, appliedRules: [BANK_JOURNAL_RECONCILIATION_RULE], errors: [
      `bank transaction ${input.bankTransactionId} is already reconciled to journal entry ${existing.journalEntryId}`,
    ] };
  }

  const bank = db.query(
    `SELECT bt.id, bt.amount, bt.amount_dkk, bt.currency, bt.transaction_date,
            ba.ledger_account_no
       FROM bank_transactions bt
       LEFT JOIN bank_accounts ba ON ba.id = bt.bank_account_id
      WHERE bt.id = ?`,
  ).get(input.bankTransactionId) as {
    id: number; amount: number; amount_dkk: number | null; currency: string;
    transaction_date: string; ledger_account_no: string | null;
  } | null;
  if (!bank) return { ok: false as const, appliedRules: [BANK_JOURNAL_RECONCILIATION_RULE], errors: [`bank transaction ${input.bankTransactionId} does not exist`] };
  if (!bank.ledger_account_no) return { ok: false as const, appliedRules: [BANK_JOURNAL_RECONCILIATION_RULE], errors: [`bank transaction ${input.bankTransactionId} has no bank account with a ledger mapping`] };

  const journal = db.query(
    `SELECT je.id, je.entry_no, je.transaction_date, je.status, je.reversal_of_entry_id,
            je.source_bank_transaction_id,
            COALESCE(SUM(CASE WHEN a.account_no = ? THEN jl.debit_amount - jl.credit_amount ELSE 0 END), 0) AS bank_movement,
            EXISTS(SELECT 1 FROM journal_entries reversal WHERE reversal.reversal_of_entry_id = je.id) AS has_reversal
       FROM journal_entries je
       LEFT JOIN journal_lines jl ON jl.journal_entry_id = je.id
       LEFT JOIN accounts a ON a.id = jl.account_id
      WHERE je.id = ?
      GROUP BY je.id`,
  ).get(bank.ledger_account_no, input.journalEntryId) as {
    id: number; entry_no: string; transaction_date: string; status: string;
    reversal_of_entry_id: number | null; source_bank_transaction_id: number | null;
    bank_movement: number; has_reversal: number;
  } | null;
  if (!journal) return { ok: false as const, appliedRules: [BANK_JOURNAL_RECONCILIATION_RULE], errors: [`journal entry ${input.journalEntryId} does not exist`] };
  if (journal.status !== "posted" || journal.reversal_of_entry_id != null || Number(journal.has_reversal) !== 0) {
    errors.push(`journal entry ${input.journalEntryId} is not an active original posted entry`);
  }
  if (journal.source_bank_transaction_id != null) errors.push(`journal entry ${input.journalEntryId} already carries a direct bank link`);
  const bankAmountDkk = bank.currency.trim().toUpperCase() === "DKK" ? Number(bank.amount) : Number(bank.amount_dkk);
  if (!Number.isFinite(bankAmountDkk)) errors.push(`bank transaction ${input.bankTransactionId} has no deterministic DKK amount`);
  if (Number.isFinite(bankAmountDkk) && roundDkk(Number(journal.bank_movement)) !== roundDkk(bankAmountDkk)) {
    errors.push(`journal entry ${input.journalEntryId} bank movement ${roundDkk(Number(journal.bank_movement))} does not equal bank transaction ${input.bankTransactionId} amount ${roundDkk(bankAmountDkk)}`);
  }
  if (errors.length > 0) return { ok: false as const, appliedRules: [BANK_JOURNAL_RECONCILIATION_RULE], errors };

  const actor = resolveActor(input);
  try {
    return db.transaction(() => {
      const inserted = db.query(
        `INSERT INTO bank_journal_reconciliation_links
           (bank_transaction_id, journal_entry_id, match_method, source_reference, note, created_by, created_by_program)
         VALUES (?, ?, ?, ?, ?, ?, ?)`,
      ).run(
        input.bankTransactionId,
        input.journalEntryId,
        input.matchMethod,
        input.sourceReference?.trim() || null,
        input.note?.trim() || null,
        actor.createdBy,
        actor.createdByProgram,
      );
      insertAuditLog(db, {
        eventType: "bank_journal_reconciliation_link",
        entityType: "bank_transaction",
        entityId: String(input.bankTransactionId),
        message: `Reconciled bank transaction ${input.bankTransactionId} to existing journal entry ${journal.entry_no} without a new posting (${input.matchMethod})`,
        createdBy: actor.createdBy,
        createdByProgram: actor.createdByProgram,
      });
      resolveOpenExceptionsForBankTransaction(
        db,
        input.bankTransactionId,
        `Resolved by append-only reconciliation link to journal entry ${journal.entry_no}`,
        actor.createdBy,
      );
      return {
        ok: true as const,
        id: Number(inserted.lastInsertRowid),
        idempotent: false,
        bankTransactionId: input.bankTransactionId,
        journalEntryId: input.journalEntryId,
        journalEntryNo: journal.entry_no,
        linkKind: "append-only",
        bankAmount: roundDkk(bankAmountDkk),
        journalBankMovement: roundDkk(Number(journal.bank_movement)),
        appliedRules: [BANK_JOURNAL_RECONCILIATION_RULE],
        errors: [] as string[],
      };
    }).immediate();
  } catch (error) {
    return { ok: false as const, appliedRules: [BANK_JOURNAL_RECONCILIATION_RULE], errors: [error instanceof Error ? error.message : String(error)] };
  }
}
