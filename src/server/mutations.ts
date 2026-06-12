// The shared write pipeline for the Cockpit backend (#213, slice 1).
//
// Background — why this file must exist:
//   The Cockpit (`serve`) becomes a THIRD write-stack over `src/core/`,
//   alongside the CLI (`src/cli.ts`) and the MCP server (`src/mcp/`). All
//   three reuse the SAME core bookkeeping functions — but two cross-cutting
//   policies do NOT live in `core`:
//
//     1. the BEK 205/2024 §4 backup lock — enforced in the CLI dispatch
//        (`src/cli.ts`) and in the MCP `registerTool` interceptor
//        (`src/mcp/registry.ts`);
//     2. actor attribution — resolved in the CLI from `--actor`/env, and in
//        MCP from the client handshake (`src/mcp/actor.ts`).
//
//   A server write path inherits NEITHER. Without replicating them here, a
//   Cockpit mutation would bypass the statutory backup lock and write
//   un-attributed `audit_log` rows. `withCompanyMutation` is the server's
//   analogue of MCP's `withCompanyDb` (`src/mcp/tool-runtime.ts`): it owns the
//   lock gate, the confirm gate, the actor resolution and the localhost
//   hard-gate so every Cockpit write goes through exactly one door.
//
// A handler wrapped by `withCompanyMutation` receives an OPEN, migrated db, a
// resolved core `ActorContext`, and a parsed body. It returns a core-style
// `{ ok, errors }` result; a business rejection (`ok:false`) is mapped to a
// 400/409 `ApiError`, never a 500.

import { existsSync } from "node:fs";
import type { Database } from "bun:sqlite";
import { migrate, openDb } from "../core/db";
import { companyPaths } from "../core/paths";
import type { ActorContext } from "../core/actor";
import { evaluateBackupLock } from "../core/backup-governance";
import { findWorkspaceCompany, companyRootForSlug } from "../core/workspace";
import type { ServerConfig } from "./config";
import { ApiError } from "./errors";
import { authMiddleware, type Principal } from "./auth";
import { resolveCockpitActor } from "./actor";

/**
 * The context handed to a write handler once every gate has passed:
 * an open + migrated db, the resolved actor, and the company root on disk.
 */
export type MutationContext = {
  db: Database;
  actor: ActorContext;
  companyRoot: string;
  /** The authenticated principal (Phase 1: always the localhost actor). */
  principal: Principal;
};

/** A core-style business result — what `resolveException` and friends return. */
export type CoreResult = { ok: boolean; errors?: string[] };

export type WithCompanyMutationOptions = {
  /**
   * When true the action is DESTRUCTIVE and the request body must carry
   * `confirm: true` — mirrors `withCompanyDbConfirmed` in MCP. Slice 1's
   * resolve-exception action is non-destructive (the UI modal IS the consent),
   * so it leaves this off; slices 2–4 set it for irreversible postings.
   */
  requireConfirm?: boolean;
  /**
   * Optional hard cap on the request body size, in bytes. When set, a body
   * larger than this is rejected with a 400 *before* it is read into memory —
   * DoS hardening for the file-upload routes (#213, slices 2-3), where the
   * frontend POSTs CSV text / base64-encoded documents inline. Routes whose
   * body is a tiny JSON object (slice 1's resolve-exception) leave this off.
   */
  maxBodyBytes?: number;
};

/**
 * The localhost hard-gate. A Cockpit write is refused when auth is disabled
 * (Phase 1, `authRequired === false`) AND the request did not arrive over a
 * loopback host. Phase 1 trusts the caller purely because it is local; a
 * non-loopback `Host` with no auth would be an unauthenticated write from the
 * network, so it fails closed. When auth IS required the bearer-token check in
 * `authMiddleware` is the gate and this check steps aside.
 */
function assertLocalhostWriteAllowed(request: Request, config: ServerConfig): void {
  if (config.authRequired) return;
  const hostHeader = (request.headers.get("host") ?? "").trim().toLowerCase();
  // Strip the optional `:port` suffix — but not from a bracketed IPv6 host.
  const host = hostHeader.startsWith("[")
    ? hostHeader.slice(1, hostHeader.indexOf("]") === -1 ? undefined : hostHeader.indexOf("]"))
    : (hostHeader.split(":")[0] ?? "");
  const isLoopback =
    host === "127.0.0.1" ||
    host === "localhost" ||
    host === "::1" ||
    host === "0:0:0:0:0:0:0:1";
  if (!isLoopback) {
    throw ApiError.unauthorized(
      "Skrivehandlinger fra Cockpit er kun tilladt fra localhost, " +
        "medmindre godkendelse er slået til.",
    );
  }
}

/** The shared Danish backup-lock message — kept identical to the CLI/MCP wording. */
function backupLockMessage(reason: string): string {
  return (
    `Bogføring er låst: ${reason}. ` +
    "Kør en backup (system backup) for at låse op. Placér derefter backup-arkivet " +
    "på en EU/EØS-destination for at opfylde BEK 205/2024 § 4."
  );
}

/**
 * Reads + JSON-parses the request body, mapping any failure to a 400. An empty
 * body is treated as `{}` so a handler whose fields are all optional (the
 * resolve-exception `{ note? }` body) works with no body at all.
 *
 * When `maxBodyBytes` is given the body is size-checked: a declared
 * `Content-Length` over the cap is rejected before reading, and the actual
 * byte length is re-checked after reading (a `Content-Length` header is
 * client-supplied and may lie). This is the DoS guard for the file-upload
 * routes — a Cockpit CSV/document POST carries inline file content.
 */
