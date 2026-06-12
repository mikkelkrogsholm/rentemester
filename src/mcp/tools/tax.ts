/**
 * MCP-tool for selskabsskat / oplysningsskema-forberedelse.
 *
 *  - `tax_return_prepare` (read)
 *
 * Forbereder selskabets skattepligtige indkomst (oplysningsskema) for et
 * lukket regnskabsår — årets resultat plus de skattemæssige reguleringer
 * systemet kan se deterministisk, samt 22% selskabsskat for et ApS. Det er en
 * bevidst smal FØRSTE SLICE: ikke-deterministiske poster (skattemæssige
 * afskrivninger, fremført underskud, andre selskabsformer) markeres som
 * needs-review og beregnes ikke. Ikke en TastSelv-integration.
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import { buildTaxReturn } from "../../core/tax-return";
import { envelopeShape, wrapCoreResult } from "../envelope";
import { withCompanyDb } from "../tool-runtime";

export function registerTaxTools(server: McpServer): void {
  server.registerTool(
    "tax_return_prepare",
    {
      title: "Prepare corporate tax return figures (oplysningsskema)",
      description:
        "Forbereder selskabets skattepligtige indkomst for et lukket regnskabsår: " +
        "årets resultat + deterministiske skattemæssige reguleringer + 22% selskabsskat (kun ApS). " +
        "Ikke-deterministiske poster markeres som needs-review. Forudsætter et lukket regnskabsår " +
        "med registreret CVR og balancerede bøger. Read-only.",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        from: z.string().min(1).describe("Fiscal-year start date in YYYY-MM-DD format."),
        to: z.string().min(1).describe("Fiscal-year end date in YYYY-MM-DD format."),
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDb<{ company: string; from: string; to: string }>(server, ({ db, args }) => {
      const result = buildTaxReturn(db, args.from, args.to);
      return wrapCoreResult(result);
    }),
  );
}
