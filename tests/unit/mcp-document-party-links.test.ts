import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { appendFileSync, mkdtempSync, rmSync, statSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { createCompany } from "../../src/core/company";
import { companyPaths } from "../../src/core/paths";
import { createParty, linkPartyRole } from "../../src/core/party-registry";
import { companyRootForSlug, initWorkspace } from "../../src/core/workspace";
import { openWorkspaceControlDb } from "../../src/core/workspace-control";
import { migrate, openDb } from "../../src/core/db";
import { createVendor } from "../../src/core/master-data";
import { addBankAccount } from "../../src/core/bank";
import { postJournalEntry, seedAccounts } from "../../src/core/ledger";
import { ingestDocument } from "../../src/core/documents";
import { openWorkspaceBetterAuth } from "../../src/server/better-auth";
import { createWorkspaceServicePrincipal } from "../../src/core/workspace-service-principals";
import { activateWorkspaceUser, grantCompanyMembership } from "../../src/core/workspace-access";

const SERVER_PATH = new URL("../../src/mcp/server.ts", import.meta.url).pathname;
type Rpc = { id?: number; error?: unknown; result?: any };

class Client {
  private proc: ReturnType<typeof Bun.spawn>;
  private reader: ReadableStreamDefaultReader<Uint8Array>;
  private decoder = new TextDecoder();
  private buffer = "";
  private id = 0;
  constructor(workspace: string, token: string) {
    this.proc = Bun.spawn(["bun", SERVER_PATH], {
      stdin: "pipe",
      stdout: "pipe",
      stderr: "pipe",
      env: { ...process.env, RENTEMESTER_WORKSPACE: workspace, RENTEMESTER_SERVICE_PRINCIPAL_TOKEN: token, RENTEMESTER_MCP_PROFILE: "full" },
    });
    this.reader = this.proc.stdout.getReader();
  }
  async send(method: string, params: Record<string, unknown> = {}): Promise<Rpc> {
    const id = ++this.id;
    await this.proc.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", id, method, params })}\n`);
    await (this.proc.stdin as any).flush?.();
    for (;;) {
      const lineEnd = this.buffer.indexOf("\n");
      if (lineEnd < 0) {
        const next = await this.reader.read();
        if (next.done) throw new Error("MCP server closed before responding");
        this.buffer += this.decoder.decode(next.value, { stream: true });
        continue;
      }
      const line = this.buffer.slice(0, lineEnd).trim();
      this.buffer = this.buffer.slice(lineEnd + 1);
      if (!line) continue;
      const response = JSON.parse(line) as Rpc;
      if (response.id === id) return response;
    }
  }
  async notify(method: string): Promise<void> {
    await this.proc.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", method })}\n`);
    await (this.proc.stdin as any).flush?.();
  }
  async close() {
    this.proc.stdin.end();
    this.reader.releaseLock();
    this.proc.kill();
    await this.proc.exited;
  }
}

let workspace = "";
let companyRoot = "";
let client: Client;
let serviceAccountId: string;
let serviceToken: string;
let reviewedSourceDocumentId: number;

function eventCount() {
  const db = openDb(companyPaths(companyRoot).db);
  try { return (db.query("SELECT count(*) AS n FROM document_party_link_events").get() as { n: number }).n; }
  finally { db.close(); }
}

beforeAll(async () => {
  workspace = mkdtempSync(join(tmpdir(), "mcp-document-party-"));
  initWorkspace(workspace);
  const company = createCompany(workspace, { name: "Acme ApS" });
  companyRoot = companyRootForSlug(workspace, company.slug);
  appendFileSync(join(companyRoot, "config", "policy.yaml"), "  agents:\n    - agent:document-party-black-box/1.0.0\n    - agent:second-party-reviewer/1.0.0\n");
  const ledger = openDb(companyPaths(companyRoot).db);
  migrate(ledger);
  seedAccounts(ledger);
  ledger.query(`INSERT INTO documents
    (document_no,sha256_hash,payload_json,upload_datetime,source,status,
     supplier_country_code,supplier_identifier_kind,sender_vat_cvr,sender_name,retain_until)
    VALUES(?,?,?,?,?,?,?,?,?,?,?)`).run("DOC-MCP", "b".repeat(64), "{}", "2026-08-30T00:00:00.000Z", "synthetic", "inbox", "DK", "dk_cvr", "DK12345678", "Evidence name", "2032-01-01");
  const vendor=createVendor(ledger,{name:"No-ID Supplier Inc.",address:"1 Source Road",countryCode:"US",identifierKind:"non_eu",notes:"MCP preserved note"});
  if(!vendor.ok)throw new Error(vendor.errors.join("; "));
  ledger.query(`INSERT INTO documents(document_no,sha256_hash,payload_json,upload_datetime,source,status,document_type,sender_name,sender_address,sender_vat_cvr,supplier_country_code,supplier_identifier_kind,supplier_identity_status,retain_until) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)`).run("DOC-MCP-LEGACY","c".repeat(64),"{}","2026-08-30T00:00:00.000Z","synthetic","inbox","purchase_sale","No-ID Supplier Inc.","1 Source Road",null,"US","non_eu","resolved","2032-01-01");
  const imported=createVendor(ledger,{name:"Imported MCP ApS",address:"2 Evidence Road",vatOrCvr:"DK11223344",notes:"MCP enrichment note"});
  if(!imported.ok)throw new Error(imported.errors.join("; "));
  const source=join(workspace,"mcp-vendor-invoice.txt");writeFileSync(source,"immutable MCP vendor invoice");
  const importedDocument=ingestDocument(ledger,companyRoot,source,{source:"synthetic",documentType:"purchase_sale",issueDate:"2026-09-02",invoiceNo:"MCP-ENRICH",deliveryDescription:"Synthetic service",amountIncVat:125,vatAmount:25,currency:"DKK",sender:{name:"Imported MCP ApS",address:"2 Evidence Road",vatOrCvr:"DK11223344",countryCode:"DK",identifierKind:"dk_cvr"},recipient:{name:"Acme ApS",address:"Buyer Road",vatOrCvr:"DK87654321"}});
  if(!importedDocument.ok)throw new Error(importedDocument.errors.join("; "));
  const reviewedSource=join(workspace,"mcp-legacy-evidence.txt");writeFileSync(reviewedSource,"immutable synthetic source naming Foreign Merchant");
  const reviewedDocument=ingestDocument(ledger,companyRoot,reviewedSource,{source:"synthetic",documentType:"purchase_sale",issueDate:"2026-09-03",deliveryDescription:"Synthetic service",amountIncVat:10,vatAmount:0,currency:"DKK",sender:{name:"Foreign Merchant",address:"Source Road",countryCode:"US",identifierKind:"non_eu"},recipient:{name:"Acme ApS",address:"Buyer Road"}});
  if(!reviewedDocument.ok)throw new Error(reviewedDocument.errors.join("; ")); reviewedSourceDocumentId=reviewedDocument.documentId; ledger.query("UPDATE documents SET supplier_country_code=NULL,supplier_identifier_kind=NULL,supplier_identity_status=NULL WHERE id=?").run(reviewedSourceDocumentId);
  const coverageDocument=ledger.query(`INSERT INTO documents(document_no,sha256_hash,payload_json,upload_datetime,source,status,document_type,sender_name,sender_vat_cvr,supplier_country_code,supplier_identifier_kind,retain_until) VALUES(?,?,?,?,?,?,?,?,?,?,?,?) RETURNING id`).get("COVERAGE-MCP","d".repeat(64),"{}","2026-09-04T00:00:00.000Z","synthetic","posted","purchase_sale","Coverage Supplier","DK44332211","DK","dk_cvr","2032-01-01") as {id:number};
  const bank=addBankAccount(ledger,{name:"Synthetic Bank",slug:"synthetic-bank",ledgerAccountNo:"2000"}).account!;
  ledger.query("INSERT INTO bank_transactions(id,transaction_date,text,amount,currency,transaction_hash,bank_account_id,retain_until) VALUES(700,'2026-09-04','Coverage payment',-100,'DKK','coverage-bank-700',?,'2031-12-31')").run(bank.id);
  const coveragePosting=postJournalEntry(ledger,{transactionDate:"2026-09-04",text:"Coverage purchase",documentId:coverageDocument.id,sourceBankTransactionId:700,createdBy:"agent:synthetic",lines:[{accountNo:"7000",debitAmount:100},{accountNo:"2000",creditAmount:100}]});
  if(!coveragePosting.ok)throw new Error(coveragePosting.errors.join("; "));
  ledger.close();
  const registry = openWorkspaceControlDb(workspace);
  createParty(registry, { partyId: "party-mcp", kind: "organization", name: "Canonical name", identifiers: [{ country: "DK", identifier: "DK12345678", identifierKind: "dk_cvr" }], source: "synthetic", observedAt: "2026-08-30T00:00:00.000Z", reviewAssertion: "reviewed synthetic identity", actor: "user:test" });
  linkPartyRole(registry, { partyId: "party-mcp", companySlug: company.slug, role: "vendor", actor: "user:test" });
  createParty(registry,{partyId:"party-mcp-no-id",kind:"organization",name:"No-ID Supplier Inc.",identifiers:[],source:"synthetic",observedAt:"2026-08-30T00:00:00.000Z",reviewAssertion:"reviewed source document",actor:"user:test"});
  linkPartyRole(registry,{partyId:"party-mcp-no-id",companySlug:company.slug,role:"vendor",actor:"user:test"});
  createParty(registry,{partyId:"party-mcp-reviewed",kind:"organization",name:"Foreign Merchant",identifiers:[],source:"synthetic",observedAt:"2026-09-03T00:00:00.000Z",reviewAssertion:"reviewed source",actor:"user:test"});
  linkPartyRole(registry,{partyId:"party-mcp-reviewed",companySlug:company.slug,role:"vendor",actor:"user:test"});
  createParty(registry,{partyId:"party-coverage-mcp",kind:"organization",name:"Coverage Supplier",identifiers:[{country:"DK",identifierKind:"dk_cvr",identifier:"DK44332211"}],source:"synthetic",observedAt:"2026-09-04T00:00:00.000Z",reviewAssertion:"reviewed typed identity",actor:"user:test"});
  linkPartyRole(registry,{partyId:"party-coverage-mcp",companySlug:company.slug,role:"vendor",actor:"user:test"});
  const runtime=openWorkspaceBetterAuth(workspace,{secret:"I0UjL6i0-ScgvjfIgzMKJxPQyDpPXwg2mMKdLW3Y3WQ",trustedOrigins:["http://127.0.0.1"],baseURL:"http://127.0.0.1"});
  const service=await createWorkspaceServicePrincipal(registry,runtime.auth,{displayName:"Document party test",actor:"user:test"});
  serviceAccountId = service.serviceAccountId;
  serviceToken = service.secret;
  activateWorkspaceUser(registry,{userId:service.serviceAccountId,workspaceRole:"member",actor:"user:test"});
  grantCompanyMembership(registry,workspace,{userId:service.serviceAccountId,companySlug:company.slug,role:"owner",actor:"user:test"});
  registry.close();runtime.close();
  client = new Client(workspace,service.secret);
  const initialized = await client.send("initialize", { protocolVersion: "2024-11-05", capabilities: {}, clientInfo: { name: "document-party-black-box", version: "1.0.0" } });
  expect(initialized.error).toBeUndefined();
  await client.notify("notifications/initialized");
});

afterAll(async () => {
  await client.close();
  rmSync(workspace, { recursive: true, force: true });
});

describe("#588 MCP black-box contract", () => {
  test("#644 discovers and executes projection → plan → confirmed idempotent apply",async()=>{
    const tools=await client.send("tools/list"); const names=(tools.result?.tools??[]).map((tool:any)=>tool.name);
    for(const name of ["party_coverage","party_coverage_plan","party_coverage_apply"])expect(names).toContain(name);
    const projected=(await client.send("tools/call",{name:"party_coverage",arguments:{company:"acme-aps"}})).result?.structuredContent;
    expect(projected).toMatchObject({ok:true,data:{totals:{exact_candidate:1},rows:[{bankTransactionId:700,status:"exact_candidate",candidate:{partyId:"party-coverage-mcp",provenance:"typed_identifier"}}]}});
    const planned=(await client.send("tools/call",{name:"party_coverage_plan",arguments:{company:"acme-aps"}})).result?.structuredContent;
    expect(planned).toMatchObject({ok:true,data:{plan:{operations:[{kind:"document_link"}]}}});
    const args={company:"acme-aps",planHash:planned.data.plan.planHash,idempotencyKey:"coverage-mcp-644",confirm:true};
    const denied=await client.send("tools/call",{name:"party_coverage_apply",arguments:{...args,confirm:false}});expect(denied.result?.structuredContent).toMatchObject({ok:false,code:"CONFIRM_REQUIRED"});
    expect((await client.send("tools/call",{name:"party_coverage_apply",arguments:args})).result?.structuredContent).toMatchObject({ok:true,data:{idempotent:false,applied:1}});
    expect((await client.send("tools/call",{name:"party_coverage_apply",arguments:args})).result?.structuredContent).toMatchObject({ok:true,data:{idempotent:true,applied:1}});
    const db=openDb(companyPaths(companyRoot).db);try{expect(db.query("SELECT actor,principal FROM party_coverage_batch_events WHERE plan_hash=?").get(planned.data.plan.planHash)).toEqual({actor:"agent:document-party-black-box/1.0.0",principal:`service-account:${serviceAccountId}`});}finally{db.close();}
  });
  test("#645 MCP plans a non-tax merchant observation and explicit legal-party follow-up",async()=>{const source=join(workspace,"mcp-merchant-receipt.txt");writeFileSync(source,"immutable receipt showing MCP MERCHANT");const db=openDb(companyPaths(companyRoot).db);const document=ingestDocument(db,companyRoot,source,{source:"synthetic",documentType:"cash_register_receipt",issueDate:"2026-09-05",deliveryDescription:"Synthetic purchase",amountIncVat:20,vatAmount:0,currency:"DKK",sender:{name:"MCP MERCHANT"},recipient:{name:"Acme ApS"}});if(!document.ok)throw new Error(document.errors.join("; "));const bank=db.query("SELECT id FROM bank_accounts ORDER BY id LIMIT 1").get() as {id:number};db.query("INSERT INTO bank_transactions(id,transaction_date,text,amount,currency,transaction_hash,bank_account_id,retain_until) VALUES(701,'2026-09-05','Merchant payment',-20,'DKK','coverage-bank-701',?,'2031-12-31')").run(bank.id);const posting=postJournalEntry(db,{transactionDate:"2026-09-05",text:"Merchant purchase",documentId:document.documentId,sourceBankTransactionId:701,createdBy:"agent:synthetic",lines:[{accountNo:"7000",debitAmount:20},{accountNo:"2000",creditAmount:20}]});if(!posting.ok)throw new Error(posting.errors.join("; "));db.close();const registry=openWorkspaceControlDb(workspace);createParty(registry,{partyId:"party-mcp-merchant",kind:"organization",name:"MCP MERCHANT",source:"synthetic",observedAt:"2026-09-05T00:00:00.000Z",reviewAssertion:"reviewed receipt",actor:"user:test"});linkPartyRole(registry,{partyId:"party-mcp-merchant",companySlug:"acme-aps",role:"establishment",actor:"user:test"});registry.close();const decision={bankTransactionId:701,partyId:"party-mcp-merchant",role:"establishment",evidenceReference:"receipt header",rationale:"reviewed merchant only",sourceReview:{observedName:"MCP MERCHANT",sourceReference:"receipt header",sourceLocation:"line 1",rationale:"exact source observation"}};const planned=(await client.send("tools/call",{name:"party_coverage_plan",arguments:{company:"acme-aps",decisions:[decision]}})).result?.structuredContent;expect(planned).toMatchObject({ok:true,data:{plan:{operations:[{kind:"document_link"}]}}});const args={company:"acme-aps",decisions:[decision],planHash:planned.data.plan.planHash,idempotencyKey:"coverage-mcp-645-observed",confirm:true};expect((await client.send("tools/call",{name:"party_coverage_apply",arguments:args})).result?.structuredContent).toMatchObject({ok:true,data:{idempotent:false}});const projected=(await client.send("tools/call",{name:"party_coverage",arguments:{company:"acme-aps"}})).result?.structuredContent;expect(projected.data.rows.find((row:any)=>row.bankTransactionId===701)).toMatchObject({status:"source_observed",candidate:{provenance:"source_observed_non_tax"}});const unresolved={bankTransactionId:701,unresolvedExternalParty:true,evidenceReference:"legal identity absent",rationale:"merchant is not legal supplier evidence",nextAction:"Request supplier identity evidence."};const followup=(await client.send("tools/call",{name:"party_coverage_plan",arguments:{company:"acme-aps",decisions:[unresolved]}})).result?.structuredContent;expect(followup.ok).toBeTrue();expect((await client.send("tools/call",{name:"party_coverage_apply",arguments:{company:"acme-aps",decisions:[unresolved],planHash:followup.data.plan.planHash,idempotencyKey:"coverage-mcp-645-followup",confirm:true}})).result?.structuredContent).toMatchObject({ok:true,data:{idempotent:false}});});
  for (const kind of ["dk_cvr", "non_eu"] as const) {
    test(`public imported ${kind} enrichment → mapping → document resolution`, async () => {
      const name = `Synthetic imported ${kind}`;
      const identifier = kind === "dk_cvr" ? "DK99887766" : undefined;
      const countryCode = kind === "dk_cvr" ? "DK" : "US";
      const db = openDb(companyPaths(companyRoot).db);
      const vendor = createVendor(db, { name, address: "4 Evidence Road", vatOrCvr: identifier, notes: "Preserved imported note" });
      if (!vendor.ok) throw new Error("fixture vendor failed");
      const source = join(workspace, `${kind}-public.txt`);
      writeFileSync(source, `Synthetic immutable invoice ${kind}`);
      const document = ingestDocument(db, companyRoot, source, {
        source: "synthetic", documentType: "purchase_sale", issueDate: "2026-09-02",
        invoiceNo: `PUBLIC-${kind}`, deliveryDescription: "Synthetic service", amountIncVat: 125,
        vatAmount: kind === "dk_cvr" ? 25 : 0, currency: "DKK",
        sender: { name, address: "4 Evidence Road", vatOrCvr: identifier, countryCode, identifierKind: kind },
        recipient: { name: "Acme ApS", address: "Buyer Road", vatOrCvr: "DK87654321" },
      });
      if (!document.ok) throw new Error("fixture document failed");
      db.close();
      const control = openWorkspaceControlDb(workspace);
      const partyId = `public-${kind}`;
      createParty(control, { partyId, kind: "organization", name,
        identifiers: identifier ? [{ country: "DK", identifier, identifierKind: "dk_cvr" }] : [],
        source: "synthetic", observedAt: "2026-09-02T00:00:00.000Z", reviewAssertion: "Reviewed original", actor: "user:test" });
      linkPartyRole(control, { partyId, companySlug: "acme-aps", role: "vendor", actor: "user:test" });
      control.close();
      const call = async (operation: string, args: Record<string, unknown>) => {
        const response = await client.send("tools/call", { name: operation, arguments: args });
        const envelope = response.result?.structuredContent;
        expect(envelope?.ok, JSON.stringify(response)).toBeTrue();
        return envelope.data;
      };
      const identity = { company: "acme-aps", vendorId: vendor.vendorId, documentId: document.documentId,
        countryCode, identifierKind: kind, ...(identifier ? { identifier } : {}), reviewedReference: `review:${kind}` };
      const enrichment = await call("vendor_identity_enrichment_plan", identity);
      await call("vendor_identity_enrichment_apply", { ...identity, planHash: enrichment.plan.planHash, idempotencyKey: `identity-${kind}`, confirm: true });
      const mapping = { company: "acme-aps", legacyKind: "vendor", legacyId: String(vendor.vendorId), partyId,
        role: "vendor", documentId: document.documentId, reviewedLegacyReference: `review:${kind}` };
      const planned = await call("legacy_party_mapping_plan", mapping);
      await call("legacy_party_mapping_apply", { ...mapping, planHash: planned.plan.planHash, idempotencyKey: `mapping-${kind}`, confirm: true });
      const linked = await call("documents_party_link_plan", mapping);
      await call("documents_party_link_apply", { ...mapping, planHash: linked.plan.planHash, idempotencyKey: `link-${kind}`, confirm: true, principal: "service-account:spoofed" });
      const inspected = await call("documents_party_link_inspect", { company: "acme-aps", documentId: document.documentId });
      expect(inspected.links).toContainEqual(expect.objectContaining({ party_id: partyId, event_type: "linked" }));
      expect(inspected.links).toContainEqual(expect.objectContaining({
        principal: `service-account:${serviceAccountId}`,
        actor: "agent:document-party-black-box/1.0.0",
      }));
    });
  }
  test("discovers and executes the read-plan → confirmed apply → inspect lifecycle", async () => {
    const tools = await client.send("tools/list");
    const names = (tools.result?.tools ?? []).map((tool: any) => tool.name);
    expect(names).toContain("documents_party_link_plan");
    expect(names).toContain("documents_party_link_apply");
    const schema = (tools.result.tools as any[]).find((tool) => tool.name === "documents_party_link_plan")?.inputSchema;
    expect(schema.properties.workspace).toBeUndefined();
    expect(schema.properties.companySlug).toBeUndefined();
    expect(schema.properties.sourceReview.properties).toMatchObject({ observedName:{type:"string"}, sourceLocation:{type:"string"}, rationale:{type:"string"} });

    const input = { company: "acme-aps", documentId: 1, role: "vendor", partyId: "party-mcp", jurisdiction: "DK", identifierKind: "dk_cvr", identifier: "DK12345678" };
    const beforeCount = eventCount();
    const beforeMtime = statSync(companyPaths(companyRoot).db).mtimeMs;
    const planned = await client.send("tools/call", { name: "documents_party_link_plan", arguments: input });
    const planEnvelope = planned.result?.structuredContent;
    expect(planEnvelope?.ok, JSON.stringify(planEnvelope)).toBe(true);
    expect(planEnvelope.data.plan.partyId).toBe("party-mcp");
    expect(eventCount()).toBe(beforeCount);
    expect(statSync(companyPaths(companyRoot).db).mtimeMs).toBe(beforeMtime);

    const denied = await client.send("tools/call", { name: "documents_party_link_apply", arguments: { ...input, planHash: planEnvelope.data.plan.planHash, idempotencyKey: "mcp-588-1", confirm: false } });
    expect(denied.result?.structuredContent).toMatchObject({ ok: false, code: "CONFIRM_REQUIRED" });
    expect(eventCount()).toBe(beforeCount);

    const applyArgs = { ...input, planHash: planEnvelope.data.plan.planHash, idempotencyKey: "mcp-588-1", confirm: true };
    const applied = await client.send("tools/call", { name: "documents_party_link_apply", arguments: applyArgs });
    expect(applied.result?.structuredContent).toMatchObject({ ok: true, data: { idempotent: false } });
    const retried = await client.send("tools/call", { name: "documents_party_link_apply", arguments: applyArgs });
    expect(retried.result?.structuredContent).toMatchObject({ ok: true, data: { idempotent: true } });
    expect(eventCount()).toBe(beforeCount + 1);

    const inspected = await client.send("tools/call", { name: "documents_party_link_inspect", arguments: { company: "acme-aps", documentId: 1 } });
    expect(inspected.result?.structuredContent).toMatchObject({ ok: true, data: { links: [{ party_id: "party-mcp", event_type: "linked" }] } });
    const listed = await client.send("tools/call", { name: "documents_party_link_list", arguments: { company: "acme-aps", status: "linked" } });
    expect(listed.result?.structuredContent?.data.links).toContainEqual(expect.objectContaining({id:1,resolution_state:"resolved"}));
  });

  test("plans and applies an immutable-source-bound ID-less legacy counterparty",async()=>{
    const input={company:"acme-aps",documentId:reviewedSourceDocumentId,partyId:"party-mcp-reviewed",role:"vendor",sourceReview:{observedName:"Foreign Merchant",jurisdiction:"US",identifierKind:"non_eu",sourceReference:"synthetic source evidence",sourceLocation:"page 1",rationale:"reviewed external counterparty"}};
    const planned=await client.send("tools/call",{name:"documents_party_link_plan",arguments:input}); expect(planned.result?.structuredContent?.ok,JSON.stringify(planned)).toBeTrue(); const plan=planned.result.structuredContent.data.plan; expect(plan.evidence).toMatchObject({kind:"reviewed_source_observation",accountingEffect:"none",taxEffect:"none"});
    const denied=await client.send("tools/call",{name:"documents_party_link_apply",arguments:{...input,planHash:plan.planHash,idempotencyKey:"mcp-source-640",confirm:false}}); expect(denied.result?.structuredContent).toMatchObject({ok:false,code:"CONFIRM_REQUIRED"});
    const applied=await client.send("tools/call",{name:"documents_party_link_apply",arguments:{...input,planHash:plan.planHash,idempotencyKey:"mcp-source-640",confirm:true}}); expect(applied.result?.structuredContent).toMatchObject({ok:true,data:{idempotent:false}});
  });

  test("discovers and executes the reviewed no-identifier legacy mapping before document linking",async()=>{
    const tools=await client.send("tools/list");
    const names=(tools.result?.tools??[]).map((tool:any)=>tool.name);
    for(const name of ["legacy_party_mapping_plan","legacy_party_mapping_apply","legacy_party_mapping_list","legacy_party_mapping_supersede"])expect(names).toContain(name);
    const input={company:"acme-aps",legacyKind:"vendor",legacyId:"1",partyId:"party-mcp-no-id",role:"vendor",documentId:2,reviewedLegacyReference:"review:synthetic-document:2"};
    const planned=await client.send("tools/call",{name:"legacy_party_mapping_plan",arguments:input});
    expect(planned.result?.structuredContent?.ok,JSON.stringify(planned)).toBe(true);
    const planHash=planned.result.structuredContent.data.plan.planHash;
    const denied=await client.send("tools/call",{name:"legacy_party_mapping_apply",arguments:{...input,planHash,idempotencyKey:"legacy-mcp-1",confirm:false}});
    expect(denied.result?.structuredContent).toMatchObject({ok:false,code:"CONFIRM_REQUIRED"});
    const applyArgs={...input,planHash,idempotencyKey:"legacy-mcp-1",confirm:true};
    expect((await client.send("tools/call",{name:"legacy_party_mapping_apply",arguments:applyArgs})).result?.structuredContent).toMatchObject({ok:true,data:{idempotent:false}});
    expect((await client.send("tools/call",{name:"legacy_party_mapping_apply",arguments:applyArgs})).result?.structuredContent).toMatchObject({ok:true,data:{idempotent:true}});
    const history=(await client.send("tools/call",{name:"legacy_party_mapping_list",arguments:{company:"acme-aps",legacyKind:"vendor",legacyId:"1"}})).result?.structuredContent;
    expect(history).toMatchObject({ok:true,data:{rows:[{partyId:"party-mcp-no-id",contactSnapshot:{notes:"MCP preserved note"}}]}});
    const documentPlan=await client.send("tools/call",{name:"documents_party_link_plan",arguments:{company:"acme-aps",documentId:2,partyId:"party-mcp-no-id",role:"vendor",legacyKind:"vendor",legacyId:"1",reviewedLegacyReference:input.reviewedLegacyReference}});
    expect(documentPlan.result?.structuredContent?.ok,JSON.stringify(documentPlan)).toBe(true);
  });

  test("discovers and executes byte-bound imported vendor identity enrichment",async()=>{
    const tools=await client.send("tools/list");
    const names=(tools.result?.tools??[]).map((tool:any)=>tool.name);
    for(const name of ["vendor_identity_enrichment_plan","vendor_identity_enrichment_apply","vendor_identity_enrichment_list"])expect(names).toContain(name);
    const input={company:"acme-aps",vendorId:2,documentId:3,countryCode:"DK",identifierKind:"dk_cvr",identifier:"DK11223344",reviewedReference:"review:mcp:vendor:3"};
    const planned=await client.send("tools/call",{name:"vendor_identity_enrichment_plan",arguments:input});
    expect(planned.result?.structuredContent?.ok,JSON.stringify(planned)).toBe(true);
    const planHash=planned.result.structuredContent.data.plan.planHash;
    const denied=await client.send("tools/call",{name:"vendor_identity_enrichment_apply",arguments:{...input,planHash,idempotencyKey:"vendor-enrich-mcp-1",confirm:false}});
    expect(denied.result?.structuredContent).toMatchObject({ok:false,code:"CONFIRM_REQUIRED"});
    const args={...input,planHash,idempotencyKey:"vendor-enrich-mcp-1",confirm:true};
    expect((await client.send("tools/call",{name:"vendor_identity_enrichment_apply",arguments:args})).result?.structuredContent).toMatchObject({ok:true,data:{idempotent:false}});
    expect((await client.send("tools/call",{name:"vendor_identity_enrichment_apply",arguments:args})).result?.structuredContent).toMatchObject({ok:true,data:{idempotent:true}});
    expect((await client.send("tools/call",{name:"vendor_identity_enrichment_list",arguments:{company:"acme-aps",vendorId:2}})).result?.structuredContent).toMatchObject({ok:true,data:{rows:[{vendor_id:2,document_id:3}]}});
  });
  test("#639 a different MCP actor retains the authenticated subject on correction", async () => {
    const second = new Client(workspace, serviceToken);
    try {
      await second.send("initialize", { protocolVersion: "2024-11-05", capabilities: {}, clientInfo: { name: "second-party-reviewer", version: "1.0.0" } });
      await second.notify("notifications/initialized");
      const history = await second.send("tools/call", { name: "documents_party_link_inspect", arguments: { company: "acme-aps", documentId: 1 } });
      const planHash = history.result.structuredContent.data.links[0].plan_hash;
      const result = await second.send("tools/call", { name: "documents_party_link_supersede", arguments: { company: "acme-aps", documentId: 1, role: "vendor", planHash, reason: "Synthetic reviewed correction", confirm: true } });
      expect(result.result?.structuredContent?.ok).toBeTrue();
      const db = openDb(companyPaths(companyRoot).db);
      try {
        expect(db.query("SELECT actor,principal FROM document_party_link_events WHERE document_id=1 ORDER BY id").all()).toEqual([
          { actor: "agent:document-party-black-box/1.0.0", principal: `service-account:${serviceAccountId}` },
          { actor: "agent:second-party-reviewer/1.0.0", principal: `service-account:${serviceAccountId}` },
        ]);
      } finally { db.close(); }
    } finally { await second.close(); }
  });
});
