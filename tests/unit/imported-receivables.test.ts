import { Database } from "bun:sqlite";
import { describe, expect, test } from "bun:test";
import { migrate } from "../../src/core/db";
import { seedAccounts } from "../../src/core/ledger";
import { applyLegacyImportedReceivableBackfill, importedReceivableBalanceOre, listImportedReceivables, planLegacyImportedReceivableBackfill, recordImportedReceivableSchedule, validateImportedReceivableSchedule } from "../../src/core/imported-receivables";

const hash = (letter: string) => letter.repeat(64);
function db() { const value = new Database(":memory:"); value.exec("PRAGMA foreign_keys = ON"); migrate(value); seedAccounts(value); value.query("INSERT INTO dinero_import_sources(id,raw_sha256,raw_size_bytes,canonical_listing_sha256,canonical_listing_count) VALUES(1,?,1,?,0)").run(hash("a"),hash("b")); value.query("INSERT INTO dinero_import_inventories(id,source_id,source_raw_sha256,canonical_listing_sha256,canonical_listing_count,entry_count,total_size_bytes) VALUES(1,1,?,?,0,0,0)").run(hash("a"),hash("b")); value.query("INSERT INTO dinero_import_attempts(id,inventory_id,source_id,source_raw_sha256,parser_contract,actor,cutover_date,outcome,result_sha256) VALUES(1,1,1,?,'dinero-v4','agent:test','2025-01-01','accepted',?)").run(hash("a"),hash("c")); return value; }
const schedule = { contract:"rentemester-imported-receivables-v1" as const, sourceDocumentHash:hash("d"), invoices:[
  { id:"INV-partial",customerId:"customer-1",customerName:"Synthetic customer",invoiceDate:"2025-01-01",dueDate:"2025-01-15",grossAmount:100,controlAccountNo:"1200",recognitionRef:"opening",documentHash:hash("e"),payments:[{id:"PAY-1",paymentDate:"2025-01-10",amount:25,paymentRef:"voucher-1",documentHash:hash("f")}] },
  { id:"INV-paid",invoiceDate:"2025-01-02",grossAmount:20,controlAccountNo:"1200",recognitionRef:"opening",documentHash:hash("1"),payments:[{id:"PAY-2",paymentDate:"2025-01-03",amount:20,paymentRef:"voucher-2",documentHash:hash("2")}] },
  { id:"INV-credit",invoiceDate:"2025-01-04",grossAmount:40,controlAccountNo:"1200",recognitionRef:"opening",documentHash:hash("3"),payments:[{id:"CN-1",eventKind:"credit_note" as const,paymentDate:"2025-01-05",amount:10,paymentRef:"credit-note-1",documentHash:hash("4")}] },
] };

