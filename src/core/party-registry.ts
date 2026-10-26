import { canonicalJson } from "./canonical-json";
import { createHash, randomUUID } from "node:crypto";
import type { Database } from "bun:sqlite";
import { resolveSupplierIdentity, type SupplierIdentifierKind } from "./supplier-identity";


export const PARTY_KINDS = ["person", "organization", "public_authority", "financial_institution"] as const;
/** Roles are deliberately relationship roles, not a classification of the
 * canonical party.  A party can hold several roles in different companies. */
export const PARTY_ROLES = ["issuer", "supplier", "customer", "recipient", "payer", "payee", "processor", "acquirer", "related_company", "establishment", "location", "payment_descriptor", "vendor", "owner", "adviser", "employee", "authority", "bank"] as const;
type PartyKind = typeof PARTY_KINDS[number];
type PartyRole = typeof PARTY_ROLES[number];
type PartyEvent = "created" | "proposed_merge" | "approved_merge" | "superseded" | "linked";
const sha = (value: unknown) => createHash("sha256").update(canonicalJson(value)).digest("hex");
const instant = (value?: string) => {
  const date = new Date(value ?? new Date().toISOString());
  if (Number.isNaN(date.valueOf())) throw new Error("observedAt must be a valid ISO timestamp");
  return date.toISOString();
};
function text(value: unknown, label: string, max = 256): string { const out = typeof value === "string" ? value.trim() : ""; if (!out || out.length > max) throw new Error(`${label} is required and bounded`); return out; }
function event(db: Database, partyId: string, eventType: PartyEvent, kind: PartyKind, payload: object, actor: string, at?: string) {
  const canonical_payload = canonicalJson(payload); const payload_hash = sha(payload);
  db.query("INSERT OR IGNORE INTO rm_party_events(party_id,event_type,canonical_kind,payload_hash,canonical_payload,actor,created_at) VALUES(?,?,?,?,?,?,?)").run(partyId,eventType,kind,payload_hash,canonical_payload,text(actor,"actor",160),instant(at));
  return payload_hash;
}
export type PartyIdentifierInput = { country: string; identifier?: string; identifierKind: SupplierIdentifierKind };
export type PartyCreateInput = { partyId?: string; kind: PartyKind; name: string; aliases?: string[]; identifiers?: PartyIdentifierInput[]; source: string; observedAt: string; reviewAssertion: string; actor: string };
export function createParty(db: Database, input: PartyCreateInput) {
  if (!PARTY_KINDS.includes(input.kind)) throw new Error("unsupported canonical party kind");
  const aliases = [...new Set((input.aliases ?? []).map((x) => text(x,"alias",160)))]; if (aliases.length > 16) throw new Error("aliases are bounded to 16");
  const identifiers = (input.identifiers ?? []).map((item) => { const value = resolveSupplierIdentity({ country:item.country, identifier:item.identifier, identifierKind:item.identifierKind }); if (!value.ok) throw new Error(value.errors.join("; ")); if (!value.identifier) throw new Error("party identifiers must not be empty"); return value; });
  const partyId = input.partyId ? text(input.partyId,"partyId",64) : `party-${randomUUID()}`;
  const payload = { name:text(input.name,"name",320), aliases, identifiers:identifiers.map(({country,identifier,identifierKind})=>({country,identifier,identifierKind})), source:text(input.source,"source",160), observedAt:instant(input.observedAt), reviewAssertion:text(input.reviewAssertion,"reviewAssertion",500) };
  db.transaction(() => { const exists = db.query("SELECT canonical_payload FROM rm_party_events WHERE party_id=? AND event_type='created'").get(partyId) as {canonical_payload:string}|null; if (exists) { if (exists.canonical_payload !== canonicalJson(payload)) throw new Error("partyId already exists with different immutable payload"); return; } for (const id of identifiers) { const conflict=db.query("SELECT party_id FROM rm_party_identifiers WHERE jurisdiction=? AND identifier_kind=? AND identifier=?").get(id.country,id.identifierKind,id.identifier) as {party_id:string}|null; if(conflict && conflict.party_id!==partyId) throw new Error("identifier conflicts with another canonical party"); } event(db,partyId,"created",input.kind,payload,input.actor,input.observedAt); for(const id of identifiers) db.query("INSERT INTO rm_party_identifiers(party_id,jurisdiction,identifier_kind,identifier) VALUES(?,?,?,?)").run(partyId,id.country,id.identifierKind,id.identifier); for (const alias of aliases) addPartyAlias(db,{partyId,alias,source:input.source,observedAt:input.observedAt,reviewState:"approved",actor:input.actor}); for (const [field,value] of [["name",payload.name],...identifiers.map((id) => ["identifier",`${id.country}:${id.identifierKind}:${id.identifier}`] as const)]) assertPartyField(db,{partyId,field,value,source:input.source,observedAt:input.observedAt,reviewState:"approved",actor:input.actor}); })();
  return inspectParty(db,partyId);
}
export type PartyReviewState = "proposed" | "approved" | "rejected";
export function addPartyAlias(db: Database, input: {partyId:string;alias:string;source:string;observedAt:string;reviewState:PartyReviewState;actor:string}) {
  const party=inspectParty(db,input.partyId); if(!party) throw new Error("party not found");
  const alias=text(input.alias,"alias",160).toLowerCase(); const source=text(input.source,"source",160); const observedAt=instant(input.observedAt);
  if (!(["proposed","approved","rejected"] as const).includes(input.reviewState)) throw new Error("unsupported review state");
  const payload={alias,source,observedAt,reviewState:input.reviewState}; const payloadHash=sha(payload);
  db.query("INSERT OR IGNORE INTO rm_party_alias_assertions(party_id,alias,source,observed_at,review_state,payload_hash,actor,created_at) VALUES(?,?,?,?,?,?,?,?)").run(input.partyId,alias,source,observedAt,input.reviewState,payloadHash,text(input.actor,"actor",160),new Date().toISOString());
  return payloadHash;
}
/** Source-backed facts are immutable assertions. A later approved assertion
 * supersedes in reads by its append-only event id; it never overwrites history. */
