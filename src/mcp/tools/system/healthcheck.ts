/**
 * `system_healthcheck` MCP tool — verifies the company directory layout.
 *
 * Split out of `../system.ts` (mechanical split, no behavior change). The
 * `registerSystemHealthcheckTools` helper below is invoked by the
 * `registerSystemTools` composer in `../system.ts` in the original order.
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import { existsSync } from "node:fs";
import { companyPaths } from "../../../core/paths";
import { envelopeShape, errorEnvelope, successEnvelope } from "../../envelope";

export function registerSystemHealthcheckTools(server: McpServer): void {
  server.registerTool(
    "system_healthcheck",
    {
      title: "Company directory healthcheck",
      description:
        "Tjekker at virksomhedsmappen og kernefilerne findes — company_root, " +
        "data_dir, ledger, documents, config. Returnerer envelope.data.checks " +
        "med { name, ok } pr. tjek og envelope.data.missing med navnene på " +
        "dem der mangler. Read-only.",
      inputSchema: {
        company: z
          .string()
          .min(1)
          .describe("Absolute path to the company directory, or a workspace slug."),
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    async ({ company }: { company: string }) => {
      if (typeof company !== "string" || company.length === 0) {
        const env = errorEnvelope("company path is required");
        return {
          content: [{ type: "text" as const, text: JSON.stringify(env) }],
          isError: true,
          structuredContent: env,
        };
      }
      const p = companyPaths(company);
      const checks: Array<{ name: string; ok: boolean }> = [
        { name: "company_root", ok: existsSync(p.root) },
        { name: "data_dir", ok: existsSync(p.data) },
        { name: "ledger", ok: existsSync(p.db) },
        { name: "documents", ok: existsSync(p.documentsInbox) },
        { name: "config", ok: existsSync(p.config) },
      ];
      const missing = checks.filter((c) => !c.ok).map((c) => c.name);
      const env =
        missing.length === 0
          ? successEnvelope({ ok: true, missing: [], checks })
          : { ...errorEnvelope(missing.map((m) => `missing: ${m}`)), data: { ok: false, missing, checks } };
      return {
        content: [{ type: "text" as const, text: JSON.stringify(env) }],
        isError: !env.ok,
        structuredContent: env,
      };
    },
  );
}
