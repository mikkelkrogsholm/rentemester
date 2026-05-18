/**
 * Fælles hjælpere til MCP-tool-adaptere.
 *
 * Reducerer boilerplate omkring:
 *  - existsSync-tjek på `company`-stien
 *  - open/migrate/close database-håndtag pr. tool-call
 *  - `confirm: true` gating på write-tools
 *  - `confirmText` gating på destructive tools
 *
 * Holder hver tool-fil tynd: tool-funktionen modtager allerede en åben
 * database og et resolvet actor-objekt, og returnerer et envelope-resultat.
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import type { Database } from "bun:sqlite";
import { existsSync } from "node:fs";
import { openDb, migrate } from "../core/db";
import { companyPaths } from "../core/paths";
import {
  envelopeToCallResult,
  errorEnvelope,
  type Envelope,
} from "./envelope";
import { deriveMcpActor, type McpActor } from "./actor";

/**
 * Wraps en handler så MCP-callbacken får:
 *   - en åben + migreret database for `args.company`
 *   - et resolvet actor-objekt fra MCP-klient-handshake
 *   - automatisk close() på db (også ved exceptions)
 *
 * Handleren returnerer kun envelope; resultatet pakkes til MCP call-result her.
 */
export function withCompanyDb<TArgs extends { company: string }>(
  server: McpServer,
  handler: (ctx: { db: Database; actor: McpActor; args: TArgs }) => Envelope | Promise<Envelope>,
): (args: TArgs) => Promise<ReturnType<typeof envelopeToCallResult>> {
  return async (args) => {
    if (!args || typeof args.company !== "string" || args.company.length === 0) {
      return envelopeToCallResult(errorEnvelope("company path is required"));
    }
    if (!existsSync(args.company)) {
      return envelopeToCallResult(
        errorEnvelope(`company path does not exist: ${args.company}`),
      );
    }
    const actor = deriveMcpActor(server.server.getClientVersion());
    const db = openDb(companyPaths(args.company).db);
    try {
      migrate(db);
      const envelope = await handler({ db, actor, args });
      return envelopeToCallResult(envelope);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      return envelopeToCallResult(errorEnvelope(message));
    } finally {
      db.close();
    }
  };
}

/**
 * Som `withCompanyDb`, men accepterer write-tools der kræver `confirm: true`.
 * Hvis flaget mangler/er falsk returneres en fejl-envelope uden at databasen
 * overhovedet åbnes.
 */
export function withCompanyDbConfirmed<TArgs extends { company: string; confirm?: boolean }>(
  server: McpServer,
  toolName: string,
  handler: (ctx: { db: Database; actor: McpActor; args: TArgs }) => Envelope | Promise<Envelope>,
): (args: TArgs) => Promise<ReturnType<typeof envelopeToCallResult>> {
  return async (args) => {
    if (args?.confirm !== true) {
      return envelopeToCallResult(
        errorEnvelope(`confirm: true required for write tool ${toolName}`),
      );
    }
    return withCompanyDb(server, handler)(args);
  };
}

/**
 * Variant der ikke åbner database — for tools der kun rører filsystem
 * (`system_restore_backup`).
 *
 * Håndhæver `confirm: true` og `confirmText`-matching mod en forventet streng.
 */
export function withDestructiveConfirm<TArgs extends { confirm?: boolean; confirmText?: string }>(
  toolName: string,
  expectedText: (args: TArgs) => string,
  handler: (args: TArgs) => Envelope | Promise<Envelope>,
): (args: TArgs) => Promise<ReturnType<typeof envelopeToCallResult>> {
  return async (args) => {
    if (args?.confirm !== true) {
      return envelopeToCallResult(
        errorEnvelope(`confirm: true required for destructive tool ${toolName}`),
      );
    }
    const expected = expectedText(args);
    const provided = typeof args?.confirmText === "string" ? args.confirmText : "";
    if (provided !== expected) {
      return envelopeToCallResult(
        errorEnvelope(`confirmText must match '${expected}' exactly (got: '${provided}')`),
      );
    }
    try {
      const envelope = await handler(args);
      return envelopeToCallResult(envelope);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      return envelopeToCallResult(errorEnvelope(message));
    }
  };
}

/**
 * Slår en faktura-document-id op via dens `invoice_no` hvis kun
 * `invoiceNumber` er angivet. Returnerer `null` hvis intet matcher.
 */
export function resolveIssuedInvoiceDocumentId(
  db: Database,
  args: { documentId?: number | null; invoiceNumber?: string | null },
): number | null {
  if (Number.isInteger(args.documentId) && Number(args.documentId) > 0) {
    return Number(args.documentId);
  }
  const value = (args.invoiceNumber ?? "").trim();
  if (!value) return null;
  const row = db
    .query(`SELECT id FROM documents WHERE document_type = 'issued_invoice' AND invoice_no = ? LIMIT 1`)
    .get(value) as { id: number } | null;
  return row?.id ?? null;
}

/**
 * Slår journal-entry-id op via entry_no, eller via match-text/match-date/document.
 * Bruges af `journal_reverse`-tool'et.
 */
export function resolveJournalEntryId(
  db: Database,
  args: {
    entryId?: number | null;
    entryNo?: string | null;
    matchText?: string | null;
    matchDate?: string | null;
    matchDocumentId?: number | null;
  },
): number | null {
  if (Number.isInteger(args.entryId) && Number(args.entryId) > 0) return Number(args.entryId);
  const entryNo = (args.entryNo ?? "").trim();
  if (entryNo) {
    const row = db.query(`SELECT id FROM journal_entries WHERE entry_no = ? LIMIT 1`).get(entryNo) as
      | { id: number }
      | null;
    if (row) return row.id;
  }
  const matchText = (args.matchText ?? "").trim();
  if (matchText) {
    const dateClause = args.matchDate ? "AND transaction_date = ?" : "";
    const docClause = args.matchDocumentId ? "AND document_id = ?" : "";
    const params: unknown[] = [matchText];
    if (args.matchDate) params.push(args.matchDate);
    if (args.matchDocumentId) params.push(args.matchDocumentId);
    const row = db
      .query(
        `SELECT id FROM journal_entries WHERE text = ? ${dateClause} ${docClause} ORDER BY id DESC LIMIT 1`,
      )
      .get(...(params as [unknown])) as { id: number } | null;
    if (row) return row.id;
  }
  return null;
}
