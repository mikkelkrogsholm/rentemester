/**
 * MCP-tool: `retention_status` (read).
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import { buildRetentionStatusReport } from "../../core/retention";
import { wrapCoreResult } from "../envelope";
import { withCompanyDb } from "../helpers";

export function registerRetentionTools(server: McpServer): void {
  server.registerTool(
    "retention_status",
    {
      title: "Retention status report",
      description: "Viser opbevaringsfrister og udløbet materiale. Read-only.",
      inputSchema: {
        company: z.string().min(1),
        asOf: z.string().optional(),
      },
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDb<{ company: string; asOf?: string }>(server, ({ db, args }) => {
      const result = buildRetentionStatusReport(db, args.asOf);
      return wrapCoreResult(result);
    }),
  );
}
