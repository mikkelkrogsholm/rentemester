import type { Database } from "bun:sqlite";
import { createHash } from "node:crypto";
import { insertAuditLog, resolveActor } from "./actor";
import { validPurchaseCompanyContext } from "./document-company-context";
import { deductibleDanishPurchaseSupplierErrors } from "./supplier-identity";

type Input = { documentId:number; bankTransactionId:number; businessEvidenceReference:string; businessEvidenceSha256:string; rationale:string; supersedesReviewSha256?:string; confirm:boolean; createdBy?:string; createdByProgram?:string; principal?:string };
export type PurchaseVatEvidenceReviewResult = { ok:boolean; applied?:boolean; reviewSha256?:string; errors:string[] };
const hash=(value:string)=>createHash("sha256").update(value).digest("hex");
const validHash=(value:unknown)=>typeof value==="string"&&/^[a-f0-9]{64}$/i.test(value);
const text=(value:unknown,name:string,errors:string[])=>{if(typeof value!=="string"||!value.trim()||value.trim().length>2000){errors.push(`${name} is required and must be at most 2000 characters`);return null;}return value.trim();};
const canonical=(value:Record<string,unknown>)=>JSON.stringify(Object.fromEntries(Object.entries(value).sort(([a],[b])=>a.localeCompare(b))));

/**
 * This is not an override. It records the extra objective information that an
 * authority may consider where an otherwise real Danish purchase invoice has
 * a formal buyer-field defect (ML §37/§36a; VAT Directive arts. 168/178;
 * C-516/14 Barlis). Source invoice facts remain untouched.
 */
export function reviewIncompleteStandardPurchaseVatEvidence(db:Database,input:Input):PurchaseVatEvidenceReviewResult {
  const errors:string[]=[];
  if(!Number.isInteger(input.documentId)||input.documentId<=0)errors.push("documentId must be a positive integer");
  if(!Number.isInteger(input.bankTransactionId)||input.bankTransactionId<=0)errors.push("bankTransactionId must be a positive integer");
  const businessEvidenceReference=text(input.businessEvidenceReference,"businessEvidenceReference",errors);
  const rationale=text(input.rationale,"rationale",errors);
  if(!validHash(input.businessEvidenceSha256))errors.push("businessEvidenceSha256 must be a SHA-256 hex digest");
  if(input.supersedesReviewSha256!==undefined&&!validHash(input.supersedesReviewSha256))errors.push("supersedesReviewSha256 must be a SHA-256 hex digest");
  if(input.confirm!==true)errors.push("purchase VAT evidence review requires explicit confirm: true");
  if(errors.length)return {ok:false,errors};
  try{return db.transaction(()=>{
    const document=db.query(`SELECT id,status,document_type,sha256_hash,payload_json,amount_inc_vat,vat_amount,sender_vat_cvr,supplier_country_code,supplier_identifier_kind,supplier_identity_status FROM documents WHERE id=?`).get(input.documentId) as Record<string,unknown>|null;
    if(!document)return {ok:false,errors:[`document ${input.documentId} does not exist`]};
    if(document.status!=="ingested")return {ok:false,errors:["document must be ingested and unposted"]};
    if(db.query("SELECT 1 FROM journal_entries WHERE document_id=? LIMIT 1").get(input.documentId)||db.query("SELECT 1 FROM payables WHERE document_id=? LIMIT 1").get(input.documentId))return {ok:false,errors:["document is linked to accounting evidence"]};
    let payload:Record<string,unknown>;try{payload=JSON.parse(String(document.payload_json)) as Record<string,unknown>;}catch{return {ok:false,errors:["document payload_json is not valid JSON"]};}
    if(payload.incompleteStandardPurchaseInvoice!==true||payload.danishSimplifiedPurchaseInvoice===true||document.document_type!=="purchase_sale")return {ok:false,errors:["review is limited to a truthfully incomplete standard Danish purchase invoice"]};
    if(!validPurchaseCompanyContext(db,input.documentId))return {ok:false,errors:["a valid hash-bound company context is required before VAT evidence review"]};
    const supplierErrors=deductibleDanishPurchaseSupplierErrors({supplierVatOrCvr:document.sender_vat_cvr as string|null,supplierCountryCode:document.supplier_country_code as string|null,supplierIdentifierKind:document.supplier_identifier_kind as string|null,supplierIdentityStatus:document.supplier_identity_status as string|null});
    if(supplierErrors.length)return {ok:false,errors:supplierErrors};
    const gross=Number(document.amount_inc_vat),vat=Number(document.vat_amount);if(!(gross>0)||!(vat>0)||Math.abs(vat-(gross-vat)*.25)>.011)return {ok:false,errors:["document must state a positive Danish 25% VAT amount"]};
    const bank=db.query("SELECT id,transaction_date,amount,currency,transaction_hash FROM bank_transactions WHERE id=?").get(input.bankTransactionId) as Record<string,unknown>|null;
    if(!bank)return {ok:false,errors:[`bank transaction ${input.bankTransactionId} does not exist`]};
    if(!(Number(bank.amount)<0)||String(bank.currency).toUpperCase()!=="DKK"||Math.abs(Math.abs(Number(bank.amount))-gross)>.011)return {ok:false,errors:["bank transaction must be a documented DKK company payment matching the invoice gross amount"]};
    const context=db.query("SELECT context_sha256 FROM document_company_contexts WHERE document_id=?").get(input.documentId) as {context_sha256:string}|null;
    if(!context)return {ok:false,errors:["a valid hash-bound company context is required before VAT evidence review"]};
    const bankEvidenceSha256=hash(canonical({id:bank.id,transactionDate:bank.transaction_date,amount:bank.amount,currency:bank.currency,transactionHash:bank.transaction_hash}));
    const actor=resolveActor({createdBy:input.createdBy,createdByProgram:input.createdByProgram});
    const principal=text(input.principal??actor.createdBy,"principal",errors)!;
    const material={documentSha256:String(document.sha256_hash),payloadSha256:hash(String(document.payload_json)),companyContextSha256:context.context_sha256,bankTransactionId:input.bankTransactionId,bankEvidenceSha256,businessEvidenceReference:businessEvidenceReference!,businessEvidenceSha256:input.businessEvidenceSha256.toLowerCase(),rationale:rationale!,legalBasis:"formal_invoice_deficiency_only",supersedesReviewSha256:input.supersedesReviewSha256??null};
    const reviewSha256=hash(canonical(material));
    const current=db.query("SELECT review_sha256 FROM document_purchase_vat_evidence_reviews WHERE document_id=? ORDER BY id DESC LIMIT 1").get(input.documentId) as {review_sha256:string}|null;
    if(current?.review_sha256===reviewSha256)return {ok:true,applied:false,reviewSha256,errors:[]};
    if(current&&input.supersedesReviewSha256!==current.review_sha256)return {ok:false,errors:["new evidence must supersede the current review hash"]};
    db.query(`INSERT INTO document_purchase_vat_evidence_reviews(document_id,document_sha256,payload_sha256,company_context_sha256,bank_transaction_id,bank_evidence_sha256,business_evidence_reference,business_evidence_sha256,rationale,legal_basis,supersedes_review_sha256,review_sha256,actor,principal,program) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)`).run(input.documentId,material.documentSha256,material.payloadSha256,material.companyContextSha256,input.bankTransactionId,bankEvidenceSha256,businessEvidenceReference!,input.businessEvidenceSha256.toLowerCase(),rationale!,"formal_invoice_deficiency_only",input.supersedesReviewSha256??null,reviewSha256,actor.createdBy,principal,actor.createdByProgram);
    insertAuditLog(db,{eventType:"document_purchase_vat_evidence_reviewed",entityType:"document",entityId:input.documentId,message:`Recorded formal-deficiency VAT evidence review for document ${input.documentId} (review_sha256=${reviewSha256})`,createdBy:actor.createdBy,createdByProgram:actor.createdByProgram});
    return {ok:true,applied:true,reviewSha256,errors:[]};
  }).immediate();}catch(error){return {ok:false,errors:[error instanceof Error?error.message:String(error)]};}
}

