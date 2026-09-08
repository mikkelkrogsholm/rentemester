/** Generic, reviewed reporting-chart profiles and read-only consolidated reports. */
import { type Database, type SQLQueryBindings } from "bun:sqlite";
import { createHash } from "node:crypto";
import type { ResolveActorInput } from "./actor";
import { resolveActor } from "./actor";
import { readAppliedBalanceEliminations } from "./consolidation-eliminations";
import { buildIntercompanyReconciliation } from "./intercompany-reconciliation";
import { parseGroupAsOf, readCurrentGroupManifest, type EffectiveInterval, type GroupManifest } from "./group-manifest";
import { verifyAuditChain } from "./ledger";
import { fromOre, toOre } from "./money";
import { companyPaths } from "./paths";
import { assertSchemaCompatibility } from "./schema-version";
import { companyRootForSlug, listWorkspaceCompanies } from "./workspace";
import { insertWorkspaceAudit } from "./workspace-control";
import { openCurrentLedgerReadOnly } from "./ledger-inspection";

const IDENTIFIER = /^[a-z][a-z0-9-]{0,63}$/;
const ACCOUNT = /^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$/;
type ReportSection = "asset" | "liability" | "equity" | "income" | "expense";
export type ConsolidationReportingLine = { id: string; label: string; section: ReportSection; displayOrder: number; role?: "current-result" };
export type ConsolidationAccountMapping = EffectiveInterval & { id: string; companySlug: string; accountNo: string; reportingLineId: string };
export type ConsolidationProfile = EffectiveInterval & { version: 1; id: string; groupId: string; currency: string; reportingLines: ConsolidationReportingLine[]; accountMappings: ConsolidationAccountMapping[]; evidenceRefs: string[] };

type ProfileEvent = { id: number; profile_id: string; event_type: "proposed" | "approved" | "revoked"; profile_hash: string; canonical_profile: string; previous_hash: string | null; event_hash: string; actor: string; created_at: string };
function sha(value: string): string { return createHash("sha256").update(value).digest("hex"); }
function compare(a: string, b: string): number { return a < b ? -1 : a > b ? 1 : 0; }
function string(value: unknown, label: string): string { if (typeof value !== "string" || !value.trim()) throw new Error(`${label} is required`); return value.trim().normalize("NFC"); }
function id(value: unknown, label: string): string { const result = string(value, label); if (!IDENTIFIER.test(result)) throw new Error(`${label} must be a lowercase stable identifier`); return result; }
function date(value: unknown, label: string): string { try { return parseGroupAsOf(value); } catch { throw new Error(`${label} must be a real ISO date`); } }
function active(interval: EffectiveInterval, asOf: string): boolean { return interval.validFrom <= asOf && (interval.validToExclusive == null || asOf < interval.validToExclusive); }
function overlaps(a: EffectiveInterval, b: EffectiveInterval): boolean { return (a.validToExclusive == null || b.validFrom < a.validToExclusive) && (b.validToExclusive == null || a.validFrom < b.validToExclusive); }
function covers(a: EffectiveInterval, b: EffectiveInterval): boolean { return a.validFrom <= b.validFrom && (a.validToExclusive == null || (b.validToExclusive != null && a.validToExclusive >= b.validToExclusive)); }
function refs(value: unknown, label: string): string[] { if (!Array.isArray(value) || value.length === 0 || value.length > 64) throw new Error(`${label} must contain 1 through 64 values`); const result = value.map((entry, index) => string(entry, `${label}[${index}]`)).sort(compare); if (new Set(result).size !== result.length || result.some((entry) => entry.length > 256)) throw new Error(`${label} is invalid`); return result; }

