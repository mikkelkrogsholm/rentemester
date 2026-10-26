/**
 * MCP-tool: `company_profile_get` (read).
 *
 * Read-only access to a company's stored profile (name, CVR, address,
 * payment terms, VAT cadence, …). Mirrors the CLI's `company profile`
 * surface and the HTTP `GET /api/companies/:slug/company`.
 *
 * Without this tool an agent cannot answer simple questions like
 * "what's my CVR?" or "which payment terms am I issuing invoices with?"
 * without listing the entire workspace via `portfolio_overview` — caught
 * in the AI-agent fresh-eyes review.
 *
 * Klassifikation: `read` — ingen state-bivirkninger.
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import { getCompanySettings } from "../../core/company";
import { envelopeShape, successEnvelope } from "../envelope";
import { withCompanyDb } from "../tool-runtime";

export function registerCompanyProfileTools(server: McpServer): void {
  server.registerTool(
    "company_profile_get",
    {
      title: "Get the stored company profile",
      description:
        "Returnerer virksomhedens gemte profil-stamdata: navn, CVR, valuta, " +
        "land, adresse, regnskabsår-start, regnskabsklasse-strategi, " +
        "betalingsfrist (dage), momsperiode-kadence, revisor-fravalg og " +
        "CVR-status. Hver fakturering, momsrapport og årsrapport bygger " +
        "implicit på disse felter, så en agent bør læse dem før den udsteder " +
        "noget på virksomhedens vegne. Read-only.\n\n" +
        "envelope.data.profile har shape { id, name, country, currency, " +
        "cvr, fiscalYearStartMonth, fiscalYearLabelStrategy, address, " +
        "postalCode, city, companyForm, industryCode, industryText, " +
        "cvrStatus, auditWaived, cvrSyncedAt, paymentTermsDays, " +
        "vatPeriodType, vatRegistered, invoiceNumberFormat }. invoiceNumberFormat er 'fiscal-year' eller 'daily'. `vatPeriodType` er 'month' / " +
        "'quarter' / 'half-year', ELLER `null` når virksomheden IKKE er " +
        "momsregistreret (holdingselskab m.fl.); `vatRegistered` er den " +
        "afledte boolean (false ⇔ vatPeriodType er null) — branch på den " +
        "frem for at antage en kadence. For et ikke-momsregistreret selskab " +
        "afviser moms-rapport-værktøjerne, og købsmoms absorberes i udgiften " +
        "(non_deductible). Defaults indsættes for felter ledgeren ikke har " +
        "(ny virksomhed), så shape'en er stabil.",
      inputSchema: {
        company: z
          .string()
          .min(1)
          .describe("Absolute path to the company directory, or a workspace slug."),
      },
      outputSchema: envelopeShape,
      annotations: {
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    withCompanyDb<{ company: string }>(server, ({ db }) => {
      const profile = getCompanySettings(db);
      // Surface the derived registration boolean so an agent branches on it
      // directly instead of having to know that a null cadence means "not
      // VAT-registered".
      return successEnvelope({
        profile: { ...profile, vatRegistered: profile.vatPeriodType !== null },
      });
    }),
  );
}
