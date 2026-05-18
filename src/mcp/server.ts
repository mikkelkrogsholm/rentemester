#!/usr/bin/env bun
/**
 * Rentemester MCP-server (stdio transport).
 *
 * Scaffold-version (issue #77). Registrerer to tools — `audit_verify`
 * og `journal_post` — og lytter på stdio så Claude Desktop / Cursor /
 * Claude Code / Codex kan tale med Rentemester-kernen som MCP-tools.
 *
 * Brug:
 *   bun src/mcp/server.ts                  # start over stdio
 *   bun src/mcp/server.ts --company /path  # accepteres men ikke krævet;
 *                                           agenten passer typisk
 *                                           `company` per tool-call.
 *
 * Globalt installeret:
 *   rentemester-mcp                        # via package.json "bin"
 *
 * Tool-surface (jf. docs/mcp-tool-surface.md) implementeres bredt i
 * issue #78; her står kun det minimum der gør ende-til-ende-flowet
 * verificerbart.
 */

import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { registerAllTools } from "./registry";

const SERVER_NAME = "rentemester-mcp";
const SERVER_VERSION = "0.0.1";

async function main(): Promise<void> {
  const server = new McpServer({
    name: SERVER_NAME,
    version: SERVER_VERSION,
  });

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
