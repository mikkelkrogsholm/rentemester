/**
 * Central tools-registrering for Rentemester-MCP-serveren.
 *
 * `registerAllTools` registrerer hele tool-surface'en — 246 tools fordelt
 * på de domæne-funktioner der kaldes herunder. Den autoritative liste
 * (klassifikation, inputs, CLI-mapping) står i docs/mcp-tool-surface.md;
 * driv en kørende server med `tools/list` for den faktiske, aktuelle liste.
 * Tæl-konsistens er bevogtet af tests/unit/mcp-tool-count-docs.test.ts (#367).
 *
 * Sikkerhedsklasser:
 *  - read              — bivirkningsfrie; må kaldes frit og parallelt.
 *  - write-reversible  — opretter state der kan tilbageføres; kræver
 *                        `confirm: true`.
 *  - write-irreversible — bogfører i den append-only hash-kæde; kræver
 *                         `confirm: true`. Rettes kun via en modpostering.
 *  - destructive       — `system_restore_backup`; kræver derudover
 *                        `confirmText`.
 *
 * Hver `register*Tools(server)`-funktion lever i `src/mcp/tools/<area>.ts`
 * og tilføjer kun sit eget domæne. Tool-surface'en er IKKE 1:1 med
 * `src/cli-meta.ts`: der er mindst 10 dokumenterede afvigelser fordelt på
 * en MCP-only-liste (fx `cvr`, `peppol`, `portfolio`, `period_list`) og en
 * CLI-only-liste (fx `agent`, `annual-report`, `dashboard`,
 * `opening-balance`, `reg`, `report`, `serve`, `bank-account`, `init`,
 * plus kommando-niveau-afvigelser som `gdpr forget`). Den maskinlæsbare diff vedligeholdes pr. fil i
 * `docs/mcp-tool-surface.md`-sektionerne "MCP-only — tools uden
 * CLI-pendant" og "CLI-only — kommandoer uden MCP-pendant", og
 * `tests/unit/surface-diff-discoverable.test.ts` (#376) fejler, hvis en ny
 * `src/cli/<x>.ts` eller `src/mcp/tools/<x>.ts` tilføjes uden at blive
 * listet der.
 */

