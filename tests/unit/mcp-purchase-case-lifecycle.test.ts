import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { appendFileSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { initWorkspace, companyRootForSlug } from "../../src/core/workspace";
import { createCompany } from "../../src/core/company";
import { companyPaths } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { openWorkspaceBetterAuth } from "../../src/server/better-auth";
import { createWorkspaceServicePrincipal } from "../../src/core/workspace-service-principals";
import { activateWorkspaceUser, grantCompanyMembership } from "../../src/core/workspace-access";
import { openWorkspaceControlDb } from "../../src/core/workspace-control";

const SERVER_PATH = new URL("../../src/mcp/server.ts", import.meta.url).pathname;
const AUTH_SECRET = "I0UjL6i0-ScgvjfIgzMKJxPQyDpPXwg2mMKdLW3Y3WQ";
type Rpc = { id?: number; result?: { structuredContent?: { ok?: boolean; data?: any } }; error?: unknown };

class Client {
  private readonly proc: ReturnType<typeof Bun.spawn>;
  private readonly reader: ReadableStreamDefaultReader<Uint8Array>;
  private stderr = "";
  private buffer = "";
  private id = 0;
  constructor(workspace: string, token: string) {
    this.proc = Bun.spawn(["bun", SERVER_PATH], { stdin: "pipe", stdout: "pipe", stderr: "pipe", env: { ...process.env, RENTEMESTER_WORKSPACE: workspace, RENTEMESTER_SERVICE_PRINCIPAL_TOKEN: token } });
    this.reader = this.proc.stdout.getReader();
    const errors = this.proc.stderr.getReader();
    void (async () => { for (;;) { const next = await errors.read(); if (next.done) return; this.stderr += new TextDecoder().decode(next.value, { stream: true }); } })();
  }
  async call(method: string, params: Record<string, unknown> = {}): Promise<Rpc> {
    const id = ++this.id;
    await this.proc.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", id, method, params })}\n`);
    while (true) {
      const newline = this.buffer.indexOf("\n");
      if (newline < 0) { const next = await this.reader.read(); if (next.done) throw new Error(`MCP server closed before response: ${this.stderr}`); this.buffer += new TextDecoder().decode(next.value, { stream: true }); continue; }
      const message = JSON.parse(this.buffer.slice(0, newline)) as Rpc;
      this.buffer = this.buffer.slice(newline + 1);
      if (message.id === id) return message;
    }
  }
  async initialize(name: string) {
    const response = await this.call("initialize", { protocolVersion: "2024-11-05", capabilities: {}, clientInfo: { name, version: "1" } });
    expect(response.error).toBeUndefined();
    await this.proc.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", method: "notifications/initialized" })}\n`);
  }
  async close() { try { this.proc.stdin.end(); } catch {} try { this.reader.releaseLock(); } catch {} this.proc.kill(); await this.proc.exited; }
}

let workspace = "";
let company = "";
let writer: Client;
let reviewer: Client;

beforeAll(async () => {
  workspace = mkdtempSync(join(tmpdir(), "rentemester-mcp-purchase-case-"));
  initWorkspace(workspace);
  const created = createCompany(workspace, { name: "Synthetic ApS", cvr: "DK90000001" });
  company = companyRootForSlug(workspace, created.slug);
  const ledger = openDb(companyPaths(company).db);
  try { migrate(ledger); ledger.run("INSERT INTO bank_transactions(transaction_date,amount,currency,text,transaction_hash) VALUES(?,?,?,?,?)", "2026-01-10", -125, "DKK", "Synthetic MCP purchase", "a".repeat(64)); }
  finally { ledger.close(); }
  appendFileSync(join(company, "config", "policy.yaml"), "  agents:\n    - agent:synthetic-purchase-writer/1\n    - agent:synthetic-purchase-reviewer/1\n");
  const runtime = openWorkspaceBetterAuth(workspace, { secret: AUTH_SECRET, trustedOrigins: ["http://127.0.0.1"], baseURL: "http://127.0.0.1" });
  const control = openWorkspaceControlDb(workspace);
  try {
    const writePrincipal = await createWorkspaceServicePrincipal(control, runtime.auth, { displayName: "Synthetic purchase writer", actor: "user:test" });
    const reviewPrincipal = await createWorkspaceServicePrincipal(control, runtime.auth, { displayName: "Synthetic purchase reviewer", actor: "user:test" });
    for (const principal of [writePrincipal, reviewPrincipal]) activateWorkspaceUser(control, { userId: principal.serviceAccountId, workspaceRole: "member", actor: "user:test" });
    grantCompanyMembership(control, workspace, { userId: writePrincipal.serviceAccountId, companySlug: created.slug, role: "bookkeeper", actor: "user:test" });
    grantCompanyMembership(control, workspace, { userId: reviewPrincipal.serviceAccountId, companySlug: created.slug, role: "reviewer", actor: "user:test" });
    writer = new Client(workspace, writePrincipal.secret); reviewer = new Client(workspace, reviewPrincipal.secret);
  } finally { control.close(); runtime.close(); }
  await writer.initialize("synthetic-purchase-writer");
  await reviewer.initialize("synthetic-purchase-reviewer");
});

afterAll(async () => { await writer?.close(); await reviewer?.close(); if (workspace) rmSync(workspace, { recursive: true, force: true }); });

describe("black-box MCP purchase-case lifecycle (#632)", () => {
  test("discovers, creates, independently reviews, and reads back one synthetic source-bound case", async () => {
    const about = await writer.call("tools/call", { name: "meta_about", arguments: {} });
    expect(about.result?.structuredContent?.ok).toBe(true);
    const discovery = await writer.call("tools/call", { name: "agent_capability_search", arguments: { query: "provisional purchase case", limit: 10 } });
    expect(discovery.result?.structuredContent?.data?.items.some((item: { workflowIds?: string[] }) => item.workflowIds?.includes("purchase-case-lifecycle"))).toBe(true);

    const created = await writer.call("tools/call", { name: "purchase_case_create", arguments: { company, caseId: "synthetic-mcp-case", source: { kind: "bank_transaction", id: 1 }, documentationOutcome: "unresolved", idempotencyKey: "synthetic-create-1", confirm: true } });
    expect(created.result?.structuredContent?.ok, JSON.stringify(created)).toBe(true);
    const initial = created.result?.structuredContent?.data?.purchaseCase;
    expect(initial).toMatchObject({ caseId: "synthetic-mcp-case", version: 1, accountingProgress: "unposted", documentationOutcome: "unresolved" });

    const review = await reviewer.call("tools/call", { name: "purchase_case_review", arguments: { company, caseId: initial.caseId, expectedVersion: initial.version, expectedSourceFingerprint: initial.sourceFingerprint, documentationOutcome: "ordinary_evidence_sufficient", idempotencyKey: "synthetic-review-1", confirm: true } });
    expect(review.result?.structuredContent?.ok, JSON.stringify(review)).toBe(true);
    const readback = await reviewer.call("tools/call", { name: "purchase_case_get", arguments: { company, caseId: initial.caseId } });
    expect(readback.result?.structuredContent?.data?.purchaseCase).toMatchObject({ caseId: initial.caseId, version: 2, documentationOutcome: "ordinary_evidence_sufficient", accountingProgress: "unposted" });
  });
});
