import { describe, expect, test } from "bun:test";
import { appendFileSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { openWorkspaceControlDb } from "../../src/core/workspace-control";
import { makeWorkspace } from "./server-api/_shared";
import { companyRootForSlug } from "../../src/core/workspace";
import { companyPaths } from "../../src/core/paths";
import { migrate, openDb } from "../../src/core/db";
import { createVendor } from "../../src/core/master-data";
import { ingestDocument } from "../../src/core/documents";
import { createParty, linkPartyRole } from "../../src/core/party-registry";
import { openWorkspaceBetterAuth } from "../../src/server/better-auth";
import { createWorkspaceServicePrincipal } from "../../src/core/workspace-service-principals";
import { activateWorkspaceUser, grantCompanyMembership } from "../../src/core/workspace-access";

const emptyActorEnv = { PATH: process.env.PATH ?? "", HOME: process.env.HOME ?? "" };

function command(args: string[], env = process.env) {
  return Bun.spawn(["bun", "run", "src/cli.ts", ...args], {
    cwd: process.cwd(), env, stdout: "pipe", stderr: "pipe",
  });
}

describe("workspace registry CLI safety gates", () => {
  test("rejects an unconfirmed mutation and then a mutation without an audit actor", async () => {
    const workspace = mkdtempSync(join(tmpdir(), "rentemester-workspace-registry-cli-"));
    const input = join(workspace, "party.json");
    writeFileSync(input, JSON.stringify({ kind: "organization", name: "Synthetic party", source: "test", observedAt: "2026-01-01", reviewAssertion: "synthetic evidence" }));
    try {
      const noConfirm = command(["party", "create", "--workspace", workspace, "--input", input], emptyActorEnv);
      expect(await noConfirm.exited).toBe(2);
      expect(await new Response(noConfirm.stderr).text()).toContain("--confirm must be exactly yes");

      const noActor = command(["party", "create", "--workspace", workspace, "--input", input, "--confirm", "yes"], emptyActorEnv);
      expect(await noActor.exited).toBe(2);
      expect(await new Response(noActor.stderr).text()).toContain("actor required for mutations");
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });

  test("keeps read commands independent of mutation actor and confirmation gates", async () => {
    const workspace = makeWorkspace("workspace-registry-cli-read", ["Synthetic Company"]);
    const db = openWorkspaceControlDb(workspace);
    db.close();
    try {
      const read = command(["party", "search", "--workspace", workspace, "--company", "synthetic-company"], emptyActorEnv);
      expect(await read.exited).toBe(0);
      expect(JSON.parse(await new Response(read.stdout).text())).toMatchObject({ ok: true, rows: [], count: 0 });
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });

  test("offers read-plan and authenticated confirmed apply through the CLI",async()=>{
    const workspace=makeWorkspace("legacy-mapping-cli",["Synthetic Company"]),root=companyRootForSlug(workspace,"synthetic-company");
    appendFileSync(join(root,"config","policy.yaml"),"  agents:\n    - agent:legacy-cli/1\n");
    const ledger=openDb(companyPaths(root).db);migrate(ledger);const vendor=createVendor(ledger,{name:"Foreign CLI Inc.",address:"1 CLI Road",countryCode:"US",identifierKind:"non_eu",notes:"CLI note"});if(!vendor.ok)throw new Error(vendor.errors.join("; "));ledger.query(`INSERT INTO documents(document_no,sha256_hash,payload_json,upload_datetime,source,status,document_type,sender_name,sender_address,sender_vat_cvr,supplier_country_code,supplier_identifier_kind,supplier_identity_status) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)`).run("CLI-638","8".repeat(64),"{}","2026-09-01T00:00:00.000Z","synthetic","inbox","purchase_sale","Foreign CLI Inc.","1 CLI Road",null,"US","non_eu","resolved");ledger.close();
    const control=openWorkspaceControlDb(workspace),runtime=openWorkspaceBetterAuth(workspace,{secret:"I0UjL6i0-ScgvjfIgzMKJxPQyDpPXwg2mMKdLW3Y3WQ",trustedOrigins:["http://127.0.0.1"],baseURL:"http://127.0.0.1"});
    try{createParty(control,{partyId:"party-cli-638",kind:"organization",name:"Foreign CLI Inc.",identifiers:[],source:"synthetic",observedAt:"2026-09-01T00:00:00.000Z",reviewAssertion:"reviewed",actor:"user:test"});linkPartyRole(control,{partyId:"party-cli-638",companySlug:"synthetic-company",role:"vendor",actor:"user:test"});const service=await createWorkspaceServicePrincipal(control,runtime.auth,{displayName:"Legacy CLI",actor:"user:test"});activateWorkspaceUser(control,{userId:service.serviceAccountId,workspaceRole:"member",actor:"user:test"});grantCompanyMembership(control,workspace,{userId:service.serviceAccountId,companySlug:"synthetic-company",role:"owner",actor:"user:test"});
      const common=["--workspace",workspace,"--company","synthetic-company","--legacy-kind","vendor","--legacy-id",String(vendor.vendorId),"--party-id","party-cli-638","--role","vendor","--document-id","1","--reviewed-legacy-reference","review:cli:1"];
      const planned=command(["legacy-party-mapping","plan",...common],emptyActorEnv);expect(await planned.exited).toBe(0);const plan=JSON.parse(await new Response(planned.stdout).text());expect(plan).toMatchObject({ok:true,plan:{partyId:"party-cli-638"}});
      const env={...process.env,RENTEMESTER_WORKSPACE:workspace,RENTEMESTER_SERVICE_PRINCIPAL_TOKEN:service.secret};const applied=command(["legacy-party-mapping","apply",...common,"--plan-hash",plan.plan.planHash,"--idempotency-key","cli-638-1","--actor","agent:legacy-cli/1","--confirm","yes"],env);expect(await applied.exited).toBe(0);expect(JSON.parse(await new Response(applied.stdout).text())).toMatchObject({ok:true,idempotent:false});
    }finally{control.close();runtime.close();rmSync(workspace,{recursive:true,force:true});}
  });

  test("enriches an imported vendor only with a confirmed authorized service principal",async()=>{
    const workspace=makeWorkspace("vendor-enrichment-cli",["Synthetic Company"]),root=companyRootForSlug(workspace,"synthetic-company");
    appendFileSync(join(root,"config","policy.yaml"),"  agents:\n    - agent:vendor-enrichment-cli/1\n");
    const source=join(workspace,"vendor-enrichment-source.txt");writeFileSync(source,"immutable CLI vendor invoice");
    const ledger=openDb(companyPaths(root).db);migrate(ledger);
    const vendor=createVendor(ledger,{name:"Imported CLI ApS",address:"CLI Evidence Road 2",vatOrCvr:"DK11223344",notes:"keep CLI note"});
    if(!vendor.ok)throw new Error(vendor.errors.join("; "));
    const document=ingestDocument(ledger,root,source,{source:"synthetic",documentType:"purchase_sale",issueDate:"2026-09-02",invoiceNo:"CLI-ENRICH",deliveryDescription:"Synthetic",amountIncVat:125,vatAmount:25,currency:"DKK",sender:{name:"Imported CLI ApS",address:"CLI Evidence Road 2",vatOrCvr:"DK11223344",countryCode:"DK",identifierKind:"dk_cvr"},recipient:{name:"Synthetic Company",address:"Buyer Road",vatOrCvr:"DK87654321"}});
    if(!document.ok)throw new Error(document.errors.join("; "));
    ledger.close();
    const control=openWorkspaceControlDb(workspace),runtime=openWorkspaceBetterAuth(workspace,{secret:"I0UjL6i0-ScgvjfIgzMKJxPQyDpPXwg2mMKdLW3Y3WQ",trustedOrigins:["http://127.0.0.1"],baseURL:"http://127.0.0.1"});
    try{
      const service=await createWorkspaceServicePrincipal(control,runtime.auth,{displayName:"Vendor enrichment CLI",actor:"user:test"});
      activateWorkspaceUser(control,{userId:service.serviceAccountId,workspaceRole:"member",actor:"user:test"});
      grantCompanyMembership(control,workspace,{userId:service.serviceAccountId,companySlug:"synthetic-company",role:"bookkeeper",actor:"user:test"});
      const common=["--workspace",workspace,"--company","synthetic-company","--vendor-id",String(vendor.vendorId),"--document-id",String(document.documentId),"--country-code","DK","--identifier-kind","dk_cvr","--identifier","DK11223344","--reviewed-reference","review:cli:vendor"];
      const planned=command(["vendor-identity-enrichment","plan",...common],emptyActorEnv);expect(await planned.exited).toBe(0);const plan=JSON.parse(await new Response(planned.stdout).text());expect(plan).toMatchObject({ok:true,plan:{proposedIdentity:{countryCode:"DK"}}});
      const withoutCredential=command(["vendor-identity-enrichment","apply",...common,"--plan-hash",plan.plan.planHash,"--idempotency-key","cli-vendor-enrich-1","--actor","agent:vendor-enrichment-cli/1","--confirm","yes"],emptyActorEnv);
      expect(await withoutCredential.exited).toBe(2);expect(await new Response(withoutCredential.stderr).text()).toContain("RENTEMESTER_SERVICE_PRINCIPAL_TOKEN");
      const env={...process.env,RENTEMESTER_WORKSPACE:workspace,RENTEMESTER_SERVICE_PRINCIPAL_TOKEN:service.secret};
      const applied=command(["vendor-identity-enrichment","apply",...common,"--plan-hash",plan.plan.planHash,"--idempotency-key","cli-vendor-enrich-1","--actor","agent:vendor-enrichment-cli/1","--confirm","yes"],env);
      expect(await applied.exited).toBe(0);expect(JSON.parse(await new Response(applied.stdout).text())).toMatchObject({ok:true,idempotent:false});
      const listed=command(["vendor-identity-enrichment","list","--workspace",workspace,"--company","synthetic-company","--vendor-id",String(vendor.vendorId)],emptyActorEnv);
      expect(await listed.exited).toBe(0);expect(JSON.parse(await new Response(listed.stdout).text())).toMatchObject({ok:true,rows:[{vendor_id:vendor.vendorId}]});
    }finally{control.close();runtime.close();rmSync(workspace,{recursive:true,force:true});}
  });
});
