/**
 * Shared lifecycle for one company-scoped write operation.
 *
 * Surface adapters decide confirmation, identity and response formatting.
 * This module owns only the resolved target plus one open/migrate/lock/close
 * session, so a cross-cutting lock check cannot accidentally create a second
 * SQLite session before the actual operation.
 */

import { existsSync } from "node:fs";
import { isAbsolute, resolve, sep } from "node:path";
import type { Database } from "bun:sqlite";
import { evaluateBackupLock } from "./backup-governance";
import { migrate, openDb } from "./db";
import { companyPaths } from "./paths";
import { isValidSlug, resolveConfiguredWorkspaceRoot, resolveWorkspaceSlug } from "./workspace";

export type CompanyOperationTarget =
  | { ok: true; companyRoot: string }
  | { ok: false; error: string };

/** Resolve a workspace slug or a safe absolute company directory. */
export function resolveCompanyOperationTarget(raw: string): CompanyOperationTarget {
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
        error: `ingen virksomhed med slug '${raw}' findes i det konfigurerede workspace (slug ikke i workspace-manifestet — tjek de registrerede slugs)`,
      };
    }
    return {
      ok: false,
      error: `intet workspace konfigureret: sæt RENTEMESTER_WORKSPACE til en workspace-mappe for at bruge slug '${raw}', eller angiv en absolut virksomhedssti i stedet`,
    };
  }

  const segments = raw.split(/[\\/]+/);
  if (segments.includes("..")) {
    return { ok: false, error: "company must not contain parent-directory ('..') segments" };
  }
  const companyRoot = resolve(raw);
  if (!isAbsolute(companyRoot) || companyRoot.split(sep).includes("..")) {
    return { ok: false, error: "company resolved to an unsafe path" };
  }
  return { ok: true, companyRoot };
}

export type CompanyWriteSessionResult<T> =
  | { kind: "completed"; value: T }
  | { kind: "backup_locked"; reason: string };

type CompanyWriteSessionOptions = {
  companyRoot: string;
  checkBackupLock: boolean;
};

type CompanyWriteSessionDependencies = {
  openDb: typeof openDb;
  migrate: typeof migrate;
  evaluateBackupLock: typeof evaluateBackupLock;
};

const defaultDependencies: CompanyWriteSessionDependencies = { openDb, migrate, evaluateBackupLock };

/**
 * Opens and migrates exactly one writable database session, optionally checks
 * the bookkeeping lock in that same session, and always closes it.
 */
export async function runCompanyWriteSession<T>(
  options: CompanyWriteSessionOptions,
  operation: (db: Database) => T | Promise<T>,
  dependencies: CompanyWriteSessionDependencies = defaultDependencies,
): Promise<CompanyWriteSessionResult<T>> {
  const db = dependencies.openDb(companyPaths(options.companyRoot).db);
  try {
    dependencies.migrate(db);
    if (options.checkBackupLock) {
      const lock = dependencies.evaluateBackupLock(db, options.companyRoot);
      if (lock.locked) return { kind: "backup_locked", reason: lock.reason };
    }
    return { kind: "completed", value: await operation(db) };
  } finally {
    db.close();
  }
}

/** Kept beside the session helper so adapters share the initialized-target rule. */
export function companyOperationTargetExists(companyRoot: string): boolean {
  return existsSync(companyRoot);
}
