import { createHash } from "node:crypto";
import type { Database } from "bun:sqlite";
import { canonicalJson } from "./canonical-json";
import { applyDocumentPartyLink, planDocumentPartyLink, type DocumentPartyLinkPlanInput, type DocumentPartyRole } from "./document-party-links";
import { inspectParty } from "./party-registry";
import { resolveSupplierIdentity } from "./supplier-identity";

export type PartyCoverageStatus = "linked" | "resolved_no_external_party" | "exact_candidate" | "ambiguous" | "missing_source";
export type PartyCoverageFilter = { companySlug:string; asOf?:string; bankAccountId?:number };
export type PartyCoverageDecision = { bankTransactionId:number; partyId?:string; role?:DocumentPartyRole; noExternalParty?:boolean; evidenceReference:string; rationale:string };

const sha=(value:unknown)=>createHash("sha256").update(canonicalJson(value)).digest("hex");
const bounded=(value:unknown,max=1000)=>typeof value==="string"&&value.trim()&&value.trim().length<=max?value.trim():null;
const roleOrder:DocumentPartyRole[]=["vendor","supplier","customer","recipient","payee","payer","bank","related_company","processor","authority"];

function exactDocumentOperations(rows: Array<any>) {
  const documents=new Set<number>(); const operations:any[]=[];
  for(const row of rows)if(row.status==="exact_candidate"&&row.documentId&&!documents.has(row.documentId)){documents.add(row.documentId);operations.push({actionKey:`document:${row.documentId}`,kind:"document_link",documentId:row.documentId,input:row.candidate.input,documentPlanHash:row.candidate.documentPlanHash});}
  return operations;
}

function sourceDocumentIds(db:Database,bankTransactionId:number,journalDocumentId:number|null):number[]{
  const ids=new Set<number>(); if(journalDocumentId)ids.add(journalDocumentId);
  for(const row of db.query(`SELECT p.document_id AS id FROM payable_payments payment JOIN payables p ON p.id=payment.payable_id WHERE payment.bank_transaction_id=?
    UNION SELECT invoice_document_id AS id FROM invoice_payments WHERE bank_transaction_id=?`).all(bankTransactionId,bankTransactionId) as Array<{id:number}>)ids.add(row.id);
  return [...ids].sort((a,b)=>a-b);
}

