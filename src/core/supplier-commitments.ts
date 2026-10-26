import { canonicalJson } from "./canonical-json";
/** Reviewed supplier commitments are planning evidence, never invoices,
 * payables, payments or ledger postings. */
import { createHash } from "node:crypto";
import type { Database } from "bun:sqlite";
import { addDays, isValidIsoDate } from "./dates";
import { addMonths } from "./recurring-invoices";

type Status = "active"|"paused"|"ended"|"unresolved";
type Frequency = "weekly"|"monthly"|"quarterly"|"yearly";
export type CommitmentInput = { commitmentId:string; vendorPartyId:string; vendorSnapshot?:string; type:string; description:string; businessPurpose:string; amount:number; currency:string; frequency:Frequency; nextDate:string; startDate?:string; renewalDate?:string; noticeDate?:string; endDate?:string; evidenceRefs:string[]; vatProposal?:string; status?:Status };
export type CommitmentProposal = Omit<CommitmentInput,"vendorSnapshot"|"status"|"startDate"|"renewalDate"|"noticeDate"|"endDate"|"vatProposal"> & { status:Status; vendorSnapshot:string|null; startDate:string|null; renewalDate:string|null; noticeDate:string|null; endDate:string|null; vatProposal:string|null; payloadHash:string; schedule:Array<{date:string; amount:number; currency:string}>; warnings:string[] };
const canonical = canonicalJson;
const digest=(v:unknown)=>createHash("sha256").update(canonical(v)).digest("hex");
const validText=(v:unknown,max=500)=>typeof v==="string"&&v.trim().length>0&&v.trim().length<=max;
const validFrequency=(v:unknown):v is Frequency=>v==="weekly"||v==="monthly"||v==="quarterly"||v==="yearly";
const next=(date:string, frequency:Frequency)=>frequency==="weekly"?addDays(date,7):addMonths(date,frequency==="monthly"?1:frequency==="quarterly"?3:12);

