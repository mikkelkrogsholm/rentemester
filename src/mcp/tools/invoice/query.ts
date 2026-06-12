/**
 * Read-only invoice_* MCP tools: invoice_validate, invoice_status,
 * invoice_list, invoice_find, invoice_overdue, invoice_interest_calc,
 * invoice_compensation_calc.
 *
 * Split out of `../invoice.ts` (Batch G). Registration order preserved.
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import { validateInvoice, type InvoicePayload } from "../../../core/invoice";
import {
  buildInvoiceList,
  findInvoices,
  buildOverdueInvoiceList,
  type InvoiceQueryStatus,
} from "../../../core/invoice-list";
import { getInvoiceStatus } from "../../../core/invoice-payments";
import { calculateInvoiceLateInterest, proposeInterestCorrection } from "../../../core/invoice-interest";
import { calculateInvoiceLateCompensation } from "../../../core/invoice-compensation";
import { envelopeShape, wrapCoreResult } from "../../envelope";
import {
  withCompanyDb,
  resolveIssuedInvoiceDocumentId,
} from "../../tool-runtime";
import {
  invoicePayloadSchema,
  docIdOrNumberSchema,
  notFoundEnvelope,
} from "./_shared";

const statusEnum = z
  .enum(["open", "paid", "credited", "refunded", "overpaid", "written_off", "overdue", "all"])
  .optional();

export function registerInvoiceQueryTools(server: McpServer): void {
  server.registerTool(
    "invoice_validate",
    {
      title: "Validate invoice payload",
      description:
        "Validerer faktura-payload uden at gemme. Read-only. " +
        "Alle beløb er i kroner (decimal DKK, 2 decimaler — ikke øre); vatRate er en brøk (0.25 = 25%).",
      inputSchema: { payload: invoicePayloadSchema },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    async ({ payload }) => {
      const result = validateInvoice(payload as InvoicePayload);
      return {
        content: [{ type: "text" as const, text: JSON.stringify(wrapCoreResult(result)) }],
        isError: !result.ok,
        structuredContent: wrapCoreResult(result),
      };
    },
  );

  server.registerTool(
    "invoice_status",
    {
      title: "Invoice status",
      description: "Viser åben saldo og status på en faktura. Read-only.",
      inputSchema: {
        ...docIdOrNumberSchema,
        asOf: z
          .string()
          .optional()
          .describe(
            "As-of date in YYYY-MM-DD format for the status snapshot. " +
              "Defaults to today.",
          ),
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDb<{ company: string; documentId?: number; invoiceNumber?: string; asOf?: string }>(
      server,
      ({ db, args }) => {
        const id = resolveIssuedInvoiceDocumentId(db, args);
        if (!id) return notFoundEnvelope(args);
        const result = getInvoiceStatus(db, id, args.asOf);
        return wrapCoreResult(result);
      },
    ),
  );

  server.registerTool(
    "invoice_list",
    {
      title: "List invoices",
      description:
        "Lister udstedte fakturaer med filtre. Read-only. " +
        "Rækkefølge: invoice_date ASC, id ASC (deterministisk).",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        status: statusEnum.describe(
          "Filter by invoice status: 'open', 'paid', 'credited', 'refunded', " +
            "'overpaid', 'written_off', 'overdue' or 'all'. Defaults to 'all'.",
        ),
        from: z
          .string()
          .optional()
          .describe("Only invoices issued on or after this date (YYYY-MM-DD)."),
        to: z
          .string()
          .optional()
          .describe("Only invoices issued on or before this date (YYYY-MM-DD)."),
        customerCvr: z
          .string()
          .optional()
          .describe("Filter by the customer's CVR/VAT number, e.g. 'DK12345678'."),
        customer: z
          .string()
          .optional()
          .describe("Filter by a substring of the customer name."),
        invoiceNumber: z
          .string()
          .optional()
          .describe("Filter by exact invoice number, e.g. '2026-001'."),
        minAmount: z
          .number()
          .optional()
          .describe("Only invoices with a gross total at or above this amount (kroner, decimal DKK)."),
        maxAmount: z
          .number()
          .optional()
          .describe("Only invoices with a gross total at or below this amount (kroner, decimal DKK)."),
        asOf: z
          .string()
          .optional()
          .describe("As-of date (YYYY-MM-DD) used for status/balance computation. Defaults to today."),
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDb<{
      company: string;
      status?: InvoiceQueryStatus;
      from?: string;
      to?: string;
      customerCvr?: string;
      customer?: string;
      invoiceNumber?: string;
      minAmount?: number;
      maxAmount?: number;
      asOf?: string;
    }>(server, ({ db, args }) => {
      const result = buildInvoiceList(db, {
        status: args.status ?? "all",
        from: args.from,
        to: args.to,
        customerCvr: args.customerCvr,
        customer: args.customer,
        invoiceNumber: args.invoiceNumber,
        minAmount: args.minAmount,
        maxAmount: args.maxAmount,
        asOfDate: args.asOf,
      });
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "invoice_find",
    {
      title: "Find invoices",
      description:
        "Søger udstedte fakturaer på nummer, kunde eller beløb. Read-only. " +
        "Med flere filtre kombineres alle som AND. Kald uden filtre returnerer alle udstedte fakturaer. " +
        "Bemærk: 'amount' er eksakt match — brug minAmount/maxAmount til range-søgning " +
        "(fx bank-afstemning, hvor øre-afvigelser er normale).",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        query: z
          .string()
          .optional()
          .describe(
            "Fritekst-delstreng (substring, case-insensitive) der matches mod både " +
              "fakturanummer OG kundenavn. Ikke regex, ikke LIKE-wildcards.",
          ),
        customer: z
          .string()
          .optional()
          .describe("Delstreng (substring, case-insensitive) der matches mod kundenavnet."),
        invoiceNumber: z
          .string()
          .optional()
          .describe(
            "Delstreng (substring, case-insensitive) der matches mod fakturanummeret, fx '2026-001'. " +
              "Bemærk: substring, ikke eksakt — '001' matcher både '2026-001' og '2026-0010'.",
          ),
        amount: z
          .number()
          .optional()
          .describe(
            "Eksakt bruttobeløb i DKK med 2 decimaler (fx 10000.00). " +
              "Internt sættes både minAmount og maxAmount til denne værdi, så 10000 matcher KUN " +
              "fakturaer hvor brutto er præcis 10.000,00. Til bank-afstemning eller andre flows " +
              "med øre-afvigelser: brug minAmount/maxAmount i stedet.",
          ),
        minAmount: z
          .number()
          .optional()
          .describe(
            "Kun fakturaer med bruttobeløb på eller over dette beløb (DKK, 2 decimaler). " +
              "Kombinér med maxAmount for range-søgning. Ignoreres hvis 'amount' også er sat.",
          ),
        maxAmount: z
          .number()
          .optional()
          .describe(
            "Kun fakturaer med bruttobeløb på eller under dette beløb (DKK, 2 decimaler). " +
              "Kombinér med minAmount for range-søgning. Ignoreres hvis 'amount' også er sat.",
          ),
        asOf: z
          .string()
          .optional()
          .describe(
            "Skæringsdato (YYYY-MM-DD) brugt til status- og saldoberegning på de returnerede fakturaer. " +
              "Påvirker IKKE hvilke fakturaer der returneres (ingen dato-filter på udstedelse), " +
              "kun deres status/restbeløb. Default: i dag.",
          ),
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDb<{
      company: string;
      query?: string;
      customer?: string;
      invoiceNumber?: string;
      amount?: number;
      minAmount?: number;
      maxAmount?: number;
      asOf?: string;
    }>(server, ({ db, args }) => {
      const exact = args.amount;
      const result = findInvoices(db, {
        query: args.query,
        customer: args.customer,
        invoiceNumber: args.invoiceNumber,
        minAmount: exact !== undefined ? exact : args.minAmount,
        maxAmount: exact !== undefined ? exact : args.maxAmount,
        asOfDate: args.asOf,
      });
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "invoice_overdue",
    {
      title: "List overdue invoices",
      description: "Lister forfaldne udstedte fakturaer som ikke er fuldt afregnet. Read-only.",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        asOf: z.string().optional(),
        minDays: z.number().int().nonnegative().optional(),
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDb<{ company: string; asOf?: string; minDays?: number }>(server, ({ db, args }) => {
      const result = buildOverdueInvoiceList(db, { asOfDate: args.asOf, minDays: args.minDays });
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "invoice_interest_calc",
    {
      title: "Calculate invoice late interest",
      description:
        "Beregner morarente uden at registrere. Read-only. accruedInterestAmount er " +
        "den rente der kan opkræves NU — for perioden siden sidste registrerede rentekrav " +
        "(eller fra forfald hvis der ikke er noget). Felterne priorClaimedInterest og " +
        "totalInterestToDate viser allerede opkrævet hhv. samlet rente til dato.",
      inputSchema: {
        ...docIdOrNumberSchema,
        asOf: z
          .string()
          .min(1)
          .describe("As-of date in YYYY-MM-DD format the interest is calculated up to."),
        referenceRate: z
          .number()
          .describe(
            "Nationalbanken's reference rate as a percentage (e.g. 2.65 for 2.65%); " +
              "the statutory late-interest surcharge is added on top.",
          ),
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDb<{
      company: string;
      documentId?: number;
      invoiceNumber?: string;
      asOf: string;
      referenceRate: number;
    }>(server, ({ db, args }) => {
      const id = resolveIssuedInvoiceDocumentId(db, args);
      if (!id) return notFoundEnvelope(args);
      const result = calculateInvoiceLateInterest(db, {
        invoiceDocumentId: id,
        asOfDate: args.asOf,
        referenceRatePercent: args.referenceRate,
      });
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "invoice_interest_correction_calc",
    {
      title: "Propose late-interest correction",
      description:
        "Foreslår en korrektion af for meget opkrævet morarente. Read-only. Opstår når en " +
        "betaling eller kreditnota er registreret med en virkningsdato INDE i et allerede bogført " +
        "rentekravs vindue, så den lovlige dato-bevidste rente er lavere end det bogførte beløb. " +
        "Returnerer hasProposal, overClaimedAmount, postedInterest, lawfulInterest, alreadyCorrected.",
      inputSchema: { ...docIdOrNumberSchema },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDb<{ company: string; documentId?: number; invoiceNumber?: string }>(server, ({ db, args }) => {
      const id = resolveIssuedInvoiceDocumentId(db, args);
      if (!id) return notFoundEnvelope(args);
      const result = proposeInterestCorrection(db, { invoiceDocumentId: id });
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "invoice_compensation_calc",
    {
      title: "Calculate invoice late compensation",
      description: "Beregner kompensationskrav for sen betaling uden at registrere. Read-only.",
      inputSchema: {
        ...docIdOrNumberSchema,
        asOf: z
          .string()
          .min(1)
          .describe("As-of date in YYYY-MM-DD format the compensation is calculated up to."),
        amountDkk: z
          .number()
          .optional()
          .describe(
            "Optional fixed compensation amount in kroner (decimal DKK). When " +
              "omitted, the statutory DKK 310 late-payment compensation is used.",
          ),
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDb<{
      company: string;
      documentId?: number;
      invoiceNumber?: string;
      asOf: string;
      amountDkk?: number;
    }>(server, ({ db, args }) => {
      const id = resolveIssuedInvoiceDocumentId(db, args);
      if (!id) return notFoundEnvelope(args);
      const result = calculateInvoiceLateCompensation(db, {
        invoiceDocumentId: id,
        asOfDate: args.asOf,
        compensationAmountDkk: args.amountDkk,
      });
      return wrapCoreResult(result);
    }),
  );
}