export function parseConsolidationProfile(input: unknown, groupManifest: GroupManifest): ConsolidationProfile {
  if (!input || typeof input !== "object" || Array.isArray(input)) throw new Error("consolidation profile must be an object");
  const row = input as Record<string, unknown>;
  if (row.version !== 1) throw new Error("consolidation profile version must be 1");
  const profileId = id(row.id, "profile.id");
  const groupId = id(row.groupId, "profile.groupId");
  const group = groupManifest.groups.find((candidate) => candidate.id === groupId);
  if (!group) throw new Error("consolidation profile references an unknown group");
  const currency = string(row.currency, "profile.currency").toUpperCase();
  if (!/^[A-Z]{3}$/.test(currency)) throw new Error("profile.currency must be an ISO 4217 code");
  const validFrom = date(row.validFrom, "profile.validFrom");
  const validToExclusive = row.validToExclusive == null ? undefined : date(row.validToExclusive, "profile.validToExclusive");
  if (validToExclusive && validToExclusive <= validFrom) throw new Error("consolidation profile interval must be non-empty");
  const profileInterval = { validFrom, ...(validToExclusive ? { validToExclusive } : {}) };
  if (!Array.isArray(row.reportingLines) || row.reportingLines.length < 5 || row.reportingLines.length > 512) throw new Error("profile.reportingLines must contain 5 through 512 lines");
  const reportingLines = row.reportingLines.map((entry, index): ConsolidationReportingLine => {
    if (!entry || typeof entry !== "object" || Array.isArray(entry)) throw new Error(`reportingLines[${index}] must be an object`);
    const line = entry as Record<string, unknown>;
    const section = string(line.section, `reportingLines[${index}].section`) as ReportSection;
    if (!(["asset", "liability", "equity", "income", "expense"] as const).includes(section)) throw new Error("reporting line section is invalid");
    if (!Number.isInteger(line.displayOrder) || (line.displayOrder as number) < 0 || (line.displayOrder as number) > 100000) throw new Error("reporting line displayOrder is invalid");
    const role = line.role == null ? undefined : string(line.role, `reportingLines[${index}].role`);
    if (role !== undefined && (role !== "current-result" || section !== "equity")) throw new Error("current-result role requires an equity line");
    const label = string(line.label, `reportingLines[${index}].label`);
    if (label.length > 160) throw new Error("reporting line label is too long");
    return { id: id(line.id, `reportingLines[${index}].id`), label, section, displayOrder: line.displayOrder as number, ...(role ? { role: "current-result" as const } : {}) };
  }).sort((a, b) => a.displayOrder - b.displayOrder || compare(a.id, b.id));
  if (new Set(reportingLines.map((line) => line.id)).size !== reportingLines.length) throw new Error("reporting line ids must be unique");
  if (reportingLines.filter((line) => line.role === "current-result").length !== 1) throw new Error("profile requires exactly one current-result equity line");
  for (const section of ["asset", "liability", "equity", "income", "expense"] as const) if (!reportingLines.some((line) => line.section === section)) throw new Error(`profile requires at least one ${section} line`);
  const lineById = new Map(reportingLines.map((line) => [line.id, line]));
  if (!Array.isArray(row.accountMappings) || row.accountMappings.length === 0 || row.accountMappings.length > 4096) throw new Error("profile.accountMappings must contain 1 through 4096 mappings");
  const accountMappings = row.accountMappings.map((entry, index): ConsolidationAccountMapping => {
    if (!entry || typeof entry !== "object" || Array.isArray(entry)) throw new Error(`accountMappings[${index}] must be an object`);
    const mapping = entry as Record<string, unknown>;
    const companySlug = string(mapping.companySlug, `accountMappings[${index}].companySlug`);
    const accountNo = string(mapping.accountNo, `accountMappings[${index}].accountNo`);
    if (!ACCOUNT.test(accountNo)) throw new Error("profile account number is invalid");
    const reportingLineId = id(mapping.reportingLineId, `accountMappings[${index}].reportingLineId`);
    if (!lineById.has(reportingLineId) || lineById.get(reportingLineId)?.role === "current-result") throw new Error("profile account mapping references an unavailable reporting line");
    const mappingFrom = date(mapping.validFrom, `accountMappings[${index}].validFrom`);
    const mappingTo = mapping.validToExclusive == null ? undefined : date(mapping.validToExclusive, `accountMappings[${index}].validToExclusive`);
    const interval = { validFrom: mappingFrom, ...(mappingTo ? { validToExclusive: mappingTo } : {}) };
    if ((mappingTo && mappingTo <= mappingFrom) || !covers(profileInterval, interval)) throw new Error("account mapping interval must be non-empty and covered by its profile");
    if (!group.memberships.some((membership) => membership.companySlug === companySlug && covers(membership, interval))) throw new Error("account mapping must be covered by an active group membership");
    return { id: id(mapping.id, `accountMappings[${index}].id`), companySlug, accountNo, reportingLineId, ...interval };
  }).sort((a, b) => compare(a.id, b.id));
  if (new Set(accountMappings.map((mapping) => mapping.id)).size !== accountMappings.length) throw new Error("account mapping ids must be unique");
  for (let i = 0; i < accountMappings.length; i += 1) for (let j = i + 1; j < accountMappings.length; j += 1) if (accountMappings[i]!.companySlug === accountMappings[j]!.companySlug && accountMappings[i]!.accountNo === accountMappings[j]!.accountNo && overlaps(accountMappings[i]!, accountMappings[j]!)) throw new Error("one company account may have only one active reporting-line mapping");
  return { version: 1, id: profileId, groupId, currency, reportingLines, accountMappings, evidenceRefs: refs(row.evidenceRefs, "profile.evidenceRefs"), ...profileInterval };
}

