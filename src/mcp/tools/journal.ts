/**
 * MCP-tools for finansposteringer.
 *
 *  - `journal_post` (write-irreversible) — bogfør manuel postering
 *  - `journal_reverse` (write-irreversible) — modposter eksisterende postering
 *  - `journal_list` (read) — lister posteringer
 *
 * Aktor-attribution sker ved at læse MCP-klient-handshake (via
 * `deriveMcpActor(server.getClientVersion())`) og passe `createdBy` /
 * `createdByProgram` ind som **eksplicitte payload-felter** — ikke som
 * proces-env-vars. Det er race-safe når flere tool-calls overlapper.
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import {
  postJournalEntry,
  reverseJournalEntry,
  dryRunJournalEntry,
  type JournalEntryInput,
} from "../../core/ledger";
import { withActor } from "../actor";
import { envelopeShape, errorEnvelope, successEnvelope, wrapCoreResult } from "../envelope";
import { withCompanyDb, withCompanyDbConfirmed, resolveJournalEntryId, confirmField } from "../tool-runtime";
import { applyPagination, paginationFields, paginationDescriptionSuffix } from "../pagination";
import { isValidIsoDate } from "../../core/dates";

const lineSchema = z.object({
  accountNo: z
    .string()
    .min(1)
    .describe("Account number from the chart of accounts, e.g. '3000'. See accounts_list."),
  debitAmount: z
    .number()
    .nonnegative()
    .optional()
    .describe(
      "Debit amount for this line, in kroner (decimal DKK, 2 decimals — NOT øre). " +
        "Set exactly one of debitAmount or creditAmount per line.",
    ),
  creditAmount: z
    .number()
    .nonnegative()
    .optional()
    .describe(
      "Credit amount for this line, in kroner (decimal DKK, 2 decimals — NOT øre). " +
        "Set exactly one of debitAmount or creditAmount per line.",
    ),
  vatCode: z
    .string()
    .optional()
    .describe("Optional VAT code for the line, e.g. 'DK_PURCHASE_25', 'DK_SALE_25'."),
  text: z.string().optional().describe("Optional free-text description of the line."),
});

const payloadSchema = z.object({
  transactionDate: z
    .string()
    .min(1)
    .describe("Posting date in YYYY-MM-DD format."),
  text: z.string().min(1).describe("Human-readable description of the journal entry."),
  documentId: z
    .number()
    .int()
    .positive()
    .optional()
    .describe(
      "ID of the underlying document (bilag). REQUIRED whenever any line posts to an " +
        "expense or income account — the core rejects such an entry with 'documentId is " +
        "required when posting expense or income lines'. Optional only for entries that " +
        "touch balance-sheet accounts exclusively (e.g. owner contributions, bank transfers). " +
        "Use documents_list to find an existing document ID.",
    ),
  sourceBankTransactionId: z
    .number()
    .int()
    .positive()
    .optional()
    .describe("Optional ID of the bank transaction this entry settles. See bank_list."),
  currency: z
    .string()
    .optional()
    .describe("3-letter ISO currency code (default 'DKK'). Non-DKK entries also require amountForeign, amountDkk and fxRateToDkk."),
  amountForeign: z
    .number()
    .optional()
    .describe("For non-DKK entries: the entry amount in the foreign currency, in major units (e.g. euros — NOT cents)."),
  amountDkk: z
    .number()
    .optional()
    .describe("For non-DKK entries: the entry amount converted to kroner (decimal DKK, 2 decimals). Must equal amountForeign * fxRateToDkk."),
  fxRateToDkk: z
    .number()
    .optional()
    .describe("For non-DKK entries: the FX rate from the foreign currency to DKK (e.g. 7.46)."),
  lines: z
    .array(lineSchema)
    .min(2, "at least two journal lines are required")
    .describe(
      "Journal lines — at least two. Double-entry: total debit must exactly " +
        "equal total credit (in kroner). Each line sets exactly one of " +
        "debitAmount or creditAmount; the core rejects an entry whose debit " +
        "and credit do not balance, and any entry with fewer than two lines.",
    ),
});

export function registerJournalTools(server: McpServer): void {
  server.registerTool(
    "journal_post",
    {
      title: "Post journal entry",
      description:
        "Bogfører en manuel finanspostering i den append-only kæde. " +
        "Kræver confirm:true. Modposteres via journal_reverse. " +
        "Kør journal_dry_run med samme payload først for en ikke-bindende " +
        "forhåndsvisning af postering og kontosaldi. " +
        "Alle beløb er i kroner (decimal DKK, 2 decimaler — ikke øre). " +
        "payload.documentId er påkrævet hvis nogen linje bogfører på en udgifts- eller " +
        "indtægtskonto; valgfri kun for posteringer der udelukkende rører balancekonti. " +
        "write-irreversible.",
      inputSchema: {
        company: z.string().min(1, "company path is required"),
        payload: payloadSchema,
        confirm: confirmField,
      },
      outputSchema: envelopeShape,
      annotations: {
        readOnlyHint: false,
        destructiveHint: false,
        idempotentHint: false,
        openWorldHint: false,
      },
    },
    // `withCompanyDbConfirmed` enforces `confirm: true`, resolves + existsSync-
    // guards `company`, and returns a *path-redacted* error envelope on a
    // bad/missing directory — the absolute host path never reaches the caller
    // (#228). It also opens/migrates/closes the db and derives the actor.
    withCompanyDbConfirmed<{
      company: string;
      payload: z.infer<typeof payloadSchema>;
      confirm?: boolean;
    }>(server, "journal_post", ({ db, actor, args }) => {
      const entry: JournalEntryInput = withActor(args.payload as JournalEntryInput, actor);
      return wrapCoreResult(postJournalEntry(db, entry));
    }),
  );

  server.registerTool(
    "journal_dry_run",
    {
      title: "Dry-run journal entry",
      description:
        "Forhåndsviser hvad journal_post ville gøre med præcis samme payload — UDEN at " +
        "skrive noget. Posteringen køres mod den append-only kæde i en transaktion der " +
        "altid rulles tilbage: intet journalnummer forbruges, intet bilag bogføres. " +
        "Svaret rummer det entryNo og entryHash posteringen ville få, den nuværende " +
        "previousHash, samt accountEffects: saldo før/efter pr. berørt konto opgjort " +
        "som debet-minus-kredit-netto i kroner — kreditnormale konti (indtægt, moms, " +
        "egenkapital, gæld) står derfor negative. " +
        "VIGTIGT: resultatet er ikke-bindende — entryNo, previousHash og saldi kan " +
        "ændre sig hvis en anden postering bogføres inden journal_post kaldes. " +
        "Read-only; intet confirm påkrævet.",
      inputSchema: {
        company: z.string().min(1, "company path is required"),
        payload: payloadSchema,
      },
      outputSchema: envelopeShape,
      annotations: {
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    withCompanyDb<{
      company: string;
      payload: z.infer<typeof payloadSchema>;
    }>(server, ({ db, actor, args }) => {
      const entry: JournalEntryInput = withActor(args.payload as JournalEntryInput, actor);
      return wrapCoreResult(dryRunJournalEntry(db, entry));
    }),
  );

  server.registerTool(
    "journal_reverse",
    {
      title: "Reverse journal entry",
      description:
        "Tilbagefører en bogført finanspostering ved at oprette en modpost. " +
        "Identificér posten via entryId, entryNo eller matchText (+ valgfri matchDate/matchDocumentId). " +
        "write-irreversible.",
      inputSchema: {
        company: z.string().min(1),
        entryId: z
          .number()
          .int()
          .positive()
          .optional()
          .describe(
            "ID of the journal entry to reverse. Identify the entry by exactly one " +
              "of entryId, entryNo or matchText. See journal_list.",
          ),
        entryNo: z
          .string()
          .optional()
          .describe(
            "Entry number of the journal entry to reverse. Identify the entry by " +
              "exactly one of entryId, entryNo or matchText.",
          ),
        matchText: z
          .string()
          .optional()
          .describe(
            "Substring of the entry text used to locate the journal entry to " +
              "reverse. Identify the entry by exactly one of entryId, entryNo or " +
              "matchText; narrow an ambiguous text match with matchDate / " +
              "matchDocumentId.",
          ),
        matchDate: z
          .string()
          .optional()
          .describe(
            "Optional posting date (YYYY-MM-DD) that further narrows a matchText lookup.",
          ),
        matchDocumentId: z
          .number()
          .int()
          .positive()
          .optional()
          .describe("Optional document ID that further narrows a matchText lookup."),
        date: z
          .string()
          .min(1, "date (transaction date for the reversal) is required")
          .describe("Posting date of the reversal entry, in YYYY-MM-DD format."),
        reason: z
          .string()
          .min(1, "reason is required")
          .describe("Human-readable reason for the reversal, recorded on the counter-entry."),
        confirm: confirmField,
      },
      outputSchema: envelopeShape,
      annotations: {
        readOnlyHint: false,
        destructiveHint: false,
        idempotentHint: false,
        openWorldHint: false,
      },
    },
    withCompanyDbConfirmed<{
      company: string;
      entryId?: number;
      entryNo?: string;
      matchText?: string;
      matchDate?: string;
      matchDocumentId?: number;
      date: string;
      reason: string;
      confirm?: boolean;
    }>(server, "journal_reverse", ({ db, args, actor }) => {
      const id = resolveJournalEntryId(db, args);
      if (!id) {
        return errorEnvelope(
          "Could not resolve journal entry: provide entryId, entryNo or matchText (with optional matchDate/matchDocumentId)",
        );
      }
      const result = reverseJournalEntry(db, {
        entryId: id,
        transactionDate: args.date,
        reason: args.reason,
        createdBy: actor.createdBy,
        createdByProgram: actor.createdByProgram,
      });
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "journal_list",
    {
      title: "List journal entries",
      description:
        "Lister finansposteringer i append-only kæden med valgfri filtre på status og datointerval. Read-only." +
        paginationDescriptionSuffix,
      inputSchema: {
        company: z.string().min(1),
        from: z
          .string()
          .optional()
          .describe("Only entries with transaction_date on or after this date (YYYY-MM-DD)."),
        to: z
          .string()
          .optional()
          .describe("Only entries with transaction_date on or before this date (YYYY-MM-DD)."),
        status: z
          .enum(["all", "posted", "reversed"])
          .optional()
          .describe(
            "Filter by entry status: 'all' (default), 'posted' (active entries) or 'reversed' " +
              "(entries that have been modposteret).",
          ),
        ...paginationFields,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDb<{
      company: string;
      from?: string;
      to?: string;
      status?: "all" | "posted" | "reversed";
      limit?: number;
      offset?: number;
    }>(server, ({ db, args }) => {
      const errors: string[] = [];
      if (args.from && !isValidIsoDate(args.from)) errors.push("from must be YYYY-MM-DD when present");
      if (args.to && !isValidIsoDate(args.to)) errors.push("to must be YYYY-MM-DD when present");
      if (errors.length > 0) return errorEnvelope(errors);

      const status = args.status ?? "all";
      const rows = db
        .query(
          `SELECT id, entry_no, transaction_date, text, currency, amount_foreign, amount_dkk,
                  fx_rate_to_dkk, document_id, source_bank_transaction_id, status, reversal_of_entry_id
           FROM journal_entries
           WHERE (? IS NULL OR transaction_date >= ?)
             AND (? IS NULL OR transaction_date <= ?)
             AND (? = 'all' OR status = ?)
           ORDER BY id DESC`,
        )
        .all(
          args.from ?? null, args.from ?? null,
          args.to ?? null, args.to ?? null,
          status, status,
        ) as Array<{
          id: number;
          entry_no: string;
          transaction_date: string;
          text: string;
          currency: string | null;
          amount_foreign: number | null;
          amount_dkk: number | null;
          fx_rate_to_dkk: number | null;
          document_id: number | null;
          source_bank_transaction_id: number | null;
          status: string;
          reversal_of_entry_id: number | null;
        }>;
      const mapped = rows.map((row) => ({
        id: row.id,
        entryNo: row.entry_no,
        transactionDate: row.transaction_date,
        text: row.text,
        currency: row.currency,
        amountForeign: row.amount_foreign,
        amountDkk: row.amount_dkk,
        fxRateToDkk: row.fx_rate_to_dkk,
        documentId: row.document_id,
        sourceBankTransactionId: row.source_bank_transaction_id,
        status: row.status,
        reversalOfEntryId: row.reversal_of_entry_id,
      }));
      const { pageRows, meta } = applyPagination(mapped, { limit: args.limit, offset: args.offset });
      return successEnvelope({ entries: pageRows, ...meta });
    }),
  );
}
