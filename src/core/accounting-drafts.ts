/**
 * Generic four-eyes accounting drafts. Draft evidence is append-only; only an
 * independently reviewed, exact submitted version can reach the ledger, and
 * the journal post plus approval evidence commit in one SQLite transaction.
 */
import type { Database } from "bun:sqlite";
import { createHash } from "node:crypto";
import { insertAuditLog, resolveActor, type ResolveActorInput } from "./actor";
import { evaluateAccountingApproval, getAccountingApprovalPolicy } from "./accounting-approval-policy";
import { asJournalEntryId } from "./ids";
import { postJournalEntryInCurrentTransaction, validateJournalEntry, type JournalEntryInput, type JournalPostResult } from "./ledger";

const DRAFT_ID = /^[a-z][a-z0-9-]{0,63}$/;
const SHA256 = /^[0-9a-f]{64}$/;
const DRAFT_PROGRAM = "rentemester-accounting-draft";

type DraftEventType = "created" | "revised" | "submitted" | "rejected" | "approved_posted";
type DraftEventRow = {
  id: number;
  draft_id: string;
  version: number;
  event_type: DraftEventType;
  payload_hash: string;
  canonical_payload: string;
  reason: string | null;
  journal_entry_id: number | null;
  actor_id: string;
  actor_program: string;
  principal_id: string | null;
  approval_policy_hash: string | null;
  previous_hash: string | null;
  event_hash: string;
  created_at: string;
};

export type AccountingDraftState = {
  id: string;
  version: number;
  status: DraftEventType;
  payloadHash: string;
  eventHash: string;
  payload: Omit<JournalEntryInput, "createdBy" | "createdByProgram">;
  actorId: string;
  principalId?: string;
  approvalPolicyHash?: string;
  reason?: string;
  journalEntryId?: number;
};

/** Principal evidence is separate from actor attribution. */
export type AccountingDraftMutationContext = { principalId: string };
export type AccountingDraftApprovalContext = AccountingDraftMutationContext & {
  controlDb: Database;
  workspaceRoot: string;
  companySlug: string;
  expectedPolicyEventHash?: string | null;
};

function sha256(value: string): string {
  return createHash("sha256").update(value).digest("hex");
}

function assertDraftId(value: string): string {
  const normalized = value.trim().normalize("NFC");
  if (!DRAFT_ID.test(normalized)) throw new Error("draft id must be a lowercase stable identifier");
  return normalized;
}

function canonicalPayload(input: JournalEntryInput): string {
  const payload: Omit<JournalEntryInput, "createdBy" | "createdByProgram"> = {
    transactionDate: input.transactionDate,
    text: input.text,
    ...(input.documentId == null ? {} : { documentId: input.documentId }),
    ...(input.sourceBankTransactionId == null ? {} : { sourceBankTransactionId: input.sourceBankTransactionId }),
    ...(input.currency == null ? {} : { currency: input.currency }),
    ...(input.amountForeign == null ? {} : { amountForeign: input.amountForeign }),
    ...(input.amountDkk == null ? {} : { amountDkk: input.amountDkk }),
    ...(input.fxRateToDkk == null ? {} : { fxRateToDkk: input.fxRateToDkk }),
    lines: input.lines.map((line) => ({
      accountNo: line.accountNo,
      ...(line.debitAmount == null ? {} : { debitAmount: line.debitAmount }),
      ...(line.creditAmount == null ? {} : { creditAmount: line.creditAmount }),
      ...(line.vatCode == null ? {} : { vatCode: line.vatCode }),
      ...(line.text == null ? {} : { text: line.text }),
    })),
  };
  const canonical = JSON.stringify(payload);
  if (Buffer.byteLength(canonical, "utf8") > 262144) throw new Error("accounting draft payload exceeds 262144 bytes");
  return canonical;
}

function eventHash(previousHash: string | null, event: Omit<DraftEventRow, "event_hash">): string {
  return sha256(JSON.stringify({
    previousHash,
    id: event.id,
    draftId: event.draft_id,
    version: event.version,
    eventType: event.event_type,
    payloadHash: event.payload_hash,
    canonicalPayload: event.canonical_payload,
    reason: event.reason,
    journalEntryId: event.journal_entry_id,
    actorId: event.actor_id,
    actorProgram: event.actor_program,
    ...(event.principal_id == null ? {} : { principalId: event.principal_id }),
    ...(event.approval_policy_hash == null ? {} : { approvalPolicyHash: event.approval_policy_hash }),
    createdAt: event.created_at,
  }));
}