export function canonicalizeConsolidationProfile(profile: ConsolidationProfile): string { return JSON.stringify(profile); }
function eventHash(previous: string | null, event: Omit<ProfileEvent, "event_hash">): string { return sha(JSON.stringify({ previousHash: previous, id: event.id, profileId: event.profile_id, eventType: event.event_type, profileHash: event.profile_hash, canonicalProfile: event.canonical_profile, actor: event.actor, createdAt: event.created_at })); }
function readEvents(db: Database): ProfileEvent[] { const events = db.query("SELECT id,profile_id,event_type,profile_hash,canonical_profile,previous_hash,event_hash,actor,created_at FROM rm_consolidation_profile_events ORDER BY id").all() as ProfileEvent[]; let previous: string | null = null; for (const event of events) { if (event.previous_hash !== previous || event.event_hash !== eventHash(previous, event)) throw new Error("consolidation profile event hash-chain is invalid"); previous = event.event_hash; } return events; }
function current(events: readonly ProfileEvent[]): Map<string, ProfileEvent> { const result = new Map<string, ProfileEvent>(); for (const event of events) result.set(event.profile_id, event); return result; }
function decoded(event: ProfileEvent, groups: GroupManifest): ConsolidationProfile { const profile = parseConsolidationProfile(JSON.parse(event.canonical_profile), groups); if (profile.id !== event.profile_id || sha(event.canonical_profile) !== event.profile_hash) throw new Error("consolidation profile evidence is invalid"); return profile; }
function append(db: Database, type: ProfileEvent["event_type"], profile: ConsolidationProfile, audit: ResolveActorInput): ProfileEvent { const events = readEvents(db); const previous = events.at(-1)?.event_hash ?? null; const body = canonicalizeConsolidationProfile(profile); if (Buffer.byteLength(body, "utf8") > 524288) throw new Error("consolidation profile exceeds 524288 bytes"); const actor = resolveActor(audit).auditActor; const event = { id: (events.at(-1)?.id ?? 0) + 1, profile_id: profile.id, event_type: type, profile_hash: sha(body), canonical_profile: body, previous_hash: previous, actor, created_at: new Date().toISOString() }; const complete: ProfileEvent = { ...event, event_hash: eventHash(previous, event) }; db.query("INSERT INTO rm_consolidation_profile_events (id,profile_id,event_type,profile_hash,canonical_profile,previous_hash,event_hash,actor,created_at) VALUES (?,?,?,?,?,?,?,?,?)").run(complete.id,complete.profile_id,complete.event_type,complete.profile_hash,complete.canonical_profile,complete.previous_hash,complete.event_hash,complete.actor,complete.created_at); insertWorkspaceAudit(db,{...audit,eventType:`consolidation_profile_${type}`,entityType:"consolidation_profile",entityId:profile.id}); return complete; }

