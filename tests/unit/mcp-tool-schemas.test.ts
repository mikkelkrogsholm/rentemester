// Tests: src/mcp/tool-runtime.ts, src/mcp/tools (typed payload schemas + confirm envelope)
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, existsSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { seedAccounts } from "../../src/core/ledger";

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

type JsonRpcResponse = {
  jsonrpc: "2.0";
  id?: number;
  result?: any;
  error?: { code: number; message: string };
};

const SERVER_PATH = new URL("../../src/mcp/server.ts", import.meta.url).pathname;

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

let companyRoot: string;
let client: StdioMcpClient;

beforeAll(async () => {
  companyRoot = mkdtempSync(join(tmpdir(), "mcp-schemas-company-"));
  const paths = ensureCompanyDirs(companyRoot);
  const db = openDb(paths.db);
  migrate(db);
  seedAccounts(db);
  db.close();

  client = new StdioMcpClient();
  const initResponse = await client.send("initialize", {
    protocolVersion: "2024-11-05",
    capabilities: {},
    clientInfo: { name: "rentemester-schema-test", version: "0.0.1" },
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

describe("#201 — an omitted confirm yields an error envelope, not a raw -32602", () => {
  // The set of write tools whose confirm-gating must survive an omitted flag.
  const cases: Array<{ name: string; args: Record<string, unknown> }> = [
    {
      name: "journal_post",
      args: {
        company: "__COMPANY__",
        payload: {
          transactionDate: "2026-05-18",
          text: "confirm omitted",
          lines: [
            { accountNo: "2000", debitAmount: 100 },
            { accountNo: "5000", creditAmount: 100 },
          ],
        },
        // confirm intentionally omitted
      },
    },
    {
      name: "invoice_issue",
      args: {
        company: "__COMPANY__",
        payload: { invoiceType: "full", invoiceNumber: "X" },
        // confirm intentionally omitted
      },
    },
    {
      name: "period_close",
      args: {
        company: "__COMPANY__",
        from: "2026-05-01",
        to: "2026-05-31",
        // confirm intentionally omitted
      },
    },
  ];

  for (const { name, args } of cases) {
    test(`${name}: omitted confirm returns { ok:false, errors:[...] } envelope`, async () => {
      const resolved = JSON.parse(
        JSON.stringify(args).replace(/__COMPANY__/g, companyRoot),
      );
      const response = await client.send("tools/call", { name, arguments: resolved });
      // The whole point: no raw JSON-RPC error, a structured envelope instead.
      expect(response.error).toBeUndefined();
      const structured = response.result?.structuredContent;
      expect(structured).toBeDefined();
      expect(structured.ok).toBe(false);
      expect(Array.isArray(structured.errors)).toBe(true);
      expect(
        structured.errors.some((m: string) => m.includes("confirm: true required")),
      ).toBe(true);
    });
  }

  test("an omitted confirm produces the same envelope as confirm:false", async () => {
    const base = {
      company: companyRoot,
      payload: { invoiceType: "full" as const, invoiceNumber: "X" },
    };
    const omitted = await client.send("tools/call", {
      name: "invoice_issue",
      arguments: base,
    });
    const explicitFalse = await client.send("tools/call", {
      name: "invoice_issue",
      arguments: { ...base, confirm: false },
    });
    expect(omitted.result?.structuredContent).toEqual(
      explicitFalse.result?.structuredContent,
    );
  });
});

describe("#200/#206/#208/#210 — write tools expose fully-typed input schemas", () => {
  let tools: any[];

  beforeAll(async () => {
    const response = await client.send("tools/list");
    tools = response.result?.tools ?? [];
  });

  function schemaOf(name: string) {
    const tool = tools.find((t) => t.name === name);
    expect(tool, `tool ${name} not found`).toBeDefined();
    return tool.inputSchema as any;
  }

  test("invoice_issue payload is a typed object, not an empty catchall", () => {
    const schema = schemaOf("invoice_issue");
    const payload = schema.properties?.payload;
    expect(payload?.type).toBe("object");
    // A real contract: the discriminating invoiceType field is present.
    expect(payload?.properties?.invoiceType).toBeDefined();
    expect(payload?.properties?.totals).toBeDefined();
    // Field-level descriptions exist.
    expect(typeof payload?.properties?.invoiceType?.description).toBe("string");
  });

  test("#206 — invoice totals fields state the kroner unit", () => {
    const schema = schemaOf("invoice_issue");
    const totals = schema.properties?.payload?.properties?.totals;
    const grossDesc: string = totals?.properties?.grossAmount?.description ?? "";
    expect(grossDesc.toLowerCase()).toContain("kroner");
    const vatRateDesc: string = totals?.properties?.vatRate?.description ?? "";
    // vatRate is a fraction, not a monetary amount — must be documented as such.
    expect(vatRateDesc).toContain("fraction");
  });

  test("#206 — vat_post_eu_service_purchase netAmount states the kroner unit", () => {
    const schema = schemaOf("vat_post_eu_service_purchase");
    const desc: string = schema.properties?.payload?.properties?.netAmount?.description ?? "";
    expect(desc.toLowerCase()).toContain("kroner");
  });

  test("#206 — invoice_apply_payment amount states the kroner unit", () => {
    const schema = schemaOf("invoice_apply_payment");
    const desc: string = schema.properties?.payload?.properties?.amount?.description ?? "";
    expect(desc.toLowerCase()).toContain("kroner");
  });

  test("#208 — journal_post documentId description states the expense/income requirement", () => {
    const schema = schemaOf("journal_post");
    const desc: string = schema.properties?.payload?.properties?.documentId?.description ?? "";
    expect(desc.toLowerCase()).toContain("expense");
    expect(desc.toLowerCase()).toContain("income");
    expect(desc.toLowerCase()).toContain("required");
  });

  test("#210 — documents_ingest filePath is documented as server-side", () => {
    const schema = schemaOf("documents_ingest");
    const desc: string = schema.properties?.filePath?.description ?? "";
    expect(desc.toLowerCase()).toContain("server");
    // The tool description rules out an inline-content alternative.
    const tool = tools.find((t) => t.name === "documents_ingest");
    expect((tool.description ?? "").toLowerCase()).toContain("filepath");
  });

  test("#274 — mail intake metadata schemas are self-describing", () => {
    for (const name of ["mail_intake_ingest", "imap_intake_poll"]) {
      const schema = schemaOf(name);
      const metadata = schema.properties?.metadata;
      expect(metadata?.type, `${name}.metadata`).toBe("object");
      expect(metadata?.properties?.issueDate, `${name}.metadata.issueDate`).toBeDefined();
      expect(metadata?.properties?.amountIncVat, `${name}.metadata.amountIncVat`).toBeDefined();
      expect(metadata?.properties?.sender?.properties?.vatOrCvr, `${name}.metadata.sender.vatOrCvr`).toBeDefined();
      expect(metadata?.properties?.recipient?.properties?.vatOrCvr, `${name}.metadata.recipient.vatOrCvr`).toBeDefined();
      expect(metadata?.properties?.source, `${name}.metadata.source`).toBeUndefined();

      const perMessage = schema.properties?.metadataPerMessage;
      const nested = perMessage?.additionalProperties;
      expect(nested?.type, `${name}.metadataPerMessage.*`).toBe("object");
      expect(nested?.properties?.issueDate, `${name}.metadataPerMessage.*.issueDate`).toBeDefined();
      expect(nested?.properties?.source, `${name}.metadataPerMessage.*.source`).toBeUndefined();
    }
  });
});

describe("#232 — the remaining write tools carry field-level schemas", () => {
  let tools: any[];

  beforeAll(async () => {
    const response = await client.send("tools/list");
    tools = response.result?.tools ?? [];
  });

  function schemaOf(name: string) {
    const tool = tools.find((t) => t.name === name);
    expect(tool, `tool ${name} not found`).toBeDefined();
    return tool.inputSchema as any;
  }

  test("recurring_invoice_create.invoice is a typed object, not a schemaless catchall", () => {
    const schema = schemaOf("recurring_invoice_create");
    const invoice = schema.properties?.invoice;
    expect(invoice?.type).toBe("object");
    // A real contract: discriminating fields and a description are present.
    expect(invoice?.properties?.invoiceType).toBeDefined();
    expect(invoice?.properties?.totals).toBeDefined();
    expect(invoice?.properties?.lines).toBeDefined();
    expect(typeof invoice?.description).toBe("string");
    // It must not be the old fully-open record (no named properties).
    expect(Object.keys(invoice?.properties ?? {}).length).toBeGreaterThan(3);
  });

  test("customer_create / vendor_create input fields carry descriptions", () => {
    for (const name of ["customer_create", "vendor_create"]) {
      const input = schemaOf(name).properties?.input;
      expect(input?.type, `${name}.input`).toBe("object");
      expect(typeof input?.properties?.name?.description, `${name}.input.name`).toBe("string");
      expect(typeof input?.properties?.vatOrCvr?.description, `${name}.input.vatOrCvr`).toBe("string");
    }
  });

  test("mileage_log input fields document units and source-backed rate", () => {
    const input = schemaOf("mileage_log").properties?.input;
    const km: string = input?.properties?.kilometers?.description ?? "";
    expect(km.toLowerCase()).toContain("kilomet");
    const rate: string = input?.properties?.ratePerKm?.description ?? "";
    expect(rate.toLowerCase()).toContain("kroner");
    expect(typeof input?.properties?.rateBasis?.description).toBe("string");
  });

  test("asset_register / asset_write_off flat fields carry descriptions", () => {
    const reg = schemaOf("asset_register").properties ?? {};
    expect((reg.cost?.description ?? "").toLowerCase()).toContain("kroner");
    expect((reg.usefulLifeMonths?.description ?? "").toLowerCase()).toContain("month");
    const wo = schemaOf("asset_write_off").properties ?? {};
    expect(typeof wo.thresholdRuleSource?.description).toBe("string");
    expect(typeof wo.confirmImmediateWriteOff?.description).toBe("string");
  });

  test("period_close documents the closed/reported status semantics", () => {
    const status = schemaOf("period_close").properties?.status;
    const desc: string = status?.description ?? "";
    expect(desc).toContain("closed");
    expect(desc).toContain("reported");
    // The default must be stated.
    expect(desc.toLowerCase()).toContain("default");
  });

  test("company_add documents the workspace fallback when workspace is omitted", () => {
    const ws = schemaOf("company_add").properties?.workspace;
    const desc: string = ws?.description ?? "";
    expect(desc).toContain("RENTEMESTER_WORKSPACE");
    expect(desc.toLowerCase()).toContain("omitted");
  });

  test("invoice_send_email documents config/smtp.json required fields and dry-run", () => {
    const tool = tools.find((t) => t.name === "invoice_send_email");
    expect(tool, "invoice_send_email not found").toBeDefined();
    const desc: string = (tool.description ?? "").toLowerCase();
    expect(desc).toContain("smtp.json");
    // Required fields named.
    expect(desc).toContain("host");
    expect(desc).toContain("port");
    expect(desc).toContain("fromaddress");
    // Dry-run behaviour stated.
    expect(desc).toContain("dryrun");
  });

  test("customer_validate_vat read/write classification is documented as consistent", () => {
    const tool = tools.find((t) => t.name === "customer_validate_vat");
    expect(tool, "customer_validate_vat not found").toBeDefined();
    // It stays readOnlyHint:true — but the description must explain that
    // it writes a transparent cache, so CLI and MCP agree on the meaning.
    expect(tool.annotations?.readOnlyHint).toBe(true);
    const desc: string = (tool.description ?? "").toLowerCase();
    expect(desc).toContain("cache");
    expect(desc).toContain("validate-vat");
  });
});

describe("#202 — every tool declares the shared envelope outputSchema", () => {
  let tools: any[];

  beforeAll(async () => {
    const response = await client.send("tools/list");
    tools = response.result?.tools ?? [];
  });

  test("all 81 tools expose an outputSchema in tools/list", () => {
    expect(tools.length).toBeGreaterThanOrEqual(81);
    const missing = tools.filter((t) => !t.outputSchema).map((t) => t.name);
    expect(missing).toEqual([]);
  });

  test("the outputSchema is the shared { ok, data?, errors[], appliedRules? } envelope", () => {
    for (const tool of tools) {
      const schema = tool.outputSchema;
      expect(schema?.type, `tool ${tool.name} outputSchema`).toBe("object");
      const props = schema?.properties ?? {};
      // The machine-known envelope contract.
      expect(props.ok?.type, `${tool.name}.ok`).toBe("boolean");
      expect(props.errors?.type, `${tool.name}.errors`).toBe("array");
      expect(props.data?.type, `${tool.name}.data`).toBe("object");
      expect(props.appliedRules?.type, `${tool.name}.appliedRules`).toBe("array");
      // ok + errors are always present on the envelope.
      expect(schema?.required).toContain("ok");
      expect(schema?.required).toContain("errors");
    }
  });

  test("a success response's structuredContent validates against the outputSchema", async () => {
    // audit_verify is a read tool: a fresh company yields a clean ok envelope.
    const response = await client.send("tools/call", {
      name: "audit_verify",
      arguments: { company: companyRoot },
    });
    // With an outputSchema declared the SDK validates structuredContent on
    // success — a malformed envelope would come back as isError with no
    // structuredContent. Getting structuredContent back proves it validated.
    expect(response.error).toBeUndefined();
    expect(response.result?.isError).toBe(false);
    const structured = response.result?.structuredContent;
    expect(structured?.ok).toBe(true);
    expect(Array.isArray(structured?.errors)).toBe(true);
  });
});

describe("#204 — journal_post no longer advertises an unbacked idempotencyKey", () => {
  test("journal_post inputSchema does not contain idempotencyKey", async () => {
    const response = await client.send("tools/list");
    const tool = (response.result?.tools ?? []).find(
      (t: any) => t.name === "journal_post",
    );
    expect(tool, "journal_post not found").toBeDefined();
    const props = tool.inputSchema?.properties ?? {};
    // The field was documented as retry-safe but had no backing cache — #204
    // removed the false promise. It must not reappear in the schema.
    expect(props.idempotencyKey).toBeUndefined();
    expect(Object.keys(props).sort()).toEqual(["company", "confirm", "payload"]);
  });
});

describe("#238 — journal_post requires at least two lines", () => {
  let tools: any[];

  beforeAll(async () => {
    const response = await client.send("tools/list");
    tools = response.result?.tools ?? [];
  });

  test("journal_post lines schema declares minItems 2, matching the core", () => {
    // The core (src/core/ledger.ts) rejects any entry with fewer than two
    // lines. The MCP schema must advertise the same minimum so an agent
    // building from tools/list does not get its first posting rejected.
    const tool = tools.find((t) => t.name === "journal_post");
    expect(tool, "journal_post not found").toBeDefined();
    const lines = tool.inputSchema?.properties?.payload?.properties?.lines;
    expect(lines?.type).toBe("array");
    expect(lines?.minItems).toBe(2);
  });

  test("journal_post lines description states the debit-must-balance-credit rule", () => {
    const tool = tools.find((t) => t.name === "journal_post");
    const desc: string = (
      tool.inputSchema?.properties?.payload?.properties?.lines?.description ?? ""
    ).toLowerCase();
    expect(desc).toContain("debit");
    expect(desc).toContain("credit");
    // It must spell out the two-line minimum too.
    expect(desc).toContain("two");
  });

  test("a single-line journal_post payload is rejected before the handler", async () => {
    const response = await client.send("tools/call", {
      name: "journal_post",
      arguments: {
        company: companyRoot,
        payload: {
          transactionDate: "2026-05-18",
          text: "one line only",
          lines: [{ accountNo: "2000", debitAmount: 100 }],
        },
        confirm: true,
      },
    });
    // The min(2) schema makes the SDK reject this before the handler runs.
    const structured = response.result?.structuredContent;
    const failed =
      response.error !== undefined ||
      response.result?.isError === true ||
      structured?.ok === false;
    expect(failed).toBe(true);
  });
});

describe("#243 — previously-undescribed MCP tool fields now carry describe() text", () => {
  let tools: any[];

  beforeAll(async () => {
    const response = await client.send("tools/list");
    tools = response.result?.tools ?? [];
  });

  function schemaOf(name: string) {
    const tool = tools.find((t) => t.name === name);
    expect(tool, `tool ${name} not found`).toBeDefined();
    return tool.inputSchema as any;
  }

  test("bank_import.profile is an enum exposing the danske-bank value with a description", () => {
    const profile = schemaOf("bank_import").properties?.profile;
    // It must no longer be a bare string — the valid value must be discoverable.
    expect(Array.isArray(profile?.enum)).toBe(true);
    expect(profile?.enum).toContain("danske-bank");
    expect((profile?.description ?? "").toLowerCase()).toContain("danske-bank");
  });

  test("bank_import.account / csvPath / csvContent carry descriptions", () => {
    const props = schemaOf("bank_import").properties ?? {};
    expect(typeof props.account?.description).toBe("string");
    expect(typeof props.csvPath?.description).toBe("string");
    expect(typeof props.csvContent?.description).toBe("string");
  });

  test("expense_book.vatTreatment description explains each treatment", () => {
    const vt = schemaOf("expense_book").properties?.vatTreatment;
    const desc: string = (vt?.description ?? "").toLowerCase();
    expect(desc.length).toBeGreaterThan(20);
    // Each enum value should be explained.
    expect(desc).toContain("reverse_charge");
    expect(desc).toContain("representation");
    expect(desc).toContain("exempt");
  });

  test("documents_ingest force describes bypassing duplicate detection", () => {
    const force = schemaOf("documents_ingest").properties?.force;
    const desc: string = (force?.description ?? "").toLowerCase();
    expect(desc).toContain("duplicate");
    expect(typeof schemaOf("documents_ingest").properties?.vendorId?.description).toBe(
      "string",
    );
  });

  test("portfolio_overview.workspace documents the RENTEMESTER_WORKSPACE fallback", () => {
    const ws = schemaOf("portfolio_overview").properties?.workspace;
    const desc: string = ws?.description ?? "";
    expect(desc).toContain("RENTEMESTER_WORKSPACE");
    expect(desc.toLowerCase()).toContain("omitted");
  });
});

describe("#200 — typed schemas reject structurally invalid payloads", () => {
  test("invoice_issue rejects a payload missing the required invoiceType", async () => {
    // With the typed schema the SDK rejects this before the handler. The point
    // of #200 is that the contract is real — an agent that omits a required
    // field gets told so, instead of the call silently being accepted.
    const response = await client.send("tools/call", {
      name: "invoice_issue",
      arguments: {
        company: companyRoot,
        payload: { invoiceNumber: "X" },
        confirm: true,
      },
    });
    // The typed schema makes the SDK reject this: either a JSON-RPC error, an
    // isError result, or an error envelope — never a success.
    const structured = response.result?.structuredContent;
    const failed =
      response.error !== undefined ||
      response.result?.isError === true ||
      structured?.ok === false;
    expect(failed).toBe(true);
  });
});
