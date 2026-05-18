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
  type CreateCustomerInput,
} from "../../core/master-data";
import { validateVatAgainstVies } from "../../core/vies";
import { wrapCoreResult } from "../envelope";
import { withCompanyDb, withCompanyDbConfirmed } from "../helpers";

export function registerCustomerTools(server: McpServer): void {
  server.registerTool(
    "customer_list",
    {
      title: "List customers",
      description: "Lister kendte kunder. Read-only.",
      inputSchema: {
        company: z.string().min(1),
        archived: z.boolean().optional(),
      },
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDb<{ company: string; archived?: boolean }>(server, ({ db, args }) => {
      const result = listCustomers(db, { archived: args.archived === true });
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "customer_validate_vat",
    {
      title: "Validate VAT number via VIES",
      description: "Validerer et EU-VAT-nummer mod VIES og cacher resultatet.",
      inputSchema: {
        company: z.string().min(1),
        cvr: z.string().min(1),
      },
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
        "Opretter en append-only kundepost. write-reversible — kræver confirm:true. Kan arkiveres senere.",
      inputSchema: {
        company: z.string().min(1),
        input: z.object({
          name: z.string().min(1),
          address: z.string().optional(),
          vatOrCvr: z.string().optional(),
          email: z.string().optional(),
          eanNumber: z.string().optional(),
          paymentTermsDays: z.number().int().positive().optional(),
          defaultCurrency: z.string().optional(),
          notes: z.string().optional(),
        }),
        confirm: z.boolean(),
      },
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{ company: string; input: CreateCustomerInput; confirm: boolean }>(
      server,
      "customer_create",
      ({ db, args }) => {
        const result = createCustomer(db, args.input);
        return wrapCoreResult(result);
      },
    ),
  );
}
