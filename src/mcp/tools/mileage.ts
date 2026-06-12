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
import { envelopeShape, wrapCoreResult } from "../envelope";
import { withCompanyDb, withCompanyDbConfirmed, confirmField } from "../tool-runtime";

export function registerMileageTools(server: McpServer): void {
  server.registerTool(
    "mileage_list",
    {
      title: "List mileage entries",
      description:
        "Lister registrerede kørselsposter (kørselsregnskab). Read-only. " +
        "Rækkefølge: travel_date DESC, id DESC (nyeste først, deterministisk).",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
      },
      outputSchema: envelopeShape,
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
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        from: z.string().min(1).describe("Start of the report period (inclusive), in YYYY-MM-DD format."),
        to: z.string().min(1).describe("End of the report period (inclusive), in YYYY-MM-DD format."),
      },
      outputSchema: envelopeShape,
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
        "Tilføjer en append-only kørselspost til kørselsregnskabet. Kræver confirm:true. " +
        "ratePerKm/rateBasis skal være bruger-oplyst og kilde-bakket; Rentemester fører kun loggen, " +
        "skattemæssig behandling er brugerens/rådgiverens ansvar. write-reversible.",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        input: z
          .object({
            tripDate: z.string().min(1).describe("Date the trip was driven, in YYYY-MM-DD format."),
            purpose: z.string().min(1).describe("Business purpose of the trip (free text)."),
            fromLocation: z.string().min(1).describe("Where the trip started (free text, e.g. an address or place name)."),
            toLocation: z.string().min(1).describe("Where the trip ended (free text, e.g. an address or place name)."),
            kilometers: z
              .number()
              .positive()
              .describe("Distance driven, in kilometres (a positive number, not metres or miles)."),
            vehicle: z.string().min(1).describe("Identifier of the vehicle used, e.g. a registration number."),
            driver: z.string().min(1).describe("Name of the person who drove the trip."),
            ratePerKm: z
              .number()
              .positive()
              .describe(
                "Per-kilometre rate applied, in kroner per km (decimal DKK). User-supplied — Rentemester " +
                  "does not own the tax rate; it only records what you confirm.",
              ),
            rateBasis: z
              .string()
              .min(1)
              .describe(
                "Source-backed basis for ratePerKm that you confirm, e.g. which official rate table or " +
                  "Skattestyrelsen circular the rate comes from.",
              ),
            rateSource: z
              .string()
              .optional()
              .describe("Optional citation or URL documenting the rate basis."),
            notes: z.string().optional().describe("Optional free-text notes about the trip."),
          })
          .describe("Mileage entry. This is audit/documentation data only — nothing is posted to the ledger."),
        confirm: confirmField,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{ company: string; input: CreateMileageEntryInput; confirm?: boolean }>(
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
        "Kræver confirm:true. write-reversible.",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        from: z.string().min(1).describe("Start of the export period (inclusive), in YYYY-MM-DD format."),
        to: z.string().min(1).describe("End of the export period (inclusive), in YYYY-MM-DD format."),
        outputDir: z
          .string()
          .min(1)
          .describe(
            "Server-side directory path where the JSON + CSV export artifacts are written. " +
              "This path is on the MCP server's host, not the caller's machine.",
          ),
        confirm: confirmField,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{ company: string; from: string; to: string; outputDir: string; confirm?: boolean }>(
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
