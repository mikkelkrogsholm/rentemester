/**
 * The authoritative internal MCP operation registry.
 *
 * Registrars still own their operation contracts.  This module only captures
 * the exact config and callback after the normal security/backup wrappers have
 * been applied.  The SDK is deliberately not inspected: registration is the
 * public boundary and the captured Zod values are retained verbatim.
 */

import { createHash } from "node:crypto";
import { z } from "zod";
import type { CallToolResult } from "@modelcontextprotocol/sdk/types.js";
import { MCP_TOOL_PERMISSIONS } from "./security";
import { retryClassForOperation, type OperationSafety, type RetryClass } from "../agent-discovery-catalog";
import {
  CONTROLLED_MCP_ACTIONS,
  MCP_OPERATION_NAMING,
  MCP_OPERATION_NAMING_ENTRIES,
  type McpOperationNaming,
} from "./operation-naming";

export { CONTROLLED_MCP_ACTIONS } from "./operation-naming";

export type McpOperationProfile = "compact" | "full";

export const COMPACT_MCP_TOOL_NAMES = Object.freeze([
  "system_server_about",
  "agent_capability_search",
  "agent_workflow_describe",
  "agent_operation_search",
  "agent_operation_describe",
  "agent_operation_read",
  "agent_operation_write",
  "agent_operation_destroy",
] as const);

export type McpToolAnnotations = {
  readOnlyHint?: boolean;
  destructiveHint?: boolean;
  idempotentHint?: boolean;
  openWorldHint?: boolean;
  [key: string]: unknown;
};

/** The shape accepted by McpServer.registerTool without depending on SDK
 * private fields or its internal RegisteredTool object. */
export type McpRegisterConfig = {
  title?: string;
  description?: string;
  inputSchema?: unknown;
  outputSchema?: unknown;
  annotations?: McpToolAnnotations;
  _meta?: Record<string, unknown>;
};

export type McpOperationCallback = (...args: unknown[]) => unknown;

export type CapturedMcpOperation = Readonly<{
  originalName: string;
  config: McpRegisterConfig;
  callback: McpOperationCallback;
}>;

export type OperationSchema = {
  input: Record<string, unknown>;
  output: Record<string, unknown> | null;
};

export type McpOperationMetadata = Readonly<{
  originalName: string;
  canonicalName: string;
  domain: string;
  resource: string;
  action: string;
  aliases: readonly string[];
  title: string | null;
  description: string;
  safety: OperationSafety;
  writeClass: "read" | "write-reversible" | "write-irreversible" | "destructive";
  retryClass: RetryClass;
  idempotent: boolean;
  requiresActor: boolean;
  requiresConfirmation: boolean;
  permission: string;
  invocationRoute: "agent_operation_read" | "agent_operation_write" | "agent_operation_destroy";
  schema: OperationSchema;
  schemaIdentityHash: string;
  identityHash: string;
  prerequisites: readonly string[];
  deprecatedAliasOf: string | null;
}>;

export type McpOperationRecord = Readonly<{
  metadata: McpOperationMetadata;
  original: CapturedMcpOperation;
}>;

export type McpOperationRegistry = Readonly<{
  profile: McpOperationProfile;
  operations: readonly McpOperationRecord[];
  byOriginalName: ReadonlyMap<string, McpOperationRecord>;
  byCanonicalName: ReadonlyMap<string, McpOperationRecord>;
  byAlias: ReadonlyMap<string, McpOperationRecord>;
  identityHash: string;
}>;

type AnySchema = {
  safeParseAsync?: (value: unknown) => Promise<{ success: boolean; data?: unknown; error?: unknown }>;
  safeParse?: (value: unknown) => { success: boolean; data?: unknown; error?: unknown };
};

const SNAKE_CASE = /^[a-z][a-z0-9]*(?:_[a-z0-9]+)+$/;


