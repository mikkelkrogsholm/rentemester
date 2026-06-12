/**
 * Kørselstids-laget (runtime) for MCP-tool-adaptere.
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
import { z } from "zod";
import { existsSync } from "node:fs";
import { isAbsolute, resolve, sep } from "node:path";
import { openDb, migrate } from "../core/db";
import { companyPaths } from "../core/paths";
import { isValidSlug, resolveConfiguredWorkspaceRoot, resolveWorkspaceSlug } from "../core/workspace";
import {
  asDocumentId,
  asJournalEntryId,
  type DocumentId,
  type JournalEntryId,
} from "../core/ids";
import {
  envelopeToCallResult,
  errorEnvelope,
  type Envelope,
} from "./envelope";
import { deriveMcpActor, type McpActor } from "./actor";

/**
 * Redacts absolute filesystem paths from a message destined for the
 * MCP caller. Absolute POSIX paths (`/...`) and Windows drive paths
 * (`C:\...`) are replaced with a `<path>` placeholder so host layout
 * and key-file locations are not disclosed to the connected client.
 * Full detail is kept in server-side stderr only.
 */
export function redactPaths(message: string): string {
  return message
    .replace(/[A-Za-z]:\\[^\s:]+/g, "<path>")
    .replace(/(?<![\w<])\/[^\s:]+/g, "<path>");
}

/**
 * Logs the full (unredacted) error to server-side stderr, then returns
 * a path-redacted error envelope safe to hand back to the caller.
 */
function safeErrorEnvelope(context: string, message: string): Envelope {
  console.error(`[mcp:${context}] ${message}`);
  return errorEnvelope(redactPaths(message));
}

/**
 * Result of resolving the `company` tool argument: either a concrete company
 * directory, or a caller-safe error message (already path-redacted).
 */
type CompanyArgResolution =
  | { ok: true; companyRoot: string }
  | { ok: false; error: string };

/**
 * Resolves the `company` argument of an MCP tool to a concrete company
 * directory. The argument may be EITHER:
 *
 *   - a workspace *slug* — a bare, separator-free, slug-shaped token. When
 *     `RENTEMESTER_WORKSPACE` is configured it is looked up in that workspace's
 *     manifest; an unknown slug is an error.
 *   - a raw filesystem *path* — resolved and `..`-guarded, mirroring the
 *     `--company` guard in `src/cli.ts`.
 *
 * Doing this in the single `withCompanyDb` helper means every existing tool
 * accepts a slug with zero per-tool changes — no endpoint or schema churn.
 */
export function resolveCompanyArg(raw: string): CompanyArgResolution {
  // Only a bare, separator-free, slug-shaped value is a slug candidate, so a
  // real path can never be misread as a slug.
  const looksLikeBareSlug = !raw.includes("/") && !raw.includes("\\") && isValidSlug(raw);
  if (looksLikeBareSlug) {
    let workspaceRoot: string | null;
    try {
      workspaceRoot = resolveConfiguredWorkspaceRoot();
    } catch (error) {
      return { ok: false, error: error instanceof Error ? error.message : String(error) };
    }
    if (workspaceRoot) {
      const fromSlug = resolveWorkspaceSlug(workspaceRoot, raw);
      if (fromSlug) return { ok: true, companyRoot: fromSlug };
      return {
        ok: false,
        error: `ingen virksomhed med slug '${raw}' findes i det konfigurerede workspace`,
      };
    }
    // No workspace configured: fall through and treat the value as a path.
  }

  const segments = raw.split(/[\\/]+/);
  if (segments.includes("..")) {
    return { ok: false, error: "company must not contain parent-directory ('..') segments" };
  }
  const resolved = resolve(raw);
  if (!isAbsolute(resolved) || resolved.split(sep).includes("..")) {
    return { ok: false, error: "company resolved to an unsafe path" };
  }
  return { ok: true, companyRoot: resolved };
}

