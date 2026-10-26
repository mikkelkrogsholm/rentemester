import { canonicalJson } from "./canonical-json";
/** Explicit, one-time adoption of legacy bank and creditor records (#601).
 *
 * This module deliberately has no matching or posting logic. Callers name all
 * four source identities; the apply path only appends canonical open-item
 * records and evidence after recomputing the reviewed plan.
 */
import { createHash } from "node:crypto";
import type { Database } from "bun:sqlite";
import { insertAuditLog } from "./actor";
import { resolveAccountRole } from "./account-roles";
import { resolveBankAccount } from "./bank";
import { isValidIsoDate } from "./dates";
import type { StablePrincipal } from "./idempotency";
import { toOre } from "./money";

const canonical = canonicalJson;
const digest=(value:unknown)=>createHash("sha256").update(canonical(value)).digest("hex");
const text=(v:unknown)=>typeof v==="string"?v.trim():"";
const validHash=(v:unknown)=>/^[a-f0-9]{64}$/i.test(text(v));
const validId=(v:unknown)=>Number.isInteger(v)&&Number(v)>0;
const ore=(v:unknown)=>toOre(Number(v));
const ledgerHead=(db:Database)=>(db.query("SELECT entry_hash FROM journal_entries ORDER BY id DESC LIMIT 1").get() as {entry_hash:string}|null)?.entry_hash??null;
const auditHead=(db:Database)=>digest(db.query("SELECT id,event_type,entity_type,entity_id,message,actor,created_at FROM audit_log ORDER BY id DESC LIMIT 1").get()??{empty:true});
const activeBalance=(db:Database, accountNo:string, cutoff:string)=>ore((db.query(`SELECT COALESCE(SUM(l.debit_amount-l.credit_amount),0) AS amount FROM journal_entries j JOIN journal_lines l ON l.journal_entry_id=j.id JOIN accounts a ON a.id=l.account_id WHERE a.account_no=? AND j.status='posted' AND j.transaction_date<=?`).get(accountNo,cutoff) as {amount:number}).amount);

/**
 * Historical Dinero reverse-charge vouchers retain their original liability
 * classifications for the two VAT controls.  This is deliberately narrower
 * than accepting a liability as VAT: the exact source-native 64040/64060
 * pair, an explicit reverse-charge base code, and the 25% control amounts
 * must all agree in the same already-posted journal.
 */
function hasDocumentedLegacyReverseChargeControls(db:Database,journalEntryId:number){
  const lines=db.query(`SELECT a.account_no,a.type,a.normal_balance,l.debit_amount,l.credit_amount,l.vat_code
    FROM journal_lines l JOIN accounts a ON a.id=l.account_id WHERE l.journal_entry_id=? ORDER BY l.id`).all(journalEntryId) as Array<{account_no:string;type:string;normal_balance:string;debit_amount:number;credit_amount:number;vat_code:string|null}>;
  const output=lines.filter(line=>line.account_no==="64040"&&line.type==="liability"&&line.normal_balance==="credit");
  const input=lines.filter(line=>line.account_no==="64060"&&line.type==="liability"&&["debit","credit"].includes(line.normal_balance));
  const controlLines=lines.filter(line=>["64040","64060"].includes(line.account_no));
  if(output.length!==1||input.length!==1||controlLines.length!==2)return false;
  const outputOre=ore(Number(output[0]!.credit_amount)-Number(output[0]!.debit_amount));
  const inputOre=ore(Number(input[0]!.debit_amount)-Number(input[0]!.credit_amount));
  const bases=lines.filter(line=>line.type==="expense"&&["EU_SERVICE_REVERSE_CHARGE","NON_EU_SERVICE_REVERSE_CHARGE"].includes(text(line.vat_code)));
  const codes=new Set(bases.map(line=>text(line.vat_code)));
  const baseOre=bases.reduce((sum,line)=>sum+ore(Number(line.debit_amount)-Number(line.credit_amount)),0n);
  return codes.size===1&&bases.length===1&&outputOre>0n&&ore(output[0]!.debit_amount)===0n&&ore(input[0]!.credit_amount)===0n&&outputOre===inputOre&&baseOre>0n&&outputOre*4n===baseOre;
}

