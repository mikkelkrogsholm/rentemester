/**
 * Central tools-registrering for Rentemester-MCP-serveren.
 *
 * Scaffold-version (issue #77): kun to tools — ét read, ét write —
 * for at verificere ende-til-ende-flowet (stdio → MCP → kerne →
 * envelope → MCP). Resten af tool-surface'en (jf. #76) implementeres
 * i #78 ved at tilføje flere `register*Tools(server)`-kald her.
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { registerAuditTools } from "./tools/audit";
import { registerJournalTools } from "./tools/journal";

export function registerAllTools(server: McpServer): void {
  registerAuditTools(server);
  registerJournalTools(server);
}