export function proposeConsolidationProfile(db: Database, workspaceRoot: string, input: unknown, audit: ResolveActorInput) { return db.transaction(() => { const groups = readCurrentGroupManifest(db, workspaceRoot); if (!groups) throw new Error("group structure is unavailable"); const profile = parseConsolidationProfile(input, groups.manifest); const state = current(readEvents(db)).get(profile.id); if (state && state.event_type !== "revoked") throw new Error("consolidation profile id already has an active lifecycle"); const event = append(db,"proposed",profile,audit); return { profileId: profile.id, profileHash: event.profile_hash, status: "proposed" as const }; }).immediate(); }
export function approveConsolidationProfile(db: Database, workspaceRoot: string, profileId: string, profileHash: string, audit: ResolveActorInput) { return db.transaction(() => { const groups = readCurrentGroupManifest(db,workspaceRoot); if (!groups) throw new Error("group structure is unavailable"); const event = current(readEvents(db)).get(profileId); if (!event || event.event_type !== "proposed" || event.profile_hash !== profileHash) throw new Error("exact pending consolidation profile was not found"); if (resolveActor(audit).auditActor === event.actor) throw new Error("consolidation profile approval requires a distinct reviewer"); const profile = decoded(event,groups.manifest); append(db,"approved",profile,audit); return { profileId,profileHash,status:"approved" as const }; }).immediate(); }
export function revokeConsolidationProfile(db: Database, workspaceRoot: string, profileId: string, audit: ResolveActorInput) { return db.transaction(() => { const groups=readCurrentGroupManifest(db,workspaceRoot); if(!groups) throw new Error("group structure is unavailable"); const event=current(readEvents(db)).get(profileId); if(!event||event.event_type!=="approved") throw new Error("approved consolidation profile was not found"); append(db,"revoked",decoded(event,groups.manifest),audit); return {profileId,status:"revoked" as const}; }).immediate(); }
export function readConsolidationProfileState(db: Database, workspaceRoot: string, profileId: string) { const groups=readCurrentGroupManifest(db,workspaceRoot); if(!groups) return null; const event=current(readEvents(db)).get(profileId); return event ? {profile:decoded(event,groups.manifest),profileHash:event.profile_hash,status:event.event_type,actor:event.actor} : null; }

/** Lists only active, approved profiles whose entire active group is visible. */
export function listAvailableConsolidationProfiles(
  db: Database,
  workspaceRoot: string,
  visibleCompanySlugs: ReadonlySet<string>,
  asOfInput: string,
) {
  const asOf = parseGroupAsOf(asOfInput);
  const groups = readCurrentGroupManifest(db, workspaceRoot);
  if (!groups) return { scope: "consolidation-report-profiles" as const, asOf, profiles: [] };
  const profiles = [...current(readEvents(db)).values()].flatMap((event) => {
    if (event.event_type !== "approved") return [];
    const profile = decoded(event, groups.manifest);
    const group = groups.manifest.groups.find((candidate) => candidate.id === profile.groupId);
    if (!group || !active(profile, asOf)) return [];
    const activeMembers = group.memberships.filter((membership) => active(membership, asOf));
    if (activeMembers.some((membership) => !visibleCompanySlugs.has(membership.companySlug))) return [];
    return [{
      id: profile.id,
      groupId: profile.groupId,
      currency: profile.currency,
      validFrom: profile.validFrom,
      validToExclusive: profile.validToExclusive,
    }];
  }).sort((a, b) => compare(a.id, b.id));
  return { scope: "consolidation-report-profiles" as const, asOf, profiles };
}

