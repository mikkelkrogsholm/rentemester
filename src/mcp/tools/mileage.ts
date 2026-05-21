/**
 * MCP-tools for kørselsregnskab (mileage log, #123).
 *
 *  - `mileage_list` (read)
 *  - `mileage_report` (read; deterministisk periode-rapport)
 *  - `mileage_log` (write-reversible — tilføjer en append-only kørselspost)
 *  - `mileage_export` (write-reversible — skriver et deterministisk eksport-artifact)
 *
 * Mileage entries er dokumentation/audit-data: intet bogføres i finansen.
 * Kilometer-satsen er altid bruger-oplyst og kilde-bakket (rateBasis).
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import {
  buildMileagePeriodReport,
  createMileageEntry,
  exportMileageLog,
  listMileageEntries,
  type CreateMileageEntryInput,
} from "../../core/mileage";
import { wrapCoreResult } from "../envelope";
import { withCompanyDb, withCompanyDbConfirmed } from "../tool-runtime";

export function registerMileageTools(server: McpServer): void {
  server.registerTool(
    "mileage_list",
    {
      title: "List mileage entries",
      description: "Lister registrerede kørselsposter (kørselsregnskab). Read-only.",
      inputSchema: { company: z.string().min(1) },
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDb<{ company: string }>(server, ({ db }) => {
      return wrapCoreResult(listMileageEntries(db));
    }),
  );

  server.registerTool(
    "mileage_report",
    {
      title: "Mileage period report",
      description:
        "Deterministisk periode-rapport over kilometer og beløbsgrundlag for et datointerval. Read-only.",
      inputSchema: {
        company: z.string().min(1),
        from: z.string().min(1),
        to: z.string().min(1),
      },
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDb<{ company: string; from: string; to: string }>(server, ({ db, args }) => {
      return wrapCoreResult(buildMileagePeriodReport(db, { from: args.from, to: args.to }));
    }),
  );

  server.registerTool(
    "mileage_log",
    {
      title: "Log a mileage entry",
      description:
        "Tilføjer en append-only kørselspost til kørselsregnskabet. write-reversible — kræver confirm:true. " +
        "ratePerKm/rateBasis skal være bruger-oplyst og kilde-bakket; Rentemester fører kun loggen, " +
        "skattemæssig behandling er brugerens/rådgiverens ansvar.",
      inputSchema: {
        company: z.string().min(1),
        input: z.object({
          tripDate: z.string().min(1),
          purpose: z.string().min(1),
          fromLocation: z.string().min(1),
          toLocation: z.string().min(1),
          kilometers: z.number().positive(),
          vehicle: z.string().min(1),
          driver: z.string().min(1),
          ratePerKm: z.number().positive(),
          rateBasis: z.string().min(1),
          rateSource: z.string().optional(),
          notes: z.string().optional(),
        }),
        confirm: z.boolean().optional(),
      },
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{ company: string; input: CreateMileageEntryInput; confirm: boolean }>(
      server,
      "mileage_log",
      ({ db, args }) => {
        return wrapCoreResult(createMileageEntry(db, args.input));
      },
    ),
  );

  server.registerTool(
    "mileage_export",
    {
      title: "Export mileage log",
      description:
        "Skriver et deterministisk eksport-artifact (JSON + CSV) over kørselsregnskabet for en periode. " +
        "write-reversible — kræver confirm:true.",
      inputSchema: {
        company: z.string().min(1),
        from: z.string().min(1),
        to: z.string().min(1),
        outputDir: z.string().min(1),
        confirm: z.boolean().optional(),
      },
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{ company: string; from: string; to: string; outputDir: string; confirm: boolean }>(
      server,
      "mileage_export",
      ({ db, args }) => {
        return wrapCoreResult(
          exportMileageLog(db, { from: args.from, to: args.to, outputDir: args.outputDir }),
        );
      },
    ),
  );
}
