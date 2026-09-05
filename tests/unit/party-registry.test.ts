import { afterEach, describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Database } from "bun:sqlite";
import { initWorkspace } from "../../src/core/workspace";
import { openWorkspaceControlDb } from "../../src/core/workspace-control";
import { addPartyAlias, approvePartyMerge, assertPartyField, createParty, linkLegacyPartyReference, linkPartyRole, proposePartyMerge, searchParties } from "../../src/core/party-registry";
import { planLegacyPartyMapping } from "../../src/core/legacy-party-mapping";

const roots: string[] = [];
function db() { const root = mkdtempSync(join(tmpdir(), "rm-party-")); roots.push(root); initWorkspace(root); return openWorkspaceControlDb(root); }
afterEach(() => { for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true }); });

describe("#573 workspace party registry", () => {
  test("#638 fails closed when legacy contact evidence or company-scoped role is absent", () => {
    const control = db();
    const ledger = new Database(":memory:");
    ledger.exec("CREATE TABLE vendors (id INTEGER PRIMARY KEY,name TEXT NOT NULL,address TEXT,vat_or_cvr TEXT,country_code TEXT,identifier_kind TEXT,identity_status TEXT,email TEXT,phone TEXT,website TEXT,default_expense_account TEXT,default_vat_treatment TEXT,notes TEXT,archived INTEGER,created_at TEXT)");
    ledger.exec("CREATE TABLE documents (id INTEGER PRIMARY KEY,sha256_hash TEXT,payload_json TEXT,document_type TEXT,sender_name TEXT,sender_address TEXT,sender_vat_cvr TEXT,supplier_country_code TEXT,supplier_identifier_kind TEXT,supplier_identity_status TEXT,recipient_name TEXT,recipient_address TEXT,recipient_vat_cvr TEXT)");
    ledger.run("INSERT INTO vendors(id,name,notes) VALUES(7,'Synthetic vendor','keep me')");
    const party=createParty(control,{kind:"organization",name:"Synthetic party",source:"test",observedAt:"2026-01-01T00:00:00Z",reviewAssertion:"reviewed",actor:"user:test"});
    linkPartyRole(control,{partyId:party.partyId,companySlug:"alpha",role:"vendor",actor:"user:test"});
    expect(planLegacyPartyMapping(ledger,control,{companySlug:"alpha",legacyKind:"vendor",legacyId:"7",partyId:party.partyId,role:"vendor",documentId:999,reviewedLegacyReference:"reviewed invoice"})).toEqual({ok:false,errors:["EVIDENCE_NOT_FOUND"]});
    ledger.close(); control.close();
  });
  test("supports a single party in customer/vendor roles without leaking defaults", () => {
    const control = db();
    const party = createParty(control, { kind:"organization", name:"Synthetic Shared ApS", identifiers:[{country:"DK",identifier:"12345678",identifierKind:"dk_cvr"}], source:"synthetic",observedAt:"2026-01-01T00:00:00.000Z",reviewAssertion:"checked",actor:"user:maker" });
    linkPartyRole(control,{partyId:party.partyId,companySlug:"alpha",role:"customer",defaults:{currency:"DKK",paymentTermsDays:14},actor:"user:maker"});
    linkPartyRole(control,{partyId:party.partyId,companySlug:"beta",role:"vendor",defaults:{account:"4010",vat:"purchase"},actor:"user:maker"});
    expect(searchParties(control,{companySlugs:new Set(["alpha"])}).rows[0]!.roles).toEqual([{companySlug:"alpha",role:"customer",defaults:{currency:"DKK",paymentTermsDays:14}}]);
    expect(() => linkPartyRole(control,{partyId:party.partyId,companySlug:"alpha",role:"customer",defaults:{currency:"EUR"},actor:"user:maker"})).toThrow("conflicting");
    control.close();
  });
  test("rejects identifier conflicts and requires reviewed append-only merge", () => {
    const control = db();
    const one = createParty(control,{kind:"organization",name:"One",identifiers:[{country:"DK",identifier:"87654321",identifierKind:"dk_cvr"}],source:"synthetic",observedAt:"2026-01-01T00:00:00.000Z",reviewAssertion:"checked",actor:"user:maker"});
    expect(() => createParty(control,{kind:"organization",name:"Duplicate",identifiers:[{country:"DK",identifier:"87654321",identifierKind:"dk_cvr"}],source:"synthetic",observedAt:"2026-01-01T00:00:00.000Z",reviewAssertion:"checked",actor:"user:maker"})).toThrow("conflicts");
    const two = createParty(control,{kind:"person",name:"Two",source:"synthetic",observedAt:"2026-01-01T00:00:00.000Z",reviewAssertion:"checked",actor:"user:maker"});
    const proposal = proposePartyMerge(control,{fromPartyId:two.partyId,intoPartyId:one.partyId,reviewAssertion:"human reviewed",actor:"user:reviewer"});
    expect(approvePartyMerge(control,{fromPartyId:two.partyId,proposalHash:proposal,actor:"user:approver"}).history.map((e:any)=>e.event_type)).toEqual(["created","proposed_merge","approved_merge","superseded"]);
    control.close();
  });
  test("keeps source-backed aliases/assertions and legacy ids append-only and idempotent", () => {
    const control = db();
    const party=createParty(control,{partyId:"party-synthetic",kind:"organization",name:"Synthetic",source:"import",observedAt:"2026-01-01T00:00:00.000Z",reviewAssertion:"checked",actor:"user:maker"});
    addPartyAlias(control,{partyId:party.partyId,alias:"Synthetic Trading",source:"document",observedAt:"2026-01-02T00:00:00.000Z",reviewState:"proposed",actor:"user:maker"});
    assertPartyField(control,{partyId:party.partyId,field:"name",value:"Synthetic Holdings",source:"registry",observedAt:"2026-01-03T00:00:00.000Z",reviewState:"approved",actor:"user:reviewer"});
    linkLegacyPartyReference(control,{partyId:party.partyId,companySlug:"alpha",legacyKind:"vendor",legacyId:"legacy-7",actor:"user:maker"});
    linkLegacyPartyReference(control,{partyId:party.partyId,companySlug:"alpha",legacyKind:"vendor",legacyId:"legacy-7",actor:"user:maker"});
    expect(searchParties(control,{query:"trading",companySlugs:new Set(["alpha"])}).rows).toEqual([]);
    expect(control.query("SELECT count(*) AS n FROM rm_party_legacy_links").get()).toEqual({n:1});
    expect(()=>control.run("UPDATE rm_party_field_assertions SET value='changed'")).toThrow("append-only");
    control.close();
  });
});
