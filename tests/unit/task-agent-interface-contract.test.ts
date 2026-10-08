import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { InMemoryTransport } from "@modelcontextprotocol/sdk/inMemory.js";
import { TASK_MCP_NAMES, TASK_MUTATIONS, TASK_OPERATION_INPUTS } from "../../src/task-interface-input";
import { registerAllTools } from "../../src/mcp/registry";
import { taskSpecs } from "../../src/cli-meta/task-specs";
import { MUTATING_COMMANDS, mutationPolicyScope } from "../../src/cli-actor";
import { capabilityIdsForOperation, describeWorkflow } from "../../src/agent-discovery-catalog";

const root = new URL("../..", import.meta.url).pathname;
async function cli(args: string[], env: Record<string, string> = {}) {
  const proc = Bun.spawn([process.execPath, "src/cli.ts", ...args], { cwd: root, stdout: "pipe", stderr: "pipe", env: { ...process.env, RENTEMESTER_SERVICE_PRINCIPAL_TOKEN: "", RENTEMESTER_ACTOR: "", OPENCLAW_AGENT: "", RENTEMESTER_AGENT: "", RENTEMESTER_USER: "", USER: "", LOGNAME: "", USERNAME: "", ...env } });
  const [stdout, stderr, exit] = await Promise.all([new Response(proc.stdout).text(), new Response(proc.stderr).text(), proc.exited]);
  return { stdout, stderr, exit };
}

