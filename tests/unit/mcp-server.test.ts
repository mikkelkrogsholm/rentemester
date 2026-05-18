import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, existsSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { seedAccounts } from "../../src/core/ledger";

/**
 * Integration-test for MCP-server-scaffolden (#77).
 *
 * Spawner `bun src/mcp/server.ts` som child process, kører handshake
 * over stdio og verificerer at:
 *   1. `tools/list` returnerer både `audit_verify` og `journal_post`
 *   2. `tools/call audit_verify` på en fresh virksomhedsmappe svarer med
 *      envelope `{ ok: true, data: { entries: 0 }, errors: [] }`
 *
 * Bemærk: vi skriver MCP JSON-RPC manuelt på stdin/stdout for at
 * undgå at læne os op ad SDK-client-koden — vi tester at *serveren*
 * taler protokollen korrekt, ikke at SDK'en er konsistent med sig
 * selv.
 */

const SERVER_PATH = new URL("../../src/mcp/server.ts", import.meta.url).pathname;

type JsonRpcResponse = {
  jsonrpc: "2.0";
  id?: number;
  result?: any;
  error?: { code: number; message: string };
};

class StdioMcpClient {
  private proc: ReturnType<typeof Bun.spawn>;
  private stdoutReader: ReadableStreamDefaultReader<Uint8Array>;
  private decoder = new TextDecoder();
  private buffer = "";
  private nextId = 1;

  constructor() {
    this.proc = Bun.spawn(["bun", SERVER_PATH], {
      stdin: "pipe",
      stdout: "pipe",
      stderr: "pipe",
    });
    this.stdoutReader = this.proc.stdout.getReader();
  }

  async send(method: string, params?: Record<string, unknown>): Promise<JsonRpcResponse> {
    const id = this.nextId++;
    const request = { jsonrpc: "2.0", id, method, params: params ?? {} };
    await this.proc.stdin.write(JSON.stringify(request) + "\n");
    await (this.proc.stdin as any).flush?.();
    return this.readResponse(id);
  }

  async notify(method: string, params?: Record<string, unknown>): Promise<void> {
    const note = { jsonrpc: "2.0", method, params: params ?? {} };
    await this.proc.stdin.write(JSON.stringify(note) + "\n");
    await (this.proc.stdin as any).flush?.();
  }

  private async readResponse(expectedId: number): Promise<JsonRpcResponse> {
    const deadline = Date.now() + 10_000;
    while (Date.now() < deadline) {
      const newlineIdx = this.buffer.indexOf("\n");
      if (newlineIdx === -1) {
        const { value, done } = await this.stdoutReader.read();
        if (done) throw new Error("MCP server closed stdout before responding");
        this.buffer += this.decoder.decode(value, { stream: true });
        continue;
      }
      const line = this.buffer.slice(0, newlineIdx).trim();
      this.buffer = this.buffer.slice(newlineIdx + 1);
      if (!line) continue;
      const parsed: JsonRpcResponse = JSON.parse(line);
      if (parsed.id === expectedId) return parsed;
      // Anden notification/response — ignorér og fortsæt.
    }
    throw new Error(`Timed out waiting for MCP response id=${expectedId}`);
  }

  async close(): Promise<void> {
    try {
      this.proc.stdin.end();
    } catch {}
    try {
      this.stdoutReader.releaseLock();
    } catch {}
    this.proc.kill();
    await this.proc.exited;
  }
}

let companyRoot: string;
let client: StdioMcpClient;

beforeAll(async () => {
  companyRoot = mkdtempSync(join(tmpdir(), "mcp-test-company-"));
  const paths = ensureCompanyDirs(companyRoot);
  const db = openDb(paths.db);
  migrate(db);
  seedAccounts(db);
  db.close();

  client = new StdioMcpClient();
  const initResponse = await client.send("initialize", {
    protocolVersion: "2024-11-05",
    capabilities: {},
    clientInfo: { name: "rentemester-test-harness", version: "0.0.1" },
  });
  expect(initResponse.error).toBeUndefined();
  expect(initResponse.result?.serverInfo?.name).toBe("rentemester-mcp");
  await client.notify("notifications/initialized");
});

afterAll(async () => {
  await client.close();
  if (companyRoot && existsSync(companyRoot)) {
    rmSync(companyRoot, { recursive: true, force: true });
  }
});

describe("MCP server scaffold", () => {
  test("lists audit_verify and journal_post via tools/list", async () => {
    const response = await client.send("tools/list");
    expect(response.error).toBeUndefined();
    const names = (response.result?.tools ?? []).map((tool: any) => tool.name);
    expect(names).toContain("audit_verify");
    expect(names).toContain("journal_post");
  });

  test("audit_verify on a fresh company returns ok envelope with zero entries", async () => {
    const response = await client.send("tools/call", {
      name: "audit_verify",
      arguments: { company: companyRoot },
    });
    expect(response.error).toBeUndefined();
    const structured = response.result?.structuredContent;
    expect(structured).toBeDefined();
    expect(structured.ok).toBe(true);
    expect(structured.errors).toEqual([]);
    expect(structured.data?.entries).toBe(0);
    expect(response.result?.isError).toBe(false);
  });

  test("journal_post without confirm:true returns envelope error", async () => {
    const response = await client.send("tools/call", {
      name: "journal_post",
      arguments: {
        company: companyRoot,
        payload: {
          transactionDate: "2026-05-18",
          text: "Should be rejected",
          lines: [
            { accountNo: "2000", debitAmount: 100 },
            { accountNo: "1000", creditAmount: 100 },
          ],
        },
        confirm: false,
      },
    });
    expect(response.error).toBeUndefined();
    const structured = response.result?.structuredContent;
    expect(structured?.ok).toBe(false);
    expect(structured?.errors?.some((message: string) => message.includes("confirm: true required"))).toBe(true);
  });
});
