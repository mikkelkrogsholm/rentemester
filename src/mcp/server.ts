#!/usr/bin/env bun
/**
 * Rentemester MCP-server (stdio transport).
 *
 * Eksponerer hele Rentemester-tool-surface'en (81 tools, jf.
 * docs/mcp-tool-surface.md) over stdio, så Claude Desktop / Cursor /
 * Claude Code / Codex kan tale med Rentemester-kernen som MCP-tools.
 * Tools registreres pr. domæne af `registerAllTools` i `./registry`.
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

// Exported so the `system_about` MCP tool can return the live identity
// without re-declaring it. (Batch F-2)
export const SERVER_NAME = "rentemester-mcp";
export const SERVER_VERSION = "0.0.1";

/**
 * Orientering der sendes til agenten i `initialize`-svarets
 * `instructions`-felt. Holdes kort og handlingsanvisende — den fulde
 * kontrakt for den løse tool-surface står i docs/mcp-agent-contract.md.
 */
const SERVER_INSTRUCTIONS = [
  "Rentemester er et dansk, append-only bogføringssystem. Du driver det via løse tools — der er ingen samtale-state mellem kald.",
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
  "Retries: der findes ingen generel idempotency-mekanisme — en gentaget write (fx journal_post) dobbelt-bogfører. Læs tilstanden tilbage (status/list) før du genudsteder en write. Backup-låsen kan blokere bogføring hvis den ugentlige backup er forsømt — den fejler med konvoluttens stabile `code: \"BACKUP_LOCKED\"` (cross-cutting precondition, dokumenteret i docs/mcp-tool-surface.md). Diagnosticér med system_backup_status og kør derefter system_backup (archive:true) for at låse op.",
  "",
  "Den fulde kontrakt — tool-katalog, rækkefølge og konventioner — står i docs/mcp-tool-surface.md og docs/mcp-agent-contract.md.",
].join("\n");

async function main(): Promise<void> {
  const server = new McpServer(
    {
      name: SERVER_NAME,
      version: SERVER_VERSION,
    },
    {
      instructions: SERVER_INSTRUCTIONS,
    },
  );

  registerAllTools(server);

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
