import { canonicalJson } from "./canonical-json";
/** Durable transaction-owning retry receipts for high-risk local writes (#583). */
import { createHash } from "node:crypto";
import type { Database } from "bun:sqlite";
import { insertAuditLog } from "./actor";

export const IDEMPOTENCY_KEY_MAX_LENGTH = 128;
export const IDEMPOTENCY_PAYLOAD_MAX_BYTES = 65_536;
export const IDEMPOTENCY_RETENTION_DAYS = 30;
export type RetryClass = "safe-read" | "key-idempotent" | "natural-idempotent" | "external-provider-reconciled" | "unsafe-read-back";
export const RETRY_CLASS_BY_OPERATION: Readonly<Record<string, RetryClass>> = Object.freeze({
  journal_post: "key-idempotent", journal_reverse: "key-idempotent", expense_book: "key-idempotent", payable_register: "key-idempotent", payable_pay: "key-idempotent",
  purchase_case_create: "key-idempotent", purchase_case_review: "key-idempotent",
  bookkeeping_batch_apply: "natural-idempotent", reconcile_bank: "natural-idempotent", bank_import: "natural-idempotent",
  bank_reconciliation_correction_apply: "key-idempotent",
  direct_bank_purchase_payable_correction_apply: "key-idempotent",
  efaktura_send: "external-provider-reconciled", invoice_send_email: "unsafe-read-back",
});
/** Single reviewed retry registry for runtime and agent discovery. */
export const RETRY_OPERATION_NAMES = Object.freeze({
  keyIdempotent: new Set(["journal_post", "journal_reverse", "expense_book", "payable_register", "payable_pay", "bank_reconciliation_correction_apply", "direct_bank_purchase_payable_correction_apply"]),
  naturalIdempotent: new Set(["bank_import", "bank_legacy_binding_apply", "bookkeeping_batch_apply", "bookkeeping_batch_approve", "bookkeeping_batch_dry_run", "bookkeeping_batch_persist", "dimension_budget_apply", "documents_enrich", "documents_extract_invoice", "documents_parse", "documents_parse_pending", "documents_set_company_context", "documents_review_purchase_vat_evidence", "documents_review_non_eu_reverse_charge_evidence", "documents_party_link_apply", "documents_party_link_supersede", "documents_internal_no_external_party", "documents_internal_no_external_party_supersede", "invoice_imported_receivables_backfill_apply", "invoice_imported_receivable_settlement_apply", "invoice_render", "payable_legacy_backfill_apply", "posting_rule_propose", "recurring_invoice_generate", "recurring_invoice_run_workspace", "vat_filing_evidence_record"]),
  externalProviderReconciled: new Set(["efaktura_konfigurer", "efaktura_onboard", "efaktura_registrer", "efaktura_send", "efaktura_modtag", "efaktura_modtag_workspace", "efaktura_status", "peppol_submit_public_invoice", "mail_intake_ingest", "imap_intake_poll", "customer_validate_vat", "cvr_lookup"]),
  naturalIdempotentCli: new Set(["import contacts", "bookkeeping-batch persist", "bookkeeping-batch dry-run", "bookkeeping-batch approve", "bookkeeping-batch apply", "dimensions budget-apply"]),
});
export type StablePrincipal = { kind: "user" | "service-account"; subjectId: string };
export type IdempotencyReceipt = { replayed: boolean; receiptId: number; createdAt: string; expiresAt: string };
export type BusinessRejection = { ok: false; errors?: string[] };
export class IdempotencyError extends Error {
  constructor(readonly code: "IDEMPOTENCY_CONFLICT" | "IDEMPOTENCY_OUTCOME_EXPIRED" | "IDEMPOTENCY_AUTH_REQUIRED" | "IDEMPOTENCY_STORAGE_FAILURE", message: string) { super(message); this.name = "IdempotencyError"; }
}

export function canonicalPayloadHash(value: unknown): string {
  const serialized = canonicalJson(value);
  if (Buffer.byteLength(serialized, "utf8") > IDEMPOTENCY_PAYLOAD_MAX_BYTES) throw new IdempotencyError("IDEMPOTENCY_STORAGE_FAILURE", "validated idempotency payload exceeds receipt limit");
  return createHash("sha256").update(serialized).digest("hex");
}
export function validateIdempotencyKey(value: unknown): string | undefined {
  if (value === undefined || value === null) return undefined;
  if (typeof value !== "string" || value.trim().length === 0 || value.length > IDEMPOTENCY_KEY_MAX_LENGTH) throw new IdempotencyError("IDEMPOTENCY_STORAGE_FAILURE", `idempotency key must be a non-empty string of at most ${IDEMPOTENCY_KEY_MAX_LENGTH} characters`);
  return value;
}
export function withoutIdempotencyTransportFields(payload: Record<string, unknown>): Record<string, unknown> { const { idempotencyKey: _key, confirm: _confirm, ...business } = payload; return business; }
function keyHash(key: string): string { return createHash("sha256").update(key).digest("hex"); }
type Row = { id: number; payload_hash: string; outcome_json: string | null; created_at: string; expires_at: string };

/** A real core rejection is a normal result, never a durable receipt. */
export function isBusinessRejection(value: unknown): value is BusinessRejection {
  return typeof value === "object" && value !== null && (value as { ok?: unknown }).ok === false;
}
class RollbackBusinessRejection<T> extends Error { constructor(readonly result: T) { super("rollback business rejection"); } }
function ledgerUuid(db: Database): string {
  const row = db.query("SELECT ledger_uuid FROM ledger_identity WHERE id = 1").get() as { ledger_uuid?: string } | null;
  if (!row?.ledger_uuid) throw new IdempotencyError("IDEMPOTENCY_STORAGE_FAILURE", "ledger identity is unavailable");
  return row.ledger_uuid;
}