function stableJson(value: unknown): string {
  if (value === undefined) return "null";
  if (typeof value === "function") return "null";
  if (Array.isArray(value)) return `[${value.map(stableJson).join(",")}]`;
  if (value && typeof value === "object") {
    const record = value as Record<string, unknown>;
    return `{${Object.keys(record).sort().map((key) => `${JSON.stringify(key)}:${stableJson(record[key])}`).join(",")}}`;
  }
  return JSON.stringify(value);
}

function hash(value: unknown): string {
  return createHash("sha256").update(stableJson(value)).digest("hex");
}

function isZodSchema(value: unknown): value is AnySchema {
  return Boolean(value && typeof value === "object" && (typeof (value as AnySchema).safeParseAsync === "function" || typeof (value as AnySchema).safeParse === "function"));
}

function asZodSchema(value: unknown): AnySchema | null {
  if (value === undefined) return null;
  if (isZodSchema(value)) return value;
  if (value && typeof value === "object" && !Array.isArray(value)) return z.object(value as z.ZodRawShape);
  return null;
}

function schemaJson(value: unknown, operationName: string, io: "input" | "output"): Record<string, unknown> | null {
  if (value === undefined) return null;
  const schema = asZodSchema(value);
  if (!schema) throw new Error("MCP operation " + operationName + " " + io + " schema is not a supported Zod or object schema");
  try {
    // Input semantics are essential for Zod transforms: the SDK validates
    // caller input before executing the callback, so the catalogue must expose
    // the input side rather than attempting to represent transformed output.
    return z.toJSONSchema(schema as never, { io, target: "draft-7" }) as Record<string, unknown>;
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    throw new Error("MCP operation " + operationName + " " + io + " schema conversion failed: " + detail, { cause: error });
  }
}

export function operationSchemaIdentity(config: McpRegisterConfig, operationName = "unknown"): { schema: OperationSchema; hash: string } {
  const schema = {
    input: schemaJson(config.inputSchema, operationName, "input") ?? { type: "object", properties: {}, additionalProperties: false },
    output: schemaJson(config.outputSchema, operationName, "output"),
  } satisfies OperationSchema;
  return { schema, hash: hash(schema) };
}

function validateNamingMetadata(naming: McpOperationNaming): void {
  if (!naming.originalName || !SNAKE_CASE.test(naming.originalName)) throw new Error("invalid original MCP operation name: " + naming.originalName);
  if (!SNAKE_CASE.test(naming.canonicalName)) throw new Error("invalid canonical MCP operation name: " + naming.canonicalName);
  if (!/^[a-z][a-z0-9]*$/.test(naming.domain) || !/^[a-z][a-z0-9]*(?:_[a-z0-9]+)*$/.test(naming.resource) || !/^[a-z][a-z0-9]*$/.test(naming.action)) {
    throw new Error("invalid MCP operation naming metadata for " + naming.originalName);
  }
  if (naming.canonicalName !== naming.domain + "_" + naming.resource + "_" + naming.action) {
    throw new Error("MCP canonical name does not match explicit components: " + naming.originalName);
  }
  if (!CONTROLLED_MCP_ACTIONS.has(naming.action)) throw new Error("MCP operation has uncontrolled action '" + naming.action + "'");
  // Resource components are singular nouns.  The acronym-like "oss" token
  // is kept as part of "oss_filing" rather than treated as a plural resource.
  if (naming.resource.split("_").some((token) => token.endsWith("s") && token !== "oss")) {
    throw new Error("MCP operation resource must be singular: " + naming.originalName);
  }
}

function namingForOperation(originalName: string): McpOperationNaming {
  const naming = MCP_OPERATION_NAMING[originalName];
  if (!naming) throw new Error("MCP operation " + originalName + " lacks explicit canonical metadata");
  if (naming.originalName !== originalName) throw new Error("MCP naming metadata key mismatch for " + originalName);
  validateNamingMetadata(naming);
  return naming;
}

export function canonicalNameForOperation(originalName: string): string {
  return namingForOperation(originalName).canonicalName;
}

