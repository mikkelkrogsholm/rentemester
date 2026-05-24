/**
 * MCP-tools for leverandører (master data).
 *
 *  - `vendor_list` (read)
 *  - `vendor_create` (write-reversible)
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import {
  createVendor,
  listVendors,
  vendorInputFromCvr,
  type CreateVendorInput,
} from "../../core/master-data";
import { envelopeShape, successEnvelope, wrapCoreResult } from "../envelope";
import { withCompanyDb, withCompanyDbConfirmed, confirmField } from "../tool-runtime";
import { applyPagination, paginationFields, paginationDescriptionSuffix } from "../pagination";

export function registerVendorTools(server: McpServer): void {
  server.registerTool(
    "vendor_list",
    {
      title: "List vendors",
      description: "Lister kendte leverandører. Read-only." + paginationDescriptionSuffix,
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        archived: z
          .boolean()
          .optional()
          .describe("When true, list archived vendors instead of active ones (default false)."),
        ...paginationFields,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDb<{ company: string; archived?: boolean; limit?: number; offset?: number }>(server, ({ db, args }) => {
      const result = listVendors(db, { archived: args.archived === true });
      if (!result.ok) return wrapCoreResult(result);
      const { pageRows, meta } = applyPagination(result.rows, { limit: args.limit, offset: args.offset });
      return successEnvelope({ rows: pageRows, ...meta });
    }),
  );

  server.registerTool(
    "vendor_create",
    {
      title: "Create vendor",
      description:
        "Opretter en leverandørpost. Kræver confirm:true. Kan arkiveres eller rettes senere. Med fromCvr udfyldes felter der ikke er sat i input fra CVR-registret. write-reversible.",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        // `name` is optional only because `fromCvr` can supply it; a create
        // with neither a name nor fromCvr is rejected by createVendor.
        input: z
          .object({
            name: z
              .string()
              .min(1)
              .optional()
              .describe("Vendor name. Required unless fromCvr is given (then the CVR register supplies it)."),
            address: z.string().optional().describe("Postal address (free text)."),
            vatOrCvr: z
              .string()
              .optional()
              .describe("VAT or CVR number, e.g. 'DK12345678' (Danish) or 'DE123456789' (EU)."),
            email: z.string().optional().describe("Contact email address."),
            phone: z.string().optional().describe("Contact phone number."),
            website: z.string().optional().describe("Website URL."),
            defaultExpenseAccount: z
              .string()
              .optional()
              .describe("Default expense account number from the chart of accounts (e.g. '3000') for this vendor's bills."),
            defaultVatTreatment: z
              .string()
              .optional()
              .describe(
                "Default VAT treatment for this vendor's purchases — one of 'standard', " +
                  "'domestic_reverse_charge' or 'foreign_reverse_charge'.",
              ),
            notes: z.string().optional().describe("Free-text internal notes."),
          })
          .describe("Vendor master-data fields. Fields left unset are filled from the CVR register when fromCvr is supplied."),
        fromCvr: z
          .string()
          .optional()
          .describe(
            "Optional Danish CVR number. When set, every field not present in `input` is filled from the " +
              "official CVR register before the vendor is created.",
          ),
        confirm: confirmField,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      input: CreateVendorInput;
      fromCvr?: string;
      confirm?: boolean;
    }>(server, "vendor_create", async ({ db, args }) => {
      let input = args.input;
      if (args.fromCvr) {
        const resolved = await vendorInputFromCvr(db, args.fromCvr, args.input);
        if (!resolved.ok) return wrapCoreResult({ ok: false, errors: resolved.errors });
        input = resolved.input;
      }
      const result = createVendor(db, input);
      return wrapCoreResult(result);
    }),
  );
}
