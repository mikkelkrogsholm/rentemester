import { canonicalJson } from "./canonical-json";
/**
 * Reviewable legal ownership/control evidence (#576).
 *
 * This is deliberately a workspace control model.  It never opens a ledger,
 * changes the v1 group manifest, or turns a registry observation into legal
 * truth.  A caller must explicitly review then apply the exact proposal hash.
 */
import { createHash, randomUUID } from "node:crypto";
import type { Database } from "bun:sqlite";
import { isValidSlug } from "./workspace";

export type OwnershipPrincipal = { kind: "user" | "service_account" | "local_operator"; id: string };
export type OwnershipEndpoint = { kind: "company"; companySlug: string } | { kind: "party"; partyId: string };
export type OwnershipFactInput = {
  owner: OwnershipEndpoint; ownedCompanySlug: string; validFrom: string; validToExclusive?: string;
  economicBasisPoints?: number; economicIntervalBasisPoints?: { min: number; max: number };
  votingBasisPoints?: number; controlType: "equity" | "voting" | "board" | "agreement" | "other";
  shareClass?: string; jurisdiction: string; evidenceRefs: string[];
};
export type OwnershipSnapshotInput = { snapshotId?: string; source: string; observedAt: string; facts: OwnershipFactInput[]; actor: string; principal: OwnershipPrincipal };

/** Derive authorization scope from proposal facts or the stored immutable snapshot. */
export function ownershipEndpointSlugs(db: Database | undefined, input: { facts?: unknown; snapshotId?: unknown }): Set<string> {
  let facts: unknown;
  if ("facts" in input) facts = input.facts;
  else {
    if (!db || typeof input.snapshotId !== "string") throw new Error("ownership snapshot not found");
    const row = db.query("SELECT canonical_facts FROM rm_ownership_source_snapshots WHERE snapshot_id=?").get(input.snapshotId) as { canonical_facts: string } | null;
    if (!row) throw new Error("ownership snapshot not found");
    try { facts = JSON.parse(row.canonical_facts); } catch { throw new Error("ownership snapshot is invalid"); }
  }
  if (!Array.isArray(facts)) throw new Error("ownership facts must be an array");
  const slugs = new Set<string>();
  for (const raw of facts) {
    const fact = raw as OwnershipFactInput | null;
    if (!fact || typeof fact.ownedCompanySlug !== "string" || !isValidSlug(fact.ownedCompanySlug.trim())) throw new Error("ownership fact requires ownedCompanySlug");
    slugs.add(fact.ownedCompanySlug.trim());
    if (fact.owner?.kind === "company") {
      if (typeof fact.owner.companySlug !== "string" || !isValidSlug(fact.owner.companySlug.trim())) throw new Error("company owner requires companySlug");
      slugs.add(fact.owner.companySlug.trim());
    }
  }
  return slugs;
}
const sha = (value: unknown) => createHash("sha256").update(canonical(value)).digest("hex");
const canonical = (value: unknown): string => value === null || typeof value !== "object" ? JSON.stringify(value) : Array.isArray(value) ? `[${value.map(canonical).join(",")}]` : `{${Object.keys(value as object).sort().map(k => `${JSON.stringify(k)}:${canonical((value as any)[k])}`).join(",")}}`;
const text = (v: unknown, label: string, max = 256) => { const s = typeof v === "string" ? v.trim() : ""; if (!s || s.length > max) throw new Error(`${label} is required and bounded`); return s; };
const date = (v: unknown, label: string) => { const s = text(v, label, 10); if (!/^\d{4}-\d{2}-\d{2}$/.test(s) || Number.isNaN(Date.parse(`${s}T00:00:00Z`))) throw new Error(`${label} must be an ISO date`); return s; };
const instant = (v: unknown, label: string) => { const parsed = new Date(text(v,label,64)); if (Number.isNaN(parsed.valueOf())) throw new Error(`${label} must be ISO-8601`); return parsed.toISOString(); };
const endpointKey = (e: OwnershipEndpoint) => e.kind === "company" ? `company:${e.companySlug}` : `party:${e.partyId}`;
const active = (row: { validFrom:string; validToExclusive?:string|null }, asOf:string) => row.validFrom <= asOf && (!row.validToExclusive || asOf < row.validToExclusive);

