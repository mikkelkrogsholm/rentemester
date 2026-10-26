import { afterEach, describe, expect, test } from "bun:test";
import { Database } from "bun:sqlite";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { initWorkspace } from "../../src/core/workspace";
import { openWorkspaceControlDb } from "../../src/core/workspace-control";
import { addPartyAlias, createParty, linkPartyRole } from "../../src/core/party-registry";
import { partyHub, partyProfile } from "../../src/core/party-hub";

const roots:string[]=[];
function setup(){
  const root=mkdtempSync(join(tmpdir(),"rm-party-hub-")); roots.push(root); initWorkspace(root);
  const control=openWorkspaceControlDb(root), ledger=new Database(":memory:");
  ledger.exec(`CREATE TABLE documents(id INTEGER PRIMARY KEY,document_no TEXT,invoice_date TEXT,amount_inc_vat REAL,currency TEXT,sender_name TEXT);
    CREATE TABLE current_document_party_links(id INTEGER PRIMARY KEY,document_id INTEGER,party_id TEXT,party_role TEXT,evidence_kind TEXT,created_at TEXT);`);
  return {control,ledger};
}
afterEach(()=>{ for(const root of roots.splice(0)) rmSync(root,{recursive:true,force:true}); });

describe("#653 party hub projection",()=>{
  test("never infers an accounting link from a matching display name and respects the requested period",()=>{
    const {control,ledger}=setup();
    const party=createParty(control,{partyId:"party-explicit",kind:"organization",name:"Same Display Name",source:"test",observedAt:"2026-01-01T00:00:00Z",reviewAssertion:"reviewed",actor:"user:test"});
    linkPartyRole(control,{partyId:party.partyId,companySlug:"alpha",role:"vendor",actor:"user:test"});
    ledger.run("INSERT INTO documents VALUES(1,'UNLINKED','2026-01-10',99,'DKK','Same Display Name')");
    expect(partyProfile(ledger,control,{companySlug:"alpha",partyId:party.partyId,visibleCompanies:new Set(["alpha"]),from:"2026-01-01",asOf:"2026-01-31"})?.computed).toMatchObject({purchase:{amount:0},sourceCoverage:{documents:0}});
    ledger.run("INSERT INTO current_document_party_links VALUES(1,1,'party-explicit','vendor','exact_identifier','2026-01-10T00:00:00Z')");
    expect(partyProfile(ledger,control,{companySlug:"alpha",partyId:party.partyId,visibleCompanies:new Set(["alpha"]),from:"2026-02-01",asOf:"2026-02-28"})?.computed).toMatchObject({period:{from:"2026-02-01",to:"2026-02-28"},purchase:{amount:0},sourceCoverage:{documents:0}});
    expect(partyProfile(ledger,control,{companySlug:"alpha",partyId:party.partyId,visibleCompanies:new Set(["alpha"]),from:"2026-01-01",asOf:"2026-01-31"})?.computed).toMatchObject({purchase:{amount:99},sourceCoverage:{documents:1}});
    ledger.close(); control.close();
  });

  test("sorts tied computed spend deterministically by canonical display name",()=>{
    const {control,ledger}=setup();
    for(const [id,name] of [["party-b","Beta"],["party-a","Alpha"]] as const){createParty(control,{partyId:id,kind:"organization",name,source:"test",observedAt:"2026-01-01T00:00:00Z",reviewAssertion:"reviewed",actor:"user:test"});linkPartyRole(control,{partyId:id,companySlug:"alpha",role:"vendor",actor:"user:test"});}
    expect(partyHub(ledger,control,{companySlug:"alpha",visibleCompanies:new Set(["alpha"]),sort:"spend",from:"2026-01-01",asOf:"2026-01-31"}).rows.map(row=>row.partyId)).toEqual(["party-a","party-b"]);
    ledger.close();control.close();
  });

  test("keeps unmapped ledger references out of coverage without inferring a display-name link",()=>{
    const {control,ledger}=setup();
    const party=createParty(control,{partyId:"party-research",kind:"organization",name:"Known name",source:"test",observedAt:"2026-01-01T00:00:00Z",reviewAssertion:"reviewed",actor:"user:test"});
    linkPartyRole(control,{partyId:party.partyId,companySlug:"alpha",role:"vendor",actor:"user:test"});
    addPartyAlias(control,{partyId:party.partyId,alias:"Proposed alias",source:"import",observedAt:"2026-01-03T00:00:00Z",reviewState:"proposed",actor:"user:test"});
    ledger.run("INSERT INTO documents VALUES(7,'UNMAPPED','2026-01-10',500,'DKK','Known name')");
    const result=partyProfile(ledger,control,{companySlug:"alpha",partyId:party.partyId,visibleCompanies:new Set(["alpha"]),from:"2026-01-01",asOf:"2026-01-31"})!;
    expect(result.computed).toMatchObject({purchase:{amount:0},sourceCoverage:{documents:0,unmappedLedgerReferences:0}});
    expect(result.research.warnings).toEqual(["Foreslået kilde kræver review: import"]);
    ledger.close();control.close();
  });
});
