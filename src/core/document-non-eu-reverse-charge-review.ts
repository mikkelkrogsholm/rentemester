import type { Database } from "bun:sqlite";
import { createHash } from "node:crypto";
import { insertAuditLog, resolveActor } from "./actor";
import { getCompanySettings } from "./company";

export type NonEuReverseChargeReviewInput = {
  documentId:number; supplierCountryCode:string; actualBuyerVat:string; taxPeriod:string; deductionPercent:number;
  supplierEvidenceReference:string; supplierEvidenceSha256:string; buyerEvidenceReference:string; buyerEvidenceSha256:string;
  serviceEvidenceReference:string; serviceEvidenceSha256:string; formalDeficiencies:string[]; rationale:string;
  foreignVatCharged:boolean; confirm:boolean; supersedesReviewSha256?:string; createdBy?:string; createdByProgram?:string; principal?:string;
};
export type NonEuReverseChargeReviewResult = {ok:boolean;applied?:boolean;reviewSha256?:string;errors:string[]};
const hash=(v:string)=>createHash("sha256").update(v).digest("hex");
const canonical=(v:unknown):string=>{if(v===null||typeof v!=="object")return JSON.stringify(v);if(Array.isArray(v))return `[${v.map(canonical).join(",")}]`;const r=v as Record<string,unknown>;return `{${Object.keys(r).sort().map(k=>`${JSON.stringify(k)}:${canonical(r[k])}`).join(",")}}`;};
const validHash=(v:unknown)=>typeof v==="string"&&/^[a-f0-9]{64}$/i.test(v);
const required=(v:unknown,name:string,e:string[])=>{if(typeof v!=="string"||!v.trim()||v.trim().length>2000){e.push(`${name} is required and must be at most 2000 characters`);return null;}return v.trim();};
const period=(v:string)=>/^\d{4}-(0[1-9]|1[0-2])$/.test(v);

/**
 * Records material evidence for a non-EU service reverse-charge decision with
 * formal invoice defects. It never changes invoice facts or treats an OSS/IE
 * tax id as evidence of EU establishment.
 */