/**
 * Delt `confirm`-felt for write- og destructive-tools.
 *
 * **Bevidst `.optional()`** (#201): hvis `confirm` var et påkrævet
 * `z.boolean()` ville SDK'ens zod-validering afvise et kald hvor `confirm`
 * mangler *før* handleren kører — og returnere en rå `-32602`-fejl helt uden
 * `structuredContent`-envelope. En agent der brancher på `structuredContent.ok`
 * (som docs beskriver) ville så crashe på `undefined`.
 *
 * Ved at gøre feltet valgfrit kommer et udeladt `confirm` helt frem til
 * `withCompanyDbConfirmed` / `withDestructiveConfirm`, som behandler det
 * præcis som `confirm: false` og returnerer den samme
 * `{ ok:false, errors:[...] }`-envelope.
 */
export const confirmField = z
  .boolean()
  .optional()
  .describe(
    "Must be set to true to acknowledge the write side effects of this tool. " +
      "Omitting it (or sending false) returns an { ok:false, errors:[...] } envelope " +
      "rather than performing the write.",
  );

/**
 * Delt `idempotencyKey`-felt for irreversible-write-tools (Batch F-3).
 *
 * **Currently RESERVED — not yet enforced server-side.** Adding the schema
 * field NOW (forward-compatible surface) lets an agent code its retry
 * pipeline against a stable contract while the actual dedup-cache lands in a
 * later release. The server presently logs the key into the audit chain so
 * an operator can correlate retries, but a duplicate call with the same
 * `idempotencyKey` will STILL double-book until the cache ships.
 *
 * Recommended shape: any caller-generated unique string (UUIDv4, ULID,
 * `<tool>:<biz-key>:<attempt>`, …) ≤ 128 chars. The key only needs to be
 * unique per `(company, tool)`; agents can use the same key on retries of
 * the SAME logical operation to deduplicate.
 */
export const idempotencyKeyField = z
  .string()
  .min(1)
  .max(128)
  .optional()
  .describe(
    "RESERVED: caller-generated unique key (UUID, ULID, or any ≤128-char " +
      "string) for write-deduplication on retry. The MCP server records the " +
      "key in the audit log NOW so retries are correlatable, but the actual " +
      "server-side dedup cache is not yet active — a duplicate call with the " +
      "same key currently STILL double-books. Add the key on every retry of " +
      "the same logical write; the dedup cache will be activated in a later " +
      "release without breaking the schema contract.",
  );

/**
 * Wraps en handler så MCP-callbacken får:
 *   - en åben + migreret database for `args.company`
 *   - et resolvet actor-objekt fra MCP-klient-handshake
 *   - automatisk close() på db (også ved exceptions)
 *
 * `args.company` accepterer både en workspace-slug og en rå sti — opslaget
 * sker centralt her, så alle eksisterende tools får slug-support gratis.
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
    const resolution = resolveCompanyArg(args.company);
    if (!resolution.ok) {
      console.error(`[mcp:withCompanyDb] ${resolution.error}: ${args.company}`);
      return envelopeToCallResult(errorEnvelope(redactPaths(resolution.error)));
    }
    const companyRoot = resolution.companyRoot;
    if (!existsSync(companyRoot)) {
      console.error(`[mcp:withCompanyDb] company path does not exist: ${companyRoot}`);
      return envelopeToCallResult(
        errorEnvelope("company path does not exist or is not initialized"),
      );
    }
    const actor = deriveMcpActor(server.server.getClientVersion());
    const db = openDb(companyPaths(companyRoot).db);
    try {
      migrate(db);
      // Hand the handler the *resolved* company directory under `args.company`
      // so tools that pass it on to core APIs (e.g. `getBackupComplianceStatus`,
      // `issueInvoice`) keep working whether a slug or a raw path was supplied.
      const resolvedArgs = { ...args, company: companyRoot };
      const envelope = await handler({ db, actor, args: resolvedArgs });
      return envelopeToCallResult(envelope);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      return envelopeToCallResult(safeErrorEnvelope("withCompanyDb", message));
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
      // The machine-readable `code: "CONFIRM_REQUIRED"` lets an agent branch
      // on the missing-confirm precondition without parsing the free-text
      // message. The message text itself stays stable for callers that pin
      // the docs-published string (`docs/confirm-contract.md`).
      return envelopeToCallResult(
        errorEnvelope(
          `confirm: true required for write tool ${toolName}`,
          { code: "CONFIRM_REQUIRED" },
        ),
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
 *
 * **`confirmText` er bevidst schema-valgfri** (#307): var feltet et påkrævet
 * `z.string()` ville SDK'ens zod-validering afvise et kald hvor `confirmText`
 * mangler *før* handleren kører — og returnere en rå `-32602`-fejl uden
 * `structuredContent`-envelope. Det gav en inkonsistens hvor et *udeladt*
 * `confirmText` returnerede `-32602`, mens et *forkert* `confirmText`
 * returnerede den normale envelope. Ved at gøre feltet valgfrit når både et
 * udeladt og et forkert `confirmText` frem til denne handler, som behandler
 * et manglende/tomt felt præcis som et mismatch og returnerer den samme
 * `{ ok:false, errors:[...] }`-envelope.
 */