export function planSupplierCommitment(input:CommitmentInput, horizonDays=91):{ok:true;proposal:CommitmentProposal}|{ok:false;errors:string[]} {
  const errors:string[]=[];
  if(!validText(input.commitmentId,96)||!validText(input.vendorPartyId,96)) errors.push("commitmentId and canonical vendorPartyId are required");
  if(!validText(input.type)||!validText(input.description,2000)||!validText(input.businessPurpose,2000)) errors.push("type, description and businessPurpose are required");
  if(!Number.isFinite(input.amount)||input.amount<=0) errors.push("amount must be positive");
  if(!/^[A-Z]{3}$/.test(input.currency)) errors.push("currency must be ISO-4217 uppercase");
  if(!validFrequency(input.frequency)||!isValidIsoDate(input.nextDate)) errors.push("frequency and nextDate are invalid");
  if(!Array.isArray(input.evidenceRefs)||input.evidenceRefs.length===0||input.evidenceRefs.some(r=>!validText(r,500))) errors.push("at least one source evidence reference is required");
  for(const d of [input.startDate,input.renewalDate,input.noticeDate,input.endDate]) if(d!==undefined&&!isValidIsoDate(d)) errors.push("commitment dates must be YYYY-MM-DD");
  if(errors.length) return {ok:false,errors};
  const status=input.status??"active"; const schedule:CommitmentProposal["schedule"]=[]; let cursor=input.nextDate;
  const until=addDays(input.nextDate,horizonDays);
  while(cursor<=until){ if(!input.endDate||cursor<=input.endDate) schedule.push({date:cursor,amount:input.amount,currency:input.currency}); cursor=next(cursor,input.frequency); }
  const {commitmentId,vendorPartyId,vendorSnapshot,type,description,businessPurpose,amount,currency,frequency,nextDate,startDate,renewalDate,noticeDate,endDate,evidenceRefs,vatProposal}=input;
  const payload={commitmentId,vendorPartyId,status,vendorSnapshot:vendorSnapshot??null,type,description,businessPurpose,amount,currency,frequency,nextDate,startDate:startDate??null,renewalDate:renewalDate??null,noticeDate:noticeDate??null,endDate:endDate??null,evidenceRefs,vatProposal:vatProposal??null};
  return {ok:true,proposal:{...payload,payloadHash:digest(payload),schedule,warnings:["VAT treatment is a proposal only; invoice evidence and VAT preflight remain required."]}};
}
export function applySupplierCommitment(db:Database,input:CommitmentInput & {payloadHash:string;confirm:boolean;actor?:string;principal?:string;idempotencyKey?:string}){
  if(!input.confirm) return {ok:false,errors:["CONFIRMATION_REQUIRED"]}; if(!validText(input.actor,160)||!validText(input.principal,160)) return {ok:false,errors:["ACTOR_AND_PRINCIPAL_REQUIRED"]};
  const plan=planSupplierCommitment(input); if(!plan.ok)return plan; if(plan.proposal.payloadHash!==input.payloadHash)return {ok:false,errors:["PLAN_HASH_MISMATCH"]};
  const existing=input.idempotencyKey?db.query("SELECT payload_hash FROM supplier_commitment_events WHERE idempotency_key=?").get(input.idempotencyKey) as any:null;
  if(existing&&existing.payload_hash!==input.payloadHash)return {ok:false,errors:["IDEMPOTENCY_CONFLICT"]}; if(existing)return {ok:true,idempotent:true,commitmentId:input.commitmentId,payloadHash:input.payloadHash};
  const same=db.query("SELECT 1 FROM supplier_commitment_events WHERE commitment_id=? AND event_type='approved' AND payload_hash=?").get(input.commitmentId,input.payloadHash); if(same)return {ok:true,idempotent:true,commitmentId:input.commitmentId,payloadHash:input.payloadHash};
  db.query("INSERT INTO supplier_commitment_events(commitment_id,event_type,payload_json,payload_hash,idempotency_key,actor,principal,created_at) VALUES(?,?,?,?,?,?,?,?)").run(input.commitmentId,"approved",canonical(plan.proposal),input.payloadHash,input.idempotencyKey??null,input.actor!,input.principal!,new Date().toISOString());
  return {ok:true,idempotent:false,commitmentId:input.commitmentId,payloadHash:input.payloadHash};
}
export function changeSupplierCommitment(db:Database,input:{commitmentId:string;action:"paused"|"ended"|"superseded";reason:string;confirm:boolean;actor?:string;principal?:string}){
 if(!input.confirm)return {ok:false,errors:["CONFIRMATION_REQUIRED"]};if(!validText(input.actor,160)||!validText(input.principal,160)||!validText(input.reason,2000))return {ok:false,errors:["ACTOR_PRINCIPAL_AND_REASON_REQUIRED"]};
 const current=db.query("SELECT payload_hash,payload_json FROM current_supplier_commitments WHERE commitment_id=? ORDER BY id DESC LIMIT 1").get(input.commitmentId) as any;if(!current)return {ok:false,errors:["COMMITMENT_NOT_ACTIVE"]}; const payload={commitmentId:input.commitmentId,reason:input.reason,previousPayloadHash:current.payload_hash};const h=digest(payload);
 const prior=db.query("SELECT 1 FROM supplier_commitment_events WHERE commitment_id=? AND event_type=? AND payload_hash=?").get(input.commitmentId,input.action,h);if(prior)return {ok:true,idempotent:true,payloadHash:h};
 db.query("INSERT INTO supplier_commitment_events(commitment_id,event_type,payload_json,payload_hash,previous_payload_hash,actor,principal,created_at) VALUES(?,?,?,?,?,?,?,?)").run(input.commitmentId,input.action,canonical(payload),h,current.payload_hash,input.actor,input.principal,new Date().toISOString());return {ok:true,idempotent:false,payloadHash:h};
}
export function listSupplierCommitments(db:Database){return db.query("SELECT commitment_id,payload_json,payload_hash,created_at FROM current_supplier_commitments ORDER BY commitment_id").all() as Array<{commitment_id:string;payload_json:string;payload_hash:string;created_at:string}>;}
function scheduledOccurrence(proposal:CommitmentProposal,date:string){let cursor=proposal.nextDate;for(let count=0;count<10000&&cursor<=date;count++){if(cursor===date&&(!proposal.endDate||cursor<=proposal.endDate))return {date:cursor,amount:proposal.amount,currency:proposal.currency};cursor=next(cursor,proposal.frequency);}return null;}
export function plannedCommitmentOccurrences(db:Database,startDate:string,weeks=13){const end=addDays(startDate,weeks*7-1);const result:Array<{commitmentId:string;date:string;amount:number;currency:string;payloadHash:string}>=[];const matched=new Set((db.query("SELECT commitment_id,occurrence_date FROM supplier_commitment_occurrence_matches").all() as Array<{commitment_id:string;occurrence_date:string}>).map(row=>`${row.commitment_id}\u0000${row.occurrence_date}`));for(const row of listSupplierCommitments(db)){let p:CommitmentProposal;try{p=JSON.parse(row.payload_json);}catch{continue;}if(p.status!=="active")continue;let cursor=p.nextDate;for(let count=0;count<10000&&cursor<=end;count++){if(cursor>=startDate&&(!p.endDate||cursor<=p.endDate)&&!matched.has(`${row.commitment_id}\u0000${cursor}`))result.push({commitmentId:row.commitment_id,date:cursor,amount:p.amount,currency:p.currency,payloadHash:row.payload_hash});cursor=next(cursor,p.frequency);}}return result.sort((a,b)=>a.date.localeCompare(b.date)||a.commitmentId.localeCompare(b.commitmentId));}

