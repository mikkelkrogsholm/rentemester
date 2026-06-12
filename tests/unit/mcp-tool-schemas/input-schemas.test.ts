// Tests: src/mcp/tool-runtime.ts, src/mcp/tools (typed payload schemas + confirm envelope)
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { startMcpFixture, stopMcpFixture, type StdioMcpClient } from "./_shared";

let companyRoot: string;
let client: StdioMcpClient;

beforeAll(async () => {
  ({ companyRoot, client } = await startMcpFixture());
});

afterAll(async () => {
  await stopMcpFixture(companyRoot, client);
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