describe("task agent interfaces", () => {
  test("all operations have exact schemas, live permission metadata and correct mutation routing", () => {
    const registry = registerAllTools(new McpServer({ name: "task-contract", version: "1" }));
    const taskNames = Object.values(TASK_MCP_NAMES);
    expect(new Set(taskNames).size).toBe(20);
    expect(taskSpecs).toHaveLength(20);
    for (const [operation, name] of Object.entries(TASK_MCP_NAMES)) {
      const record = registry.byOriginalName.get(name)!;
      expect(record).toBeDefined();
      const mutation = TASK_MUTATIONS.has(operation as keyof typeof TASK_OPERATION_INPUTS);
      expect(record.metadata.safety).toBe(mutation ? "write" : "read");
      expect(record.metadata.requiresConfirmation).toBe(mutation);
      expect(record.metadata.retryClass).toBe(mutation ? "key-idempotent" : "safe-read");
      expect(record.metadata.permission).toBe("workspace.tasks.read");
      expect(record.metadata.schema.input.required).toContain("workspace");
      expect(record.metadata.schema.input.properties).not.toHaveProperty("principal");
      expect(record.metadata.schema.input.properties).not.toHaveProperty("userId");
      expect(capabilityIdsForOperation(`mcp:${name}`)).toContain("workspace-tasks");
      if (mutation) expect(record.metadata.schema.input.required).toContain("idempotencyKey");
    }
    const workflow = describeWorkflow("task-lifecycle", { tools: registry.operations.map(record => ({ name: record.original.originalName, annotations: record.original.config.annotations })) });
    expect(workflow?.workflow.live).toBe(true);
    for (const spec of taskSpecs) {
      expect(spec.allowedFlags).not.toContain("--company");
      if (MUTATING_COMMANDS.has(spec.key)) expect(mutationPolicyScope(spec.key)).toBe("handler");
    }
  });

  test("input cannot inject authority, completion assurance or protected source state", () => {
    const input = { title: "Review synthetic receipts", scope: { kind: "company", companySlug: "synthetic" }, idempotencyKey: "create-1" };
    expect(TASK_OPERATION_INPUTS.create.safeParse(input).success).toBe(true);
    for (const extra of [{ principal: "service-account:owner" }, { userId: "owner" }, { actor: "user:owner" }, { source: { state: "resolved" } }, { completion: { assurance: "product_verified" } }]) {
      expect(TASK_OPERATION_INPUTS.create.safeParse({ ...input, ...extra }).success).toBe(false);
    }
    expect(TASK_OPERATION_INPUTS.update.safeParse({ taskId: "task-1", patch: { status: "done" }, expectedVersion: 1, idempotencyKey: "edit-1" }).success).toBe(false);
    expect(TASK_OPERATION_INPUTS.complete.safeParse({ taskId: "task-1", outcome: "completed", note: "Done", expectedVersion: 1 }).success).toBe(false);
    expect(TASK_OPERATION_INPUTS.complete.safeParse({ taskId: "task-1", outcome: "completed", note: "Done", expectedVersion: 0, idempotencyKey: "complete-1" }).success).toBe(false);
  });

  test("CLI requires actor/key/explicit scope without default company or ledger access", async () => {
    const workspace = mkdtempSync(join(tmpdir(), "tasks-agent-cli-"));
    try {
      const file = join(workspace, "task.json");
      writeFileSync(file, JSON.stringify({ title: "Synthetic task", scope: { kind: "workspace", companySlugs: [] }, idempotencyKey: "create-1" }));
      const missingActor = await cli(["tasks", "create", "--workspace", workspace, "--input", file, "--json"]);
      expect(missingActor.exit).toBe(2);
      expect(missingActor.stderr).toContain("actor required for mutations");
      expect(missingActor.stderr).not.toContain("--company is required");
      writeFileSync(file, JSON.stringify({ title: "Synthetic task", idempotencyKey: "create-2" }));
      const missingScope = await cli(["tasks", "create", "--workspace", workspace, "--input", file, "--actor", "user:synthetic", "--json"]);
      expect(missingScope.exit).toBe(2);
      expect(missingScope.stderr).toContain("scope");
      const readInvalid = await cli(["tasks", "list", "--workspace", workspace, "--show-done", "yes", "--json"]);
      expect(readInvalid.exit).toBe(2);
      expect(readInvalid.stderr).toContain("true or false");
      const invalidWorkspace = await cli(["tasks", "list", "--workspace", "../invalid", "--json"]);
      expect(invalidWorkspace.exit).toBe(2);
      expect(invalidWorkspace.stderr).toContain("parent-directory");
      const unknownFlag = await cli(["tasks", "list", "--company", "synthetic", "--json"]);
      expect(unknownFlag.exit).toBe(2);
    } finally { rmSync(workspace, { recursive: true, force: true }); }
  });

  test("a forged audit actor cannot bypass a rejected service credential on direct or gateway calls", async () => {
    for (const profile of ["full", "compact"] as const) {
      const server = new McpServer({ name: "task-security", version: "1" });
      registerAllTools(server, { workspaceRoot: "/unopened-authenticated-workspace", verify: async () => null }, { profile });
      const [clientTransport, serverTransport] = InMemoryTransport.createLinkedPair();
      const client = new Client({ name: "synthetic", version: "1" });
      const connection = server.connect(serverTransport);
      await client.connect(clientTransport); await connection;
      try {
        const input = { workspace: "/unopened-authenticated-workspace", title: "Synthetic", scope: { kind: "workspace", companySlugs: [] }, idempotencyKey: "create-1", actor: "user:owner", confirm: true };
        const response = await client.callTool({ name: profile === "full" ? "tasks_create" : "agent_operation_write", arguments: profile === "full" ? input : { operation: "tasks_create", input } });
        expect(response.structuredContent).toMatchObject({ ok: false, code: "MCP_UNAUTHORIZED" });
        const read = await client.callTool({ name: profile === "full" ? "tasks_list" : "agent_operation_read", arguments: profile === "full" ? { workspace: input.workspace } : { operation: "tasks_list", input: { workspace: input.workspace } } });
        expect(read.structuredContent).toMatchObject({ ok: false, code: "MCP_UNAUTHORIZED" });
      } finally { await client.close(); await server.close(); }
    }
  });

  test("MCP full and compact writes preserve the confirmation envelope before touching the service", async () => {
    for (const profile of ["full", "compact"] as const) {
      const server = new McpServer({ name: "task-confirm", version: "1" });
      registerAllTools(server, undefined, { profile });
      const [clientTransport, serverTransport] = InMemoryTransport.createLinkedPair();
      const client = new Client({ name: "synthetic", version: "1" });
      const connection = server.connect(serverTransport);
      await client.connect(clientTransport); await connection;
      try {
        const input = { workspace: "/unopened-workspace", title: "Synthetic", scope: { kind: "workspace", companySlugs: [] }, idempotencyKey: "create-1" };
        const response = await client.callTool({ name: profile === "full" ? "tasks_create" : "agent_operation_write", arguments: profile === "full" ? input : { operation: "tasks_create", input } });
        expect(response.structuredContent).toMatchObject({ ok: false, code: "CONFIRM_REQUIRED" });
        expect(response.isError).toBe(true);
      } finally { await client.close(); await server.close(); }
    }
  });
});
