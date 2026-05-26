// Bilagsmail — IMAP config storage + per-company mail alias (#348, #350).
//
// IMAP credentials lever ALDRIG i ledger-DB'en; de gemmes i en JSON-fil i
// virksomhedens config-mappe (`<companyRoot>/config/imap.json`) med 0600-
// rettigheder. Cockpit/CLI er ansvarlig for at hydrere ImapConfig før hver
// poll (det er den eksisterende contract i imap-intake.ts:22).
//
// Mail-alias derimod ER en del af virksomhedens stamdata (ikke et secret) og
// gemmes i `companies.mail_alias`-kolonnen, så den kan returneres af
// /api/companies/:slug og vises i cockpit-views uden ekstra fil-IO.

import { existsSync, mkdirSync, readFileSync, writeFileSync, chmodSync } from "node:fs";
import { join } from "node:path";
import type { Database } from "bun:sqlite";

export type BilagsmailImapConfig = {
  host: string;
  port: number;
  /** TLS on connect (IMAPS). Defaults to true. */
  secure?: boolean;
  username: string;
  /** Password is stored in plain text in the config file (0600). */
  password: string;
  mailbox?: string;
};

function imapConfigPath(companyRoot: string): string {
  return join(companyRoot, "config", "imap.json");
}

function ensureConfigDir(companyRoot: string): void {
  mkdirSync(join(companyRoot, "config"), { recursive: true });
}

export function loadBilagsmailImapConfig(
  companyRoot: string,
): BilagsmailImapConfig | null {
  const path = imapConfigPath(companyRoot);
  if (!existsSync(path)) return null;
  const raw = readFileSync(path, "utf8");
  return JSON.parse(raw) as BilagsmailImapConfig;
}

export function saveBilagsmailImapConfig(
  companyRoot: string,
  config: BilagsmailImapConfig,
): { path: string } {
  if (!config.host?.trim()) throw new Error("imap config: host is required");
  if (!Number.isInteger(config.port) || config.port <= 0) {
    throw new Error("imap config: port must be a positive integer");
  }
  if (!config.username?.trim()) throw new Error("imap config: username is required");
  if (!config.password?.trim()) throw new Error("imap config: password is required");
  ensureConfigDir(companyRoot);
  const path = imapConfigPath(companyRoot);
  const normalized: BilagsmailImapConfig = {
    host: config.host.trim(),
    port: config.port,
    secure: config.secure ?? true,
    username: config.username.trim(),
    password: config.password,
    mailbox: config.mailbox?.trim() || "INBOX",
  };
  writeFileSync(path, JSON.stringify(normalized, null, 2));
  // 0600 — only owner can read the password.
  try {
    chmodSync(path, 0o600);
  } catch {
    // Best-effort; some filesystems (FAT) don't support unix perms.
  }
  return { path };
}

export function deleteBilagsmailImapConfig(companyRoot: string): boolean {
  const path = imapConfigPath(companyRoot);
  if (!existsSync(path)) return false;
  // Overwrite first to obliterate the in-file password before unlinking
  writeFileSync(path, "{}\n");
  // Defer to fs.unlinkSync via require — Bun supports node:fs.
  try {
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    const fs = require("node:fs") as typeof import("node:fs");
    fs.unlinkSync(path);
    return true;
  } catch {
    return true;
  }
}

// 3-64 chars total: first + middle (1-62) + last; pattern enforces leading
// and trailing alphanumeric so the localpart is always RFC-5322-friendly.
const ALIAS_PATTERN = /^[a-z0-9][a-z0-9._-]{1,62}[a-z0-9]$/;

/**
 * Sets the per-company mail alias (#350). Stores in
 * `companies.mail_alias`. Validation:
 *   - 3–64 chars, lowercase a-z, 0-9, dot, underscore, dash.
 *   - Starts and ends with alphanumeric.
 *   - Must be unique across the workspace — but uniqueness across companies
 *     is enforced by the caller (cockpit/CLI) because each company has its
 *     own SQLite DB; this function only owns the per-company write.
 */
export function setCompanyMailAlias(db: Database, alias: string | null): void {
  if (alias === null || alias === "") {
    db.run("UPDATE companies SET mail_alias = NULL WHERE id = 1");
    return;
  }
  const normalized = alias.trim().toLowerCase();
  if (!ALIAS_PATTERN.test(normalized)) {
    throw new Error(
      "mail_alias must be 3-64 chars, lowercase alnum/./_/-, starting and ending alphanumeric",
    );
  }
  db.run("UPDATE companies SET mail_alias = ? WHERE id = 1", normalized);
}

export function getCompanyMailAlias(db: Database): string | null {
  const row = db
    .query("SELECT mail_alias FROM companies WHERE id = 1 LIMIT 1")
    .get() as { mail_alias: string | null } | null;
  return row?.mail_alias ?? null;
}
