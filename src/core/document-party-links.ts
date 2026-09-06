import { canonicalJson } from "./canonical-json";
/** Canonical, append-only document-to-party evidence.  Party identity remains
 * in the workspace registry; a company ledger stores only its immutable
 * snapshot at the moment a reviewed link is made. */
import { createHash } from "node:crypto";
import type { Database } from "bun:sqlite";
import { DocumentEvidenceError, snapshotRegisteredDocument } from "./document-storage";
import { inspectParty } from "./party-registry";
import { resolveSupplierIdentity, type SupplierIdentifierKind } from "./supplier-identity";

export const DOCUMENT_PARTY_LINK_ERRORS = {
  DOCUMENT_NOT_FOUND: "DOCUMENT_NOT_FOUND", PARTY_NOT_FOUND: "PARTY_NOT_FOUND",
  SCOPE_MISMATCH: "SCOPE_MISMATCH", NO_IDENTIFIER_EVIDENCE: "NO_IDENTIFIER_EVIDENCE",
  IDENTIFIER_CONFLICT: "IDENTIFIER_CONFLICT", MULTIPLE_CANDIDATES: "MULTIPLE_CANDIDATES",
  LEGACY_REFERENCE_UNREVIEWED: "LEGACY_REFERENCE_UNREVIEWED", PLAN_HASH_MISMATCH: "PLAN_HASH_MISMATCH",
  CONFIRMATION_REQUIRED: "CONFIRMATION_REQUIRED", ACTOR_REQUIRED: "ACTOR_REQUIRED", PRINCIPAL_REQUIRED: "PRINCIPAL_REQUIRED",
  LINK_NOT_FOUND: "LINK_NOT_FOUND", SUPERSEDE_REASON_REQUIRED: "SUPERSEDE_REASON_REQUIRED", IDEMPOTENCY_CONFLICT: "IDEMPOTENCY_CONFLICT",
  CURRENT_STATE_CONFLICT: "CURRENT_STATE_CONFLICT",
  SOURCE_REVIEW_INVALID: "SOURCE_REVIEW_INVALID", SOURCE_EVIDENCE_INVALID: "SOURCE_EVIDENCE_INVALID",
  CONTACT_CONFLICT: "CONTACT_CONFLICT", IDEMPOTENCY_KEY_REQUIRED: "IDEMPOTENCY_KEY_REQUIRED",
} as const;
export const DOCUMENT_PARTY_ROLES = ["issuer", "supplier", "customer", "recipient", "payer", "payee", "processor", "acquirer", "related_company", "establishment", "location", "payment_descriptor", "vendor", "owner", "adviser", "employee", "authority", "bank"] as const;
export type DocumentPartyRole = typeof DOCUMENT_PARTY_ROLES[number];
type PlanError = typeof DOCUMENT_PARTY_LINK_ERRORS[keyof typeof DOCUMENT_PARTY_LINK_ERRORS];
const roles = new Set<DocumentPartyRole>(DOCUMENT_PARTY_ROLES);
const sourceObservationRoles = new Set<DocumentPartyRole>(["establishment","location","payment_descriptor"]);
const canonical = (v: unknown): string => v === null || typeof v !== "object" ? JSON.stringify(v) : Array.isArray(v) ? `[${v.map(canonical).join(",")}]` : `{${Object.keys(v as object).sort().map(k=>`${JSON.stringify(k)}:${canonical((v as any)[k])}`).join(",")}}`;
const hash = (v: unknown) => createHash("sha256").update(canonical(v)).digest("hex");
const value = (v: unknown, max = 512) => typeof v === "string" && v.trim() && v.trim().length <= max ? v.trim() : null;
const fail = (error: PlanError) => ({ ok:false as const, errors:[error] as PlanError[] });