function readEvents(db: Database): DraftEventRow[] {
  const rows = db.query(
    `SELECT id,draft_id,version,event_type,payload_hash,canonical_payload,reason,journal_entry_id,
            actor_id,actor_program,principal_id,approval_policy_hash,previous_hash,event_hash,created_at
       FROM accounting_draft_events ORDER BY id`,
  ).all() as DraftEventRow[];
  let previous: string | null = null;
  for (const row of rows) {
    if (row.previous_hash !== previous || row.event_hash !== eventHash(previous, row)) {
      throw new Error("accounting draft event hash-chain is invalid");
    }
    if (!SHA256.test(row.payload_hash) || sha256(row.canonical_payload) !== row.payload_hash) {
      throw new Error("accounting draft payload evidence is invalid");
    }
    previous = row.event_hash;
  }
  return rows;
}

function draftEvents(db: Database, draftId: string): DraftEventRow[] {
  return readEvents(db).filter((event) => event.draft_id === draftId);
}

function currentEvent(db: Database, draftId: string): DraftEventRow | null {
  return draftEvents(db, draftId).at(-1) ?? null;
}

function stateFromEvent(event: DraftEventRow): AccountingDraftState {
  return {
    id: event.draft_id,
    version: event.version,
    status: event.event_type,
    payloadHash: event.payload_hash,
    eventHash: event.event_hash,
    payload: JSON.parse(event.canonical_payload) as AccountingDraftState["payload"],
    actorId: event.actor_id,
    ...(event.principal_id == null ? {} : { principalId: event.principal_id }),
    ...(event.approval_policy_hash == null ? {} : { approvalPolicyHash: event.approval_policy_hash }),
    ...(event.reason == null ? {} : { reason: event.reason }),
    ...(event.journal_entry_id == null ? {} : { journalEntryId: event.journal_entry_id }),
  };
}

function appendEvent(
  db: Database,
  input: { draftId: string; version: number; type: DraftEventType; canonical: string; reason?: string; journalEntryId?: number; principalId?: string; approvalPolicyHash?: string | null },
  audit: ResolveActorInput,
): DraftEventRow {
  const events = readEvents(db);
  const actor = resolveActor(audit);
  const previousHash = events.at(-1)?.event_hash ?? null;
  const event = {
    id: (events.at(-1)?.id ?? 0) + 1,
    draft_id: input.draftId,
    version: input.version,
    event_type: input.type,
    payload_hash: sha256(input.canonical),
    canonical_payload: input.canonical,
    reason: input.reason ?? null,
    journal_entry_id: input.journalEntryId ?? null,
    actor_id: actor.createdBy,
    actor_program: actor.createdByProgram,
    principal_id: input.principalId ?? null,
    approval_policy_hash: input.approvalPolicyHash ?? null,
    previous_hash: previousHash,
    created_at: new Date().toISOString(),
  };
  const complete: DraftEventRow = { ...event, event_hash: eventHash(previousHash, event) };
  db.query(
    `INSERT INTO accounting_draft_events
       (id,draft_id,version,event_type,payload_hash,canonical_payload,reason,journal_entry_id,
        actor_id,actor_program,principal_id,approval_policy_hash,previous_hash,event_hash,created_at)
     VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)`,
  ).run(
    complete.id, complete.draft_id, complete.version, complete.event_type,
    complete.payload_hash, complete.canonical_payload, complete.reason,
    complete.journal_entry_id, complete.actor_id, complete.actor_program,
    complete.principal_id, complete.approval_policy_hash, complete.previous_hash, complete.event_hash, complete.created_at,
  );
  insertAuditLog(db, {
    ...audit,
    eventType: `accounting_draft_${input.type}`,
    entityType: "accounting_draft",
    entityId: input.draftId,
    message: `Accounting draft ${input.draftId} version ${input.version}: ${input.type}`,
  });
  return complete;
}

function validatePayload(db: Database, payload: JournalEntryInput): string {
  const validation = validateJournalEntry(db, payload);
  if (!validation.ok) throw new Error(`accounting draft is invalid: ${validation.errors.join("; ")}`);
  return canonicalPayload(payload);
}

