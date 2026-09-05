import { createHash, randomUUID } from "node:crypto";
import type { Database } from "bun:sqlite";
import { canonicalJson } from "./canonical-json";
import { insertAuditLog, type ActorContext } from "./actor";

export type PurchaseCaseSource = { kind: "document"; id: number } | { kind: "bank_transaction"; id: number } | { kind: "payable"; id: number };
export type DocumentationOutcome = "unresolved" | "ordinary_evidence_sufficient" | "alternative_evidence_assessed";
export type AccountingProgress = "unposted" | "posted";
export type PurchaseCase = {
  caseId: string;
  version: number;
  source: PurchaseCaseSource;
  sourceFingerprint: string;
  documentationOutcome: DocumentationOutcome;
  accountingProgress: AccountingProgress;
  canonicalBooking?: { kind: "journal"; id: number };
  vatEvidence: { status: "not_applicable" | "pending" | "passed" | "failed"; reference?: number };
  note: string;
  eventHash: string;
  createdAt: string;
};
export type PurchaseCaseResult = { ok: true; purchaseCase: PurchaseCase } | { ok: false; errors: string[] };
export type PurchaseCaseNeed = { key: "source:stale" | "documentation:unresolved" | "documentation:alternative_evidence_assessed" | "booking:unposted"; question: string };
export type PurchaseCaseGroupMember = { caseId: string; expectedVersion: number; expectedSourceFingerprint: string };
export type PurchaseCaseGroupResult = { ok: true; group: { groupId: string; selectionHash: string; need: PurchaseCaseNeed; caseIds: string[]; eventHash: string }; purchaseCases: PurchaseCase[] } | { ok: false; errors: string[] };
type Row = { case_id:string; version:number; source_kind:PurchaseCaseSource["kind"]; source_id:number; source_fingerprint:string; documentation_outcome:DocumentationOutcome; note:string; event_hash:string; created_at:string };

const caseId = /^[a-z][a-z0-9-]{2,63}$/;
const sha = (value: unknown) => createHash("sha256").update(canonicalJson(value)).digest("hex");
const failure = (...errors: string[]): PurchaseCaseResult => ({ ok:false, errors });
const sourceValid = (source: PurchaseCaseSource) =>
  (source.kind === "document" || source.kind === "bank_transaction" || source.kind === "payable") &&
  Number.isInteger(source.id) && source.id > 0;

/** Canonical, read-only source fingerprint. It is deliberately independent of
 * a case payload, so a source mutation makes an earlier review stale. */
export function purchaseCaseSourceFingerprint(db: Database, source: PurchaseCaseSource): string | null {
  if (!sourceValid(source)) return null;
  if (source.kind === "document") {
    const row = db.query("SELECT id,sha256_hash,payload_json,document_type,invoice_date,currency,amount_inc_vat,vat_amount FROM documents WHERE id=?").get(source.id);
    return row ? sha({ source, row }) : null;
  }
  if (source.kind === "bank_transaction") {
    const row = db.query("SELECT id,transaction_hash,transaction_date,amount,currency,text FROM bank_transactions WHERE id=?").get(source.id);
    return row ? sha({ source, row }) : null;
  }
  const row = db.query("SELECT id,document_id,journal_entry_id,bill_date,due_date,gross_amount,net_amount,vat_amount,currency FROM payables WHERE id=?").get(source.id);
  return row ? sha({ source, row }) : null;
}

function booking(db: Database, source: PurchaseCaseSource): PurchaseCase["canonicalBooking"] {
  let row: { id:number } | null = null;
  if (source.kind === "bank_transaction") row = db.query("SELECT journal_entry_id AS id FROM bank_journal_reconciliations WHERE bank_transaction_id=? LIMIT 1").get(source.id) as {id:number}|null;
  // A case observes the existing ledger; it must never make a historic,
  // reversed journal look like the current canonical booking.
  if (source.kind === "document") row = db.query(`SELECT entry.id FROM journal_entries entry
    WHERE entry.document_id=? AND entry.status='posted' AND entry.reversal_of_entry_id IS NULL
      AND NOT EXISTS (SELECT 1 FROM journal_entries reversal WHERE reversal.reversal_of_entry_id=entry.id)
    ORDER BY entry.id DESC LIMIT 1`).get(source.id) as {id:number}|null;
  if (source.kind === "payable") row = db.query(`SELECT entry.id FROM payables payable
    JOIN journal_entries entry ON entry.id=payable.journal_entry_id
    WHERE payable.id=? AND entry.status='posted' AND entry.reversal_of_entry_id IS NULL
      AND NOT EXISTS (SELECT 1 FROM journal_entries reversal WHERE reversal.reversal_of_entry_id=entry.id)
    LIMIT 1`).get(source.id) as {id:number}|null;
  return row ? { kind:"journal", id:row.id } : undefined;
}

