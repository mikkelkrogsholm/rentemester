// Tests: src/mcp/pagination.ts, src/mcp/tools/customer.ts, src/mcp/tools/journal.ts,
//        src/mcp/tools/documents.ts, src/mcp/tools/vendor.ts, src/mcp/tools/bank.ts
//
// #381 — MCP *_list-tools mangler limit/offset/cursor.
//
// Før denne fix returnerede flere list-tools hele tabellen i ét MCP-response
// uden default-cap og uden at agenten kunne vide om svaret var trunkeret.
// Denne test fastlåser den fælles pagination-kontrakt på tværs af alle
// fem berørte list-tools:
//
//   - inputSchema accepterer `limit` og `offset`
//   - envelope.data har `total`, `count`, `limit`, `offset`, `hasMore`
//     og (når `hasMore=true`) `nextOffset`
//   - `count` ≤ `limit`
//   - `hasMore` matcher virkeligheden (offset+count < total)
//   - sammenkædning af `nextOffset` → `offset` returnerer resten

import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, existsSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { seedAccounts } from "../../src/core/ledger";
import { createCustomer, createVendor } from "../../src/core/master-data";
import {
  applyPagination,
  DEFAULT_PAGE_LIMIT,
  MAX_PAGE_LIMIT,
} from "../../src/mcp/pagination";

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
    const deadline = Date.now() + 15_000;
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
    try { this.proc.stdin.end(); } catch {}
    try { this.stdoutReader.releaseLock(); } catch {}
    this.proc.kill();
    await this.proc.exited;
  }
}

let companyRoot: string;
let client: StdioMcpClient;

beforeAll(async () => {
  companyRoot = mkdtempSync(join(tmpdir(), "mcp-pagination-test-"));
  const paths = ensureCompanyDirs(companyRoot);
  const db = openDb(paths.db);
  migrate(db);
  seedAccounts(db);

  // Seed 7 customers + 4 vendors so we can prove that `count ≤ limit` and
  // `hasMore` matches reality across page boundaries.
  for (let i = 1; i <= 7; i++) {
    createCustomer(db, { name: `Customer ${String(i).padStart(2, "0")}` });
  }
  for (let i = 1; i <= 4; i++) {
    createVendor(db, { name: `Vendor ${String(i).padStart(2, "0")}` });
  }
  db.close();

  client = new StdioMcpClient();
  const initResponse = await client.send("initialize", {
    protocolVersion: "2024-11-05",
    capabilities: {},
    clientInfo: { name: "rentemester-pagination-test", version: "0.0.1" },
  });
  expect(initResponse.error).toBeUndefined();
  await client.notify("notifications/initialized");
});

afterAll(async () => {
  await client.close();
  if (companyRoot && existsSync(companyRoot)) {
    rmSync(companyRoot, { recursive: true, force: true });
  }
});

describe("applyPagination (#381) — helper unit tests", () => {
  test("defaults: limit=500, offset=0, hasMore=false when total ≤ limit", () => {
    const rows = Array.from({ length: 7 }, (_, i) => i);
    const { pageRows, meta } = applyPagination(rows, {});
    expect(pageRows.length).toBe(7);
    expect(meta.total).toBe(7);
    expect(meta.count).toBe(7);
    expect(meta.limit).toBe(DEFAULT_PAGE_LIMIT);
    expect(meta.offset).toBe(0);
    expect(meta.hasMore).toBe(false);
    expect(meta.nextOffset).toBeUndefined();
  });

  test("limit < total: hasMore=true and nextOffset=offset+count", () => {
    const rows = Array.from({ length: 7 }, (_, i) => i);
    const { pageRows, meta } = applyPagination(rows, { limit: 3 });
    expect(pageRows).toEqual([0, 1, 2]);
    expect(meta.count).toBe(3);
    expect(meta.limit).toBe(3);
    expect(meta.offset).toBe(0);
    expect(meta.hasMore).toBe(true);
    expect(meta.nextOffset).toBe(3);
  });

  test("walking pages via nextOffset returns the full set, no overlap", () => {
    const rows = Array.from({ length: 10 }, (_, i) => i);
    const seen: number[] = [];
    let offset = 0;
    let safety = 100;
    while (safety-- > 0) {
      const { pageRows, meta } = applyPagination(rows, { limit: 3, offset });
      seen.push(...pageRows);
      if (!meta.hasMore) break;
      offset = meta.nextOffset!;
    }
    expect(seen).toEqual(rows);
  });

  test("limit clamped to MAX_PAGE_LIMIT", () => {
    const rows = Array.from({ length: 10 }, (_, i) => i);
    const { meta } = applyPagination(rows, { limit: MAX_PAGE_LIMIT + 5_000 });
    expect(meta.limit).toBe(MAX_PAGE_LIMIT);
  });

  test("offset past total: count=0, hasMore=false", () => {
    const rows = Array.from({ length: 5 }, (_, i) => i);
    const { pageRows, meta } = applyPagination(rows, { offset: 100 });
    expect(pageRows).toEqual([]);
    expect(meta.count).toBe(0);
    expect(meta.total).toBe(5);
    expect(meta.hasMore).toBe(false);
  });
});

