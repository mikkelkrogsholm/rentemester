import { existsSync, mkdirSync, readFileSync } from "node:fs";
import { isAbsolute, join, resolve, sep } from "node:path";
import { writeFileAtomic } from "./atomic-file";
import { companyPaths } from "./paths";
import { openLedgerReadOnly } from "./ledger-inspection";

/**
 * Workspace model.
 *
 * A *workspace* is a root directory that owns one or more company volumes as
 * subdirectories `<workspace>/<slug>/`. Each subdirectory is an ordinary
 * Rentemester company directory (a `data/ledger.sqlite` plus the usual
 * `companyPaths` subdirs).
 *
 * `workspace.json` is authoritative. A directory is not a workspace company
 * until an explicit write registers it in the manifest.
 *
 * This module is intentionally pure filesystem + JSON: the later cockpit API
 * (#170) and MCP tools (#172) call these same functions.
 */

export const WORKSPACE_MANIFEST_FILE = "workspace.json";

const MANIFEST_VERSION = 2 as const;
const LEGACY_MANIFEST_VERSION = 1 as const;

export type WorkspaceCompanyPurpose = "live" | "test" | "dry-run" | "restore" | "backup" | "retest" | "baseline";

export type WorkspaceCompanyEntry = {
  /** Filesystem-safe identifier; the subdirectory name under the workspace. */
  slug: string;
  /** Human-readable display name. */
  name: string;
  /** ISO-8601 timestamp of when the entry was first registered. */
  createdAt: string;
  /** Soft-deletion flag; archived companies stay on disk. */
  archived: boolean;
  /** `live` is the backwards-compatible default for pre-purpose manifests. */
  purpose?: WorkspaceCompanyPurpose;
};

export type WorkspaceManifest = {
  version: typeof MANIFEST_VERSION;
  companies: WorkspaceCompanyEntry[];
};

/**
 * Derives a deterministic, filesystem-safe slug from a display name.
 * Danish letters are transliterated so a slug never depends on locale.
 */
export function slugifyCompanyName(name: string): string {
  const slug = name
    .trim()
    .toLowerCase()
    .replace(/æ/g, "ae")
    .replace(/ø/g, "oe")
    .replace(/å/g, "aa")
    .replace(/[àáâãä]/g, "a")
    .replace(/[èéêë]/g, "e")
    .replace(/[ìíîï]/g, "i")
    .replace(/[òóôõö]/g, "o")
    .replace(/[ùúûü]/g, "u")
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return slug;
}

/**
 * Validates a slug used as a workspace subdirectory name. A slug must be a
 * single safe path segment — no separators, no traversal, no leading dot.
 */
export function isValidSlug(slug: string): boolean {
  return /^[a-z0-9][a-z0-9-]*$/.test(slug);
}

function assertValidSlug(slug: string): void {
  if (!isValidSlug(slug)) {
    throw new Error(
      `invalid company slug '${slug}': use lowercase letters, digits and dashes`,
    );
  }
}

function manifestPath(workspaceRoot: string): string {
  return join(workspaceRoot, WORKSPACE_MANIFEST_FILE);
}

/** True when `workspaceRoot` already holds a `workspace.json` manifest. */
export function workspaceExists(workspaceRoot: string): boolean {
  return existsSync(manifestPath(workspaceRoot));
}

/**
 * Resolves and validates a workspace root path. Rejects `..` traversal so the
 * workspace tree cannot be relocated by a malicious value, mirroring the
 * `--company` path guard in `src/cli.ts`.
 */
export function resolveWorkspaceRoot(raw: string): string {
  if (raw.split(/[\\/]+/).includes("..")) {
    throw new Error("workspace root must not contain parent-directory ('..') segments");
  }
  const resolved = resolve(raw);
  if (!isAbsolute(resolved) || resolved.split(sep).includes("..")) {
    throw new Error("workspace root resolved to an unsafe path");
  }
  return resolved;
}

/**
 * Reads and validates the configured workspace root from
 * `RENTEMESTER_WORKSPACE`. Returns null when the env var is unset/empty so
 * callers can fall back to the legacy single-company (`--company <path>`)
 * behaviour. Throws on an unsafe value.
 */
