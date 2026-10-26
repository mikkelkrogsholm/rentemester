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

import type { Database } from "bun:sqlite";
import { migrate, openDb } from "../core/db";
import type { ActorContext } from "../core/actor";
import { evaluateBackupLock } from "../core/backup-governance";
import { resolveWorkspaceCompany } from "../core/workspace-company-resolver";
import type { ServerConfig } from "./config";
import { ApiError } from "./errors";
import type { Principal } from "./auth";
import { resolveCockpitActor } from "./actor";
import { executeLocalIdempotentMutation, IdempotencyError, RETRY_CLASS_BY_OPERATION, validateIdempotencyKey, withoutIdempotencyTransportFields, type StablePrincipal } from "../core/idempotency";

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

function httpMutationOperation(request: Request): string {
  const path = new URL(request.url).pathname.replace(/\/companies\/[^/]+/g, "/companies/:company");
  return `${request.method.toUpperCase()} ${path}`;
}

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
  keyIdempotent?: keyof typeof RETRY_CLASS_BY_OPERATION;
  /** Require a receipt key for this operation instead of allowing unkeyed writes. */
  requireIdempotencyKey?: boolean;
  /** Canonical transport-neutral DTO used for durable retry identity. */
  idempotencyPayload?: (body: Record<string, unknown>) => Record<string, unknown>;
};

/**
 * The localhost hard-gate. A Cockpit write is refused when auth is disabled
 * (Phase 1, `authRequired === false`) AND the request did not arrive over a
 * loopback host. Phase 1 trusts the caller purely because it is local; a
 * non-loopback `Host` with no auth would be an unauthenticated write from the
 * network, so it fails closed. When auth IS required the bearer-token check in
 * `authMiddleware` is the gate and this check steps aside.
 */