describe("#381 — MCP *_list tools all advertise the pagination contract", () => {
  const LIST_TOOLS = [
    "customer_list",
    "vendor_list",
    "documents_list",
    "journal_list",
    "bank_list",
  ];

  test("inputSchema for every list-tool exposes `limit` and `offset`", async () => {
    const response = await client.send("tools/list");
    expect(response.error).toBeUndefined();
    const tools = response.result?.tools ?? [];
    for (const name of LIST_TOOLS) {
      const tool = tools.find((t: any) => t.name === name);
      expect(tool, `tool ${name} must be registered`).toBeDefined();
      const props = tool.inputSchema?.properties ?? {};
      expect(props.limit, `${name} must expose limit`).toBeDefined();
      expect(props.offset, `${name} must expose offset`).toBeDefined();
    }
  });

  test("description for every list-tool documents default-limit and pagination", async () => {
    const response = await client.send("tools/list");
    const tools = response.result?.tools ?? [];
    for (const name of LIST_TOOLS) {
      const tool = tools.find((t: any) => t.name === name);
      const description: string = tool.description ?? "";
      expect(description, `${name} description must mention default page limit`).toContain(
        String(DEFAULT_PAGE_LIMIT),
      );
      expect(description.toLowerCase(), `${name} description must mention nextOffset/hasMore`)
        .toContain("nextoffset");
    }
  });
});

describe("#381 — customer_list pagination round-trip on a seeded company", () => {
  test("default call caps at DEFAULT_PAGE_LIMIT but returns full envelope metadata", async () => {
    const response = await client.send("tools/call", {
      name: "customer_list",
      arguments: { company: companyRoot },
    });
    const structured = response.result?.structuredContent;
    expect(structured?.ok).toBe(true);
    expect(structured?.data?.total).toBe(7);
    expect(structured?.data?.count).toBe(7);
    expect(structured?.data?.limit).toBe(DEFAULT_PAGE_LIMIT);
    expect(structured?.data?.offset).toBe(0);
    expect(structured?.data?.hasMore).toBe(false);
  });

  test("limit=3, offset=0 returns first page with hasMore=true, nextOffset=3", async () => {
    const response = await client.send("tools/call", {
      name: "customer_list",
      arguments: { company: companyRoot, limit: 3, offset: 0 },
    });
    const structured = response.result?.structuredContent;
    expect(structured?.ok).toBe(true);
    expect(structured?.data?.count).toBe(3);
    expect(structured?.data?.total).toBe(7);
    expect(structured?.data?.limit).toBe(3);
    expect(structured?.data?.offset).toBe(0);
    expect(structured?.data?.hasMore).toBe(true);
    expect(structured?.data?.nextOffset).toBe(3);
    expect(structured?.data?.rows?.length).toBe(3);
  });

  test("walking limit=3 pages via nextOffset retrieves all 7 customers exactly once", async () => {
    const seen = new Set<number>();
    let offset = 0;
    let lastHasMore = true;
    let pages = 0;
    while (pages++ < 10) {
      const response = await client.send("tools/call", {
        name: "customer_list",
        arguments: { company: companyRoot, limit: 3, offset },
      });
      const data = response.result?.structuredContent?.data;
      expect(data?.count).toBeLessThanOrEqual(3);
      for (const row of data?.rows ?? []) seen.add(row.id);
      lastHasMore = Boolean(data?.hasMore);
      if (!lastHasMore) break;
      offset = data.nextOffset;
    }
    expect(lastHasMore).toBe(false);
    expect(seen.size).toBe(7);
  });
});

describe("#381 — journal_list accepts the issue's required filters", () => {
  test("journal_list inputSchema exposes limit/offset/from/to/status", async () => {
    const response = await client.send("tools/list");
    const tools = response.result?.tools ?? [];
    const tool = tools.find((t: any) => t.name === "journal_list");
    expect(tool).toBeDefined();
    const props = tool.inputSchema?.properties ?? {};
    for (const field of ["limit", "offset", "from", "to", "status"]) {
      expect(props[field], `journal_list must accept ${field}`).toBeDefined();
    }
  });
});