export type CommitmentEvidenceKind="canonical_document"|"payable"|"bank_transaction";
/** Callers identify immutable evidence by kind+id only. Date, amount,
 * currency, party and document status are resolved from canonical ledger rows
 * here so a client cannot manufacture a favourable variance. */
export type CommitmentOccurrenceMatchInput={commitmentId:string;occurrenceDate:string;evidence:{kind:CommitmentEvidenceKind;id:string};confirm:boolean;actor?:string;principal?:string};
type CanonicalEvidence={kind:CommitmentEvidenceKind;id:string;date:string;amount:number;currency:string;partyId:string|null;documented:boolean;sourceHash:string|null};
function canonicalEvidence(db:Database,evidence:CommitmentOccurrenceMatchInput["evidence"]):CanonicalEvidence|null {
 const id=Number(evidence.id); if(!Number.isSafeInteger(id)||id<1)return null;
 if(evidence.kind==="canonical_document") { const row=db.query("SELECT id,invoice_date,amount_inc_vat,currency,sha256_hash FROM documents WHERE id=?").get(id) as any; if(!row||!isValidIsoDate(row.invoice_date)||!Number.isFinite(Number(row.amount_inc_vat)))return null; const party=db.query("SELECT party_id FROM current_document_party_links WHERE document_id=? AND party_role IN ('supplier','vendor') ORDER BY CASE party_role WHEN 'supplier' THEN 0 ELSE 1 END,id DESC LIMIT 1").get(id) as any;return {kind:evidence.kind,id:String(row.id),date:row.invoice_date,amount:Number(row.amount_inc_vat),currency:String(row.currency??"DKK").toUpperCase(),partyId:party?.party_id??null,documented:true,sourceHash:row.sha256_hash}; }
 if(evidence.kind==="payable") { const row=db.query("SELECT p.id,p.due_date,p.gross_amount,p.currency,d.sha256_hash,p.document_id FROM payables p JOIN documents d ON d.id=p.document_id WHERE p.id=?").get(id) as any;if(!row||!isValidIsoDate(row.due_date)||!Number.isFinite(Number(row.gross_amount)))return null;const party=db.query("SELECT party_id FROM current_document_party_links WHERE document_id=? AND party_role IN ('supplier','vendor') ORDER BY CASE party_role WHEN 'supplier' THEN 0 ELSE 1 END,id DESC LIMIT 1").get(row.document_id) as any;return {kind:evidence.kind,id:String(row.id),date:row.due_date,amount:Number(row.gross_amount),currency:String(row.currency??"DKK").toUpperCase(),partyId:party?.party_id??null,documented:true,sourceHash:row.sha256_hash}; }
 const row=db.query("SELECT id,transaction_date,amount,currency,transaction_hash FROM bank_transactions WHERE id=?").get(id) as any;if(!row||!isValidIsoDate(row.transaction_date)||!Number.isFinite(Number(row.amount)))return null;return {kind:evidence.kind,id:String(row.id),date:row.transaction_date,amount:Math.abs(Number(row.amount)),currency:String(row.currency??"DKK").toUpperCase(),partyId:null,documented:false,sourceHash:row.transaction_hash??null};
}
/** Confirmed evidence link only.  It deliberately does not post, settle, or
 * infer FX: an occurrence and an evidence item may each be linked once. */