function endpoint(db: Database, input: unknown, label: string): OwnershipEndpoint {
  if (!input || typeof input !== "object" || Array.isArray(input)) throw new Error(`${label} is required`);
  const value = input as any;
  if (value.kind === "company") return { kind:"company", companySlug:text(value.companySlug, `${label}.companySlug`,120) };
  if (value.kind === "party") { const partyId=text(value.partyId, `${label}.partyId`,64); if (!db.query("SELECT 1 FROM rm_party_events WHERE party_id=? AND event_type='created'").get(partyId)) throw new Error(`${label} party does not exist`); return { kind:"party", partyId }; }
  throw new Error(`${label}.kind must be company or party`);
}
function fact(db: Database, raw: OwnershipFactInput, label: string): OwnershipFactInput {
  const owner=endpoint(db,raw.owner,`${label}.owner`), ownedCompanySlug=text(raw.ownedCompanySlug,`${label}.ownedCompanySlug`,120);
  if(owner.kind === "company" && owner.companySlug === ownedCompanySlug) throw new Error("ownership cannot be self-referential");
  const validFrom=date(raw.validFrom,`${label}.validFrom`), validToExclusive=raw.validToExclusive == null ? undefined : date(raw.validToExclusive,`${label}.validToExclusive`);
  if(validToExclusive && validToExclusive<=validFrom) throw new Error(`${label} must be a non-empty interval`);
  const exact=raw.economicBasisPoints, interval=raw.economicIntervalBasisPoints;
  if ((exact == null) === (interval == null)) throw new Error(`${label} requires exactly one economic exact value or interval`);
  if(exact != null && (!Number.isInteger(exact)||exact<0||exact>10000)) throw new Error(`${label}.economicBasisPoints must be 0..10000`);
  if(interval && (!Number.isInteger(interval.min)||!Number.isInteger(interval.max)||interval.min<0||interval.max>10000||interval.min>interval.max)) throw new Error(`${label}.economicIntervalBasisPoints must be a bounded interval`);
  if(raw.votingBasisPoints != null && (!Number.isInteger(raw.votingBasisPoints)||raw.votingBasisPoints<0||raw.votingBasisPoints>10000)) throw new Error(`${label}.votingBasisPoints must be 0..10000`);
  const controlType=raw.controlType; if(!["equity","voting","board","agreement","other"].includes(controlType)) throw new Error(`${label}.controlType is unsupported`);
  const jurisdiction=text(raw.jurisdiction,`${label}.jurisdiction`,2).toUpperCase(); if(!/^[A-Z]{2}$/.test(jurisdiction)) throw new Error(`${label}.jurisdiction must be ISO-3166 alpha-2`);
  const evidenceRefs=[...new Set((raw.evidenceRefs??[]).map((x,i)=>text(x,`${label}.evidenceRefs[${i}]`,256)))].sort(); if(!evidenceRefs.length||evidenceRefs.length>32) throw new Error(`${label}.evidenceRefs must contain 1..32 references`);
  return {owner,ownedCompanySlug,validFrom,...(validToExclusive?{validToExclusive}:{}),...(exact!=null?{economicBasisPoints:exact}:{economicIntervalBasisPoints:{min:interval!.min,max:interval!.max}}),...(raw.votingBasisPoints!=null?{votingBasisPoints:raw.votingBasisPoints}:{}),controlType,...(raw.shareClass?{shareClass:text(raw.shareClass,`${label}.shareClass`,80)}:{}),jurisdiction,evidenceRefs};
}
function state(db:Database,snapshotId:string) { return (db.query("SELECT event_type FROM rm_ownership_snapshot_events WHERE snapshot_id=? ORDER BY id DESC LIMIT 1").get(snapshotId) as any)?.event_type ?? null; }
function inspectSnapshot(db:Database,snapshotId:string) { const row=db.query("SELECT * FROM rm_ownership_source_snapshots WHERE snapshot_id=?").get(snapshotId) as any; if(!row)return null; const diff=JSON.parse(row.diff_json); return {snapshotId:row.snapshot_id,source:row.source,observedAt:row.observed_at,snapshotHash:row.snapshot_hash,diffHash:row.diff_hash,diff,state:state(db,snapshotId),actor:row.actor,principal:{kind:row.principal_kind,id:row.principal_id},history:db.query("SELECT event_type,actor,principal_kind,principal_id,created_at FROM rm_ownership_snapshot_events WHERE snapshot_id=? ORDER BY id").all(snapshotId)}; }
type Ending = { factHash:string; effectiveToExclusive:string; successorFactHash:string };
function approvedRows(db: Database) { return db.query("SELECT fact_hash,canonical_fact FROM rm_ownership_facts WHERE review_state='approved' ORDER BY fact_hash").all() as Array<{fact_hash:string;canonical_fact:string}>; }
function storedEndings(db: Database) { return new Map((db.query("SELECT fact_hash,MIN(effective_to_exclusive) AS end_at FROM rm_ownership_fact_events WHERE event_type='ended' GROUP BY fact_hash").all() as any[]).map(row=>[row.fact_hash,row.end_at as string])); }
function withEnd(fact: OwnershipFactInput, end?: string): OwnershipFactInput { return end&&(!fact.validToExclusive||end<fact.validToExclusive)?{...fact,validToExclusive:end}:fact; }
function plannedEndings(db: Database, incoming: OwnershipFactInput[]): Ending[] {
  const result: Ending[]=[]; const alreadyEnded=storedEndings(db);
  for(const old of approvedRows(db)) { const previous=withEnd(JSON.parse(old.canonical_fact) as OwnershipFactInput,alreadyEnded.get(old.fact_hash)); for(const next of incoming) {
    if(endpointKey(previous.owner)!==endpointKey(next.owner)||previous.ownedCompanySlug!==next.ownedCompanySlug||!overlaps(previous,next)) continue;
    if(next.validFrom<=previous.validFrom) throw new Error("direct ownership facts for the same endpoints must not overlap");
    result.push({factHash:old.fact_hash,effectiveToExclusive:next.validFrom,successorFactHash:sha(next)});
  }}
  return result.sort((a,b)=>a.factHash.localeCompare(b.factHash)||a.effectiveToExclusive.localeCompare(b.effectiveToExclusive));
}
function proposedDiff(db:Database,facts:OwnershipFactInput[],endings:Ending[]) {
  const approved = db.query("SELECT canonical_fact FROM rm_ownership_facts WHERE review_state='approved' ORDER BY fact_hash").all() as any[];
  const known=new Map(approved.map(r=>{const f=JSON.parse(r.canonical_fact);return [sha(f),f];})); const incoming=new Map(facts.map(f=>[sha(f),f]));
  const add=[...incoming].filter(([h])=>!known.has(h)).map(([,f])=>f).sort((a,b)=>sha(a).localeCompare(sha(b)));
  const unchanged=[...incoming.keys()].filter(h=>known.has(h)).sort();
  // Missing observations never end a fact.  An explicit later fact for the
  // same endpoint is the only reviewable, hash-bound ending instruction.
  const conflicts=[...known].filter(([h])=>!incoming.has(h)).map(([,f])=>f).filter((f:any)=>facts.some(n=>n.ownedCompanySlug===f.ownedCompanySlug && endpointKey(n.owner)===endpointKey(f.owner))).sort((a,b)=>sha(a).localeCompare(sha(b)));
  return {version:2,add,unchanged,conflicts,endings,note:"Only an explicit later fact can end an approved legal fact"};
}
function overlaps(a:OwnershipFactInput,b:OwnershipFactInput){return a.validFrom<(b.validToExclusive??"9999-12-31")&&b.validFrom<(a.validToExclusive??"9999-12-31");}
/** Reject ambiguous legal graph input before it becomes a reviewable proposal.
 * The check is intentionally narrower than a legal conclusion: it only
 * rejects contradictory direct facts (overlap, >100%, company cycles). */