import { existsSync } from "node:fs";
import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import { McpError, ErrorCode } from "@modelcontextprotocol/sdk/types.js";
import type { CallToolResult } from "@modelcontextprotocol/sdk/types.js";
import { migrate, openDb } from "../core/db";
import { companyPaths } from "../core/paths";
import { evaluateBackupLock } from "../core/backup-governance";
import { assertMcpCompanyReadOnlyHandler, resolveCompanyArg, runMcpReadOnlyTool } from "./tool-runtime";
import { envelopeShape, envelopeToCallResult, errorEnvelope, successEnvelope } from "./envelope";
import { registerAccountsTools } from "./tools/accounts";
import { registerAuditTools } from "./tools/audit";
import { registerBankTools } from "./tools/bank";
import { registerCustomerTools } from "./tools/customer";
import { registerCvrTools } from "./tools/cvr";
import { registerDocumentTools } from "./tools/documents";
import { registerDimensionTools } from "./tools/dimensions";
import { registerExceptionTools } from "./tools/exceptions";
import { registerExpenseTools } from "./tools/expense";
import { registerInvoiceTools } from "./tools/invoice";
import { registerJournalTools } from "./tools/journal";
// PEPPOL submission (#128)
import { registerPeppolTools } from "./tools/peppol";
import { registerPeriodTools } from "./tools/period";
import { registerRetentionTools } from "./tools/retention";
import { registerGdprTools } from "./tools/gdpr";
import { registerSystemTools } from "./tools/system";
import { registerVatTools } from "./tools/vat";
import { registerVendorTools } from "./tools/vendor";
// ===== RECURRING INVOICES (#118) =====
import { registerRecurringInvoiceTools } from "./tools/recurring-invoice";
// ===== END RECURRING INVOICES (#118) =====
// ===== MAIL INTAKE (#122) =====
import { registerMailIntakeTools } from "./tools/mail-intake";
// ===== IMAP INTAKE (#181) =====
import { registerImapIntakeTools } from "./tools/imap-intake";
// ===== END IMAP INTAKE (#181) =====
// ===== DIGISENSE E-FAKTURA MODTAG (#efaktura) =====
import { registerEfakturaTools } from "./tools/efaktura";
// ===== END DIGISENSE E-FAKTURA MODTAG (#efaktura) =====
// ===== MILEAGE LOG (#123) =====
import { registerMileageTools } from "./tools/mileage";
// Fixed assets (#124, #125)
import { registerAssetTools } from "./tools/asset";
// Multi-company portfolio: company_add + portfolio_overview (#172)
import { registerPortfolioTools } from "./tools/portfolio";
// ===== EMAIL DELIVERY (#180) =====
import { registerEmailTools } from "./tools/email";
// ===== END EMAIL DELIVERY (#180) =====
// ===== IMPORT ARCHIVE (#197) =====
import { registerImportTools } from "./tools/import";
// ===== END IMPORT ARCHIVE (#197) =====
// ===== TAX RETURN PREPARATION =====
import { registerTaxTools } from "./tools/tax";
// ===== END TAX RETURN PREPARATION =====
// ===== ACCRUALS / PERIODEAFGRÆNSNINGSPOSTER =====
import { registerAccrualTools } from "./tools/accrual";
// ===== END ACCRUALS / PERIODEAFGRÆNSNINGSPOSTER =====
// ===== BUDGET + LIQUIDITY FORECAST =====
import { registerBudgetTools } from "./tools/budget";
// ===== END BUDGET + LIQUIDITY FORECAST =====
// ===== PAYABLES / KREDITORSTYRING =====
import { registerPayableTools } from "./tools/payable";
// ===== END PAYABLES / KREDITORSTYRING =====
// ===== COMPANY PROFILE READ =====
import { registerCompanyProfileTools } from "./tools/company";
// ===== END COMPANY PROFILE READ =====
// ===== META / SERVER ABOUT =====
import { registerMetaTools } from "./tools/meta";
import { registerPostingRuleTools } from "./tools/posting-rules";
import { registerBookkeepingBatchTools } from "./tools/bookkeeping-batch";
import { registerPurchaseCaseTools } from "./tools/purchase-cases";
import { registerBookkeepingWorkbenchTools } from "./tools/bookkeeping-workbench";
import { registerWorkspaceRegistryTools } from "./tools/workspace-registry";
import { registerIntercompanyDispositionTools } from "./tools/intercompany-dispositions";
import { registerWorkspaceDocumentInboxTools } from "./tools/workspace-document-inbox";
import { registerAgentDiscoveryTools } from "./tools/agent-discovery";
import { registerCfoAnalyticsTools } from "./tools/cfo-analytics";
import { registerSupplierCommitmentTools } from "./tools/supplier-commitments";
import type { LiveTool } from "../agent-discovery-catalog";
import { authorizeMcpTool, runWithMcpAuthenticatedPrincipal, type McpSecurityContext, MCP_TOOL_PERMISSIONS } from "./security";
import {
  COMPACT_MCP_TOOL_NAMES,
  createMcpOperationRegistry,
  operationDescribePayload,
  operationIdentityForLiveTool,
  parseMcpOperationOutput,
  parseMcpOperationInput,
  resolveMcpOperation,
  type CapturedMcpOperation,
  type McpOperationProfile,
  type McpOperationRecord,
  type McpOperationRegistry,
  type McpRegisterConfig,
} from "./operation-registry";
// ===== END META / SERVER ABOUT =====

// Wraps a write tool's callback with the opt-in backup lock. The MCP tool
// files are not uniform — some use the withCompanyDbConfirmed helper, some
// have inline handlers — so the lock cannot live in one helper. Intercepting
// registerTool catches every tool regardless of how its handler is written.
function lockGuardedCallback(
  toolName: string,
  callback: (...args: unknown[]) => unknown,
): (...args: unknown[]) => unknown {
  return (...invocation: unknown[]) => {
    const args = invocation[0] as { company?: unknown } | undefined;
    const company = args?.company;
    if (typeof company === "string" && company.length > 0) {
      const resolution = resolveCompanyArg(company);
      if (resolution.ok && existsSync(companyPaths(resolution.companyRoot).db)) {
        const db = openDb(companyPaths(resolution.companyRoot).db);
        try {
          migrate(db);
          const lock = evaluateBackupLock(db, resolution.companyRoot);
          if (lock.locked) {
            return envelopeToCallResult(
              errorEnvelope(
                `Bogføring er låst (${toolName}): ${lock.reason}. ` +
                  "Diagnosticér med system_backup_status; kør derefter system_backup " +
                  "med archive:true for at låse op og placér kopien på en EU/EØS-" +
                  "destination med system_backup_place.",
                { code: "BACKUP_LOCKED" },
              ),
            );
          }
        } finally {
          db.close();
        }
      }
    }
    return callback(...invocation);
  };
}

