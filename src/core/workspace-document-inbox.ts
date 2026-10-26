import { canonicalJson } from "./canonical-json";
/**
 * Workspace document inbox (#577).
 *
 * The control database owns ingress evidence and routing history. A company
 * ledger is opened only by `completeWorkspaceInboxAssignment`, after a caller
 * has already checked access to both the anchor and selected legal entity.
 * This intentionally keeps unassigned mail/uploads out of every ledger.
 */
import { createHash, randomUUID } from "node:crypto";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import type { Database } from "bun:sqlite";
import { ingestDocument, type DocumentMetadata } from "./documents";

export const WORKSPACE_INBOX_TRANSPORTS = ["upload", "mail", "connector"] as const;
export const WORKSPACE_INBOX_EVIDENCE_KINDS = ["recipient_alias", "buyer_vat", "buyer_name", "account", "portal"] as const;
type Transport = typeof WORKSPACE_INBOX_TRANSPORTS[number];
type EvidenceKind = typeof WORKSPACE_INBOX_EVIDENCE_KINDS[number];
type CandidateClaim = { companySlug: string; evidenceKind: EvidenceKind; evidence: string };

const canonical = (value: unknown): string => value === null || typeof value !== "object"
  ? JSON.stringify(value)
  : Array.isArray(value) ? `[${value.map(canonical).join(",")}]`
  : `{${Object.keys(value).sort().map(k => `${JSON.stringify(k)}:${canonical((value as Record<string, unknown>)[k])}`).join(",")}}`;
const digest = (value: string | Uint8Array) => createHash("sha256").update(value).digest("hex");
const text = (value: unknown, label: string, max = 512): string => {
  const result = typeof value === "string" ? value.trim() : "";
  if (!result || result.length > max || /[\0-\x1f\x7f]/.test(result)) throw new Error(`${label} is required and bounded`);
  return result;
};
const iso = (value: unknown, label: string): string => {
  const parsed = new Date(text(value, label, 64));
  if (Number.isNaN(parsed.valueOf())) throw new Error(`${label} must be ISO-8601`);
  return parsed.toISOString();
};
function metadata(input: unknown): Record<string, unknown> {
  if (!input || typeof input !== "object" || Array.isArray(input)) throw new Error("metadata must be an object");
  const encoded = canonical(input);
  if (encoded.length > 64 * 1024) throw new Error("metadata exceeds 64 KiB bound");
  return JSON.parse(encoded) as Record<string, unknown>;
}
function append(db: Database, sourceId: string, eventType: string, payload: unknown, actor: string, at: string): string {
  const body = canonical(payload), hash = digest(body);
  db.query("INSERT INTO rm_workspace_inbox_events(source_id,event_type,payload_hash,canonical_payload,actor,created_at) VALUES(?,?,?,?,?,?)")
    .run(sourceId, eventType, hash, body, text(actor, "actor", 160), at);
  return hash;
}
function claims(input: readonly CandidateClaim[], visibleCompanySlugs: ReadonlySet<string>): CandidateClaim[] {
  const unique = new Map<string, CandidateClaim>();
  for (const candidate of input) {
    const companySlug = text(candidate?.companySlug, "candidate companySlug", 100);
    if (!visibleCompanySlugs.has(companySlug)) continue; // filtering precedes all output/counting
    if (!(WORKSPACE_INBOX_EVIDENCE_KINDS as readonly string[]).includes(candidate?.evidenceKind)) throw new Error("candidate evidenceKind is invalid");
    const evidence = text(candidate?.evidence, "candidate evidence", 1000);
    const key = `${companySlug}\u0000${candidate.evidenceKind}\u0000${evidence}`;
    unique.set(key, { companySlug, evidenceKind: candidate.evidenceKind, evidence });
  }
  return [...unique.values()].sort((a, b) => a.companySlug.localeCompare(b.companySlug) || a.evidenceKind.localeCompare(b.evidenceKind) || a.evidence.localeCompare(b.evidence));
}