describe("imported receivables v37", () => {
  test("keeps source-evidenced imported invoices and payments exact at arbitrary cutoffs", () => {
    const value=db(); expect(recordImportedReceivableSchedule(value,1,schedule,"2025-01-31")).toMatchObject({ok:true});
    expect(importedReceivableBalanceOre(value,"2025-01-02","1200").total).toBe(12000n);
    expect(importedReceivableBalanceOre(value,"2025-01-10","1200").total).toBe(10500n);
    expect(importedReceivableBalanceOre(value,"2025-01-31","1200").total).toBe(10500n);
    const evidence=importedReceivableBalanceOre(value,"2025-01-31","1200").evidence; expect(evidence).toHaveLength(3); expect(evidence[0]).toMatchObject({externalInvoiceId:"INV-partial",customerExternalId:"customer-1",sourceDocumentHash:hash("e")});
    value.close();
  });
  test("is replay-safe and fails closed on conflicts and amount-only invented data", () => {
    const value=db(); const first=recordImportedReceivableSchedule(value,1,schedule,"2025-01-31"); expect(first.ok).toBe(true); expect(recordImportedReceivableSchedule(value,1,schedule,"2025-01-31")).toMatchObject({ok:true,scheduleHash:first.scheduleHash});
    expect(recordImportedReceivableSchedule(value,1,{...schedule,invoices:[{...schedule.invoices[0]!,grossAmount:101}]},"2025-01-31").errors.join(" ")).toContain("conflicts with accepted source");
    expect(recordImportedReceivableSchedule(value,1,schedule,"2025-02-01").errors.join(" ")).toContain("control date");
    expect(validateImportedReceivableSchedule({contract:"rentemester-imported-receivables-v1",sourceDocumentHash:hash("a"),invoices:[{id:"x",invoiceDate:"2025-01-01",grossAmount:1,controlAccountNo:"1200"}]}).ok).toBe(false); value.close();
  });
  test("keeps the accepted cut-over boundary append-only", () => {
    const value=db(); expect(recordImportedReceivableSchedule(value,1,schedule,"2025-01-31").ok).toBe(true);
    expect(()=>value.run("UPDATE imported_receivable_boundaries SET control_date='2025-02-01' WHERE dinero_import_attempt_id=1")).toThrow("append-only");
    expect(()=>value.run("DELETE FROM imported_receivable_boundaries WHERE dinero_import_attempt_id=1")).toThrow("append-only");
    value.close();
  });
  test("keeps the archive read model separate from native invoices and exposes its evidence boundary", () => {
    const value=db(); expect(recordImportedReceivableSchedule(value,1,schedule,"2025-01-31").ok).toBe(true);
    const read=listImportedReceivables(value,"2025-01-31");
    expect(read).toMatchObject({ok:true,count:3,totalOpen:105,boundary:expect.stringContaining("native")});
    expect(read.rows.map(row=>row.openBalance)).toEqual([75,0,30]);
    expect(read.rows[0]).toMatchObject({source:"imported",externalInvoiceId:"INV-partial",sourceDocumentHash:hash("e"),scheduleHash:expect.any(String)});
    value.close();
  });
  test("rejects future imported claims and events beyond the authoritative cut-over", () => {
    const futureInvoice={...schedule,invoices:[...schedule.invoices,{...schedule.invoices[0]!,id:"INV-future",invoiceDate:"2025-02-01",payments:[]}]};
    const futureInvoiceResult=validateImportedReceivableSchedule(futureInvoice,"2025-01-31");
    expect(futureInvoiceResult.ok).toBe(false);
    if (!futureInvoiceResult.ok) expect(futureInvoiceResult.errors.join(" ")).toContain("after control date");
    const futurePayment={...schedule,invoices:[{...schedule.invoices[0]!,payments:[{...schedule.invoices[0]!.payments![0]!,paymentDate:"2025-02-01"}]}]};
    const futurePaymentResult=validateImportedReceivableSchedule(futurePayment,"2025-01-31");
    expect(futurePaymentResult.ok).toBe(false);
    if (!futurePaymentResult.ok) expect(futurePaymentResult.errors.join(" ")).toContain("after control date");
  });
});

