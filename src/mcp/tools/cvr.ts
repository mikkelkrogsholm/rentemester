/**
 * MCP-tools for CVR-registret.
 *
 *  - `cvr_lookup` (read; cacher snapshot fra CVR-registret)
 *  - `company_sync_cvr` (write-reversible; opdaterer companies-rækken)
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import { lookupCvrCompany } from "../../core/cvr";
import { syncCompanyFromCvr } from "../../core/company";
import { envelopeShape, wrapCoreResult } from "../envelope";
import { withCompanyDb, withCompanyDbConfirmed, confirmField } from "../tool-runtime";

export function registerCvrTools(server: McpServer): void {
  server.registerTool(
    "cvr_lookup",
    {
      title: "Look up a company in the CVR register",
      description:
        "Slår en dansk virksomhed op i CVR-registret via CVR-nummer og cacher snapshottet lokalt. Read-only. Kræver miljøvariablerne CVR_USERNAME/CVR_PASSWORD.",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        cvr: z.string().min(1),
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: true },
    },
    withCompanyDb<{ company: string; cvr: string }>(server, async ({ db, args }) => {
      const result = await lookupCvrCompany(db, args.cvr);
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "company_sync_cvr",
    {
      title: "Sync company stamdata from the CVR register",
      description:
        "Henter virksomhedens egne stamdata fra CVR-registret og opdaterer companies-rækken (navn, adresse, branche, virksomhedsform, status). Kræver confirm:true. Regnskabsåret røres aldrig; et afvigende regnskabsår rapporteres kun. write-reversible.",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        confirm: confirmField,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: true },
    },
    withCompanyDbConfirmed<{ company: string; confirm?: boolean }>(
      server,
      "company_sync_cvr",
      async ({ db }) => {
        const result = await syncCompanyFromCvr(db);
        return wrapCoreResult(result);
      },
    ),
  );
}