export type WorkspaceInboxSource = ReturnType<typeof inspectWorkspaceInboxSource>;
export function ingestWorkspaceInboxSource(db: Database, input: {
  sourceId?: string; visibilityAnchorSlug: string; idempotencyKey: string; bytes: Uint8Array; filename: string; mimeType: string;
  transport: Transport; transportIdentity?: string; receivedAt: string; metadata: Record<string, unknown>; candidates?: readonly CandidateClaim[];
  visibleCompanySlugs: ReadonlySet<string>; actor: string; at?: string;
}) {
  if (!(WORKSPACE_INBOX_TRANSPORTS as readonly string[]).includes(input.transport)) throw new Error("unsupported workspace inbox transport");
  if (!input.bytes.byteLength || input.bytes.byteLength > 25 * 1024 * 1024) throw new Error("workspace inbox source bytes are required and bounded");
  const anchor = text(input.visibilityAnchorSlug, "visibilityAnchorSlug", 100);
  const idempotencyKey = text(input.idempotencyKey, "idempotencyKey", 200);
  const at = input.at ? iso(input.at, "at") : new Date().toISOString();
  const sourceHash = digest(input.bytes);
  const existing = db.query("SELECT source_id,sha256 FROM rm_workspace_inbox_sources WHERE idempotency_key=?").get(idempotencyKey) as { source_id: string; sha256: string } | null;
  if (existing) {
    if (existing.sha256 !== sourceHash) throw new Error("idempotencyKey already belongs to different workspace inbox bytes");
    return inspectWorkspaceInboxSource(db, existing.source_id)!;
  }
  const sourceId = input.sourceId ? text(input.sourceId, "sourceId", 100) : `inbox-${randomUUID()}`;
  const candidateRows = claims(input.candidates ?? [], input.visibleCompanySlugs);
  const companies = [...new Set(candidateRows.map(x => x.companySlug))];
  const sourceMetadata = metadata(input.metadata);
  db.transaction(() => {
    db.query("INSERT INTO rm_workspace_inbox_sources(source_id,visibility_anchor_slug,idempotency_key,original_bytes,sha256,filename,mime_type,transport,transport_identity,received_at,metadata_json,created_by,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)")
      .run(sourceId, anchor, idempotencyKey, input.bytes, sourceHash, text(input.filename, "filename", 512), text(input.mimeType, "mimeType", 160), input.transport, input.transportIdentity ? text(input.transportIdentity, "transportIdentity", 512) : null, iso(input.receivedAt, "receivedAt"), canonical(sourceMetadata), text(input.actor, "actor", 160), at);
    append(db, sourceId, "ingested", { sourceHash, filename: input.filename, mimeType: input.mimeType, transport: input.transport, transportIdentity: input.transportIdentity ?? null }, input.actor, at);
    append(db, sourceId, "candidate_resolved", { candidates: candidateRows }, input.actor, at);
    if (companies.length !== 1) {
      const code = companies.length ? "INBOX_AMBIGUOUS_CANDIDATES" : "INBOX_NO_CANDIDATE";
      db.query("INSERT INTO rm_workspace_inbox_exceptions(source_id,code,required_action,opened_at) VALUES(?,?,?,?)").run(sourceId, code, "review_and_assign", at);
      append(db, sourceId, "exception_opened", { code, candidates: candidateRows, requiredAction: "review_and_assign" }, input.actor, at);
    }
  })();
  return inspectWorkspaceInboxSource(db, sourceId)!;
}

