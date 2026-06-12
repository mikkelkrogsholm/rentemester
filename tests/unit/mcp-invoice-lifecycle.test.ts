// Tests: src/mcp/tools/invoice.ts — invoice lifecycle preconditions (#374)
//
// The adversarial-review finding (#374) is that MCP `invoice_*`-tools describe
// the isolated operation but not the required prior state. A solo `description`
// reader cannot know that `invoice_post` must run before `invoice_settle_bank`,
// and a precondition violation surfaces only as a downstream core error that
// doesn't name the missing prior tool.
//
// These tests pin the contract:
//   1. Each lifecycle-dependent write-tool's `description` (and CLI-surface
//      mapping in docs/mcp-tool-surface.md) names the required prior tool.
//   2. Calling a "must be posted" write-tool (settle, apply-payment, refund,
//      remind, claim-interest, claim-compensation, write-off-bad-debt) on an
//      issued-but-not-posted invoice returns an `ok:false` envelope whose
//      `errors` mention `invoice_post` — not just a downstream balance error.

import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { createCompany } from "../../src/core/company";
import { initWorkspace, companyRootForSlug } from "../../src/core/workspace";

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

let workspaceRoot: string;
let companyRoot: string;
let client: StdioMcpClient;
let toolsList: any[];

function issuePayload(): Record<string, unknown> {
  return {
    invoiceType: "full",
    issueDate: "2026-05-16",
    dueDate: "2026-06-15",
    deliveryDate: "2026-05-16",
    lines: [
      {
        description: "Konsulent maj",
        quantity: 1,
        unitPriceExVat: 1000,
        lineTotalExVat: 1000,
      },
    ],
    totals: {
      netAmount: 1000,
      vatRate: 0.25,
      vatAmount: 250,
      grossAmount: 1250,
    },
    seller: {
      name: "Acme ApS",
      address: "Testvej 1, 2100 København Ø",
      vatOrCvr: "DK12345678",
    },
    buyer: {
      name: "Kunde A/S",
      address: "Købervej 9, 8000 Aarhus C",
      vatOrCvr: "DK87654321",
    },
  };
}

beforeAll(async () => {
  workspaceRoot = mkdtempSync(join(tmpdir(), "mcp-lifecycle-ws-"));
  initWorkspace(workspaceRoot);
  const created = createCompany(workspaceRoot, { name: "Acme ApS" });
  companyRoot = companyRootForSlug(workspaceRoot, created.slug);

  client = new StdioMcpClient();
  const initResponse = await client.send("initialize", {
    protocolVersion: "2024-11-05",
    capabilities: {},
    clientInfo: { name: "rentemester-lifecycle-tests", version: "0.0.1" },
  });
  expect(initResponse.error).toBeUndefined();
  await client.notify("notifications/initialized");

  const list = await client.send("tools/list");
  toolsList = list.result?.tools ?? [];
});

afterAll(async () => {
  await client.close();
  if (workspaceRoot) rmSync(workspaceRoot, { recursive: true, force: true });
});

function descriptionOf(name: string): string {
  const tool = toolsList.find((t: any) => t.name === name);
  if (!tool) throw new Error(`tool ${name} not in tools/list`);
  return String(tool.description ?? "");
}

