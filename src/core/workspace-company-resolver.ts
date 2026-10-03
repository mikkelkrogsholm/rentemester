/**
 * Transport-neutral resolution of a company registered in a workspace.
 *
 * This is deliberately a filesystem/manifest preamble only. Authentication,
 * database sessions, migrations, backup locks and response mapping stay with
 * their respective transports.
 */
import { existsSync } from "node:fs";
import { companyPaths, type CompanyPaths } from "./paths";
import {
  companyRootForSlug,
  findRoutableWorkspaceCompany,
  findWorkspaceCompany,
  isCompanyInsideWorkspace,
  isValidSlug,
  type WorkspaceCompanyEntry,
} from "./workspace";

export type WorkspaceCompanySelection = "registered" | "routable-live";
export type WorkspaceCompanyArchivePolicy = "allow" | "deny";
export type WorkspaceCompanyLedgerPolicy = "required" | "optional";

export type WorkspaceCompanyResolutionOptions = Readonly<{
  selection: WorkspaceCompanySelection;
  archived: WorkspaceCompanyArchivePolicy;
  ledger: WorkspaceCompanyLedgerPolicy;
}>;

export type ResolvedWorkspaceCompany = Readonly<{
  entry: Readonly<WorkspaceCompanyEntry>;
  companyRoot: string;
  paths: Readonly<CompanyPaths>;
  ledgerDbPath: string;
}>;

export type WorkspaceCompanyResolutionFailure =
  | "INVALID_SLUG"
  | "NOT_REGISTERED"
  | "NOT_ROUTABLE_LIVE"
  | "ARCHIVED"
  | "PATH_OUTSIDE_WORKSPACE"
  | "LEDGER_MISSING";

export type WorkspaceCompanyResolution =
  | Readonly<{ ok: true; company: ResolvedWorkspaceCompany }>
  | Readonly<{ ok: false; reason: WorkspaceCompanyResolutionFailure }>;

/**
 * Resolves a manifest company without opening a database. The returned values
 * are immutable snapshots so a caller cannot accidentally change the manifest
 * entry or derived paths before passing them to its own transport adapter.
 */
export function resolveWorkspaceCompany(
  workspaceRoot: string,
  slug: string,
  options: WorkspaceCompanyResolutionOptions,
): WorkspaceCompanyResolution {
  if (!isValidSlug(slug)) return Object.freeze({ ok: false as const, reason: "INVALID_SLUG" as const });

  const entry = options.selection === "routable-live"
    ? findRoutableWorkspaceCompany(workspaceRoot, slug)
    : findWorkspaceCompany(workspaceRoot, slug);
  if (!entry) {
    return Object.freeze({
      ok: false as const,
      reason: options.selection === "routable-live" ? "NOT_ROUTABLE_LIVE" as const : "NOT_REGISTERED" as const,
    });
  }
  if (options.archived === "deny" && entry.archived) {
    return Object.freeze({ ok: false as const, reason: "ARCHIVED" as const });
  }

  const lexicalCompanyRoot = companyRootForSlug(workspaceRoot, entry.slug);
  // The shared path gate includes canonical child identity and symlink checks.
  if (!isCompanyInsideWorkspace(workspaceRoot, lexicalCompanyRoot)) {
    return Object.freeze({ ok: false as const, reason: "PATH_OUTSIDE_WORKSPACE" as const });
  }
  const companyRoot = lexicalCompanyRoot;
  const paths = companyPaths(companyRoot);
  if (options.ledger === "required" && !existsSync(paths.db)) {
    return Object.freeze({ ok: false as const, reason: "LEDGER_MISSING" as const });
  }

  return Object.freeze({
    ok: true as const,
    company: Object.freeze({
      entry: Object.freeze({ ...entry }),
      companyRoot,
      paths: Object.freeze({ ...paths }),
      ledgerDbPath: paths.db,
    }),
  });
}