// A registerTool-intercepting proxy: every write tool (not read-only, not a
// `system_*` tool) gets its callback wrapped with the backup lock. Read tools
// and system/backup tools pass through untouched — backing up must always
// stay possible, since that is the only way out of the lock.
export function lockGuardServer(server: McpServer, options: {
  capture?: (operation: CapturedMcpOperation) => void;
  forward?: boolean;
  transform?: (name: string, config: McpRegisterConfig, callback: (...args: unknown[]) => unknown) => (...args: unknown[]) => unknown;
} = {}): McpServer {
  return new Proxy(server, {
    get(target, prop, receiver) {
      if (prop === "registerTool") {
        return (name: string, config: McpRegisterConfig, callback: (...a: unknown[]) => unknown) => {
          const readOnly = config?.annotations?.readOnlyHint === true;
          // Workspace-control mutations do not alter a company ledger. Give
          // their runtime-published metadata the central reversible class if
          // an adapter omitted it, rather than maintaining a second list of
          // tool names next to the discovery/retry registry.
          if (!readOnly && config?.annotations?.destructiveHint !== true && !/\bwrite-(?:reversible|irreversible)\b/i.test(config.description ?? "")) {
            config = { ...config, description: `${config.description?.trim() ?? "Mutation."} Requires actor attribution and confirm:true; retry reads canonical state. write-reversible.` };
          }
          // A retry key is a domain contract, never a registry-wide affordance.
          // Tools expose it only when their mutation and durable receipt are
          // committed in the same transaction (currently the five #583 tools).
          // A company-scoped read tool is only registrable through the shared
          // runtime.  Raw callbacks could open SQLite in its default writable
          // mode and silently recreate WAL/SHM files or apply migrations.
          if (readOnly && config.inputSchema && typeof config.inputSchema === "object" && "company" in config.inputSchema) {
            assertMcpCompanyReadOnlyHandler(name, callback);
          }
          const opening = (callback as { companyDbOpening?: string }).companyDbOpening;
          const guarded = readOnly
            ? (...args: unknown[]) => runMcpReadOnlyTool(() => callback(...args))
            : name.startsWith("system_") || opening === "write" ? callback : lockGuardedCallback(name, callback);
          const finalCallback = options.transform?.(name, config, guarded) ?? guarded;
          options.capture?.({ originalName: name, config, callback: finalCallback });
          if (options.forward === false) return undefined;
          return (target.registerTool as (...a: unknown[]) => unknown)(name, config, finalCallback);
        };
      }
      const value = Reflect.get(target, prop, receiver);
      return typeof value === "function" ? value.bind(target) : value;
    },
  });
}

type RegisterAllOptions = { profile?: McpOperationProfile } | McpOperationProfile;

/**
 * Register the complete precise surface (the historical default for direct
 * embedders).  The stdio entrypoint calls this with `{profile:"compact"}`;
 * keeping the function's default full preserves existing in-process users and
 * makes the compatibility profile an explicit, testable choice.
 */
