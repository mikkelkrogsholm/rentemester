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
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  importBankCsv,
  listBankAccounts,
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
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
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
        "Foreslår deterministiske match mellem uafstemte banktransaktioner og fakturaer/bilag. " +
        "Read-only.\n\n" +
        "Matching-signaler (deterministiske, kombineres til en konfidens-score): " +
        "(1) eksakt beløb i øre mod åben saldo (issued invoice), gross-beløb (purchase_sale, " +
        "credit_note_refund) eller delvist refunderet beløb (supplier_credit_refund); " +
        "(2) invoice number / kreditnota-nummer / krediteret fakturanummer fundet som " +
        "substring i bank-tekst/reference/counterparty/message (case-insensitive); " +
        "(3) navne-tokens overlappende mellem bank-tekst og kunde-/leverandørnavn " +
        "(stop-ord som ApS/A/S/DKK fjernes); (4) dato-nærhed (faktura: 7 dage, " +
        "kreditnota: 14 dage); plus for supplier_credit_refund kræves et eksplicit " +
        "refund-cue (KREDIT/KREDITNOTA/REFUSION/REFUND/TILBAGEBETALING/CREDIT).\n\n" +
        "Confidence-skala: float i intervallet 0..1. Forslag under 0.5 returneres ikke. " +
        "Et match der kun bygger på beløb (uden invoice-nummer eller stærk navne-match " +
        "med ≥2 tokens) kappes til 0.45 og dukker derfor ikke op — dvs. enhver " +
        "returneret confidence ≥ 0.5 har altid mindst én identificerende corroboration. " +
        "Typiske intervaller: 0.50–0.65 svag/usikker, 0.65–0.80 god, ≥ 0.80 stærk. " +
        "En sikker auto-godkendelses-grænse er typisk ≥ 0.80.\n\n" +
        "Returnerer en envelope med data.rows: en række per uafstemt banktransaktion, " +
        "hver med felterne bankTransactionId, date, text, amount (DKK), currency, " +
        "reference og suggestions[]. Hver suggestion har { kind: 'issued_invoice' | " +
        "'purchase_sale' | 'credit_note_refund' | 'supplier_credit_refund', documentId, " +
        "invoiceNo, customerName?, supplierName?, confidence, reasons[] } sorteret efter " +
        "confidence faldende, derefter documentId stigende (deterministisk).\n\n" +
        "Rækkefølge på uafstemte transaktioner: transaction_date DESC, id DESC " +
        "(nyeste først, deterministisk). " +
        "Suggestions-listen pr. række er truncated til `max` (default 5); selve `rows` " +
        "er IKKE truncated af `max` (én række pr. uafstemt transaktion i scope, jf. #381).",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        bankTransactionId: z
          .number()
          .int()
          .positive()
          .optional()
          .describe(
            "Optional id of a single unmatched bank transaction to suggest matches for. " +
              "When omitted, ALL unmatched bank transactions are scored (ordered by " +
              "transaction_date DESC, id DESC) — no implicit pagination is applied at " +
              "this level. Use this to target a single transaction, e.g. after a partial " +
              "import or when reviewing one row in the UI.",
          ),
        max: z
          .number()
          .int()
          .positive()
          .optional()
          .describe(
            "Maximum number of match suggestions returned PER bank transaction (not " +
              "total). Default: 5. Suggestions are sorted by confidence DESC, then " +
              "documentId ASC, then truncated. The result's `rows` array itself is NOT " +
              "truncated by `max` — every unmatched bank transaction in scope produces " +
              "exactly one row (with up to `max` suggestions); if you need to limit the " +
              "number of rows, target a single transaction via bankTransactionId, or " +
              "narrow the bank import. See #381 for the related pagination contract.",
          ),
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
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
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
        "BIVIRKNING ved csvContent: indholdet skrives midlertidigt til en " +
        "tmpdir under os.tmpdir() (mønster `rentemester-mcp-bank-*`). " +
        "Den tmpdir slettes altid før kaldet returnerer — både ved success og " +
        "ved fejl/exception — så agenten kan retry'e uden at efterlade spor " +
        "uden for virksomhedsmappen.\n\n" +
        "CSV-format (når `profile` IKKE er sat — den generiske parser): " +
        "header-rækken auto-detekteres; kolonnenavnene matches case-insensitivt. " +
        "Påkrævede kolonner: " +
        "`transaction_date` (eller `date` / `dato`) — YYYY-MM-DD eller dd-mm-yyyy; " +
        "`text` (eller `description` / `tekst` / `narrative`) — fri tekst der vises i Bank-listen; " +
        "`amount` (eller `beløb`) — DKK med punktum eller komma som decimaltegn, negativ = debit (træk). " +
        "Valgfri kolonner: `reference` (entydig nøgle for dedup), `currency` (default DKK), " +
        "`counterparty` / `modpart`, `message` / `besked`. " +
        "Dedup: rækker matches på (transaction_date + amount + reference + counterparty) og " +
        "genimporteres ikke — agenten kan retry'e samme CSV uden duplikater " +
        "(idempotentHint: true).\n\n" +
        "Find tilgængelige `account`-slugs med `bank_account_list` før kaldet. " +
        "Den slug du sender skal matche `slug`-feltet returneret derfra; et ukendt " +
        "navn afvises før parsing. " +
        "write-reversible.",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
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
            "Optional bank account identifier — the `slug` field returned by " +
              "`bank_account_list` (or the human-readable name). When omitted, the " +
              "company's default bank account is used. An unknown account aborts " +
              "the import with a clear error before parsing.",
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
      // `bank_import` is idempotent by design: each row is deduplicated by
      // (date + amount + reference) so re-running the same import never
      // double-creates transactions. The annotation reflects that contract so
      // an agent retrying after a network hiccup knows it's safe.
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: true, openWorldHint: false },
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
        // The tmpdir created for the inline csvContent variant is a write
        // side-effect *outside* the company directory. The annotation says
        // destructiveHint:false, so we MUST guarantee the side-effect is
        // reverted on every path (success, ok:false from importBankCsv, AND
        // unexpected throws) — otherwise an agent retrying a failing import
        // would pile up tmp files it cannot see. (#383)
        try {
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
        } finally {
          if (tmpDir) rmSync(tmpDir, { recursive: true, force: true });
        }
      },
    ),
  );

  // bank_account_list — read-only enumeration of the bank-accounts registry
  // (mirrors CLI `bank-account list`). Without this, an agent can't discover
  // which `--account` slugs exist before passing one to `bank_import`.
  server.registerTool(
    "bank_account_list",
    {
      title: "List registered bank accounts",
      description:
        "Lister de bankkonti der er registreret på virksomheden — slug, " +
        "navn, bank, valuta, IBAN og om kontoen er aktiv. Den slug, der " +
        "returneres her, er den værdi en agent kan sende som `account` til " +
        "`bank_import` og `bank_list`. Read-only.\n\n" +
        "Default returneres ALLE konti (aktive + inaktive). Sæt " +
        "`includeInactive: false` for kun at få de aktive — fx før et import-" +
        "kald, hvor en inaktiv konto vil blive afvist længere nede i kæden.",
      inputSchema: {
        company: z
          .string()
          .min(1)
          .describe("Absolute path to the company directory, or a workspace slug."),
        includeInactive: z
          .boolean()
          .optional()
          .describe(
            "When false, only active bank accounts are returned. Default true.",
          ),
      },
      outputSchema: envelopeShape,
      annotations: {
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    withCompanyDb<{ company: string; includeInactive?: boolean }>(server, ({ db, args }) => {
      const includeInactive = args.includeInactive !== false;
      return wrapCoreResult(listBankAccounts(db, includeInactive));
    }),
  );
}