export function resolveConfiguredWorkspaceRoot(): string | null {
  const raw = process.env.RENTEMESTER_WORKSPACE;
  if (typeof raw !== "string" || raw.trim().length === 0) return null;
  return resolveWorkspaceRoot(raw.trim());
}

function emptyManifest(): WorkspaceManifest {
  return { version: MANIFEST_VERSION, companies: [] };
}

function purposeOf(entry: WorkspaceCompanyEntry): WorkspaceCompanyPurpose {
  return entry.purpose ?? "live";
}

const PURPOSES = new Set<WorkspaceCompanyPurpose>(["live", "test", "dry-run", "restore", "backup", "retest", "baseline"]);

/** Normalizes a CVR/VAT identifier for comparison without exposing it. */
export function normalizeCompanyCvr(value: unknown): string | null {
  if (typeof value !== "string") return null;
  let normalized = value.trim().toUpperCase();
  if (normalized.startsWith("DK")) normalized = normalized.slice(2);
  return /^\d{8}$/.test(normalized) ? `DK${normalized}` : null;
}

export type CanonicalLiveCompany = { entry: WorkspaceCompanyEntry; companyRoot: string; cvr: string };
export type CanonicalLiveCompanyReasonCode =
  | "WORKSPACE_COMPANY_NOT_LIVE"
  | "WORKSPACE_COMPANY_ARCHIVED"
  | "WORKSPACE_LIVE_COMPANY_LEDGER_UNAVAILABLE"
  | "WORKSPACE_LIVE_COMPANY_IDENTITY_MISSING"
  | "WORKSPACE_DUPLICATE_LEGAL_IDENTITY";
export type CanonicalLiveCompanyDiagnostic = {
  slug: string;
  reasonCode: CanonicalLiveCompanyReasonCode;
};
export type CanonicalLiveCompanyResolution = {
  companies: CanonicalLiveCompany[];
  excluded: CanonicalLiveCompanyDiagnostic[];
  blockers: CanonicalLiveCompanyDiagnostic[];
};

export class WorkspaceCanonicalityError extends Error {
  readonly diagnostics: CanonicalLiveCompanyDiagnostic[];

  constructor(diagnostics: CanonicalLiveCompanyDiagnostic[]) {
    super(diagnostics.map((item) => `${item.reasonCode}:${item.slug}`).join(","));
    this.name = "WorkspaceCanonicalityError";
    this.diagnostics = diagnostics;
  }
}

/**
 * The single read-only authority for companies which may participate in a
 * workspace-wide live surface. It never creates, migrates, renames or adopts.
 */
export function resolveCanonicalLiveCompanies(workspaceRoot: string): CanonicalLiveCompanyResolution {
  const candidates = listWorkspaceCompanies(workspaceRoot).slice().sort((a, b) => a.slug.localeCompare(b.slug));
  const excluded: CanonicalLiveCompanyDiagnostic[] = [];
  const blockers: CanonicalLiveCompanyDiagnostic[] = [];
  const valid: CanonicalLiveCompany[] = [];
  for (const entry of candidates) {
    if (entry.archived) { excluded.push({ slug: entry.slug, reasonCode: "WORKSPACE_COMPANY_ARCHIVED" }); continue; }
    if (purposeOf(entry) !== "live") { excluded.push({ slug: entry.slug, reasonCode: "WORKSPACE_COMPANY_NOT_LIVE" }); continue; }
    const companyRoot = companyRootForSlug(workspaceRoot, entry.slug);
    try {
      const db = openLedgerReadOnly(companyPaths(companyRoot).db);
      let cvr: string | null;
      try { cvr = normalizeCompanyCvr((db.query("SELECT cvr FROM companies ORDER BY id ASC LIMIT 1").get() as { cvr?: unknown } | null)?.cvr); }
      finally { db.close(); }
      if (!cvr) { blockers.push({ slug: entry.slug, reasonCode: "WORKSPACE_LIVE_COMPANY_IDENTITY_MISSING" }); continue; }
      valid.push({ entry: { ...entry, purpose: purposeOf(entry) }, companyRoot, cvr });
    } catch { blockers.push({ slug: entry.slug, reasonCode: "WORKSPACE_LIVE_COMPANY_LEDGER_UNAVAILABLE" }); }
  }
  const duplicateCvrs = new Set<string>();
  const seen = new Set<string>();
  for (const item of valid) { if (seen.has(item.cvr)) duplicateCvrs.add(item.cvr); seen.add(item.cvr); }
  const companies = valid.filter((item) => {
    if (!duplicateCvrs.has(item.cvr)) return true;
    blockers.push({ slug: item.entry.slug, reasonCode: "WORKSPACE_DUPLICATE_LEGAL_IDENTITY" });
    return false;
  });
  const ordered = (items: CanonicalLiveCompanyDiagnostic[]) => items.sort(
    (a, b) => a.slug.localeCompare(b.slug) || a.reasonCode.localeCompare(b.reasonCode),
  );
  return { companies, excluded: ordered(excluded), blockers: ordered(blockers) };
}