export type DocumentPartySourceReview = {
  observedName: string; observedAddress?: string; jurisdiction?: string;
  identifierKind?: SupplierIdentifierKind; identifier?: string;
  sourceReference: string; sourceLocation: string; rationale: string; vendorId?: number;
};
export type DocumentPartyLinkPlanInput = { documentId:number; companySlug:string; partyId?:string; role:DocumentPartyRole; jurisdiction?:string; identifierKind?:string; identifier?:string; legacyKind?:"customer"|"vendor"; legacyId?:string; reviewedLegacyReference?:string; sourceReview?:DocumentPartySourceReview };
const comparable = (v: unknown) => typeof v === "string" ? v.trim().replace(/\s+/g," ").toLocaleLowerCase("en-US") : "";
const normalizedIdentifier = (v: unknown) => typeof v === "string" ? v.replace(/[^a-z0-9]/gi,"").toUpperCase() : "";
const vendorSnapshot = (db:Database,id:number) => db.query("SELECT id,name,address,vat_or_cvr,country_code,identifier_kind,identity_status,notes FROM vendors WHERE id=?").get(id) as any;
/** This is deliberately read-only. Name is never an input because names are
 * not identity evidence. Exact ID evidence beats all party defaults. */
export function planDocumentPartyLink(ledger: Database, registry: Database, input: DocumentPartyLinkPlanInput, companyRoot?:string) {
  if (!Number.isInteger(input.documentId) || input.documentId <= 0 || !roles.has(input.role)) return fail(DOCUMENT_PARTY_LINK_ERRORS.DOCUMENT_NOT_FOUND);
  const doc = ledger.query("SELECT id,sha256_hash,payload_json,document_type,status,retain_until,sender_name,sender_address,supplier_country_code,supplier_identifier_kind,supplier_identity_status,sender_vat_cvr FROM documents WHERE id=?").get(input.documentId) as any;
  if (!doc) return fail(DOCUMENT_PARTY_LINK_ERRORS.DOCUMENT_NOT_FOUND);
  const partyId = value(input.partyId,64);
  let candidateIds: string[] = [];
  let evidence: any;
  if (input.sourceReview) {
    const review=input.sourceReview, observedName=value(review.observedName,320), sourceReference=value(review.sourceReference,500), sourceLocation=value(review.sourceLocation,300), rationale=value(review.rationale,1000);
    if (!partyId || !observedName || !sourceReference || !sourceLocation || !rationale || !companyRoot) return fail(DOCUMENT_PARTY_LINK_ERRORS.SOURCE_REVIEW_INVALID);
    const observationOnly=sourceObservationRoles.has(input.role);
    if(observationOnly&&(review.jurisdiction||review.identifierKind||review.identifier||review.vendorId!==undefined))return fail(DOCUMENT_PARTY_LINK_ERRORS.SOURCE_REVIEW_INVALID);
    const normalized=observationOnly?null:resolveSupplierIdentity({country:review.jurisdiction??"",identifierKind:review.identifierKind,identifier:review.identifier});
    if(normalized&&!normalized.ok) return fail(DOCUMENT_PARTY_LINK_ERRORS.SOURCE_REVIEW_INVALID);
    let source:ReturnType<typeof snapshotRegisteredDocument>;
    try { source=snapshotRegisteredDocument(ledger,companyRoot,input.documentId); }
    catch(error) { return fail(error instanceof DocumentEvidenceError ? DOCUMENT_PARTY_LINK_ERRORS.SOURCE_EVIDENCE_INVALID : DOCUMENT_PARTY_LINK_ERRORS.SOURCE_REVIEW_INVALID); }
    if(normalized?.identifier){
      const rows=registry.query("SELECT party_id FROM rm_party_identifiers WHERE jurisdiction=? AND identifier_kind=? AND identifier=? ORDER BY party_id").all(normalized.country,normalized.identifierKind,normalized.identifier) as any[];
      if(!rows.length)return fail(DOCUMENT_PARTY_LINK_ERRORS.NO_IDENTIFIER_EVIDENCE);
      if(rows.length>1)return fail(DOCUMENT_PARTY_LINK_ERRORS.MULTIPLE_CANDIDATES);
      if(rows[0]!.party_id!==partyId)return fail(DOCUMENT_PARTY_LINK_ERRORS.IDENTIFIER_CONFLICT);
    }
    candidateIds=[partyId];
    const imported={jurisdiction:doc.supplier_country_code,identifierKind:doc.supplier_identifier_kind,identifier:doc.sender_vat_cvr};
    if(normalized&&((imported.jurisdiction&&imported.jurisdiction!==normalized.country)||(imported.identifierKind&&imported.identifierKind!==normalized.identifierKind)||(imported.identifier&&normalizedIdentifier(imported.identifier)!==normalizedIdentifier(normalized.identifier)))) return fail(DOCUMENT_PARTY_LINK_ERRORS.IDENTIFIER_CONFLICT);
    let contactChange:any=null;
    if(review.vendorId!==undefined){
      if(!Number.isSafeInteger(review.vendorId)||review.vendorId!<=0||!(input.role==="vendor"||input.role==="supplier"))return fail(DOCUMENT_PARTY_LINK_ERRORS.CONTACT_CONFLICT);
      const before=vendorSnapshot(ledger,review.vendorId!); if(!before)return fail(DOCUMENT_PARTY_LINK_ERRORS.CONTACT_CONFLICT);
      if(comparable(before.name)!==comparable(observedName)||(before.address&&review.observedAddress&&comparable(before.address)!==comparable(review.observedAddress)))return fail(DOCUMENT_PARTY_LINK_ERRORS.CONTACT_CONFLICT);
      if(!normalized||(before.country_code&&before.country_code!==normalized.country)||(before.identifier_kind&&before.identifier_kind!==normalized.identifierKind)||(before.vat_or_cvr&&normalizedIdentifier(before.vat_or_cvr)!==normalizedIdentifier(normalized.identifier)))return fail(DOCUMENT_PARTY_LINK_ERRORS.CONTACT_CONFLICT);
      const after={...before,country_code:before.country_code??normalized.country,identifier_kind:before.identifier_kind??normalized.identifierKind,vat_or_cvr:before.vat_or_cvr??normalized.identifier,identity_status:"resolved"};
      if(after.vat_or_cvr&&(ledger.query("SELECT id,vat_or_cvr FROM vendors WHERE id<>? AND vat_or_cvr IS NOT NULL").all(review.vendorId!) as any[]).some(row=>normalizedIdentifier(row.vat_or_cvr)===normalizedIdentifier(after.vat_or_cvr)))return fail(DOCUMENT_PARTY_LINK_ERRORS.CONTACT_CONFLICT);
      contactChange=canonical(before)===canonical(after)?null:{vendorId:review.vendorId,before,after};
    }
    evidence={kind:"reviewed_source_observation",observationSubject:observationOnly?input.role:"external_counterparty",identityScope:observationOnly?"source_observed_non_tax":"legal_or_tax_identity",observedIdentity:observationOnly?{name:observedName,address:value(review.observedAddress,500)}:{name:observedName,address:value(review.observedAddress,500),jurisdiction:normalized!.country,identifierKind:normalized!.identifierKind,identifier:normalized!.identifier},source:{documentId:doc.id,documentSha256:source.sha256,documentType:doc.document_type,location:sourceLocation,reference:sourceReference,authorshipIsIdentityEvidence:false},documentMetadataSnapshot:{status:doc.status,retainUntil:doc.retain_until,senderName:doc.sender_name,senderAddress:doc.sender_address,supplierCountryCode:doc.supplier_country_code,supplierIdentifierKind:doc.supplier_identifier_kind,supplierIdentityStatus:doc.supplier_identity_status,senderVatOrCvr:doc.sender_vat_cvr},rationale,presentationDifferences:{name:comparable(observedName)!==comparable(doc.sender_name),address:Boolean(review.observedAddress)&&comparable(review.observedAddress)!==comparable(doc.sender_address)},contactChange,accountingEffect:"none",taxEffect:"none",...(observationOnly?{satisfiesLegalSupplierIdentity:false,satisfiesVatIdentity:false}:{}),invoiceClassification:doc.document_type==="internal_voucher"?"not_supplier_invoice":"unchanged"};
  } else if (value(input.jurisdiction,2) && value(input.identifierKind,32) && value(input.identifier,160)) {
    const normalized = resolveSupplierIdentity({ country: input.jurisdiction!, identifierKind: input.identifierKind as any, identifier: input.identifier });
    if (!normalized.ok || !normalized.identifier) return fail(DOCUMENT_PARTY_LINK_ERRORS.NO_IDENTIFIER_EVIDENCE);
    if (doc.supplier_country_code !== normalized.country || doc.supplier_identifier_kind !== normalized.identifierKind || doc.sender_vat_cvr !== normalized.identifier) return fail(DOCUMENT_PARTY_LINK_ERRORS.NO_IDENTIFIER_EVIDENCE);
    const rows = registry.query("SELECT party_id FROM rm_party_identifiers WHERE jurisdiction=? AND identifier_kind=? AND identifier=? ORDER BY party_id",).all(normalized.country,normalized.identifierKind,normalized.identifier) as any[];
    candidateIds = rows.map(r=>r.party_id);
    if (!candidateIds.length) return fail(DOCUMENT_PARTY_LINK_ERRORS.NO_IDENTIFIER_EVIDENCE);
    if (candidateIds.length > 1) return fail(DOCUMENT_PARTY_LINK_ERRORS.MULTIPLE_CANDIDATES);
    if (partyId && partyId !== candidateIds[0]) return fail(DOCUMENT_PARTY_LINK_ERRORS.IDENTIFIER_CONFLICT);
    evidence={kind:"exact_identifier",jurisdiction:normalized.country,identifierKind:normalized.identifierKind,identifier:normalized.identifier};
  } else if (input.legacyKind && value(input.legacyId,160)) {
    const legacyId=value(input.legacyId,160)!;
    const reviewedLegacyReference=value(input.reviewedLegacyReference,500);
    if (!reviewedLegacyReference) return fail(DOCUMENT_PARTY_LINK_ERRORS.LEGACY_REFERENCE_UNREVIEWED);
    const lifecycleCount=(registry.query("SELECT count(*) AS n FROM rm_legacy_party_mapping_events WHERE company_slug=? AND legacy_kind=? AND legacy_id=?")
      .get(input.companySlug,input.legacyKind,legacyId) as {n:number}).n;
    if (lifecycleCount > 0) {
      const mapping=registry.query("SELECT party_id,party_role,evidence_json,event_hash FROM current_legacy_party_mappings WHERE company_slug=? AND legacy_kind=? AND legacy_id=?")
        .get(input.companySlug,input.legacyKind,legacyId) as {party_id:string;party_role:string;evidence_json:string;event_hash:string}|null;
      if (!mapping) return fail(DOCUMENT_PARTY_LINK_ERRORS.NO_IDENTIFIER_EVIDENCE);
      let mappingEvidence:any; try { mappingEvidence=JSON.parse(mapping.evidence_json); } catch { return fail(DOCUMENT_PARTY_LINK_ERRORS.NO_IDENTIFIER_EVIDENCE); }
      if (mapping.party_role!==input.role || mappingEvidence.reviewedLegacyReference!==reviewedLegacyReference) return fail(DOCUMENT_PARTY_LINK_ERRORS.LEGACY_REFERENCE_UNREVIEWED);
      candidateIds=[mapping.party_id];
      evidence={kind:"reviewed_legacy_reference",legacyKind:input.legacyKind,legacyId,reviewedLegacyReference,mappingEventHash:mapping.event_hash,sourceDocumentId:mappingEvidence.documentId,sourceDocumentSha256:mappingEvidence.documentSha256};
    } else {
      const rows=registry.query("SELECT party_id FROM rm_party_legacy_links WHERE company_slug=? AND legacy_kind=? AND legacy_id=? ORDER BY party_id").all(input.companySlug,input.legacyKind,legacyId) as any[];
      candidateIds=rows.map(r=>r.party_id);
      evidence={kind:"reviewed_legacy_reference",legacyKind:input.legacyKind,legacyId,reviewedLegacyReference};
    }
    if(!candidateIds.length) return fail(DOCUMENT_PARTY_LINK_ERRORS.NO_IDENTIFIER_EVIDENCE); if(candidateIds.length>1) return fail(DOCUMENT_PARTY_LINK_ERRORS.MULTIPLE_CANDIDATES); if(partyId&&partyId!==candidateIds[0]) return fail(DOCUMENT_PARTY_LINK_ERRORS.IDENTIFIER_CONFLICT);
  } else return fail(DOCUMENT_PARTY_LINK_ERRORS.NO_IDENTIFIER_EVIDENCE);
  const party=inspectParty(registry,candidateIds[0]!); if(!party) return fail(DOCUMENT_PARTY_LINK_ERRORS.PARTY_NOT_FOUND);
  if(input.sourceReview && !evidence.observedIdentity.identifier) {
    const approvedNames=[{kind:"canonical_name",value:party.name,payloadHash:null},...party.aliases.filter((alias:any)=>alias.review_state==="approved").map((alias:any)=>({kind:"approved_alias",value:alias.alias,payloadHash:alias.payload_hash}))];
    const matched=approvedNames.find((entry:any)=>comparable(entry.value)===comparable(evidence.observedIdentity.name));
    if(!matched)return fail(DOCUMENT_PARTY_LINK_ERRORS.NO_IDENTIFIER_EVIDENCE);
    if(evidence.identityScope==="source_observed_non_tax"){
      const candidates=(registry.query("SELECT party_id FROM rm_party_events WHERE event_type='created' ORDER BY party_id").all() as Array<{party_id:string}>).map(row=>inspectParty(registry,row.party_id)!).filter(candidate=>candidate.roles.some((role:any)=>role.companySlug===input.companySlug&&sourceObservationRoles.has(role.role))&&[candidate.name,...candidate.aliases.filter((alias:any)=>alias.review_state==="approved").map((alias:any)=>alias.alias)].some(name=>comparable(name)===comparable(evidence.observedIdentity.name)));
      if(candidates.length>1)return fail(DOCUMENT_PARTY_LINK_ERRORS.MULTIPLE_CANDIDATES);
      if(candidates.length!==1||candidates[0]!.partyId!==partyId)return fail(DOCUMENT_PARTY_LINK_ERRORS.NO_IDENTIFIER_EVIDENCE);
    }
    evidence.canonicalNameEvidence=matched;
  }
  if (!party.roles.some((r:any)=>r.companySlug===input.companySlug && r.role===input.role)) return fail(DOCUMENT_PARTY_LINK_ERRORS.SCOPE_MISMATCH);
  const snapshot={partyId:party.partyId,kind:party.kind,name:party.name,identifiers:party.identifiers ?? [],roles:party.roles.filter((r:any)=>r.companySlug===input.companySlug&&r.role===input.role),history:party.history,...(input.sourceReview?{aliases:party.aliases,assertions:party.assertions}:{})};
  const payload={documentId:doc.id,documentSha256:doc.sha256_hash,documentPayloadSha256:hash(JSON.parse(doc.payload_json ?? "{}")),partyId:party.partyId,partySnapshot:snapshot,role:input.role,evidence};
  return {ok:true as const, plan:{...payload,planHash:hash(payload)}};
}
export function applyDocumentPartyLink(ledger:Database, registry:Database, input:DocumentPartyLinkPlanInput & {planHash:string; confirm:boolean; actor?:string; principal?:string; idempotencyKey?:string}, companyRoot?:string) {
  if(!input.confirm) return fail(DOCUMENT_PARTY_LINK_ERRORS.CONFIRMATION_REQUIRED); if(!value(input.actor,160)) return fail(DOCUMENT_PARTY_LINK_ERRORS.ACTOR_REQUIRED); if(!value(input.principal,160)) return fail(DOCUMENT_PARTY_LINK_ERRORS.PRINCIPAL_REQUIRED);
  if(input.sourceReview&&!value(input.idempotencyKey,128))return fail(DOCUMENT_PARTY_LINK_ERRORS.IDEMPOTENCY_KEY_REQUIRED);
  return ledger.transaction(()=>{
    const keyed=input.idempotencyKey ? ledger.query("SELECT id,plan_hash FROM document_party_link_events WHERE idempotency_key=?").get(input.idempotencyKey) as any : null;
    if(keyed){if(keyed.plan_hash!==input.planHash)return fail(DOCUMENT_PARTY_LINK_ERRORS.IDEMPOTENCY_CONFLICT);return {ok:true as const,id:keyed.id,idempotent:true,planHash:input.planHash};}
    const planned=planDocumentPartyLink(ledger,registry,input,companyRoot); if(!planned.ok) return planned; if(input.planHash!==planned.plan.planHash) return fail(DOCUMENT_PARTY_LINK_ERRORS.PLAN_HASH_MISMATCH);
    if (ledger.query("SELECT 1 FROM current_document_party_resolution_events WHERE document_id=? AND state='internal_no_external_party'").get(input.documentId)) return fail(DOCUMENT_PARTY_LINK_ERRORS.CURRENT_STATE_CONFLICT);
    const current=ledger.query("SELECT id,plan_hash FROM current_document_party_links WHERE document_id=? AND party_role=? ORDER BY id DESC LIMIT 1").get(input.documentId,input.role) as {id:number;plan_hash:string}|null;
    if (current && current.plan_hash !== input.planHash) return fail(DOCUMENT_PARTY_LINK_ERRORS.CURRENT_STATE_CONFLICT);
    const existing=ledger.query("SELECT id FROM document_party_link_events WHERE document_id=? AND party_role=? AND event_type='linked' AND plan_hash=?").get(input.documentId,input.role,input.planHash) as any;
    if(existing)return {ok:true as const,id:existing.id,idempotent:true,planHash:input.planHash};
    const change=planned.plan.evidence.contactChange;
    if(change){const changed=ledger.query("UPDATE vendors SET vat_or_cvr=?,country_code=?,identifier_kind=?,identity_status=? WHERE id=? AND COALESCE(vat_or_cvr,'')=COALESCE(?,'') AND COALESCE(country_code,'')=COALESCE(?,'') AND COALESCE(identifier_kind,'')=COALESCE(?,'') AND identity_status=?").run(change.after.vat_or_cvr,change.after.country_code,change.after.identifier_kind,change.after.identity_status,change.vendorId,change.before.vat_or_cvr,change.before.country_code,change.before.identifier_kind,change.before.identity_status).changes;if(changed!==1)return fail(DOCUMENT_PARTY_LINK_ERRORS.CONTACT_CONFLICT);}
    const event=ledger.query("INSERT INTO document_party_link_events(document_id,party_id,party_role,event_type,evidence_kind,evidence_json,document_sha256,document_payload_sha256,party_snapshot_json,plan_hash,idempotency_key,actor,principal,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?) RETURNING id").get(input.documentId,planned.plan.partyId,input.role,"linked",planned.plan.evidence.kind,canonical(planned.plan.evidence),planned.plan.documentSha256,planned.plan.documentPayloadSha256,canonical(planned.plan.partySnapshot),input.planHash,input.idempotencyKey??null,input.actor,input.principal,new Date().toISOString()) as any;
    return {ok:true as const,id:event.id,idempotent:false,planHash:input.planHash};
  }).immediate();
}
export function supersedeDocumentPartyLink(ledger:Database,input:{documentId:number;role:DocumentPartyRole;planHash:string;reason:string;actor?:string;principal?:string;confirm:boolean}) {
  if(!input.confirm) return fail(DOCUMENT_PARTY_LINK_ERRORS.CONFIRMATION_REQUIRED); if(!value(input.actor,160)) return fail(DOCUMENT_PARTY_LINK_ERRORS.ACTOR_REQUIRED); if(!value(input.principal,160)) return fail(DOCUMENT_PARTY_LINK_ERRORS.PRINCIPAL_REQUIRED); if(!value(input.reason,1000)) return fail(DOCUMENT_PARTY_LINK_ERRORS.SUPERSEDE_REASON_REQUIRED);
  const link=ledger.query("SELECT * FROM current_document_party_links WHERE document_id=? AND party_role=? AND plan_hash=?").get(input.documentId,input.role,input.planHash) as any; if(!link) return fail(DOCUMENT_PARTY_LINK_ERRORS.LINK_NOT_FOUND);
  const e=ledger.query("INSERT OR IGNORE INTO document_party_link_events(document_id,party_id,party_role,event_type,evidence_kind,evidence_json,document_sha256,document_payload_sha256,party_snapshot_json,plan_hash,reason,actor,principal,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?) RETURNING id").get(input.documentId,link.party_id,input.role,"superseded",link.evidence_kind,link.evidence_json,link.document_sha256,link.document_payload_sha256,link.party_snapshot_json,input.planHash,input.reason,input.actor,input.principal,new Date().toISOString()) as any;
  return {ok:true as const,id:e?.id ?? null,idempotent:!e};
}
export function inspectDocumentPartyLinks(ledger:Database, documentId:number) { return ledger.query(`SELECT id,party_id,party_role,event_type,evidence_kind,evidence_json,document_sha256,document_payload_sha256,party_snapshot_json,plan_hash,reason,actor,principal,created_at FROM document_party_link_events WHERE document_id=?
  UNION ALL SELECT id,NULL,NULL,'unresolved_external_party',NULL,json_object('evidenceReference',evidence_reference,'nextAction',next_action),document_sha256,NULL,NULL,plan_hash,rationale,actor,principal,created_at FROM party_coverage_bank_resolution_events WHERE document_id=? AND resolution_type='unresolved_external_party' ORDER BY created_at,id`).all(documentId,documentId); }