export function registerAllTools(
  server: McpServer,
  security?: McpSecurityContext | null,
  options: RegisterAllOptions = { profile: "full" },
): McpOperationRegistry {
  const profile = typeof options === "string" ? options : options.profile ?? "full";
  if (profile !== "compact" && profile !== "full") throw new Error(`unknown MCP startup profile: ${String(profile)}`);
  const publicServer = server;
  const liveTools: LiveTool[] = [];
  const captured: CapturedMcpOperation[] = [];
  const guarded = lockGuardServer(server, {
    forward: profile === "full",
    transform: (name, config, callback) => secureMcpCallback(security, name, config, callback),
    capture: (operation) => {
      captured.push(operation);
      liveTools.push({ name: operation.originalName, annotations: operation.config.annotations });
    },
  });
  server = guarded;
  registerAccountsTools(server);
  registerAuditTools(server);
  registerBankTools(server);
  registerCustomerTools(server);
  registerCvrTools(server);
  registerDocumentTools(server);
  registerDimensionTools(server);
  registerExceptionTools(server);
  registerExpenseTools(server);
  registerInvoiceTools(server);
  registerJournalTools(server);
  // PEPPOL submission (#128)
  registerPeppolTools(server);
  registerPeriodTools(server);
  registerRetentionTools(server);
  registerGdprTools(server);
  registerSystemTools(server);
  registerVatTools(server);
  registerVendorTools(server);
  // ===== RECURRING INVOICES (#118) =====
  registerRecurringInvoiceTools(server);
  // ===== END RECURRING INVOICES (#118) =====
  // ===== MAIL INTAKE (#122) =====
  registerMailIntakeTools(server);
  // ===== IMAP INTAKE (#181) =====
  registerImapIntakeTools(server);
  // ===== END IMAP INTAKE (#181) =====
  // ===== DIGISENSE E-FAKTURA MODTAG (#efaktura) =====
  registerEfakturaTools(server);
  // ===== END DIGISENSE E-FAKTURA MODTAG (#efaktura) =====
  // ===== MILEAGE LOG (#123) =====
  registerMileageTools(server);
  // Fixed assets (#124, #125)
  registerAssetTools(server);
  // Multi-company portfolio: company_add + portfolio_overview (#172)
  registerPortfolioTools(server);
  registerCfoAnalyticsTools(server);
  registerSupplierCommitmentTools(server);
  // ===== EMAIL DELIVERY (#180) =====
  registerEmailTools(server);
  // ===== END EMAIL DELIVERY (#180) =====
  // ===== IMPORT ARCHIVE (#197) =====
  registerImportTools(server);
  // ===== END IMPORT ARCHIVE (#197) =====
  // ===== TAX RETURN PREPARATION =====
  registerTaxTools(server);
  // ===== END TAX RETURN PREPARATION =====
  // ===== ACCRUALS / PERIODEAFGRÆNSNINGSPOSTER =====
  registerAccrualTools(server);
  // ===== END ACCRUALS / PERIODEAFGRÆNSNINGSPOSTER =====
  // ===== BUDGET + LIQUIDITY FORECAST =====
  registerBudgetTools(server);
  // ===== END BUDGET + LIQUIDITY FORECAST =====
  // ===== PAYABLES / KREDITORSTYRING =====
  registerPayableTools(server);
  // ===== END PAYABLES / KREDITORSTYRING =====
  // ===== COMPANY PROFILE READ =====
  registerCompanyProfileTools(server);
  // ===== END COMPANY PROFILE READ =====
  // ===== META / SERVER ABOUT =====
  registerMetaTools(server, () => liveTools);
  registerPostingRuleTools(server);
  registerBookkeepingBatchTools(server);
  registerPurchaseCaseTools(server);
  registerBookkeepingWorkbenchTools(server);
  registerWorkspaceRegistryTools(server);
  registerIntercompanyDispositionTools(server);
  registerWorkspaceDocumentInboxTools(server);
  // Must be last: workflow descriptions resolve the live registered tool set.
  registerAgentDiscoveryTools(server, () => liveTools);
  // ===== END META / SERVER ABOUT =====
  const registry = createMcpOperationRegistry(captured, profile);
  for (const live of liveTools) {
    const record = registry.byOriginalName.get(live.name);
    if (record) Object.assign(live, operationIdentityForLiveTool(record, profile === "full"));
  }
  if (security) assertMcpPermissionCoverage(liveTools);
  // Compact projection tools are transport registrations too.  Keep the
  // outer security guard on these newly-created gateway names so discovery
  // and gateway calls preserve the same authenticated request context as the
  // captured original callbacks.  The selected callback then re-authorizes
  // once more against its original name before it can touch a company.
  if (profile === "compact") registerCompactSurface(publicServer, security, registry, liveTools);
  return registry;
}

/** Compact is the startup/default profile.  This named entry point makes the
 * transport decision obvious to embedders without changing the full-profile
 * compatibility default of registerAllTools. */
export function registerCompactTools(server: McpServer, security?: McpSecurityContext | null): McpOperationRegistry {
  return registerAllTools(server, security, { profile: "compact" });
}

