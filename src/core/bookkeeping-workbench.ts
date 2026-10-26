import { canonicalJson } from "./canonical-json";
/**
 * Read-only bookkeeping workbench (#591).
 *
 * This is deliberately a projection: bank_transactions and
 * bank_journal_reconciliations remain the to-do/completion truth, while the
 * existing deterministic bookkeeping batch remains the only route to apply.
 */
import { createHash } from "node:crypto";
import type { Database } from "bun:sqlite";
import { planBookkeepingBatch, type BookkeepingBatchItem } from "./bookkeeping-batch";
import { computePeriodCloseReadiness } from "./period-close-readiness";
import { documentResolution, type DocumentResolutionState } from "./document-party-links";

export type WorkbenchStatus = "ready" | "suggestedMatch" | "missingDocument" | "partyUnresolved" | "accountingDecisionRequired" | "vatEvidenceRequired" | "dimensionEvidenceRequired" | "stalePlan" | "applyFailed";
export type WorkbenchFilter = { from: string; to: string; status?: WorkbenchStatus; bankAccountId?: number; partyId?: string; documentQuality?: "matched" | "missing"; account?: string; vatTreatment?: string; dimension?: string; limit?: number; cursor?: number; search?: string };
export type WorkbenchRow = { bankTransactionId:number; date:string; text:string; amount:number; currency:string; bankAccount:{id:number|null;name:string|null}; document:{id:number;quality:"matched";party:{id:string;name:string}|null;resolutionState:DocumentResolutionState}|null; proposed:{account:string|null;vatTreatment:string|null;dimensions:Array<{dimensionId:string;memberId:string;status:"active"|"inactive"|"missing"}>;partyDefaults?:{account:string|null;vat:string|null;advisoryOnly:true}|null}; status:WorkbenchStatus; nextAction:string; drilldown:{documentId?:number;partyId?:string;bankTransactionId:number;bankAccountId?:number;runId?:number;journalEntryId?:number;periodClose:{from:string;to:string}}; sourceHash:string };
const canonical = canonicalJson;
const digest=(value:unknown)=>createHash("sha256").update(canonical(value)).digest("hex");
const iso=(value:string)=>/^\d{4}-\d{2}-\d{2}$/.test(value);
function companyId(db:Database){const rows=db.query("SELECT id FROM companies ORDER BY id").all() as Array<{id:number}>;if(rows.length!==1)throw new Error("selected ledger must contain exactly one company");return rows[0]!.id;}
function cutoff(db:Database){return (db.query("SELECT cut_over_date FROM opening_balances ORDER BY id DESC LIMIT 1").get() as {cut_over_date:string}|null)?.cut_over_date??null;}
function next(status:WorkbenchStatus){return ({ready:"Review the exact batch plan",suggestedMatch:"Review the suggested document match",missingDocument:"Attach or ingest source evidence",partyUnresolved:"Resolve the canonical party",accountingDecisionRequired:"Review account and posting rule",vatEvidenceRequired:"Provide VAT evidence",dimensionEvidenceRequired:"Activate or correct the required dimensions",stalePlan:"Create and review a fresh batch plan",applyFailed:"Read the existing batch run and resume its exact hash"} as const)[status];}
const workbenchStatuses:WorkbenchStatus[]=["ready","suggestedMatch","missingDocument","partyUnresolved","accountingDecisionRequired","vatEvidenceRequired","dimensionEvidenceRequired","stalePlan","applyFailed"];
const emptyCounts=()=>Object.fromEntries(workbenchStatuses.map(status=>[status,0]));
const relationExists=(db:Database,name:string)=>Boolean(db.query("SELECT 1 FROM sqlite_master WHERE type IN ('table','view') AND name=?").get(name));