export function assertCanonicalOperationName(name: string): void {
  if (!SNAKE_CASE.test(name)) throw new Error("invalid canonical MCP operation name: " + name);
  const reviewed = MCP_OPERATION_NAMING_ENTRIES.filter((entry) => entry.canonicalName === name);
  if (reviewed.length > 0) {
    for (const entry of reviewed) validateNamingMetadata(entry);
    return;
  }
  throw new Error("canonical MCP operation lacks explicit naming metadata: " + name);
}

function safetyFor(config: McpRegisterConfig, operationName: string): OperationSafety {
  const annotations = config.annotations;
  if (!annotations || typeof annotations.readOnlyHint !== "boolean" || typeof annotations.destructiveHint !== "boolean" || typeof annotations.idempotentHint !== "boolean") {
    throw new Error("MCP operation " + operationName + " is missing explicit safety/idempotency annotations");
  }
  if (annotations.readOnlyHint && annotations.destructiveHint) throw new Error("MCP operation " + operationName + " cannot be both read-only and destructive");
  if (annotations.readOnlyHint) return "read";
  if (annotations.destructiveHint) return "destructive";
  return "write";
}

function writeClassFor(config: McpRegisterConfig, safety: OperationSafety, operationName: string): McpOperationMetadata["writeClass"] {
  if (safety === "read") return "read";
  const description = operationDescription(config);
  if (/\bwrite-irreversible\b/i.test(description)) return "write-irreversible";
  if (/\bwrite-reversible\b/i.test(description)) return "write-reversible";
  if (/\bdestructive\b/i.test(description)) return "destructive";
  throw new Error("MCP operation " + operationName + " is missing explicit write-class metadata");
}

function invocationRouteFor(safety: OperationSafety): McpOperationMetadata["invocationRoute"] {
  return safety === "read" ? "agent_operation_read" : safety === "destructive" ? "agent_operation_destroy" : "agent_operation_write";
}

function prerequisitesFor(config: McpRegisterConfig): string[] {
  const candidate = config._meta?.prerequisites;
  return Array.isArray(candidate) && candidate.every((item) => typeof item === "string") ? [...candidate] : [];
}

function operationDescription(config: McpRegisterConfig): string {
  return typeof config.description === "string" ? config.description : "";
}

function hasConfirmationField(config: McpRegisterConfig): boolean {
  const input = config.inputSchema;
  return Boolean(input && typeof input === "object" && !Array.isArray(input) && !isZodSchema(input) && Object.prototype.hasOwnProperty.call(input, "confirm"));
}

function requiresConfirmationFor(config: McpRegisterConfig, safety: OperationSafety, operationName: string): boolean {
  if (safety === "read") return false;
  if (config._meta?.requiresConfirmation === true || hasConfirmationField(config)) return true;
  throw new Error("MCP operation " + operationName + " is missing explicit confirmation metadata");
}

function validateNamingTable(captured: readonly CapturedMcpOperation[]): void {
  const tableNames = new Set<string>();
  for (const entry of MCP_OPERATION_NAMING_ENTRIES) {
    validateNamingMetadata(entry);
    if (tableNames.has(entry.originalName)) throw new Error("duplicate MCP naming metadata: " + entry.originalName);
    tableNames.add(entry.originalName);
  }
  const capturedNames = new Set(captured.map((operation) => operation.originalName));
  if (capturedNames.size !== captured.length) throw new Error("duplicate captured MCP operation names");
  for (const name of capturedNames) namingForOperation(name);
  for (const name of tableNames) if (!capturedNames.has(name)) throw new Error("MCP naming metadata has no captured operation: " + name);
}

type NamespaceOwner = { record: McpOperationRecord; kind: "original" | "canonical" | "alias" };