function candidateForDocument(db:Database,registry:Database,companySlug:string,documentId:number){
  const doc=db.query(`SELECT id,sha256_hash,document_type,sender_name,sender_vat_cvr,supplier_country_code,supplier_identifier_kind
    FROM documents WHERE id=?`).get(documentId) as any; if(!doc)return {kind:"missing" as const};
  const links=db.query("SELECT party_id,party_role,evidence_kind,plan_hash FROM current_document_party_links WHERE document_id=? ORDER BY party_role,party_id").all(documentId) as any[];
  if(links.length)return {kind:"linked" as const,document:doc,links};
  if(db.query("SELECT 1 FROM current_document_party_resolution_events WHERE document_id=? AND state='internal_no_external_party'").get(documentId))return {kind:"no_external" as const,document:doc};
  const inputs:DocumentPartyLinkPlanInput[]=[];
  if(doc.supplier_country_code&&doc.supplier_identifier_kind&&doc.sender_vat_cvr){
    const identity=resolveSupplierIdentity({country:doc.supplier_country_code,identifierKind:doc.supplier_identifier_kind,identifier:doc.sender_vat_cvr});
    if(identity.ok&&identity.identifier){
      const rows=registry.query("SELECT party_id FROM rm_party_identifiers WHERE jurisdiction=? AND identifier_kind=? AND identifier=?").all(identity.country,identity.identifierKind,identity.identifier) as Array<{party_id:string}>;
      for(const row of rows){const party=inspectParty(registry,row.party_id);for(const role of roleOrder)if(party?.roles.some((item:any)=>item.companySlug===companySlug&&item.role===role))inputs.push({documentId,companySlug,partyId:row.party_id,role,jurisdiction:identity.country,identifierKind:identity.identifierKind,identifier:identity.identifier});}
    }
  }
  const vendorIds=(db.query(`SELECT vendor_id FROM payables WHERE document_id=? AND vendor_id IS NOT NULL
    UNION SELECT vendor_id FROM document_vendor_identity_links WHERE document_id=? AND vendor_id IS NOT NULL`).all(documentId,documentId) as Array<{vendor_id:number}>).map(row=>String(row.vendor_id));
  for(const legacyId of vendorIds){const mapping=registry.query("SELECT party_id,party_role,evidence_json FROM current_legacy_party_mappings WHERE company_slug=? AND legacy_kind='vendor' AND legacy_id=?").get(companySlug,legacyId) as any;if(mapping){try{const evidence=JSON.parse(mapping.evidence_json);if(bounded(evidence.reviewedLegacyReference,500))inputs.push({documentId,companySlug,partyId:mapping.party_id,role:mapping.party_role,legacyKind:"vendor",legacyId,reviewedLegacyReference:evidence.reviewedLegacyReference});}catch{}}}
  const unique=[...new Map(inputs.map(input=>[`${input.partyId}:${input.role}`,input])).values()];
  if(unique.length===1){const planned=planDocumentPartyLink(db,registry,unique[0]!);if(planned.ok)return {kind:"candidate" as const,document:doc,input:unique[0]!,plan:planned.plan};}
  if(unique.length>1)return {kind:"ambiguous" as const,document:doc,candidates:unique.map(input=>({partyId:input.partyId,role:input.role,provenance:input.legacyKind?"reviewed_legacy_mapping":"typed_identifier"}))};
  const names=bounded(doc.sender_name,320)?registry.query(`SELECT DISTINCT e.party_id FROM rm_party_events e JOIN rm_party_company_roles role ON role.party_id=e.party_id
    WHERE e.event_type='created' AND role.company_slug=? AND lower(trim(json_extract(e.canonical_payload,'$.name')))=lower(?)`).all(companySlug,doc.sender_name.trim()) as Array<{party_id:string}>:[];
  return names.length>1?{kind:"ambiguous" as const,document:doc,candidates:names.map(row=>({partyId:row.party_id,provenance:"name_collision_not_evidence"}))}:{kind:"missing" as const,document:doc};
}

