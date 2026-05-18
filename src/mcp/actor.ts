/**
 * MCP-call attribution.
 *
 * Hver MCP-call skal tilskrives som `actor.kind = 'agent'` (jf. #63) så
 * append-only audit_log kan spore en bogføring tilbage til den agent (og
 * den agent-bruger) der trykkede knappen.
 *
 * Format (jf. docs/mcp-tool-surface.md §5):
 *
 *   agent:<client-name>/<client-version> (user:<email-or-id>)
 *
 * Kilder, i prioriteret rækkefølge:
 *  1. MCP-klientens egen `Implementation` (name + version) fra
 *     initialize-handshake. Dette er den autoritative kilde — det er
 *     Claude Desktop / Cursor / Claude Code / Codex der præsenterer sig
 *     selv.
 *  2. `RENTEMESTER_MCP_USER` env-var hvis sat (typisk login-email).
 *  3. `RENTEMESTER_MCP_AGENT` env-var som fallback hvis klient-handshake
 *     ikke er kørt endnu (fx før første tool-call).
 *
 * VIGTIGT: Vi sætter actor som **eksplicit parameter** når vi kalder
 * kernen — ikke som proces-env-var. Det er den anbefalede vej i #76
 * fordi env-vars er race-prone når flere requests behandles parallelt.
 * Kerne-funktionerne accepterer allerede `createdBy` / `createdByProgram`
 * i deres input-payload (se `JournalEntryInput`).
 */

import type { Implementation } from "@modelcontextprotocol/sdk/types.js";

export type McpActor = {
  /** Kanonisk actor-id, fx `agent:claude-code/0.4.1`. */
  createdBy: string;
  /**
   * Hvor kaldet kom fra — typisk `mcp:<user>` hvis vi kender brugeren,
   * ellers bare `mcp`. Skrives ind i journal_entries.created_by_program
   * og indgår i auditActor-rendering.
   */
  createdByProgram: string;
  /**
   * Det fulde render-format som ender i audit_log.actor:
   * `agent:claude-code/0.4.1 via mcp:user@example.com`
   */
  auditActor: string;
};

const FALLBACK_AGENT = "agent:unknown-mcp-client";
const FALLBACK_PROGRAM = "rentemester-mcp";

function trim(value: string | null | undefined): string | null {
  if (typeof value !== "string") return null;
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
}

/**
 * Bygger en `McpActor` ud fra MCP-klientens handshake-info.
 *
 * @param clientInfo Resultatet af `mcpServer.server.getClientVersion()`.
 *   Kan være `undefined` hvis serveren endnu ikke har modtaget
 *   initialize-handshake (fx ved skarpe restart-races).
 */
export function deriveMcpActor(clientInfo: Implementation | undefined): McpActor {
  const clientName = trim(clientInfo?.name);
  const clientVersion = trim(clientInfo?.version);
  const agentId = clientName
    ? `agent:${clientName}${clientVersion ? `/${clientVersion}` : ""}`
    : trim(process.env.RENTEMESTER_MCP_AGENT)
      ? `agent:${process.env.RENTEMESTER_MCP_AGENT}`
      : FALLBACK_AGENT;

  const userHint = trim(process.env.RENTEMESTER_MCP_USER);
  const program = userHint ? `mcp:${userHint}` : FALLBACK_PROGRAM;

  return {
    createdBy: agentId,
    createdByProgram: program,
    auditActor: `${agentId} via ${program}`,
  };
}

/**
 * Hjælper der pakker `createdBy` / `createdByProgram` ind i et kerne-payload
 * uden at overskrive eksplicitte kalds-værdier (defensiv).
 */
export function withActor<T extends { createdBy?: string; createdByProgram?: string }>(
  payload: T,
  actor: McpActor,
): T {
  return {
    ...payload,
    createdBy: payload.createdBy ?? actor.createdBy,
    createdByProgram: payload.createdByProgram ?? actor.createdByProgram,
  };
}
