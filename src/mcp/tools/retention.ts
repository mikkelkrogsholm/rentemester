/**
 * MCP-tool: `retention_status` (read).
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import { buildRetentionStatusReport } from "../../core/retention";
import { envelopeShape, wrapCoreResult } from "../envelope";
import { withCompanyDb } from "../tool-runtime";

export function registerRetentionTools(server: McpServer): void {
  server.registerTool(
    "retention_status",
    {
      title: "Retention status report",
      description: "Viser opbevaringsfrister og udløbet materiale. Read-only.",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        asOf: z.string().optional(),
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDb<{ company: string; asOf?: string }>(server, ({ db, args }) => {
      const result = buildRetentionStatusReport(db, args.asOf);
      return wrapCoreResult(result);
    }),
  );
}