export function assertLocalhostWriteAllowed(request: Request, config: ServerConfig): void {
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

/**
 * CSRF/DNS-rebinding-hærdning (audit 2026-06-11, SEC-1) — gate 1 af 2:
 * Content-Type.
 *
 * `assertLocalhostWriteAllowed` alene er ikke nok: Host-headeren sættes af
 * browseren til SERVERENS adresse, så et ondsindet website kunne sende en
 * CORS-"simple request" (text/plain) med JSON-body — uden preflight — og
 * udføre writes mod localhost-API'et.
 *
 * Reglen: bærer mutationen en Content-Type-header, SKAL medietypen være
 * `application/json` (parametre som `; charset=utf-8` er ok). En browser kan
 * ikke sende en bodied cross-origin POST uden Content-Type (fetch/XHR/form
 * sætter altid en), så text/plain-vektoren er lukket. En HELT fraværende
 * Content-Type tillades fortsat — det er CLI/curl/ikke-browser-klienter, og
 * den body-løse browser-vektor dækkes af Origin-gaten nedenfor.
 */
export function assertMutationContentType(request: Request): void {
  const raw = request.headers.get("content-type");
  if (raw === null) return;
  const mediaType = (raw.split(";")[0] ?? "").trim().toLowerCase();
  if (mediaType !== "application/json") {
    throw ApiError.badRequest(
      `mutationer kræver 'Content-Type: application/json' — fik '${mediaType || raw.trim()}'`,
      { subcode: "INVALID_CONTENT_TYPE" },
    );
  }
}

/** Loopback-værtsnavne en browser-Origin må pege på — vilkårlig port. */
const LOOPBACK_ORIGIN_HOSTNAMES = new Set([
  "localhost",
  "127.0.0.1",
  "::1",
  "[::1]",
  "0:0:0:0:0:0:0:1",
  "[0:0:0:0:0:0:0:1]",
]);

/**
 * CSRF/DNS-rebinding-hærdning (audit 2026-06-11, SEC-1) — gate 2 af 2: Origin.
 *
 * En browser sætter ALTID en Origin-header på en cross-origin POST (også på
 * simple requests uden preflight) — og siden kan ikke forfalske den. Reglen:
 *
 *   - manglende Origin → tilladt (CLI/curl/MCP/ikke-browser-klienter);
 *   - loopback-origin  → tilladt på ENHVER port: produktion serverer SPA'en
 *                        fra samme loopback-server, og Bun-dev kører på
 *                        http://localhost:5319 og proxy'er `/api` videre
 *                        (app/scripts/serve.ts) med Origin-headeren intakt;
 *   - alt andet        → afvist med stabil subcode FORBIDDEN_ORIGIN. Det
 *                        dækker også `Origin: null` (sandboxed iframe).
 *
 * I lokal/shared-secret drift er en manglende Origin fortsat den eksplicitte
 * CLI/MCP-kontrakt. Hosted Better Auth er anderledes: cookie-sessioner kan
 * sendes af browseren uden en Authorization-header, så alle unsafe custom API
 * calls skal komme fra en valideret `trustedOrigins`-origin. Better Auth
 * beskytter kun `/api/auth/*`, ikke disse routes.
 */
export function assertMutationOriginAllowed(request: Request, config: ServerConfig): void {
  if (config.betterAuthProvider) {
    assertHostedMutationOriginAllowed(request, config);
    return;
  }
  if (config.authRequired) return;
  const origin = (request.headers.get("origin") ?? "").trim();
  if (origin === "") return;
  let parsed: URL | null = null;
  try {
    parsed = new URL(origin);
  } catch {
    parsed = null;
  }
  const allowed =
    parsed !== null &&
    (parsed.protocol === "http:" || parsed.protocol === "https:") &&
    LOOPBACK_ORIGIN_HOSTNAMES.has(parsed.hostname.toLowerCase());
  if (!allowed) {
    throw new ApiError(
      "unauthorized",
      "Skrivehandlinger fra Cockpit accepterer kun browser-kald fra en " +
        "loopback-origin (http://localhost, http://127.0.0.1 eller http://[::1]).",
      { subcode: "FORBIDDEN_ORIGIN" },
    );
  }
}

/**
 * Hosted cookie-authenticated mutations have no headerless CLI escape hatch.
 * A browser supplies Origin; Referer is accepted only as a fallback for
 * privacy-restricted same-origin browser requests. Both are checked as full
 * URL origins against the already validated hosted `trustedOrigins` list.
 */
export function assertHostedMutationOriginAllowed(request: Request, config: ServerConfig): void {
  if (!config.betterAuthProvider) return;
  const trustedOrigins = new Set(config.hostedBetterAuth?.trustedOrigins ?? []);
  const header = (request.headers.get("origin") ?? "").trim();
  const referer = (request.headers.get("referer") ?? "").trim();
  const candidate = header || referer;
  let origin = "";
  try {
    origin = new URL(candidate).origin;
  } catch {
    // Missing, malformed and `Origin: null` all fail closed below.
  }
  if (!candidate || !trustedOrigins.has(origin)) {
    throw new ApiError("unauthorized", "missing or invalid credentials", {
      subcode: "FORBIDDEN_ORIGIN",
    });
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
 *                              auth is disabled; plus browser-hærdning
 *                              (SEC-1): refuse a non-loopback Origin and a
 *                              non-JSON Content-Type;
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
  // (1) Localhost hard-gate + browser-hærdning (SEC-1): en ikke-loopback
  // Origin og en ikke-JSON Content-Type afvises FØR body'en læses.
  assertLocalhostWriteAllowed(request, config);
  assertMutationOriginAllowed(request, config);
  assertMutationContentType(request);

  // Authentication belongs to the router. In Phase 1 actor mapping is fixed,
  // but keep the request principal explicit so Better Auth can replace that
  // mapping without re-authenticating or using mutable request-global state.
  const principal = config.requestPrincipal;
  if (!principal) {
    throw ApiError.unauthorized("request authentication context missing");
  }

  // (2) Company resolution. A registered slug whose ledger is missing on disk
  // is a 404 — the same shape the read routes return.
  const target = resolveWorkspaceCompany(config.workspaceRoot, slug, {
    selection: "registered", archived: "allow", ledger: "required",
  });
  if (!target.ok && target.reason !== "LEDGER_MISSING") {
    throw ApiError.notFound(`ingen virksomhed med slug '${slug}' findes i workspacet`);
  }
  if (!target.ok) {
    throw ApiError.notFound(`virksomheden '${slug}' har ingen ledger`);
  }
  const { companyRoot, ledgerDbPath: dbPath } = target.company;

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

    // (7) Authorization, confirmation, policy and period gates above run for
    // every call. Only a completed, matching receipt skips the executor.
    const result = options.keyIdempotent
      ? (() => {
          try {
            const stable: StablePrincipal | undefined = principal.via === "service-principal"
              ? (principal.serviceAccountId ? { kind: "service-account", subjectId: principal.serviceAccountId } : undefined)
              : (principal.userId ? { kind: "user", subjectId: principal.userId } : undefined);
            const key = validateIdempotencyKey(request.headers.get("idempotency-key") ?? body.idempotencyKey);
            if (!key && options.requireIdempotencyKey) throw new IdempotencyError("IDEMPOTENCY_STORAGE_FAILURE", "idempotency key is required");
            const run = executeLocalIdempotentMutation(db, { key, operation: options.keyIdempotent, principal: stable, payload: options.idempotencyPayload ? options.idempotencyPayload(body) : withoutIdempotencyTransportFields(body), actor, execute: () => {
              const value = handler({ db, actor, companyRoot, principal }, body);
              if (value instanceof Promise) throw new IdempotencyError("IDEMPOTENCY_STORAGE_FAILURE", "key-idempotent HTTP operation must execute synchronously");
              return value;
            }});
            return run.receipt ? Object.assign(run.result, { idempotency: run.receipt }) : run.result;
          } catch (error) { if (error instanceof IdempotencyError) throw ApiError.conflict(error.message, { subcode: error.code }); throw error; }
        })()
      : await handler({ db, actor, companyRoot, principal }, body);

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
    //     (`vat_period period … overlaps existing period …`). Closing an
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
    // An HTTP mutation is complete before its response is observable. A
    // strict close prevents Bun from deferring WAL cleanup until the next
    // (possibly read-only) request in this long-lived server process.
    db.close(true);
  }
}