function assertGraphIntegrity(db:Database,incoming:OwnershipFactInput[]): Ending[]{
  if(new Set(incoming.map(sha)).size!==incoming.length)throw new Error("ownership snapshot contains duplicate canonical facts");
  const endings=plannedEndings(db,incoming); const endByHash=new Map(endings.map(end=>[end.factHash,end.effectiveToExclusive])); const alreadyEnded=storedEndings(db);
  const approved=approvedRows(db).map(row=>withEnd(withEnd(JSON.parse(row.canonical_fact)as OwnershipFactInput,alreadyEnded.get(row.fact_hash)),endByHash.get(row.fact_hash)));
  const all=[...approved,...incoming.filter(f=>!approved.some(a=>sha(a)===sha(f)))];
  for(let i=0;i<all.length;i++)for(let j=i+1;j<all.length;j++){
    const a=all[i]!,b=all[j]!;
    if(endpointKey(a.owner)===endpointKey(b.owner)&&a.ownedCompanySlug===b.ownedCompanySlug&&overlaps(a,b))throw new Error("direct ownership facts for the same endpoints must not overlap");
  }
  const boundaries=[...new Set(all.flatMap(f=>[f.validFrom,f.validToExclusive].filter(Boolean)as string[]))].sort();
  for(const asOf of boundaries){
    const activeFacts=all.filter(f=>active(f,asOf)); const totals=new Map<string,number>();
    // An interval is not a free pass around the 100% invariant.  Its minimum
    // is the legally asserted ownership floor and must participate in every
    // effective-slice total; a pair of 70..100% observations therefore fails
    // just as two exact 70% observations do.
    for(const row of activeFacts){const minimum=row.economicBasisPoints ?? row.economicIntervalBasisPoints!.min;totals.set(row.ownedCompanySlug,(totals.get(row.ownedCompanySlug)??0)+minimum);}
    if([...totals.values()].some(total=>total>10000))throw new Error("combined active direct ownership for a company must not exceed 10000 basis points");
    const edges=activeFacts.filter(f=>f.owner.kind==="company").map(f=>[(f.owner as {companySlug:string}).companySlug,f.ownedCompanySlug]as const); const visiting=new Set<string>(),done=new Set<string>();
    const visit=(node:string):boolean=>{if(visiting.has(node))return true;if(done.has(node))return false;visiting.add(node);for(const [,child]of edges.filter(([parent])=>parent===node))if(visit(child))return true;visiting.delete(node);done.add(node);return false;};
    if([...new Set(edges.flat())].some(visit))throw new Error("ownership graph contains an effective company cycle");
  }
  return endings;
}
export function proposeOwnershipSnapshot(db:Database,input:OwnershipSnapshotInput) {
  const facts=(input.facts??[]).map((f,i)=>fact(db,f,`facts[${i}]`)); if(!facts.length||facts.length>2048)throw new Error("facts must contain 1..2048 rows"); const endings=assertGraphIntegrity(db,facts);
  const snapshotId=input.snapshotId?text(input.snapshotId,"snapshotId",80):`ownership-${randomUUID()}`; const source=text(input.source,"source",160), observedAt=instant(input.observedAt,"observedAt"), actor=text(input.actor,"actor",160), principalId=text(input.principal.id,"principal id",160);
  const canonicalFacts=[...facts].sort((a,b)=>sha(a).localeCompare(sha(b))); const snapshotHash=sha({source,observedAt,facts:canonicalFacts}), diff=proposedDiff(db,canonicalFacts,endings), diffHash=sha(diff);
  db.transaction(()=>{const existing=inspectSnapshot(db,snapshotId);if(existing){if(existing.snapshotHash!==snapshotHash||existing.diffHash!==diffHash)throw new Error("snapshotId already exists with different immutable content");return;} db.query("INSERT INTO rm_ownership_source_snapshots(snapshot_id,source,observed_at,snapshot_hash,diff_hash,canonical_facts,diff_json,actor,principal_kind,principal_id,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)").run(snapshotId,source,observedAt,snapshotHash,diffHash,canonical(canonicalFacts),canonical(diff),actor,input.principal.kind,principalId,new Date().toISOString());db.query("INSERT INTO rm_ownership_snapshot_events(snapshot_id,event_type,actor,principal_kind,principal_id,created_at) VALUES(?,?,?,?,?,?)").run(snapshotId,"proposed",actor,input.principal.kind,principalId,new Date().toISOString());})(); return inspectSnapshot(db,snapshotId)!;
}
export function reviewOwnershipSnapshot(db:Database,input:{snapshotId:string;decision:"approved"|"rejected";actor:string;principal:OwnershipPrincipal}) { const id=text(input.snapshotId,"snapshotId",80);const snapshot=inspectSnapshot(db,id);if(!snapshot)throw new Error("ownership snapshot not found");const actor=text(input.actor,"actor",160),principalId=text(input.principal.id,"principal id",160);if(snapshot.principal.kind===input.principal.kind&&snapshot.principal.id===principalId)throw new Error("ownership reviewer must be a different authenticated principal than proposer");db.transaction(()=>{const current=state(db,id);if(current===input.decision)return;if(current!=="proposed")throw new Error("ownership snapshot has already been reviewed");db.query("INSERT INTO rm_ownership_snapshot_events(snapshot_id,event_type,actor,principal_kind,principal_id,created_at) VALUES(?,?,?,?,?,?)").run(id,input.decision,actor,input.principal.kind,principalId,new Date().toISOString());})();return inspectSnapshot(db,id)!; }
/** Applies exactly an approved, source-hashed diff.  Authorisation belongs in
 * adapters; this core still requires a durable authenticated principal so an
 * audit actor can never be used as the authority. */