/** Returns the canonical live set or fails the whole discovery read safely. */
export function requireCanonicalLiveCompanies(workspaceRoot: string): CanonicalLiveCompany[] {
  const resolution = resolveCanonicalLiveCompanies(workspaceRoot);
  const fatal = resolution.blockers.filter(
    (item) => item.reasonCode !== "WORKSPACE_LIVE_COMPANY_IDENTITY_MISSING",
  );
  if (fatal.length > 0) {
    throw new WorkspaceCanonicalityError(fatal);
  }
  return resolution.companies;
}

function sortedManifest(manifest: WorkspaceManifest): WorkspaceManifest {
  return {
    version: MANIFEST_VERSION,
    companies: [...manifest.companies].sort((a, b) => a.slug.localeCompare(b.slug)),
  };
}

/**
 * Loads the workspace manifest. Returns an empty manifest if none exists yet,
 * so callers can treat a brand-new root uniformly.
 */
export function loadWorkspaceManifest(workspaceRoot: string): WorkspaceManifest {
  const path = manifestPath(workspaceRoot);
  if (!existsSync(path)) return emptyManifest();
  let parsed: unknown;
  try {
    parsed = JSON.parse(readFileSync(path, "utf8"));
  } catch {
    throw new Error(`workspace manifest at ${path} is not valid JSON`);
  }
  if (!parsed || typeof parsed !== "object" || !Array.isArray((parsed as any).companies)) {
    throw new Error(`workspace manifest at ${path} is malformed`);
  }
  const parsedVersion = (parsed as any).version;
  if (parsedVersion !== MANIFEST_VERSION && parsedVersion !== LEGACY_MANIFEST_VERSION) {
    throw new Error(
      `workspace manifest at ${path} has unsupported version ${parsedVersion}; expected ${MANIFEST_VERSION}`,
    );
  }
  const companies: WorkspaceCompanyEntry[] = (parsed as any).companies.map((raw: any) => {
    if (parsedVersion === LEGACY_MANIFEST_VERSION && raw.purpose !== undefined) {
      throw new Error("workspace manifest purpose requires version 2");
    }
    const purpose = raw.purpose === undefined ? "live" : raw.purpose;
    if (!PURPOSES.has(purpose)) {
      throw new Error(`workspace manifest has unsupported company purpose`);
    }
    return {
      slug: String(raw.slug),
      name: String(raw.name),
      createdAt: String(raw.createdAt),
      archived: Boolean(raw.archived),
      purpose,
    };
  });
  return { version: MANIFEST_VERSION, companies };
}

/**
 * Writes the manifest atomically. Entries are sorted by slug and the JSON is
 * pretty-printed so the file round-trips deterministically and diffs cleanly.
 */
export function saveWorkspaceManifest(
  workspaceRoot: string,
  manifest: WorkspaceManifest,
): void {
  mkdirSync(workspaceRoot, { recursive: true });
  const ordered = sortedManifest(manifest);
  writeFileAtomic(manifestPath(workspaceRoot), `${JSON.stringify(ordered, null, 2)}\n`);
}

/** Creates the workspace root directory and an empty manifest if absent. */
export function initWorkspace(workspaceRoot: string): WorkspaceManifest {
  mkdirSync(workspaceRoot, { recursive: true });
  if (workspaceExists(workspaceRoot)) return loadWorkspaceManifest(workspaceRoot);
  const manifest = emptyManifest();
  saveWorkspaceManifest(workspaceRoot, manifest);
  return manifest;
}