export function reviewNonEuReverseChargeEvidence(db:Database,input:NonEuReverseChargeReviewInput):NonEuReverseChargeReviewResult {
 const errors:string[]=[];
 if(!Number.isInteger(input.documentId)||input.documentId<=0)errors.push("documentId must be a positive integer");
 const supplierCountry=typeof input.supplierCountryCode==="string"?input.supplierCountryCode.trim().toUpperCase():"";
 if(!/^[A-Z]{2}$/.test(supplierCountry)||supplierCountry==="DK"||["AT","BE","BG","HR","CY","CZ","DE","EE","EL","ES","FI","FR","GR","HU","IE","IT","LT","LU","LV","MT","NL","PL","PT","RO","SE","SI","SK"].includes(supplierCountry))errors.push("supplierCountryCode must document a non-EU supplier establishment");
 const buyerVat=typeof input.actualBuyerVat==="string"?input.actualBuyerVat.trim().toUpperCase().replace(/\s/g,""):"";
 if(!/^DK\d{8}$/.test(buyerVat))errors.push("actualBuyerVat must be a Danish VAT identifier (DK + 8 digits)");
 if(!period(input.taxPeriod))errors.push("taxPeriod must be YYYY-MM");
 if(!Number.isFinite(input.deductionPercent)||input.deductionPercent<=0||input.deductionPercent>100)errors.push("deductionPercent must be greater than 0 and at most 100");
 for(const [n,v] of [["supplierEvidenceReference",input.supplierEvidenceReference],["buyerEvidenceReference",input.buyerEvidenceReference],["serviceEvidenceReference",input.serviceEvidenceReference],["rationale",input.rationale]] as const)required(v,n,errors);
 for(const [n,v] of [["supplierEvidenceSha256",input.supplierEvidenceSha256],["buyerEvidenceSha256",input.buyerEvidenceSha256],["serviceEvidenceSha256",input.serviceEvidenceSha256]] as const)if(!validHash(v))errors.push(`${n} must be a SHA-256 hex digest`);
 if(!Array.isArray(input.formalDeficiencies)||input.formalDeficiencies.length===0||input.formalDeficiencies.some(v=>typeof v!=="string"||!v.trim()||v.length>200))errors.push("formalDeficiencies must identify one or more concrete invoice defects");
 if(input.foreignVatCharged!==false)errors.push("foreign or local VAT charged by the supplier cannot be treated as Danish reverse charge");
 if(input.confirm!==true)errors.push("non-EU reverse-charge evidence review requires explicit confirm: true");
 if(input.supersedesReviewSha256!==undefined&&!validHash(input.supersedesReviewSha256))errors.push("supersedesReviewSha256 must be a SHA-256 hex digest");
 if(errors.length)return {ok:false,errors};
 try{return db.transaction(()=>{
  const d=db.query("SELECT id,status,document_type,sha256_hash,payload_json,supplier_country_code,supplier_identifier_kind,supplier_identity_status FROM documents WHERE id=?").get(input.documentId) as Record<string,unknown>|null;
  if(!d)return {ok:false,errors:[`document ${input.documentId} does not exist`]};
  if(d.status!=="ingested")return {ok:false,errors:["document must be ingested and unposted"]};
  if(d.document_type!=="purchase_sale")return {ok:false,errors:["review is limited to purchase invoices"]};
  if(d.supplier_country_code!==supplierCountry||d.supplier_identifier_kind!=="non_eu"||d.supplier_identity_status!=="resolved")return {ok:false,errors:["review supplier establishment must match the document's resolved non-EU identity; observed tax IDs do not establish EU presence"]};
  if(db.query("SELECT 1 FROM journal_entries WHERE document_id=? LIMIT 1").get(input.documentId)||db.query("SELECT 1 FROM payables WHERE document_id=? LIMIT 1").get(input.documentId))return {ok:false,errors:["document is linked to accounting evidence"]};
  const company=getCompanySettings(db);if(company.cvr!==buyerVat||company.vatPeriodType===null)return {ok:false,errors:["actual buyer must match the configured VAT-registered Danish company"]};
  const material={documentSha256:String(d.sha256_hash),payloadSha256:hash(String(d.payload_json)),supplierCountryCode:supplierCountry,supplierEvidenceReference:input.supplierEvidenceReference.trim(),supplierEvidenceSha256:input.supplierEvidenceSha256.toLowerCase(),buyerVat,buyerEvidenceReference:input.buyerEvidenceReference.trim(),buyerEvidenceSha256:input.buyerEvidenceSha256.toLowerCase(),serviceEvidenceReference:input.serviceEvidenceReference.trim(),serviceEvidenceSha256:input.serviceEvidenceSha256.toLowerCase(),taxPeriod:input.taxPeriod,deductionPercent:input.deductionPercent,formalDeficiencies:[...new Set(input.formalDeficiencies.map(v=>v.trim()))].sort(),rationale:input.rationale.trim(),treatment:"non_eu_service_reverse_charge",supersedesReviewSha256:input.supersedesReviewSha256??null};
  const reviewSha256=hash(canonical(material));const current=db.query("SELECT review_sha256 FROM document_non_eu_reverse_charge_reviews WHERE document_id=? ORDER BY id DESC LIMIT 1").get(input.documentId) as {review_sha256:string}|null;
  if(current?.review_sha256===reviewSha256)return {ok:true,applied:false,reviewSha256,errors:[]};if(current&&input.supersedesReviewSha256!==current.review_sha256)return {ok:false,errors:["new evidence must supersede the current review hash"]};
  const actor=resolveActor({createdBy:input.createdBy,createdByProgram:input.createdByProgram});const principal=required(input.principal??actor.createdBy,"principal",errors)!;
  db.query("INSERT INTO document_non_eu_reverse_charge_reviews(document_id,document_sha256,payload_sha256,supplier_country_code,supplier_evidence_reference,supplier_evidence_sha256,buyer_evidence_reference,buyer_evidence_sha256,service_evidence_reference,service_evidence_sha256,tax_period,deduction_percent,formal_deficiencies_json,rationale,treatment,supersedes_review_sha256,review_sha256,actor,principal,program) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)").run(input.documentId,material.documentSha256,material.payloadSha256,supplierCountry,material.supplierEvidenceReference,material.supplierEvidenceSha256,material.buyerEvidenceReference,material.buyerEvidenceSha256,material.serviceEvidenceReference,material.serviceEvidenceSha256,material.taxPeriod,material.deductionPercent,JSON.stringify(material.formalDeficiencies),material.rationale,material.treatment,material.supersedesReviewSha256,reviewSha256,actor.createdBy,principal,actor.createdByProgram);
  insertAuditLog(db,{eventType:"document_non_eu_reverse_charge_reviewed",entityType:"document",entityId:input.documentId,message:`Recorded non-EU reverse-charge material review for document ${input.documentId} (review_sha256=${reviewSha256})`,createdBy:actor.createdBy,createdByProgram:actor.createdByProgram});return {ok:true,applied:true,reviewSha256,errors:[]};
 }).immediate();}catch(error){return {ok:false,errors:[error instanceof Error?error.message:String(error)]};}
}
export function validNonEuReverseChargeReview(db:Database,documentId:number):{deductionPercent:number}|null{
 const r=db.query("SELECT r.*,d.sha256_hash,d.payload_json,d.supplier_country_code,d.supplier_identifier_kind,d.supplier_identity_status FROM document_non_eu_reverse_charge_reviews r JOIN documents d ON d.id=r.document_id WHERE r.document_id=? ORDER BY r.id DESC LIMIT 1").get(documentId) as Record<string,unknown>|null;if(!r)return null;
 try{const material={documentSha256:String(r.sha256_hash),payloadSha256:hash(String(r.payload_json)),supplierCountryCode:r.supplier_country_code,supplierEvidenceReference:r.supplier_evidence_reference,supplierEvidenceSha256:r.supplier_evidence_sha256,buyerVat:getCompanySettings(db).cvr,buyerEvidenceReference:r.buyer_evidence_reference,buyerEvidenceSha256:r.buyer_evidence_sha256,serviceEvidenceReference:r.service_evidence_reference,serviceEvidenceSha256:r.service_evidence_sha256,taxPeriod:r.tax_period,deductionPercent:r.deduction_percent,formalDeficiencies:JSON.parse(String(r.formal_deficiencies_json)),rationale:r.rationale,treatment:r.treatment,supersedesReviewSha256:r.supersedes_review_sha256};return r.document_sha256===r.sha256_hash&&r.payload_sha256===material.payloadSha256&&r.supplier_identifier_kind==="non_eu"&&r.supplier_identity_status==="resolved"&&r.supplier_country_code===r.supplier_country_code&&r.treatment==="non_eu_service_reverse_charge"&&r.review_sha256===hash(canonical(material))&&Number(r.deduction_percent)>0&&Number(r.deduction_percent)<=100?{deductionPercent:Number(r.deduction_percent)}:null;}catch{return null;}
}
