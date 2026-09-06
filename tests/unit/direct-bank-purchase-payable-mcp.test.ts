// Black-box MCP schema/safety parity for #594. The accounting transition is
// covered by core tests; this file proves a client cannot bypass its transport
// contract over stdio.
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { migrate, openDb } from "../../src/core/db";
import { seedAccounts } from "../../src/core/ledger";

const serverPath = new URL("../../src/mcp/server.ts", import.meta.url).pathname;

class Client {
  private proc = Bun.spawn(["bun", serverPath], { stdin: "pipe", stdout: "pipe", stderr: "pipe", env: { ...process.env, RENTEMESTER_MCP_PROFILE: "full" } });
  private reader = this.proc.stdout.getReader();
  private decoder = new TextDecoder();
  private buffer = "";
  private id = 1;

  async call(method: string, params: Record<string, unknown> = {}) {
    const id = this.id++;
    await this.proc.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", id, method, params })}\n`);
    const deadline = Date.now() + 10_000;
    while (Date.now() < deadline) {
      const newline = this.buffer.indexOf("\n");
      if (newline < 0) {
        const chunk = await this.reader.read();
        if (chunk.done) throw new Error("MCP server closed before responding");
        this.buffer += this.decoder.decode(chunk.value, { stream: true });
        continue;
      }
      const line = this.buffer.slice(0, newline).trim();
      this.buffer = this.buffer.slice(newline + 1);
      if (!line) continue;
      const response = JSON.parse(line);
      if (response.id === id) return response;
    }
    throw new Error("MCP server timed out");
  }

  async close() {
    this.proc.stdin.end();
    this.proc.kill();
    await this.proc.exited;
  }
}

let company: string;
let client: Client;

beforeAll(async () => {
  company = mkdtempSync(join(tmpdir(), "rentemester-direct-payable-mcp-"));
  const paths = ensureCompanyDirs(company);
  const db = openDb(paths.db);
  migrate(db);
  seedAccounts(db);
  db.close();
  client = new Client();
  const initialized = await client.call("initialize", {
    protocolVersion: "2024-11-05", capabilities: {}, clientInfo: { name: "synthetic-test", version: "1" },
  });
  expect(initialized.error).toBeUndefined();
});

afterAll(async () => {
  await client.close();
  rmSync(company, { recursive: true, force: true });
});

describe("direct-bank purchase payable correction MCP surface (#594)", () => {
  test("publishes read/write annotations and exact required safety fields", async () => {
    const response = await client.call("tools/list");
    const tools = response.result.tools as Array<any>;
    const plan = tools.find((tool) => tool.name === "direct_bank_purchase_payable_correction_plan");
    const apply = tools.find((tool) => tool.name === "direct_bank_purchase_payable_correction_apply");
    expect(plan.annotations).toMatchObject({ readOnlyHint: true, idempotentHint: true });
    expect(apply.annotations).toMatchObject({ readOnlyHint: false, idempotentHint: false });
    expect(Object.keys(apply.inputSchema.properties)).toEqual(expect.arrayContaining([
      "company", "documentId", "bankTransactionId", "billDate", "dueDate", "expenseAccountNo", "planHash", "reason", "idempotencyKey", "confirm",
    ]));
  });

  test("apply fails schema validation before mutation without confirm or idempotency key", async () => {
    const base = {
      company, documentId: 1, bankTransactionId: 1, billDate: "2026-01-10", dueDate: "2026-01-10", expenseAccountNo: "3000", planHash: "0".repeat(64), reason: "synthetic review",
    };
    const noConfirm = await client.call("tools/call", { name: "direct_bank_purchase_payable_correction_apply", arguments: { ...base, idempotencyKey: "synthetic-key" } });
    expect(noConfirm.error).toBeUndefined();
    expect(noConfirm.result?.isError).toBe(true);
    expect(noConfirm.result?.structuredContent?.errors?.join(" ")).toContain("confirm");
    const noKey = await client.call("tools/call", { name: "direct_bank_purchase_payable_correction_apply", arguments: { ...base, confirm: true } });
    expect(noKey.error).toBeUndefined();
    expect(noKey.result?.isError).toBe(true);
  });
});