export function listDocumentPartyLinks(ledger:Database, input:{status?:"linked"|"unlinked"|DocumentResolutionState;limit?:number}={}) { const legal="EXISTS(SELECT 1 FROM current_document_party_links l WHERE l.document_id=d.id AND l.party_role NOT IN ('establishment','location','payment_descriptor'))"; const observed="EXISTS(SELECT 1 FROM current_document_party_links l WHERE l.document_id=d.id AND l.party_role IN ('establishment','location','payment_descriptor'))"; const internal="EXISTS(SELECT 1 FROM current_document_party_resolution_events r WHERE r.document_id=d.id)"; const external="EXISTS(SELECT 1 FROM party_coverage_bank_resolution_events r WHERE r.document_id=d.id AND r.resolution_type='unresolved_external_party')"; const state=`CASE WHEN ${internal} THEN 'internal_no_external_party' WHEN ${legal} THEN 'resolved' WHEN ${observed} THEN 'source_observed' WHEN ${external} THEN 'unresolved_external_party' ELSE 'unresolved' END`; const wanted=input.status==="linked"?"resolved":input.status==="unlinked"?"unresolved":input.status; const where=wanted?`WHERE ${state}=?`:""; return ledger.query(`SELECT d.id,d.document_no,d.sha256_hash, ${legal} AS linked, ${state} AS resolution_state FROM documents d ${where} ORDER BY d.id DESC LIMIT ?`).all(...(wanted?[wanted,Math.min(Math.max(input.limit??100,1),100)]:[Math.min(Math.max(input.limit??100,1),100)])); }

