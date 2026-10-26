import { canonicalJson } from "./canonical-json";
/** Correct one direct-bank purchase into the existing payable lifecycle (#594). */
import type { Database } from "bun:sqlite";
import { createHash } from "node:crypto";
import { insertAuditLog } from "./actor";
import type { StablePrincipal } from "./idempotency";
import { reverseJournalEntryInCurrentTransaction } from "./ledger";
import { payPayableFromBankInCurrentTransaction, registerPayableInCurrentTransaction } from "./payables";
import { applyBankReconciliationCorrection, planBankReconciliationCorrection } from "./bank-journal-reconciliation";

const canonical = canonicalJson;
const hash=(value:unknown)=>createHash("sha256").update(canonical(value)).digest("hex");
const text=(v:unknown,max=1000)=>typeof v==="string"&&v.trim().length>0&&v.trim().length<=max?v.trim():null;
export type DirectBankPurchasePayableCorrectionPlanInput={documentId:number;bankTransactionId:number;billDate:string;dueDate:string;expenseAccountNo:string;vatTreatment?:"standard"|"exempt"|"non_deductible";vendorId?:number;note?:string};
export type ApplyDirectBankPurchasePayableCorrectionInput=DirectBankPurchasePayableCorrectionPlanInput&{planHash:string;reason:string;actor?:string;principal?:StablePrincipal;confirm:boolean};