/** A deterministic, side-effect-free page of unresolved bank work. */
export function buildBookkeepingWorkbench(db:Database,input:WorkbenchFilter){
  if(!iso(input.from)||!iso(input.to)||input.from>input.to)throw new Error("ordered ISO from and to dates are required");
  const limit=Math.max(1,Math.min(100,Math.trunc(input.limit??50))); const cursor=Math.max(0,Math.trunc(input.cursor??0));
  const required=["companies","opening_balances","bank_transactions","bank_accounts","bank_journal_reconciliations","bookkeeping_batch_runs","bookkeeping_batch_revisions","bookkeeping_batch_item_attempts","bookkeeping_batch_applied_links","documents","current_document_party_links","current_document_party_resolution_events","party_coverage_bank_resolution_events","current_accounting_dimension_members"];
  const missingRelations=required.filter(name=>!relationExists(db,name));
  if(missingRelations.length){
    const completeness={state:"unavailable" as const,reasonCode:"schema_not_current",nextAction:"Run the required schema migration before using bookkeeping workbench."};
    return {scope:{from:input.from,to:input.to,cutOverDate:null},state:completeness.state,completeness,rows:[],page:{cursor,limit,total:0,nextCursor:null},counts:emptyCounts(),population:{total:0,ready:0,blockers:0},selection:{total:0,ready:0,blockers:0},staleSources:[],sourceHash:digest({scope:{from:input.from,to:input.to},state:completeness.state}),periodClose:{status:"unavailable" as const,blockers:0},plan:null};
  }
  const cut=cutoff(db); const cid=companyId(db);
  const plan=planBookkeepingBatch(db,{companyId:cid,accountingFrom:input.from,accountingTo:input.to,bankFrom:input.from,bankTo:input.to});
  const byBank=new Map<number,BookkeepingBatchItem>(); for(const item of plan.items)if(item.bankTransactionId)byBank.set(item.bankTransactionId,item);
  const staleBanks=new Set<number>(); const failedBanks=new Map<number,number>();
  const staleSources:Array<{bankTransactionId:number|null;actionKey:string;changedSource:"evidence_changed"|"added_to_candidate_set"|"removed_from_candidate_set"}>=[];
  // A saved run is stale only when its recorded candidate universe differs
  // from the current one. This names the affected canonical bank rows without
  // inventing a task state.
  const active=db.query(`SELECT r.id,r.candidate_set_hash,r.plan_json FROM bookkeeping_batch_revisions r
    JOIN bookkeeping_batch_runs b ON b.id=r.run_id
    WHERE b.accounting_from=? AND b.accounting_to=? AND b.bank_from=? AND b.bank_to=?
    ORDER BY r.id DESC LIMIT 1`).get(input.from,input.to,input.from,input.to) as {id:number;candidate_set_hash:string;plan_json:string}|null;
  let revisionStateReadable=true;
  if(active&&active.candidate_set_hash!==plan.candidateSetHash)try{const saved=JSON.parse(active.plan_json) as {items?:BookkeepingBatchItem[]};if(!Array.isArray(saved.items))throw new Error("invalid plan items");const savedBanks=new Set<number>();for(const item of saved.items){if(!item.bankTransactionId)continue;savedBanks.add(item.bankTransactionId);const current=byBank.get(item.bankTransactionId);if(current&&current.evidenceHash!==item.evidenceHash){staleBanks.add(item.bankTransactionId);staleSources.push({bankTransactionId:item.bankTransactionId,actionKey:item.actionKey,changedSource:"evidence_changed"});}else if(!current)staleSources.push({bankTransactionId:item.bankTransactionId,actionKey:item.actionKey,changedSource:"removed_from_candidate_set"});}for(const [bankTransactionId,item] of byBank)if(!savedBanks.has(bankTransactionId)){staleBanks.add(bankTransactionId);staleSources.push({bankTransactionId,actionKey:item.actionKey,changedSource:"added_to_candidate_set"});}}catch{revisionStateReadable=false;}
  // Item attempts retain failed recovery evidence and do not require a second store.
  for(const row of db.query("SELECT i.action_key,r.id AS run_id FROM bookkeeping_batch_item_attempts i JOIN bookkeeping_batch_runs r ON r.id=i.run_id WHERE i.outcome='failed' ORDER BY i.id DESC").all() as Array<{action_key:string;run_id:number}>){const match=/bank:(\d+)$/.exec(row.action_key);if(match&&!failedBanks.has(Number(match[1])))failedBanks.set(Number(match[1]),row.run_id);}
  const base=db.query(`SELECT bt.id,bt.transaction_date,bt.text,bt.amount,bt.currency,bt.bank_account_id,ba.name AS bank_name
    FROM bank_transactions bt LEFT JOIN bank_accounts ba ON ba.id=bt.bank_account_id
    WHERE bt.transaction_date BETWEEN ? AND ? AND NOT EXISTS(SELECT 1 FROM bank_journal_reconciliations r WHERE r.bank_transaction_id=bt.id)
    AND (? IS NULL OR bt.transaction_date>=?)
    ORDER BY bt.transaction_date,bt.id`).all(input.from,input.to,cut,cut) as Array<{id:number;transaction_date:string;text:string;amount:number;currency:string;bank_account_id:number|null;bank_name:string|null}>;
  const needle=input.search?.trim().toLowerCase();
  const rows:WorkbenchRow[]=[];
  for(const bank of base){const item=byBank.get(bank.id);let status:WorkbenchStatus="missingDocument";let document:WorkbenchRow["document"]=null;let proposed:WorkbenchRow["proposed"]={account:null,vatTreatment:null,dimensions:[]};
    if(item?.documentId){const d=db.query("SELECT id FROM documents WHERE id=?").get(item.documentId) as {id:number}|null;const link=d?db.query(`SELECT party_id,party_snapshot_json FROM current_document_party_links WHERE document_id=? ORDER BY CASE party_role WHEN 'supplier' THEN 0 WHEN 'vendor' THEN 1 WHEN 'issuer' THEN 2 WHEN 'establishment' THEN 10 WHEN 'location' THEN 11 WHEN 'payment_descriptor' THEN 12 ELSE 20 END,id DESC LIMIT 1`).get(d.id) as {party_id:string;party_snapshot_json:string}|null:null;let party:null|{id:string;name:string}=null;if(link)try{const snapshot=JSON.parse(link.party_snapshot_json) as {name?:unknown};party={id:link.party_id,name:typeof snapshot.name==="string"?snapshot.name:link.party_id};}catch{party={id:link.party_id,name:link.party_id};}const resolutionState=d?documentResolution(db,d.id).state:"unresolved" as const;document=d?{id:d.id,quality:"matched",party,resolutionState}:null;const detail=item.detail as any;const requested=Object.entries(detail?.rule?.outcome?.dimensions??{}) as Array<[string,unknown]>;const defaults=detail?.partyDefaults;proposed={account:typeof detail?.rule?.outcome?.account==="string"?detail.rule.outcome.account:null,vatTreatment:typeof detail?.rule?.outcome?.vatTreatment==="string"?detail.rule.outcome.vatTreatment:null,partyDefaults:defaults&&typeof defaults==="object"?{account:typeof defaults.account==="string"?defaults.account:null,vat:typeof defaults.vat==="string"?defaults.vat:null,advisoryOnly:true}:null,dimensions:requested.map(([dimensionId,member])=>{const row=db.query("SELECT status FROM current_accounting_dimension_members WHERE dimension_id=? AND member_id=?").get(dimensionId,String(member)) as {status:"active"|"inactive"}|null;return {dimensionId,memberId:String(member),status:row?.status??"missing"};})};if(resolutionState!=="resolved"&&resolutionState!=="internal_no_external_party")status="partyUnresolved";else if(proposed.dimensions.some(dimension=>dimension.status!=="active"))status="dimensionEvidenceRequired";else if(detail?.vatReady===false)status="vatEvidenceRequired";else if(item.partition==="ready")status="ready";else status="accountingDecisionRequired";
    } else if(item?.partition==="suggestedMatch") status="suggestedMatch";
    if(staleBanks.has(bank.id))status="stalePlan"; if(failedBanks.has(bank.id))status="applyFailed";
    const applied=db.query("SELECT journal_entry_id FROM bookkeeping_batch_applied_links WHERE bank_transaction_id=? ORDER BY id DESC LIMIT 1").get(bank.id) as {journal_entry_id:number}|null;
    const row:WorkbenchRow={bankTransactionId:bank.id,date:bank.transaction_date,text:bank.text,amount:Number(bank.amount),currency:bank.currency,bankAccount:{id:bank.bank_account_id,name:bank.bank_name},document,proposed,status,nextAction:next(status),drilldown:{bankTransactionId:bank.id,...(bank.bank_account_id?{bankAccountId:bank.bank_account_id}:{}),...(document?{documentId:document.id}:{}),...(document?.party?{partyId:document.party.id}:{}),...(failedBanks.has(bank.id)?{runId:failedBanks.get(bank.id)!}:{}),...(applied?{journalEntryId:applied.journal_entry_id}:{}),periodClose:{from:input.from,to:input.to}},sourceHash:digest({bank,item,document,proposed,status})};
    // `rows` is the canonical population. Facet filters are applied only to
    // the returned page so its readiness/blocker counts never lie.
    rows.push(row);
  }
  const populationRows=rows;
  const filtered=populationRows.filter(row=>{
    const text=`${row.text} ${row.amount} ${row.currency} ${row.document?.party?.name??""} ${row.proposed.account??""}`.toLowerCase();
    return (!input.status||row.status===input.status)&&(!input.bankAccountId||row.bankAccount.id===input.bankAccountId)&&(!needle||text.includes(needle))&&(!input.partyId||row.document?.party?.id===input.partyId)&&(!input.documentQuality||((row.document?.quality??"missing")===input.documentQuality))&&(!input.account||row.proposed.account===input.account)&&(!input.vatTreatment||row.proposed.vatTreatment===input.vatTreatment)&&(!input.dimension||row.proposed.dimensions.some(dimension=>`${dimension.dimensionId}:${dimension.memberId}`===input.dimension));
  });
  const page=filtered.slice(cursor,cursor+limit);
  let periodClose:null|{status:"available"|"unavailable";blockers:number;hash?:string}=null;
  try { const packet=computePeriodCloseReadiness(db,{periodStart:input.from,periodEnd:input.to,cutoff:input.to});periodClose={status:"available",blockers:packet.blockers,hash:packet.hash}; } catch { periodClose={status:"unavailable",blockers:0}; }
  const counts=Object.fromEntries(workbenchStatuses.map(status=>[status,populationRows.filter(row=>row.status===status).length]));
  const ready=Number(counts.ready??0), blockers=populationRows.length-ready;
  const selectedReady=filtered.filter(row=>row.status==="ready").length;
  const completeness=!revisionStateReadable?{state:"incomplete" as const,reasonCode:"reviewed_plan_unreadable",nextAction:"Inspect the latest reviewed batch revision before relying on this workbench."}:staleSources.length?{state:"incomplete" as const,reasonCode:"canonical_state_changed",nextAction:"Review only the named changed sources and create a fresh exact batch plan."}:periodClose?.status==="unavailable"?{state:"incomplete" as const,reasonCode:"period_close_unavailable",nextAction:"Period-close readiness is unavailable; repair its named control before relying on a complete workbench."}:base.length===0?{state:"zero" as const,reasonCode:"no_unresolved_bank_work",nextAction:"No unresolved bank work exists in this scope."}:{state:"available" as const,reasonCode:"complete",nextAction:"Review the bounded next action for each row."};
  return {scope:{from:input.from,to:input.to,cutOverDate:cut},state:completeness.state,completeness,rows:page,page:{cursor,limit,total:filtered.length,nextCursor:cursor+limit<filtered.length?cursor+limit:null},counts,population:{total:populationRows.length,ready,blockers},selection:{total:filtered.length,ready:selectedReady,blockers:filtered.length-selectedReady},staleSources:staleSources.slice(0,100),sourceHash:digest({scope:{from:input.from,to:input.to,cut},filter:{status:input.status??null,bankAccountId:input.bankAccountId??null,partyId:input.partyId??null,documentQuality:input.documentQuality??null,account:input.account??null,vatTreatment:input.vatTreatment??null,dimension:input.dimension??null,search:needle??null},rows:filtered.map(r=>r.sourceHash)}),periodClose,plan:{planHash:plan.planHash,candidateSetHash:plan.candidateSetHash,readyCount:plan.items.filter(item=>item.partition==="ready").length}};
}
