import { afterAll, beforeAll, describe, expect, test } from "bun:test";

const serverPath = new URL("../../src/mcp/server.ts", import.meta.url).pathname;
let proc: ReturnType<typeof Bun.spawn>;
let reader: ReadableStreamDefaultReader<Uint8Array>;
let buffer = "";
let nextId = 0;

type RpcResponse = {
  id?: number;
  result?: { instructions?: string; tools?: Array<{ name: string }>; structuredContent?: { ok?: boolean; data?: any }; isError?: boolean; content?: Array<{ text?: string }> };
  error?: { code?: number; message?: string };
};

async function call(method: string, params: Record<string, unknown> = {}): Promise<RpcResponse> {
  const id = ++nextId;
  await proc.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", id, method, params })}\n`);
  while (true) {
    const newline = buffer.indexOf("\n");
    if (newline < 0) {
      const next = await reader.read();
      if (next.done) throw new Error("compact MCP server closed before responding");
      buffer += new TextDecoder().decode(next.value, { stream: true });
      continue;
    }
    const message = JSON.parse(buffer.slice(0, newline)) as RpcResponse;
    buffer = buffer.slice(newline + 1);
    if (message.id === id) return message;
  }
}

beforeAll(() => {
  // Deliberately omit RENTEMESTER_MCP_PROFILE: compact is the public default.
  proc = Bun.spawn(["bun", serverPath], { stdin: "pipe", stdout: "pipe", stderr: "pipe", env: { ...process.env, RENTEMESTER_MCP_PROFILE: undefined } });
  reader = proc.stdout.getReader();
});

afterAll(async () => {
  try { proc.stdin.end(); } catch {}
  proc.kill();
  await proc.exited;
});

describe("compact MCP discovery black box (#647)", () => {
  test("initializes, discovers, describes and routes one operation through each gateway class", async () => {
    const initialized = await call("initialize", { protocolVersion: "2024-11-05", capabilities: {}, clientInfo: { name: "compact-discovery-client", version: "1" } });
    expect(initialized.error).toBeUndefined();
    expect(initialized.result?.instructions).toContain("system_server_about");
    await proc.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", method: "notifications/initialized" })}\n`);

    const listed = await call("tools/list");
    const expectedNames = [
      "system_server_about", "agent_capability_search", "agent_workflow_describe", "agent_operation_search",
      "agent_operation_describe", "agent_operation_read", "agent_operation_write", "agent_operation_destroy",
    ];
    expect((listed.result?.tools ?? []).map((tool) => tool.name)).toEqual(expectedNames);

    const about = await call("tools/call", { name: "system_server_about", arguments: {} });
    expect(about.result?.structuredContent?.data).toMatchObject({ profile: "compact", toolCount: 8 });

    const capabilities = await call("tools/call", { name: "agent_capability_search", arguments: { query: "bank", limit: 2 } });
    expect(capabilities.result?.structuredContent?.ok).toBe(true);

    const workflow = await call("tools/call", { name: "agent_workflow_describe", arguments: { id: "bank-reconciliation-batch" } });
    expect(workflow.result?.structuredContent?.ok).toBe(true);

    const search = await call("tools/call", { name: "agent_operation_search", arguments: { query: "invoice", limit: 3 } });
    const searchItems = search.result?.structuredContent?.data?.items ?? [];
    expect(searchItems.length).toBeGreaterThan(0);
    expect(searchItems.every((item: any) => item.available === true && item.directlyListed === false)).toBe(true);

    const described = await call("tools/call", { name: "agent_operation_describe", arguments: { operation: "sales_invoice_list" } });
    expect(described.result?.structuredContent?.data?.operation).toMatchObject({ available: true, directlyListed: false, canonicalName: "sales_invoice_list" });

    // Read/plan/status are all read-class routes; apply is intentionally sent
    // invalid input so schema validation proves the gateway boundary without
    // opening a company or mutating a ledger.
    for (const operation of ["system_server_about", "bookkeeping_batch_plan", "bookkeeping_batch_status"]) {
      const response = await call("tools/call", { name: "agent_operation_read", arguments: { operation, input: {} } });
      expect(response.error?.message ?? "").not.toContain("GATEWAY_CLASS_MISMATCH");
    }
    const apply = await call("tools/call", { name: "agent_operation_write", arguments: { operation: "bookkeeping_batch_apply", input: {} } });
    expect(apply.error?.message ?? "").not.toContain("GATEWAY_CLASS_MISMATCH");
    expect(apply.result?.isError).toBe(true);
    expect(apply.result?.content?.[0]?.text ?? "").toContain("Input validation");

    const listedAgain = await call("tools/list");
    expect((listedAgain.result?.tools ?? []).map((tool) => tool.name)).toEqual(expectedNames);
  });
});