type LedgerAccountValue = { accountNo: string; accountType: string; ore: bigint; sourceHash: string };
function readLedger(workspaceRoot: string, companySlug: string, periodFrom: string, asOf: string): { currency: string; head: string | null; entries: number; accounts: Array<{ accountNo: string; accountType: string; active: boolean }>; pnl: LedgerAccountValue[]; cumulativeResult: LedgerAccountValue[]; balance: LedgerAccountValue[] } {
  const root=companyRootForSlug(workspaceRoot,companySlug); const db=openCurrentLedgerReadOnly(companyPaths(root).db);
  try { db.exec("PRAGMA query_only=ON; PRAGMA foreign_keys=ON"); assertSchemaCompatibility(db); const audit=verifyAuditChain(db,{companyRoot:root}); if(!audit.ok) throw new Error("ledger audit verification failed"); const company=db.query("SELECT currency FROM companies WHERE id=1").get() as {currency:string}|null; if(!company) throw new Error("company currency unavailable");
    const query=(from:string|null,types:string[]):LedgerAccountValue[]=>{ const typeBindings:SQLQueryBindings[]=[...types]; const dateClause=from?"AND je.transaction_date>=? AND je.transaction_date<=?":"AND je.transaction_date<=?"; const bindings:SQLQueryBindings[]=[...typeBindings,...(from?[from,asOf]:[asOf])]; const rows=db.query(`SELECT a.account_no,a.type,jl.id AS line_id,je.id AS entry_id,je.entry_hash,je.transaction_date,jl.debit_amount,jl.credit_amount FROM accounts a JOIN journal_lines jl ON jl.account_id=a.id JOIN journal_entries je ON je.id=jl.journal_entry_id WHERE a.type IN (${types.map(()=>"?").join(",")}) ${dateClause} ORDER BY je.id,jl.id`).all(...bindings) as Array<{account_no:string;type:string;line_id:number;entry_id:number;entry_hash:string;transaction_date:string;debit_amount:number;credit_amount:number}>; const by=new Map<string,{type:string;ore:bigint;refs:unknown[]}>(); for(const row of rows){const value=by.get(row.account_no)??{type:row.type,ore:0n,refs:[]}; value.ore+=toOre(row.debit_amount)-toOre(row.credit_amount); value.refs.push([row.entry_id,row.entry_hash,row.line_id,row.transaction_date,row.debit_amount,row.credit_amount]); by.set(row.account_no,value);} return [...by].map(([accountNo,value])=>({accountNo,accountType:value.type,ore:value.ore,sourceHash:sha(JSON.stringify({companySlug,accountNo,from,asOf,refs:value.refs}))})); };
    const head=db.query("SELECT entry_hash FROM journal_entries ORDER BY id DESC LIMIT 1").get() as {entry_hash:string}|null; const accounts=(db.query("SELECT account_no,type,active FROM accounts ORDER BY account_no").all() as Array<{account_no:string;type:string;active:number}>).map(account=>({accountNo:account.account_no,accountType:account.type,active:account.active===1})); return {currency:company.currency.trim().toUpperCase(),head:head?.entry_hash??null,entries:audit.entries,accounts,pnl:query(periodFrom,["income","expense"]),cumulativeResult:query(null,["income","expense"]),balance:query(null,["asset","liability","equity","vat"])};
  } finally {db.close();}
}

export type ConsolidatedReportResult = { scope:"consolidated-report"; profileId?:string; profileHash?:string; groupId?:string; period:{from:string;to:string}; currency?:string; status:"ready"|"blocked"; blockers:string[]; rawCompanySums:Array<{lineId:string;byCompany:Array<{companySlug:string;amount:number;sourceHashes:string[]}>;total:number}>; appliedEliminations:Array<{eliminationId:string;payloadHash:string;adjustments:Array<{lineId:string;amount:number}>}>; consolidatedFigures:null|Array<{lineId:string;label:string;section:ReportSection;rawCompanySum:number;eliminationAdjustment:number;consolidatedAmount:number}>; sourceSnapshots:Array<{companySlug:string;ledgerHeadHash:string|null;entryCount:number}> };

