import { createHash } from "node:crypto";
import { describe, expect, test } from "bun:test";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { InMemoryTransport } from "@modelcontextprotocol/sdk/inMemory.js";
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import {
  COMPACT_MCP_TOOL_NAMES,
  createMcpOperationRegistry,
  operationSchemaIdentity,
  validateMcpOperationRegistry,
  type McpOperationRecord,
} from "../../src/mcp/operation-registry";
import { registerAllTools } from "../../src/mcp/registry";
import { resolveMcpProfile } from "../../src/mcp/server";

function stable(value: unknown): string {
  if (value === undefined) return "null";
  if (Array.isArray(value)) return `[${value.map(stable).join(",")}]`;
  if (value && typeof value === "object") {
    const object = value as Record<string, unknown>;
    return `{${Object.keys(object).sort().map((key) => `${JSON.stringify(key)}:${stable(object[key])}`).join(",")}}`;
  }
  return JSON.stringify(value);
}

function schemaHash(input: unknown, output: unknown): string {
  return createHash("sha256").update(stable({ input, output: output ?? null })).digest("hex");
}

async function connected(profile: "compact" | "full") {
  const server = new McpServer({ name: `registry-${profile}`, version: "1" });
  const registry = registerAllTools(server, undefined, { profile });
  const [clientTransport, serverTransport] = InMemoryTransport.createLinkedPair();
  const client = new Client({ name: "registry-test", version: "1" });
  const serverConnected = server.connect(serverTransport);
  await client.connect(clientTransport);
  await serverConnected;
  return { server, client, registry };
}

function cloneRecord(record: McpOperationRecord, metadata: Partial<McpOperationRecord["metadata"]>): McpOperationRecord {
  return { ...record, metadata: { ...record.metadata, ...metadata } } as McpOperationRecord;
}

