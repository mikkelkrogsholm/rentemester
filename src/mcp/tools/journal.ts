/**
 * MCP-tool: `journal_post` (write-irreversible).
 *
 * 1:1-mapping af CLI-kommandoen `journal post`. Bogfører en manuel
 * finanspostering ind i den append-only kæde.
 *
 * Klassifikation: `write-irreversible` — skriver i kæden og kan kun
 * "rulles tilbage" via en modpostering (`journal_reverse`). Kræver
 * derfor `confirm: true` på input. Hvis flaget mangler, returnerer vi
 * fejl-envelope uden at røre databasen.
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
import { postJournalEntry, type JournalEntryInput } from "../../core/ledger";
import { deriveMcpActor, withActor } from "../actor";
import { envelopeToCallResult, errorEnvelope, wrapCoreResult } from "../envelope";

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

const inputSchema = {
  company: z.string().min(1, "company path is required"),
  payload: payloadSchema,
  confirm: z
    .boolean()
    .describe("Must be true to acknowledge write-irreversible tool side effects."),
  idempotencyKey: z.string().optional(),
};

export function registerJournalTools(server: McpServer): void {
  server.registerTool(
    "journal_post",
    {
      title: "Post journal entry",
      description:
        "Bogfører en manuel finanspostering i den append-only kæde. " +
        "Write-irreversible — kræver confirm:true. Modposteres via journal_reverse.",
      inputSchema,
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
}