function addNamespaceName(namespace: Map<string, NamespaceOwner>, name: string, owner: NamespaceOwner): void {
  const previous = namespace.get(name);
  if (!previous) {
    namespace.set(name, owner);
    return;
  }
  if (previous.record !== owner.record) {
    throw new Error("MCP operation namespace collision: " + name + " (" + previous.record.original.originalName + " " + previous.kind + ", " + owner.record.original.originalName + " " + owner.kind + ")");
  }
  // A canonical may be the same spelling as its own original name, and a
  // compatibility alias may retain its own original spelling.  An alias
  // cannot, however, shadow its canonical spelling.
  if (owner.kind === "alias" && previous.kind === "canonical") {
    throw new Error("MCP alias-vs-canonical collision: " + name);
  }
}

export function validateMcpOperationRegistry(records: readonly McpOperationRecord[]): void {
  const originals = new Set<string>();
  const canonicalPrimaries = new Map<string, McpOperationRecord>();
  for (const record of records) {
    const originalName = record.original.originalName;
    if (originals.has(originalName)) throw new Error("duplicate MCP operation: " + originalName);
    originals.add(originalName);
    const canonical = record.metadata.canonicalName;
    assertCanonicalOperationName(canonical);
    if (record.metadata.safety !== "read" && record.metadata.safety !== "write" && record.metadata.safety !== "destructive") {
      throw new Error(originalName + ": missing safety classification");
    }
    const expectedRoute = invocationRouteFor(record.metadata.safety);
    if (record.metadata.invocationRoute !== expectedRoute) throw new Error(originalName + ": missing invocation route classification");
    if (typeof record.metadata.permission !== "string" || record.metadata.permission.length === 0) throw new Error(originalName + ": missing permission classification");
    if (typeof record.metadata.retryClass !== "string" || record.metadata.retryClass.length === 0) throw new Error(originalName + ": missing retry classification");
    if (typeof record.metadata.idempotent !== "boolean") throw new Error(originalName + ": missing idempotency classification");
    if (typeof record.metadata.requiresConfirmation !== "boolean" || (record.metadata.safety !== "read" && !record.metadata.requiresConfirmation)) {
      throw new Error(originalName + ": missing confirmation classification");
    }
    if (!record.metadata.schema || !record.metadata.schema.input || record.metadata.schema.input.type === "unknown") {
      throw new Error(originalName + ": missing exact input schema");
    }
    if (!Array.isArray(record.metadata.aliases)) throw new Error(originalName + ": aliases must be an array");
    if (record.metadata.deprecatedAliasOf) continue;
    const previous = canonicalPrimaries.get(canonical);
    if (previous) throw new Error("MCP canonical operation collision: " + canonical + " (" + previous.original.originalName + ", " + originalName + ")");
    canonicalPrimaries.set(canonical, record);
  }

  for (const record of records) {
    const deprecatedTarget = record.metadata.deprecatedAliasOf;
    if (!deprecatedTarget) continue;
    const target = canonicalPrimaries.get(deprecatedTarget);
    if (!target) throw new Error("MCP deprecated alias " + record.original.originalName + " points to missing canonical operation " + deprecatedTarget);
    if (record.metadata.canonicalName !== target.metadata.canonicalName) {
      throw new Error("MCP deprecated alias " + record.original.originalName + " does not resolve to target canonical identity");
    }
  }

  const namespace = new Map<string, NamespaceOwner>();
  for (const record of records) addNamespaceName(namespace, record.original.originalName, { record, kind: "original" });
  for (const record of records) {
    if (!record.metadata.deprecatedAliasOf) addNamespaceName(namespace, record.metadata.canonicalName, { record, kind: "canonical" });
  }

  const aliases = new Map<string, string>();
  for (const record of records) {
    for (const alias of record.metadata.aliases) {
      if (typeof alias !== "string" || !SNAKE_CASE.test(alias)) throw new Error("MCP alias is not lowercase snake_case: " + String(alias));
      if (aliases.has(alias)) throw new Error("MCP alias collision: " + alias);
      if (alias === record.metadata.canonicalName) throw new Error("MCP alias cycle: " + alias);
      addNamespaceName(namespace, alias, { record, kind: "alias" });
      aliases.set(alias, record.metadata.canonicalName);
    }
  }
  for (const [alias, canonical] of aliases) {
    if (!canonicalPrimaries.has(canonical)) throw new Error("MCP alias " + alias + " points to missing canonical operation " + canonical);
    if (aliases.has(canonical)) throw new Error("MCP alias cycle: " + alias + " -> " + canonical);
  }
}

