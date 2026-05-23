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

  test("all 82 tools expose an outputSchema in tools/list", () => {
    expect(tools.length).toBeGreaterThanOrEqual(82);
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

describe("#274 — intake tools expose the full DocumentMetadata schema", () => {
  let tools: any[];

  beforeAll(async () => {
    const response = await client.send("tools/list");
    tools = response.result?.tools ?? [];
  });

  function metadataSchemaOf(name: string) {
    const tool = tools.find((t) => t.name === name);
    expect(tool, `tool ${name} not found`).toBeDefined();
    return tool.inputSchema?.properties?.metadata as any;
  }

  // The named DocumentMetadata properties an external agent must be able to see.
  const EXPECTED_METADATA_PROPS = [
    "documentType",
    "issueDate",
    "invoiceNo",
    "deliveryDescription",
    "amountIncVat",
    "currency",
    "sender",
    "recipient",
    "vatAmount",
    "paymentDetails",
    "exemptionCode",
  ];

  for (const toolName of ["imap_intake_poll", "mail_intake_ingest"]) {
    test(`${toolName}.metadata is the unfolded DocumentMetadata, not an opaque object`, () => {
      const metadata = metadataSchemaOf(toolName);
      expect(metadata?.type, `${toolName}.metadata.type`).toBe("object");
      const props = metadata?.properties ?? {};
      // The old bare `type: object` catchall has zero named properties.
      expect(
        Object.keys(props).length,
        `${toolName}.metadata named properties`,
      ).toBeGreaterThanOrEqual(EXPECTED_METADATA_PROPS.length);
      for (const prop of EXPECTED_METADATA_PROPS) {
        expect(props[prop], `${toolName}.metadata.${prop}`).toBeDefined();
        expect(
          typeof props[prop]?.description,
          `${toolName}.metadata.${prop} description`,
        ).toBe("string");
      }
    });
  }

  test("the intake metadata schema matches documents_ingest.metadata", () => {
    // Reused definition: imap_intake_poll / mail_intake_ingest must carry the
    // SAME named properties as documents_ingest so they cannot drift apart.
    const ingestProps = Object.keys(
      metadataSchemaOf("documents_ingest")?.properties ?? {},
    ).sort();
    for (const toolName of ["imap_intake_poll", "mail_intake_ingest"]) {
      const props = Object.keys(metadataSchemaOf(toolName)?.properties ?? {}).sort();
      // The intake variants omit `source` (it is set by the pipeline), so they
      // must equal documents_ingest's properties minus `source`.
      const expected = ingestProps.filter((p) => p !== "source");
      expect(props, `${toolName}.metadata properties`).toEqual(expected);
    }
  });
});

describe("#275 — system_backup_destination_add documents kind + attestations", () => {
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

  test("kind is an enum exposing the five valid destination kinds", () => {
    const kind = schemaOf("system_backup_destination_add").properties?.kind;
    expect(Array.isArray(kind?.enum), "kind must be an enum").toBe(true);
    for (const value of ["local-folder", "dropbox", "google-drive", "ssh", "other"]) {
      expect(kind?.enum, `kind enum missing ${value}`).toContain(value);
    }
    expect(typeof kind?.description).toBe("string");
  });

  test("every attestation field carries a description", () => {
    const props = schemaOf("system_backup_destination_add").properties ?? {};
    for (const field of [
      "inEeaOrEu",
      "nonRelatedParty",
      "itSecurityMeetsStandards",
      "regionCountry",
      "attestedBy",
    ]) {
      expect(typeof props[field]?.description, `${field} description`).toBe("string");
      expect((props[field]?.description ?? "").length, `${field} description length`).toBeGreaterThan(
        10,
      );
    }
  });

  test("inEeaOrEu and attestedBy descriptions carry the human-attestation warning", () => {
    const props = schemaOf("system_backup_destination_add").properties ?? {};
    const inEea: string = (props.inEeaOrEu?.description ?? "").toLowerCase();
    expect(inEea).toContain("attest");
    // The legal anchor must be present.
    expect(inEea).toContain("205/2024");
    const attestedBy: string = (props.attestedBy?.description ?? "").toLowerCase();
    // It must be clear a human attests — Rentemester cannot know it itself.
    expect(attestedBy).toContain("human");
  });
});

describe("#276 — period_close warns that reported is irreversible", () => {
  test("period_close.status states reported cannot be reopened", async () => {
    const response = await client.send("tools/list");
    const tool = (response.result?.tools ?? []).find(
      (t: any) => t.name === "period_close",
    );
    expect(tool, "period_close not found").toBeDefined();
    const desc: string = (
      tool.inputSchema?.properties?.status?.description ?? ""
    ).toLowerCase();
    // The irreversibility of `reported` must be spelled out for the agent.
    expect(desc).toContain("irreversible");
    expect(desc).toContain("reopen");
  });
});

describe("#277 — older tools' flat scalar fields carry field descriptions", () => {
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

  test("system_export_authority — all six fields carry descriptions", () => {
    const props = schemaOf("system_export_authority").properties ?? {};
    for (const field of ["company", "from", "to", "out", "requestedAt", "requester"]) {
      expect(typeof props[field]?.description, `${field} description`).toBe("string");
      expect((props[field]?.description ?? "").length, `${field} description`).toBeGreaterThan(
        5,
      );
    }
    // `out` is an output path — the description must say so.
    expect((props.out?.description ?? "").toLowerCase()).toContain("output");
  });

  test("system_restore_backup.confirmText states the exact RESTORE formula", () => {
    const props = schemaOf("system_restore_backup").properties ?? {};
    const desc: string = props.confirmText?.description ?? "";
    expect(typeof desc).toBe("string");
    expect(desc).toContain("RESTORE <targetCompany>");
  });

  test("#306 — system_restore_backup.verifyKey is described as the symmetric HMAC key", () => {
    const props = schemaOf("system_restore_backup").properties ?? {};
    const desc: string = props.verifyKey?.description ?? "";
    expect(desc.toLowerCase()).toContain("hmac");
    expect(desc.toLowerCase()).toContain("symmetric");
    // It must NOT mislabel itself AS the ed25519 public key — the previous
    // (buggy) description said "an ed25519 public key used to verify ...".
    expect(desc.toLowerCase()).not.toContain("an ed25519 public key");
  });

  test("#306 — system_restore_backup exposes a publicKey (ed25519) field", () => {
    const props = schemaOf("system_restore_backup").properties ?? {};
    const desc: string = props.publicKey?.description ?? "";
    expect(typeof props.publicKey, "publicKey field must exist").toBe("object");
    expect(desc.toLowerCase()).toContain("ed25519");
  });

  test("#307 — system_restore_backup.confirmText is schema-optional", () => {
    // confirmText must NOT be in the required[] list, so an omitted
    // confirmText reaches the handler and gets the envelope (not -32602).
    const schema = schemaOf("system_restore_backup");
    const required: string[] = schema.required ?? [];
    expect(required).not.toContain("confirmText");
  });
});

describe("#294 — scalar-flag tools carry field descriptions and the documentId|invoiceNumber selector", () => {
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

  test("expense_book flat scalar fields carry descriptions", () => {
    const props = schemaOf("expense_book").properties ?? {};
    for (const field of [
      "documentId",
      "bankTransactionId",
      "expenseAccount",
      "paymentAccount",
      "date",
      "text",
    ]) {
      expect(typeof props[field]?.description, `expense_book.${field} description`).toBe(
        "string",
      );
      expect(
        (props[field]?.description ?? "").length,
        `expense_book.${field} description length`,
      ).toBeGreaterThan(10);
    }
    // date must state the YYYY-MM-DD format.
    expect(props.date?.description).toContain("YYYY-MM-DD");
    // paymentAccount must state the account-2000 default.
    expect(props.paymentAccount?.description).toContain("2000");
  });

  test("invoice_remind / journal_reverse flat scalar fields carry descriptions", () => {
    const remind = schemaOf("invoice_remind").properties ?? {};
    expect(remind.date?.description).toContain("YYYY-MM-DD");
    expect(typeof remind.fee?.description).toBe("string");
    expect(typeof remind.note?.description).toBe("string");

    const reverse = schemaOf("journal_reverse").properties ?? {};
    for (const field of ["entryId", "entryNo", "matchText", "date", "reason"]) {
      expect(typeof reverse[field]?.description, `journal_reverse.${field}`).toBe("string");
    }
    expect(reverse.date?.description).toContain("YYYY-MM-DD");
  });

  test("read tools bank_list / invoice_list / invoice_status / reconcile_bank carry field descriptions", () => {
    const bank = schemaOf("bank_list").properties ?? {};
    expect(typeof bank.status?.description).toBe("string");
    expect(bank.from?.description).toContain("YYYY-MM-DD");
    expect(bank.to?.description).toContain("YYYY-MM-DD");
    expect(typeof bank.account?.description).toBe("string");

    const invList = schemaOf("invoice_list").properties ?? {};
    expect(typeof invList.status?.description).toBe("string");
    expect(invList.from?.description).toContain("YYYY-MM-DD");

    const invStatus = schemaOf("invoice_status").properties ?? {};
    expect(invStatus.asOf?.description).toContain("YYYY-MM-DD");

    const recon = schemaOf("reconcile_bank").properties ?? {};
    expect(recon.from?.description).toContain("YYYY-MM-DD");
    expect(recon.to?.description).toContain("YYYY-MM-DD");
  });

  test("#387 — vat_report / vat_eu_sales_list / vat_oss_report from+to carry YYYY-MM-DD describes", () => {
    for (const name of ["vat_report", "vat_eu_sales_list", "vat_oss_report"]) {
      const props = schemaOf(name).properties ?? {};
      expect(props.from?.description, `${name}.from description`).toContain("YYYY-MM-DD");
      expect(props.to?.description, `${name}.to description`).toContain("YYYY-MM-DD");
      // The endpoint-inclusion convention must be visible so agents don't drift by one day.
      expect(
        (props.to?.description ?? "").toLowerCase(),
        `${name}.to states inclusion`,
      ).toMatch(/inclusiv|inclusive/);
    }
  });

  // The documentId|invoiceNumber selector must be visible in BOTH fields.
  const SELECTOR_TOOLS = [
    "invoice_status",
    "invoice_post",
    "invoice_render",
    "invoice_remind",
    "invoice_post_reminder",
    "invoice_claim_interest",
    "invoice_post_interest",
    "invoice_claim_compensation",
    "invoice_post_compensation",
    "invoice_interest_calc",
    "invoice_compensation_calc",
  ];

  for (const name of SELECTOR_TOOLS) {
    test(`${name}: documentId and invoiceNumber both describe the exactly-one selector rule`, () => {
      const props = schemaOf(name).properties ?? {};
      const docDesc: string = props.documentId?.description ?? "";
      const numDesc: string = props.invoiceNumber?.description ?? "";
      expect(docDesc.length, `${name}.documentId description`).toBeGreaterThan(10);
      expect(numDesc.length, `${name}.invoiceNumber description`).toBeGreaterThan(10);
      // The "provide exactly one" rule must be visible in BOTH fields.
      for (const [field, desc] of [
        ["documentId", docDesc],
        ["invoiceNumber", numDesc],
      ] as const) {
        const lc = desc.toLowerCase();
        expect(lc, `${name}.${field} mentions the alternative`).toContain("invoicenumber");
        expect(lc, `${name}.${field} mentions documentId`).toContain("documentid");
        expect(lc, `${name}.${field} states the one-of rule`).toMatch(
          /exactly one|provide (?:either|one)/,
        );
      }
    });
  }
});

describe("#295 — every write tool's description ends with a consistent write-class token", () => {
  let tools: any[];

  beforeAll(async () => {
    const response = await client.send("tools/list");
    tools = response.result?.tools ?? [];
  });

  test("no write tool's description ends with a bare 'write.'", () => {
    const offenders = tools
      .filter((t) => t.annotations?.readOnlyHint !== true)
      .filter((t) => /(^|[^-])\bwrite\.\s*$/.test((t.description ?? "").trim()))
      .map((t) => t.name);
    expect(offenders).toEqual([]);
  });

  test("every non-destructive write tool ENDS with write-reversible. or write-irreversible.", () => {
    // The class token must be the final sentence so it is reliably the last
    // thing an agent parses — not buried mid-description.
    const ENDS_WITH = /\b(write-reversible|write-irreversible)\.\s*$/;
    const missing: string[] = [];
    for (const tool of tools) {
      if (tool.annotations?.readOnlyHint === true) continue; // read tools
      if (tool.annotations?.destructiveHint === true) continue; // destructive class
      const desc: string = (tool.description ?? "").trim();
      if (!ENDS_WITH.test(desc)) missing.push(tool.name);
    }
    expect(missing).toEqual([]);
  });

  test("the class token appears exactly once per write tool description", () => {
    // No description may carry two conflicting class tokens.
    const offenders: string[] = [];
    for (const tool of tools) {
      if (tool.annotations?.readOnlyHint === true) continue;
      if (tool.annotations?.destructiveHint === true) continue;
      const matches = (tool.description ?? "").match(
        /\bwrite-(?:reversible|irreversible)\b/gi,
      );
      if ((matches?.length ?? 0) !== 1) offenders.push(tool.name);
    }
    expect(offenders).toEqual([]);
  });

  test("the previously-untokened backup + workspace tools carry an explicit class token", () => {
    const TOKEN = /\b(write-reversible|write-irreversible)\b/;
    for (const name of [
      "system_backup_archive",
      "system_backup_destination_add",
      "system_backup_destination_remove",
      "system_backup_place",
      "system_backup_confirm_placement",
      "system_backup_lock",
      "company_add",
    ]) {
      const tool = tools.find((t) => t.name === name);
      expect(tool, `tool ${name} not found`).toBeDefined();
      expect(TOKEN.test(tool.description ?? ""), `${name} has a write-class token`).toBe(
        true,
      );
    }
  });

  test("bank_import stays write-reversible and journal_post stays write-irreversible", () => {
    const bank = tools.find((t) => t.name === "bank_import");
    const journal = tools.find((t) => t.name === "journal_post");
    expect((bank?.description ?? "")).toContain("write-reversible");
    expect((journal?.description ?? "")).toContain("write-irreversible");
    // The two classes must be machine-distinguishable from the description.
    expect((journal?.description ?? "")).not.toContain("write-reversible");
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