describe("authoritative MCP operation registry (#647)", () => {
  test("profile selection is compact by default, explicit full, and fail-closed", () => {
    expect(resolveMcpProfile({})).toBe("compact");
    expect(resolveMcpProfile({ RENTEMESTER_MCP_PROFILE: "full" })).toBe("full");
    expect(() => resolveMcpProfile({ RENTEMESTER_MCP_PROFILE: "unknown" })).toThrow(/unknown MCP startup profile/);
  });

  test("full tools/list preserves every exact captured schema and identity hash", async () => {
    const { server, client, registry } = await connected("full");
    try {
      const listed = (await client.listTools()).tools;
      expect(listed).toHaveLength(registry.operations.length);
      for (const record of registry.operations) {
        const tool = listed.find((candidate) => candidate.name === record.original.originalName);
        expect(tool).toBeDefined();
        expect(tool?.inputSchema).toEqual(record.metadata.schema.input);
        expect(tool?.outputSchema ?? null).toEqual(record.metadata.schema.output);
        expect(schemaHash(tool?.inputSchema, tool?.outputSchema)).toBe(record.metadata.schemaIdentityHash);
        expect(operationSchemaIdentity(record.original.config, record.original.originalName).hash).toBe(record.metadata.schemaIdentityHash);
      }

      for (const originalName of ["corporate_record_ingest", "workspace_inbox_ingest"]) {
        const record = registry.byOriginalName.get(originalName);
        expect(record?.metadata.schema.input.type).toBe("object");
        expect(JSON.stringify(record?.metadata.schema)).not.toContain('"type":"unknown"');
      }
      expect(registry.byOriginalName.get("corporate_record_ingest")?.metadata.schemaIdentityHash)
        .not.toBe(registry.byOriginalName.get("workspace_inbox_ingest")?.metadata.schemaIdentityHash);
      const persist = registry.byOriginalName.get("bookkeeping_batch_persist")!;
      const dryRun = registry.byOriginalName.get("bookkeeping_batch_dry_run")!;
      expect(registry.byAlias.get("bookkeeping_batch_dry_run")).toBe(persist);
      expect(dryRun.metadata.deprecatedAliasOf).toBe(persist.metadata.canonicalName);
      expect(dryRun.metadata.retryClass).toBe(persist.metadata.retryClass);
    } finally {
      await client.close();
      await server.close();
    }
  });

  test("compact operation descriptions expose the immutable profile and exact schemas", async () => {
    const { server, client, registry } = await connected("compact");
    try {
      expect((await client.listTools()).tools.map((tool) => tool.name)).toEqual([...COMPACT_MCP_TOOL_NAMES]);
      for (const record of registry.operations) {
        const response = await client.callTool({ name: "agent_operation_describe", arguments: { operation: record.original.originalName } });
        const operation = (response.structuredContent as any)?.data?.operation;
        expect(operation).toBeDefined();
        expect(operation.available).toBe(true);
        expect(operation.directlyListed).toBe(false);
        expect(operation.schema).toEqual(record.metadata.schema);
        expect(operation.schemaIdentityHash).toBe(record.metadata.schemaIdentityHash);
      }
    } finally {
      await client.close();
      await server.close();
    }
  });

  test("registry construction fails closed for missing annotations and exact-schema conversion failures", () => {
    const base = registerAllTools(new McpServer({ name: "construction", version: "1" }), undefined, { profile: "full" });
    const captured = base.operations.map((record) => record.original);
    const missingAnnotations = {
      ...captured.find((operation) => operation.originalName === "meta_about")!,
      config: { ...captured.find((operation) => operation.originalName === "meta_about")!.config, annotations: undefined },
    };
    expect(() => createMcpOperationRegistry(captured.map((operation) => operation.originalName === "meta_about" ? missingAnnotations : operation))).toThrow(/safety\/idempotency annotations/);

    const outputTransform = {
      ...captured.find((operation) => operation.originalName === "meta_about")!,
      config: {
        ...captured.find((operation) => operation.originalName === "meta_about")!.config,
        outputSchema: z.object({ value: z.string().transform((value) => value.length) }),
      },
    };
    expect(() => createMcpOperationRegistry(captured.map((operation) => operation.originalName === "meta_about" ? outputTransform : operation))).toThrow(/schema conversion failed/);
  });

  test("the single operation namespace rejects collisions, cycles and dangling aliases", () => {
    const full = registerAllTools(new McpServer({ name: "namespace", version: "1" }), undefined, { profile: "full" });
    const about = full.byOriginalName.get("meta_about")!;
    const accounts = full.byOriginalName.get("accounts_list")!;
    expect(() => validateMcpOperationRegistry([about, accounts])).not.toThrow();

    const aliasVsOriginal = cloneRecord(about, { aliases: ["accounts_list"] });
    expect(() => validateMcpOperationRegistry([aliasVsOriginal, accounts])).toThrow(/namespace collision/);

    const aliasVsCanonical = cloneRecord(about, { aliases: ["accounting_account_list"] });
    expect(() => validateMcpOperationRegistry([aliasVsCanonical, accounts])).toThrow(/namespace collision|alias-vs-canonical/);

    const dangling = cloneRecord(about, { deprecatedAliasOf: "missing_canonical" });
    expect(() => validateMcpOperationRegistry([dangling])).toThrow(/missing canonical/);

    const cycle = cloneRecord(about, { aliases: [about.metadata.canonicalName] });
    expect(() => validateMcpOperationRegistry([cycle])).toThrow(/alias cycle/);
  });

  test("direct and gateway invocation preserve success, domain errors and schema failures", async () => {
    const full = await connected("full");
    const compact = await connected("compact");
    try {
      const directAbout = await full.client.callTool({ name: "meta_about", arguments: {} });
      const gatewayAbout = await compact.client.callTool({ name: "agent_operation_read", arguments: { operation: "system_server_about", input: {} } });
      expect(directAbout.structuredContent).toMatchObject({ ok: true, data: { serverName: "rentemester-mcp", serverVersion: "0.2.0" } });
      expect(gatewayAbout.structuredContent).toMatchObject({ ok: true, data: { serverName: "rentemester-mcp", serverVersion: "0.2.0" } });

      const directDomainError = await full.client.callTool({ name: "accounts_list", arguments: { company: "missing-company" } });
      const gatewayDomainError = await compact.client.callTool({ name: "agent_operation_read", arguments: { operation: "accounting_account_list", input: { company: "missing-company" } } });
      expect(directDomainError.structuredContent).toEqual(gatewayDomainError.structuredContent);
      expect(directDomainError.isError).toBe(true);

      const directInvalid = await full.client.callTool({ name: "accounts_list", arguments: { company: 123 } });
      const gatewayInvalid = await compact.client.callTool({ name: "agent_operation_read", arguments: { operation: "accounting_account_list", input: { company: 123 } } });
      expect(directInvalid.isError).toBe(true);
      expect(gatewayInvalid.isError).toBe(true);
      expect(directInvalid.content[0]?.type).toBe("text");
      expect(gatewayInvalid.content[0]?.type).toBe("text");
      expect((directInvalid.content[0] as { text: string }).text).toContain("Input validation");
      expect((gatewayInvalid.content[0] as { text: string }).text).toContain("Input validation");

      const mismatch = await compact.client.callTool({ name: "agent_operation_write", arguments: { operation: "accounting_account_list", input: { company: "missing-company" } } });
      expect(mismatch.isError).toBe(true);
      expect(mismatch.structuredContent).toMatchObject({ ok: false, code: "GATEWAY_CLASS_MISMATCH" });
    } finally {
      await full.client.close();
      await full.server.close();
      await compact.client.close();
      await compact.server.close();
    }
  });
});