describe("legacy imported receivable backfill v38",()=>{
  const legacySchedule={contract:"rentemester-imported-receivables-v1" as const,sourceDocumentHash:hash("d"),invoices:[
    {id:"OPEN-1",invoiceDate:"2025-01-01",grossAmount:150,controlAccountNo:"1100",recognitionRef:"legacy:invoice:1",documentHash:hash("e"),payments:[{id:"PAY-1",paymentDate:"2025-01-20",amount:50,paymentRef:"legacy:payment:1",documentHash:hash("f")}]},
    {id:"PAID-2",invoiceDate:"2025-01-02",grossAmount:25,controlAccountNo:"1100",recognitionRef:"legacy:invoice:2",documentHash:hash("1"),payments:[{id:"PAY-2",paymentDate:"2025-01-15",amount:25,paymentRef:"legacy:payment:2",documentHash:hash("2")}]},
  ]};
  function legacy(){const value=db();value.query("INSERT INTO journal_entries(id,entry_no,transaction_date,text,rule_version,created_by,created_by_program,status,previous_hash,entry_hash,locked) VALUES(1,'2025-00001','2025-01-31','legacy control','test','agent:test','rentemester-import:dinero','posted','GENESIS',?,1)").run(hash("9"));const debtors=value.query("SELECT id FROM accounts WHERE account_no='1100'").get() as {id:number};const equity=value.query("SELECT id FROM accounts WHERE account_no='5000'").get() as {id:number};value.query("INSERT INTO journal_lines(journal_entry_id,account_id,debit_amount,credit_amount,currency) VALUES(1,?,100,0,'DKK')").run(debtors.id);value.query("INSERT INTO journal_lines(journal_entry_id,account_id,debit_amount,credit_amount,currency) VALUES(1,?,0,100,'DKK')").run(equity.id);return value;}
  function addNativeInvoice(value:Database,date:string,id:number){
    const debtors=value.query("SELECT id FROM accounts WHERE account_no='1100'").get() as {id:number};
    const revenue=value.query("SELECT id FROM accounts WHERE account_no='1000'").get() as {id:number};
    value.query("INSERT INTO documents(id,document_no,source,sha256_hash,invoice_no,invoice_date,amount_inc_vat,document_type) VALUES(?,?,?,?,?,?,?,'issued_invoice')").run(id,`DOC-${id}`,"synthetic",hash(String(id%10)),`INV-${id}`,date,10);
    value.query("INSERT INTO journal_entries(id,entry_no,transaction_date,text,rule_version,created_by,created_by_program,status,document_id,previous_hash,entry_hash,locked) VALUES(?,?,?,'native invoice','test','agent:test','test','posted',?,?,?,1)").run(id,`2025-${String(id).padStart(5,"0")}`,date,id,hash("9"),hash(String((id+1)%10)));
    value.query("INSERT INTO journal_lines(journal_entry_id,account_id,debit_amount,credit_amount,currency) VALUES(?,?,10,0,'DKK')").run(id,debtors.id);
    value.query("INSERT INTO journal_lines(journal_entry_id,account_id,debit_amount,credit_amount,currency,vat_code) VALUES(?,?,0,10,'DKK','OUT25')").run(id,revenue.id);
    value.query("INSERT INTO issued_invoice_postings(invoice_document_id,journal_entry_id,receivable_account_id,booked_gross_dkk) VALUES(?,?,?,10)").run(id,id,debtors.id);
  }
  const input=(artifactSha256:string)=>({dineroImportAttemptId:1,sourceRawSha256:hash("a"),canonicalInventorySha256:hash("b"),controlDate:"2025-01-31",controlAccountNo:"1100",artifactSha256,schedule:legacySchedule});

  test("plans read-only against exact immutable source, ledger and audit heads",()=>{const value=legacy();const scheduleHash=validateImportedReceivableSchedule(legacySchedule,"2025-01-31");expect(scheduleHash.ok).toBe(true);if(!scheduleHash.ok)return;const before=value.query("SELECT COUNT(*) n FROM imported_receivable_headers").get() as {n:number};const result=planLegacyImportedReceivableBackfill(value,input(scheduleHash.hash));expect(result).toMatchObject({ok:true,alreadyApplied:false,plan:{sourceRawSha256:hash("a"),canonicalInventorySha256:hash("b"),ledgerBalanceOre:"10000",scheduleBalanceOre:"10000",ledgerHeadHash:hash("9"),planHash:expect.any(String)}});expect((value.query("SELECT COUNT(*) n FROM imported_receivable_headers").get() as {n:number}).n).toBe(before.n);value.close();});
  test("applies atomically without replaying journals, documents, archives or import attempts",()=>{const value=legacy();const checked=validateImportedReceivableSchedule(legacySchedule,"2025-01-31");if(!checked.ok)throw new Error("fixture");const proposal=planLegacyImportedReceivableBackfill(value,input(checked.hash));if(!proposal.ok)throw new Error(proposal.errors.join());const counts=()=>({journals:(value.query("SELECT COUNT(*) n FROM journal_entries").get() as any).n,documents:(value.query("SELECT COUNT(*) n FROM documents").get() as any).n,attempts:(value.query("SELECT COUNT(*) n FROM dinero_import_attempts").get() as any).n,archives:(value.query("SELECT COUNT(*) n FROM import_archive_years").get() as any).n});const before=counts();const applied=applyLegacyImportedReceivableBackfill(value,{...input(checked.hash),planHash:proposal.plan.planHash,idempotencyKey:"backfill-1",actor:"agent:test",principal:{kind:"service-account",subjectId:"svc:test"},confirm:true});expect(applied).toMatchObject({ok:true,idempotent:false,scheduleHash:checked.hash});expect(counts()).toEqual(before);expect(listImportedReceivables(value,"2025-01-31")).toMatchObject({ok:true,count:2,totalOpen:100});expect((value.query("SELECT event_type FROM audit_log ORDER BY id DESC LIMIT 1").get() as any).event_type).toBe("legacy_imported_receivables_backfilled");expect(()=>value.run("UPDATE legacy_imported_receivable_backfills SET control_date='2025-02-01'")).toThrow("append-only");value.close();});
  test("is idempotent and fails closed on stale plans, conflicts, overlap and missing identity",()=>{const value=legacy();const checked=validateImportedReceivableSchedule(legacySchedule,"2025-01-31");if(!checked.ok)throw new Error("fixture");const proposal=planLegacyImportedReceivableBackfill(value,input(checked.hash));if(!proposal.ok)throw new Error("fixture");const apply={...input(checked.hash),planHash:proposal.plan.planHash,idempotencyKey:"same-key",actor:"agent:test",principal:{kind:"service-account" as const,subjectId:"svc:test"},confirm:true};expect(applyLegacyImportedReceivableBackfill(value,apply)).toMatchObject({ok:true,idempotent:false});expect(applyLegacyImportedReceivableBackfill(value,apply)).toMatchObject({ok:true,idempotent:true});expect(applyLegacyImportedReceivableBackfill(value,{...apply,planHash:hash("0")})).toMatchObject({ok:false,errors:["IDEMPOTENCY_CONFLICT"]});expect(applyLegacyImportedReceivableBackfill(value,{...apply,idempotencyKey:"new",actor:""})).toMatchObject({ok:false,errors:["ACTOR_AND_PRINCIPAL_REQUIRED"]});value.close();const stale=legacy();const plan=planLegacyImportedReceivableBackfill(stale,input(checked.hash));if(!plan.ok)throw new Error("fixture");stale.query("INSERT INTO audit_log(event_type,entity_type,message,actor) VALUES('other','test','changed','agent:test')").run();expect(applyLegacyImportedReceivableBackfill(stale,{...apply,planHash:plan.plan.planHash})).toMatchObject({ok:false,errors:["PLAN_HASH_MISMATCH"]});stale.close();});
  test("rejects mismatched evidence and native overlap while allowing native post-cut-over invoices",()=>{
    const checked=validateImportedReceivableSchedule(legacySchedule,"2025-01-31");if(!checked.ok)throw new Error("fixture");
    const evidence=legacy();
    expect(planLegacyImportedReceivableBackfill(evidence,{...input(checked.hash),sourceRawSha256:hash("8")})).toMatchObject({ok:false,errors:expect.arrayContaining(["SOURCE_RAW_HASH_MISMATCH"])});
    expect(planLegacyImportedReceivableBackfill(evidence,{...input(checked.hash),canonicalInventorySha256:hash("7")})).toMatchObject({ok:false,errors:expect.arrayContaining(["INVENTORY_HASH_MISMATCH"])});
    expect(planLegacyImportedReceivableBackfill(evidence,{...input(hash("6"))})).toMatchObject({ok:false,errors:expect.arrayContaining(["ARTIFACT_HASH_MISMATCH"])});
    evidence.close();
    const overlap=legacy();addNativeInvoice(overlap,"2025-01-30",2);
    expect(planLegacyImportedReceivableBackfill(overlap,input(checked.hash))).toMatchObject({ok:false,errors:expect.arrayContaining(["NATIVE_RECEIVABLE_OVERLAP"])});overlap.close();
    const coexist=legacy();addNativeInvoice(coexist,"2025-02-01",2);
    expect(planLegacyImportedReceivableBackfill(coexist,input(checked.hash))).toMatchObject({ok:true,alreadyApplied:false});coexist.close();
  });
});
