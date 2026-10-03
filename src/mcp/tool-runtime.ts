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
import type { Database, SQLQueryBindings } from "bun:sqlite";
import { z } from "zod";
import { existsSync } from "node:fs";
import { AsyncLocalStorage } from "node:async_hooks";
import { openDb, migrate } from "../core/db";
import { inspectOpenLedger, openLedgerReadOnly } from "../core/ledger-inspection";
import { companyPaths } from "../core/paths";
import { resolveConfiguredWorkspaceRoot } from "../core/workspace";
import {
  companyOperationTargetExists,
  resolveCompanyOperationTarget,
  runCompanyWriteSession,
  type CompanyOperationTarget,
} from "../core/company-operation";
import {
  asDocumentId,
  asJournalEntryId,
  type DocumentId,
  type JournalEntryId,
} from "../core/ids";
import {
  envelopeToCallResult,
  errorEnvelope,
  errorEnvelopeWithData,
  type Envelope,
} from "./envelope";
import { deriveMcpActor, type McpActor } from "./actor";
import { checkActorAllowlist } from "../cli-actor";
import { currentMcpAuthenticatedPrincipal } from "./security";
import { executeLocalIdempotentMutation, IdempotencyError, RETRY_CLASS_BY_OPERATION, withoutIdempotencyTransportFields, validateIdempotencyKey } from "../core/idempotency";

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
type CompanyArgResolution = CompanyOperationTarget;

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
  return resolveCompanyOperationTarget(raw);
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
 * Reuse a key only for an identical retry. The confirmed-write runtime
 * persists a bounded, actor- and company-scoped receipt; replay returns the
 * original envelope without executing the business mutation again.
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
    "Caller-generated retry key (UUID/ULID, ≤128 chars). An identical retry " +
      "returns the durable original envelope with data.idempotency.replayed=true; " +
      "a different validated payload returns IDEMPOTENCY_CONFLICT.",
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
/**
 * Optional cross-cutting checks `withCompanyDb` runs AFTER the company path is
 * resolved and the actor derived, but BEFORE the db is opened or the handler
 * runs. Used by `withCompanyDbConfirmed` to enforce the shared actor allowlist
 * (SEC-2) on the MCP write path.
 */
type WithCompanyDbOptions = {
  /**
   * SEC-2 (Audit 2026-06-11): when true, the derived MCP actor must pass the
   * company's `config/policy.yaml` actor_allowlist — the SAME gate the CLI
   * applies via `enforceMutationActorPolicy`. A non-allowlisted (or, per SEC-3,
   * policy-less) company rejects the write with a path-redacted envelope.
   */
  enforceActorAllowlist?: boolean;
};

/**
 * Registration wraps every `readOnlyHint:true` invocation in this context.
 * Keeping the bit in AsyncLocalStorage is important: tool callbacks can be
 * asynchronous and must not inherit a concurrent write tool's opening mode.
 */
const readOnlyToolInvocation = new AsyncLocalStorage<boolean>();

/** Used by the registration proxy; exported for its single integration point. */
export function runMcpReadOnlyTool<T>(callback: () => T): T {
  return readOnlyToolInvocation.run(true, callback);
}

type CompanyRuntimeCallback = ((...args: never[]) => unknown) & {
  readonly companyDbOpening?: "adaptive" | "readonly" | "write";
};

/** Registration-time contract used by the public MCP registry and its gate. */
export function assertMcpCompanyReadOnlyHandler(name: string, callback: unknown): void {
  const opening = (callback as CompanyRuntimeCallback | undefined)?.companyDbOpening;
  if (opening !== "readonly" && opening !== "adaptive") {
    throw new Error(`read-only MCP tool ${name} must use the strict company read runtime`);
  }
}

function markCompanyRuntime<T extends (...args: never[]) => unknown>(
  callback: T,
  opening: CompanyRuntimeCallback["companyDbOpening"],
): T {
  Object.defineProperty(callback, "companyDbOpening", { value: opening });
  return callback;
}

/** Mark a bespoke callback after it has been audited to use snapshot-only APIs. */
export function strictMcpReadOnlyHandler<T extends (...args: never[]) => unknown>(callback: T): T {
  return markCompanyRuntime(callback, "readonly");
}

/** Own the snapshot handle and schema gate; wrappers retain their actor/error policy. */
async function withReadLedger(
  dbPath: string,
  handler: (db: Database) => Envelope | Promise<Envelope>,
  allowSchemaNotCurrent = false,
): Promise<Envelope> {
  const db = openLedgerReadOnly(dbPath);
  try {
    const schema = inspectOpenLedger(db);
    if (schema.status !== "current" && !allowSchemaNotCurrent) {
      return errorEnvelopeWithData(
        `schema_${schema.status}: current=${schema.currentVersion} required=${schema.requiredVersion}`,
        { schema },
      );
    }
    return await handler(db);
  } finally {
    db.close();
  }
}

