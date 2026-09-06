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

/** Explicitly named namespaces prevent accidental underscore segmentation. */
const DOMAIN_BY_PREFIX: Readonly<Record<string, string>> = Object.freeze({
  accounts: "accounting",
  accrual: "accounting",
  asset: "accounting",
  audit: "system",
  bank: "banking",
  bookkeeping: "accounting",
  budget: "planning",
  cfo: "reporting",
  company: "workspace",
  corporate: "workspace",
  cvr: "workspace",
  customer: "sales",
  dimension: "accounting",
  documents: "documents",
  efaktura: "external",
  exception: "accounting",
  exceptions: "accounting",
  expense: "purchasing",
  gdpr: "privacy",
  import: "system",
  imap: "documents",
  intercompany: "ownership",
  invoice: "sales",
  journal: "accounting",
  legacy: "workspace",
  liquidity: "planning",
  mail: "documents",
  meta: "system",
  mileage: "expenses",
  ownership: "ownership",
  payable: "purchasing",
  party: "workspace",
  peppol: "external",
  period: "accounting",
  portfolio: "workspace",
  posting: "accounting",
  purchase: "purchasing",
  reconcile: "banking",
  recurring: "sales",
  retention: "system",
  supplier: "purchasing",
  system: "system",
  tax: "tax",
  vat: "tax",
  vendor: "purchasing",
  workspace: "workspace",
});

/** A controlled action vocabulary.  Compound resources (for example
 * imported_receivables) retain their spelling; only the final action token is
 * checked. */
export const CONTROLLED_MCP_ACTIONS: ReadonlySet<string> = Object.freeze(new Set<string>([
  "about", "actual", "add", "alerts", "apply", "approve", "archive", "assign", "audit", "authority", "backfill", "backup", "bank", "book", "calc", "change", "close", "compensation", "complete",
  "confirm", "context", "correction", "coverage", "create", "cvr", "debt", "depreciate", "describe", "destroy", "discover", "download", "dry", "email", "enrich", "export", "extract", "extraction", "evidence", "explain", "filing", "find", "forecast", "generate", "get", "governance", "healthcheck", "history", "import", "ingest", "inspect",
  "interest", "invoice", "issue", "konfigurer", "lifecycle", "link", "list", "lock", "log", "lookup", "match", "matches", "merge", "modtag", "note", "off", "onboard", "overdue", "overview", "parse", "party", "pay", "payment", "pending", "persist", "place", "placement", "plan", "poll", "post", "preflight", "prepare", "propose", "purchase", "query", "read", "readiness", "reassess", "receivables", "recognize", "record", "register", "registrer", "remind", "reminder", "remove", "render", "report", "resolve", "reverse", "review", "reopen", "replace", "restore", "roles", "role", "run", "search", "send", "set", "settle", "status", "reconcile",
  "supersede", "suggest", "text", "update", "validate", "vat", "verify", "vs", "workbench", "workspace", "write", "week", "year", "execute",
]));

const CANONICAL_OVERRIDES: Readonly<Record<string, string>> = Object.freeze({
  meta_about: "system_server_about",
  company_add: "workspace_company_add",
  cvr_lookup: "workspace_vat_lookup",
  accounts_list: "accounting_account_list",
  accounts_add: "accounting_account_add",
  accounts_roles_status: "accounting_account_roles_status",
  reconcile_bank: "banking_bank_reconcile",
  bookkeeping_batch_dry_run: "bookkeeping_batch_persist",
});

const SINGULAR_RESOURCES: Readonly<Record<string, string>> = Object.freeze({
  accounts: "account",
  companies: "company",
  customers: "customer",
  documents: "document",
  dimensions: "dimension",
  exceptions: "exception",
  invoices: "invoice",
  parties: "party",
  vendors: "vendor",
  workspaces: "workspace",
});

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

function schemaJson(value: unknown): Record<string, unknown> | null {
  const schema = asZodSchema(value);
  if (!schema) return null;
  try {
    // z.toJSONSchema is the public Zod v4 conversion API.  It preserves the
    // exact captured schema semantics without inspecting MCP SDK internals.
    return z.toJSONSchema(schema as never) as Record<string, unknown>;
  } catch {
    return { type: "unknown", schemaType: value?.constructor?.name ?? "unknown" };
  }
}