export function inspectWorkspaceInboxSource(db: Database, sourceId: string, visibilityAnchorSlug?: string, visibleCompanySlugs?: ReadonlySet<string>) {
  const row = db.query("SELECT source_id,visibility_anchor_slug,sha256,filename,mime_type,transport,transport_identity,received_at,metadata_json,created_by,created_at FROM rm_workspace_inbox_sources WHERE source_id=?").get(text(sourceId, "sourceId", 100)) as any;
  if (!row || (visibilityAnchorSlug && row.visibility_anchor_slug !== visibilityAnchorSlug) || (visibleCompanySlugs && !visibleCompanySlugs.has(row.visibility_anchor_slug))) return null;
  const events = db.query("SELECT event_type,payload_hash,actor,created_at,canonical_payload FROM rm_workspace_inbox_events WHERE source_id=? ORDER BY id").all(sourceId) as any[];
  const assignments = db.query("SELECT company_slug,state,document_id,document_no,assigned_by,assigned_at,completed_at FROM rm_workspace_inbox_assignments WHERE source_id=? ORDER BY company_slug").all(sourceId) as any[];
  const exception = db.query("SELECT code,required_action,opened_at,resolved_at FROM rm_workspace_inbox_exceptions WHERE source_id=?").get(sourceId) as any;
  const allowed = (slug: string) => !visibleCompanySlugs || visibleCompanySlugs.has(slug);
  const scrubPayload = (payload: any) => {
    if (!payload || typeof payload !== "object") return payload;
    const copy = { ...payload };
    if (Array.isArray(copy.candidates)) copy.candidates = copy.candidates.filter((candidate: any) => allowed(candidate?.companySlug));
    if (typeof copy.companySlug === "string" && !allowed(copy.companySlug)) return null;
    if (typeof copy.requestedCompanySlug === "string" && !allowed(copy.requestedCompanySlug)) return null;
    if (typeof copy.existingCompanySlug === "string" && !allowed(copy.existingCompanySlug)) return null;
    return copy;
  };
  return { sourceId: row.source_id, visibilityAnchorSlug: row.visibility_anchor_slug, sha256: row.sha256, filename: row.filename, mimeType: row.mime_type, transport: row.transport, transportIdentity: row.transport_identity, receivedAt: row.received_at, metadata: JSON.parse(row.metadata_json), createdBy: row.created_by, createdAt: row.created_at, exception: exception ? { code: exception.code, requiredAction: exception.required_action, openedAt: exception.opened_at, resolvedAt: exception.resolved_at } : null, assignments: assignments.filter(x => allowed(x.company_slug)).map(x => ({ companySlug: x.company_slug, state: x.state, documentId: x.document_id, documentNo: x.document_no, assignedBy: x.assigned_by, assignedAt: x.assigned_at, completedAt: x.completed_at })), history: events.map(({ canonical_payload, ...event }) => ({ ...event, payload: scrubPayload(JSON.parse(canonical_payload)) })).filter(event => event.payload !== null) };
}

/** Lists anchor-scoped sources in stable source-id order; filtering is before pagination. */
export function listWorkspaceInboxSources(db: Database, input: { visibilityAnchorSlug: string; cursor?: number; limit?: number; visibleCompanySlugs?: ReadonlySet<string> }) {
  const limit = Math.min(Math.max(input.limit ?? 25, 1), 100), cursor = Math.max(input.cursor ?? 0, 0);
  const rows = db.query("SELECT source_id FROM rm_workspace_inbox_sources WHERE visibility_anchor_slug=? ORDER BY source_id").all(text(input.visibilityAnchorSlug, "visibilityAnchorSlug", 100)) as Array<{ source_id: string }>;
  const sources = rows.map(row => inspectWorkspaceInboxSource(db, row.source_id, input.visibilityAnchorSlug, input.visibleCompanySlugs)!).filter(Boolean);
  return { rows: sources.slice(cursor, cursor + limit), count: sources.length, nextCursor: cursor + limit < sources.length ? cursor + limit : null };
}

