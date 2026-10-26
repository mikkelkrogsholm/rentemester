import { afterEach, describe, expect, test } from "bun:test";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { Database } from "bun:sqlite";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { addBankAccount } from "../../src/core/bank";
import { migrate, openDb } from "../../src/core/db";
import { postJournalEntry, seedAccounts } from "../../src/core/ledger";
import { applyPartyCoverage, planPartyCoverage, projectPartyCoverage, type PartyCoverageDecision } from "../../src/core/party-coverage";
import { createParty, linkPartyRole } from "../../src/core/party-registry";
import { companyPaths } from "../../src/core/paths";
import { initWorkspace } from "../../src/core/workspace";
import { openWorkspaceControlDb } from "../../src/core/workspace-control";
import { buildTrialBalance } from "../../src/core/financial-statements";
import { buildVatReport } from "../../src/core/vat";

const roots:string[]=[];

function setupSharedDocument(){
  const root=mkdtempSync(join(tmpdir(),"party-row-"));roots.push(root);initWorkspace(root);
  const db=openDb(companyPaths(root).db);migrate(db);seedAccounts(db);db.run("INSERT INTO companies(id,name) VALUES(1,'Synthetic')");
  const bank=addBankAccount(db,{name:"Bank",slug:"bank",ledgerAccountNo:"2000"}).account!;
  const documentHash="d".repeat(64);
  db.query("INSERT INTO documents(id,source,sha256_hash,payload_json,status,document_type,sender_name) VALUES(1,'synthetic',?,'{}','posted','external_accounting_evidence','Shared source')").run(documentHash);
  for(const [id,hash,text] of [[1,"row-a","Authority payment"],[2,"row-b","Payee payment"]] as const){
    db.query("INSERT INTO bank_transactions(id,transaction_date,text,amount,currency,transaction_hash,bank_account_id) VALUES(?,? ,?,-100,'DKK',?,?)").run(id,`2026-01-0${id}`,text,hash,bank.id);
    const posted=postJournalEntry(db,{transactionDate:`2026-01-0${id}`,text,documentId:1,sourceBankTransactionId:id,createdBy:"agent:fixture",lines:[{accountNo:"7000",debitAmount:100},{accountNo:"2000",creditAmount:100}]});
    if(!posted.ok)throw new Error(posted.errors.join(","));
  }
  const registry=openWorkspaceControlDb(root);
  for(const party of [{partyId:"party-authority",name:"Public Authority",role:"authority" as const},{partyId:"party-payee",name:"Synthetic Payee",role:"employee" as const},{partyId:"party-replacement",name:"Replacement Payee",role:"employee" as const}]){
    createParty(registry,{partyId:party.partyId,kind:party.role==="employee"?"person":"organization",name:party.name,source:"synthetic",observedAt:"2026-01-01T00:00:00.000Z",reviewAssertion:"reviewed synthetic identity",actor:"user:test"});
    linkPartyRole(registry,{partyId:party.partyId,companySlug:"alpha",role:party.role,actor:"user:test"});
  }
  const projection=projectPartyCoverage(db,registry,{companySlug:"alpha"});
  const byId=(id:number)=>projection.rows.find(row=>row.bankTransactionId===id)!;
  const decisions:PartyCoverageDecision[]=[
    {bankTransactionId:1,scope:"bank_transaction",transactionHash:"row-a",documentHash,partyId:"party-authority",role:"authority",provenance:"reviewed_bank_statement_counterparty",evidenceReference:"synthetic row A",rationale:"row A names the authority"},
    {bankTransactionId:2,scope:"bank_transaction",transactionHash:"row-b",documentHash,partyId:"party-payee",role:"employee",provenance:"reviewed_bank_statement_counterparty",evidenceReference:"synthetic row B",rationale:"row B names the payee"},
  ];
  expect(byId(1).documentId).toBe(1);expect(byId(2).documentId).toBe(1);
  return {root,db,registry,documentHash,decisions};
}

afterEach(()=>{for(const root of roots.splice(0))rmSync(root,{recursive:true,force:true});});

