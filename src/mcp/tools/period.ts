/**
 * MCP-tools for regnskabsperioder.
 *
 *  - `period_close` (write-irreversible) — lukker eller markerer en periode
 *  - `period_list` (read) — lister kendte regnskabsperioder. Bemærk: CLI har
 *    ikke en dedikeret `period list` endnu (jf. docs/mcp-tool-surface.md);
 *    vi eksponerer her tabellen direkte fra MCP for at give agenter et
 *    minimums-view uden at gå udenom kerne-laget.
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import {
  closeAccountingPeriod,
  type AccountingPeriodKind,
} from "../../core/periods";
import { wrapCoreResult, successEnvelope } from "../envelope";
import { withCompanyDb, withCompanyDbConfirmed } from "../helpers";

export function registerPeriodTools(server: McpServer): void {
  server.registerTool(
    "period_list",
    {
      title: "List accounting periods",
      description: "Lister regnskabsperioder (open/closed/reported). Read-only.",
      inputSchema: { company: z.string().min(1) },
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDb<{ company: string }>(server, ({ db }) => {
      const rows = db
        .query(
          `SELECT id, period_start, period_end, kind, status, reference, created_at
           FROM accounting_periods
           ORDER BY period_start DESC, id DESC`,
        )
        .all() as Array<{
          id: number;
          period_start: string;
          period_end: string;
          kind: string;
          status: string;
          reference: string | null;
          created_at: string;
        }>;
      return successEnvelope({
        periods: rows.map((row) => ({
          id: row.id,
          periodStart: row.period_start,
          periodEnd: row.period_end,
          kind: row.kind,
          status: row.status,
          reference: row.reference,
          createdAt: row.created_at,
        })),
        count: rows.length,
      });
    }),
  );

  server.registerTool(
    "period_close",
    {
      title: "Close accounting period",
      description: "Lukker eller markerer regnskabsperiode (closed/reported). write-irreversible.",
      inputSchema: {
        company: z.string().min(1),
        from: z.string().min(1),
        to: z.string().min(1),
        kind: z.enum(["vat_quarter", "fiscal_year", "custom"]).optional(),
        status: z.enum(["closed", "reported"]).optional(),
        reference: z.string().optional(),
        confirm: z.boolean(),
      },
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      from: string;
      to: string;
      kind?: AccountingPeriodKind;
      status?: "closed" | "reported";
      reference?: string;
      confirm: boolean;
    }>(server, "period_close", ({ db, args }) => {
      const result = closeAccountingPeriod(db, {
        periodStart: args.from,
        periodEnd: args.to,
        kind: args.kind,
        status: args.status,
        reference: args.reference,
      });
      return wrapCoreResult(result);
    }),
  );
}
