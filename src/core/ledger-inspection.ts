import { Database } from "bun:sqlite";
import { readFileSync, readdirSync } from "node:fs";
import { createHash } from "node:crypto";
import { join } from "node:path";
import {
  CURRENT_SCHEMA_VERSION,
  supportedSchemaMigrations,
  type SchemaMigrationIdentity,
} from "./schema-version";
import { openSqliteDisposableSnapshot, openSqliteReadOnlySnapshot } from "./sqlite-readonly-snapshot";

export type LedgerInspection =
  | { status: "current"; currentVersion: number; requiredVersion: number; pending: [] }
  | { status: "pending"; currentVersion: number; requiredVersion: number; pending: SchemaMigrationIdentity[] }
  | { status: "newer"; currentVersion: number; requiredVersion: number; pending: [] ; error: string }
  | { status: "corrupt"; currentVersion: number; requiredVersion: number; pending: []; error: string }
  | { status: "unavailable"; currentVersion: number; requiredVersion: number; pending: []; error: string };

const requiredVersion = CURRENT_SCHEMA_VERSION;
const unavailable = (error: string): LedgerInspection => ({ status: "unavailable", currentVersion: 0, requiredVersion, pending: [], error });
const corrupt = (currentVersion: number, error: string): LedgerInspection => ({ status: "corrupt", currentVersion, requiredVersion, pending: [], error });

export type SchemaViewDefinition = { name: string; sql: string; normalizedSql: string };
export type SchemaViewInspection = {
  ok: boolean;
  errors: string[];
  affectedNames: string[];
  catalogueDigest: string;
  actualDigest: string;
};