describe("#646 bank-row-specific party coverage",()=>{
  test("schema v56 upgrades a v55 decision into the current append-only projection",()=>{
    const db=new Database(":memory:");
    db.exec("CREATE TABLE bank_transactions(id INTEGER PRIMARY KEY); CREATE TABLE journal_entries(id INTEGER PRIMARY KEY); CREATE TABLE documents(id INTEGER PRIMARY KEY); INSERT INTO bank_transactions VALUES(1); INSERT INTO journal_entries VALUES(1); INSERT INTO documents VALUES(1); CREATE TABLE party_coverage_bank_resolution_events(id INTEGER PRIMARY KEY,bank_transaction_id INTEGER NOT NULL UNIQUE REFERENCES bank_transactions(id),reconciliation_id TEXT NOT NULL,journal_entry_id INTEGER NOT NULL REFERENCES journal_entries(id),transaction_hash TEXT NOT NULL,journal_entry_hash TEXT NOT NULL,resolution_type TEXT NOT NULL,document_id INTEGER REFERENCES documents(id),document_sha256 TEXT,party_id TEXT,party_role TEXT,evidence_reference TEXT NOT NULL,rationale TEXT NOT NULL,next_action TEXT,plan_hash TEXT NOT NULL,actor TEXT NOT NULL,principal TEXT NOT NULL,created_at TEXT NOT NULL); INSERT INTO party_coverage_bank_resolution_events VALUES(1,1,'reconciliation-1',1,'transaction-1','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa','linked',NULL,NULL,'legacy-party','vendor','legacy evidence','legacy rationale',NULL,'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb','agent:legacy','service-account:legacy','2026-01-01T00:00:00.000Z');");
    const migration=JSON.parse(readFileSync(new URL("../../src/core/migrations/0056-bank-row-party-decisions.json",import.meta.url),"utf8")) as {sql:string};db.exec(migration.sql);
    expect(db.query("SELECT bank_transaction_id,party_id,provenance,decision_hash FROM current_party_coverage_bank_resolution_events").get()).toEqual({bank_transaction_id:1,party_id:"legacy-party",provenance:"legacy_party_coverage_v55",decision_hash:"0".repeat(63)+"1"});
    expect(()=>db.run("DELETE FROM party_coverage_bank_resolution_events")).toThrow("append-only");db.close();
  });

  test("CLI plans the same two hash-bound bank-row decisions from JSON",async()=>{
    const {root,db,registry,decisions}=setupSharedDocument();db.close();registry.close();const decisionsPath=join(root,"row-decisions.json");writeFileSync(decisionsPath,JSON.stringify(decisions));
    const process=Bun.spawn(["bun","run","src/cli.ts","documents","party-coverage-plan","--company",root,"--workspace",root,"--company-slug","alpha","--decisions",decisionsPath],{stdout:"pipe",stderr:"pipe"});const [stdout,stderr]=await Promise.all([new Response(process.stdout).text(),new Response(process.stderr).text()]);expect(await process.exited,stderr).toBe(0);const result=JSON.parse(stdout);expect(result.plan.operations).toEqual([expect.objectContaining({actionKey:"bank:1",partyId:"party-authority"}),expect.objectContaining({actionKey:"bank:2",partyId:"party-payee"})]);
  });

  test("persists two distinct parties for rows sharing one immutable document without economic mutation",()=>{
    const {root,db,registry,decisions}=setupSharedDocument();
    const before={documents:db.query("SELECT * FROM documents ORDER BY id").all(),banks:db.query("SELECT * FROM bank_transactions ORDER BY id").all(),journals:db.query("SELECT * FROM journal_entries ORDER BY id").all(),lines:db.query("SELECT * FROM journal_lines ORDER BY id").all(),trialBalance:buildTrialBalance(db,"2026-01-01","2026-01-31"),vat:buildVatReport(db,"2026-01-01","2026-01-31")};
    const planned=planPartyCoverage(db,registry,{companySlug:"alpha",decisions});
    expect(planned.plan.operations).toHaveLength(2);
    expect(planned.plan.operations).toEqual(expect.arrayContaining([
      expect.objectContaining({kind:"bank_decision",bankTransactionId:1,documentId:1,documentHash:"d".repeat(64),partyId:"party-authority",role:"authority",decisionHash:expect.stringMatching(/^[a-f0-9]{64}$/)}),
      expect.objectContaining({kind:"bank_decision",bankTransactionId:2,documentId:1,documentHash:"d".repeat(64),partyId:"party-payee",role:"employee",decisionHash:expect.stringMatching(/^[a-f0-9]{64}$/)}),
    ]));
    const input={companySlug:"alpha",decisions,planHash:planned.plan.planHash,idempotencyKey:"shared-source",confirm:true,actor:"agent:test",principal:"service-account:test"};
    expect(applyPartyCoverage(db,registry,root,input)).toMatchObject({ok:true,idempotent:false,applied:2});
    expect(applyPartyCoverage(db,registry,root,input)).toMatchObject({ok:true,idempotent:true,applied:2});
    const rows=projectPartyCoverage(db,registry,{companySlug:"alpha"}).rows;
    expect(rows.find(row=>row.bankTransactionId===1)).toMatchObject({status:"linked",documentId:1,candidate:{partyId:"party-authority",role:"authority",provenance:"reviewed_bank_statement_counterparty"}});
    expect(rows.find(row=>row.bankTransactionId===2)).toMatchObject({status:"linked",documentId:1,candidate:{partyId:"party-payee",role:"employee",provenance:"reviewed_bank_statement_counterparty"}});
    expect(db.query("SELECT actor,principal,transaction_hash,document_sha256,rationale,provenance FROM current_party_coverage_bank_resolution_events ORDER BY bank_transaction_id").all()).toEqual([
      {actor:"agent:test",principal:"service-account:test",transaction_hash:"row-a",document_sha256:"d".repeat(64),rationale:"row A names the authority",provenance:"reviewed_bank_statement_counterparty"},
      {actor:"agent:test",principal:"service-account:test",transaction_hash:"row-b",document_sha256:"d".repeat(64),rationale:"row B names the payee",provenance:"reviewed_bank_statement_counterparty"},
    ]);
    expect({documents:db.query("SELECT * FROM documents ORDER BY id").all(),banks:db.query("SELECT * FROM bank_transactions ORDER BY id").all(),journals:db.query("SELECT * FROM journal_entries ORDER BY id").all(),lines:db.query("SELECT * FROM journal_lines ORDER BY id").all(),trialBalance:buildTrialBalance(db,"2026-01-01","2026-01-31"),vat:buildVatReport(db,"2026-01-01","2026-01-31")}).toEqual(before);
    db.close();registry.close();
  });

  test("fails stale/conflicting decisions closed and corrects one exact row append-only",()=>{
    const {root,db,registry,documentHash,decisions}=setupSharedDocument();
    expect(()=>planPartyCoverage(db,registry,{companySlug:"alpha",decisions:[{...decisions[0]!,transactionHash:"stale"}]})).toThrow("exact transaction hash");
    expect(()=>planPartyCoverage(db,registry,{companySlug:"alpha",decisions:[{...decisions[0]!,documentHash:"0".repeat(64)}]})).toThrow("exact current document hash");
    const initial=planPartyCoverage(db,registry,{companySlug:"alpha",decisions});
    applyPartyCoverage(db,registry,root,{companySlug:"alpha",decisions,planHash:initial.plan.planHash,idempotencyKey:"initial",confirm:true,actor:"user:reviewer",principal:"service-account:reviewer"});
    const current=projectPartyCoverage(db,registry,{companySlug:"alpha"}).rows.find(row=>row.bankTransactionId===2)!;
    const replacement:PartyCoverageDecision={bankTransactionId:2,scope:"bank_transaction",transactionHash:"row-b",documentHash,partyId:"party-replacement",role:"employee",provenance:"reviewed_correction",evidenceReference:"corrected synthetic row B",rationale:"review found the exact payee",supersedesEventId:current.currentDecision!.id,supersedesDecisionHash:current.currentDecision!.decisionHash};
    expect(()=>planPartyCoverage(db,registry,{companySlug:"alpha",decisions:[{...replacement,supersedesEventId:undefined,supersedesDecisionHash:undefined}]})).toThrow("conflicting current");
    expect(()=>planPartyCoverage(db,registry,{companySlug:"alpha",decisions:[{...replacement,supersedesDecisionHash:"0".repeat(64)}]})).toThrow("exact current");
    const corrected=planPartyCoverage(db,registry,{companySlug:"alpha",decisions:[replacement]});
    applyPartyCoverage(db,registry,root,{companySlug:"alpha",decisions:[replacement],planHash:corrected.plan.planHash,idempotencyKey:"correction",confirm:true,actor:"user:reviewer",principal:"service-account:reviewer"});
    const after=projectPartyCoverage(db,registry,{companySlug:"alpha"}).rows;
    expect(after.find(row=>row.bankTransactionId===1)?.candidate).toMatchObject({partyId:"party-authority"});
    expect(after.find(row=>row.bankTransactionId===2)?.candidate).toMatchObject({partyId:"party-replacement",provenance:"reviewed_correction"});
    expect(db.query("SELECT count(*) AS count FROM party_coverage_bank_resolution_events").get()).toEqual({count:3});
    expect(()=>db.run("UPDATE party_coverage_bank_resolution_events SET rationale='changed'")).toThrow("append-only");
    expect(()=>applyPartyCoverage(db,registry,root,{companySlug:"alpha",decisions:[replacement],planHash:corrected.plan.planHash,idempotencyKey:"unauthorized",confirm:true,actor:"not-an-actor",principal:""})).toThrow("actor, authenticated principal");
    db.close();registry.close();
  });
});