export function operationSchemaIdentity(config: McpRegisterConfig): { schema: OperationSchema; hash: string } {
  const schema = {
    input: schemaJson(config.inputSchema) ?? { type: "object", properties: {}, additionalProperties: false },
    output: schemaJson(config.outputSchema),
  } satisfies OperationSchema;
  return { schema, hash: hash(schema) };
}

export function canonicalNameForOperation(originalName: string): string {
  if (CANONICAL_OVERRIDES[originalName]) return CANONICAL_OVERRIDES[originalName]!;
  if (!SNAKE_CASE.test(originalName)) throw new Error(`MCP operation ${originalName} is not lowercase snake_case`);

  // Three-or-more tokens already carry explicit domain/resource/action
  // boundaries in the source registration.  No underscore splitting or
  // semantic guessing is performed here.  The only normalization below is
  // for a known plural resource prefix whose source operation has only two
  // tokens (for example documents_list -> documents_document_list).
  if (originalName.indexOf("_") !== originalName.lastIndexOf("_")) return originalName;

  for (const [prefix, domain] of Object.entries(DOMAIN_BY_PREFIX)) {
    if (originalName.startsWith(`${prefix}_`)) {
      const resource = SINGULAR_RESOURCES[prefix] ?? prefix;
      const action = originalName.slice(prefix.length + 1);
      return `${domain}_${resource}_${action}`;
    }
  }

  // Do not invent a domain/resource split for an unreviewed two-token name.
  // Adding a new operation requires an explicit mapping above; startup then
  // fails closed instead of silently publishing an ambiguous identity.
  throw new Error(`MCP operation ${originalName} lacks explicit canonical metadata`);
}

function safetyFor(config: McpRegisterConfig): OperationSafety {
  if (config.annotations?.readOnlyHint === true) return "read";
  if (config.annotations?.destructiveHint === true) return "destructive";
  return "write";
}