export function matchSupplierCommitmentOccurrence(db:Database,input:CommitmentOccurrenceMatchInput){
 if(!input.confirm)return {ok:false,errors:["CONFIRMATION_REQUIRED"]};if(!validText(input.actor,160)||!validText(input.principal,160))return {ok:false,errors:["ACTOR_AND_PRINCIPAL_REQUIRED"]};
 if(!isValidIsoDate(input.occurrenceDate)||!validText(input.evidence.id,160))return {ok:false,errors:["INVALID_OCCURRENCE_OR_EVIDENCE"]};
 const row=listSupplierCommitments(db).find(item=>item.commitment_id===input.commitmentId);let proposal:CommitmentProposal|undefined;try{proposal=row?JSON.parse(row.payload_json):undefined;}catch{}const expected=proposal?scheduledOccurrence(proposal,input.occurrenceDate):null;if(!row||!expected)return {ok:false,errors:["EXPECTED_OCCURRENCE_NOT_FOUND"]};
 const evidence=canonicalEvidence(db,input.evidence);if(!evidence)return {ok:false,errors:["CANONICAL_EVIDENCE_NOT_FOUND_OR_UNSUPPORTED"]};const existingOccurrence=db.query("SELECT payload_hash FROM supplier_commitment_occurrence_matches WHERE commitment_id=? AND occurrence_date=?").get(input.commitmentId,input.occurrenceDate) as any;
 const existingEvidence=db.query("SELECT payload_hash FROM supplier_commitment_occurrence_matches WHERE evidence_kind=? AND evidence_id=?").get(evidence.kind,evidence.id) as any;
 const expectedWithHash={...expected,payloadHash:row.payload_hash};const payload={commitmentId:input.commitmentId,occurrenceDate:input.occurrenceDate,expected:expectedWithHash,evidence};const payloadHash=digest(payload);
 if(existingOccurrence||existingEvidence){if(existingOccurrence?.payload_hash===payloadHash&&existingEvidence?.payload_hash===payloadHash)return {ok:true,idempotent:true,payloadHash,variance:commitmentVariance(expected,evidence,proposal?.vendorPartyId)};return {ok:false,errors:["OCCURRENCE_OR_EVIDENCE_ALREADY_MATCHED"]};}
 db.query("INSERT INTO supplier_commitment_occurrence_matches(commitment_id,occurrence_date,expected_payload_hash,evidence_kind,evidence_id,canonical_payload,payload_hash,actor,principal,created_at) VALUES(?,?,?,?,?,?,?,?,?,?)").run(input.commitmentId,input.occurrenceDate,expectedWithHash.payloadHash,evidence.kind,evidence.id,canonical(payload),payloadHash,input.actor!,input.principal!,new Date().toISOString());
 return {ok:true,idempotent:false,payloadHash,variance:commitmentVariance(expected,evidence,proposal?.vendorPartyId)};
}
export function commitmentVariance(expected:{date:string;amount:number;currency:string},evidence:CanonicalEvidence,expectedPartyId?:string){const currencyMatch=expected.currency===evidence.currency;return {amount:currencyMatch?evidence.amount-expected.amount:null,dateDays:Math.round((Date.parse(evidence.date+"T00:00:00Z")-Date.parse(expected.date+"T00:00:00Z"))/86400000),late:evidence.date>expected.date,currency:{expected:expected.currency,actual:evidence.currency,status:currencyMatch?"same":"unsupported_fx"},party:{expected:expectedPartyId??null,actual:evidence.partyId,status:!evidence.partyId?"missing":evidence.partyId===expectedPartyId?"same":"mismatch"},documentation:evidence.documented?"present":"missing",comparability:currencyMatch?"native_currency":"unsupported_fx"};}
export function listSupplierCommitmentOccurrenceMatches(db:Database,commitmentId?:string){const rows=(commitmentId?db.query("SELECT canonical_payload,payload_hash,actor,principal,created_at FROM supplier_commitment_occurrence_matches WHERE commitment_id=? ORDER BY occurrence_date,id").all(commitmentId):db.query("SELECT canonical_payload,payload_hash,actor,principal,created_at FROM supplier_commitment_occurrence_matches ORDER BY commitment_id,occurrence_date,id").all()) as any[];return rows.map(row=>({...JSON.parse(row.canonical_payload),payloadHash:row.payload_hash,actor:row.actor,principal:row.principal,createdAt:row.created_at}));}
export function supplierCommitmentAlerts(db:Database,asOf:string){if(!isValidIsoDate(asOf))return [];const end=addDays(asOf,30);const alerts:any[]=[];for(const row of listSupplierCommitments(db)){const p=JSON.parse(row.payload_json) as CommitmentProposal;for(const [kind,date] of [["notice",p.noticeDate],["renewal",p.renewalDate]] as const)if(date&&date>=asOf&&date<=end)alerts.push({commitmentId:row.commitment_id,kind,date,severity:date===asOf?"due":"upcoming"});}return alerts.sort((a,b)=>a.date.localeCompare(b.date)||a.commitmentId.localeCompare(b.commitmentId));}
