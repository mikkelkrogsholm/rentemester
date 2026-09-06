import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { openWorkspaceBetterAuth } from "../../src/server/better-auth";
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { createWorkspaceServicePrincipal, revokeWorkspaceServiceCredential, rotateWorkspaceServiceCredential } from "../../src/core/workspace-service-principals";
import { activateWorkspaceUser, grantCompanyMembership, revokeCompanyMembership } from "../../src/core/workspace-access";
import { openWorkspaceControlDb } from "../../src/core/workspace-control";
import { authorizeMcpTool, createMcpSecurityContextFromEnv, MCP_TOOL_PERMISSIONS, resolveMcpWorkspaceCompany, runWithMcpAuthenticatedPrincipal } from "../../src/mcp/security";
import { makeWorkspace } from "./server-api/_shared";
import { companyRootForSlug } from "../../src/core/workspace";
import { companyPaths } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { ingestDocument } from "../../src/core/documents";
import { registerPayableTools } from "../../src/mcp/tools/payable";

const SECRET = "I0UjL6i0-ScgvjfIgzMKJxPQyDpPXwg2mMKdLW3Y3WQ";
const ORIGIN = "http://127.0.0.1:4319";

function payableHarness() {
  const server = new McpServer({ name: "service-idempotency-test", version: "0.0.0" });
  registerPayableTools(server);
  const transport = server.server as unknown as { _clientVersion?: unknown };
  transport._clientVersion = { name: "first-agent", version: "1" };
  const tools = (server as unknown as { _registeredTools: Record<string, { handler: (args: unknown, extra: unknown) => Promise<{ structuredContent: unknown }> }> })._registeredTools;
  return {
    setActor(name: string) { transport._clientVersion = { name, version: "1" }; },
    async call(args: Record<string, unknown>) { return (await tools.payable_register!.handler(args, { signal: new AbortController().signal })).structuredContent as { ok: boolean; data?: Record<string, unknown>; errors: string[] }; },
  };
}

function purchaseDocument(workspace: string, slug: string, invoiceNo: string): number {
  const root = companyRootForSlug(workspace, slug); const db = openDb(companyPaths(root).db);
  try {
    migrate(db); const source = join(root, `${invoiceNo}.txt`); writeFileSync(source, invoiceNo);
    const result = ingestDocument(db, root, source, { source: "email", issueDate: "2026-01-10", invoiceNo, deliveryDescription: "synthetic", amountIncVat: 100, currency: "DKK", sender: { name: "Synthetic supplier", address: "Road 1", vatOrCvr: "DK11223344" }, recipient: { name: "Synthetic buyer", address: "Road 2", vatOrCvr: "DK12345678" }, vatAmount: 0, paymentDetails: "bank" });
    expect(result.ok).toBe(true); return result.documentId!;
  } finally { db.close(); }
}

