/**
 * MCP-tools for banktransaktioner.
 *
 *  - `bank_list` (read) — lister importerede transaktioner med filtre
 *  - `bank_suggest_matches` (read) — foreslår deterministiske match
 *  - `reconcile_bank` (read) — bygger afstemningsrapport for periode
 *  - `bank_import` (write-reversible) — importerer CSV
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  importBankCsv,
  type BankImportResult,
} from "../../core/bank";
import {
  listBankTransactions,
  buildBankReconciliationReport,
} from "../../core/reconciliation";
import { suggestBankMatches } from "../../core/bank-suggest-matches";
import { syncUnmatchedBankTransactionExceptions } from "../../core/exceptions";
import { wrapCoreResult, successEnvelope } from "../envelope";
import { withCompanyDb, withCompanyDbConfirmed } from "../helpers";

const statusSchema = z.enum(["all", "matched", "unmatched"]).optional();

export function registerBankTools(server: McpServer): void {
  server.registerTool(
    "bank_list",
    {
      title: "List bank transactions",
      description:
        "Lister importerede banktransaktioner med valgfri filtre på status, dato, tekstmatch og beløb. Read-only.",
      inputSchema: {
        company: z.string().min(1),
        status: statusSchema,
        from: z.string().optional(),
        to: z.string().optional(),
        textMatch: z.string().optional(),
        amount: z.number().optional(),
      },
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDb<{
      company: string;
      status?: "all" | "matched" | "unmatched";
      from?: string;
      to?: string;
      textMatch?: string;
      amount?: number;
    }>(server, ({ db, args }) => {
      const result = listBankTransactions(db, {
        status: args.status,
        from: args.from,
        to: args.to,
        textMatch: args.textMatch,
        amount: args.amount,
      });
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "bank_suggest_matches",
    {
      title: "Suggest bank-transaction matches",
      description:
        "Foreslår deterministiske match mellem uafstemte banktransaktioner og fakturaer/bilag. Read-only.",
      inputSchema: {
        company: z.string().min(1),
        bankTransactionId: z.number().int().positive().optional(),
        max: z.number().int().positive().optional(),
      },
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDb<{ company: string; bankTransactionId?: number; max?: number }>(
      server,
      ({ db, args }) => {
        const result = suggestBankMatches(db, {
          bankTransactionId: args.bankTransactionId,
          max: args.max,
        });
        return wrapCoreResult(result);
      },
    ),
  );

  server.registerTool(
    "reconcile_bank",
    {
      title: "Bank reconciliation report",
      description:
        "Bygger en bank-afstemningsrapport for en periode med valgfri status/tekst/beløb-filtre. Read-only.",
      inputSchema: {
        company: z.string().min(1),
        from: z.string().min(1),
        to: z.string().min(1),
        status: statusSchema,
        textMatch: z.string().optional(),
        amount: z.number().optional(),
      },
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDb<{
      company: string;
      from: string;
      to: string;
      status?: "all" | "matched" | "unmatched";
      textMatch?: string;
      amount?: number;
    }>(server, ({ db, args }) => {
      const result = buildBankReconciliationReport(db, args.from, args.to, {
        status: args.status,
        textMatch: args.textMatch,
        amount: args.amount,
      });
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "bank_import",
    {
      title: "Import bank CSV",
      description:
        "Importerer banktransaktioner fra CSV. write-reversible — kræver confirm:true. " +
        "Send enten csvPath (absolut sti) eller csvContent (rå CSV-tekst).",
      inputSchema: {
        company: z.string().min(1),
        csvPath: z.string().optional(),
        csvContent: z.string().optional(),
        confirm: z.boolean(),
      },
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{ company: string; csvPath?: string; csvContent?: string; confirm: boolean }>(
      server,
      "bank_import",
      ({ db, args }) => {
        let path = args.csvPath;
        let tmpDir: string | null = null;
        if (!path) {
          if (typeof args.csvContent !== "string" || args.csvContent.length === 0) {
            return wrapCoreResult({
              ok: false,
              errors: ["either csvPath or csvContent is required"],
            } as BankImportResult);
          }
          tmpDir = mkdtempSync(join(tmpdir(), "rentemester-mcp-bank-"));
          path = join(tmpDir, "bank-import.csv");
          writeFileSync(path, args.csvContent, "utf8");
        }
        const result = importBankCsv(db, args.company, path);
        const sync = result.ok
          ? syncUnmatchedBankTransactionExceptions(db)
          : { ok: true, created: 0, errors: [] };
        return wrapCoreResult({
          ...(result as Record<string, unknown>),
          exceptionsCreated: sync.created,
        } as unknown as BankImportResult & { exceptionsCreated: number });
      },
    ),
  );
}