export function buildConsolidatedReport(db:Database,workspaceRoot:string,visibleCompanySlugs:ReadonlySet<string>,profileId:string,periodFromInput:string,asOfInput:string):ConsolidatedReportResult {
  const periodFrom=parseGroupAsOf(periodFromInput),asOf=parseGroupAsOf(asOfInput); if(periodFrom>asOf) throw new Error("report period must not be inverted"); const groups=readCurrentGroupManifest(db,workspaceRoot); const state=readConsolidationProfileState(db,workspaceRoot,profileId); if(!groups||!state||state.status!=="approved") throw new Error("approved consolidation profile was not found"); const profile=state.profile; const group=groups.manifest.groups.find(candidate=>candidate.id===profile.groupId)!; const activeMembers=group.memberships.filter(member=>active(member,asOf));
  // Deliberately return a redacted contract before exposing any profile, group,
  // company, account, elimination or ledger identity. Hosted users must never
  // learn hidden workspace topology through a report request.
  if(activeMembers.some(member=>!visibleCompanySlugs.has(member.companySlug))) return {scope:"consolidated-report",period:{from:periodFrom,to:asOf},status:"blocked",blockers:["all active group companies must be visible"],rawCompanySums:[],appliedEliminations:[],consolidatedFigures:null,sourceSnapshots:[]};
  const blockers:string[]=[]; if(!active(profile,asOf)) blockers.push("consolidation profile is not active at report date"); const companies=new Map(listWorkspaceCompanies(workspaceRoot).map(company=>[company.slug,company])); if(activeMembers.some(member=>companies.get(member.companySlug)?.archived)) blockers.push("active group company is archived");
  if(activeMembers.length>1){const incoming=new Map<string,number>(); for(const edge of group.ownership.filter(edge=>active(edge,asOf))) incoming.set(edge.childCompanySlug,(incoming.get(edge.childCompanySlug)??0)+edge.basisPoints); const roots=activeMembers.filter(member=>(incoming.get(member.companySlug)??0)===0); if(roots.length!==1||activeMembers.some(member=>member.companySlug!==roots[0]?.companySlug&&(incoming.get(member.companySlug)??0)!==10000)) blockers.push("first consolidated-report slice requires one root and 100 percent ownership of every child");}
  const ledgers=new Map<string,ReturnType<typeof readLedger>>(); if(blockers.length===0){for(const member of activeMembers){try{const ledger=readLedger(workspaceRoot,member.companySlug,periodFrom,asOf); ledgers.set(member.companySlug,ledger); if(ledger.currency!==profile.currency) blockers.push("all company functional currencies must equal profile currency");}catch{blockers.push("company ledger is unavailable or failed integrity validation");}}}
  const lineById=new Map(profile.reportingLines.map(line=>[line.id,line])); const activeMappings=profile.accountMappings.filter(mapping=>active(mapping,asOf)); const mappingByAccount=new Map(activeMappings.map(mapping=>[`${mapping.companySlug}\0${mapping.accountNo}`,mapping])); const byLine=new Map<string,Map<string,{ore:bigint;sourceHashes:string[]}>>(); const add=(lineId:string,companySlug:string,ore:bigint,sourceHash:string)=>{const companiesForLine=byLine.get(lineId)??new Map(); const value=companiesForLine.get(companySlug)??{ore:0n,sourceHashes:[]}; value.ore+=ore; value.sourceHashes.push(sourceHash); companiesForLine.set(companySlug,value); byLine.set(lineId,companiesForLine);};
  if(blockers.length===0){for(const mapping of activeMappings){const account=ledgers.get(mapping.companySlug)?.accounts.find(candidate=>candidate.accountNo===mapping.accountNo),line=lineById.get(mapping.reportingLineId)!; if(!account||!account.active){blockers.push("reporting profile references a missing or inactive source account");continue;} if(!(account.accountType===line.section||(account.accountType==="vat"&&(line.section==="asset"||line.section==="liability")))) blockers.push("reporting mapping is incompatible with source account type");}}
  if(blockers.length===0){for(const [companySlug,ledger] of ledgers){for(const value of [...ledger.pnl,...ledger.balance]){if(value.ore===0n)continue; const mapping=mappingByAccount.get(`${companySlug}\0${value.accountNo}`); if(!mapping){blockers.push("one or more non-zero company accounts lack an active reporting mapping");continue;} const line=lineById.get(mapping.reportingLineId)!; const compatible=value.accountType===line.section||(value.accountType==="vat"&&(line.section==="asset"||line.section==="liability")); if(!compatible){blockers.push("reporting mapping is incompatible with source account type");continue;} const natural=(line.section==="asset"||line.section==="expense")?value.ore:-value.ore; add(line.id,companySlug,natural,value.sourceHash);}}
    const currentResult=profile.reportingLines.find(line=>line.role==="current-result")!; for(const [companySlug,ledger] of ledgers){let result=0n; for(const value of ledger.cumulativeResult){const mapping=mappingByAccount.get(`${companySlug}\0${value.accountNo}`); if(value.ore!==0n&&!mapping){blockers.push("one or more cumulative result accounts lack an active reporting mapping");continue;} if(mapping) result-=value.ore;} add(currentResult.id,companySlug,result,sha(JSON.stringify({companySlug,period:"inception-to-asOf",asOf,sourceHashes:ledger.cumulativeResult.map(value=>value.sourceHash)})));}}
  const rawCompanySums=[...byLine].map(([lineId,rows])=>({lineId,byCompany:[...rows].map(([companySlug,value])=>({companySlug,amount:fromOre(value.ore),sourceHashes:value.sourceHashes.sort(compare)})).sort((a,b)=>compare(a.companySlug,b.companySlug)),total:fromOre([...rows.values()].reduce((sum,value)=>sum+value.ore,0n))})).sort((a,b)=>(lineById.get(a.lineId)?.displayOrder??0)-(lineById.get(b.lineId)?.displayOrder??0));
  const eliminationByLine=new Map<string,bigint>(); const appliedEliminations:Array<{eliminationId:string;payloadHash:string;adjustments:Array<{lineId:string;amount:number}>}>=[]; if(blockers.length===0){
    const reconciliationByMapping=new Map(buildIntercompanyReconciliation(db,workspaceRoot,visibleCompanySlugs,asOf).rows.filter(row=>row.mappingId).map(row=>[row.mappingId!,row]));
    for(const elimination of readAppliedBalanceEliminations(db,asOf)){
      if(elimination.payload.groupId!==profile.groupId) continue;
      const fresh=reconciliationByMapping.get(elimination.payload.mappingId);
      const evidenceMatches=fresh?.status==="matched"&&fresh.mappingHash===elimination.payload.mappingHash&&fresh.left&&fresh.right&&
        fresh.left.sourceSnapshot.selectionHash===elimination.payload.left.selectionHash&&fresh.left.sourceSnapshot.ledgerHeadHash===elimination.payload.left.ledgerHeadHash&&fresh.left.sourceSnapshot.entryCount===elimination.payload.left.entryCount&&
        fresh.right.sourceSnapshot.selectionHash===elimination.payload.right.selectionHash&&fresh.right.sourceSnapshot.ledgerHeadHash===elimination.payload.right.ledgerHeadHash&&fresh.right.sourceSnapshot.entryCount===elimination.payload.right.entryCount&&
        toOre(fresh.left.balance)===BigInt(elimination.payload.amountOre)&&toOre(fresh.right.balance)===BigInt(elimination.payload.amountOre);
      if(!evidenceMatches){blockers.push("applied elimination source snapshot changed; create and apply a new elimination");continue;}
      const leftLines=new Set(elimination.payload.left.accountNos.map(account=>mappingByAccount.get(`${elimination.payload.left.companySlug}\0${account}`)?.reportingLineId)); const rightLines=new Set(elimination.payload.right.accountNos.map(account=>mappingByAccount.get(`${elimination.payload.right.companySlug}\0${account}`)?.reportingLineId)); if(leftLines.size!==1||rightLines.size!==1||leftLines.has(undefined)||rightLines.has(undefined)){blockers.push("applied elimination accounts do not resolve to one reporting line per side");continue;} const leftLine=[...leftLines][0]!,rightLine=[...rightLines][0]!; if(lineById.get(leftLine)?.section!=="asset"||lineById.get(rightLine)?.section!=="liability"){blockers.push("balance elimination requires asset and liability reporting lines");continue;} const amount=-BigInt(elimination.payload.amountOre); eliminationByLine.set(leftLine,(eliminationByLine.get(leftLine)??0n)+amount); eliminationByLine.set(rightLine,(eliminationByLine.get(rightLine)??0n)+amount); appliedEliminations.push({eliminationId:elimination.eliminationId,payloadHash:elimination.payloadHash,adjustments:[{lineId:leftLine,amount:fromOre(amount)},{lineId:rightLine,amount:fromOre(amount)}]});
    }
  }
  let figures:null|ConsolidatedReportResult["consolidatedFigures"]=null; if(blockers.length===0){figures=profile.reportingLines.map(line=>{const raw=toOre(rawCompanySums.find(row=>row.lineId===line.id)?.total??0),adjustment=eliminationByLine.get(line.id)??0n;return{lineId:line.id,label:line.label,section:line.section,rawCompanySum:fromOre(raw),eliminationAdjustment:fromOre(adjustment),consolidatedAmount:fromOre(raw+adjustment)};}); const total=(section:ReportSection)=>figures!.filter(line=>line.section===section).reduce((sum,line)=>sum+toOre(line.consolidatedAmount),0n); if(total("asset")!==total("liability")+total("equity")) {blockers.push("consolidated balance does not satisfy assets equals liabilities plus equity");figures=null;}}
  return{scope:"consolidated-report",profileId:profile.id,profileHash:state.profileHash,groupId:profile.groupId,period:{from:periodFrom,to:asOf},currency:profile.currency,status:blockers.length?"blocked":"ready",blockers:[...new Set(blockers)],rawCompanySums,appliedEliminations,consolidatedFigures:figures,sourceSnapshots:[...ledgers].map(([companySlug,ledger])=>({companySlug,ledgerHeadHash:ledger.head,entryCount:ledger.entries}))};
}