/** Lists the company entries recorded in the manifest. */
export function listWorkspaceCompanies(workspaceRoot: string): WorkspaceCompanyEntry[] {
  return loadWorkspaceManifest(workspaceRoot).companies;
}

/** Returns the manifest entry for `slug`, or null if it is not registered. */
export function findWorkspaceCompany(
  workspaceRoot: string,
  slug: string,
): WorkspaceCompanyEntry | null {
  return listWorkspaceCompanies(workspaceRoot).find((c) => c.slug === slug) ?? null;
}

/** Manifest-only routing check; never opens, discovers or adopts a directory. */
export function findRoutableWorkspaceCompany(
  workspaceRoot: string,
  slug: string,
): WorkspaceCompanyEntry | null {
  const entry = findWorkspaceCompany(workspaceRoot, slug);
  return entry && purposeOf(entry) === "live" ? entry : null;
}

/** The absolute company directory for `slug` (whether or not it exists). */
export function companyRootForSlug(workspaceRoot: string, slug: string): string {
  assertValidSlug(slug);
  return join(workspaceRoot, slug);
}

/**
 * Resolves a workspace slug to its company directory.
 *
 * Returns the absolute path when the slug is registered in the manifest,
 * otherwise null — callers (e.g. `resolveCompanyRoot` in the CLI) decide
 * whether to fall back to treating the value as a raw path.
 */
export function resolveWorkspaceSlug(workspaceRoot: string, slug: string): string | null {
  if (!isValidSlug(slug)) return null;
  if (!findWorkspaceCompany(workspaceRoot, slug)) return null;
  return companyRootForSlug(workspaceRoot, slug);
}

/**
 * Registers a company entry in the manifest. Idempotent if the slug already
 * exists with identical data; throws on a conflicting duplicate.
 */
export function registerWorkspaceCompany(
  workspaceRoot: string,
  entry: WorkspaceCompanyEntry,
): WorkspaceCompanyEntry {
  assertValidSlug(entry.slug);
  if (entry.purpose !== undefined && !PURPOSES.has(entry.purpose)) {
    throw new Error("workspace company purpose is unsupported");
  }
  const manifest = loadWorkspaceManifest(workspaceRoot);
  if (manifest.companies.some((c) => c.slug === entry.slug)) {
    throw new Error(`virksomheden med slug '${entry.slug}' er allerede registreret i workspacet`);
  }
  manifest.companies.push(entry);
  saveWorkspaceManifest(workspaceRoot, manifest);
  return entry;
}

/**
 * True when `companyRoot` is a direct child directory of `workspaceRoot`, i.e.
 * `<workspaceRoot>/<slug>/`. A company nested deeper, or outside the workspace,
 * is not a workspace member and is rejected by the manifest helpers below.
 */
export function isCompanyInsideWorkspace(
  workspaceRoot: string,
  companyRoot: string,
): boolean {
  const ws = resolve(workspaceRoot);
  const company = resolve(companyRoot);
  const parent = company.slice(0, company.lastIndexOf(sep));
  return parent === ws && company !== ws;
}

/**
 * Result of {@link registerCompanyDirIntoWorkspace}: whether the company was
 * newly added to the manifest, already present, or could not be registered
 * because its directory does not sit directly inside the workspace.
 */
export type WorkspaceAutoRegisterResult =
  | { status: "registered"; slug: string }
  | { status: "already-registered"; slug: string }
  | { status: "outside-workspace" };

/**
 * Registers a company directory that lives directly inside `workspaceRoot`
 * (`<workspaceRoot>/<slug>/`) into the workspace manifest.
 *
 * This is the bridge that makes a company created via `rentemester init`
 * visible to the cockpit (#216): `init` calls this after building the volume
 * when a workspace is configured. It is intentionally forgiving — it never
 * throws — so an unrelated `--company` path can never break `init`:
 *  - directory not inside the workspace  → `outside-workspace` (no-op)
 *  - slug already in the manifest        → `already-registered` (no-op)
 *  - otherwise                           → `registered`
 *
 * The workspace manifest is created if it does not exist yet.
 */