function assertExactCurrent(current: DraftEventRow | null, expectedEventHash: string, expectedStatus: DraftEventType): DraftEventRow {
  if (!current || current.event_type !== expectedStatus || current.event_hash !== expectedEventHash) {
    throw new Error(`exact ${expectedStatus} accounting draft was not found`);
  }
  return current;
}

function principalId(context: AccountingDraftMutationContext | undefined): string | null {
  const value = context?.principalId?.trim().normalize("NFC") ?? "";
  if (value.length > 160) throw new Error("draft principal id is bounded");
  return value || null;
}

function versionAuthor(db: Database, submitted: DraftEventRow): DraftEventRow | null {
  return [...draftEvents(db, submitted.draft_id)].reverse().find(
    (event) => event.version === submitted.version && (event.event_type === "created" || event.event_type === "revised"),
  ) ?? null;
}

export function createAccountingDraft(db: Database, draftIdInput: string, payload: JournalEntryInput, audit: ResolveActorInput, context?: AccountingDraftMutationContext): AccountingDraftState {
  return db.transaction(() => {
    const draftId = assertDraftId(draftIdInput);
    if (currentEvent(db, draftId)) throw new Error("accounting draft id already exists");
    const event = appendEvent(db, { draftId, version: 1, type: "created", canonical: validatePayload(db, payload), principalId: principalId(context) ?? undefined }, audit);
    return stateFromEvent(event);
  }).immediate();
}

export function reviseAccountingDraft(db: Database, draftIdInput: string, expectedEventHash: string, payload: JournalEntryInput, audit: ResolveActorInput, context?: AccountingDraftMutationContext): AccountingDraftState {
  return db.transaction(() => {
    const draftId = assertDraftId(draftIdInput);
    const current = currentEvent(db, draftId);
    if (!current || !(["created", "revised", "rejected"] as DraftEventType[]).includes(current.event_type) || current.event_hash !== expectedEventHash) {
      throw new Error("exact editable accounting draft was not found");
    }
    const event = appendEvent(db, { draftId, version: current.version + 1, type: "revised", canonical: validatePayload(db, payload), principalId: principalId(context) ?? undefined }, audit);
    return stateFromEvent(event);
  }).immediate();
}

export function submitAccountingDraft(db: Database, draftIdInput: string, expectedEventHash: string, audit: ResolveActorInput, context?: AccountingDraftMutationContext): AccountingDraftState {
  return db.transaction(() => {
    const draftId = assertDraftId(draftIdInput);
    const current = currentEvent(db, draftId);
    if (!current || !(["created", "revised"] as DraftEventType[]).includes(current.event_type) || current.event_hash !== expectedEventHash) {
      throw new Error("exact editable accounting draft was not found");
    }
    // Revalidation makes a draft fail closed if account, evidence or period
    // preconditions changed between editing and submission.
    validatePayload(db, JSON.parse(current.canonical_payload) as JournalEntryInput);
    const author = versionAuthor(db, current);
    if (!author) throw new Error("accounting draft author evidence is missing");
    const submitterPrincipal = principalId(context);
    return stateFromEvent(appendEvent(db, { draftId, version: current.version, type: "submitted", canonical: current.canonical_payload, principalId: submitterPrincipal ?? undefined }, audit));
  }).immediate();
}

function assertIndependentReviewer(db: Database, submitted: DraftEventRow, audit: ResolveActorInput, context?: AccountingDraftMutationContext): DraftEventRow {
  const reviewer = resolveActor(audit).createdBy;
  const author = versionAuthor(db, submitted);
  if (!author || reviewer === submitted.actor_id || reviewer === author.actor_id) {
    throw new Error("accounting draft review requires an actor distinct from author and submitter");
  }
  const reviewerPrincipal = principalId(context);
  if ((author.principal_id != null || submitted.principal_id != null) &&
    (!reviewerPrincipal || reviewerPrincipal === author.principal_id || reviewerPrincipal === submitted.principal_id)) {
    throw new Error("accounting draft review requires a principal distinct from author and submitter");
  }
  return author;
}