export type LegacyBankBindingInput={bankAccountId:number;ledgerAccountNo:string;cutoff:string};
export type ApplyLegacyBankBindingInput=LegacyBankBindingInput&{planHash:string;idempotencyKey:string;actor?:string;principal?:StablePrincipal;confirm:boolean};
export type LegacyPayablePaymentBackfillInput={purchaseJournalEntryId:number;paymentJournalEntryId:number;documentId:number;bankTransactionId:number};
export type ApplyLegacyPayablePaymentBackfillInput=LegacyPayablePaymentBackfillInput&{planHash:string;idempotencyKey:string;actor?:string;principal?:StablePrincipal;confirm:boolean};

function bindingContext(db:Database,input:LegacyBankBindingInput){
  const errors:string[]=[]; const accountNo=text(input.ledgerAccountNo);
  if(!validId(input.bankAccountId)||!accountNo||!isValidIsoDate(input.cutoff)) errors.push("BANK_ACCOUNT_LEDGER_AND_CUTOFF_REQUIRED");
  const bank=validId(input.bankAccountId)?resolveBankAccount(db,input.bankAccountId):null;
  if(!bank) errors.push("BANK_ACCOUNT_NOT_FOUND");
  else {
    if(bank.ledgerAccountNo!==null) errors.push("BANK_ACCOUNT_ALREADY_BOUND");
    if(!bank.active) errors.push("BANK_ACCOUNT_INACTIVE");
    if(bank.currency.toUpperCase()!=="DKK") errors.push("BANK_ACCOUNT_CURRENCY_MUST_BE_DKK");
  }
  const role=resolveAccountRole(db,"bank");
  if(!role.ok||role.accountNo!==accountNo) errors.push("CONFIRMED_BANK_ROLE_REQUIRED");
  const ledgerAccount=db.query("SELECT active,type FROM accounts WHERE account_no=?").get(accountNo) as {active:number;type:string}|null;
  if(!ledgerAccount||ledgerAccount.active!==1||ledgerAccount.type!=="asset") errors.push("ACTIVE_BANK_LEDGER_ACCOUNT_REQUIRED");
  const imported=bank?db.query("SELECT COUNT(*) AS n FROM bank_transactions WHERE bank_account_id=?").get(bank.id) as {n:number}:null;
  if(!imported||imported.n===0) errors.push("BANK_SOURCE_ROWS_REQUIRED");
  const foreign=bank?db.query("SELECT id FROM bank_transactions WHERE bank_account_id=? AND UPPER(currency)!='DKK' LIMIT 1").get(bank.id):null;
  if(foreign) errors.push("BANK_SOURCE_CURRENCY_MISMATCH");
  const endpoints=bank?db.query(`SELECT id,transaction_date,amount,balance_after,transaction_hash FROM bank_transactions
    WHERE bank_account_id=? AND transaction_date<=? AND UPPER(currency)='DKK' AND balance_after IS NOT NULL
    AND transaction_date=(SELECT MAX(transaction_date) FROM bank_transactions WHERE bank_account_id=? AND transaction_date<=? AND UPPER(currency)='DKK' AND balance_after IS NOT NULL) ORDER BY id`).all(bank.id,input.cutoff,bank.id,input.cutoff) as Array<{id:number;transaction_date:string;amount:number;balance_after:number;transaction_hash:string|null}>:[];
  if(endpoints.length!==1) errors.push("BANK_STATEMENT_ENDPOINT_AMBIGUOUS");
  const endpoint=endpoints[0];
  if(endpoint && !validHash(endpoint.transaction_hash)) errors.push("BANK_SOURCE_HASH_REQUIRED");
  const balance=endpoint?ore(endpoint.balance_after):0n; const ledger=accountNo&&isValidIsoDate(input.cutoff)?activeBalance(db,accountNo,input.cutoff):0n;
  if(endpoint&&ledger!==balance) errors.push(`BANK_LEDGER_CUTOFF_MISMATCH:${ledger}:${balance}`);
  const existing=bank?db.query("SELECT plan_hash FROM legacy_bank_account_bindings WHERE bank_account_id=?").get(bank.id) as {plan_hash:string}|null:null;
  const state={contract:"rentemester-legacy-bank-binding-v1",bankAccountId:input.bankAccountId,bankAccountSlug:bank?.slug??null,ledgerAccountNo:accountNo,cutoff:input.cutoff,statementTransactionId:endpoint?.id??null,statementTransactionHash:endpoint?.transaction_hash??null,statementBalanceOre:balance.toString(),ledgerBalanceOre:ledger.toString(),ledgerHeadHash:ledgerHead(db),auditHeadHash:auditHead(db)};
  return {errors,state,existing};
}