export function projectPartyCoverage(db:Database,registry:Database,input:PartyCoverageFilter){
  const rows=db.query(`SELECT bt.id AS bank_transaction_id,bt.transaction_hash,bt.transaction_date,bt.amount,bt.currency,bt.bank_account_id,
    r.reconciliation_id,r.journal_entry_id,j.entry_hash,j.document_id FROM bank_transactions bt
    LEFT JOIN bank_journal_reconciliations r ON r.bank_transaction_id=bt.id LEFT JOIN journal_entries j ON j.id=r.journal_entry_id
    WHERE (? IS NULL OR bt.transaction_date<=?) AND (? IS NULL OR bt.bank_account_id=?) ORDER BY bt.id`).all(input.asOf??null,input.asOf??null,input.bankAccountId??null,input.bankAccountId??null) as any[];
  const projected=rows.map(row=>{
    const base={bankTransactionId:row.bank_transaction_id,transactionHash:row.transaction_hash,transactionDate:row.transaction_date,amount:row.amount,currency:row.currency,bankAccountId:row.bank_account_id,reconciliation:row.reconciliation_id?{id:row.reconciliation_id,journalEntryId:row.journal_entry_id,journalEntryHash:row.entry_hash}:null};
    if(!row.reconciliation_id)return {...base,status:"missing_source" as const,documentId:null,candidate:null,reason:"Bank transaction has no current reconciliation.",nextAction:"Reconcile the bank transaction."};
    const decision=db.query("SELECT resolution_type,party_id,party_role,evidence_reference,plan_hash,transaction_hash,journal_entry_hash,reconciliation_id FROM party_coverage_bank_resolution_events WHERE bank_transaction_id=?").get(row.bank_transaction_id) as any;
    if(decision){if(decision.transaction_hash!==row.transaction_hash||decision.journal_entry_hash!==row.entry_hash||decision.reconciliation_id!==row.reconciliation_id)return {...base,status:"ambiguous" as const,documentId:null,candidate:null,reason:"Stored bank-party decision no longer matches current source hashes.",nextAction:"Review and correct the stale decision."};return {...base,status:decision.resolution_type==="linked"?"linked" as const:"resolved_no_external_party" as const,documentId:null,candidate:decision.party_id?{partyId:decision.party_id,role:decision.party_role,provenance:"reviewed_bank_journal_decision",planHash:decision.plan_hash}:null,reason:"Resolved by an append-only bank/journal decision.",nextAction:null};}
    const documentIds=sourceDocumentIds(db,row.bank_transaction_id,row.document_id); if(documentIds.length!==1)return {...base,status:documentIds.length>1?"ambiguous" as const:"missing_source" as const,documentId:null,candidate:null,reason:documentIds.length>1?"Several source documents resolve from the same bank chain.":"The reconciled journal has no source document or reviewed party decision.",nextAction:documentIds.length>1?"Review the conflicting source chain.":"Record a hash-bound bank/journal party decision."};
    const resolved=candidateForDocument(db,registry,input.companySlug,documentIds[0]!);
    if(resolved.kind==="linked")return {...base,status:"linked" as const,documentId:documentIds[0],documentHash:resolved.document.sha256_hash,candidate:{links:resolved.links,provenance:"current_document_party_links"},reason:"Document already has a current canonical party link.",nextAction:null};
    if(resolved.kind==="no_external")return {...base,status:"resolved_no_external_party" as const,documentId:documentIds[0],documentHash:resolved.document.sha256_hash,candidate:null,reason:"Document has a current reviewed no-external-party decision.",nextAction:null};
    if(resolved.kind==="candidate")return {...base,status:"exact_candidate" as const,documentId:documentIds[0],documentHash:resolved.document.sha256_hash,candidate:{partyId:resolved.input.partyId,role:resolved.input.role,provenance:resolved.input.legacyKind?"reviewed_legacy_mapping":"typed_identifier",documentPlanHash:resolved.plan.planHash,input:resolved.input},reason:"One deterministic existing party workflow resolves this document.",nextAction:"Review and apply the exact batch plan."};
    return {...base,status:resolved.kind==="ambiguous"?"ambiguous" as const:"missing_source" as const,documentId:documentIds[0],documentHash:resolved.document?.sha256_hash??null,candidate:resolved.kind==="ambiguous"?{candidates:resolved.candidates}:null,reason:resolved.kind==="ambiguous"?"Several candidates or a name collision require review.":"No deterministic identifier or reviewed mapping exists.",nextAction:"Use the source-bound review flow or leave unresolved."};
  });
  const totals={linked:0,resolved_no_external_party:0,exact_candidate:0,ambiguous:0,missing_source:0};for(const row of projected)totals[row.status]++;
  const identity={filter:{asOf:input.asOf??null,bankAccountId:input.bankAccountId??null},rows:projected};
  const populationHash=sha(identity); const operations=exactDocumentOperations(projected);
  const planPayload={filter:{companySlug:input.companySlug,asOf:input.asOf??null,bankAccountId:input.bankAccountId??null},populationHash,operations};
  return {ok:true as const,rows:projected,totals,uniqueDocuments:new Set(projected.map(row=>row.documentId).filter(Boolean)).size,populationHash,planHash:sha(planPayload)};
}

