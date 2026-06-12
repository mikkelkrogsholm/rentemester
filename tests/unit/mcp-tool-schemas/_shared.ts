// Shared fixtures + helpers for the split mcp-tool-schemas tests.
// Originally lived at the top of tests/unit/mcp-tool-schemas.test.ts.
//
// Tests: src/mcp/tool-runtime.ts, src/mcp/tools (typed payload schemas + confirm envelope)
import { expect } from "bun:test";
import { mkdtempSync, rmSync, existsSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../../src/core/paths";
import { openDb, migrate } from "../../../src/core/db";
import { seedAccounts } from "../../../src/core/ledger";

/**
 * Coverage for #200/#201/#206/#208/#210:
 *
 *  - #201: an *omitted* `confirm` must yield the same structured
 *    `{ ok:false, errors:[...] }` envelope as `confirm:false` — NOT a raw
 *    JSON-RPC `-32602` error with no `structuredContent`.
 *  - #200/#206: the write tools expose fully-typed payload schemas — the
 *    `tools/list` inputSchema carries field-level descriptions (incl. amount
 *    units) and required/optional status.
 *  - #208: `journal_post`'s `payload.documentId` is documented as required
 *    for expense/income lines.
 *  - #210: `documents_ingest`'s `filePath` is documented as server-side.
 */

export type JsonRpcResponse = {
  jsonrpc: "2.0";
  id?: number;
  result?: any;
  error?: { code: number; message: string };
};

const SERVER_PATH = new URL("../../../src/mcp/server.ts", import.meta.url).pathname;

export class StdioMcpClient {
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

/**
 * Set up the per-file company tempdir + MCP client. Each split file calls
 * this from its own beforeAll; Bun runs each *.test.ts in its own process,
 * so the original single-file beforeAll cannot be shared across files.
 */
export async function startMcpFixture(): Promise<{
  companyRoot: string;
  client: StdioMcpClient;
}> {
  const companyRoot = mkdtempSync(join(tmpdir(), "mcp-schemas-company-"));
  const paths = ensureCompanyDirs(companyRoot);
  const db = openDb(paths.db);
  migrate(db);
  seedAccounts(db);
  db.close();

  const client = new StdioMcpClient();
  const initResponse = await client.send("initialize", {
    protocolVersion: "2024-11-05",
    capabilities: {},
    clientInfo: { name: "rentemester-schema-test", version: "0.0.1" },
  });
  expect(initResponse.error).toBeUndefined();
  await client.notify("notifications/initialized");

  return { companyRoot, client };
}

export async function stopMcpFixture(
  companyRoot: string,
  client: StdioMcpClient,
): Promise<void> {
  await client.close();
  if (companyRoot && existsSync(companyRoot)) {
    rmSync(companyRoot, { recursive: true, force: true });
  }
}