export function registerCompanyDirIntoWorkspace(
  workspaceRoot: string,
  companyRoot: string,
  options?: { name?: string; createdAt?: string },
): WorkspaceAutoRegisterResult {
  if (!isCompanyInsideWorkspace(workspaceRoot, companyRoot)) {
    return { status: "outside-workspace" };
  }
  const resolvedCompany = resolve(companyRoot);
  const slug = resolvedCompany.slice(resolvedCompany.lastIndexOf(sep) + 1);
  if (!isValidSlug(slug)) {
    // The directory name is not a usable slug — treat it like a non-member so
    // `init` is never blocked by a path the workspace model cannot index.
    return { status: "outside-workspace" };
  }
  if (findWorkspaceCompany(workspaceRoot, slug)) {
    return { status: "already-registered", slug };
  }
  if (!workspaceExists(workspaceRoot)) initWorkspace(workspaceRoot);
  const p = companyPaths(resolvedCompany);
  const name = options?.name ?? readCompanyName(p.db) ?? slug;
  registerWorkspaceCompany(workspaceRoot, {
    slug,
    name,
    createdAt: options?.createdAt ?? new Date().toISOString(),
    archived: false,
    purpose: "live",
  });
  return { status: "registered", slug };
}

/**
 * Adopts a present-but-unlisted company directory into the manifest. The
 * directory must already contain a ledger DB; the company name is read from
 * its `companies` table when available, otherwise defaults to the slug.
 */
export function adoptCompanyDir(
  workspaceRoot: string,
  slug: string,
  options?: { name?: string; createdAt?: string },
): WorkspaceCompanyEntry {
  assertValidSlug(slug);
  if (findWorkspaceCompany(workspaceRoot, slug)) {
    throw new Error(`virksomheden med slug '${slug}' er allerede registreret i workspacet`);
  }
  const companyRoot = companyRootForSlug(workspaceRoot, slug);
  const p = companyPaths(companyRoot);
  if (!existsSync(p.db)) {
    throw new Error(
      `kan ikke adoptere '${slug}': ingen ledger fundet — mappen er ikke en virksomhed`,
    );
  }
  const name = options?.name ?? readCompanyName(p.db) ?? slug;
  return registerWorkspaceCompany(workspaceRoot, {
    slug,
    name,
    createdAt: options?.createdAt ?? new Date().toISOString(),
    archived: false,
    purpose: "live",
  });
}

/**
 * Updates the display name of a registered company in the manifest. The slug
 * (and therefore the on-disk directory + ledger) is never touched — only the
 * human-readable label changes. Throws when the slug is not registered.
 */
export function renameWorkspaceCompany(
  workspaceRoot: string,
  slug: string,
  name: string,
): WorkspaceCompanyEntry {
  const trimmed = name.trim();
  if (trimmed.length === 0) {
    throw new Error("firmanavn må ikke være tomt");
  }
  const manifest = loadWorkspaceManifest(workspaceRoot);
  const entry = manifest.companies.find((c) => c.slug === slug);
  if (!entry) {
    throw new Error(`ingen virksomhed med slug '${slug}' findes i workspacet`);
  }
  entry.name = trimmed;
  saveWorkspaceManifest(workspaceRoot, manifest);
  return entry;
}

/**
 * Sets the soft-deletion (`archived`) flag of a registered company. Archiving
 * is non-destructive: the company directory and its ledger stay on disk, the
 * entry is only flagged so the cockpit can hide/segregate it. Throws when the
 * slug is not registered.
 */
export function setWorkspaceCompanyArchived(
  workspaceRoot: string,
  slug: string,
  archived: boolean,
): WorkspaceCompanyEntry {
  const manifest = loadWorkspaceManifest(workspaceRoot);
  const entry = manifest.companies.find((c) => c.slug === slug);
  if (!entry) {
    throw new Error(`ingen virksomhed med slug '${slug}' findes i workspacet`);
  }
  entry.archived = archived;
  saveWorkspaceManifest(workspaceRoot, manifest);
  return entry;
}

function readCompanyName(dbPath: string): string | null {
  try {
    const db = openLedgerReadOnly(dbPath);
    try {
      const row = db.query("SELECT name FROM companies ORDER BY id ASC LIMIT 1").get() as
        | { name: string }
        | null;
      const name = row?.name?.trim();
      return name && name.length > 0 ? name : null;
    } finally {
      db.close();
    }
  } catch {
    return null;
  }
}