export function planPartyCoverage(db:Database,registry:Database,input:PartyCoverageFilter&{decisions?:PartyCoverageDecision[]}){
  const projection=projectPartyCoverage(db,registry,input); const operations=exactDocumentOperations(projection.rows);
  const decided=new Set<number>();for(const decision of input.decisions??[]){if(decided.has(decision.bankTransactionId))throw new Error("one bank transaction can have only one reviewed decision");decided.add(decision.bankTransactionId);const row=projection.rows.find(item=>item.bankTransactionId===decision.bankTransactionId);const reference=bounded(decision.evidenceReference,500),rationale=bounded(decision.rationale,1000);if(!row?.reconciliation||row.documentId||row.status!=="missing_source"||!reference||!rationale)throw new Error("bank decision requires one reconciled missing-source row and bounded evidence");const noExternal=decision.noExternalParty===true;if(noExternal===Boolean(decision.partyId))throw new Error("bank decision requires exactly one party or noExternalParty");if(noExternal&&registry.query("SELECT 1 FROM rm_intercompany_disposition_journal_links WHERE company_slug=? AND journal_entry_id=? LIMIT 1").get(input.companySlug,row.reconciliation.journalEntryId))throw new Error("an intercompany journal requires a related_company party decision");if(!noExternal){const party=inspectParty(registry,decision.partyId!);if(!party||!decision.role||!party.roles.some((role:any)=>role.companySlug===input.companySlug&&role.role===decision.role))throw new Error("bank decision party role is not visible in the company");}operations.push({actionKey:`bank:${row.bankTransactionId}`,kind:"bank_decision",bankTransactionId:row.bankTransactionId,transactionHash:row.transactionHash,reconciliation:row.reconciliation,resolutionType:noExternal?"no_external":"linked",partyId:decision.partyId??null,role:decision.role??null,evidenceReference:reference,rationale});}
  operations.sort((a,b)=>a.actionKey.localeCompare(b.actionKey)); const payload={filter:{companySlug:input.companySlug,asOf:input.asOf??null,bankAccountId:input.bankAccountId??null},populationHash:projection.populationHash,operations};
  return {ok:true as const,plan:{...payload,planHash:sha(payload)},projection};
}

export function applyPartyCoverage(db:Database,registry:Database,companyRoot:string,input:PartyCoverageFilter&{decisions?:PartyCoverageDecision[];planHash:string;idempotencyKey:string;confirm:boolean;actor:string;principal:string}){
  if(!input.confirm)throw new Error("confirmation required");if(!/^(user|agent|system):\S+$/.test(input.actor)||!bounded(input.principal,200)||!bounded(input.idempotencyKey,128))throw new Error("actor, authenticated principal and idempotencyKey are required");const keyHash=sha(input.idempotencyKey);
  return db.transaction(()=>{const prior=db.query("SELECT id,plan_hash,result_json FROM party_coverage_batch_events WHERE principal=? AND idempotency_key_hash=?").get(input.principal,keyHash) as any;if(prior){if(prior.plan_hash!==input.planHash)throw new Error("idempotency key already binds another plan");return {...JSON.parse(prior.result_json),idempotent:true};}
    const planned=planPartyCoverage(db,registry,input);if(planned.plan.planHash!==input.planHash)throw new Error("stale party coverage plan");const results:any[]=[];
    for(const operation of planned.plan.operations){if(operation.kind==="document_link"){const applied=applyDocumentPartyLink(db,registry,{...operation.input,planHash:operation.documentPlanHash,confirm:true,actor:input.actor,principal:input.principal,idempotencyKey:`coverage:${input.planHash}:${operation.actionKey}`},companyRoot);if(!applied.ok)throw new Error(applied.errors.join(","));results.push({actionKey:operation.actionKey,eventId:applied.id});}else{const row=db.query("INSERT INTO party_coverage_bank_resolution_events(bank_transaction_id,reconciliation_id,journal_entry_id,transaction_hash,journal_entry_hash,resolution_type,party_id,party_role,evidence_reference,rationale,plan_hash,actor,principal,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?) RETURNING id").get(operation.bankTransactionId,operation.reconciliation.id,operation.reconciliation.journalEntryId,operation.transactionHash,operation.reconciliation.journalEntryHash,operation.resolutionType,operation.partyId,operation.role,operation.evidenceReference,operation.rationale,input.planHash,input.actor,input.principal,new Date().toISOString()) as {id:number};results.push({actionKey:operation.actionKey,eventId:row.id});}}
    const result={ok:true as const,idempotent:false,applied:results.length,results,planHash:input.planHash,populationHash:planned.plan.populationHash};db.query("INSERT INTO party_coverage_batch_events(plan_hash,population_hash,plan_json,result_json,idempotency_key_hash,actor,principal,created_at) VALUES(?,?,?,?,?,?,?,?)").run(input.planHash,planned.plan.populationHash,canonicalJson(planned.plan),canonicalJson(result),keyHash,input.actor,input.principal,new Date().toISOString());return result;
  }).immediate();
}
