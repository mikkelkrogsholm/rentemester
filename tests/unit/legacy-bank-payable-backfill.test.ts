import { Database } from "bun:sqlite";
import { describe, expect, test } from "bun:test";
import { migrate } from "../../src/core/db";
import { seedAccounts, postJournalEntry } from "../../src/core/ledger";
import { applyLegacyBankBinding, applyLegacyPayablePaymentBackfill, planLegacyBankBinding, planLegacyPayablePaymentBackfill } from "../../src/core/legacy-bank-payable-backfill";

const hash=(c:string)=>c.repeat(64);
function fresh(){const db=new Database(":memory:");db.exec("PRAGMA foreign_keys=ON");migrate(db);seedAccounts(db);return db;}
function insertBank(db:Database,id:number,accountId:number,amount:number,date:string,letter:string,balance?:number){db.query("INSERT INTO bank_transactions(id,transaction_date,text,amount,currency,transaction_hash,bank_account_id,balance_after) VALUES(?,?, 'synthetic',?,'DKK',?,?,?)").run(id,date,amount,hash(letter),accountId,balance??null);}

describe("legacy bank/payable adoption (#601)",()=>{
  test("binds only an exact NULL legacy bank account at a balanced cutoff and refuses remapping",()=>{
    const db=fresh();
    db.query("INSERT INTO bank_accounts(id,slug,name,currency,ledger_account_no) VALUES(1,'legacy','Legacy','DKK',NULL)").run();
    insertBank(db,1,1,100,"2026-01-31","a",100);
    expect(postJournalEntry(db,{transactionDate:"2026-01-31",text:"opening bank",lines:[{accountNo:"2000",debitAmount:100},{accountNo:"5000",creditAmount:100}]}).ok).toBe(true);
    const plan=planLegacyBankBinding(db,{bankAccountId:1,ledgerAccountNo:"2000",cutoff:"2026-01-31"});expect(plan.ok,plan.ok?"":plan.errors.join("; ")).toBe(true);if(!plan.ok)return;
    const journals=(db.query("SELECT COUNT(*) n FROM journal_entries").get() as any).n, bankRows=(db.query("SELECT COUNT(*) n FROM bank_transactions").get() as any).n;
    const applied=applyLegacyBankBinding(db,{bankAccountId:1,ledgerAccountNo:"2000",cutoff:"2026-01-31",planHash:plan.plan.planHash,idempotencyKey:"bind-1",actor:"agent:test",principal:{kind:"service-account",subjectId:"svc-test"},confirm:true});expect(applied).toMatchObject({ok:true,idempotent:false});
    expect(db.query("SELECT ledger_account_no FROM bank_accounts WHERE id=1").get()).toEqual({ledger_account_no:"2000"});expect((db.query("SELECT COUNT(*) n FROM journal_entries").get() as any).n).toBe(journals);expect((db.query("SELECT COUNT(*) n FROM bank_transactions").get() as any).n).toBe(bankRows);
    expect(applyLegacyBankBinding(db,{bankAccountId:1,ledgerAccountNo:"2000",cutoff:"2026-01-31",planHash:plan.plan.planHash,idempotencyKey:"bind-1",actor:"agent:test",principal:{kind:"service-account",subjectId:"svc-test"},confirm:true})).toMatchObject({ok:true,idempotent:true});
    expect(planLegacyBankBinding(db,{bankAccountId:1,ledgerAccountNo:"2000",cutoff:"2026-01-31"})).toMatchObject({ok:false,errors:["BANK_ACCOUNT_ALREADY_BOUND"]});expect(()=>db.run("UPDATE legacy_bank_account_bindings SET cutoff='2026-02-01'")).toThrow("append-only");db.close();
  });

  test("backfills only an explicit causal purchase/payment quartet, never amount candidates or journals",()=>{
    const db=fresh();db.query("INSERT INTO bank_accounts(id,slug,name,currency,ledger_account_no) VALUES(1,'bank','Bank','DKK','2000')").run();insertBank(db,1,1,-125,"2026-02-10","b");
    db.query("INSERT INTO documents(id,source,sha256_hash,supplier_name,invoice_no,invoice_date,amount_inc_vat,vat_amount,currency,document_type) VALUES(1,'synthetic',?,'Synthetic supplier','BILL-1','2026-02-01',125,25,'DKK','purchase_sale')").run(hash("c"));
    const purchase=postJournalEntry(db,{transactionDate:"2026-02-01",text:"purchase",documentId:1,lines:[{accountNo:"3000",debitAmount:100,vatCode:"DK_PURCHASE_25"},{accountNo:"4000",debitAmount:25},{accountNo:"7000",creditAmount:125}]});expect(purchase.ok,purchase.errors.join("; ")).toBe(true);
    const payment=postJournalEntry(db,{transactionDate:"2026-02-10",text:"payment",documentId:1,sourceBankTransactionId:1,lines:[{accountNo:"7000",debitAmount:125},{accountNo:"2000",creditAmount:125}]});expect(payment.ok).toBe(true);
    // A same-amount decoy must be inert: no API accepts an amount to choose it.
    db.query("INSERT INTO documents(id,source,sha256_hash,invoice_date,amount_inc_vat,vat_amount,currency,document_type) VALUES(2,'synthetic',?,'2026-02-01',125,25,'DKK','purchase_sale')").run(hash("d"));
    const input={purchaseJournalEntryId:purchase.entryId!,paymentJournalEntryId:payment.entryId!,documentId:1,bankTransactionId:1};const plan=planLegacyPayablePaymentBackfill(db,input);expect(plan.ok,plan.ok?"":plan.errors.join("; ")).toBe(true);if(!plan.ok)return;
    const before=(db.query("SELECT COUNT(*) n FROM journal_entries").get() as any).n;const applied=applyLegacyPayablePaymentBackfill(db,{...input,planHash:plan.plan.planHash,idempotencyKey:"payable-1",actor:"agent:test",principal:{kind:"service-account",subjectId:"svc-test"},confirm:true});expect(applied).toMatchObject({ok:true,idempotent:false});expect((db.query("SELECT COUNT(*) n FROM journal_entries").get() as any).n).toBe(before);expect(db.query("SELECT document_id,journal_entry_id FROM payables").get()).toEqual({document_id:1,journal_entry_id:purchase.entryId});expect(db.query("SELECT bank_transaction_id,journal_entry_id FROM payable_payments").get()).toEqual({bank_transaction_id:1,journal_entry_id:payment.entryId});
    expect(planLegacyPayablePaymentBackfill(db,{...input,documentId:2})).toMatchObject({ok:false,errors:expect.arrayContaining(["PURCHASE_DOCUMENT_MISMATCH"])});expect(()=>db.run("UPDATE legacy_payable_payment_backfills SET plan_hash=?",hash("e"))).toThrow("append-only");db.close();
  });

  test("accepts the documented legacy reverse-charge control pair, but not an arbitrary liability",()=>{
    const planFor=(outputAccountNo:"64040"|"7900")=>{
      const db=fresh();
      db.query("INSERT INTO bank_accounts(id,slug,name,currency,ledger_account_no) VALUES(1,'bank','Bank','DKK','2000')").run();insertBank(db,1,1,-125,"2026-02-10","e");
      db.query("INSERT INTO documents(id,source,sha256_hash,supplier_name,invoice_no,invoice_date,amount_inc_vat,vat_amount,currency,document_type) VALUES(1,'synthetic',?,'Synthetic supplier','RC-1','2026-02-01',125,0,'DKK','purchase_sale')").run(hash("s"));
      db.query("INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('64040','Legacy reverse-charge output VAT','liability','credit'),('64060','Legacy reverse-charge input VAT','liability','debit'),('7900','Arbitrary liability','liability','credit')").run();
      const purchase=postJournalEntry(db,{transactionDate:"2026-02-01",text:"reverse charge purchase",documentId:1,lines:[{accountNo:"3010",debitAmount:125,vatCode:"EU_SERVICE_REVERSE_CHARGE"},{accountNo:"64060",debitAmount:31.25},{accountNo:outputAccountNo,creditAmount:31.25},{accountNo:"7000",creditAmount:125}]});expect(purchase.ok,purchase.errors.join("; ")).toBe(true);
      const payment=postJournalEntry(db,{transactionDate:"2026-02-10",text:"payment",documentId:1,sourceBankTransactionId:1,lines:[{accountNo:"7000",debitAmount:125},{accountNo:"2000",creditAmount:125}]});expect(payment.ok).toBe(true);
      const plan=planLegacyPayablePaymentBackfill(db,{purchaseJournalEntryId:purchase.entryId!,paymentJournalEntryId:payment.entryId!,documentId:1,bankTransactionId:1});db.close();return plan;
    };
    expect(planFor("64040")).toMatchObject({ok:true});
    expect(planFor("7900")).toMatchObject({ok:false,errors:expect.arrayContaining(["PURCHASE_LINES_NOT_EXPENSE_OR_VAT"])});
  });
});
