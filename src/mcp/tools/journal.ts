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
import { existsSync } from "node:fs";
import { companyPaths } from "../../core/paths";
import { openDb, migrate } from "../../core/db";
import {
  postJournalEntry,
  reverseJournalEntry,
  type JournalEntryInput,
} from "../../core/ledger";
import { deriveMcpActor, withActor } from "../actor";
import {
  envelopeToCallResult,
  errorEnvelope,
  successEnvelope,
  wrapCoreResult,
} from "../envelope";
import { withCompanyDb, withCompanyDbConfirmed, resolveJournalEntryId } from "../helpers";

const lineSchema = z.object({
  accountNo: z.string().min(1),
  debitAmount: z.number().nonnegative().optional(),
  creditAmount: z.number().nonnegative().optional(),
  vatCode: z.string().optional(),
  text: z.string().optional(),
});

const payloadSchema = z.object({
  transactionDate: z.string().min(1),
  text: z.string().min(1),
  documentId: z.number().int().positive().optional(),
  sourceBankTransactionId: z.number().int().positive().optional(),
  currency: z.string().optional(),
  amountForeign: z.number().optional(),
  amountDkk: z.number().optional(),
  fxRateToDkk: z.number().optional(),
  lines: z.array(lineSchema).min(1, "at least one journal line is required"),
});

export function registerJournalTools(server: McpServer): void {
  server.registerTool(
    "journal_post",
    {
      title: "Post journal entry",
      description:
        "Bogfører en manuel finanspostering i den append-only kæde. " +
        "Write-irreversible — kræver confirm:true. Modposteres via journal_reverse.",
      inputSchema: {
        company: z.string().min(1, "company path is required"),
        payload: payloadSchema,
        confirm: z
          .boolean()
          .describe("Must be true to acknowledge write-irreversible tool side effects."),
        idempotencyKey: z.string().optional(),
      },
      annotations: {
        readOnlyHint: false,
        destructiveHint: false,
        idempotentHint: false,
        openWorldHint: false,
      },
    },
    async ({ company, payload, confirm }) => {
      if (confirm !== true) {
        return envelopeToCallResult(
          errorEnvelope("confirm: true required for write tool journal_post"),
        );
      }
      if (!existsSync(company)) {
        return envelopeToCallResult(
          errorEnvelope(`company path does not exist: ${company}`),
        );
      }
      const actor = deriveMcpActor(server.server.getClientVersion());
      const db = openDb(companyPaths(company).db);
      try {
        migrate(db);
        const entry: JournalEntryInput = withActor(payload as JournalEntryInput, actor);
        const result = postJournalEntry(db, entry);
        return envelopeToCallResult(wrapCoreResult(result));
      } finally {
        db.close();
      }
    },
  );

  server.registerTool(
    "journal_reverse",
    {
      title: "Reverse journal entry",
      description:
        "Tilbagefører en bogført finanspostering ved at oprette en modpost. write-irreversible. " +
        "Identificér posten via entryId, entryNo eller matchText (+ valgfri matchDate/matchDocumentId).",
      inputSchema: {
        company: z.string().min(1),
        entryId: z.number().int().positive().optional(),
        entryNo: z.string().optional(),
        matchText: z.string().optional(),
        matchDate: z.string().optional(),
        matchDocumentId: z.number().int().positive().optional(),
        date: z.string().min(1, "date (transaction date for the reversal) is required"),
        reason: z.string().min(1, "reason is required"),
        confirm: z.boolean(),
      },
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
      confirm: boolean;
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
      description: "Lister finansposteringer i append-only kæden. Read-only.",
      inputSchema: { company: z.string().min(1) },
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDb<{ company: string }>(server, ({ db }) => {
      const rows = db
        .query(
          `SELECT id, entry_no, transaction_date, text, currency, amount_foreign, amount_dkk,
                  fx_rate_to_dkk, document_id, source_bank_transaction_id, status, reversal_of_entry_id
           FROM journal_entries
           ORDER BY id DESC`,
        )
        .all() as Array<{
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
      return successEnvelope({
        entries: rows.map((row) => ({
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
        })),
        count: rows.length,
      });
    }),
  );
}
