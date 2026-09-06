#!/usr/bin/env bun
/**
 * Rentemester MCP-server (stdio transport).
 *
 * Eksponerer den kompakte, discovery-ledede MCP-surface (otte tools som
 * standard) over stdio, så Claude Desktop / Cursor / Claude Code / Codex kan
 * tale med Rentemester-kernen. `RENTEMESTER_MCP_PROFILE=full` vælger den
 * bagudkompatible direkte legacy-surface. Alle præcise operationer registreres
 * internt af `registerAllTools` i `./registry`.
 *
 * Serveren leverer også en `instructions`-streng i `initialize`-svaret —
 * en kort orientering til en agent om rækkefølge, confirm/destructive-
 * konventioner og hvor forudsætningerne ligger. Den fulde kontrakt for
 * den løse tool-surface står i docs/mcp-agent-contract.md.
 *
 * Brug:
 *   bun src/mcp/server.ts                  # start over stdio
 *   bun src/mcp/server.ts --company /path  # accepteres men ikke krævet;
 *                                           agenten passer typisk
 *                                           `company` per tool-call.
 *
 * Globalt installeret:
 *   rentemester-mcp                        # via package.json "bin"
 */

import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { registerAllTools } from "./registry";
import type { McpOperationProfile } from "./operation-registry";
import { createMcpSecurityContextFromEnv } from "./security";
import { PRODUCT_VERSION } from "../core/build-identity";

// Exported so the `system_about` MCP tool can return the live identity
// without re-declaring it. (Batch F-2)
export const SERVER_NAME = "rentemester-mcp";
export const SERVER_VERSION = PRODUCT_VERSION;

/** Startup selection is immutable for the lifetime of a transport. */
export function resolveMcpProfile(env: NodeJS.ProcessEnv = process.env): McpOperationProfile {
  const value = env.RENTEMESTER_MCP_PROFILE?.trim().toLowerCase() || "compact";
  if (value !== "compact" && value !== "full") throw new Error(`unknown MCP startup profile: ${value}`);
  return value;
}

/**
 * Orientering der sendes til agenten i `initialize`-svarets
 * `instructions`-felt. Holdes kort og handlingsanvisende — den fulde
 * kontrakt for den løse tool-surface står i docs/mcp-agent-contract.md.
 */
const SERVER_INSTRUCTIONS = [
  "Rentemester er et dansk, append-only bogføringssystem. Du driver det via løse tools — der er ingen samtale-state mellem kald.",
  "The default compact profile lists exactly eight tools: system_server_about, agent_capability_search, agent_workflow_describe, agent_operation_search, agent_operation_describe, agent_operation_read, agent_operation_write and agent_operation_destroy. Start discovery with system_server_about, then capability/workflow search and operation describe; do not guess tool names or capabilities.",
  "Set RENTEMESTER_MCP_PROFILE=full at startup for the backward-compatible direct legacy tool surface. Profile selection is fixed for the transport; search and calls never change tools/list.",
  "",
  "Identifikation: hvert tool tager en eksplicit absolut `company`-sti (workspace-tools tager `workspace`). Der er aldrig en implicit \"current company\".",
  "",
  "Sikkerhedsklasser (se hvert tools `annotations`): read er bivirkningsfri og må kaldes frit; write-tools kræver `confirm: true` i argumenterne ellers afvises kaldet før kernen kaldes; det destruktive `system_restore_backup` kræver derudover `confirmText: \"RESTORE <targetCompany>\"`.",
  "",
  "Rækkefølge: læs før du skriver. Et typisk flow er validate/status/list (read) → issue/post/settle (write). Bogføring sker i en hash-kædet append-only ledger — der findes ingen sletning; en fejlpostering rettes med en modpostering (journal_reverse / invoice_credit_note) eller løses via exception_resolve.",
  "",
  "Forudsætninger og fejl: hvert kald svarer med konvolutten { ok, data?, errors[], appliedRules? }. ok=false betyder at en forudsætning manglede — errors[] forklarer hvad (fx manglende confirm, ubalanceret postering, manglende VIES-validering, periode-lås eller en aktiv backup-lås). Ret forudsætningen og kald igen; gæt aldrig.",
  "",
  "Én svarform er IKKE konvolutten: hvis payload'en er schema-ugyldig (manglende påkrævet felt, forkert type, fx journal_post med færre end 2 linjer), afviser MCP-SDK'ens input-validering kaldet FØR handleren — svaret er da en rå JSON-RPC-fejl med code -32602 (\"Input validation error\"), isError:true og UDEN structuredContent/errors[]. Forgren på isError===true && structuredContent===undefined før du læser errors[]; ret det navngivne felt og kald igen. Den fulde -32602-kontrakt står i docs/mcp-agent-contract.md.",
  "",
  "Retries: læs den kanoniske retry-klasse fra discovery. Kun journal_post, journal_reverse, expense_book, payable_register og payable_pay har key-idempotent receipts; naturligt idempotente flows de-duplikerer deres egen domæneidentitet; provider-kald kræver status/reconciliation; alle øvrige writes kræver read-back før nyt forsøg. Backup-låsen kan blokere bogføring med `code: \"BACKUP_LOCKED\"`; diagnosticér med system_backup_status og kør derefter system_backup (archive:true) for at låse op.",
  "",
  "Den fulde kontrakt — tool-katalog, rækkefølge og konventioner — står i docs/mcp-tool-surface.md og docs/mcp-agent-contract.md.",
].join("\n");

async function main(): Promise<void> {
  // Resolve the immutable transport profile before reading credentials or
  // registering any operation. Unknown values therefore fail at startup,
  // before security setup can observe a partially selected surface.
  const profile = resolveMcpProfile();
  const server = new McpServer(
    {
      name: SERVER_NAME,
      version: SERVER_VERSION,
    },
    {
      instructions: SERVER_INSTRUCTIONS,
    },
  );

  const security = createMcpSecurityContextFromEnv();
  // Hosted/service MCP is an authenticated product boundary.  The local
  // single-owner CLI remains deliberately explicit and is the only mode that
  // may start without a service credential.
  const serviceMode = process.env.RENTEMESTER_DEPLOYMENT_PROFILE === "hosted"
    || process.env.RENTEMESTER_MCP_SERVICE_MODE === "true";
  if (serviceMode && !security) {
    throw new Error("MCP service mode requires RENTEMESTER_SERVICE_PRINCIPAL_TOKEN and RENTEMESTER_WORKSPACE");
  }
  registerAllTools(server, security, { profile });

  const transport = new StdioServerTransport();
  await server.connect(transport);
  // Forbliv kørende indtil stdin lukkes; SDK'en lukker transport
  // automatisk når den ser EOF.
}

main().catch((error) => {
  // Skriv til stderr så stdout-stream'en (MCP-framing) ikke bliver
  // korrumperet af logs.
  console.error("[rentemester-mcp] fatal:", error);
  process.exit(1);
});
