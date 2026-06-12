/**
 * MCP-tools for kunder (master data).
 *
 *  - `customer_list` (read)
 *  - `customer_validate_vat` (read; cacher resultat fra VIES)
 *  - `customer_create` (write-reversible)
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import {
  createCustomer,
  listCustomers,
  customerInputFromCvr,
  type CreateCustomerInput,
} from "../../core/master-data";
import { validateVatAgainstVies } from "../../core/vies";
import { envelopeShape, successEnvelope, wrapCoreResult } from "../envelope";
import { withCompanyDb, withCompanyDbConfirmed, confirmField } from "../tool-runtime";
import { applyPagination, paginationFields, paginationDescriptionSuffix } from "../pagination";

export function registerCustomerTools(server: McpServer): void {
  server.registerTool(
    "customer_list",
    {
      title: "List customers",
      description:
        "Lister kendte kunder. Read-only. " +
        "Rækkefølge: lower(name) ASC, id ASC (deterministisk)." +
        paginationDescriptionSuffix,
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        archived: z
          .boolean()
          .optional()
          .describe("When true, list archived customers instead of active ones (default false)."),
        ...paginationFields,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDb<{ company: string; archived?: boolean; limit?: number; offset?: number }>(server, ({ db, args }) => {
      const result = listCustomers(db, { archived: args.archived === true });
      if (!result.ok) return wrapCoreResult(result);
      const { pageRows, meta } = applyPagination(result.rows, { limit: args.limit, offset: args.offset });
      return successEnvelope({ rows: pageRows, ...meta });
    }),
  );

  server.registerTool(
    "customer_validate_vat",
    {
      title: "Validate VAT number via VIES",
      description:
        "Validerer et EU-VAT-nummer mod EU-Kommissionens VIES-tjeneste og opdaterer en lokal " +
        "validerings-cache (vies_validations) med resultatet. Klassificeret read (readOnlyHint:true): " +
        "den skriver ikke bogførings- eller stamdata-state — kun en gennemsigtig opslags-cache med TTL — " +
        "og kræver derfor ikke confirm:true. Idempotent: et gentaget opslag inden for TTL genbruger cachen. " +
        "Den tilsvarende CLI-kommando `customer validate-vat` gør præcis det samme.",
      inputSchema: {
        company: z
          .string()
          .min(1)
          .describe("Absolute path to the company directory, or a workspace slug."),
        cvr: z
          .string()
          .min(1)
          .describe(
            "EU VAT number to validate, including the 2-letter country prefix, e.g. 'DK12345678' " +
              "or 'DE123456789'. Spaces and dots are tolerated.",
          ),
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: true },
    },
    withCompanyDb<{ company: string; cvr: string }>(server, async ({ db, args }) => {
      const result = await validateVatAgainstVies(db, args.cvr);
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "customer_create",
    {
      title: "Create customer",
      description:
        "Opretter en kundepost. Kræver confirm:true. Kan arkiveres eller rettes senere. Med fromCvr udfyldes felter der ikke er sat i input fra CVR-registret. write-reversible.",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        // `name` is optional only because `fromCvr` can supply it; a create
        // with neither a name nor fromCvr is rejected by createCustomer.
        input: z
          .object({
            name: z
              .string()
              .min(1)
              .optional()
              .describe("Customer name. Required unless fromCvr is given (then the CVR register supplies it)."),
            address: z.string().optional().describe("Postal address (free text)."),
            vatOrCvr: z
              .string()
              .optional()
              .describe("VAT or CVR number, e.g. 'DK12345678' (Danish) or 'DE123456789' (EU)."),
            email: z.string().optional().describe("Contact email address."),
            phone: z.string().optional().describe("Contact phone number."),
            website: z.string().optional().describe("Website URL."),
            eanNumber: z
              .string()
              .optional()
              .describe("13-digit EAN/GLN number — required to invoice this customer as a Danish public-sector recipient."),
            paymentTermsDays: z
              .number()
              .int()
              .positive()
              .optional()
              .describe("Default payment terms in days (a positive integer). Used as the invoice due-date offset."),
            defaultCurrency: z
              .string()
              .optional()
              .describe("Default 3-letter ISO currency code for this customer's invoices, e.g. 'DKK' or 'EUR'."),
            notes: z.string().optional().describe("Free-text internal notes."),
          })
          .describe("Customer master-data fields. Fields left unset are filled from the CVR register when fromCvr is supplied."),
        fromCvr: z
          .string()
          .optional()
          .describe(
            "Optional Danish CVR number. When set, every field not present in `input` is filled from the " +
              "official CVR register before the customer is created.",
          ),
        confirm: confirmField,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      input: CreateCustomerInput;
      fromCvr?: string;
      confirm?: boolean;
    }>(server, "customer_create", async ({ db, args }) => {
      let input = args.input;
      if (args.fromCvr) {
        const resolved = await customerInputFromCvr(db, args.fromCvr, args.input);
        if (!resolved.ok) return wrapCoreResult({ ok: false, errors: resolved.errors });
        input = resolved.input;
      }
      const result = createCustomer(db, input);
      return wrapCoreResult(result);
    }),
  );
}
