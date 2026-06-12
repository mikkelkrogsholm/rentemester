/**
 * MCP tools for budgeting and the liquidity (cash-flow) forecast.
 *
 * 1:1 mapping of the CLI commands `budget set|list|vs-actual|forecast`:
 *  - budget_set        — append a budget revision for a konto/periode
 *  - budget_list       — read the effective budget lines
 *  - budget_vs_actual  — compare budget against the real ledger per month
 *  - budget_forecast   — project the bank balance forward per month
 *
 * The forecast is purely deterministic arithmetic over known rows (open
 * invoices, recurring templates, budgeted costs) — no statistical model.
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import { setBudget, listBudget, buildBudgetVsActual } from "../../core/budget";
import { buildLiquidityForecast } from "../../core/liquidity-forecast";
import { envelopeShape, wrapCoreResult } from "../envelope";
import { withCompanyDb, withCompanyDbConfirmed, confirmField } from "../tool-runtime";

export function registerBudgetTools(server: McpServer): void {
  server.registerTool(
    "budget_set",
    {
      title: "Set budget for an account in a period",
      description:
        "Sætter et budget for én konto i én kalendermåned (YYYY-MM). Budgetlinjer er append-only — hvert kald tilføjer en revision, seneste vinder. write-reversible.",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        accountNo: z
          .string()
          .min(1)
          .describe("Account number the budget applies to — must exist in the chart of accounts."),
        period: z
          .string()
          .min(1)
          .describe("Calendar month in YYYY-MM form, e.g. '2026-06'."),
        amount: z
          .number()
          .nonnegative()
          .describe("Budgeted amount in kroner (decimal DKK) in the account's natural sign — never negative."),
        notes: z.string().optional().describe("Optional free-text note stored on the budget line."),
        confirm: confirmField,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      accountNo: string;
      period: string;
      amount: number;
      notes?: string;
      confirm?: boolean;
    }>(server, "budget_set", ({ db, args }) => {
      const result = setBudget(db, {
        accountNo: args.accountNo,
        period: args.period,
        amount: args.amount,
        notes: args.notes,
      });
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "budget_list",
    {
      title: "List budget lines",
      description:
        "Lister de gældende (seneste-revision) budgetlinjer, valgfrit filtreret på periode eller konto. read-only.",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        period: z
          .string()
          .optional()
          .describe("Optional YYYY-MM filter — only budget lines for this calendar month."),
        accountNo: z
          .string()
          .optional()
          .describe("Optional account-number filter — only budget lines for this account."),
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDb<{ company: string; period?: string; accountNo?: string }>(server, ({ db, args }) => {
      const result = listBudget(db, { period: args.period, accountNo: args.accountNo });
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "budget_vs_actual",
    {
      title: "Budget vs actual report",
      description:
        "Sammenligner budget mod faktisk bogføring pr. konto pr. måned i [from, to]. Afvigelse = budget − faktisk for udgift, faktisk − budget for indtægt. read-only.",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        from: z.string().min(1).describe("First calendar month of the range, YYYY-MM."),
        to: z.string().min(1).describe("Last calendar month of the range, YYYY-MM. Must be >= from."),
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDb<{ company: string; from: string; to: string }>(server, ({ db, args }) => {
      const result = buildBudgetVsActual(db, args.from, args.to);
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "budget_forecast",
    {
      title: "Liquidity (cash-flow) forecast",
      description:
        "Fremskriver banksaldoen måned for måned: primosaldo + åbne fakturaer der forfalder + planlagte gentagne fakturaer + budgetterede omkostninger = projiceret ultimosaldo pr. periode. Rent deterministisk aritmetik. read-only.",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        startDate: z
          .string()
          .min(1)
          .describe("First day of the first month to project, YYYY-MM-DD."),
        months: z
          .number()
          .int()
          .min(1)
          .max(18)
          .describe("Number of consecutive monthly periods to project (1-18)."),
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDb<{ company: string; startDate: string; months: number }>(server, ({ db, args }) => {
      const result = buildLiquidityForecast(db, { startDate: args.startDate, months: args.months });
      return wrapCoreResult(result);
    }),
  );
}