export function withCompanyDb<TArgs extends { company: string }>(
  server: McpServer,
  handler: (ctx: { db: Database; actor: McpActor; args: TArgs }) => Envelope | Promise<Envelope>,
  options: WithCompanyDbOptions = {},
): (args: TArgs) => Promise<ReturnType<typeof envelopeToCallResult>> {
  return markCompanyRuntime(async (args) => {
    if (!args || typeof args.company !== "string" || args.company.length === 0) {
      return envelopeToCallResult(errorEnvelope("company path is required"));
    }
    const resolution = resolveCompanyArg(args.company);
    if (!resolution.ok) {
      console.error(`[mcp:withCompanyDb] ${resolution.error}: ${args.company}`);
      return envelopeToCallResult(errorEnvelope(redactPaths(resolution.error)));
    }
    const companyRoot = resolution.companyRoot;
    // The shared company runtime is fail-closed: only the confirmed write
    // wrapper below may opt into a writable/migrating handle. This also keeps
    // domain registrars safe when they are tested or embedded without the
    // top-level registration proxy.
    const readOnly = readOnlyToolInvocation.getStore() !== false;
    const dbPath = companyPaths(companyRoot).db;
    if (!companyOperationTargetExists(companyRoot) || (readOnly && !existsSync(dbPath))) {
      console.error(`[mcp:withCompanyDb] company path does not exist: ${companyRoot}`);
      return envelopeToCallResult(
        errorEnvelope("company path does not exist or is not initialized"),
      );
    }
    const actor = deriveMcpActor(server.server.getClientVersion());
    // SEC-2: enforce the shared actor allowlist for write tools. The MCP actor
    // is derived from the client handshake (`actor.createdBy`, e.g.
    // `agent:claude-code/0.4.1`). NOTE (SEC-4): handshake-supplied attribution
    // is advisory and client-controlled — the allowlist is a coarse policy
    // hook, not a strong identity boundary on a multi-tenant transport.
    if (options.enforceActorAllowlist) {
      const decision = checkActorAllowlist(companyRoot, actor.createdBy);
      if (!decision.allowed) {
        console.error(`[mcp:withCompanyDb] actor gate: ${decision.reason}`);
        return envelopeToCallResult(
          errorEnvelope(redactPaths(decision.reason), { code: "ACTOR_NOT_ALLOWED" }),
        );
      }
    }
    // `openDb`/`migrate` must stay INSIDE the try: under concurrent load they can
    // throw (lock contention, disk pressure). If that escapes here, the SDK wraps
    // it as an isError text result WITHOUT `structuredContent`, breaking the
    // envelope contract (the "structuredContent undefined" flake). Keeping them
    // inside guarantees every failure returns a proper `{ ok:false }` envelope.
    let db: Database | undefined;
    try {
      if (readOnly) {
        return envelopeToCallResult(await withReadLedger(dbPath, (snapshot) =>
          handler({ db: snapshot, actor, args: { ...args, company: companyRoot } }),
        ));
      }
      db = openDb(dbPath);
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
      db?.close();
    }
  }, "adaptive");
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
  options: { keyIdempotent?: keyof typeof RETRY_CLASS_BY_OPERATION; requireIdempotencyKey?: boolean; idempotencyPayload?: (args: TArgs) => Record<string, unknown> } = {},
): (args: TArgs) => Promise<ReturnType<typeof envelopeToCallResult>> {
  return markCompanyRuntime(async (args) => {
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
    // SEC-2: every confirmed MCP write passes the same actor allowlist gate as
    // the CLI. `confirm: true` alone is no longer sufficient.
    const resolution = resolveCompanyArg(args.company);
    if (!resolution.ok) {
      console.error(`[mcp:withCompanyDbConfirmed] ${resolution.error}: ${args.company}`);
      return envelopeToCallResult(errorEnvelope(redactPaths(resolution.error)));
    }
    const companyRoot = resolution.companyRoot;
    if (!companyOperationTargetExists(companyRoot)) {
      console.error(`[mcp:withCompanyDbConfirmed] company path does not exist: ${companyRoot}`);
      return envelopeToCallResult(errorEnvelope("company path does not exist or is not initialized"));
    }
    const actor = deriveMcpActor(server.server.getClientVersion());
    const decision = checkActorAllowlist(companyRoot, actor.createdBy);
    if (!decision.allowed) {
      console.error(`[mcp:withCompanyDbConfirmed] actor gate: ${decision.reason}`);
      return envelopeToCallResult(errorEnvelope(redactPaths(decision.reason), { code: "ACTOR_NOT_ALLOWED" }));
    }
    try {
      const session = await runCompanyWriteSession(
        { companyRoot, checkBackupLock: !toolName.startsWith("system_") },
        async (db) => {
          const ctx = { db, actor, args: { ...args, company: companyRoot } as TArgs };
          if (!options.keyIdempotent) return handler(ctx);
          try {
            const key = validateIdempotencyKey((ctx.args as TArgs & { idempotencyKey?: unknown }).idempotencyKey);
            if (!key) {
              if (options.requireIdempotencyKey) return errorEnvelope("idempotencyKey is required", { code: "IDEMPOTENCY_KEY_REQUIRED" });
              return handler(ctx);
            }
            const principal = currentMcpAuthenticatedPrincipal();
            const execution = executeLocalIdempotentMutation(ctx.db, {
              key, operation: options.keyIdempotent, workspaceScope: resolveConfiguredWorkspaceRoot() ?? ctx.args.company,
              companyScope: ctx.args.company, principal: principal && { kind: principal.kind, subjectId: principal.subjectId },
              payload: options.idempotencyPayload ? options.idempotencyPayload(ctx.args) : withoutIdempotencyTransportFields(ctx.args as Record<string, unknown>), actor: ctx.actor,
              execute: () => {
                const result = handler(ctx);
                if (result instanceof Promise) throw new IdempotencyError("IDEMPOTENCY_STORAGE_FAILURE", "key-idempotent operation must execute synchronously");
                return result;
              },
            });
            if (!execution.receipt) return execution.result;
            const data = execution.result.data && typeof execution.result.data === "object" ? { ...execution.result.data, idempotency: execution.receipt } : { idempotency: execution.receipt };
            return { ...execution.result, data };
          } catch (error) {
            if (error instanceof IdempotencyError) return errorEnvelope(error.message, { code: error.code });
            throw error;
          }
        },
      );
      if (session.kind === "backup_locked") {
        return envelopeToCallResult(errorEnvelope(
          `Bogføring er låst (${toolName}): ${session.reason}. ` +
            "Diagnosticér med system_backup_status; kør derefter system_backup " +
            "med archive:true for at låse op og placér kopien på en EU/EØS-" +
            "destination med system_backup_place.",
          { code: "BACKUP_LOCKED" },
        ));
      }
      return envelopeToCallResult(session.value);
    } catch (error) {
      return envelopeToCallResult(safeErrorEnvelope("withCompanyDbConfirmed", error instanceof Error ? error.message : String(error)));
    }
  }, "write");
}