export function planLegacyBankBinding(db:Database,input:LegacyBankBindingInput){const c=bindingContext(db,input);if(c.errors.length)return {ok:false as const,errors:c.errors};return {ok:true as const,alreadyApplied:!!c.existing,plan:{...c.state,planHash:c.existing?.plan_hash??digest(c.state)},errors:[] as string[]};}
export function applyLegacyBankBinding(db:Database,input:ApplyLegacyBankBindingInput){
  if(!input.confirm)return {ok:false as const,errors:["CONFIRMATION_REQUIRED"]}; const actor=text(input.actor),principal=input.principal,key=text(input.idempotencyKey);
  if(!actor||!principal?.subjectId||!["user","service-account"].includes(principal.kind))return {ok:false as const,errors:["ACTOR_AND_PRINCIPAL_REQUIRED"]}; if(!key||key.length>128)return {ok:false as const,errors:["IDEMPOTENCY_KEY_REQUIRED"]};
  try{return db.transaction(()=>{
    const replay=db.query("SELECT id,plan_hash,bank_account_id FROM legacy_bank_account_bindings WHERE principal_kind=? AND principal_subject_id=? AND idempotency_key=?").get(principal.kind,principal.subjectId,key) as {id:number;plan_hash:string;bank_account_id:number}|null;
    if(replay)return replay.plan_hash===input.planHash&&replay.bank_account_id===input.bankAccountId?{ok:true as const,idempotent:true,id:replay.id,planHash:replay.plan_hash,errors:[] as string[]}:{ok:false as const,errors:["IDEMPOTENCY_CONFLICT"]};
    const plan=planLegacyBankBinding(db,input);if(!plan.ok)return plan;if(plan.alreadyApplied)return plan.plan.planHash===input.planHash?{ok:true as const,idempotent:true,planHash:input.planHash,errors:[] as string[]}:{ok:false as const,errors:["BANK_BINDING_CONFLICT"]};if(plan.plan.planHash!==input.planHash)return {ok:false as const,errors:["PLAN_HASH_MISMATCH"]};
    const changed=db.query("UPDATE bank_accounts SET ledger_account_no=? WHERE id=? AND ledger_account_no IS NULL RETURNING id").get(input.ledgerAccountNo,input.bankAccountId) as {id:number}|null;if(!changed) return {ok:false as const,errors:["BANK_ACCOUNT_ALREADY_BOUND"]};
    const row=db.query("INSERT INTO legacy_bank_account_bindings(bank_account_id,ledger_account_no,cutoff,statement_transaction_id,statement_transaction_hash,statement_balance_ore,ledger_balance_ore,ledger_head_hash,audit_head_hash,plan_hash,idempotency_key,actor,principal_kind,principal_subject_id,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?) RETURNING id").get(input.bankAccountId,input.ledgerAccountNo,input.cutoff,plan.plan.statementTransactionId,plan.plan.statementTransactionHash,plan.plan.statementBalanceOre,plan.plan.ledgerBalanceOre,plan.plan.ledgerHeadHash,plan.plan.auditHeadHash,input.planHash,key,actor,principal.kind,principal.subjectId,new Date().toISOString()) as {id:number};
    insertAuditLog(db,{eventType:"legacy_bank_account_bound",entityType:"bank_account",entityId:input.bankAccountId,message:`Bound previously unassigned bank account to ${input.ledgerAccountNo} at reviewed cutoff ${input.cutoff}`,createdBy:actor,createdByProgram:"legacy-bank-binding"});return {ok:true as const,idempotent:false,id:row.id,planHash:input.planHash,errors:[] as string[]};
  }).immediate();}catch(e){return {ok:false as const,errors:[e instanceof Error?e.message:String(e)]};}
}

