import { describe, expect, test } from "bun:test";
import { existsSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { createTar, readTar } from "../../src/core/tar";
import { companyPaths } from "../../src/core/paths";
import { createWorkspaceSnapshot, restoreWorkspaceSnapshot } from "../../src/core/workspace-snapshot";
import { openWorkspaceControlDb, workspaceControlPaths } from "../../src/core/workspace-control";
import { activateWorkspaceUser, grantCompanyMembership } from "../../src/core/workspace-access";
import { openDb } from "../../src/core/db";
import { verifyAuditChain } from "../../src/core/ledger";
import { companyRootForSlug } from "../../src/core/workspace";
import { makeWorkspace } from "./server-api/_shared";
import { proposeCompanyKnowledge, reviewCompanyKnowledge, queryCompanyKnowledge } from "../../src/core/company-knowledge";
import { applyOwnershipSnapshot, ownershipHistory, projectExactCompanyOwnership, proposeOwnershipSnapshot, queryOwnershipGraph, reviewOwnershipSnapshot } from "../../src/core/ownership-graph";
import { createParty, inspectParty, linkLegacyPartyReference, linkPartyRole } from "../../src/core/party-registry";
import { ingestCorporateRecord, inspectCorporateRecord, readCorporateRecordBytes } from "../../src/core/corporate-records";
import { ingestWorkspaceInboxSource } from "../../src/core/workspace-document-inbox";
import { postJournalEntry } from "../../src/core/ledger";
import { approveIntercompanyDisposition, inspectIntercompanyDisposition, linkIntercompanyDispositionJournal, proposeIntercompanyDisposition, settleIntercompanyDisposition } from "../../src/core/intercompany-dispositions";

function tempPath(label: string) { return join(mkdtempSync(join(tmpdir(), `${label}-`)), "artifact.tar"); }

function addOwner(workspace: string) {
  const db = openWorkspaceControlDb(workspace);
  const createdAt = "2026-08-23T10:00:00.000Z";
  db.query(`INSERT INTO "user"
    (id,name,email,emailVerified,createdAt,updatedAt,twoFactorEnabled)
    VALUES ('owner','Snapshot Owner','owner@example.test',1,?,?,1)`).run(createdAt, createdAt);
  db.query(`INSERT INTO "account"
    (id,accountId,providerId,issuer,userId,password,createdAt,updatedAt)
    VALUES ('credential','owner','credential','credential','owner','private-password-hash',?,?)`).run(createdAt, createdAt);
  db.query(`INSERT INTO "session"
    (id,expiresAt,token,createdAt,updatedAt,userId)
    VALUES ('private-session',?,'private-session-token',?,?,'owner')`).run(
    "2026-08-24T10:00:00.000Z", createdAt, createdAt,
  );
  activateWorkspaceUser(db, {
    userId: "owner", workspaceRole: "workspace_owner",
    createdBy: "agent:test", createdByProgram: "unit-test",
  });
  for (const companySlug of ["alpha-company", "beta-company"]) {
    grantCompanyMembership(db, workspace, {
      userId: "owner", companySlug, role: "owner",
      createdBy: "agent:test", createdByProgram: "unit-test",
    });
  }
  db.close();
}

describe("credential-free workspace snapshot and restore", () => {
  test("preserves linked and settled intercompany evidence lifecycle without credentials", () => {
    const workspace=makeWorkspace("workspace-disposition-snapshot",["Alpha Company","Beta Company"]),outPath=tempPath("workspace-disposition-out"),target=join(mkdtempSync(join(tmpdir(),"workspace-disposition-target-")),"restored");
    try { const control=openWorkspaceControlDb(workspace); let dispositionId="disposition-snapshot";
      try { const party=createParty(control,{partyId:"party-disposition",kind:"organization",name:"Synthetic counterparty",source:"synthetic",observedAt:"2026-01-01T00:00:00Z",reviewAssertion:"synthetic",actor:"user:maker"});const record=ingestCorporateRecord(control,{recordId:"record-disposition",type:"intercompany_agreement",bytes:new TextEncoder().encode("synthetic agreement"),filename:"agreement.txt",source:"synthetic",receivedAt:"2026-01-01T00:00:00Z",uploader:"user:maker",actor:"user:maker"});const input={dispositionId,type:"loan" as const,economicDate:"2026-02-01",amount:125.5,currency:"DKK",partyIds:[party.partyId],evidenceRecordIds:[record.recordId],left:{companySlug:"alpha-company",role:"lender",expectedSide:"receivable" as const},right:{companySlug:"beta-company",role:"borrower",expectedSide:"payable" as const}};const proposal=proposeIntercompanyDisposition(control,input,{actor:"user:maker",principal:{kind:"user",id:"maker"}});const approved=approveIntercompanyDisposition(control,dispositionId,proposal.payloadHash,{actor:"user:review",principal:{kind:"user",id:"review"}});
        const post=(slug:string,lines:any[])=>{const db=openDb(companyPaths(companyRootForSlug(workspace,slug)).db);try{const result=postJournalEntry(db,{transactionDate:"2026-02-01",text:"Synthetic intercompany",createdBy:"user:maker",createdByProgram:"test",lines});expect(result.ok).toBe(true);return {id:result.entryId!,head:(db.query("SELECT entry_hash FROM journal_entries ORDER BY id DESC LIMIT 1").get() as any).entry_hash};}finally{db.close();}};
        const left=post("alpha-company",[{accountNo:"1100",debitAmount:125.5},{accountNo:"5000",creditAmount:125.5}]);const right=post("beta-company",[{accountNo:"5000",debitAmount:125.5},{accountNo:"7000",creditAmount:125.5}]);linkIntercompanyDispositionJournal(control,workspace,{dispositionId,payloadHash:approved.payloadHash,side:"left",journalEntryId:left.id,expectedLedgerHeadHash:left.head,actor:"user:link",principal:{kind:"user",id:"linker"}});linkIntercompanyDispositionJournal(control,workspace,{dispositionId,payloadHash:approved.payloadHash,side:"right",journalEntryId:right.id,expectedLedgerHeadHash:right.head,actor:"user:link",principal:{kind:"user",id:"linker"}});settleIntercompanyDisposition(control,workspace,{dispositionId,payloadHash:approved.payloadHash,settlementEvidenceRecordIds:[record.recordId],actor:"user:settle",principal:{kind:"user",id:"settler"}});
      } finally { control.close(); }
      const beforeSnapshot=openWorkspaceControlDb(workspace);try{expect(beforeSnapshot.query("SELECT count(*) AS n FROM rm_intercompany_disposition_lifecycle_events WHERE disposition_id=?").get(dispositionId)).toEqual({n:4});}finally{beforeSnapshot.close();}expect(createWorkspaceSnapshot(workspace,{outPath,createdAt:"2026-08-30T10:00:00.000Z"}).ok).toBeTrue();const entries=readTar(readFileSync(outPath));expect(entries.some(entry=>entry.path==="intercompany-dispositions.json")).toBeTrue();const archive=readFileSync(outPath);expect(archive.includes(Buffer.from("apikey"))).toBe(false);expect(restoreWorkspaceSnapshot({snapshotPath:outPath,targetWorkspaceRoot:target}).ok).toBeTrue();const restored=openWorkspaceControlDb(target);try{expect(inspectIntercompanyDisposition(restored,dispositionId)).toMatchObject({status:"settled",links:expect.arrayContaining([expect.objectContaining({side:"left"}),expect.objectContaining({side:"right"})])});expect(restored.query("SELECT count(*) AS n FROM rm_intercompany_disposition_lifecycle_events WHERE disposition_id=?").get(dispositionId)).toEqual({n:4});}finally{restored.close();}
    } finally { rmSync(workspace,{recursive:true,force:true});rmSync(dirname(outPath),{recursive:true,force:true});rmSync(dirname(target),{recursive:true,force:true}); }
  });
  test("round-trips immutable inbox bytes, events, assignment and durable claim without credentials",()=>{const workspace=makeWorkspace("workspace-inbox-snapshot",["Alpha Company"]),outPath=tempPath("workspace-inbox-out"),target=join(mkdtempSync(join(tmpdir(),"workspace-inbox-target-")),"restored");try{const db=openWorkspaceControlDb(workspace);try{const item=ingestWorkspaceInboxSource(db,{visibilityAnchorSlug:"alpha-company",idempotencyKey:"snapshot-inbox",bytes:new TextEncoder().encode("synthetic inbox bytes"),filename:"synthetic.txt",mimeType:"text/plain",transport:"upload",receivedAt:"2026-08-30T10:00:00.000Z",metadata:{},candidates:[],visibleCompanySlugs:new Set(["alpha-company"]),actor:"agent:test"});db.query("INSERT INTO rm_workspace_inbox_handoff_claims(source_id,company_slug,source_hash,state,claim_id,lease_expires_at,created_at,updated_at) VALUES(?,?,?,'claimed','claim-synthetic','2026-08-30T11:00:00.000Z','2026-08-30T10:00:00.000Z','2026-08-30T10:00:00.000Z')").run(item.sourceId,"alpha-company",item.sha256);}finally{db.close();}expect(createWorkspaceSnapshot(workspace,{outPath,createdAt:"2026-08-30T10:00:00.000Z"}).ok).toBeTrue();const text=new TextDecoder().decode(readFileSync(outPath));expect(text).not.toContain("apikey");expect(restoreWorkspaceSnapshot({snapshotPath:outPath,targetWorkspaceRoot:target}).ok).toBeTrue();const restored=openWorkspaceControlDb(target);try{expect(restored.query("SELECT sha256,original_bytes FROM rm_workspace_inbox_sources").get()).toMatchObject({sha256:expect.any(String),original_bytes:expect.any(Uint8Array)});expect(restored.query("SELECT count(*) AS n FROM rm_workspace_inbox_events").get()).toEqual({n:3});expect(restored.query("SELECT count(*) AS n FROM rm_workspace_inbox_handoff_claims").get()).toEqual({n:1});}finally{restored.close();}}finally{rmSync(workspace,{recursive:true,force:true});rmSync(dirname(outPath),{recursive:true,force:true});rmSync(dirname(target),{recursive:true,force:true});}});
  test("preserves reviewed ownership snapshots, facts and v1-safe projection without credentials", () => {
    const workspace=makeWorkspace("workspace-ownership-snapshot",["Alpha Company","Beta Company"]);const outPath=tempPath("workspace-ownership-out");const target=join(mkdtempSync(join(tmpdir(),"workspace-ownership-target-")),"restored");
    try { addOwner(workspace);const db=openWorkspaceControlDb(workspace);try { const principal={kind:"local_operator" as const,id:"snapshot-test"};const snapshot=proposeOwnershipSnapshot(db,{snapshotId:"ownership-snapshot",source:"synthetic-registry",observedAt:"2026-01-01T00:00:00Z",facts:[{owner:{kind:"company",companySlug:"alpha-company"},ownedCompanySlug:"beta-company",validFrom:"2026-01-01",economicBasisPoints:10000,controlType:"equity",jurisdiction:"DK",evidenceRefs:["synthetic-evidence"]}],actor:"user:test",principal});reviewOwnershipSnapshot(db,{snapshotId:snapshot.snapshotId,decision:"approved",actor:"user:review",principal:{kind:"local_operator",id:"snapshot-review"}});applyOwnershipSnapshot(db,{snapshotId:snapshot.snapshotId,snapshotHash:snapshot.snapshotHash,diffHash:snapshot.diffHash,actor:"user:review",principal,authorized:true}); } finally {db.close();}
      expect(createWorkspaceSnapshot(workspace,{outPath,createdAt:"2026-08-23T11:00:00.000Z"}).ok).toBeTrue();const entries=readTar(readFileSync(outPath));const ownershipEntry=entries.find(entry=>entry.path==="ownership-graph.json");expect(ownershipEntry).toBeDefined();const archiveText=entries.map(entry=>new TextDecoder().decode(entry.content)).join("\n");expect(archiveText).not.toContain("private-session-token");expect(archiveText).not.toContain("private-password-hash");const restored=restoreWorkspaceSnapshot({snapshotPath:outPath,targetWorkspaceRoot:target});expect(restored.ok).toBeTrue();const read=openWorkspaceControlDb(target);try{expect(ownershipHistory(read,"ownership-snapshot")[0]).toMatchObject({state:"applied"});expect(queryOwnershipGraph(read,{asOf:"2026-02-01"}).facts).toHaveLength(1);expect(projectExactCompanyOwnership(read,"2026-02-01")).toMatchObject({eligible:true,edges:[{parentCompanySlug:"alpha-company",childCompanySlug:"beta-company",basisPoints:10000}]});expect(read.query("SELECT count(*) AS n FROM rm_ownership_snapshot_events").get()).toEqual({n:3});}finally{read.close();}
    } finally {rmSync(workspace,{recursive:true,force:true});rmSync(dirname(outPath),{recursive:true,force:true});rmSync(dirname(target),{recursive:true,force:true});}
  });
  test("includes source-backed company knowledge without exporting credentials", () => {
    const workspace=makeWorkspace("workspace-knowledge-snapshot",["Alpha Company"]);const outPath=tempPath("workspace-knowledge-out");const target=join(mkdtempSync(join(tmpdir(),"workspace-knowledge-target-")),"restored");
    try { const db=openWorkspaceControlDb(workspace);try { const principal={kind:"local_operator" as const,id:"snapshot-test"};const assertion=proposeCompanyKnowledge(db,{companySlug:"alpha-company",predicate:"markets",value:["DK"],source:{kind:"external_snapshot",ref:"synthetic-source"},validFrom:"2026-01-01",actor:"user:test",principal});reviewCompanyKnowledge(db,{assertionId:assertion.assertionId,decision:"approved",actor:"user:review",principal}); } finally {db.close();}
      expect(createWorkspaceSnapshot(workspace,{outPath,createdAt:"2026-08-23T11:00:00.000Z"}).ok).toBeTrue();const restored=restoreWorkspaceSnapshot({snapshotPath:outPath,targetWorkspaceRoot:target});expect(restored.ok).toBeTrue();const read=openWorkspaceControlDb(target);try{expect(queryCompanyKnowledge(read,{companySlug:"alpha-company",asOf:"2026-02-01"}).assertions).toMatchObject([{predicate:"markets",source:{kind:"external_snapshot",ref:"synthetic-source"}}]);}finally{read.close();}
    } finally {rmSync(workspace,{recursive:true,force:true});rmSync(dirname(outPath),{recursive:true,force:true});rmSync(dirname(target),{recursive:true,force:true});}
  });
  test("preserves party provenance and immutable corporate bytes without credentials", () => {
    const workspace=makeWorkspace("workspace-registry-snapshot",["Alpha Company"]);const outPath=tempPath("workspace-registry-out");const target=join(mkdtempSync(join(tmpdir(),"workspace-registry-target-")),"restored");
    try { const db=openWorkspaceControlDb(workspace);try { const party=createParty(db,{partyId:"party-snapshot",kind:"organization",name:"Synthetic Party",aliases:["Synthetic Alias"],source:"synthetic",observedAt:"2026-01-01T00:00:00Z",reviewAssertion:"synthetic evidence",actor:"user:test"});linkPartyRole(db,{partyId:party.partyId,companySlug:"alpha-company",role:"vendor",actor:"user:test"});linkLegacyPartyReference(db,{partyId:party.partyId,companySlug:"alpha-company",legacyKind:"vendor",legacyId:"legacy-synthetic",actor:"user:test"});db.query(`INSERT INTO rm_legacy_party_mapping_events(company_slug,legacy_kind,legacy_id,party_id,party_role,event_type,version,prior_event_hash,event_hash,contact_snapshot,contact_fingerprint,evidence_json,plan_hash,idempotency_key_hash,idempotency_payload_hash,reason,actor,principal,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)`).run("alpha-company","vendor","42",party.partyId,"vendor","mapped",1,null,"1".repeat(64),JSON.stringify({id:42,kind:"vendor",notes:"preserved note"}),"2".repeat(64),JSON.stringify({kind:"reviewed_source_document",documentId:7,documentSha256:"3".repeat(64),reviewedLegacyReference:"synthetic review"}),"4".repeat(64),"5".repeat(64),"6".repeat(64),null,"agent:test","service-snapshot","2026-01-01T00:00:00.000Z");ingestCorporateRecord(db,{recordId:"record-snapshot",type:"articles",bytes:new TextEncoder().encode("immutable synthetic bytes"),filename:"synthetic.pdf",source:"synthetic",receivedAt:"2026-01-01T00:00:00Z",uploader:"user:test",actor:"user:test",links:[{type:"company",id:"alpha-company"},{type:"party",id:party.partyId}]});}finally{db.close();}
      expect(createWorkspaceSnapshot(workspace,{outPath,createdAt:"2026-08-23T11:00:00.000Z"}).ok).toBeTrue();const restored=restoreWorkspaceSnapshot({snapshotPath:outPath,targetWorkspaceRoot:target});expect(restored.ok).toBeTrue();const read=openWorkspaceControlDb(target);try{expect(inspectParty(read,"party-snapshot")).toMatchObject({aliases:expect.arrayContaining([expect.objectContaining({alias:"synthetic alias"})]),roles:[expect.objectContaining({companySlug:"alpha-company"})]});expect(read.query("SELECT count(*) AS n FROM rm_party_legacy_links").get()).toEqual({n:1});expect(read.query("SELECT event_hash,contact_snapshot FROM rm_legacy_party_mapping_events").get()).toEqual({event_hash:"1".repeat(64),contact_snapshot:JSON.stringify({id:42,kind:"vendor",notes:"preserved note"})});expect(new TextDecoder().decode(readCorporateRecordBytes(read,"record-snapshot"))).toBe("immutable synthetic bytes");expect(inspectCorporateRecord(read,"record-snapshot")).toMatchObject({links:expect.arrayContaining([{type:"company",id:"alpha-company"},{type:"party",id:"party-snapshot"}])});}finally{read.close();}
    } finally {rmSync(workspace,{recursive:true,force:true});rmSync(dirname(outPath),{recursive:true,force:true});rmSync(dirname(target),{recursive:true,force:true});}
  });
  test("restores every ledger and a safe access plan without credentials", () => {
    const workspace = makeWorkspace("workspace-snapshot", ["Alpha Company", "Beta Company"]);
    const outPath = tempPath("workspace-snapshot-out");
    const target = join(mkdtempSync(join(tmpdir(), "workspace-snapshot-target-parent-")), "restored");
    try {
      addOwner(workspace);
      for (const slug of ["alpha-company", "beta-company"]) {
        const config = companyPaths(companyRootForSlug(workspace, slug)).config;
        writeFileSync(join(config, "digisense.json"), '{"apiLicenseKey":"private-digisense-key"}', { mode: 0o600 });
        writeFileSync(join(config, "imap.json"), '{"password":"private-imap-password"}', { mode: 0o600 });
        writeFileSync(join(config, "smtp.json"), '{"password":"private-smtp-password"}', { mode: 0o600 });
      }
      const created = createWorkspaceSnapshot(workspace, {
        outPath,
        createdAt: "2026-08-23T11:00:00.000Z",
        createdBy: "agent:test",
        createdByProgram: "unit-test",
      });
      expect(created).toMatchObject({ ok: true, companyCount: 2, accessIdentityCount: 1 });
      const archive = readFileSync(outPath);
      for (const credential of [
        "private-password-hash", "private-session-token", "private-digisense-key",
        "private-imap-password", "private-smtp-password",
      ]) expect(archive.includes(Buffer.from(credential))).toBe(false);

      const restored = restoreWorkspaceSnapshot({ snapshotPath: outPath, targetWorkspaceRoot: target });
      expect(restored).toMatchObject({
        ok: true, companyCount: 2, nextStep: "bootstrap-owner-then-reinvite",
      });
      expect(existsSync(workspaceControlPaths(target).db)).toBe(false);
      const planPath = join(target, ".rentemester", "restored-access-plan.json");
      expect(statSync(planPath).mode & 0o777).toBe(0o600);
      expect(JSON.parse(readFileSync(planPath, "utf8"))).toMatchObject({
        version: 1,
        recovery: "bootstrap-owner-then-reinvite",
        users: [{
          name: "Snapshot Owner", email: "owner@example.test", workspaceRole: "workspace_owner",
          memberships: [
            { companySlug: "alpha-company", role: "owner" },
            { companySlug: "beta-company", role: "owner" },
          ],
        }],
      });
      for (const slug of ["alpha-company", "beta-company"]) {
        const root = companyRootForSlug(target, slug);
        for (const secretFile of ["digisense.json", "imap.json", "smtp.json"]) {
          expect(existsSync(join(companyPaths(root).config, secretFile))).toBe(false);
        }
        const db = openDb(companyPaths(root).db);
        expect(verifyAuditChain(db).ok).toBe(true);
        db.close();
      }
    } finally {
      rmSync(workspace, { recursive: true, force: true });
      rmSync(dirname(outPath), { recursive: true, force: true });
      rmSync(dirname(target), { recursive: true, force: true });
    }
  });

  test("rejects tampering before publishing a target workspace", () => {
    const workspace = makeWorkspace("workspace-snapshot-tamper", ["Alpha Company"]);
    const outPath = tempPath("workspace-snapshot-tamper-out");
    const tamperedPath = tempPath("workspace-snapshot-tampered");
    const target = join(mkdtempSync(join(tmpdir(), "workspace-snapshot-tamper-target-")), "restored");
    try {
      const created = createWorkspaceSnapshot(workspace, {
        outPath, createdAt: "2026-08-23T12:00:00.000Z",
        createdBy: "agent:test", createdByProgram: "unit-test",
      });
      expect(created.ok).toBe(true);
      const entries = readTar(readFileSync(outPath)).map((entry) => entry.path === "access-plan.json"
        ? { ...entry, content: Buffer.from('{"tampered":true}\n') }
        : entry);
      writeFileSync(tamperedPath, createTar(entries));
      const restored = restoreWorkspaceSnapshot({ snapshotPath: tamperedPath, targetWorkspaceRoot: target });
      expect(restored.ok).toBe(false);
      expect(restored.errors.join(" ")).toContain("checksum mismatch");
      expect(existsSync(target)).toBe(false);
    } finally {
      rmSync(workspace, { recursive: true, force: true });
      rmSync(dirname(outPath), { recursive: true, force: true });
      rmSync(dirname(tamperedPath), { recursive: true, force: true });
      rmSync(dirname(target), { recursive: true, force: true });
    }
  });
});