export function assertPartyField(db: Database, input: {partyId:string;field:string;value:string;source:string;observedAt:string;reviewState:PartyReviewState;actor:string}) {
  if(!inspectParty(db,input.partyId)) throw new Error("party not found");
  const field=text(input.field,"field",64), value=text(input.value,"value",512), source=text(input.source,"source",160), observedAt=instant(input.observedAt);
  if (!(["proposed","approved","rejected"] as const).includes(input.reviewState)) throw new Error("unsupported review state");
  const payload={field,value,source,observedAt,reviewState:input.reviewState}; const payloadHash=sha(payload);
  db.query("INSERT OR IGNORE INTO rm_party_field_assertions(party_id,field,value,source,observed_at,review_state,payload_hash,actor,created_at) VALUES(?,?,?,?,?,?,?,?,?)").run(input.partyId,field,value,source,observedAt,input.reviewState,payloadHash,text(input.actor,"actor",160),new Date().toISOString());
  return payloadHash;
}
/** Idempotently maps a legacy local customer/vendor id; no ledger row or
 * historical reference is rewritten. */
export function linkLegacyPartyReference(db: Database, input:{partyId:string;companySlug:string;legacyKind:"customer"|"vendor";legacyId:string;actor:string;observedAt?:string}) {
  const party=inspectParty(db,input.partyId); if(!party) throw new Error("party not found");
  const companySlug=text(input.companySlug,"companySlug",120), legacyId=text(input.legacyId,"legacyId",160);
  const existing=db.query("SELECT party_id FROM rm_party_legacy_links WHERE company_slug=? AND legacy_kind=? AND legacy_id=?").get(companySlug,input.legacyKind,legacyId) as {party_id:string}|null;
  if(existing && existing.party_id!==input.partyId) throw new Error("legacy reference is already linked to another party");
  db.transaction(()=>{db.query("INSERT OR IGNORE INTO rm_party_legacy_links(company_slug,legacy_kind,legacy_id,party_id,actor,created_at) VALUES(?,?,?,?,?,?)").run(companySlug,input.legacyKind,legacyId,input.partyId,text(input.actor,"actor",160),instant(input.observedAt));event(db,input.partyId,"linked",party.kind,{companySlug,legacyKind:input.legacyKind,legacyId},input.actor,input.observedAt);})();
  return inspectParty(db,input.partyId)!;
}
export function linkPartyRole(db: Database, input: { partyId:string; companySlug:string; role:PartyRole; defaults?: { account?:string; vat?:string; currency?:string; paymentTermsDays?:number }; actor:string; observedAt?:string }) {
  if (!PARTY_ROLES.includes(input.role)) throw new Error("unsupported party role"); const party=inspectParty(db,input.partyId); if(!party) throw new Error("party not found"); const defaults=input.defaults ?? {}; if(Object.keys(defaults).some((key)=>!["account","vat","currency","paymentTermsDays"].includes(key))) throw new Error("unsupported company-scoped default");
  const existing=db.query("SELECT defaults_json FROM rm_party_company_roles WHERE party_id=? AND company_slug=? AND role=?").get(input.partyId,text(input.companySlug,"companySlug",120),input.role) as {defaults_json:string}|null; const encoded=canonicalJson(defaults); if(existing && existing.defaults_json!==encoded) throw new Error("conflicting company-scoped defaults require supersession");
  db.transaction(()=>{ db.query("INSERT OR IGNORE INTO rm_party_company_roles(party_id,company_slug,role,defaults_json) VALUES(?,?,?,?)").run(input.partyId,input.companySlug,input.role,encoded); event(db,input.partyId,"linked",party.kind,{companySlug:input.companySlug,role:input.role,defaults},input.actor,input.observedAt); })(); return inspectParty(db,input.partyId)!;
}
export function proposePartyMerge(db: Database, input:{ fromPartyId:string; intoPartyId:string; reviewAssertion:string; actor:string; observedAt?:string }) { const from=inspectParty(db,input.fromPartyId), into=inspectParty(db,input.intoPartyId); if(!from||!into) throw new Error("both parties must exist"); if(from.partyId===into.partyId) throw new Error("cannot merge a party into itself"); return event(db,from.partyId,"proposed_merge",from.kind,{intoPartyId:into.partyId,reviewAssertion:text(input.reviewAssertion,"reviewAssertion",500)},input.actor,input.observedAt); }
export function approvePartyMerge(db: Database, input:{ fromPartyId:string; proposalHash:string; actor:string; observedAt?:string }) { const from=inspectParty(db,input.fromPartyId); if(!from) throw new Error("party not found"); const proposal=db.query("SELECT canonical_payload FROM rm_party_events WHERE party_id=? AND event_type='proposed_merge' AND payload_hash=?").get(input.fromPartyId,input.proposalHash) as {canonical_payload:string}|null; if(!proposal) throw new Error("exact merge proposal not found"); const payload=JSON.parse(proposal.canonical_payload); event(db,input.fromPartyId,"approved_merge",from.kind,payload,input.actor,input.observedAt); event(db,input.fromPartyId,"superseded",from.kind,payload,input.actor,input.observedAt); return inspectParty(db,input.fromPartyId)!; }
export function inspectParty(db: Database, partyId:string) { const created=db.query("SELECT canonical_kind,canonical_payload FROM rm_party_events WHERE party_id=? AND event_type='created'").get(partyId) as {canonical_kind:PartyKind;canonical_payload:string}|null; if(!created)return null; const history=db.query("SELECT event_type,payload_hash,actor,created_at FROM rm_party_events WHERE party_id=? ORDER BY id").all(partyId); const roles=db.query("SELECT company_slug,role,defaults_json FROM rm_party_company_roles WHERE party_id=? ORDER BY company_slug,role").all(partyId); const aliases=db.query("SELECT alias,source,observed_at,review_state,payload_hash FROM rm_party_alias_assertions WHERE party_id=? ORDER BY id").all(partyId); const assertions=db.query("SELECT field,value,source,observed_at,review_state,payload_hash FROM rm_party_field_assertions WHERE party_id=? ORDER BY id").all(partyId); return {partyId,kind:created.canonical_kind,...JSON.parse(created.canonical_payload),aliases:aliases.length?aliases:JSON.parse(created.canonical_payload).aliases.map((alias:string)=>({alias,source:"legacy-created",observed_at:null,review_state:"approved",payload_hash:null})),assertions,roles:roles.map((r:any)=>({companySlug:r.company_slug,role:r.role,defaults:JSON.parse(r.defaults_json)})),history}; }
/** Exact lookup for adapters.  Never emulate authorization with a paginated
 * search: a party after page 100 is still either visible or not visible. */
