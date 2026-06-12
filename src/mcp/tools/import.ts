/**
 * MCP-tools for import-arkivet (#197).
 *
 *  - `import_archive_list` (read) — lister de arkiverede regnskabsår fra et
 *    flerårigt eksport (fx Dinero). Kun overskrifter pr. år.
 *  - `import_archive_year` (read) — henter ét arkiveret års fulde
 *    Posteringer / SaldoBalance som reference- og matchnings-kontekst.
 *
 * Arkivet er READ-ONLY referencedata UDEN for hovedbogen: de pre-cut-over år i
 * et flerårigt eksport bogføres aldrig i den hash-kædede journal. Kun cut-over
 * året lander i hovedbogen (#194). Disse tools giver agenten et audit- og
 * matchnings-view af de tidligere år uden at gå udenom kerne-laget.
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import { queryArchive } from "../../core/import/dinero-archive";
import { envelopeShape, wrapCoreResult } from "../envelope";
import { withCompanyDb } from "../tool-runtime";

export function registerImportTools(server: McpServer): void {
  server.registerTool(
    "import_archive_list",
    {
      title: "List archived fiscal years",
      description:
        "Lister de pre-cut-over regnskabsår der er arkiveret fra et flerårigt eksport. Read-only referencedata uden for hovedbogen.",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        sourceSystem: z.string().min(1).optional(),
      },
      outputSchema: envelopeShape,
      annotations: {
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    withCompanyDb<{ company: string; sourceSystem?: string }>(server, ({ db, args }) => {
      const result = queryArchive(db, { sourceSystem: args.sourceSystem });
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "import_archive_year",
    {
      title: "Read an archived fiscal year",
      description:
        "Henter ét arkiveret regnskabsårs fulde Posteringer og SaldoBalance som audit- og matchnings-kontekst. Read-only.",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        fiscalYear: z.number().int(),
        sourceSystem: z.string().min(1).optional(),
      },
      outputSchema: envelopeShape,
      annotations: {
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    withCompanyDb<{ company: string; fiscalYear: number; sourceSystem?: string }>(
      server,
      ({ db, args }) => {
        const result = queryArchive(db, {
          sourceSystem: args.sourceSystem,
          fiscalYear: args.fiscalYear,
        });
        return wrapCoreResult(result);
      },
    ),
  );
}