function makeRecord(captured: CapturedMcpOperation): McpOperationRecord {
  const naming = namingForOperation(captured.originalName);
  const canonicalName = naming.canonicalName;
  const safety = safetyFor(captured.config, captured.originalName);
  const writeClass = writeClassFor(captured.config, safety, captured.originalName);
  const idempotent = captured.config.annotations!.idempotentHint === true;
  const schema = operationSchemaIdentity(captured.config, captured.originalName);
  const permission = MCP_TOOL_PERMISSIONS[captured.originalName];
  if (!permission) throw new Error("MCP operation " + captured.originalName + " has no permission classification");
  const retryClass = retryClassForOperation("mcp:" + captured.originalName, { safety, idempotent });
  if (!retryClass) throw new Error("MCP operation " + captured.originalName + " has no retry classification");
  const deprecatedAliasOf = captured.originalName === "bookkeeping_batch_dry_run" ? "bookkeeping_batch_persist" : null;
  const aliases = canonicalName === captured.originalName || deprecatedAliasOf ? [] : [captured.originalName];
  const metadataBase = {
    originalName: captured.originalName,
    canonicalName,
    domain: naming.domain,
    resource: naming.resource,
    action: naming.action,
    aliases,
    title: typeof captured.config.title === "string" ? captured.config.title : null,
    description: operationDescription(captured.config),
    safety,
    writeClass,
    retryClass,
    idempotent,
    requiresActor: safety !== "read",
    requiresConfirmation: requiresConfirmationFor(captured.config, safety, captured.originalName),
    permission,
    invocationRoute: invocationRouteFor(safety),
    schema: schema.schema,
    schemaIdentityHash: schema.hash,
    prerequisites: prerequisitesFor(captured.config),
    deprecatedAliasOf,
  } satisfies Omit<McpOperationMetadata, "identityHash">;
  const identityHash = hash(metadataBase);
  const metadata: McpOperationMetadata = Object.freeze({ ...metadataBase, identityHash });
  const record: McpOperationRecord = Object.freeze({ original: Object.freeze(captured), metadata });
  return record;
}

export function createMcpOperationRegistry(
  captured: readonly CapturedMcpOperation[],
  profile: McpOperationProfile = "compact",
): McpOperationRegistry {
  validateNamingTable(captured);
  const recordsByOriginal = new Map<string, McpOperationRecord>();
  const records = captured.map((operation) => makeRecord(operation));
  for (const record of records) recordsByOriginal.set(record.original.originalName, record);

  // The mutating legacy dry-run registration is a compatibility alias of the
  // persisted review-state implementation.  Keep its captured callback in the
  // registry for full-profile parity, but route canonical/alias gateways to
  // the canonical record.
  const canonicalRecords = new Map<string, McpOperationRecord>();
  for (const record of records) {
    if (record.metadata.deprecatedAliasOf) continue;
    canonicalRecords.set(record.metadata.canonicalName, record);
  }
  validateMcpOperationRegistry(records);

  const byAlias = new Map<string, McpOperationRecord>();
  for (const record of records) {
    const target = record.metadata.deprecatedAliasOf
      ? canonicalRecords.get(record.metadata.deprecatedAliasOf)
      : canonicalRecords.get(record.metadata.canonicalName);
    if (!target) throw new Error(`MCP operation ${record.original.originalName} has a dangling canonical target`);
    if (record.metadata.aliases.length > 0) {
      for (const alias of record.metadata.aliases) byAlias.set(alias, target);
    }
    // Every legacy original name is a compatibility alias when its canonical
    // spelling differs.  This does not rename permissions or persisted keys.
    if (record.original.originalName !== target.metadata.canonicalName) byAlias.set(record.original.originalName, target);
  }

  const byCanonicalName = new Map<string, McpOperationRecord>(canonicalRecords);
  const identityHash = hash(records.map((record) => ({
    originalName: record.metadata.originalName,
    metadataIdentityHash: record.metadata.identityHash,
  })).sort((a, b) => a.originalName.localeCompare(b.originalName)));
  return Object.freeze({
    profile,
    operations: Object.freeze(records),
    byOriginalName: recordsByOriginal,
    byCanonicalName,
    byAlias,
    identityHash,
  });
}