type GatewayArgs = { operation: string; input?: Record<string, unknown>; arguments?: Record<string, unknown> };

function gatewayError(message: string, code: string) {
  return envelopeToCallResult(errorEnvelope(message, { code }));
}

function patchCallResultData(result: CallToolResult, patch: Record<string, unknown>): CallToolResult {
  const structured = result.structuredContent;
  if (!structured || typeof structured !== "object") return result;
  const envelope = structured as Record<string, unknown>;
  if (!envelope.data || typeof envelope.data !== "object") return result;
  const data = { ...(envelope.data as Record<string, unknown>), ...patch };
  const content = result.content.map((item) => {
    if (item.type !== "text") return item;
    try {
      const parsed = JSON.parse(item.text) as Record<string, unknown>;
      if (!parsed || typeof parsed !== "object" || !parsed.data || typeof parsed.data !== "object") return item;
      return { ...item, text: JSON.stringify({ ...parsed, data }) };
    } catch {
      return item;
    }
  });
  return { ...result, content, structuredContent: { ...envelope, data } };
}

function withDeprecatedDryRunResponse(result: unknown): unknown {
  if (!result || typeof result !== "object" || !("content" in result)) return result;
  return patchCallResultData(result as CallToolResult, {
    dryRun: true,
    deprecatedAlias: "bookkeeping_batch_dry_run",
    canonicalOperation: "bookkeeping_batch_persist",
  });
}

async function invokeCapturedOperation(
  record: McpOperationRecord,
  input: unknown,
  extra: unknown,
  alias: string | null = null,
): Promise<import("@modelcontextprotocol/sdk/types.js").CallToolResult> {
  const parsed = await parseMcpOperationInput(record, input);
  if (!parsed.success) {
    const message = parsed.error instanceof Error ? parsed.error.message : "invalid selected operation input";
    // Match the MCP SDK's input-validation boundary.  This is thrown before
    // the captured callback, so no DB/filesystem/ledger side effect can occur.
    throw new McpError(ErrorCode.InvalidParams, `Input validation error: Invalid arguments for operation ${record.metadata.canonicalName}: ${message}`);
  }
  const callback = record.original.callback;
  const result = record.original.config.inputSchema === undefined
    ? await callback(extra)
    : await callback(parsed.data, extra);
  const output = await parseMcpOperationOutput(record, result);
  if (!output.success) {
    const message = output.error instanceof Error ? output.error.message : "invalid selected operation output";
    throw new McpError(ErrorCode.InvalidParams, `Output validation error: Invalid structured content for operation ${record.metadata.canonicalName}: ${message}`);
  }
  return (alias === "bookkeeping_batch_dry_run" ? withDeprecatedDryRunResponse(result) : result) as import("@modelcontextprotocol/sdk/types.js").CallToolResult;
}

function operationSearch(registry: McpOperationRegistry, query: string | undefined, cursor: number, limit: number) {
  const tokens = (query ?? "").trim().toLowerCase().split(/\s+/).filter(Boolean);
  const matching = registry.operations.filter((record) => {
    const haystack = [
      record.original.originalName,
      record.metadata.canonicalName,
      ...record.metadata.aliases,
      record.metadata.title ?? "",
      record.metadata.description,
    ].join(" ").toLowerCase();
    return tokens.every((token) => haystack.includes(token));
  });
  const items = matching.slice(cursor, cursor + limit).map((record) => ({
    originalName: record.metadata.originalName,
    canonicalName: record.metadata.canonicalName,
    aliases: record.metadata.aliases,
    title: record.metadata.title,
    description: record.metadata.description,
    safety: record.metadata.safety,
    writeClass: record.metadata.writeClass,
    retryClass: record.metadata.retryClass,
    idempotent: record.metadata.idempotent,
    requiresActor: record.metadata.requiresActor,
    requiresConfirmation: record.metadata.requiresConfirmation,
    permission: record.metadata.permission,
    invocationRoute: record.metadata.invocationRoute,
    schemaIdentityHash: record.metadata.schemaIdentityHash,
    operationIdentityHash: record.metadata.identityHash,
    available: true,
    directlyListed: registry.profile === "full",
    deprecated: Boolean(record.metadata.deprecatedAliasOf),
    deprecatedAliasOf: record.metadata.deprecatedAliasOf,
  }));
  return {
    profile: registry.profile,
    registryIdentityHash: registry.identityHash,
    total: matching.length,
    count: items.length,
    cursor,
    limit,
    hasMore: cursor + items.length < matching.length,
    nextCursor: cursor + items.length < matching.length ? cursor + items.length : null,
    items,
  };
}

