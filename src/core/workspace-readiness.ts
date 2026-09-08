// Read-only workspace readiness checks.  This module deliberately never calls
// the normal DB openers: those create directories and apply migrations.

import { Database } from "bun:sqlite";
import { existsSync, lstatSync } from "node:fs";
import { companyPaths } from "./paths";
import { inspectOpenLedger, openLedgerReadOnly } from "./ledger-inspection";
import { openSqliteReadOnlySnapshot } from "./sqlite-readonly-snapshot";
import {
  CURRENT_WORKSPACE_CONTROL_SCHEMA_VERSION,
  assertWorkspaceControlCompatibility,
  assertWorkspaceControlPrimitives,
  readWorkspaceControlMigrations,
  workspaceControlPaths,
} from "./workspace-control";
import {
  WORKSPACE_MANIFEST_FILE,
  companyRootForSlug,
  isValidSlug,
  loadWorkspaceManifest,
  resolveCanonicalLiveCompanies,
  type CanonicalLiveCompanyDiagnostic,
  workspaceExists,
} from "./workspace";
import { verifyAuditChain } from "./ledger";
import { join } from "node:path";

export type ReadinessCheck = "ok" | "failed";

/** Public-safe aggregate only: never attach exception text or file identity. */
export type WorkspaceReadiness = {
  ready: boolean;
  checks: {
    workspaceManifest: ReadinessCheck;
    workspaceControl: ReadinessCheck;
    companyLedgers: ReadinessCheck;
  };
  companyCount: number;
  diagnostics: CanonicalLiveCompanyDiagnostic[];
};

function isRegularFile(path: string): boolean {
  try {
    const stat = lstatSync(path);
    return stat.isFile() && !stat.isSymbolicLink();
  } catch {
    return false;
  }
}

function openReadonly(path: string): Database {
  if (!existsSync(path) || !isRegularFile(path)) {
    throw new Error("readiness database is unavailable");
  }
  const db = openSqliteReadOnlySnapshot(path);
  try {
    // The connection-level guard makes accidental write SQL fail even if a
    // future check is changed. Neither pragma persists into the database.
    db.exec("PRAGMA query_only = ON; PRAGMA foreign_keys = ON;");
    return db;
  } catch (error) {
    db.close();
    throw error;
  }
}

function assertQuickAndForeignKeyIntegrity(db: Database): void {
  const quickCheckRows = db.query("PRAGMA quick_check").all() as Array<Record<string, unknown>>;
  if (
    quickCheckRows.length !== 1 ||
    Object.values(quickCheckRows[0]!).length !== 1 ||
    Object.values(quickCheckRows[0]!)[0] !== "ok"
  ) {
    throw new Error("sqlite quick check failed");
  }
  if (db.query("PRAGMA foreign_key_check").all().length !== 0) {
    throw new Error("sqlite foreign key check failed");
  }
}

function checkWorkspaceControl(workspaceRoot: string): boolean {
  let db: Database | undefined;
  try {
    db = openReadonly(workspaceControlPaths(workspaceRoot).db);
    assertWorkspaceControlCompatibility(db);
    const migrations = readWorkspaceControlMigrations(db);
    if (
      migrations.length !== CURRENT_WORKSPACE_CONTROL_SCHEMA_VERSION ||
      migrations.at(-1)?.id !== CURRENT_WORKSPACE_CONTROL_SCHEMA_VERSION
    ) {
      throw new Error("workspace control schema is not current");
    }
    assertWorkspaceControlPrimitives(db);
    assertQuickAndForeignKeyIntegrity(db);
    return true;
  } catch {
    return false;
  } finally {
    db?.close();
  }
}

function checkLedger(workspaceRoot: string, slug: string): boolean {
  let db: Database | undefined;
  try {
    const companyRoot = companyRootForSlug(workspaceRoot, slug);
    db = openLedgerReadOnly(companyPaths(companyRoot).db);
    if (inspectOpenLedger(db).status !== "current") {
      throw new Error("ledger schema is not current");
    }
    // This is intentionally the existing full ledger/audit verifier. It is
    // read-only, including its evidence-file reads, and makes readiness fail
    // closed on an append-only or evidence-integrity violation.
    if (!verifyAuditChain(db, { companyRoot }).ok) {
      throw new Error("ledger audit verification failed");
    }
    return true;
  } catch {
    return false;
  } finally {
    db?.close();
  }
}

/**
 * Checks only the configured manifest entries. It never discovers/adopts
 * directories, writes a migration, creates a DB, or returns local paths.
 */
export function assessWorkspaceReadiness(workspaceRoot: string): WorkspaceReadiness {
  let manifestOk = false;
  let canonicalLiveOk = false;
  let companyCount = 0;
  let slugs: string[] = [];
  let diagnostics: CanonicalLiveCompanyDiagnostic[] = [];
  try {
    const manifestPath = join(workspaceRoot, WORKSPACE_MANIFEST_FILE);
    if (!workspaceExists(workspaceRoot) || !isRegularFile(manifestPath)) {
      throw new Error("workspace manifest is unavailable");
    }
    const manifest = loadWorkspaceManifest(workspaceRoot);
    const seen = new Set<string>();
    for (const company of manifest.companies) {
      if (!isValidSlug(company.slug) || seen.has(company.slug)) {
        throw new Error("workspace manifest has invalid companies");
      }
      seen.add(company.slug);
    }
    const canonical = resolveCanonicalLiveCompanies(workspaceRoot);
    diagnostics = canonical.blockers;
    canonicalLiveOk = diagnostics.length === 0;
    slugs = canonical.companies.map((company) => company.entry.slug);
    companyCount = slugs.length;
    manifestOk = true;
  } catch {
    // The public result deliberately contains no parser/filesystem detail.
  }

  const workspaceControlOk = checkWorkspaceControl(workspaceRoot);
  const companyLedgersOk = manifestOk && canonicalLiveOk && slugs.every((slug) => checkLedger(workspaceRoot, slug));
  const checks = {
    workspaceManifest: manifestOk ? "ok" : "failed",
    workspaceControl: workspaceControlOk ? "ok" : "failed",
    companyLedgers: companyLedgersOk ? "ok" : "failed",
  } as const;
  return {
    ready: checks.workspaceManifest === "ok" &&
      checks.workspaceControl === "ok" &&
      checks.companyLedgers === "ok",
    checks,
    companyCount: manifestOk ? companyCount : 0,
    diagnostics: manifestOk ? diagnostics : [],
  };
}
