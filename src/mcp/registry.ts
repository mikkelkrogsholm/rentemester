/**
 * Central tools-registrering for Rentemester-MCP-serveren.
 *
 * `registerAllTools` registrerer hele tool-surface'en — 236 tools fordelt
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
import { migrate, openDb } from "../core/db";
import { companyPaths } from "../core/paths";
import { evaluateBackupLock } from "../core/backup-governance";
import { assertMcpCompanyReadOnlyHandler, resolveCompanyArg, runMcpReadOnlyTool } from "./tool-runtime";
import { envelopeToCallResult, errorEnvelope } from "./envelope";
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
export function lockGuardServer(server: McpServer): McpServer {
  return new Proxy(server, {
    get(target, prop, receiver) {
      if (prop === "registerTool") {
        return (name: string, config: { annotations?: { readOnlyHint?: boolean; destructiveHint?: boolean }; inputSchema?: Record<string, unknown>; description?: string }, callback: (...a: unknown[]) => unknown) => {
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
          if (readOnly && config.inputSchema && "company" in config.inputSchema) {
            assertMcpCompanyReadOnlyHandler(name, callback);
          }
          const opening = (callback as { companyDbOpening?: string }).companyDbOpening;
          const guarded = readOnly
            ? (...args: unknown[]) => runMcpReadOnlyTool(() => callback(...args))
            : name.startsWith("system_") || opening === "write" ? callback : lockGuardedCallback(name, callback);
          return (target.registerTool as (...a: unknown[]) => unknown)(name, config, guarded);
        };
      }
      const value = Reflect.get(target, prop, receiver);
      return typeof value === "function" ? value.bind(target) : value;
    },
  });
}

function recordingServer(server: McpServer, tools: LiveTool[]): McpServer {
  return new Proxy(server, {
    get(target, prop, receiver) {
      if (prop === "registerTool") {
        return (name: string, config: { annotations?: LiveTool["annotations"] }, callback: (...args: unknown[]) => unknown) => {
          tools.push({ name, annotations: config.annotations });
          return (target.registerTool as (...args: unknown[]) => unknown)(name, config, callback);
        };
      }
      const value = Reflect.get(target, prop, receiver);
      return typeof value === "function" ? value.bind(target) : value;
    },
  });
}

export function registerAllTools(server: McpServer, security?: McpSecurityContext | null): void {
  const liveTools: LiveTool[] = [];
  server = recordingServer(lockGuardServer(securityGuardServer(server, security)), liveTools);
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
  if (security) assertMcpPermissionCoverage(liveTools);
  // ===== END META / SERVER ABOUT =====
}

function securityGuardServer(server: McpServer, context?: McpSecurityContext | null): McpServer {
  if (!context) return server;
  return new Proxy(server, {
    get(target, prop, receiver) {
      if (prop === "registerTool") return (name: string, config: unknown, callback: (...args: unknown[]) => unknown) => {
        const guarded = async (args: Record<string, unknown>, ...rest: unknown[]) => {
          const access = await authorizeMcpTool(context, name, args ?? {});
          if (!access) return envelopeToCallResult(errorEnvelope("missing or invalid credentials", { code: "MCP_UNAUTHORIZED" }));
          // A service-principal MCP session is bound to one canonical
          // workspace root at startup.  Never let an untrusted tool argument
          // select a second root; workspace fan-out handlers receive the
          // verified canonical path, while company tools receive their
          // verified canonical company root.
          const securedArgs = access.root
            ? { ...args, company: access.root }
            : name === "efaktura_modtag_workspace" || name === "recurring_invoice_run_workspace" || (args && "workspace" in args)
              ? { ...args, workspace: context.workspaceRoot }
              : args;
          return runWithMcpAuthenticatedPrincipal(access.principal, context.workspaceRoot, () => Promise.resolve(callback(securedArgs, ...rest)));
        };
        return (target.registerTool as (...a: unknown[]) => unknown)(name, config, guarded);
      };
      const value = Reflect.get(target, prop, receiver);
      return typeof value === "function" ? value.bind(target) : value;
    },
  });
}

export function assertMcpPermissionCoverage(liveTools: readonly LiveTool[]): void {
  const names = liveTools.map((tool) => tool.name);
  const duplicates = names.filter((name, index) => names.indexOf(name) !== index);
  const missing = names.filter((name) => !(name in MCP_TOOL_PERMISSIONS));
  const extra = Object.keys(MCP_TOOL_PERMISSIONS).filter((name) => !names.includes(name));
  if (duplicates.length || missing.length || extra.length) throw new Error(`MCP permission coverage mismatch: duplicate=${[...new Set(duplicates)].join(",")} missing=${missing.join(",")} extra=${extra.join(",")}`);
}