/** Records a reviewed company selection. It is idempotent only for the exact selected entity. */
export function approveWorkspaceInboxAssignment(db: Database, input: { sourceId: string; companySlug: string; actor: string; at?: string }) {
  const source = inspectWorkspaceInboxSource(db, input.sourceId);
  if (!source) throw new Error("workspace inbox source not found");
  const companySlug = text(input.companySlug, "companySlug", 100), at = input.at ? iso(input.at, "at") : new Date().toISOString();
  const complete = source.assignments.find((x: any) => x.state === "completed");
  if (complete && complete.companySlug !== companySlug) {
    append(db, input.sourceId, "reassignment_denied", { requestedCompanySlug: companySlug, existingCompanySlug: complete.companySlug, reason: "company document already handed off" }, input.actor, at);
    throw new Error("workspace inbox source is already handed off; use company correction or supersession controls");
  }
  const same = source.assignments.find((x: any) => x.companySlug === companySlug);
  if (same) return source;
  db.transaction(() => {
    const other = db.query("SELECT company_slug FROM rm_workspace_inbox_assignments WHERE source_id=? AND state IN ('approved','completed') AND company_slug<>? LIMIT 1").get(input.sourceId, companySlug) as { company_slug: string } | null;
    if (other) throw new Error("workspace inbox source already has an approved legal destination");
    db.query("INSERT INTO rm_workspace_inbox_assignments(source_id,company_slug,state,assigned_by,assigned_at) VALUES(?,?, 'approved',?,?)").run(input.sourceId, companySlug, text(input.actor, "actor", 160), at);
    db.query("UPDATE rm_workspace_inbox_exceptions SET resolved_at=? WHERE source_id=? AND resolved_at IS NULL").run(at, input.sourceId);
    append(db, input.sourceId, "assigned", { companySlug }, input.actor, at);
  })();
  return inspectWorkspaceInboxSource(db, input.sourceId)!;
}

/**
 * Performs the only ledger boundary. A deterministic source marker lets a
 * retry recover after a crash between company commit and control completion.
 */