/** Read-only company wrapper: never migrates, changes journal mode, or creates sidecars. */
export function withCompanyReadOnlyDb<TArgs extends { company: string }>(
  handler: (ctx: { db: Database; args: TArgs }) => Envelope | Promise<Envelope>,
  options: { allowSchemaNotCurrent?: boolean } = {},
): (args: TArgs) => Promise<ReturnType<typeof envelopeToCallResult>> {
  return markCompanyRuntime(async (args) => {
    if (!args || typeof args.company !== "string" || args.company.length === 0) {
      return envelopeToCallResult(errorEnvelope("company path is required"));
    }
    const resolved = resolveCompanyArg(args.company);
    if (!resolved.ok) return envelopeToCallResult(errorEnvelope(resolved.error));
    const dbPath = companyPaths(resolved.companyRoot).db;
    if (!existsSync(resolved.companyRoot) || !existsSync(dbPath)) {
      return envelopeToCallResult(errorEnvelope("company path does not exist or is not initialized"));
    }
    try {
      return envelopeToCallResult(await withReadLedger(dbPath, (db) =>
        handler({ db, args: { ...args, company: resolved.companyRoot } }),
        options.allowSchemaNotCurrent,
      ));
    } catch (error) {
      return envelopeToCallResult(safeErrorEnvelope("withCompanyReadOnlyDb", error instanceof Error ? error.message : String(error)));
    }
  }, "readonly");
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
    const params: SQLQueryBindings[] = [matchText];
    if (args.matchDate) params.push(args.matchDate);
    if (args.matchDocumentId) params.push(args.matchDocumentId);
    const row = db
      .query(
        `SELECT id FROM journal_entries WHERE text = ? ${dateClause} ${docClause} ORDER BY id DESC LIMIT 1`,
      )
      .get(...params) as { id: number } | null;
    if (row) return asJournalEntryId(row.id);
  }
  return null;
}
