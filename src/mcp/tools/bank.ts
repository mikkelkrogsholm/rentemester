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
  resolveBankAccount,
  type BankImportResult,
} from "../../core/bank";
import {
  listBankTransactions,
  buildBankReconciliationReport,
} from "../../core/reconciliation";
import { suggestBankMatches } from "../../core/bank-suggest-matches";
import { syncUnmatchedBankTransactionExceptions } from "../../core/exceptions";
import { envelopeShape, successEnvelope, wrapCoreResult } from "../envelope";
import { withCompanyDb, withCompanyDbConfirmed, confirmField } from "../tool-runtime";
import { applyPagination, paginationFields, paginationDescriptionSuffix } from "../pagination";

const statusSchema = z.enum(["all", "matched", "unmatched"]).optional();

export function registerBankTools(server: McpServer): void {
  server.registerTool(
    "bank_list",
    {
      title: "List bank transactions",
      description:
        "Lister importerede banktransaktioner med valgfri filtre på status, dato, tekstmatch og beløb. Read-only." +
        paginationDescriptionSuffix,
      inputSchema: {
        company: z.string().min(1),
        status: statusSchema.describe(
          "Filter by reconciliation status: 'all' (default), 'matched' or 'unmatched'.",
        ),
        from: z
          .string()
          .optional()
          .describe("Only transactions dated on or after this date (YYYY-MM-DD)."),
        to: z
          .string()
          .optional()
          .describe("Only transactions dated on or before this date (YYYY-MM-DD)."),
        textMatch: z
          .string()
          .optional()
          .describe("Filter by a case-insensitive substring of the transaction text."),
        amount: z
          .number()
          .optional()
          .describe("Filter by exact transaction amount in kroner (decimal DKK)."),
        // ===== BANK CLUSTER (#187) =====
        account: z
          .string()
          .optional()
          .describe(
            "Optional bank account identifier (id or name/slug) to filter by. " +
              "When omitted, transactions across all bank accounts are listed.",
          ),
        // ===== END BANK CLUSTER (#187) =====
        ...paginationFields,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDb<{
      company: string;
      status?: "all" | "matched" | "unmatched";
      from?: string;
      to?: string;
      textMatch?: string;
      amount?: number;
      account?: string;
      limit?: number;
      offset?: number;
    }>(server, ({ db, args }) => {
      // ===== BANK CLUSTER (#187) =====
      let bankAccountId: number | undefined;
      if (args.account && args.account.trim() !== "") {
        const account = resolveBankAccount(db, args.account);
        if (!account) {
          return wrapCoreResult({ ok: false, count: 0, rows: [], errors: [`bank account '${args.account}' does not exist`] });
        }
        bankAccountId = account.id;
      }
      // ===== END BANK CLUSTER (#187) =====
      const result = listBankTransactions(db, {
        status: args.status,
        from: args.from,
        to: args.to,
        textMatch: args.textMatch,
        amount: args.amount,
        bankAccountId,
      });
      if (!result.ok) return wrapCoreResult(result);
      const { pageRows, meta } = applyPagination(result.rows, { limit: args.limit, offset: args.offset });
      return successEnvelope({ rows: pageRows, ...meta });
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
      outputSchema: envelopeShape,
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
        from: z
          .string()
          .min(1)
          .describe("Start of the reconciliation period (inclusive), in YYYY-MM-DD format."),
        to: z
          .string()
          .min(1)
          .describe("End of the reconciliation period (inclusive), in YYYY-MM-DD format."),
        status: statusSchema.describe(
          "Filter by reconciliation status: 'all' (default), 'matched' or 'unmatched'.",
        ),
        textMatch: z
          .string()
          .optional()
          .describe("Filter by a case-insensitive substring of the transaction text."),
        amount: z
          .number()
          .optional()
          .describe("Filter by exact transaction amount in kroner (decimal DKK)."),
        // ===== BANK CLUSTER (#187) =====
        account: z
          .string()
          .optional()
          .describe(
            "Optional bank account identifier (id or name/slug) to scope the " +
              "report to. When omitted, all bank accounts are reconciled.",
          ),
        // ===== END BANK CLUSTER (#187) =====
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDb<{
      company: string;
      from: string;
      to: string;
      status?: "all" | "matched" | "unmatched";
      textMatch?: string;
      amount?: number;
      account?: string;
    }>(server, ({ db, args }) => {
      // ===== BANK CLUSTER (#187) =====
      let bankAccountId: number | undefined;
      if (args.account && args.account.trim() !== "") {
        const account = resolveBankAccount(db, args.account);
        if (!account) {
          const errorReport = buildBankReconciliationReport(db, args.from, args.to, {});
          return wrapCoreResult({ ...errorReport, ok: false, errors: [`bank account '${args.account}' does not exist`] });
        }
        bankAccountId = account.id;
      }
      // ===== END BANK CLUSTER (#187) =====
      const result = buildBankReconciliationReport(db, args.from, args.to, {
        status: args.status,
        textMatch: args.textMatch,
        amount: args.amount,
        bankAccountId,
      });
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "bank_import",
    {
      title: "Import bank CSV",
      description:
        "Importerer banktransaktioner fra CSV. Kræver confirm:true. " +
        "Send enten csvPath (absolut sti) eller csvContent (rå CSV-tekst). " +
        "write-reversible.",
      inputSchema: {
        company: z.string().min(1),
        csvPath: z
          .string()
          .optional()
          .describe(
            "Absolute path to the CSV file ON THE MCP SERVER'S FILESYSTEM. " +
              "Provide either csvPath or csvContent; csvPath wins if both are set.",
          ),
        csvContent: z
          .string()
          .optional()
          .describe(
            "Raw CSV text, used as an inline alternative to csvPath when the " +
              "file does not exist on the server. Provide either csvPath or csvContent.",
          ),
        // ===== BANK CLUSTER (#187,#186) =====
        account: z
          .string()
          .optional()
          .describe(
            "Optional bank account identifier (name or number) to import the " +
              "transactions into. When omitted, the company's default bank account is used.",
          ),
        profile: z
          .enum(["danske-bank"])
          .optional()
          .describe(
            "Optional named CSV import profile that pins the file's delimiter, " +
              "encoding, date order and column→field mapping. Known value: " +
              "'danske-bank' (Danske Bank account-statement export). When omitted, " +
              "the generic CSV parser is used (auto-detected headers). An unknown " +
              "profile aborts the import before parsing.",
          ),
        // ===== END BANK CLUSTER (#187,#186) =====
        confirm: confirmField,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{ company: string; csvPath?: string; csvContent?: string; account?: string; profile?: string; confirm?: boolean }>(
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
        const result = importBankCsv(db, args.company, path, {
          account: args.account && args.account.trim() !== "" ? args.account : undefined,
          profile: args.profile && args.profile.trim() !== "" ? args.profile : undefined,
        });
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