function normalizeViewSql(sql: string): string {
  // SQL keywords and insignificant whitespace are case-insensitive; quoted
  // literals are not. Tokenise rather than lowercasing the whole definition.
  const uncommented = sql.replace(/\/\*[\s\S]*?\*\//g, " ").replace(/--[^\n\r]*/g, " ").replace(/CREATE\s+VIEW\s+IF\s+NOT\s+EXISTS/i, "CREATE VIEW");
  const tokens = uncommented.match(/'(?:''|[^'])*'|"(?:""|[^"])*"|`(?:``|[^`])*`|\[[^\]]*\]|\s+|[^\s'"`\[]+/g) ?? [];
  return tokens.map((token) => /^\s+$/.test(token) ? " " : (/^['"`\[]/.test(token) ? token : token.toLowerCase())).join("").replace(/\s+/g, " ").trim().replace(/;$/, "").trim();
}

function extractViews(sql: string): SchemaViewDefinition[] {
  return (sql.match(/CREATE\s+VIEW(?:\s+IF\s+NOT\s+EXISTS)?\s+[A-Za-z_][A-Za-z0-9_]*[\s\S]*?;/gi) ?? [])
    .map((statement) => {
      const name = /CREATE\s+VIEW(?:\s+IF\s+NOT\s+EXISTS)?\s+([A-Za-z_][A-Za-z0-9_]*)/i.exec(statement)?.[1];
      return name ? { name, sql: statement.replace(/CREATE\s+VIEW\s+IF\s+NOT\s+EXISTS/i, "CREATE VIEW"), normalizedSql: normalizeViewSql(statement) } : null;
    })
    .filter((view): view is SchemaViewDefinition => view !== null);
}

let cachedSchemaViews: SchemaViewDefinition[] | undefined;
/** Every canonical view, including views introduced in numbered migrations. */
export function canonicalSchemaViews(): SchemaViewDefinition[] {
  if (cachedSchemaViews) return cachedSchemaViews;
  const root = import.meta.dir;
  const sources = [readFileSync(join(root, "schema.sql"), "utf8")];
  for (const file of readdirSync(join(root, "migrations")).filter((name) => name.endsWith(".json")).sort()) {
    const migration = JSON.parse(readFileSync(join(root, "migrations", file), "utf8")) as { sql?: string };
    if (migration.sql) sources.push(migration.sql);
  }
  const byName = new Map<string, SchemaViewDefinition>();
  for (const view of sources.flatMap(extractViews)) byName.set(view.name, view);
  cachedSchemaViews = [...byName.values()].sort((a, b) => a.name.localeCompare(b.name));
  return cachedSchemaViews;
}

function digest(values: unknown): string {
  return createHash("sha256").update(JSON.stringify(values)).digest("hex");
}

export function inspectSchemaViews(db: Database): SchemaViewInspection {
  const expected = canonicalSchemaViews();
  const actualByName = new Map((db.query("SELECT name, sql FROM sqlite_master WHERE type = 'view'").all() as Array<{ name: string; sql: string | null }>).map((row) => [row.name, row.sql]));
  const errors: string[] = [];
  const actual = expected.map((view) => {
    const sql = actualByName.get(view.name) ?? null;
    const normalizedSql = sql === null ? null : normalizeViewSql(sql);
    if (sql === null) errors.push(`SCHEMA_VIEW_DRIFT:${view.name}:missing`);
    else if (normalizedSql !== view.normalizedSql) errors.push(`SCHEMA_VIEW_DRIFT:${view.name}:definition_mismatch`);
    return [view.name, normalizedSql];
  });
  return { ok: errors.length === 0, errors, affectedNames: errors.map((error) => error.split(":")[1]!), catalogueDigest: digest(expected.map((view) => [view.name, view.normalizedSql])), actualDigest: digest(actual) };
}

/** Applies only the checked-in catalogue; callers never provide SQL or names. */
export function repairCanonicalSchemaViews(db: Database): SchemaViewInspection {
  const before = inspectSchemaViews(db);
  if (before.ok) return before;
  const wanted = new Map(canonicalSchemaViews().map((view) => [view.name, view]));
  for (const name of before.affectedNames) {
    const view = wanted.get(name)!;
    db.exec(`DROP VIEW IF EXISTS "${name}"`);
    db.exec(view.sql);
  }
  return inspectSchemaViews(db);
}

/**
 * Opens an existing ledger without ever creating a directory, journal sidecar,
 * or migration row. Callers must close the returned handle.
 */
export function openLedgerReadOnly(path: string): Database {
  return openSqliteReadOnlySnapshot(path);
}

/**
 * Opens a byte-preserving snapshot and rejects anything except the exact,
 * checksummed schema supported by this runtime. Read surfaces use this helper
 * instead of running `migrate()` speculatively.
 */
export function openCurrentLedgerReadOnly(path: string): Database {
  const db = openLedgerReadOnly(path);
  const inspection = inspectOpenLedger(db);
  if (inspection.status === "current") return db;
  db.close();
  const detail = inspection.status === "pending"
    ? `current=${inspection.currentVersion} required=${inspection.requiredVersion}`
    : inspection.error;
  throw new Error(`ledger schema is ${inspection.status}: ${detail}`);
}

/**
 * Opens a current ledger as a disposable writable copy for previews whose
 * implementation intentionally writes and rolls back. Source bytes and
 * sidecars remain untouched, and no schema setup is performed.
 */
export function openCurrentLedgerSimulation(path: string): Database {
  const db = openSqliteDisposableSnapshot(path);
  const inspection = inspectOpenLedger(db);
  if (inspection.status === "current") return db;
  db.close();
  const detail = inspection.status === "pending"
    ? `current=${inspection.currentVersion} required=${inspection.requiredVersion}`
    : inspection.error;
  throw new Error(`ledger schema is ${inspection.status}: ${detail}`);
}

function tableExists(db: Database, name: string): boolean {
  return db.query("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?").get(name) != null;
}

export function inspectOpenLedger(db: Database): LedgerInspection {
  let currentVersion = 0;
  try {
    for (const name of ["schema_migrations", "companies", "audit_log"]) {
      if (!tableExists(db, name)) return corrupt(0, `missing critical schema primitive: ${name}`);
    }
    const columns = new Set((db.query("PRAGMA table_info(schema_migrations)").all() as Array<{ name: string }>).map((r) => r.name));
    for (const column of ["id", "name", "checksum", "applied_at", "applied_by_version"]) {
      if (!columns.has(column)) return corrupt(0, `schema_migrations is missing required column: ${column}`);
    }
    const rows = db.query("SELECT id, name, checksum FROM schema_migrations ORDER BY id").all() as SchemaMigrationIdentity[];
    currentVersion = rows.at(-1)?.id ?? 0;
    const supported = supportedSchemaMigrations();
    if (currentVersion > requiredVersion) {
      return { status: "newer", currentVersion, requiredVersion, pending: [], error: "database schema is newer than this Rentemester runtime" };
    }
    for (let i = 0; i < rows.length; i += 1) {
      const actual = rows[i]!;
      const expected = supported[i];
      if (!expected || actual.id !== expected.id || actual.name !== expected.name || actual.checksum !== expected.checksum) {
        return corrupt(currentVersion, `schema migration ${actual.id} is not the required checksummed append-only prefix`);
      }
    }
    const quick = db.query("PRAGMA quick_check").all() as Array<{ quick_check: string }>;
    if (quick.some((row) => Object.values(row)[0] !== "ok")) return corrupt(currentVersion, "sqlite quick_check failed");
    const foreignKeys = db.query("PRAGMA foreign_key_check").all();
    if (foreignKeys.length > 0) return corrupt(currentVersion, "sqlite foreign_key_check failed");
    if (currentVersion < requiredVersion) {
      return { status: "pending", currentVersion, requiredVersion, pending: supported.slice(currentVersion).map(({ id, name, checksum }) => ({ id, name, checksum })) };
    }
    const views = inspectSchemaViews(db);
    if (!views.ok) return corrupt(currentVersion, views.errors.join("; "));
    return { status: "current", currentVersion, requiredVersion, pending: [] };
  } catch (error) {
    return corrupt(currentVersion, error instanceof Error ? error.message : String(error));
  }
}

export function inspectLedger(path: string): LedgerInspection {
  let db: Database | undefined;
  try {
    db = openLedgerReadOnly(path);
    return inspectOpenLedger(db);
  } catch (error) {
    return unavailable(error instanceof Error ? error.message : String(error));
  } finally {
    db?.close();
  }
}
