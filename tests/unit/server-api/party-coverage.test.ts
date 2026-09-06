import { describe, expect, test } from "bun:test";
import { addBankAccount } from "../../../src/core/bank";
import { seedAccounts, postJournalEntry } from "../../../src/core/ledger";
import { createParty, linkPartyRole } from "../../../src/core/party-registry";
import { openWorkspaceControlDb } from "../../../src/core/workspace-control";
import { companyPaths, companyRootForSlug, config, get, makeWorkspace, migrate, openDb, rmSync } from "./_shared";

describe("#644 party coverage HTTP parity",()=>{
  test("projects, plans and applies the exact hash without changing economic records",async()=>{
    const workspace=makeWorkspace("party-coverage-http",["Synthetic Company"]);const slug="synthetic-company";const root=companyRootForSlug(workspace,slug);
    try{
      const db=openDb(companyPaths(root).db);migrate(db);seedAccounts(db);const bank=addBankAccount(db,{name:"Bank",slug:"bank",ledgerAccountNo:"2000"}).account!;
      const document=db.query("INSERT INTO documents(document_no,source,sha256_hash,payload_json,status,document_type,sender_name,sender_vat_cvr,supplier_country_code,supplier_identifier_kind,retain_until) VALUES('COVERAGE-HTTP','synthetic',?,'{}','posted','purchase_sale','Typed Supplier','DK55667788','DK','dk_cvr','2031-12-31') RETURNING id").get("e".repeat(64)) as {id:number};
      db.query("INSERT INTO bank_transactions(id,transaction_date,text,amount,currency,transaction_hash,bank_account_id,retain_until) VALUES(900,'2026-09-04','Payment',-100,'DKK','coverage-http-900',?,'2031-12-31')").run(bank.id);
      const posted=postJournalEntry(db,{transactionDate:"2026-09-04",text:"Purchase",documentId:document.id,sourceBankTransactionId:900,createdBy:"agent:test",lines:[{accountNo:"7000",debitAmount:100},{accountNo:"2000",creditAmount:100}]});if(!posted.ok)throw new Error(posted.errors.join("; "));
      const economic={bank:db.query("SELECT * FROM bank_transactions").all(),journals:db.query("SELECT * FROM journal_entries").all(),lines:db.query("SELECT * FROM journal_lines").all(),documents:db.query("SELECT * FROM documents").all()};db.close();
      const control=openWorkspaceControlDb(workspace);createParty(control,{partyId:"party-http-644",kind:"organization",name:"Typed Supplier",identifiers:[{country:"DK",identifierKind:"dk_cvr",identifier:"DK55667788"}],source:"synthetic",observedAt:"2026-09-04T00:00:00.000Z",reviewAssertion:"reviewed",actor:"user:test"});linkPartyRole(control,{partyId:"party-http-644",companySlug:slug,role:"vendor",actor:"user:test"});control.close();
      const hosted=config({workspaceRoot:workspace});const projected=await get(hosted,`/api/companies/${slug}/documents/party-coverage`);expect(projected).toMatchObject({status:200,body:{ok:true,totals:{exact_candidate:1},rows:[{bankTransactionId:900,status:"exact_candidate"}]}});
      const planned=await get(hosted,`/api/companies/${slug}/documents/party-coverage/plan`,{method:"POST",headers:{"content-type":"application/json",host:"127.0.0.1"},body:"{}"});expect(planned).toMatchObject({status:200,body:{ok:true,plan:{operations:[{kind:"document_link"}]}}});expect(projected.body.planHash).toBe(planned.body.plan.planHash);
      const denied=await get(hosted,`/api/companies/${slug}/documents/party-coverage/apply`,{method:"POST",headers:{"content-type":"application/json",host:"127.0.0.1"},body:JSON.stringify({planHash:planned.body.plan.planHash,idempotencyKey:"http-644",confirm:false})});expect(denied.status).toBe(400);
      const body={planHash:planned.body.plan.planHash,idempotencyKey:"http-644",confirm:true};const applied=await get(hosted,`/api/companies/${slug}/documents/party-coverage/apply`,{method:"POST",headers:{"content-type":"application/json",host:"127.0.0.1"},body:JSON.stringify(body)});expect(applied).toMatchObject({status:200,body:{ok:true,idempotent:false,applied:1}});expect(await get(hosted,`/api/companies/${slug}/documents/party-coverage/apply`,{method:"POST",headers:{"content-type":"application/json",host:"127.0.0.1"},body:JSON.stringify(body)})).toMatchObject({status:200,body:{ok:true,idempotent:true}});
      const after=openDb(companyPaths(root).db);try{expect({bank:after.query("SELECT * FROM bank_transactions").all(),journals:after.query("SELECT * FROM journal_entries").all(),lines:after.query("SELECT * FROM journal_lines").all(),documents:after.query("SELECT * FROM documents").all()}).toEqual(economic);}finally{after.close();}
    }finally{rmSync(workspace,{recursive:true,force:true});}
  });
});
