import { afterAll, beforeAll, describe, expect, test } from "bun:test";

const serverPath = new URL("../../src/mcp/server.ts", import.meta.url).pathname;
let proc: ReturnType<typeof Bun.spawn>;
let reader: ReadableStreamDefaultReader<Uint8Array>;
let buffer = "";
let id = 0;

type RpcResponse = { result?: { instructions?: string; tools?: LiveTool[]; structuredContent?: { ok: boolean; data?: Record<string, unknown> } }; error?: unknown };
type LiveTool = { name: string; annotations?: { readOnlyHint?: boolean; destructiveHint?: boolean; idempotentHint?: boolean } };
type DescribedWorkflow = {
  id: string;
  live: boolean;
  blockers: unknown[];
  recovery: string[];
  stopConditions: string[];
  unsupportedBoundaries: string[];
  steps: Array<{
    boundary: string;
    expectedSafety: string;
    expectedIdempotent: boolean;
    retryClass: string;
    operation: { surface: string; name?: string; resolved: boolean; safety?: string; idempotent?: boolean };
  }>;
};

async function call(method: string, params: Record<string, unknown> = {}): Promise<RpcResponse> {
  const requestId = ++id;
  await proc.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", id: requestId, method, params })}\n`);
  while (true) {
    const newline = buffer.indexOf("\n");
    if (newline < 0) {
      const next = await reader.read();
      if (next.done) throw new Error("MCP server closed before responding");
      buffer += new TextDecoder().decode(next.value, { stream: true });
      continue;
    }
    const message = JSON.parse(buffer.slice(0, newline)) as RpcResponse & { id?: number };
    buffer = buffer.slice(newline + 1);
    if (message.id === requestId) return message;
  }
}

beforeAll(async () => {
  proc = Bun.spawn(["bun", serverPath], { stdin: "pipe", stdout: "pipe", stderr: "pipe", env: { ...process.env, RENTEMESTER_MCP_PROFILE: "full" } });
  reader = proc.stdout.getReader();
});

afterAll(async () => {
  try { proc.stdin.end(); } catch {}
  proc.kill();
  await proc.exited;
});

describe("black-box runtime agent discovery (#584/#585)", () => {
  test("a clean client reaches canonical workflows and live operations without repository knowledge or mutation", async () => {
    const initialized = await call("initialize", { protocolVersion: "2024-11-05", capabilities: {}, clientInfo: { name: "synthetic-discovery-client", version: "1" } });
    expect(initialized.error).toBeUndefined();
    expect(initialized.result?.instructions).toContain("meta_about");
    expect(initialized.result?.instructions).toContain("agent_capability_search");
    await proc.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", method: "notifications/initialized" })}\n`);

    const listed = await call("tools/list");
    const tools = listed.result?.tools ?? [];
    const toolsByName = new Map(tools.map((tool) => [tool.name, tool]));

    const about = await call("tools/call", { name: "meta_about", arguments: {} });
    const aboutData = about.result?.structuredContent?.data as { build: { version: string }; ruleBundleVersion: string; catalogue: { schemaVersion: string; hash: string; entryPoint: string; coverage: { schemaVersion: string; rulesHash: string } } };
    expect(aboutData.build.version).toMatch(/^\d+\.\d+\.\d+/);
    expect(aboutData.ruleBundleVersion).not.toBe("");
    expect(aboutData.catalogue.schemaVersion).toBe("rentemester-agent-discovery-v1");
    expect(aboutData.catalogue.hash).toMatch(/^[a-f0-9]{64}$/);
    expect(aboutData.catalogue.entryPoint).toContain("system_server_about");
    expect(aboutData.catalogue.coverage.schemaVersion).toBe("rentemester-agent-discovery-coverage-v1");
    expect(aboutData.catalogue.coverage.rulesHash).toMatch(/^[a-f0-9]{64}$/);

    const catalogueSearch = await call("tools/call", {
      name: "agent_capability_search",
      arguments: { limit: 50 },
    });
    const capabilities = (catalogueSearch.result?.structuredContent?.data as
      | { items: Array<{ workflowIds: string[] }> }
      | undefined)?.items ?? [];
    const workflowIds = [...new Set(capabilities.flatMap((item) => item.workflowIds))];
    expect(workflowIds.length).toBeGreaterThan(10);

    const described = new Map<string, DescribedWorkflow>();
    const parityErrors: string[] = [];
    for (const workflowId of workflowIds) {
      const detail = await call("tools/call", {
        name: "agent_workflow_describe",
        arguments: { id: workflowId },
      });
      const workflow = (detail.result?.structuredContent?.data as
        | { workflow: DescribedWorkflow }
        | undefined)?.workflow;
      expect(workflow, `missing workflow ${workflowId}`).toBeDefined();
      expect(workflow!.live, `unresolved operation in ${workflowId}`).toBe(true);
      for (const workflowStep of workflow!.steps.filter((item) => item.operation.surface === "mcp")) {
        const live = toolsByName.get(workflowStep.operation.name!);
        expect(live, `missing live tool ${workflowStep.operation.name}`).toBeDefined();
        const liveSafety = live!.annotations?.readOnlyHint
          ? "read"
          : live!.annotations?.destructiveHint
            ? "destructive"
            : "write";
        expect(workflowStep.operation.resolved).toBe(true);
        expect(workflowStep.operation.safety).toBe(liveSafety);
        if (workflowStep.expectedSafety !== liveSafety) {
          parityErrors.push(
            `${workflowId}/${workflowStep.operation.name}: expectedSafety=${workflowStep.expectedSafety}, live=${liveSafety}`,
          );
        }
        expect(workflowStep.operation.idempotent).toBe(live!.annotations?.idempotentHint === true);
        if (workflowStep.expectedIdempotent !== (live!.annotations?.idempotentHint === true)) {
          parityErrors.push(
            `${workflowId}/${workflowStep.operation.name}: expectedIdempotent=${workflowStep.expectedIdempotent}, live=${live!.annotations?.idempotentHint === true}`,
          );
        }
      }
      described.set(workflowId, workflow!);
    }
    expect(parityErrors).toEqual([]);

    for (const scenario of [
      { query: "reconcile bank", workflowId: "bank-reconciliation-batch", boundaries: ["read", "dry-run", "irreversible"] },
      { query: "book supplier invoice", workflowId: "supplier-expense-booking", boundaries: ["dry-run", "irreversible"] },
      { query: "prepare VAT", workflowId: "vat-preparation", boundaries: ["read", "dry-run", "irreversible"] },
      { query: "restore backup", workflowId: "backup-health-audit", boundaries: ["read", "destructive"] },
    ]) {
      const search = await call("tools/call", { name: "agent_capability_search", arguments: { query: scenario.query, limit: 10 } });
      const searchData = search.result?.structuredContent?.data as {
        items: Array<{
          workflowIds: string[];
          operations: Array<{ id: string; safety: string; retryClass: string; requiresActor: boolean; requiresConfirmation: boolean }>;
        }>;
      };
      expect(searchData.items.some((item) => item.workflowIds.includes(scenario.workflowId))).toBe(true);
      expect(searchData.items.flatMap((item) => item.operations).length).toBeGreaterThan(0);

      const workflow = described.get(scenario.workflowId)!;
      expect(workflow.live).toBe(true);
      expect(workflow.blockers.length).toBeGreaterThan(0);
      expect(workflow.recovery.length).toBeGreaterThan(0);
      expect(workflow.stopConditions.length).toBeGreaterThan(0);
      for (const boundary of scenario.boundaries) expect(workflow.steps.some((item) => item.boundary === boundary)).toBe(true);
    }
  });
});