function registerCompactSurface(
  server: McpServer,
  security: McpSecurityContext | null | undefined,
  registry: McpOperationRegistry,
  liveTools: readonly LiveTool[],
): void {
  const about = registry.byOriginalName.get("meta_about");
  const capabilities = registry.byOriginalName.get("agent_capability_search");
  const workflows = registry.byOriginalName.get("agent_workflow_describe");
  if (!about || !capabilities || !workflows) throw new Error("compact MCP surface requires meta and discovery operations");

  // Discovery operations retain their original exact schemas/callbacks.  The
  // gateway operations below are the only new schemas in the compact profile.
  registerSecureTool(server, security, "system_server_about", {
    title: "Server identity and compact operation surface",
    description: "Read-only server identity, startup profile, operation-registry digest and contract pointers.",
    inputSchema: {},
    outputSchema: envelopeShape,
    annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
  }, async (_args: unknown, extra: unknown): Promise<CallToolResult> => {
    const result = await invokeCapturedOperation(about, {}, extra);
    if (!result || typeof result !== "object") return result;
    const callResult = result as CallToolResult;
    const structured = callResult.structuredContent;
    if (!structured || typeof structured !== "object") return result;
    const envelope = structured as Record<string, unknown>;
    if (!envelope.data || typeof envelope.data !== "object") return result;
    return patchCallResultData(callResult, {
      profile: "compact",
      toolCount: COMPACT_MCP_TOOL_NAMES.length,
      directlyListedToolCount: COMPACT_MCP_TOOL_NAMES.length,
      operationCount: liveTools.length,
      operationRegistryIdentityHash: registry.identityHash,
    });
  });
  server.registerTool(capabilities.original.originalName, capabilities.original.config as never, capabilities.original.callback as never);
  server.registerTool(workflows.original.originalName, workflows.original.config as never, workflows.original.callback as never);

  registerSecureTool(server, security, "agent_operation_search", {
    title: "Search available MCP operations",
    description: "Read-only deterministic search across all internal operations. Results identify whether each operation is directly listed or gateway-invoked.",
    inputSchema: { query: z.string().max(200).optional(), cursor: z.number().int().min(0).optional(), limit: z.number().int().min(1).max(100).optional() },
    outputSchema: envelopeShape,
    annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
  }, ({ query, cursor, limit }: { query?: string; cursor?: number; limit?: number }) => envelopeToCallResult(successEnvelope(operationSearch(registry, query, cursor ?? 0, limit ?? 25))));

  registerSecureTool(server, security, "agent_operation_describe", {
    title: "Describe one MCP operation",
    description: "Return the exact captured input/output schemas, canonical identity, aliases, safety, permission, prerequisites and retry contract.",
    inputSchema: { operation: z.string().min(1).max(200) },
    outputSchema: envelopeShape,
    annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
  }, ({ operation }: { operation: string }) => {
    const record = registry.byOriginalName.get(operation) ?? resolveMcpOperation(registry, operation);
    return record
      ? envelopeToCallResult(successEnvelope({ profile: registry.profile, registryIdentityHash: registry.identityHash, operation: operationDescribePayload(record, registry) }))
      : gatewayError(`unknown operation: ${operation}`, "UNKNOWN_OPERATION");
  });

  const gatewayConfig = (route: "read" | "write" | "destroy") => ({
    title: route === "read" ? "Invoke one read operation" : route === "write" ? "Invoke one reversible or ledger operation" : "Invoke one destructive operation",
    description: `Invoke exactly one ${route} operation selected by canonical name or legacy alias. No planning, inference or batching is performed.`,
    inputSchema: { operation: z.string().min(1).max(200), input: z.record(z.string(), z.unknown()).optional(), arguments: z.record(z.string(), z.unknown()).optional() },
    outputSchema: envelopeShape,
    annotations: { readOnlyHint: route === "read", destructiveHint: route === "destroy", idempotentHint: route === "read", openWorldHint: false },
  });
  const registerGateway = (name: "agent_operation_read" | "agent_operation_write" | "agent_operation_destroy", route: "read" | "write" | "destroy") => {
    registerSecureTool(server, security, name, gatewayConfig(route), async ({ operation, input, arguments: legacyArguments }: GatewayArgs, extra: unknown) => {
      const requested = registry.byOriginalName.get(operation);
      const record = requested?.metadata.deprecatedAliasOf ? resolveMcpOperation(registry, requested.metadata.deprecatedAliasOf) : resolveMcpOperation(registry, operation);
      if (!record) return gatewayError(`unknown operation: ${operation}`, "UNKNOWN_OPERATION");
      const expectedRoute = record.metadata.invocationRoute;
      if ((route === "read" && expectedRoute !== "agent_operation_read") || (route === "write" && expectedRoute !== "agent_operation_write") || (route === "destroy" && expectedRoute !== "agent_operation_destroy")) {
        return gatewayError(`gateway class mismatch for ${operation}`, "GATEWAY_CLASS_MISMATCH");
      }
      const alias = requested?.metadata.deprecatedAliasOf ? operation : null;
      return invokeCapturedOperation(record, input ?? legacyArguments ?? {}, extra, alias);
    });
  };
  registerGateway("agent_operation_read", "read");
  registerGateway("agent_operation_write", "write");
  registerGateway("agent_operation_destroy", "destroy");
  // Keep this assertion local to the projection, so a future edit cannot
  // silently add a compact tool while preserving a misleading eight-tool
  // contract.
  const names = [...COMPACT_MCP_TOOL_NAMES];
  if (names.length !== 8 || new Set(names).size !== 8) throw new Error("compact MCP tool projection must contain exactly eight unique names");
}