function vatEvidence(db: Database, source: PurchaseCaseSource): PurchaseCase["vatEvidence"] {
  const documentId = source.kind === "document" ? source.id : source.kind === "payable" ? (db.query("SELECT document_id FROM payables WHERE id=?").get(source.id) as {document_id:number}|null)?.document_id : undefined;
  if (!documentId) return { status:"not_applicable" };
  const event = db.query("SELECT id,event_type FROM vat_validation_events WHERE document_id=? AND event_type IN ('preflight_passed','preflight_failed') ORDER BY id DESC LIMIT 1").get(documentId) as {id:number;event_type:string}|null;
  return !event ? { status:"pending" } : { status:event.event_type === "preflight_passed" ? "passed" : "failed", reference:event.id };
}

function fromRow(db: Database, row: Row): PurchaseCase {
  const source = { kind:row.source_kind, id:row.source_id } as PurchaseCaseSource;
  const canonicalBooking = booking(db, source);
  return { caseId:row.case_id, version:row.version, source, sourceFingerprint:row.source_fingerprint, documentationOutcome:row.documentation_outcome, accountingProgress:canonicalBooking ? "posted" : "unposted", ...(canonicalBooking ? {canonicalBooking} : {}), vatEvidence:vatEvidence(db,source), note:row.note, eventHash:row.event_hash, createdAt:row.created_at };
}
export function purchaseCaseNeed(db: Database, purchaseCase: PurchaseCase): PurchaseCaseNeed | null {
  const currentFingerprint = purchaseCaseSourceFingerprint(db, purchaseCase.source);
  if (currentFingerprint !== purchaseCase.sourceFingerprint) return { key: "source:stale", question: "The source changed; inspect its current evidence before review." };
  if (purchaseCase.documentationOutcome === "unresolved") return { key: "documentation:unresolved", question: "Review whether the documented evidence is sufficient." };
  if (purchaseCase.documentationOutcome === "alternative_evidence_assessed") return { key: "documentation:alternative_evidence_assessed", question: "Review the alternative evidence decision before further accounting." };
  if (purchaseCase.accountingProgress === "unposted") return { key: "booking:unposted", question: "Continue through the existing canonical booking flow." };
  return null;
}
function current(db: Database, id: string): Row | null { return db.query("SELECT case_id,version,source_kind,source_id,source_fingerprint,documentation_outcome,note,event_hash,created_at FROM current_purchase_cases WHERE case_id=?").get(id) as Row|null; }
function record(db: Database, input: { caseId:string; version:number; type:"created"|"reviewed"; source:PurchaseCaseSource; fingerprint:string; outcome:DocumentationOutcome; note:string; prior?:string; actor:ActorContext }) {
  const createdAt=new Date().toISOString();
  const eventHash=sha({caseId:input.caseId,version:input.version,type:input.type,source:input.source,sourceFingerprint:input.fingerprint,documentationOutcome:input.outcome,note:input.note,priorEventHash:input.prior??null,actor:input.actor.createdBy,program:input.actor.createdByProgram,createdAt});
  db.query("INSERT INTO purchase_case_events(case_id,version,event_type,source_kind,source_id,source_fingerprint,documentation_outcome,note,prior_event_hash,event_hash,actor,program,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)").run(input.caseId,input.version,input.type,input.source.kind,input.source.id,input.fingerprint,input.outcome,input.note,input.prior??null,eventHash,input.actor.createdBy,input.actor.createdByProgram,createdAt);
  insertAuditLog(db,{eventType:`purchase_case_${input.type}`,entityType:"purchase_case",entityId:input.caseId,message:`Recorded purchase case version ${input.version}`,createdBy:input.actor.createdBy,createdByProgram:input.actor.createdByProgram});
}
export function createPurchaseCase(db: Database, input: { caseId?:string; source:PurchaseCaseSource; documentationOutcome?:DocumentationOutcome; note?:string; actor:ActorContext }): PurchaseCaseResult {
  const id=input.caseId ?? `purchase-${randomUUID()}`; if(!caseId.test(id)) return failure("PURCHASE_CASE_ID_INVALID"); if(!sourceValid(input.source)) return failure("PURCHASE_CASE_SOURCE_INVALID");
  const fingerprint=purchaseCaseSourceFingerprint(db,input.source); if(!fingerprint) return failure("PURCHASE_CASE_SOURCE_NOT_FOUND");
  const outcome=input.documentationOutcome??"unresolved"; const note=input.note??""; if(note.length>2000) return failure("PURCHASE_CASE_NOTE_TOO_LONG");
  try { return db.transaction(()=>{ if(current(db,id)) return failure("PURCHASE_CASE_EXISTS"); const existing=db.query("SELECT case_id FROM purchase_case_events WHERE source_kind=? AND source_id=? AND event_type='created'").get(input.source.kind,input.source.id) as {case_id:string}|null; if(existing)return failure("PURCHASE_CASE_SOURCE_ALREADY_HAS_CASE"); record(db,{caseId:id,version:1,type:"created",source:input.source,fingerprint,outcome,note,actor:input.actor}); return {ok:true as const,purchaseCase:fromRow(db,current(db,id)!)}; }).immediate(); } catch { return failure("PURCHASE_CASE_WRITE_REJECTED"); }
}
export function reviewPurchaseCase(db: Database, input: { caseId:string; expectedVersion:number; expectedSourceFingerprint:string; documentationOutcome:DocumentationOutcome; note?:string; actor:ActorContext }): PurchaseCaseResult {
  const note=input.note??""; if(!Number.isInteger(input.expectedVersion)||note.length>2000)return failure("PURCHASE_CASE_REVIEW_INVALID");
  return db.transaction(()=>{ const row=current(db,input.caseId); if(!row)return failure("PURCHASE_CASE_NOT_FOUND"); if(row.version!==input.expectedVersion)return failure("STALE_PURCHASE_CASE_VERSION"); const source={kind:row.source_kind,id:row.source_id} as PurchaseCaseSource; const actual=purchaseCaseSourceFingerprint(db,source); if(!actual)return failure("PURCHASE_CASE_SOURCE_NOT_FOUND"); if(actual!==input.expectedSourceFingerprint || actual!==row.source_fingerprint)return failure("STALE_PURCHASE_CASE_SOURCE"); record(db,{caseId:row.case_id,version:row.version+1,type:"reviewed",source,fingerprint:actual,outcome:input.documentationOutcome,note,prior:row.event_hash,actor:input.actor}); return {ok:true as const,purchaseCase:fromRow(db,current(db,input.caseId)!)}; }).immediate();
}
/** Atomically records one documented shared review and one ordinary case review
 * per exact selected case. It deliberately cannot post, alter VAT, or resolve
 * a booking need: those remain existing canonical flows. */