/**
 * The executor owns one `BEGIN IMMEDIATE`: business write, immutable
 * tombstone, replay outcome and audit. Paths, actor and credentials are not
 * receipt identity; the ledger UUID + stable authenticated principal are.
 */
export function executeLocalIdempotentMutation<T>(db: Database, input: {
  key?: string; operation: keyof typeof RETRY_CLASS_BY_OPERATION;
  /** Retained only for compatible callers; never used as receipt identity. */
  workspaceScope?: string; companyScope?: string;
  principal?: StablePrincipal; payload: Record<string, unknown>; actor: { createdBy: string; createdByProgram: string }; now?: Date; execute: () => T;
}): { result: T; receipt?: IdempotencyReceipt } {
  if (!input.key) return { result: input.execute() };
  if (RETRY_CLASS_BY_OPERATION[input.operation] !== "key-idempotent") throw new IdempotencyError("IDEMPOTENCY_STORAGE_FAILURE", `operation ${input.operation} does not accept a client idempotency key`);
  if (!input.principal?.subjectId) throw new IdempotencyError("IDEMPOTENCY_AUTH_REQUIRED", "idempotency keys require an authenticated user or workspace service principal");
  const payloadHash = canonicalPayloadHash(input.payload); const now = input.now ?? new Date(); const createdAt = now.toISOString(); const expiresAt = new Date(now.getTime() + IDEMPOTENCY_RETENTION_DAYS * 86_400_000).toISOString();
  let rejectedAudit: { eventType: "idempotency_conflict" | "idempotency_outcome_expired"; entityId: number; message: string } | undefined;
  try {
    return db.transaction(() => {
      const ledger = ledgerUuid(db);
      const prior = db.query(`SELECT t.id, t.payload_hash, o.outcome_json, t.created_at, t.expires_at
        FROM mutation_idempotency_tombstones t
        LEFT JOIN mutation_idempotency_outcomes o ON o.tombstone_id = t.id
        WHERE t.client_key_hash = ? AND t.operation = ? AND t.ledger_uuid = ? AND t.principal_kind = ? AND t.principal_subject_id = ?`).get(keyHash(input.key!), input.operation, ledger, input.principal!.kind, input.principal!.subjectId) as Row | null;
      if (prior) {
        if (prior.payload_hash !== payloadHash) { rejectedAudit = { eventType: "idempotency_conflict", entityId: prior.id, message: `Rejected conflicting idempotency retry for ${input.operation}` }; throw new IdempotencyError("IDEMPOTENCY_CONFLICT", "idempotency key was already used with a different validated payload"); }
        if (prior.outcome_json === null) { rejectedAudit = { eventType: "idempotency_outcome_expired", entityId: prior.id, message: `Idempotency outcome expired for ${input.operation}; key remains reserved` }; throw new IdempotencyError("IDEMPOTENCY_OUTCOME_EXPIRED", "idempotency outcome has expired; inspect canonical state and use a new key only for a new operation"); }
        insertAuditLog(db, { eventType: "idempotency_replay", entityType: "idempotency_receipt", entityId: prior.id, message: `Replayed durable idempotency outcome for ${input.operation}`, ...input.actor });
        return { result: JSON.parse(prior.outcome_json) as T, receipt: { replayed: true, receiptId: prior.id, createdAt: prior.created_at, expiresAt: prior.expires_at } };
      }
      const result = input.execute();
      // A result-shaped validation/domain rejection must leave no business,
      // receipt or audit writes and may be retried with the same key.
      if (isBusinessRejection(result)) throw new RollbackBusinessRejection(result);
      const outcome = JSON.stringify(result);
      if (Buffer.byteLength(outcome, "utf8") > IDEMPOTENCY_PAYLOAD_MAX_BYTES) throw new IdempotencyError("IDEMPOTENCY_STORAGE_FAILURE", "mutation outcome exceeds receipt limit");
      const receiptId = Number(db.query(`INSERT INTO mutation_idempotency_tombstones (client_key_hash,ledger_uuid,operation,principal_kind,principal_subject_id,payload_hash,created_at,expires_at) VALUES (?,?,?,?,?,?,?,?)`).run(keyHash(input.key!), ledger, input.operation, input.principal!.kind, input.principal!.subjectId, payloadHash, createdAt, expiresAt).lastInsertRowid);
      db.query("INSERT INTO mutation_idempotency_outcomes(tombstone_id,outcome_json,created_at) VALUES (?,?,?)").run(receiptId, outcome, createdAt);
      insertAuditLog(db, { eventType: "idempotency_original", entityType: "idempotency_receipt", entityId: receiptId, message: `Recorded idempotent ${input.operation} outcome`, ...input.actor });
      return { result, receipt: { replayed: false, receiptId, createdAt, expiresAt } };
    }).immediate();
  } catch (error) {
    if (error instanceof RollbackBusinessRejection) return { result: error.result };
    if (rejectedAudit) insertAuditLog(db, { ...rejectedAudit, entityType: "idempotency_receipt", ...input.actor });
    throw error;
  }
}
/** Retention deletes replay material only; the immutable key tombstone remains. */
export function pruneExpiredIdempotencyOutcomes(db: Database, now = new Date()): number {
  return db.query(`DELETE FROM mutation_idempotency_outcomes WHERE tombstone_id IN (
    SELECT id FROM mutation_idempotency_tombstones WHERE expires_at <= ?
  )`).run(now.toISOString()).changes;
}