export function resolveMcpOperation(registry: McpOperationRegistry, name: string): McpOperationRecord | null {
  return registry.byCanonicalName.get(name) ?? registry.byAlias.get(name) ?? registry.byOriginalName.get(name) ?? null;
}

export async function parseMcpOperationInput(record: McpOperationRecord, input: unknown): Promise<{ success: true; data: unknown } | { success: false; error: unknown }> {
  const schema = asZodSchema(record.original.config.inputSchema);
  if (!schema) return { success: true, data: input ?? {} };
  if (typeof schema.safeParseAsync === "function") {
    const parsed = await schema.safeParseAsync(input ?? {});
    return parsed.success ? { success: true, data: parsed.data } : { success: false, error: parsed.error };
  }
  const parsed = schema.safeParse?.(input ?? {});
  return parsed?.success ? { success: true, data: parsed.data } : { success: false, error: parsed?.error };
}

/**
 * Preserve the SDK's public output-validation boundary for gateway calls.
 * Direct registrations are validated by the SDK after their handler returns;
 * a gateway invokes the captured handler itself, so it must apply the exact
 * captured output schema before returning the result to the gateway schema.
 */
export async function parseMcpOperationOutput(record: McpOperationRecord, result: unknown): Promise<{ success: true } | { success: false; error: unknown }> {
  const schema = asZodSchema(record.original.config.outputSchema);
  if (!schema || !result || typeof result !== "object") return { success: true };
  if (!("content" in result) || (result as { isError?: unknown }).isError === true) return { success: true };
  const structuredContent = (result as { structuredContent?: unknown }).structuredContent;
  if (!structuredContent) return { success: false, error: new Error("tool output has no structured content") };
  if (typeof schema.safeParseAsync === "function") {
    const parsed = await schema.safeParseAsync(structuredContent);
    return parsed.success ? { success: true } : { success: false, error: parsed.error };
  }
  const parsed = schema.safeParse?.(structuredContent);
  return parsed?.success ? { success: true } : { success: false, error: parsed?.error };
}

export function operationDescribePayload(record: McpOperationRecord, registry: McpOperationRegistry) {
  const target = record.metadata.deprecatedAliasOf ? resolveMcpOperation(registry, record.metadata.deprecatedAliasOf) : record;
  const metadata = target?.metadata ?? record.metadata;
  return {
    ...metadata,
    originalName: record.original.originalName,
    canonicalName: metadata.canonicalName,
    aliases: [...new Set([...(metadata.aliases ?? []), record.original.originalName].filter((name) => name !== metadata.canonicalName))],
    available: true,
    directlyListed: registry.profile === "full",
    deprecated: Boolean(record.metadata.deprecatedAliasOf),
    deprecatedAliasOf: record.metadata.deprecatedAliasOf,
  };
}

export function operationIdentityForLiveTool(record: McpOperationRecord, directlyListed: boolean) {
  return {
    name: record.original.originalName,
    canonicalName: record.metadata.canonicalName,
    aliases: record.metadata.aliases,
    available: true,
    directlyListed,
    invocationRoute: record.metadata.invocationRoute,
    schemaIdentityHash: record.metadata.schemaIdentityHash,
    operationIdentityHash: record.metadata.identityHash,
    annotations: record.original.config.annotations,
  };
}

export type GatewayResult = CallToolResult;