export const DOCUMENT_RESOLUTION_REASONS = ["NO_PARTY_DECISION", "NO_IDENTIFIER_EVIDENCE", "MULTIPLE_CANDIDATES", "SCOPE_MISMATCH", "REVIEW_REQUIRED"] as const;
export type DocumentResolutionReason = typeof DOCUMENT_RESOLUTION_REASONS[number];
export type DocumentResolutionState = "resolved" | "source_observed" | "unresolved_external_party" | "internal_no_external_party" | "unresolved";

/** A no-party decision is a first-class, hash-bound decision.  It exists only
 * for internal vouchers and never changes the immutable document, VAT, or
 * journal facts. */
export function decideInternalNoExternalParty(ledger: Database, input: { documentId:number; reason: string; confirm:boolean; actor?:string; principal?:string; idempotencyKey?:string }) {
  if (!input.confirm) return fail(DOCUMENT_PARTY_LINK_ERRORS.CONFIRMATION_REQUIRED);
  if (!value(input.actor,160)) return fail(DOCUMENT_PARTY_LINK_ERRORS.ACTOR_REQUIRED);
  if (!value(input.principal,160)) return fail(DOCUMENT_PARTY_LINK_ERRORS.PRINCIPAL_REQUIRED);
  const reason=value(input.reason,1000); if (!reason) return fail(DOCUMENT_PARTY_LINK_ERRORS.SUPERSEDE_REASON_REQUIRED);
  const doc=ledger.query("SELECT id,document_type,sha256_hash,payload_json FROM documents WHERE id=?").get(input.documentId) as any;
  if (!doc || doc.document_type!=="internal_voucher") return fail(DOCUMENT_PARTY_LINK_ERRORS.DOCUMENT_NOT_FOUND);
  if ((ledger.query("SELECT 1 FROM current_document_party_links WHERE document_id=? LIMIT 1").get(input.documentId) as any)) return fail(DOCUMENT_PARTY_LINK_ERRORS.IDEMPOTENCY_CONFLICT);
  const payload={documentId:doc.id,documentSha256:doc.sha256_hash,documentPayloadSha256:hash(JSON.parse(doc.payload_json??"{}")),state:"internal_no_external_party",reason}; const decisionHash=hash(payload);
  const existing=input.idempotencyKey ? ledger.query("SELECT id,decision_hash FROM document_party_resolution_events WHERE idempotency_key=?").get(input.idempotencyKey) as any : null;
  if(existing && existing.decision_hash!==decisionHash) return fail(DOCUMENT_PARTY_LINK_ERRORS.IDEMPOTENCY_CONFLICT);
  const current=ledger.query("SELECT id,decision_hash FROM current_document_party_resolution_events WHERE document_id=? AND state='internal_no_external_party'").get(input.documentId) as any;
  if(existing || current) { if(current && current.decision_hash!==decisionHash) return fail(DOCUMENT_PARTY_LINK_ERRORS.IDEMPOTENCY_CONFLICT); return {ok:true as const,id:(existing??current).id,idempotent:true,decisionHash}; }
  const row=ledger.query("INSERT INTO document_party_resolution_events(document_id,state,reason,document_sha256,document_payload_sha256,decision_hash,idempotency_key,actor,principal,created_at) VALUES(?,?,?,?,?,?,?,?,?,?) RETURNING id").get(input.documentId,"internal_no_external_party",reason,payload.documentSha256,payload.documentPayloadSha256,decisionHash,input.idempotencyKey??null,input.actor,input.principal,new Date().toISOString()) as any;
  return {ok:true as const,id:row.id,idempotent:false,decisionHash};
}
export function supersedeInternalNoExternalParty(ledger:Database,input:{documentId:number;decisionHash:string;reason:string;confirm:boolean;actor?:string;principal?:string}) {
  if(!input.confirm) return fail(DOCUMENT_PARTY_LINK_ERRORS.CONFIRMATION_REQUIRED); if(!value(input.actor,160)) return fail(DOCUMENT_PARTY_LINK_ERRORS.ACTOR_REQUIRED); if(!value(input.principal,160)) return fail(DOCUMENT_PARTY_LINK_ERRORS.PRINCIPAL_REQUIRED); const reason=value(input.reason,1000); if(!reason) return fail(DOCUMENT_PARTY_LINK_ERRORS.SUPERSEDE_REASON_REQUIRED);
  const current=ledger.query("SELECT * FROM current_document_party_resolution_events WHERE document_id=? AND state='internal_no_external_party' AND decision_hash=?").get(input.documentId,input.decisionHash) as any; if(!current)return fail(DOCUMENT_PARTY_LINK_ERRORS.LINK_NOT_FOUND);
  const row=ledger.query("INSERT OR IGNORE INTO document_party_resolution_events(document_id,state,reason,document_sha256,document_payload_sha256,decision_hash,supersedes_hash,actor,principal,created_at) VALUES(?,?,?,?,?,?,?,?,?,?) RETURNING id").get(input.documentId,"superseded",reason,current.document_sha256,current.document_payload_sha256,current.decision_hash,current.decision_hash,input.actor,input.principal,new Date().toISOString()) as any; return {ok:true as const,id:row?.id??null,idempotent:!row};
}
export function documentResolution(ledger:Database,documentId:number): {state:DocumentResolutionState; reason:DocumentResolutionReason|null; decisionHash:string|null; nextAction?:string|null} {
  const internal=ledger.query("SELECT decision_hash FROM current_document_party_resolution_events WHERE document_id=? AND state='internal_no_external_party'").get(documentId) as any;
  if(internal)return {state:"internal_no_external_party",reason:null,decisionHash:internal.decision_hash};
  const legal=ledger.query("SELECT 1 FROM current_document_party_links WHERE document_id=? AND party_role NOT IN ('establishment','location','payment_descriptor') LIMIT 1").get(documentId) as any;
  if(legal)return {state:"resolved",reason:null,decisionHash:null};
  const observed=ledger.query("SELECT 1 FROM current_document_party_links WHERE document_id=? AND party_role IN ('establishment','location','payment_descriptor') LIMIT 1").get(documentId) as any;
  const external=ledger.query("SELECT plan_hash,next_action FROM party_coverage_bank_resolution_events WHERE document_id=? AND resolution_type='unresolved_external_party' ORDER BY id DESC LIMIT 1").get(documentId) as any;
  if(observed)return {state:"source_observed",reason:"REVIEW_REQUIRED",decisionHash:external?.plan_hash??null,nextAction:external?.next_action??null};
  return external?{state:"unresolved_external_party",reason:"REVIEW_REQUIRED",decisionHash:external.plan_hash,nextAction:external.next_action}:{state:"unresolved",reason:"NO_PARTY_DECISION",decisionHash:null};
}
