// Tests: src/mcp/tools/vat.ts (vat_eu_sales_list, vat_oss_report MCP tools)
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, existsSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { seedAccounts, postJournalEntry } from "../../src/core/ledger";
import { ingestDocument } from "../../src/core/documents";

const SERVER_PATH = new URL("../../src/mcp/server.ts", import.meta.url).pathname;

type JsonRpcResponse = { jsonrpc: "2.0"; id?: number; result?: any; error?: { code: number; message: string } };

class StdioMcpClient {
  private proc: ReturnType<typeof Bun.spawn>;
  private stdoutReader: ReadableStreamDefaultReader<Uint8Array>;
  private decoder = new TextDecoder();
  private buffer = "";
  private nextId = 1;

  constructor() {
    this.proc = Bun.spawn(["bun", SERVER_PATH], { stdin: "pipe", stdout: "pipe", stderr: "pipe" });
    this.stdoutReader = this.proc.stdout.getReader();
  }

  async send(method: string, params?: Record<string, unknown>): Promise<JsonRpcResponse> {
    const id = this.nextId++;
    await this.proc.stdin.write(JSON.stringify({ jsonrpc: "2.0", id, method, params: params ?? {} }) + "\n");
    await (this.proc.stdin as any).flush?.();
    return this.readResponse(id);
  }

  async notify(method: string, params?: Record<string, unknown>): Promise<void> {
    await this.proc.stdin.write(JSON.stringify({ jsonrpc: "2.0", method, params: params ?? {} }) + "\n");
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
    try { this.proc.stdin.end(); } catch {}
    try { this.stdoutReader.releaseLock(); } catch {}
    this.proc.kill();
    await this.proc.exited;
  }
}

let companyRoot: string;
let client: StdioMcpClient;

beforeAll(async () => {
  companyRoot = mkdtempSync(join(tmpdir(), "mcp-vat-eulist-"));
  const inbox = mkdtempSync(join(tmpdir(), "mcp-vat-eulist-inbox-"));
  const paths = ensureCompanyDirs(companyRoot);
  const db = openDb(paths.db);
  migrate(db);
  seedAccounts(db);
  // An OSS consumer sale so vat_oss_report has real data to surface.
  const sourceFile = join(inbox, "oss.txt");
  await Bun.write(sourceFile, "Invoice\n2000 DKK\n");
  const doc = ingestDocument(db, companyRoot, sourceFile, {
    source: "email",
    issueDate: "2026-03-15",
    invoiceNo: "MCP-OSS-1",
    deliveryDescription: "Digital ydelse",
    amountIncVat: 2000,
    currency: "DKK",
    sender: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
    recipient: { name: "EU forbruger", address: "EU-vej 1", vatOrCvr: "DK99887766" },
    vatAmount: 0,
    paymentDetails: "Kort",
  });
  postJournalEntry(db, {
    transactionDate: "2026-03-12",
    text: "OSS salg",
    documentId: doc.documentId!,
    lines: [
      { accountNo: "2000", debitAmount: 2000 },
      { accountNo: "1000", creditAmount: 2000, vatCode: "OSS_EU_CONSUMER" },
    ],
  });
  db.close();
  rmSync(inbox, { recursive: true, force: true });

  client = new StdioMcpClient();
  const initResponse = await client.send("initialize", {
    protocolVersion: "2024-11-05",
    capabilities: {},
    clientInfo: { name: "rentemester-test-harness", version: "0.0.1" },
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

describe("vat_eu_sales_list MCP tool", () => {
  test("tools/list exposes vat_eu_sales_list and vat_oss_report", async () => {
    const response = await client.send("tools/list", {});
    const names = new Set((response.result?.tools ?? []).map((t: { name: string }) => t.name));
    expect(names.has("vat_eu_sales_list")).toBe(true);
    expect(names.has("vat_oss_report")).toBe(true);
  });

  test("vat_eu_sales_list returns an ok envelope on a company with no EU B2B sales", async () => {
    const response = await client.send("tools/call", {
      name: "vat_eu_sales_list",
      arguments: { company: companyRoot, from: "2026-01-01", to: "2026-03-31" },
    });
    const structured = response.result?.structuredContent;
    expect(structured?.ok).toBe(true);
    expect(structured?.data?.customers).toEqual([]);
    expect(structured?.data?.totalValue).toBe(0);
  });

  test("vat_oss_report surfaces the OSS consumer-sales base", async () => {
    const response = await client.send("tools/call", {
      name: "vat_oss_report",
      arguments: { company: companyRoot, from: "2026-01-01", to: "2026-03-31" },
    });
    const structured = response.result?.structuredContent;
    expect(structured?.ok).toBe(true);
    expect(structured?.data?.ossConsumerSalesBase).toBe(2000);
    expect(structured?.data?.submission).toBe(false);
  });
});
