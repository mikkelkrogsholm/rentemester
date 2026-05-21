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
import { wrapCoreResult } from "../envelope";
import { withCompanyDb, withCompanyDbConfirmed } from "../tool-runtime";

export function registerVendorTools(server: McpServer): void {
  server.registerTool(
    "vendor_list",
    {
      title: "List vendors",
      description: "Lister kendte leverandører. Read-only.",
      inputSchema: {
        company: z.string().min(1),
        archived: z.boolean().optional(),
      },
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDb<{ company: string; archived?: boolean }>(server, ({ db, args }) => {
      const result = listVendors(db, { archived: args.archived === true });
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "vendor_create",
    {
      title: "Create vendor",
      description:
        "Opretter en leverandørpost. write-reversible — kræver confirm:true. Kan arkiveres eller rettes senere. Med fromCvr udfyldes felter der ikke er sat i input fra CVR-registret.",
      inputSchema: {
        company: z.string().min(1),
        // `name` is optional only because `fromCvr` can supply it; a create
        // with neither a name nor fromCvr is rejected by createVendor.
        input: z.object({
          name: z.string().min(1).optional(),
          address: z.string().optional(),
          vatOrCvr: z.string().optional(),
          email: z.string().optional(),
          phone: z.string().optional(),
          website: z.string().optional(),
          defaultExpenseAccount: z.string().optional(),
          defaultVatTreatment: z.string().optional(),
          notes: z.string().optional(),
        }),
        fromCvr: z.string().optional(),
        confirm: z.boolean().optional(),
      },
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      input: CreateVendorInput;
      fromCvr?: string;
      confirm: boolean;
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