function context(db:Database,input:DirectBankPurchasePayableCorrectionPlanInput):any|{error:string}{
  if(!Number.isInteger(input.documentId)||!Number.isInteger(input.bankTransactionId))return {error:"DOCUMENT_AND_BANK_TRANSACTION_REQUIRED"};
  const d=db.query("SELECT id,sha256_hash,invoice_date,amount_inc_vat,vat_amount,currency,payload_json FROM documents WHERE id=?").get(input.documentId) as any;
  const b=db.query("SELECT id,transaction_date,amount,currency FROM bank_transactions WHERE id=?").get(input.bankTransactionId) as any;
  const j=db.query("SELECT id,entry_no,entry_hash,transaction_date,source_bank_transaction_id,status,reversal_of_entry_id FROM journal_entries WHERE document_id=? AND source_bank_transaction_id=? AND status='posted' ORDER BY id LIMIT 2").all(input.documentId,input.bankTransactionId) as any[];
  if(!d||!b||j.length!==1)return {error:"DIRECT_BANK_PURCHASE_RECONCILIATION_REQUIRED"};
  if(String(d.currency).toUpperCase()!=="DKK"||String(b.currency).toUpperCase()!=="DKK"||Number(b.amount)>=0)return {error:"OUTGOING_DKK_BANK_EVIDENCE_REQUIRED"};
  if(Math.round(Math.abs(Number(b.amount))*100)!==Math.round(Number(d.amount_inc_vat)*100))return {error:"DOCUMENT_AND_BANK_AMOUNT_MISMATCH"};
  if(!d.invoice_date||input.billDate!==d.invoice_date)return {error:"BILL_DATE_MUST_EQUAL_DOCUMENT_INVOICE_DATE"};
  if(j[0].reversal_of_entry_id!=null||db.query("SELECT 1 FROM journal_entries WHERE reversal_of_entry_id=?").get(j[0].id))return {error:"DIRECT_BANK_JOURNAL_ALREADY_REVERSED"};
  if(b.transaction_date<d.invoice_date)return {error:"SETTLEMENT_BEFORE_BILL_DATE"};
  if(db.query("SELECT 1 FROM payables WHERE document_id=?").get(input.documentId))return {error:"DOCUMENT_ALREADY_PAYABLE"};
  const roles=db.query("SELECT m.role,m.account_no,m.version,a.type,a.active FROM account_role_mappings m JOIN accounts a ON a.account_no=m.account_no WHERE m.status='confirmed' AND m.role IN ('bank','creditors','input_vat') ORDER BY m.role").all();
  const accounts=db.query("SELECT account_no,type,active FROM accounts WHERE account_no=?").all(input.expenseAccountNo);
  const company=db.query("SELECT vat_period_type FROM companies ORDER BY id LIMIT 1").get() as any;
  const periods=db.query("SELECT id,period_start,period_end,kind,status,closed_at,reported_at FROM accounting_periods WHERE NOT (period_end < ? OR period_start > ?) ORDER BY id").all(d.invoice_date,b.transaction_date);
  return {documentId:d.id,documentHash:d.sha256_hash,documentPayloadHash:hash(d.payload_json??null),documentVatAmount:d.vat_amount,bankTransactionId:b.id,bankDate:b.transaction_date,bankAmount:Math.abs(Number(b.amount)),originalJournalEntryId:j[0].id,originalJournalEntryNo:j[0].entry_no,originalJournalHash:j[0].entry_hash,originalJournalDate:j[0].transaction_date,billDate:d.invoice_date,dueDate:input.dueDate,expenseAccountNo:input.expenseAccountNo,vatTreatment:input.vatTreatment??null,vendorId:input.vendorId??null,note:input.note??null,vatPeriodType:company?.vat_period_type??null,accountRoles:roles,accounts,accountingPeriods:periods};
}
export function planDirectBankPurchasePayableCorrection(db:Database,input:DirectBankPurchasePayableCorrectionPlanInput){const c=context(db,input);if("error" in c)return {ok:false as const,errors:[c.error]};const plan={schemaVersion:"rentemester-direct-bank-purchase-payable-correction-v1",...c};return {ok:true as const,plan:{...plan,planHash:hash(plan)},errors:[] as string[]};}
export function applyDirectBankPurchasePayableCorrection(db:Database,input:ApplyDirectBankPurchasePayableCorrectionInput){
  if(!input.confirm)return {ok:false as const,errors:["CONFIRMATION_REQUIRED"]}; const actor=text(input.actor,160),reason=text(input.reason);const principal=input.principal?.kind==="user"||input.principal?.kind==="service-account"?input.principal:null;
  if(!actor||!principal?.subjectId)return {ok:false as const,errors:["ACTOR_AND_PRINCIPAL_REQUIRED"]};
  if(!reason)return {ok:false as const,errors:["REASON_REQUIRED"]};
  try{return db.transaction(()=>{const old=db.query("SELECT id,payable_id,payment_id,settlement_journal_entry_id,plan_hash FROM direct_bank_purchase_payable_corrections WHERE document_id=? AND bank_transaction_id=?").get(input.documentId,input.bankTransactionId) as any;if(old){if(old.plan_hash!==input.planHash)return {ok:false as const,errors:["IDEMPOTENCY_PAYLOAD_CONFLICT"]};return {ok:true as const,idempotent:true,id:old.id,payableId:old.payable_id,paymentId:old.payment_id,settlementJournalEntryId:old.settlement_journal_entry_id,errors:[] as string[]};}
    const planned=planDirectBankPurchasePayableCorrection(db,input);if(!planned.ok)return planned;if(planned.plan.planHash!==input.planHash)return {ok:false as const,errors:["PLAN_HASH_MISMATCH"]};
    const reversal=reverseJournalEntryInCurrentTransaction(db,{entryId:planned.plan.originalJournalEntryId,transactionDate:planned.plan.originalJournalDate,reason,createdBy:actor,createdByProgram:"direct-bank-purchase-payable-correction"});if(!reversal.ok)throw new Error(reversal.errors.join("; "));
    const payable=registerPayableInCurrentTransaction(db,{documentId:input.documentId,billDate:input.billDate,dueDate:input.dueDate,expenseAccountNo:input.expenseAccountNo,vatTreatment:input.vatTreatment,vendorId:input.vendorId,note:input.note,createdBy:actor,createdByProgram:"direct-bank-purchase-payable-correction"});if(!payable.ok||!payable.payableId)throw new Error(payable.errors.join("; "));
    const payment=payPayableFromBankInCurrentTransaction(db,{payableId:payable.payableId,bankTransactionId:input.bankTransactionId,paymentDate:planned.plan.bankDate,note:input.note,createdBy:actor,createdByProgram:"direct-bank-purchase-payable-correction",allowSupersededDirectBankJournalEntryId:planned.plan.originalJournalEntryId,skipBankSourceLink:true});if(!payment.ok||!payment.paymentId||!payment.journalEntryId)throw new Error(payment.errors.join("; "));
    const reconciliation=planBankReconciliationCorrection(db,{bankTransactionId:input.bankTransactionId,replacementJournalEntryId:payment.journalEntryId});if(!reconciliation.ok)throw new Error(reconciliation.errors.join("; "));
    const corrected=applyBankReconciliationCorrection(db,{bankTransactionId:input.bankTransactionId,replacementJournalEntryId:payment.journalEntryId,expectedReconciliationId:reconciliation.plan.reconciliationId,planHash:reconciliation.plan.planHash,reason,actor,principal,confirm:true});if(!corrected.ok)throw new Error(corrected.errors.join("; "));
    const row=db.query("INSERT INTO direct_bank_purchase_payable_corrections(document_id,bank_transaction_id,original_journal_entry_id,reversal_journal_entry_id,payable_id,payment_id,settlement_journal_entry_id,document_hash,original_journal_hash,plan_hash,reason,actor,principal_kind,principal_subject_id,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?) RETURNING id").get(input.documentId,input.bankTransactionId,planned.plan.originalJournalEntryId,reversal.entryId,payable.payableId,payment.paymentId,payment.journalEntryId,planned.plan.documentHash,planned.plan.originalJournalHash,input.planHash,reason,actor,principal.kind,principal.subjectId,new Date().toISOString()) as any;
    insertAuditLog(db,{eventType:"direct_bank_purchase_payable_corrected",entityType:"bank_transaction",entityId:String(input.bankTransactionId),message:`Corrected direct-bank purchase into payable ${payable.payableId} and settled it on ${planned.plan.bankDate}`,createdBy:actor,createdByProgram:"direct-bank-purchase-payable-correction"});
    return {ok:true as const,idempotent:false,id:Number(row.id),payableId:payable.payableId,paymentId:payment.paymentId,settlementJournalEntryId:payment.journalEntryId,errors:[] as string[]};
  }).immediate();}catch(error){return {ok:false as const,errors:[error instanceof Error?error.message:String(error)]};}
}
