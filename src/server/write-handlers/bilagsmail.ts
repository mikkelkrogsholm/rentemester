// Bilagsmail IMAP config + alias handlers (#348, #350).

import {
  deleteBilagsmailImapConfig,
  saveBilagsmailImapConfig,
  setCompanyMailAlias,
  type BilagsmailImapConfig,
} from "../../core/bilagsmail";
import type { ServerConfig } from "../config";
import { ApiError } from "../errors";
import { withCompanyMutation } from "../mutations";
import {
  okResponse,
  optionalBodyString,
  requireBodyString,
} from "./_shared";

/**
 * POST /api/companies/:slug/bilagsmail/imap-config — Saves the IMAP config to
 * `config/imap.json` (0600). Body: `{ host, port, username, password, secure?,
 * mailbox? }`. Credentials never enter the ledger — they live as a per-company
 * config-file (#348).
 */
export async function handleSaveBilagsmailImapConfig(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  await withCompanyMutation(request, config, slug, (ctx, body) => {
    const host = requireBodyString(body, "host");
    const port = body["port"];
    if (typeof port !== "number" || !Number.isInteger(port) || port <= 0) {
      throw ApiError.badRequest("'port' must be a positive integer");
    }
    const username = requireBodyString(body, "username");
    const password = requireBodyString(body, "password");
    const secure = body["secure"] !== false; // default true
    const mailbox = optionalBodyString(body, "mailbox") ?? "INBOX";
    const imapConfig: BilagsmailImapConfig = {
      host,
      port,
      username,
      password,
      secure,
      mailbox,
    };
    saveBilagsmailImapConfig(ctx.companyRoot, imapConfig);
    void ctx.actor;
    return { ok: true, errors: [] as string[] };
  });
  return okResponse({ imapConfig: { ok: true } });
}

/**
 * DELETE /api/companies/:slug/bilagsmail/imap-config — Removes the stored
 * IMAP config from disk (#348).
 */
export async function handleDeleteBilagsmailImapConfig(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  await withCompanyMutation(request, config, slug, (ctx) => {
    deleteBilagsmailImapConfig(ctx.companyRoot);
    void ctx.actor;
    return { ok: true, errors: [] as string[] };
  });
  return okResponse({ imapConfig: { ok: true, removed: true } });
}

/**
 * PATCH /api/companies/:slug/bilagsmail/alias — Sets or clears the per-company
 * mail alias used as the localpart in the bilagsmail address (#350).
 * Body: `{ alias: string | null }`.
 */
export async function handleSetBilagsmailAlias(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  await withCompanyMutation(request, config, slug, (ctx, body) => {
    const raw = body["alias"];
    // Only a string sets the alias and only null/absent clears it. A
    // present-but-non-string value (e.g. `{ alias: 123 }`) is a malformed
    // request that INTENDS to set an alias — rejecting it with a 400 (like
    // every other body parser) is far safer than the old coercion-to-null,
    // which silently ERASED the company's existing alias. The core trims and
    // lower-cases the string, so no handler-side normalisation is needed.
    if (raw !== null && raw !== undefined && typeof raw !== "string") {
      throw ApiError.badRequest("'alias' skal være en streng eller null");
    }
    const alias = typeof raw === "string" ? raw : null;
    setCompanyMailAlias(ctx.db, alias);
    void ctx.actor;
    return { ok: true, errors: [] as string[] };
  });
  return okResponse({ mailAlias: { ok: true } });
}