describe("MCP service principal guard", () => {
  test("authorizes ownership snapshots only when a real service credential has every endpoint role", async () => {
    const workspace=makeWorkspace("mcp-ownership-guard",["Allowed ApS","Hidden ApS"]);const runtime=openWorkspaceBetterAuth(workspace,{secret:SECRET,trustedOrigins:[ORIGIN],baseURL:ORIGIN});const db=openWorkspaceControlDb(workspace);
    try { const issued=await createWorkspaceServicePrincipal(db,runtime.auth,{displayName:"ownership agent",actor:"user:owner"});activateWorkspaceUser(db,{userId:issued.serviceAccountId,workspaceRole:"member",actor:"user:owner"});grantCompanyMembership(db,workspace,{userId:issued.serviceAccountId,companySlug:"allowed-aps",role:"reviewer",actor:"user:owner"});const contextFor=(token:string)=>createMcpSecurityContextFromEnv({RENTEMESTER_WORKSPACE:workspace,RENTEMESTER_SERVICE_PRINCIPAL_TOKEN:token})!;const args={company:"allowed-aps",snapshotId:"ownership-mcp",source:"synthetic",observedAt:"2026-01-01T00:00:00Z",facts:[{owner:{kind:"company",companySlug:"allowed-aps"},ownedCompanySlug:"hidden-aps",validFrom:"2026-01-01",economicBasisPoints:10000,controlType:"equity",jurisdiction:"DK",evidenceRefs:["synthetic"]}],confirm:true};
      expect(await authorizeMcpTool(contextFor(issued.secret),"ownership_graph_query",{company:"allowed-aps",asOf:"2026-02-01"})).not.toBeNull();expect(await authorizeMcpTool(contextFor(issued.secret),"ownership_snapshot_propose",args)).toBeNull();grantCompanyMembership(db,workspace,{userId:issued.serviceAccountId,companySlug:"hidden-aps",role:"reviewer",actor:"user:owner"});expect(await authorizeMcpTool(contextFor(issued.secret),"ownership_snapshot_propose",args)).not.toBeNull();const rotated=await rotateWorkspaceServiceCredential(db,runtime.auth,{serviceAccountId:issued.serviceAccountId,credentialId:issued.credentialId,actor:"user:owner"});expect(await authorizeMcpTool(contextFor(issued.secret),"ownership_snapshot_propose",args)).toBeNull();expect(await authorizeMcpTool(contextFor(rotated.secret),"ownership_snapshot_propose",args)).not.toBeNull();await revokeWorkspaceServiceCredential(db,runtime.auth,{serviceAccountId:issued.serviceAccountId,credentialId:rotated.credentialId,actor:"user:owner"});expect(await authorizeMcpTool(contextFor(rotated.secret),"ownership_snapshot_propose",args)).toBeNull();
    } finally {db.close();runtime.close();rmSync(workspace,{recursive:true,force:true});}
  });
  test("captures token, revalidates revocation, and confines company paths", async () => {
    const workspace = makeWorkspace("mcp-service-guard", ["Allowed ApS", "Hidden ApS"]);
    const outside = mkdtempSync(join(tmpdir(), "rentemester-mcp-outside-"));
    const runtime = openWorkspaceBetterAuth(workspace, { secret: SECRET, trustedOrigins: [ORIGIN], baseURL: ORIGIN });
    const db = openWorkspaceControlDb(workspace);
    try {
      const issued = await createWorkspaceServicePrincipal(db, runtime.auth, { displayName: "Synthetic MCP", actor: "user:owner" });
      activateWorkspaceUser(db, { userId: issued.serviceAccountId, workspaceRole: "member", actor: "user:owner" });
      grantCompanyMembership(db, workspace, { userId: issued.serviceAccountId, companySlug: "allowed-aps", role: "reader", actor: "user:owner" });
      const env: Record<string, string> = { RENTEMESTER_WORKSPACE: workspace, RENTEMESTER_SERVICE_PRINCIPAL_TOKEN: issued.secret };
      const context = createMcpSecurityContextFromEnv(env)!;
      expect(env.RENTEMESTER_SERVICE_PRINCIPAL_TOKEN).toBeUndefined();
      expect(await authorizeMcpTool(context, "accounts_list", { company: "allowed-aps" })).not.toBeNull();
      expect(await authorizeMcpTool(context, "accounts_add", { company: "allowed-aps" })).toBeNull();
      expect(await authorizeMcpTool(context, "accounts_list", { company: "hidden-aps" })).toBeNull();
      // Fan-out tools cannot use a partly authorized key.  The hidden active
      // company is denied before the handler can inspect or mutate either
      // ledger, and a caller cannot replace the canonical workspace root.
      expect(await authorizeMcpTool(context, "recurring_invoice_run_workspace", { workspace })).toBeNull();
      expect(await authorizeMcpTool(context, "efaktura_modtag_workspace", { workspace: outside })).toBeNull();
      expect(resolveMcpWorkspaceCompany(context, outside)).toBeNull();
      const link = join(workspace, "escape"); symlinkSync(outside, link);
      expect(resolveMcpWorkspaceCompany(context, link)).toBeNull();
      await revokeWorkspaceServiceCredential(db, runtime.auth, { serviceAccountId: issued.serviceAccountId, credentialId: issued.credentialId, actor: "user:owner" });
      expect(await authorizeMcpTool(context, "accounts_list", { company: "allowed-aps" })).toBeNull();
    } finally { db.close(); runtime.close(); rmSync(workspace, { recursive: true, force: true }); rmSync(outside, { recursive: true, force: true }); }
  });

  test("keeps a complete, unique map for the live MCP surface", () => {
    expect(new Set(Object.keys(MCP_TOOL_PERMISSIONS)).size).toBe(Object.keys(MCP_TOOL_PERMISSIONS).length);
      expect(Object.keys(MCP_TOOL_PERMISSIONS)).toHaveLength(246);
  });

  test("requires reviewer permission for an atomic dimension replacement", async () => {
    const workspace = makeWorkspace("mcp-dimension-replace-permission", ["Allowed ApS"]);
    const runtime = openWorkspaceBetterAuth(workspace, { secret: SECRET, trustedOrigins: [ORIGIN], baseURL: ORIGIN });
    const db = openWorkspaceControlDb(workspace);
    try {
      const issued = await createWorkspaceServicePrincipal(db, runtime.auth, { displayName: "dimension agent", actor: "user:owner" });
      activateWorkspaceUser(db, { userId: issued.serviceAccountId, workspaceRole: "member", actor: "user:owner" });
      grantCompanyMembership(db, workspace, { userId: issued.serviceAccountId, companySlug: "allowed-aps", role: "bookkeeper", actor: "user:owner" });
      const context = createMcpSecurityContextFromEnv({ RENTEMESTER_WORKSPACE: workspace, RENTEMESTER_SERVICE_PRINCIPAL_TOKEN: issued.secret })!;
      const args = { company: "allowed-aps", journalLineId: 1, expectedAssignmentId: 1, allocations: [{ dimensionId: "project", memberId: "alpha", amountMinor: 100, currency: "DKK" }], planHash: "a".repeat(64), reason: "synthetic review", idempotencyKey: "replace-synthetic", confirm: true };
      const budgetArgs = { company: "allowed-aps", accountNo: "3000", period: "2026-01", allocations: [{ dimensionId: "project", memberId: "alpha", amount: 100 }], sourceRef: "synthetic-review", planHash: "b".repeat(64), idempotencyKey: "budget-synthetic", confirm: true };
      expect(await authorizeMcpTool(context, "dimension_budget_plan", budgetArgs)).not.toBeNull();
      expect(await authorizeMcpTool(context, "dimension_budget_list", { company: "allowed-aps" })).not.toBeNull();
      expect(await authorizeMcpTool(context, "dimension_budget_apply", budgetArgs)).toBeNull();
      expect(await authorizeMcpTool(context, "dimension_assignment_replace", args)).toBeNull();
      grantCompanyMembership(db, workspace, { userId: issued.serviceAccountId, companySlug: "allowed-aps", role: "reviewer", actor: "user:owner" });
      expect(await authorizeMcpTool(context, "dimension_assignment_replace", args)).not.toBeNull();
      expect(await authorizeMcpTool(context, "dimension_budget_apply", budgetArgs)).not.toBeNull();
    } finally { db.close(); runtime.close(); rmSync(workspace, { recursive: true, force: true }); }
  });

  test("a hosted service principal replays after token rotation but never across revoked membership, principal, or company", async () => {
    const workspace = makeWorkspace("mcp-service-idempotency", ["Allowed ApS", "Other ApS"]);
    const runtime = openWorkspaceBetterAuth(workspace, { secret: SECRET, trustedOrigins: [ORIGIN], baseURL: ORIGIN });
    const db = openWorkspaceControlDb(workspace);
    const previousWorkspace = process.env.RENTEMESTER_WORKSPACE;
    process.env.RENTEMESTER_WORKSPACE = workspace;
    try {
      const first = await createWorkspaceServicePrincipal(db, runtime.auth, { displayName: "writer one", actor: "user:owner" });
      const second = await createWorkspaceServicePrincipal(db, runtime.auth, { displayName: "writer two", actor: "user:owner" });
      for (const service of [first, second]) {
        activateWorkspaceUser(db, { userId: service.serviceAccountId, workspaceRole: "member", actor: "user:owner" });
        grantCompanyMembership(db, workspace, { userId: service.serviceAccountId, companySlug: "allowed-aps", role: "bookkeeper", actor: "user:owner" });
      }
      writeFileSync(join(companyPaths(companyRootForSlug(workspace, "allowed-aps")).config, "policy.yaml"), "actor_allowlist:\n  agents:\n    - agent:first-agent/1\n");
      const documentId = purchaseDocument(workspace, "allowed-aps", "MCP-IDEM-1");
      const harness = payableHarness();
      const args = { company: "allowed-aps", documentId, billDate: "2026-01-10", dueDate: "2026-02-10", expenseAccount: "3000", vatTreatment: "exempt", confirm: true, idempotencyKey: "hosted-retry-key" };
      const contextFor = (token: string) => createMcpSecurityContextFromEnv({ RENTEMESTER_WORKSPACE: workspace, RENTEMESTER_SERVICE_PRINCIPAL_TOKEN: token })!;
      const run = async (context: ReturnType<typeof contextFor>) => {
        const authorized = await authorizeMcpTool(context, "payable_register", args);
        expect(authorized).not.toBeNull();
        return runWithMcpAuthenticatedPrincipal(authorized!.principal, () => harness.call(args));
      };
      // Commit then deliberately discard the first response: retry recovers it.
      await run(contextFor(first.secret));
      // Audit attribution can legitimately change across agent processes; it
      // is never part of receipt authorization scope.
      harness.setActor("rotated-agent");
      writeFileSync(join(companyPaths(companyRootForSlug(workspace, "allowed-aps")).config, "policy.yaml"), "actor_allowlist:\n  agents:\n    - agent:first-agent/1\n    - agent:rotated-agent/1\n");
      const replay = await run(contextFor(first.secret));
      expect(replay.ok).toBe(true); expect(replay.data?.idempotency).toMatchObject({ replayed: true });
      const rotated = await rotateWorkspaceServiceCredential(db, runtime.auth, { serviceAccountId: first.serviceAccountId, credentialId: first.credentialId, actor: "user:owner" });
      const rotatedReplay = await run(contextFor(rotated.secret));
      expect(rotatedReplay.data?.idempotency).toMatchObject({ replayed: true });
      // A distinct service account gets neither the original receipt nor the
      // business write: the document has already been registered.
      const otherAuthorized = await authorizeMcpTool(contextFor(second.secret), "payable_register", args);
      expect(otherAuthorized).not.toBeNull();
      const other = await runWithMcpAuthenticatedPrincipal(otherAuthorized!.principal, () => harness.call(args));
      expect(other.ok).toBe(false); expect(other.data?.idempotency).toBeUndefined();
      // Live membership revocation happens before receipt lookup, so replay is denied.
      revokeCompanyMembership(db, workspace, { userId: first.serviceAccountId, companySlug: "allowed-aps", actor: "user:owner" });
      expect(await authorizeMcpTool(contextFor(rotated.secret), "payable_register", args)).toBeNull();
      // The same key cannot reach an ungranted company or disclose the receipt.
      expect(await authorizeMcpTool(contextFor(second.secret), "payable_register", { ...args, company: "other-aps" })).toBeNull();
    } finally {
      if (previousWorkspace === undefined) delete process.env.RENTEMESTER_WORKSPACE;
      else process.env.RENTEMESTER_WORKSPACE = previousWorkspace;
      db.close(); runtime.close(); rmSync(workspace, { recursive: true, force: true });
    }
  });
});