export function inspectVisibleParty(db: Database, partyId: string, companySlugs: ReadonlySet<string>) {
  const party = inspectParty(db, partyId);
  if (!party || !party.roles.some((role: any) => companySlugs.has(role.companySlug))) return null;
  return { ...party, roles: party.roles.filter((role: any) => companySlugs.has(role.companySlug)), history: party.history.filter((event: any) => event.event_type !== "linked") };
}
export function searchParties(db:Database,input:{query?:string; companySlugs?:ReadonlySet<string>; cursor?:number; limit?:number}={}) { const limit=Math.min(Math.max(input.limit??25,1),100), cursor=Math.max(input.cursor??0,0); const rows=db.query("SELECT party_id FROM rm_party_events WHERE event_type='created' ORDER BY party_id").all() as Array<{party_id:string}>; const query=input.query?.trim().toLowerCase(); const visible=rows.map(r=>inspectParty(db,r.party_id)!).map(p=>input.companySlugs?{...p,roles:p.roles.filter((r:any)=>input.companySlugs!.has(r.companySlug)),history:p.history.filter((event:any)=>event.event_type!=="linked")} :p).filter(p=>!input.companySlugs||p.roles.length>0).filter(p=>!query||p.name.toLowerCase().includes(query)||p.aliases.some((a:any)=>String(a.alias ?? a).toLowerCase().includes(query))); return {rows:visible.slice(cursor,cursor+limit),count:visible.length,nextCursor:cursor+limit<visible.length?cursor+limit:null}; }
