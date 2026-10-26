import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { openDb, migrate } from "../../src/core/db";
import { ingestDocument } from "../../src/core/documents";
import { postNonEuServiceReverseChargePurchase } from "../../src/core/vat";
import { ensureCompanyDirs } from "../../src/core/paths";
import { seedAccounts, verifyAuditChain } from "../../src/core/ledger";
import { reviewNonEuReverseChargeEvidence, validNonEuReverseChargeReview } from "../../src/core/document-non-eu-reverse-charge-review";

const digest = "a".repeat(64);
function fixture() {
  const root=mkdtempSync(join(tmpdir(),"rentemester-non-eu-review-")), inbox=mkdtempSync(join(tmpdir(),"rentemester-non-eu-review-inbox-"));
  const db=openDb(ensureCompanyDirs(root).db);migrate(db);seedAccounts(db);
  db.run("INSERT INTO companies(id,name,country,currency,cvr,address,postal_code,city,vat_period_type) VALUES(1,'Synthetic Buyer ApS','DK','DKK','DK12345678','Testvej 1','1000','Testby','quarter')");
  const file=join(inbox,"synthetic-us-service.txt");writeFileSync(file,"Synthetic service invoice");
  const document=ingestDocument(db,root,file,{source:"email",documentType:"purchase_sale",issueDate:"2026-09-01",invoiceNo:"SYN-RC-623",deliveryDescription:"Synthetic US service",amountIncVat:1000,currency:"DKK",sender:{name:"Synthetic US supplier",address:"US address",vatOrCvr:"IEOSS1234567",countryCode:"US",identifierKind:"non_eu"},recipient:{name:"Printed private person",address:"Private address",vatOrCvr:"DK87654321"},vatAmount:0});
  expect(document.ok).toBe(true);return {root,inbox,db,documentId:Number(document.documentId)};
}
function close(x:ReturnType<typeof fixture>){x.db.close();rmSync(x.root,{recursive:true,force:true});rmSync(x.inbox,{recursive:true,force:true});}
function input(documentId:number){return {documentId,supplierCountryCode:"US",actualBuyerVat:"DK12345678",taxPeriod:"2026-09",deductionPercent:50,supplierEvidenceReference:"supplier registry extract",supplierEvidenceSha256:digest,buyerEvidenceReference:"company approval",buyerEvidenceSha256:"b".repeat(64),serviceEvidenceReference:"service contract",serviceEvidenceSha256:"c".repeat(64),formalDeficiencies:["missing_home_registration_number","printed_buyer_vat_incorrect","missing_reverse_charge_wording"],rationale:"Synthetic source evidence proves supplier, buyer, service and Danish use.",foreignVatCharged:false,confirm:true,createdBy:"agent:test",createdByProgram:"unit-test",principal:"service_principal:synthetic"};}

describe("non-EU reverse-charge material review (#623)",()=>{
 test("keeps the straight-through invoice gate, then permits only a hash-bound material review and partial deduction",()=>{
  const x=fixture();try{
   const before=postNonEuServiceReverseChargePurchase(x.db,{transactionDate:"2026-09-02",text:"synthetic service",documentId:x.documentId,netAmount:1000,expenseAccountNo:"3010"});
   expect(before.ok).toBe(false);expect(before.errors.join(" ")).toContain("human resolution");
   const review=reviewNonEuReverseChargeEvidence(x.db,input(x.documentId));expect(review).toMatchObject({ok:true,applied:true});
   expect(reviewNonEuReverseChargeEvidence(x.db,input(x.documentId))).toMatchObject({ok:true,applied:false});
   expect(validNonEuReverseChargeReview(x.db,x.documentId)).toEqual({deductionPercent:50});
   const posted=postNonEuServiceReverseChargePurchase(x.db,{transactionDate:"2026-09-02",text:"synthetic service",documentId:x.documentId,netAmount:1000,expenseAccountNo:"3010"});expect(posted.ok).toBe(true);
   const lines=x.db.query("SELECT debit_amount,credit_amount,vat_code FROM journal_lines WHERE journal_entry_id=? ORDER BY id").all(posted.entryId!) as any[];
   expect(lines).toEqual([{debit_amount:1000,credit_amount:0,vat_code:"NON_EU_SERVICE_REVERSE_CHARGE"},{debit_amount:125,credit_amount:0,vat_code:null},{debit_amount:125,credit_amount:0,vat_code:null},{debit_amount:0,credit_amount:1000,vat_code:null},{debit_amount:0,credit_amount:250,vat_code:null}]);
   expect(verifyAuditChain(x.db).ok).toBe(true);
  }finally{close(x);}
 });
 test("fails closed for foreign tax, EU establishment claims, buyer conflict and stale correction",()=>{
  const x=fixture();try{
   expect(reviewNonEuReverseChargeEvidence(x.db,{...input(x.documentId),foreignVatCharged:true}).errors.join(" ")).toContain("foreign or local VAT");
   expect(reviewNonEuReverseChargeEvidence(x.db,{...input(x.documentId),supplierCountryCode:"IE"}).errors.join(" ")).toContain("non-EU supplier establishment");
   expect(reviewNonEuReverseChargeEvidence(x.db,{...input(x.documentId),actualBuyerVat:"DK87654321"}).errors.join(" ")).toContain("configured VAT-registered Danish company");
   const first=reviewNonEuReverseChargeEvidence(x.db,input(x.documentId));expect(first.ok).toBe(true);
   expect(reviewNonEuReverseChargeEvidence(x.db,{...input(x.documentId),rationale:"Different evidence",supersedesReviewSha256:"d".repeat(64)}).errors).toContain("new evidence must supersede the current review hash");
  }finally{close(x);}
 });
});