function evaluateReviewPolicy(db: Database, submitted: DraftEventRow, audit: ResolveActorInput, context: AccountingDraftApprovalContext | undefined): { principalId: string | null; policyHash: string | null } {
  const author = assertIndependentReviewer(db, submitted, audit, context);
  const reviewerPrincipal = principalId(context);
  if (!context) return { principalId: reviewerPrincipal, policyHash: null };
  if (!reviewerPrincipal || !author.principal_id || !submitted.principal_id) {
    throw new Error("accounting draft policy review requires author, submitter and reviewer principals");
  }
  const active = getAccountingApprovalPolicy(context.controlDb, context.companySlug);
  if (active && context.expectedPolicyEventHash !== active.eventHash) throw new Error("STALE_APPROVAL_POLICY");
  const decision = evaluateAccountingApproval(context.controlDb, context.workspaceRoot, {
    companySlug: context.companySlug,
    action: "accounting_draft_review",
    principalId: reviewerPrincipal,
    proposedByPrincipalId: submitted.principal_id,
  });
  if (!decision.allowed) throw new Error(decision.code);
  return { principalId: reviewerPrincipal, policyHash: decision.policy?.eventHash ?? null };
}

export function rejectAccountingDraft(db: Database, draftIdInput: string, expectedEventHash: string, reasonInput: string, audit: ResolveActorInput, context?: AccountingDraftApprovalContext): AccountingDraftState {
  return db.transaction(() => {
    const draftId = assertDraftId(draftIdInput);
    const existing = currentEvent(db, draftId);
    const submitted = assertExactCurrent(existing, expectedEventHash, "submitted");
    const review = evaluateReviewPolicy(db, submitted, audit, context);
    const reason = reasonInput.trim().normalize("NFC");
    if (!reason || reason.length > 1000) throw new Error("rejection reason must contain 1 through 1000 characters");
    return stateFromEvent(appendEvent(db, { draftId, version: submitted.version, type: "rejected", canonical: submitted.canonical_payload, reason, principalId: review.principalId ?? undefined, approvalPolicyHash: review.policyHash }, audit));
  }).immediate();
}

export function approveAndPostAccountingDraft(
  db: Database,
  draftIdInput: string,
  expectedEventHash: string,
  audit: ResolveActorInput,
  context?: AccountingDraftApprovalContext,
): AccountingDraftState & { journal: JournalPostResult } {
  return db.transaction(() => {
    const draftId = assertDraftId(draftIdInput);
    const existing = currentEvent(db, draftId);
    if (existing?.event_type === "approved_posted") {
      const submittedEvidence = draftEvents(db, draftId).find(
        (event) => event.event_hash === expectedEventHash && event.event_type === "submitted",
      );
      if (!submittedEvidence || existing.journal_entry_id == null) {
        throw new Error("exact submitted accounting draft was not found");
      }
      const journalRow = db.query(
        "SELECT id,entry_no,entry_hash FROM journal_entries WHERE id = ?",
      ).get(existing.journal_entry_id) as { id: number; entry_no: string; entry_hash: string } | null;
      if (!journalRow) throw new Error("posted accounting draft has missing journal evidence");
      return {
        ...stateFromEvent(existing),
        journal: {
          ok: true,
          entryId: asJournalEntryId(journalRow.id),
          entryNo: journalRow.entry_no,
          entryHash: journalRow.entry_hash,
          appliedRules: [],
          errors: [],
        },
      };
    }
    const submitted = assertExactCurrent(existing, expectedEventHash, "submitted");
    const review = evaluateReviewPolicy(db, submitted, audit, context);
    const actor = resolveActor(audit);
    const payload = JSON.parse(submitted.canonical_payload) as JournalEntryInput;
    const journal = postJournalEntryInCurrentTransaction(db, { ...payload, createdBy: actor.createdBy, createdByProgram: DRAFT_PROGRAM });
    if (!journal.ok || journal.entryId == null) throw new Error(`accounting draft could not be posted: ${journal.errors.join("; ")}`);
    const event = appendEvent(db, { draftId, version: submitted.version, type: "approved_posted", canonical: submitted.canonical_payload, journalEntryId: Number(journal.entryId), principalId: review.principalId ?? undefined, approvalPolicyHash: review.policyHash }, audit);
    return { ...stateFromEvent(event), journal };
  }).immediate();
}

export function getAccountingDraft(db: Database, draftIdInput: string): AccountingDraftState | null {
  const current = currentEvent(db, assertDraftId(draftIdInput));
  return current ? stateFromEvent(current) : null;
}

export function listAccountingDrafts(db: Database): AccountingDraftState[] {
  const latest = new Map<string, DraftEventRow>();
  for (const event of readEvents(db)) latest.set(event.draft_id, event);
  return [...latest.values()].map(stateFromEvent).sort((left, right) => left.id.localeCompare(right.id));
}