describe("#374 — invoice lifecycle preconditions are declared in tool descriptions", () => {
  // Hver lifecycle-afhængig write-tool skal i sin description nævne det
  // forrige tool/status agenten skal opnå før kaldet, så en agent der
  // udelukkende læser tools/list kan finde sekvensen issue → post →
  // settle/credit/refund uden at læse kildekoden.
  const cases: Array<{ tool: string; mentions: string }> = [
    { tool: "invoice_post", mentions: "invoice_issue" },
    { tool: "invoice_render", mentions: "invoice_issue" },
    { tool: "invoice_credit_note", mentions: "invoice_post" },
    { tool: "invoice_settle_bank", mentions: "invoice_post" },
    { tool: "invoice_settle_claim_bank", mentions: "invoice_post" },
    { tool: "invoice_apply_payment", mentions: "invoice_post" },
    { tool: "invoice_refund_bank", mentions: "invoice_post" },
    { tool: "invoice_write_off_bad_debt", mentions: "invoice_post" },
    { tool: "invoice_remind", mentions: "invoice_post" },
    { tool: "invoice_post_reminder", mentions: "invoice_remind" },
    { tool: "invoice_claim_interest", mentions: "invoice_post" },
    { tool: "invoice_post_interest", mentions: "invoice_claim_interest" },
    { tool: "invoice_claim_compensation", mentions: "invoice_post" },
    { tool: "invoice_post_compensation", mentions: "invoice_claim_compensation" },
  ];
  for (const { tool, mentions } of cases) {
    test(`${tool} description names "${mentions}" as prior step`, () => {
      const desc = descriptionOf(tool);
      expect(desc.toLowerCase()).toContain("forudsætning");
      expect(desc).toContain(mentions);
    });
  }
});

describe("#374 — docs/mcp-tool-surface.md has an Invoice lifecycle section", () => {
  test("the surface doc spells out issue → post → settle/credit/refund", () => {
    const docPath = new URL("../../docs/mcp-tool-surface.md", import.meta.url).pathname;
    const body = readFileSync(docPath, "utf8");
    expect(body).toContain("Invoice lifecycle");
    expect(body).toContain("invoice_issue");
    expect(body).toContain("invoice_post");
    expect(body).toContain("invoice_settle_bank");
    expect(body).toContain("invoice_credit_note");
  });
});

describe("#374 — calling a 'must be posted' write-tool on an unposted invoice names invoice_post", () => {
  let invoiceDocumentId: number;
  let invoiceNumber: string;

  beforeAll(async () => {
    const issued = await client.send("tools/call", {
      name: "invoice_issue",
      arguments: {
        company: companyRoot,
        payload: issuePayload(),
        confirm: true,
      },
    });
    const env = issued.result?.structuredContent;
    expect(env?.ok, JSON.stringify(env)).toBe(true);
    invoiceDocumentId = Number(env?.data?.documentId);
    invoiceNumber = String(env?.data?.invoiceNumber);
    expect(invoiceDocumentId).toBeGreaterThan(0);
  });

  const settleLikeCases: Array<{ tool: string; args: () => Record<string, unknown> }> = [
    {
      tool: "invoice_settle_bank",
      args: () => ({
        company: companyRoot,
        payload: {
          invoiceDocumentId,
          bankTransactionId: 1,
          amount: 1250,
          paymentDate: "2026-06-10",
        },
        confirm: true,
      }),
    },
    {
      tool: "invoice_apply_payment",
      args: () => ({
        company: companyRoot,
        payload: {
          invoiceDocumentId,
          paymentDate: "2026-06-10",
          amount: 1250,
        },
        confirm: true,
      }),
    },
    {
      tool: "invoice_remind",
      args: () => ({
        company: companyRoot,
        invoiceNumber,
        date: "2026-06-20",
        confirm: true,
      }),
    },
    {
      tool: "invoice_claim_interest",
      args: () => ({
        company: companyRoot,
        invoiceNumber,
        asOf: "2026-06-20",
        referenceRate: 2.65,
        confirm: true,
      }),
    },
    {
      tool: "invoice_claim_compensation",
      args: () => ({
        company: companyRoot,
        invoiceNumber,
        asOf: "2026-06-20",
        confirm: true,
      }),
    },
  ];

  for (const { tool, args } of settleLikeCases) {
    test(`${tool} on an unposted invoice tells the agent to call invoice_post first`, async () => {
      const response = await client.send("tools/call", { name: tool, arguments: args() });
      const env = response.result?.structuredContent;
      expect(env, `${tool}: expected envelope`).toBeDefined();
      expect(env?.ok).toBe(false);
      const joined = (env?.errors ?? []).join(" | ");
      expect(joined.toLowerCase(), `${tool}: error must explain precondition: ${joined}`).toContain(
        "invoice_post",
      );
    });
  }
});
