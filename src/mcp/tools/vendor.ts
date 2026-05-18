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
  type CreateVendorInput,
} from "../../core/master-data";
import { wrapCoreResult } from "../envelope";
import { withCompanyDb, withCompanyDbConfirmed } from "../helpers";

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
        "Opretter en append-only leverandørpost. write-reversible — kræver confirm:true.",
      inputSchema: {
        company: z.string().min(1),
        input: z.object({
          name: z.string().min(1),
          address: z.string().optional(),
          vatOrCvr: z.string().optional(),
          defaultExpenseAccount: z.string().optional(),
          defaultVatTreatment: z.string().optional(),
          notes: z.string().optional(),
        }),
        confirm: z.boolean(),
      },
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{ company: string; input: CreateVendorInput; confirm: boolean }>(
      server,
      "vendor_create",
      ({ db, args }) => {
        const result = createVendor(db, args.input);
        return wrapCoreResult(result);
      },
    ),
  );
}
