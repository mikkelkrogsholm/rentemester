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
type Row = { case_id:string; version:number; source_kind:PurchaseCaseSource["kind"]; source_id:number; source_fingerprint:string; documentation_outcome:DocumentationOutcome; note:string; event_hash:string; created_at:string };

const caseId = /^[a-z][a-z0-9-]{2,63}$/;
const sha = (value: unknown) => createHash("sha256").update(canonicalJson(value)).digest("hex");
const failure = (...errors: string[]): PurchaseCaseResult => ({ ok:false, errors });
const sourceValid = (source: PurchaseCaseSource) => Number.isInteger(source.id) && source.id > 0;

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
  if (source.kind === "document") row = db.query("SELECT id FROM journal_entries WHERE document_id=? AND status='posted' ORDER BY id DESC LIMIT 1").get(source.id) as {id:number}|null;
  if (source.kind === "payable") row = db.query("SELECT journal_entry_id AS id FROM payables WHERE id=?").get(source.id) as {id:number}|null;
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
  return { caseId:row.case_id, version:row.version, source, sourceFingerprint:row.source_fingerprint, documentationOutcome:row.documentation_outcome, accountingProgress:booking(db,source) ? "posted" : "unposted", ...(booking(db,source) ? {canonicalBooking:booking(db,source)} : {}), vatEvidence:vatEvidence(db,source), note:row.note, eventHash:row.event_hash, createdAt:row.created_at };
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
export function getPurchaseCase(db: Database, id: string): PurchaseCase | null { const row=current(db,id); return row ? fromRow(db,row) : null; }
export function listPurchaseCases(db: Database): PurchaseCase[] { return (db.query("SELECT case_id,version,source_kind,source_id,source_fingerprint,documentation_outcome,note,event_hash,created_at FROM current_purchase_cases ORDER BY id DESC").all() as Row[]).map(row=>fromRow(db,row)); }