function writeClassFor(config: McpRegisterConfig, safety: OperationSafety): McpOperationMetadata["writeClass"] {
  if (safety === "read") return "read";
  if (safety === "destructive") return "destructive";
  const description = operationDescription(config);
  return /\bwrite-irreversible\b/i.test(description) ? "write-irreversible" : "write-reversible";
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

function canonicalComponents(canonicalName: string): { domain: string; resource: string; action: string } {
  const first = canonicalName.indexOf("_");
  const last = canonicalName.lastIndexOf("_");
  const domain = canonicalName.slice(0, first);
  const rawResource = canonicalName.slice(first + 1, last);
  return { domain, resource: SINGULAR_RESOURCES[rawResource] ?? rawResource, action: canonicalName.slice(last + 1) };
}

function operationPrimaryRecord(record: McpOperationRecord, recordsByCanonical: Map<string, McpOperationRecord>): McpOperationRecord {
  const canonical = record.metadata.canonicalName;
  return recordsByCanonical.get(canonical) ?? record;
}

export function assertCanonicalOperationName(name: string): void {
  if (!SNAKE_CASE.test(name)) throw new Error(`invalid canonical MCP operation name: ${name}`);
  const tokens = name.split("_");
  if (tokens.length < 3) throw new Error(`canonical MCP operation must have domain/resource/action tokens: ${name}`);
  const action = tokens[tokens.length - 1]!;
  if (!CONTROLLED_MCP_ACTIONS.has(action)) throw new Error(`canonical MCP operation has uncontrolled action '${action}': ${name}`);
}

export function validateMcpOperationRegistry(records: readonly McpOperationRecord[]): void {
  const originals = new Set<string>();
  const canonicalPrimaries = new Map<string, McpOperationRecord>();
  for (const record of records) {
    if (originals.has(record.original.originalName)) throw new Error(`duplicate MCP operation: ${record.original.originalName}`);
    originals.add(record.original.originalName);
    const canonical = record.metadata.canonicalName;
    assertCanonicalOperationName(canonical);
    if (record.metadata.safety !== "read" && record.metadata.safety !== "write" && record.metadata.safety !== "destructive") {
      throw new Error(`${record.original.originalName}: missing safety classification`);
    }
    if (!record.metadata.invocationRoute || !record.metadata.invocationRoute.startsWith("agent_operation_")) {
      throw new Error(`${record.original.originalName}: missing invocation route classification`);
    }
    if (typeof record.metadata.permission !== "string" || record.metadata.permission.length === 0) throw new Error(`${record.original.originalName}: missing permission classification`);
    if (typeof record.metadata.retryClass !== "string" || record.metadata.retryClass.length === 0) throw new Error(`${record.original.originalName}: missing retry classification`);
    if (!Array.isArray(record.metadata.aliases)) throw new Error(`${record.original.originalName}: aliases must be an array`);
    if (record.metadata.deprecatedAliasOf) continue;
    const previous = canonicalPrimaries.get(canonical);
    if (previous) throw new Error(`MCP canonical operation collision: ${canonical} (${previous.original.originalName}, ${record.original.originalName})`);
    canonicalPrimaries.set(canonical, record);
  }
  for (const record of records) {
    if (record.metadata.deprecatedAliasOf && !canonicalPrimaries.has(record.metadata.deprecatedAliasOf)) {
      throw new Error(`MCP deprecated alias ${record.original.originalName} points to missing canonical operation ${record.metadata.deprecatedAliasOf}`);
    }
  }
  const aliases = new Map<string, string>();
  for (const record of records) {
    for (const alias of record.metadata.aliases) {
      if (typeof alias !== "string" || !SNAKE_CASE.test(alias)) throw new Error(`MCP alias is not lowercase snake_case: ${String(alias)}`);
      if (aliases.has(alias)) throw new Error(`MCP alias collision: ${alias}`);
      if (alias === record.metadata.canonicalName) throw new Error(`MCP alias cycle: ${alias}`);
      aliases.set(alias, record.metadata.canonicalName);
    }
  }
  for (const [alias, canonical] of aliases) {
    if (!canonicalPrimaries.has(canonical)) throw new Error(`MCP alias ${alias} points to missing canonical operation ${canonical}`);
    if (aliases.has(canonical)) throw new Error(`MCP alias cycle: ${alias} -> ${canonical}`);
  }
}

function makeRecord(captured: CapturedMcpOperation): McpOperationRecord {
  const canonicalName = canonicalNameForOperation(captured.originalName);
  const safety = safetyFor(captured.config);
  const writeClass = writeClassFor(captured.config, safety);
  const idempotent = captured.config.annotations?.idempotentHint === true;
  const schema = operationSchemaIdentity(captured.config);
  const permission = MCP_TOOL_PERMISSIONS[captured.originalName];
  if (!permission) throw new Error(`MCP operation ${captured.originalName} has no permission classification`);
  const retryClass = retryClassForOperation(`mcp:${captured.originalName}`, { safety, idempotent });
  const deprecatedAliasOf = captured.originalName === "bookkeeping_batch_dry_run" ? "bookkeeping_batch_persist" : null;
  const aliases = canonicalName === captured.originalName || deprecatedAliasOf ? [] : [captured.originalName];
  const components = canonicalComponents(canonicalName);
  const metadataBase = {
    originalName: captured.originalName,
    canonicalName,
    ...components,
    aliases,
    title: typeof captured.config.title === "string" ? captured.config.title : null,
    description: operationDescription(captured.config),
    safety,
    writeClass,
    retryClass,
    idempotent,
    requiresActor: safety !== "read",
    requiresConfirmation: safety !== "read",
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
    deprecated: Boolean(record.metadata.deprecatedAliasOf),
    deprecatedAliasOf: record.metadata.deprecatedAliasOf,
  };
}

export function operationIdentityForLiveTool(record: McpOperationRecord, directlyListed: boolean) {
  return {
    name: record.original.originalName,
    canonicalName: record.metadata.canonicalName,
    aliases: record.metadata.aliases,
    directlyListed,
    invocationRoute: record.metadata.invocationRoute,
    schemaIdentityHash: record.metadata.schemaIdentityHash,
    operationIdentityHash: record.metadata.identityHash,
    annotations: record.original.config.annotations,
  };
}

export type GatewayResult = CallToolResult;