export function applyOwnershipSnapshot(db:Database,input:{snapshotId:string;snapshotHash:string;diffHash:string;actor:string;principal:OwnershipPrincipal;authorized:boolean}) { if(!input.authorized)throw new Error("ownership apply requires live narrow workspace permission");const id=text(input.snapshotId,"snapshotId",80), snapshot=inspectSnapshot(db,id);if(!snapshot)throw new Error("ownership snapshot not found");if(snapshot.snapshotHash!==input.snapshotHash||snapshot.diffHash!==input.diffHash)throw new Error("exact snapshot and diff hashes are required");if(state(db,id)==="applied")return {status:"unchanged" as const,snapshot};if(state(db,id)!=="approved")throw new Error("only an approved ownership snapshot can be applied");const actor=text(input.actor,"actor",160),pid=text(input.principal.id,"principal id",160);let applied=false;db.exec("BEGIN IMMEDIATE");try{const fresh=inspectSnapshot(db,id)!;if(state(db,id)!=="applied"){if(fresh.snapshotHash!==input.snapshotHash||fresh.diffHash!==input.diffHash||state(db,id)!=="approved")throw new Error("ownership snapshot became stale before apply");const additions=fresh.diff.add as OwnershipFactInput[];const endings=assertGraphIntegrity(db,additions);if(canonical(endings)!==canonical(fresh.diff.endings??[]))throw new Error("ownership snapshot became stale against current approved facts");for(const item of additions){const canonicalFact=canonical(item), factHash=sha(item);db.query("INSERT OR IGNORE INTO rm_ownership_facts(fact_id,snapshot_id,fact_hash,canonical_fact,owner_kind,owner_id,owned_company_slug,valid_from,valid_to_exclusive,economic_exact_bp,economic_min_bp,economic_max_bp,voting_bp,control_type,share_class,jurisdiction,review_state,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)").run(`ownership-fact-${factHash.slice(0,32)}`,id,factHash,canonicalFact,item.owner.kind,item.owner.kind==="company"?item.owner.companySlug:item.owner.partyId,item.ownedCompanySlug,item.validFrom,item.validToExclusive??null,item.economicBasisPoints??null,item.economicIntervalBasisPoints?.min??null,item.economicIntervalBasisPoints?.max??null,item.votingBasisPoints??null,item.controlType,item.shareClass??null,item.jurisdiction,"approved",new Date().toISOString());}for(const end of endings)db.query("INSERT INTO rm_ownership_fact_events(fact_hash,event_type,effective_to_exclusive,successor_fact_hash,snapshot_id,actor,principal_kind,principal_id,created_at) VALUES(?,?,?,?,?,?,?,?,?)").run(end.factHash,"ended",end.effectiveToExclusive,end.successorFactHash,id,actor,input.principal.kind,pid,new Date().toISOString());db.query("INSERT INTO rm_ownership_snapshot_events(snapshot_id,event_type,actor,principal_kind,principal_id,created_at) VALUES(?,?,?,?,?,?)").run(id,"applied",actor,input.principal.kind,pid,new Date().toISOString());applied=true;}db.exec("COMMIT");}catch(error){try{db.exec("ROLLBACK");}catch{}throw error;}return {status:applied?"applied" as const:"unchanged" as const,snapshot:inspectSnapshot(db,id)!}; }
export function queryOwnershipGraph(db:Database,input:{asOf:string;visibleCompanySlugs?:ReadonlySet<string>}) { const asOf=date(input.asOf,"asOf"),ends=storedEndings(db);const all=approvedRows(db).map(row=>withEnd(JSON.parse(row.canonical_fact)as OwnershipFactInput,ends.get(row.fact_hash))).filter(f=>active(f,asOf));const visible=input.visibleCompanySlugs;const facts=visible?all.filter(f=>visible.has(f.ownedCompanySlug)&&(f.owner.kind!=="company"||visible.has(f.owner.companySlug))):all;const hidden=!!visible&&facts.length!==all.length;return {asOf,facts,partial:hidden,consolidation:{eligible:false,reason:hidden?"hidden endpoints":all.some(f=>f.owner.kind!=="company"||f.economicBasisPoints==null||f.validToExclusive!=null)?"party, interval or non-exact ownership requires explicit supported scope":"existing v1 manifest remains authoritative"}}; }
/** A deliberately narrow projection helper.  It can never mutate the v1
 * manifest and rejects every ambiguity instead of inferring percentages. */
export function projectExactCompanyOwnership(db:Database,asOf:string) { const graph=queryOwnershipGraph(db,{asOf});if(graph.consolidation.reason!=="existing v1 manifest remains authoritative")return {eligible:false as const,reason:graph.consolidation.reason,edges:[]};const byChild=new Map<string,number>();for(const f of graph.facts){byChild.set(f.ownedCompanySlug,(byChild.get(f.ownedCompanySlug)??0)+f.economicBasisPoints!);}if([...byChild.values()].some(n=>n!==10000))return {eligible:false as const,reason:"incomplete or minority ownership totals",edges:[]};return {eligible:true as const,edges:graph.facts.map(f=>({parentCompanySlug:(f.owner as any).companySlug,childCompanySlug:f.ownedCompanySlug,basisPoints:f.economicBasisPoints!}))}; }
export function ownershipHistory(db:Database,snapshotId?:string){return snapshotId?[inspectSnapshot(db,snapshotId)].filter(Boolean):((db.query("SELECT snapshot_id FROM rm_ownership_source_snapshots ORDER BY id").all()as any[]).map(r=>inspectSnapshot(db,r.snapshot_id)));}