async function readMutationBody(
  request: Request,
  maxBodyBytes?: number,
): Promise<Record<string, unknown>> {
  if (maxBodyBytes !== undefined) {
    const declared = Number(request.headers.get("content-length") ?? "");
    if (Number.isFinite(declared) && declared > maxBodyBytes) {
      throw ApiError.badRequest(
        `request-body overskrider grænsen på ${maxBodyBytes} bytes`,
      );
    }
  }
  const raw = await request.text();
  if (maxBodyBytes !== undefined) {
    const actual = Buffer.byteLength(raw, "utf8");
    if (actual > maxBodyBytes) {
      throw ApiError.badRequest(
        `request-body overskrider grænsen på ${maxBodyBytes} bytes`,
      );
    }
  }
  if (raw.trim().length === 0) return {};
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    throw ApiError.badRequest("request body must be valid JSON");
  }
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
    throw ApiError.badRequest("request body must be a JSON object");
  }
  return parsed as Record<string, unknown>;
}

/**
 * Runs a Cockpit write through every cross-cutting gate, in order:
 *
 *   1. localhost hard-gate   — refuse a write from a non-loopback host when
 *                              auth is disabled;
 *   2. company resolution    — slug → company root, ledger-exists check (404);
 *   3. confirm gate          — when `requireConfirm`, the body must carry
 *                              `confirm: true` (else 400);
 *   4. open + migrate db     — closed in `finally`, always;
 *   5. backup-lock gate      — `evaluateBackupLock`; if locked → 409 conflict
 *                              with the shared Danish message;
 *   6. actor resolution      — `Principal` → core `ActorContext`;
 *   7. handler               — receives the open db, actor and parsed body;
 *   8. business-result map   — a core `{ ok:false, errors }` → 400/409 here,
 *                              never a 500.
 *
 * The handler returns a `CoreResult` plus whatever extra fields the route wants
 * echoed back to the client; those extra fields are returned untouched.
 */
export async function withCompanyMutation<T extends CoreResult>(
  request: Request,
  config: ServerConfig,
  slug: string,
  handler: (ctx: MutationContext, body: Record<string, unknown>) => T | Promise<T>,
  options: WithCompanyMutationOptions = {},
): Promise<T> {
  // (1) Localhost hard-gate.
  assertLocalhostWriteAllowed(request, config);

  // The auth seam already ran once in `handleRequest`; re-running it here is
  // cheap and yields the typed `Principal` the actor mapper needs without
  // threading it through every route signature.
  const principal = authMiddleware(request, config);

  // (2) Company resolution. A registered slug whose ledger is missing on disk
  // is a 404 — the same shape the read routes return.
  if (!findWorkspaceCompany(config.workspaceRoot, slug)) {
    throw ApiError.notFound(`ingen virksomhed med slug '${slug}' findes i workspacet`);
  }
  const companyRoot = companyRootForSlug(config.workspaceRoot, slug);
  const dbPath = companyPaths(companyRoot).db;
  if (!existsSync(dbPath)) {
    throw ApiError.notFound(`virksomheden '${slug}' har ingen ledger`);
  }

  const body = await readMutationBody(request, options.maxBodyBytes);

  // (3) Confirm gate for destructive actions.
  if (options.requireConfirm && body.confirm !== true) {
    throw ApiError.badRequest(
      "denne handling er irreversibel og kræver 'confirm: true'",
      // Stable, cross-surface code so an agent driving HTTP gets the
      // same machine-readable marker the MCP envelope sets. (Batch F-1)
      { subcode: "CONFIRM_REQUIRED" },
    );
  }

  // (4) Open + migrate; (5)/(6)/(7)/(8) run with the db open.
  const db = openDb(dbPath);
  try {
    migrate(db);

    // (5) Backup-lock gate. Replicates the CLI dispatch + MCP interceptor:
    // when the owner has opted into enforcement and a weekly backup is overdue
    // past the grace window, every bookkeeping write is refused with a 409.
    const lock = evaluateBackupLock(db, companyRoot);
    if (lock.locked) {
      throw ApiError.conflict(backupLockMessage(lock.reason));
    }

    // (6) Actor resolution — fixed Phase-1 web actor.
    const actor = resolveCockpitActor(principal);

    // (7) Handler.
    const result = await handler({ db, actor, companyRoot, principal }, body);

    // (8) Business-result map. A core rejection is the caller's fault, not the
    // server's — surface it as a 400 (or 409 for a conflict-shaped message),
    // never a 500.
    //
    // The conflict heuristic distinguishes a genuine state conflict (the
    // target is missing, or the action already happened) from a plain
    // bad-input rejection. Three message families count as conflicts:
    //   - "missing target": `findes ikke` / `does not exist` / `not found`;
    //   - "already done":   `allerede` (Danish) OR `already` (English);
    //   - "overlapping state": `overlaps` — the period core (#287) refuses a
    //     `period close` whose range collides with an existing period
    //     (`vat_quarter period … overlaps existing period …`). Closing an
    //     already-closed period is a conflict with existing ledger state,
    //     not a bad input, so it is a 409.
    // The invoice core (#213, slice 4) speaks ENGLISH for its idempotency
    // rejections — `invoice X already has journal entry Y` (double post),
    // `bank transaction N is already linked …` (double settle). Those are
    // 409s, not 400s: re-issuing a post/settle is a conflict with existing
    // ledger state, exactly like resolving an already-resolved exception.
    // The Danish-only `allerede` test missed them, so `already` is added.
    if (result.ok === false) {
      const message =
        result.errors && result.errors.length > 0
          ? result.errors.join("; ")
          : "handlingen blev afvist";
      if (
        /findes ikke|does not exist|not found|allerede|already|overlaps/i.test(
          message,
        )
      ) {
        throw ApiError.conflict(message);
      }
      throw ApiError.badRequest(message);
    }

    return result;
  } finally {
    db.close();
  }
}