export function completeWorkspaceInboxAssignment(controlDb: Database, companyDb: Database, companyRoot: string, input: { sourceId: string; companySlug: string; actor: string; at?: string; leaseMs?: number; faultAt?: "before-company-ingest"|"after-company-ingest"|"before-control-complete" }) {
  const source = inspectWorkspaceInboxSource(controlDb, input.sourceId);
  if (!source) throw new Error("workspace inbox source not found");
  const assignment = source.assignments.find((x: any) => x.companySlug === input.companySlug);
  if (!assignment) throw new Error("workspace inbox assignment is not approved");
  if (assignment.state === "completed") return source;
  const at=input.at ? iso(input.at,"at") : new Date().toISOString();
  const claimId=`inbox-claim-${randomUUID()}`, leaseExpiresAt=new Date(Date.parse(at)+Math.min(Math.max(input.leaseMs??120_000,1_000),900_000)).toISOString();
  let claimed=false;
  controlDb.exec("BEGIN IMMEDIATE");
  try {
    const row=controlDb.query("SELECT source_hash,state,lease_expires_at FROM rm_workspace_inbox_handoff_claims WHERE source_id=? AND company_slug=?").get(source.sourceId,input.companySlug) as {source_hash:string;state:string;lease_expires_at:string}|null;
    if (row?.source_hash && row.source_hash!==source.sha256) throw new Error("workspace inbox handoff claim conflicts with different source bytes");
    if (row?.state==="completed") { controlDb.exec("COMMIT"); return inspectWorkspaceInboxSource(controlDb,source.sourceId)!; }
    if (row && Date.parse(row.lease_expires_at)>Date.parse(at)) throw new Error("workspace inbox handoff is already in progress; inspect status before retrying");
    if (row) controlDb.query("UPDATE rm_workspace_inbox_handoff_claims SET state='claimed',claim_id=?,lease_expires_at=?,updated_at=? WHERE source_id=? AND company_slug=?").run(claimId,leaseExpiresAt,at,source.sourceId,input.companySlug);
    else controlDb.query("INSERT INTO rm_workspace_inbox_handoff_claims(source_id,company_slug,source_hash,state,claim_id,lease_expires_at,created_at,updated_at) VALUES(?,?,?,'claimed',?,?,?,?)").run(source.sourceId,input.companySlug,source.sha256,claimId,leaseExpiresAt,at,at);
    claimed=true; controlDb.exec("COMMIT");
  } catch(error) { try { controlDb.exec("ROLLBACK"); } catch {} throw error; }
  if (!claimed) throw new Error("workspace inbox handoff claim failed");
  if (input.faultAt==="before-company-ingest") throw new Error("injected workspace inbox fault before company ingest");
  const sourceMarker = `workspace-inbox:${source.sourceId}`;
  const recovered = companyDb.query("SELECT id,document_no,sha256_hash FROM documents WHERE source=? LIMIT 1").get(sourceMarker) as { id: number; document_no: string; sha256_hash: string } | null;
  let documentId: number, documentNo: string;
  if (recovered) {
    if (recovered.sha256_hash !== source.sha256) throw new Error("workspace inbox source marker conflicts with different company document bytes");
    documentId = recovered.id; documentNo = recovered.document_no;
  } else {
    const directory = mkdtempSync(join(tmpdir(), "rentemester-workspace-inbox-"));
    const safeName = source.filename.replace(/[^A-Za-z0-9._-]/g, "_") || "source.bin";
    const path = join(directory, safeName);
    try {
      writeFileSync(path, Buffer.from((controlDb.query("SELECT original_bytes FROM rm_workspace_inbox_sources WHERE source_id=?").get(source.sourceId) as { original_bytes: Uint8Array }).original_bytes), { mode: 0o600 });
      const supplied = { ...(source.metadata as DocumentMetadata), source: sourceMarker };
      const result = ingestDocument(companyDb, companyRoot, path, supplied, { createdBy: input.actor, createdByProgram: "workspace-document-inbox" });
      if (!result.ok || !result.documentId || !result.documentNo) throw new Error(result.errors?.join("; ") || "company document ingest failed");
      documentId = result.documentId; documentNo = result.documentNo;
    } finally { rmSync(directory, { recursive: true, force: true }); }
  }
  if (input.faultAt==="after-company-ingest") throw new Error("injected workspace inbox fault after company ingest");
  if (input.faultAt==="before-control-complete") throw new Error("injected workspace inbox fault before control completion");
  controlDb.exec("BEGIN IMMEDIATE"); try {
    const claim=controlDb.query("SELECT state,claim_id,source_hash,lease_expires_at FROM rm_workspace_inbox_handoff_claims WHERE source_id=? AND company_slug=?").get(source.sourceId,input.companySlug) as {state:string;claim_id:string;source_hash:string;lease_expires_at:string}|null;
    if (!claim || claim.source_hash!==source.sha256 || claim.claim_id!==claimId || claim.state!=="claimed" || claim.lease_expires_at!==leaseExpiresAt) throw new Error("workspace inbox handoff claim became stale");
    const current = controlDb.query("SELECT state,document_id FROM rm_workspace_inbox_assignments WHERE source_id=? AND company_slug=?").get(source.sourceId, input.companySlug) as { state: string; document_id: number | null } | null;
    if (!current) throw new Error("workspace inbox assignment disappeared");
    if (current.state === "completed" && current.document_id !== documentId) throw new Error("workspace inbox assignment completion conflicts");
    if (current.state !== "completed") {
      controlDb.query("UPDATE rm_workspace_inbox_assignments SET state='completed',document_id=?,document_no=?,completed_at=? WHERE source_id=? AND company_slug=? AND state='approved'").run(documentId, documentNo, at, source.sourceId, input.companySlug);
      append(controlDb, source.sourceId, "handoff_completed", { companySlug: input.companySlug, documentId, documentNo, sourceHash: source.sha256 }, input.actor, at);
    }
    const finalized=controlDb.query("UPDATE rm_workspace_inbox_handoff_claims SET state='completed',document_id=?,document_no=?,updated_at=? WHERE source_id=? AND company_slug=? AND state='claimed' AND claim_id=? AND lease_expires_at=?").run(documentId,documentNo,at,source.sourceId,input.companySlug,claimId,leaseExpiresAt);
    if (finalized.changes!==1) throw new Error("workspace inbox handoff claim became stale");
    controlDb.exec("COMMIT");
  } catch(error) { try { controlDb.exec("ROLLBACK"); } catch {} throw error; }
  return inspectWorkspaceInboxSource(controlDb, source.sourceId)!;
}