export function reviewPurchaseCaseGroup(db: Database, input: { groupId?: string; members: PurchaseCaseGroupMember[]; documentationOutcome: DocumentationOutcome; note?: string; actor: ActorContext }): PurchaseCaseGroupResult {
  const groupId = input.groupId ?? `purchase-group-${randomUUID()}`;
  const note = input.note ?? "";
  if (!caseId.test(groupId) || input.members.length === 0 || input.members.length > 100 || note.length > 2000) return { ok: false, errors: ["PURCHASE_CASE_GROUP_INVALID"] };
  const ids = new Set<string>();
  if (input.members.some(member => !caseId.test(member.caseId) || !Number.isInteger(member.expectedVersion) || member.expectedVersion < 1 || !/^[a-f0-9]{64}$/.test(member.expectedSourceFingerprint) || ids.has(member.caseId) || !ids.add(member.caseId))) return { ok: false, errors: ["PURCHASE_CASE_GROUP_MEMBERS_INVALID"] };
  return db.transaction(() => {
    const prepared: Array<{ row: Row; source: PurchaseCaseSource; fingerprint: string; purchaseCase: PurchaseCase; need: PurchaseCaseNeed }> = [];
    for (const member of input.members) {
      const row = current(db, member.caseId);
      if (!row) return { ok: false as const, errors: ["PURCHASE_CASE_NOT_FOUND"] };
      if (row.version !== member.expectedVersion) return { ok: false as const, errors: ["STALE_PURCHASE_CASE_VERSION"] };
      const source = { kind: row.source_kind, id: row.source_id } as PurchaseCaseSource;
      const fingerprint = purchaseCaseSourceFingerprint(db, source);
      if (!fingerprint || fingerprint !== member.expectedSourceFingerprint || fingerprint !== row.source_fingerprint) return { ok: false as const, errors: ["STALE_PURCHASE_CASE_SOURCE"] };
      const purchaseCase = fromRow(db, row);
      const need = purchaseCaseNeed(db, purchaseCase);
      if (!need || need.key !== "documentation:unresolved") return { ok: false as const, errors: ["PURCHASE_CASE_GROUP_NEED_INCOMPATIBLE"] };
      prepared.push({ row, source, fingerprint, purchaseCase, need });
    }
    const needs = new Set(prepared.map(item => item.need.key));
    if (needs.size !== 1) return { ok: false as const, errors: ["PURCHASE_CASE_GROUP_NEED_INCOMPATIBLE"] };
    const selection = prepared.map(item => ({ caseId: item.row.case_id, version: item.row.version, sourceFingerprint: item.fingerprint })).sort((a, b) => a.caseId.localeCompare(b.caseId));
    const selectionHash = sha({ needKey: "documentation:unresolved", selection, documentationOutcome: input.documentationOutcome, note });
    const createdAt = new Date().toISOString();
    const eventHash = sha({ groupId, selectionHash, actor: input.actor.createdBy, program: input.actor.createdByProgram, createdAt });
    try {
      const groupEventId = Number(db.query("INSERT INTO purchase_case_group_events(group_id,need_key,selection_hash,documentation_outcome,note,event_hash,actor,program,created_at) VALUES(?,?,?,?,?,?,?,?,?)").run(groupId, "documentation:unresolved", selectionHash, input.documentationOutcome, note, eventHash, input.actor.createdBy, input.actor.createdByProgram, createdAt).lastInsertRowid);
      const reviewed = prepared.map(item => {
        record(db, { caseId: item.row.case_id, version: item.row.version + 1, type: "reviewed", source: item.source, fingerprint: item.fingerprint, outcome: input.documentationOutcome, note, prior: item.row.event_hash, actor: input.actor });
        const updated = fromRow(db, current(db, item.row.case_id)!);
        db.query("INSERT INTO purchase_case_group_members(group_event_id,case_id,case_version,source_fingerprint,case_event_hash) VALUES(?,?,?,?,?)").run(groupEventId, item.row.case_id, item.row.version, item.fingerprint, updated.eventHash);
        return updated;
      });
      insertAuditLog(db, { eventType: "purchase_case_group_reviewed", entityType: "purchase_case_group", entityId: groupId, message: `Reviewed ${reviewed.length} purchase cases for documentation evidence`, createdBy: input.actor.createdBy, createdByProgram: input.actor.createdByProgram });
      return { ok: true as const, group: { groupId, selectionHash, need: prepared[0]!.need, caseIds: selection.map(item => item.caseId), eventHash }, purchaseCases: reviewed };
    } catch { return { ok: false as const, errors: ["PURCHASE_CASE_GROUP_WRITE_REJECTED"] }; }
  }).immediate();
}
export function getPurchaseCase(db: Database, id: string): PurchaseCase | null { const row=current(db,id); return row ? fromRow(db,row) : null; }
export function listPurchaseCases(db: Database): PurchaseCase[] { return (db.query("SELECT case_id,version,source_kind,source_id,source_fingerprint,documentation_outcome,note,event_hash,created_at FROM current_purchase_cases ORDER BY id DESC").all() as Row[]).map(row=>fromRow(db,row)); }