/** Revalidates every immutable link; the review is invalidated by any change. */
export function validIncompleteStandardPurchaseVatEvidenceReview(db:Database,documentId:number):boolean {
  const row=db.query(`SELECT r.*,d.sha256_hash,d.payload_json,b.id bank_id,b.transaction_date,b.amount,b.currency,b.transaction_hash,c.context_sha256 FROM document_purchase_vat_evidence_reviews r JOIN documents d ON d.id=r.document_id JOIN bank_transactions b ON b.id=r.bank_transaction_id JOIN document_company_contexts c ON c.document_id=d.id WHERE r.document_id=? ORDER BY r.id DESC LIMIT 1`).get(documentId) as Record<string,unknown>|null;
  if(!row||!validPurchaseCompanyContext(db,documentId))return false;
  try{const payload=JSON.parse(String(row.payload_json)) as Record<string,unknown>;const bankHash=hash(canonical({id:row.bank_id,transactionDate:row.transaction_date,amount:row.amount,currency:row.currency,transactionHash:row.transaction_hash}));const material={documentSha256:String(row.sha256_hash),payloadSha256:hash(String(row.payload_json)),companyContextSha256:row.context_sha256,bankTransactionId:row.bank_transaction_id,bankEvidenceSha256:bankHash,businessEvidenceReference:row.business_evidence_reference,businessEvidenceSha256:row.business_evidence_sha256,rationale:row.rationale,legalBasis:row.legal_basis,supersedesReviewSha256:row.supersedes_review_sha256};return payload.incompleteStandardPurchaseInvoice===true&&payload.danishSimplifiedPurchaseInvoice!==true&&row.document_sha256===row.sha256_hash&&row.payload_sha256===material.payloadSha256&&row.company_context_sha256===row.context_sha256&&row.bank_evidence_sha256===bankHash&&row.legal_basis==="formal_invoice_deficiency_only"&&row.review_sha256===hash(canonical(material));}catch{return false;}
}