function payableContext(db:Database,input:LegacyPayablePaymentBackfillInput){
  const errors:string[]=[]; for(const value of [input.purchaseJournalEntryId,input.paymentJournalEntryId,input.documentId,input.bankTransactionId])if(!validId(value))errors.push("EXACT_IDENTITIES_REQUIRED");
  const purchase=db.query("SELECT id,document_id,transaction_date,entry_hash,status,reversal_of_entry_id,source_bank_transaction_id FROM journal_entries WHERE id=?").get(input.purchaseJournalEntryId) as any;
  const payment=db.query("SELECT id,document_id,transaction_date,entry_hash,status,reversal_of_entry_id,source_bank_transaction_id FROM journal_entries WHERE id=?").get(input.paymentJournalEntryId) as any;
  const document=db.query("SELECT id,sha256_hash,invoice_date,amount_inc_vat,vat_amount,currency,supplier_name,invoice_no FROM documents WHERE id=?").get(input.documentId) as any;
  const bank=db.query("SELECT id,bank_account_id,transaction_date,amount,currency,transaction_hash FROM bank_transactions WHERE id=?").get(input.bankTransactionId) as any;
  if(!purchase||!payment||!document||!bank) errors.push("EXACT_SOURCE_NOT_FOUND");
  const live=(j:any)=>j&&j.status==="posted"&&j.reversal_of_entry_id==null&&!db.query("SELECT 1 FROM journal_entries WHERE reversal_of_entry_id=?").get(j.id);
  if(!live(purchase)||!live(payment))errors.push("POSTED_UNREVERSED_JOURNALS_REQUIRED");
  if(purchase&&purchase.document_id!==input.documentId)errors.push("PURCHASE_DOCUMENT_MISMATCH");
  if(purchase?.source_bank_transaction_id!=null)errors.push("PURCHASE_MUST_NOT_HAVE_BANK_SOURCE");
  if(!document||String(document.currency).toUpperCase()!=="DKK"||!document.invoice_date||!(Number(document.amount_inc_vat)>0))errors.push("DKK_PURCHASE_DOCUMENT_REQUIRED");
  if(!bank||String(bank.currency).toUpperCase()!=="DKK"||Number(bank.amount)>=0||!validHash(bank.transaction_hash))errors.push("OUTGOING_DKK_BANK_REQUIRED");
  if(document&&bank&&ore(Math.abs(Number(bank.amount)))!==ore(document.amount_inc_vat))errors.push("PURCHASE_PAYMENT_AMOUNT_MISMATCH");
  if(payment&&payment.source_bank_transaction_id!==input.bankTransactionId)errors.push("PAYMENT_BANK_SOURCE_MISMATCH");
  if(payment&&payment.document_id!==input.documentId)errors.push("PAYMENT_DOCUMENT_MISMATCH");
  const creditor=resolveAccountRole(db,"creditors"), bankRole=resolveAccountRole(db,"bank"); if(!creditor.ok||!bankRole.ok)errors.push("CONFIRMED_CREDITOR_AND_BANK_ROLES_REQUIRED");
  const lineTotals=(id:number,account:string)=>db.query("SELECT COALESCE(SUM(debit_amount),0) debit,COALESCE(SUM(credit_amount),0) credit FROM journal_lines l JOIN accounts a ON a.id=l.account_id WHERE l.journal_entry_id=? AND a.account_no=?").get(id,account) as {debit:number;credit:number};
  if(purchase&&creditor.ok){const x=lineTotals(purchase.id,creditor.accountNo);if(ore(x.credit)!==ore(document?.amount_inc_vat)||ore(x.debit)!==0n)errors.push("PURCHASE_CREDITOR_CREDIT_MISMATCH"); const legacyControls=hasDocumentedLegacyReverseChargeControls(db,purchase.id); const bad=db.query("SELECT a.account_no,a.type FROM journal_lines l JOIN accounts a ON a.id=l.account_id WHERE l.journal_entry_id=? AND a.account_no<>? AND a.type NOT IN ('expense','vat')").all(purchase.id,creditor.accountNo) as Array<{account_no:string;type:string}>;if(bad.some(line=>!legacyControls||!['64040','64060'].includes(line.account_no)))errors.push("PURCHASE_LINES_NOT_EXPENSE_OR_VAT");}
  if(payment&&creditor.ok&&bankRole.ok){const c=lineTotals(payment.id,creditor.accountNo), b=lineTotals(payment.id,bankRole.accountNo), amount=ore(Math.abs(Number(bank?.amount)));if(ore(c.debit)!==amount||ore(c.credit)!==0n||ore(b.credit)!==amount||ore(b.debit)!==0n)errors.push("PAYMENT_CREDITOR_BANK_MISMATCH");}
  if(payment&&bank&&payment.transaction_date!==bank.transaction_date)errors.push("PAYMENT_DATE_MISMATCH"); if(purchase&&payment&&payment.transaction_date<purchase.transaction_date)errors.push("PAYMENT_BEFORE_PURCHASE");
  const reconciliation=db.query("SELECT journal_entry_id FROM bank_journal_reconciliations WHERE bank_transaction_id=?").all(input.bankTransactionId) as Array<{journal_entry_id:number}>;if(reconciliation.length!==1||reconciliation[0]?.journal_entry_id!==input.paymentJournalEntryId)errors.push("EXACT_PAYMENT_RECONCILIATION_REQUIRED");
  if(db.query("SELECT 1 FROM payables WHERE document_id=? OR journal_entry_id=?").get(input.documentId,input.purchaseJournalEntryId)||db.query("SELECT 1 FROM payable_payments WHERE bank_transaction_id=? OR journal_entry_id=?").get(input.bankTransactionId,input.paymentJournalEntryId)||db.query("SELECT 1 FROM legacy_payable_payment_backfills WHERE purchase_journal_entry_id=? OR payment_journal_entry_id=? OR document_id=? OR bank_transaction_id=?").get(input.purchaseJournalEntryId,input.paymentJournalEntryId,input.documentId,input.bankTransactionId)) errors.push("EXISTING_PAYABLE_OR_BACKFILL_CONFLICT");
  const state={contract:"rentemester-legacy-payable-payment-backfill-v1",purchaseJournalEntryId:input.purchaseJournalEntryId,paymentJournalEntryId:input.paymentJournalEntryId,documentId:input.documentId,bankTransactionId:input.bankTransactionId,purchaseJournalHash:purchase?.entry_hash??null,paymentJournalHash:payment?.entry_hash??null,documentHash:document?.sha256_hash??null,bankTransactionHash:bank?.transaction_hash??null,amountOre:bank?ore(Math.abs(Number(bank.amount))).toString():"0",currency:"DKK",purchaseDate:purchase?.transaction_date??null,paymentDate:payment?.transaction_date??null,creditorAccountNo:creditor.ok?creditor.accountNo:null,bankAccountNo:bankRole.ok?bankRole.accountNo:null,ledgerHeadHash:ledgerHead(db),auditHeadHash:auditHead(db)};
  return {errors,state};
}
export function planLegacyPayablePaymentBackfill(db:Database,input:LegacyPayablePaymentBackfillInput){const c=payableContext(db,input);if(c.errors.length)return {ok:false as const,errors:c.errors};return {ok:true as const,plan:{...c.state,planHash:digest(c.state)},errors:[] as string[]};}
export function applyLegacyPayablePaymentBackfill(db:Database,input:ApplyLegacyPayablePaymentBackfillInput){
  if(!input.confirm)return {ok:false as const,errors:["CONFIRMATION_REQUIRED"]};const actor=text(input.actor),principal=input.principal,key=text(input.idempotencyKey);if(!actor||!principal?.subjectId||!["user","service-account"].includes(principal.kind))return {ok:false as const,errors:["ACTOR_AND_PRINCIPAL_REQUIRED"]};if(!key||key.length>128)return {ok:false as const,errors:["IDEMPOTENCY_KEY_REQUIRED"]};
  try{return db.transaction(()=>{const replay=db.query("SELECT id,plan_hash,payable_id,payment_id FROM legacy_payable_payment_backfills WHERE principal_kind=? AND principal_subject_id=? AND idempotency_key=?").get(principal.kind,principal.subjectId,key) as any;if(replay)return replay.plan_hash===input.planHash?{ok:true as const,idempotent:true,id:replay.id,payableId:replay.payable_id,paymentId:replay.payment_id,errors:[] as string[]}:{ok:false as const,errors:["IDEMPOTENCY_CONFLICT"]};const plan=planLegacyPayablePaymentBackfill(db,input);if(!plan.ok)return plan;if(plan.plan.planHash!==input.planHash)return {ok:false as const,errors:["PLAN_HASH_MISMATCH"]};const d=db.query("SELECT supplier_name,invoice_no,invoice_date,amount_inc_vat,vat_amount FROM documents WHERE id=?").get(input.documentId) as any;const due=d.invoice_date;const payable=db.query("INSERT INTO payables(document_id,supplier_name,bill_no,bill_date,due_date,gross_amount,net_amount,vat_amount,currency,journal_entry_id,note) VALUES(?,?,?,?,?,?,?,?,?,?,?) RETURNING id").get(input.documentId,d.supplier_name??null,d.invoice_no??null,d.invoice_date,due,d.amount_inc_vat,Number(d.amount_inc_vat)-Number(d.vat_amount??0),d.vat_amount??0,"DKK",input.purchaseJournalEntryId,"Legacy explicit backfill") as {id:number};const payment=db.query("INSERT INTO payable_payments(payable_id,bank_transaction_id,journal_entry_id,payment_date,amount,currency,note) VALUES(?,?,?,?,?,?,?) RETURNING id").get(payable.id,input.bankTransactionId,input.paymentJournalEntryId,plan.plan.paymentDate,Math.abs(Number((db.query("SELECT amount FROM bank_transactions WHERE id=?").get(input.bankTransactionId) as any).amount)),"DKK","Legacy explicit backfill") as {id:number};const row=db.query("INSERT INTO legacy_payable_payment_backfills(purchase_journal_entry_id,payment_journal_entry_id,document_id,bank_transaction_id,payable_id,payment_id,purchase_journal_hash,payment_journal_hash,document_hash,bank_transaction_hash,plan_hash,ledger_head_hash,audit_head_hash,idempotency_key,actor,principal_kind,principal_subject_id,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?) RETURNING id").get(input.purchaseJournalEntryId,input.paymentJournalEntryId,input.documentId,input.bankTransactionId,payable.id,payment.id,plan.plan.purchaseJournalHash,plan.plan.paymentJournalHash,plan.plan.documentHash,plan.plan.bankTransactionHash,input.planHash,plan.plan.ledgerHeadHash,plan.plan.auditHeadHash,key,actor,principal.kind,principal.subjectId,new Date().toISOString()) as {id:number};insertAuditLog(db,{eventType:"legacy_payable_payment_backfilled",entityType:"payable",entityId:payable.id,message:`Backfilled payable ${payable.id} and payment ${payment.id} from explicit existing journal and bank identities`,createdBy:actor,createdByProgram:"legacy-payable-payment-backfill"});return {ok:true as const,idempotent:false,id:row.id,payableId:payable.id,paymentId:payment.id,errors:[] as string[]};}).immediate();}catch(e){return {ok:false as const,errors:[e instanceof Error?e.message:String(e)]};}
}
