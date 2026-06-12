/**
 * `system_export_authority` MCP tool — bundle material for an authority handover.
 *
 * Split out of `../system.ts` (mechanical split, no behavior change).
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import { exportAuthorityPackage } from "../../../core/authority-export";
import { envelopeShape, wrapCoreResult } from "../../envelope";
import { withCompanyDbConfirmed, confirmField } from "../../tool-runtime";

export function registerSystemAuthorityTools(server: McpServer): void {
  server.registerTool(
    "system_export_authority",
    {
      title: "Export authority package",
      description: "Eksporterer materiale til myndighedsudlevering. write-irreversible.",
      inputSchema: {
        company: z
          .string()
          .min(1)
          .describe("Absolute path to the company directory, or a workspace slug."),
        from: z
          .string()
          .min(1)
          .describe("Start of the export period (inclusive), in YYYY-MM-DD format."),
        to: z
          .string()
          .min(1)
          .describe("End of the export period (inclusive), in YYYY-MM-DD format. Must not be before `from`."),
        out: z
          .string()
          .min(1)
          .describe(
            "Output directory path ON THE MCP SERVER'S FILESYSTEM where the authority " +
              "export package is written. Created if it does not exist.",
          ),
        requestedAt: z
          .string()
          .optional()
          .describe("Optional ISO-8601 timestamp for when the authority requested the material (default: now)."),
        requester: z
          .string()
          .optional()
          .describe("Optional name of the requesting authority/person, recorded in the export manifest."),
        confirm: confirmField,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      from: string;
      to: string;
      out: string;
      requestedAt?: string;
      requester?: string;
      confirm?: boolean;
    }>(server, "system_export_authority", ({ db, args }) => {
      const result = exportAuthorityPackage(db, args.company, {
        periodStart: args.from,
        periodEnd: args.to,
        outputDir: args.out,
        requestedAt: args.requestedAt,
        requester: args.requester,
      });
      return wrapCoreResult(result);
    }),
  );
}