function registerSecureTool(
  server: McpServer,
  security: McpSecurityContext | null | undefined,
  name: string,
  config: McpRegisterConfig,
  callback: unknown,
): void {
  server.registerTool(name, config as never, secureMcpCallback(security, name, config, callback as (...args: unknown[]) => unknown) as never);
}

function secureMcpCallback(
  context: McpSecurityContext | null | undefined,
  name: string,
  _config: McpRegisterConfig,
  callback: (...args: unknown[]) => unknown,
): (args: unknown, ...rest: unknown[]) => unknown {
  if (!context) return callback;
  return async (rawArgs: unknown, ...rest: unknown[]) => {
    const args = rawArgs && typeof rawArgs === "object" ? rawArgs as Record<string, unknown> : {};
    const access = await authorizeMcpTool(context, name, args);
    if (!access) return envelopeToCallResult(errorEnvelope("missing or invalid credentials", { code: "MCP_UNAUTHORIZED" }));
    // A service-principal MCP session is bound to one canonical workspace root
    // at startup. Never let an untrusted tool argument select a second root;
    // workspace fan-out handlers receive the verified canonical path, while
    // company tools receive their verified canonical company root.
    const securedArgs = access.root
      ? { ...args, company: access.root }
      : name === "efaktura_modtag_workspace" || name === "recurring_invoice_run_workspace" || (args && "workspace" in args)
        ? { ...args, workspace: context.workspaceRoot }
        : args;
    return runWithMcpAuthenticatedPrincipal(access.principal, context.workspaceRoot, () => Promise.resolve(callback(securedArgs, ...rest)));
  };
}

export function assertMcpPermissionCoverage(liveTools: readonly LiveTool[]): void {
  const names = liveTools.map((tool) => tool.name);
  const duplicates = names.filter((name, index) => names.indexOf(name) !== index);
  const missing = names.filter((name) => !(name in MCP_TOOL_PERMISSIONS));
  // Compact gateway names are transport projections, not internal precise
  // registrations.  Their selected original callback re-authorizes against
  // the original name, so they are intentionally absent from this parity
  // check.
  const extra = Object.keys(MCP_TOOL_PERMISSIONS).filter((name) => !names.includes(name) && !(COMPACT_MCP_TOOL_NAMES as readonly string[]).includes(name));
  if (duplicates.length || missing.length || extra.length) throw new Error(`MCP permission coverage mismatch: duplicate=${[...new Set(duplicates)].join(",")} missing=${missing.join(",")} extra=${extra.join(",")}`);
}
