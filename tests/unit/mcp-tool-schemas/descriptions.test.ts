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