export function withDestructiveConfirm<TArgs extends { confirm?: boolean; confirmText?: string }>(
  toolName: string,
  expectedText: (args: TArgs) => string,
  handler: (args: TArgs) => Envelope | Promise<Envelope>,
): (args: TArgs) => Promise<ReturnType<typeof envelopeToCallResult>> {
  return async (args) => {
    if (args?.confirm !== true) {
      return envelopeToCallResult(
        errorEnvelope(
          `confirm: true required for destructive tool ${toolName}`,
          { code: "CONFIRM_REQUIRED" },
        ),
      );
    }
    const expected = expectedText(args);
    // A missing/empty `confirmText` collapses to "" and is rejected with the
    // same envelope as a mismatch (#307) — never a raw -32602.
    const provided = typeof args?.confirmText === "string" ? args.confirmText : "";
    if (provided !== expected) {
      return envelopeToCallResult(
        errorEnvelope(
          `confirmText must match '${expected}' exactly (got: '${provided}')`,
          { code: "CONFIRMTEXT_MISMATCH" },
        ),
      );
    }
    try {
      const envelope = await handler(args);
      return envelopeToCallResult(envelope);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      return envelopeToCallResult(safeErrorEnvelope(toolName, message));
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
): DocumentId | null {
  if (Number.isInteger(args.documentId) && Number(args.documentId) > 0) {
    return asDocumentId(Number(args.documentId));
  }
  const value = (args.invoiceNumber ?? "").trim();
  if (!value) return null;
  const row = db
    .query(`SELECT id FROM documents WHERE document_type = 'issued_invoice' AND invoice_no = ? LIMIT 1`)
    .get(value) as { id: number } | null;
  return row?.id == null ? null : asDocumentId(row.id);
}

/**
 * Shared error envelope for the "{documentId|invoiceNumber} did not resolve
 * to an issued invoice" case. Three MCP tools (`invoice_*`, `peppol_*`,
 * `invoice_send_email`) hit the same selector; the wording must stay
 * identical so a string-matching agent only needs one matcher. Extracted
 * here in round-2 review's Batch D so the family doesn't drift again.
 */
export function invoiceNotFoundEnvelope(args: {
  documentId?: number | null;
  invoiceNumber?: string | null;
}): Envelope {
  return errorEnvelope(
    `Could not resolve invoice: provide documentId or invoiceNumber (got documentId=${args.documentId ?? "-"}, invoiceNumber='${args.invoiceNumber ?? ""}')`,
  );
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
): JournalEntryId | null {
  if (Number.isInteger(args.entryId) && Number(args.entryId) > 0) {
    return asJournalEntryId(Number(args.entryId));
  }
  const entryNo = (args.entryNo ?? "").trim();
  if (entryNo) {
    const row = db.query(`SELECT id FROM journal_entries WHERE entry_no = ? LIMIT 1`).get(entryNo) as
      | { id: number }
      | null;
    if (row) return asJournalEntryId(row.id);
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
    if (row) return asJournalEntryId(row.id);
  }
  return null;
}
