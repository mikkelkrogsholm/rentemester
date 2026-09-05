import { Database } from "bun:sqlite";
import { createHash } from "node:crypto";
import { chmodSync, existsSync, mkdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { resolveActor, type ResolveActorInput } from "./actor";
import { getBuildIdentity } from "./build-identity";
import { openSqliteReadOnlySnapshot } from "./sqlite-readonly-snapshot";

/** Private workspace-owned state. Company ledgers are deliberately elsewhere. */
export const WORKSPACE_CONTROL_DIRECTORY = ".rentemester";
export const WORKSPACE_CONTROL_DB_FILE = "workspace-control.sqlite";
import { loadMigrationCatalog } from "./migration-catalog";

const migrations = loadMigrationCatalog(join(import.meta.dir, "workspace-migrations"));
export const WORKSPACE_CONTROL_BASELINE_MIGRATION_ID = migrations[0]!.id;
export const WORKSPACE_CONTROL_BASELINE_MIGRATION_NAME = migrations[0]!.name;
export const WORKSPACE_CONTROL_BASELINE_MIGRATION_CHECKSUM = migrations[0]!.checksum;
export const CURRENT_WORKSPACE_CONTROL_SCHEMA_VERSION = migrations.at(-1)!.id;
export function supportedWorkspaceControlMigrations() { return migrations.map(({ id, name, checksum }) => ({ id, name, checksum })); }

export type WorkspaceMigrationRow = { id: number; name: string; checksum: string | null; };

export type WorkspaceControlPaths = {
  root: string;
  directory: string;
  db: string;
};

export function workspaceControlPaths(workspaceRoot: string): WorkspaceControlPaths {
  const directory = join(workspaceRoot, WORKSPACE_CONTROL_DIRECTORY);
  return { root: workspaceRoot, directory, db: join(directory, WORKSPACE_CONTROL_DB_FILE) };
}

function tableExists(db: Database, name: string): boolean {
  return db.query(
    "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?",
  ).get(name) != null;
}

function triggerExists(db: Database, name: string): boolean {
  return db.query(
    "SELECT 1 FROM sqlite_master WHERE type = 'trigger' AND name = ?",
  ).get(name) != null;
}

function objectSql(db: Database, type: "table" | "trigger" | "index", name: string): string | null {
  const row = db.query("SELECT sql FROM sqlite_master WHERE type = ? AND name = ?").get(type, name) as { sql: string | null } | null;
  return row?.sql ?? null;
}

function assertInvitationSchema(db: Database): void {
  const columns = db.query("PRAGMA table_info(rm_workspace_invitation_events)").all() as Array<{
    name: string;
    notnull: number;
    pk: number;
  }>;
  const expected = [
    "id", "invitation_id", "event_type", "token_hash", "token_key_version",
    "canonical_email", "email_hash", "workspace_role", "company_slug",
    "company_role", "expires_at", "user_id", "actor", "created_at",
  ];
  if (columns.length !== expected.length || expected.some((name) => !columns.some((column) => column.name === name))) {
    throw new Error("workspace invitation table has an unsupported schema");
  }
  for (const name of ["invitation_id", "event_type", "actor", "created_at"]) {
    if (columns.find((column) => column.name === name)?.notnull !== 1) {
      throw new Error("workspace invitation table requires immutable event fields");
    }
  }
  if (columns.find((column) => column.name === "id")?.pk !== 1) {
    throw new Error("workspace invitation table requires a primary event id");
  }
  const tableSql = objectSql(db, "table", "rm_workspace_invitation_events")
    ?.replace(/\s+/g, " ").toLowerCase() ?? "";
  for (const required of [
    "event_type in ('issued','delivery_confirmed','delivery_failed','accepted','cancelled')",
    "event_type='accepted'",
    "length(token_hash)=64",
    "canonical_email = lower(trim(canonical_email))",
  ]) {
    if (!tableSql.includes(required)) throw new Error("workspace invitation table has unsupported constraints");
  }
  const tokenIndex = objectSql(db, "index", "rm_workspace_invitation_events_token_uidx")
    ?.replace(/\s+/g, " ").toLowerCase() ?? "";
  const acceptedIndex = objectSql(db, "index", "rm_workspace_invitation_events_accepted_uidx")
    ?.replace(/\s+/g, " ").toLowerCase() ?? "";
  if (!tokenIndex.includes("unique index") || !tokenIndex.includes("where token_hash is not null") ||
    !acceptedIndex.includes("unique index") || !acceptedIndex.includes("where event_type='accepted'")) {
    throw new Error("workspace invitation table lacks replay protection");
  }
}

function assertGroupManifestSchema(db: Database): void {
  const columns = db.query("PRAGMA table_info(rm_group_manifest_events)").all() as Array<{ name: string; type: string; notnull: number; pk: number }>;
  const expected = ["id", "manifest_hash", "previous_hash", "canonical_manifest", "actor", "created_at"];
  if (columns.length !== expected.length || expected.some((name) => !columns.some((column) => column.name === name))) {
    throw new Error("workspace group structure table has an unsupported schema");
  }
  for (const name of ["manifest_hash", "canonical_manifest", "actor", "created_at"]) {
    if (columns.find((column) => column.name === name)?.notnull !== 1) throw new Error("workspace group structure table requires immutable event fields");
  }
  if (columns.find((column) => column.name === "id")?.pk !== 1) throw new Error("workspace group structure table requires a primary event id");
  const tableSql = objectSql(db, "table", "rm_group_manifest_events")?.replace(/\s+/g, " ").toLowerCase() ?? "";
  for (const requiredCheck of [
    "check(id > 0)",
    "check(length(manifest_hash)=64 and manifest_hash not glob '*[^0-9a-f]*')",
    "check(previous_hash is null or (length(previous_hash)=64 and previous_hash not glob '*[^0-9a-f]*'))",
    "check(length(canonical_manifest) between 2 and 262144)",
    "check(length(actor) between 3 and 160)",
    "check(length(created_at) between 19 and 30)",
  ]) {
    if (!tableSql.includes(requiredCheck)) throw new Error("workspace group structure table has unsupported constraints");
  }
  for (const [trigger, operation] of [["rm_group_manifest_events_no_update", "UPDATE"], ["rm_group_manifest_events_no_delete", "DELETE"]] as const) {
    const sql = objectSql(db, "trigger", trigger);
    if (!sql || !new RegExp(`BEFORE\\s+${operation}\\s+ON\\s+rm_group_manifest_events`, "i").test(sql) || !/RAISE\s*\(\s*ABORT\s*,\s*'group manifest events are append-only'\s*\)/i.test(sql)) {
      throw new Error("workspace group structure table is not append-only");
    }
  }
}

function assertIntercompanyMappingSchema(db: Database): void {
  const columns = db.query("PRAGMA table_info(rm_intercompany_mapping_events)").all() as Array<{ name: string; notnull: number; pk: number }>;
  const expected = ["id", "mapping_id", "event_type", "mapping_hash", "canonical_mapping", "previous_hash", "event_hash", "actor", "created_at"];
  if (columns.length !== expected.length || expected.some((name) => !columns.some((column) => column.name === name))) throw new Error("workspace intercompany mapping table has an unsupported schema");
  for (const name of ["mapping_id", "event_type", "mapping_hash", "canonical_mapping", "event_hash", "actor", "created_at"]) {
    if (columns.find((column) => column.name === name)?.notnull !== 1) throw new Error("workspace intercompany mapping table requires immutable event fields");
  }
  if (columns.find((column) => column.name === "id")?.pk !== 1) throw new Error("workspace intercompany mapping table requires a primary event id");
  const sql = objectSql(db, "table", "rm_intercompany_mapping_events")?.replace(/\s+/g, " ").toLowerCase() ?? "";
  for (const required of ["check(id > 0)", "check(event_type in ('proposed','approved','revoked'))", "check(length(mapping_hash)=64", "check(length(event_hash)=64", "check(length(canonical_mapping) between 2 and 65536)"]) {
    if (!sql.includes(required)) throw new Error("workspace intercompany mapping table has unsupported constraints");
  }
}

function assertConsolidationEliminationSchema(db: Database): void {
  const columns = db.query("PRAGMA table_info(rm_consolidation_elimination_events)").all() as Array<{ name: string; notnull: number; pk: number }>;
  const expected = ["id", "elimination_id", "event_type", "payload_hash", "canonical_payload", "previous_hash", "event_hash", "actor", "created_at"];
  if (columns.length !== expected.length || expected.some((name) => !columns.some((column) => column.name === name))) throw new Error("workspace consolidation elimination table has an unsupported schema");
  for (const name of ["elimination_id", "event_type", "payload_hash", "canonical_payload", "event_hash", "actor", "created_at"]) {
    if (columns.find((column) => column.name === name)?.notnull !== 1) throw new Error("workspace consolidation elimination table requires immutable event fields");
  }
  const sql = objectSql(db, "table", "rm_consolidation_elimination_events")?.replace(/\s+/g, " ").toLowerCase() ?? "";
  for (const required of ["check(id > 0)", "check(event_type in ('proposed','approved','rejected','applied','reversed'))", "check(length(payload_hash)=64", "check(length(event_hash)=64", "check(length(canonical_payload) between 2 and 262144)"]) {
    if (!sql.includes(required)) throw new Error("workspace consolidation elimination table has unsupported constraints");
  }
}

function assertConsolidationProfileSchema(db: Database): void {
  const columns = db.query("PRAGMA table_info(rm_consolidation_profile_events)").all() as Array<{ name: string; notnull: number; pk: number }>;
  const expected = ["id", "profile_id", "event_type", "profile_hash", "canonical_profile", "previous_hash", "event_hash", "actor", "created_at"];
  if (columns.length !== expected.length || expected.some((name) => !columns.some((column) => column.name === name))) throw new Error("workspace consolidation profile table has an unsupported schema");
  const sql = objectSql(db, "table", "rm_consolidation_profile_events")?.replace(/\s+/g, " ").toLowerCase() ?? "";
  for (const required of ["check(id > 0)", "check(event_type in ('proposed','approved','revoked'))", "check(length(profile_hash)=64", "check(length(event_hash)=64", "check(length(canonical_profile) between 2 and 524288)"]) {
    if (!sql.includes(required)) throw new Error("workspace consolidation profile table has unsupported constraints");
  }
}

function assertAuthTelemetrySchema(db: Database): void {
  const sql = objectSql(db, "table", "rm_workspace_auth_telemetry_events")
    ?.replace(/\s+/g, " ").toLowerCase() ?? "";
  if (!sql.includes("'two-factor-verify-backup-code'")) {
    throw new Error("workspace auth telemetry table lacks recovery-code evidence");
  }
  for (const [name, operation] of [
    ["rm_workspace_auth_telemetry_events_no_update", "UPDATE"],
    ["rm_workspace_auth_telemetry_events_no_delete", "DELETE"],
  ] as const) {
    const trigger = objectSql(db, "trigger", name);
    if (!trigger || !new RegExp(`BEFORE\\s+${operation}\\s+ON\\s+rm_workspace_auth_telemetry_events`, "i").test(trigger)) {
      throw new Error("workspace auth telemetry table is not append-only");
    }
  }
}

function assertStrictMigrationTable(db: Database): void {
  const columns = db.query("PRAGMA table_info(workspace_schema_migrations)").all() as Array<{
    name: string;
    notnull: number;
  }>;
  const required = [
    "id",
    "name",
    "checksum",
    "applied_at",
    "applied_by_version",
    "applied_by_commit",
  ];
  if (
    columns.length !== required.length ||
    required.some((name) => !columns.some((column) => column.name === name))
  ) {
    throw new Error("workspace control migration ledger has an unsupported schema");
  }
  for (const name of ["name", "checksum", "applied_at", "applied_by_version"]) {
    if (columns.find((column) => column.name === name)?.notnull !== 1) {
      throw new Error("workspace control migration ledger must require checksummed provenance");
    }
  }
}

/** Validate the complete, checksummed append-only prefix known by this runtime. */
export function validateWorkspaceControlMigrationHistory(
  rows: readonly WorkspaceMigrationRow[],
): void {
  if (migrations.some((migration, index) => migration.id !== index + 1)) {
    throw new Error("workspace control migration catalog must be contiguous from version 1");
  }
  const newestApplied = rows.at(-1)?.id ?? 0;
  const newestSupported = migrations.at(-1)?.id ?? 0;
  if (newestApplied > newestSupported) {
    throw new Error(
      `workspace control schema version ${newestApplied} is newer than supported version ${newestSupported}; upgrade Rentemester before opening this workspace`,
    );
  }
  for (let index = 0; index < rows.length; index += 1) {
    const row = rows[index]!;
    const expected = migrations[index];
    if (!expected || row.id !== expected.id) {
      throw new Error("workspace control migration history is not a complete append-only prefix");
    }
    if (row.name !== expected.name) {
      throw new Error(`workspace control migration ${row.id} has unexpected name '${row.name}'`);
    }
    if (!row.checksum) {
      throw new Error("workspace control migration history contains a missing checksum");
    }
    if (row.checksum !== expected.checksum) {
      throw new Error(
        `workspace control migration ${row.id} checksum mismatch; the workspace control history may have been modified`,
      );
    }
  }
}

/**
 * Read-only compatibility check for an already-existing control database.
 * Opening/migrating it remains the responsibility of `openWorkspaceControlDb`.
 */
export function assertWorkspaceControlCompatibility(db: Database): void {
  const hasHistory = tableExists(db, "workspace_schema_migrations");
  if (!hasHistory) {
    const existingUserTable = db.query(
      "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' LIMIT 1",
    ).get() as { name: string } | null;
    if (existingUserTable) {
      throw new Error(
        `workspace control database has table '${existingUserTable.name}' but no migration history; refusing to adopt unknown state`,
      );
    }
    return;
  }
  assertStrictMigrationTable(db);
  const rows = db.query(
    "SELECT id, name, checksum FROM workspace_schema_migrations ORDER BY id",
  ).all() as WorkspaceMigrationRow[];
  validateWorkspaceControlMigrationHistory(rows);
  if (rows.length === 0 && tableExists(db, "workspace_audit")) {
    throw new Error("workspace control audit table exists without its baseline migration record");
  }
}

/**
 * Checks the immutable audit and Better Auth primitives without changing DB
 * state.  Readiness uses this on a `query_only` connection.
 */
export function assertWorkspaceControlPrimitives(db: Database): void {
  if (
    !tableExists(db, "workspace_audit") ||
    !triggerExists(db, "workspace_schema_migrations_no_update") ||
    !triggerExists(db, "workspace_schema_migrations_no_delete") ||
    !triggerExists(db, "workspace_audit_no_update") ||
    !triggerExists(db, "workspace_audit_no_delete")
  ) {
    throw new Error(
      "workspace control audit primitives are incomplete; refusing to open workspace control database",
    );
  }

  // Better Auth owns the rows in these tables, but Rentemester owns the
  // reviewed, checksummed DDL which created them. Do not let Better Auth's
  // runtime migrator alter this workspace database.
  for (const table of ["user", "session", "account", "verification", "twoFactor", "rateLimit"]) {
    if (!tableExists(db, table)) {
      throw new Error(`workspace control Better Auth table '${table}' is missing`);
    }
  }

  for (const table of [
    "rm_workspace_user_access_events",
    "rm_company_membership_events",
    "rm_workspace_security_events",
    "rm_workspace_bootstrap_events",
    "rm_workspace_mfa_events",
    "rm_workspace_auth_telemetry_events",
    "rm_workspace_auth_state_events",
    "rm_workspace_document_access_events",
    "rm_workspace_authorization_events",
    "rm_workspace_invitation_events",
    "rm_intercompany_mapping_events",
    "rm_consolidation_elimination_events",
    "rm_consolidation_profile_events",
    "rm_party_events",
    "rm_party_alias_assertions",
    "rm_party_field_assertions",
    "rm_party_legacy_links",
    "rm_legacy_party_mapping_events",
    "rm_corporate_record_events",
    "rm_corporate_record_scope_assertions",
    "rm_company_knowledge_assertions",
    "rm_company_knowledge_events",
    "rm_knowledge_page_events",
    "rm_knowledge_relation_events",
    "rm_accounting_approval_policy_events",
    "rm_ownership_source_snapshots",
    "rm_ownership_snapshot_events",
    "rm_ownership_facts",
    "rm_intercompany_dispositions",
    "rm_intercompany_disposition_events",
    "rm_intercompany_disposition_journal_links",
    "rm_intercompany_disposition_lifecycle_events",
  ]) {
    if (!tableExists(db, table)) {
      throw new Error(`workspace control access table '${table}' is missing`);
    }
    for (const operation of ["update", "delete"]) {
      if (!triggerExists(db, `${table}_no_${operation}`)) {
        throw new Error(`workspace control access table '${table}' is not append-only`);
      }
    }
  }
  assertAuthTelemetrySchema(db);
  assertInvitationSchema(db);
  if (!tableExists(db, "rm_group_manifest_events") ||
    !triggerExists(db, "rm_group_manifest_events_no_update") ||
    !triggerExists(db, "rm_group_manifest_events_no_delete")) {
    throw new Error("workspace group structure table is not append-only");
  }
  assertGroupManifestSchema(db);
  assertIntercompanyMappingSchema(db);
  assertConsolidationEliminationSchema(db);
  assertConsolidationProfileSchema(db);
  if (!triggerExists(db, "rm_better_auth_mfa_enrollment_revoke_sessions")) {
    throw new Error("workspace control MFA session-assurance trigger is missing");
  }
}

/**
 * Opens the workspace-local control database and applies its own immutable
 * migration catalog. This never opens, creates, or modifies a company ledger.
 */
export function openWorkspaceControlDb(workspaceRoot: string): Database {
  const paths = workspaceControlPaths(workspaceRoot);
  mkdirSync(paths.directory, { recursive: true, mode: 0o700 });
  const db = new Database(paths.db);
  try {
    db.exec("PRAGMA foreign_keys = ON");
    assertWorkspaceControlCompatibility(db);
    const build = getBuildIdentity();
    db.transaction(() => {
      const applied = tableExists(db, "workspace_schema_migrations")
        ? db.query(
          "SELECT id, name, checksum FROM workspace_schema_migrations ORDER BY id",
        ).all() as WorkspaceMigrationRow[]
        : [];
      validateWorkspaceControlMigrationHistory(applied);
      for (const migration of migrations) {
        if (applied.some((row) => row.id === migration.id)) continue;
        const parsed = JSON.parse(migration.artifact.toString("utf8")) as {
          id: number;
          name: string;
          sql: string;
        };
        if (
          parsed.id !== migration.id ||
          parsed.name !== migration.name ||
          typeof parsed.sql !== "string"
        ) {
          throw new Error(`workspace control migration artifact ${migration.id} is malformed`);
        }
        db.exec(parsed.sql);
        db.query(
          `INSERT INTO workspace_schema_migrations
             (id, name, checksum, applied_by_version, applied_by_commit)
           VALUES (?, ?, ?, ?, ?)`,
        ).run(migration.id, migration.name, migration.checksum, build.version, build.gitCommit);
      }
    }).immediate();
    assertWorkspaceControlCompatibility(db);
    assertWorkspaceControlPrimitives(db);
    // Keep private workspace identity/control state inaccessible to other local users.
    chmodSync(paths.db, 0o600);
    return db;
  } catch (error) {
    db.close();
    throw error;
  }
}

/**
 * Strict read-only control opening for status/report surfaces. Unlike the
 * writer this never creates directories, runs a migration, chmods a file, or
 * repairs state. A status route must fail closed on a missing/tampered DB.
 */
export function openWorkspaceControlReadOnlyDb(workspaceRoot: string): Database {
  const paths = workspaceControlPaths(workspaceRoot);
  if (!existsSync(paths.db)) throw new Error("workspace control database is unavailable");
  const db = openSqliteReadOnlySnapshot(paths.db);
  try {
    assertWorkspaceControlCompatibility(db);
    assertWorkspaceControlPrimitives(db);
    return db;
  } catch (error) {
    db.close();
    throw error;
  }
}

export type WorkspaceAuditInput = {
  /** A stable action identifier, not a human-entered message or payload. */
  eventType: string;
  /** A stable resource class, such as a future `workspace_member` or `group`. */
  entityType: string;
  /** Opaque resource identifier only; never include credentials or document contents. */
  entityId?: string | number | null;
} & ResolveActorInput;

/**
 * Append a secret-free workspace control audit record. There intentionally is
 * no arbitrary message, JSON payload, before/after value, token or credential
 * parameter on this API; future features must store sensitive evidence outside
 * this general-purpose audit stream.
 */
export function insertWorkspaceAudit(db: Database, input: WorkspaceAuditInput): void {
  if (!input.eventType.trim() || !input.entityType.trim()) {
    throw new Error("workspace audit eventType and entityType are required");
  }
  const actor = resolveActor(input);
  db.query(
    "INSERT INTO workspace_audit (event_type, entity_type, entity_id, actor) VALUES (?, ?, ?, ?)",
  ).run(
    input.eventType.trim(),
    input.entityType.trim(),
    input.entityId == null ? null : String(input.entityId),
    actor.auditActor,
  );
}

export type WorkspaceDocumentAccessAuditInput = {
  actor: string;
  companySlug: string;
  resourceType: "document_file" | "issued_invoice_pdf";
  /** Null is valid only for an authorization denial where the object is not safely resolved. */
  resourceId: number | null;
  outcome: "served" | "denied";
  reasonCode: "authorized" | "authorization_denied";
  requestId?: string | null;
};

const SAFE_ACCESS_ACTOR = /^user:[A-Za-z0-9._-]{1,155}$/;
const SAFE_ACCESS_SLUG = /^[a-z0-9][a-z0-9-]{0,119}$/;
const SAFE_ACCESS_REQUEST_ID = /^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$/;

/**
 * Append an allowlisted hosted document-access event.  This deliberately
 * accepts no filename, path, MIME, request URL/body, byte count, exception or
 * arbitrary metadata, so the evidence store itself cannot become a leak.
 */
export function insertWorkspaceDocumentAccessAudit(
  db: Database,
  input: WorkspaceDocumentAccessAuditInput,
): void {
  if (!SAFE_ACCESS_ACTOR.test(input.actor) || !SAFE_ACCESS_SLUG.test(input.companySlug)) {
    throw new Error("invalid document access audit identity");
  }
  if (!Number.isInteger(input.resourceId) && input.resourceId !== null) {
    throw new Error("invalid document access audit resource");
  }
  if (input.resourceId !== null && input.resourceId <= 0) {
    throw new Error("invalid document access audit resource");
  }
  if ((input.outcome === "served") !== (input.reasonCode === "authorized")) {
    throw new Error("invalid document access audit outcome");
  }
  const requestId = input.requestId?.trim() || null;
  if (requestId !== null && !SAFE_ACCESS_REQUEST_ID.test(requestId)) {
    throw new Error("invalid document access audit request id");
  }
  db.query(
    `INSERT INTO rm_workspace_document_access_events
       (actor, company_slug, resource_type, resource_id, outcome, reason_code, request_id)
     VALUES (?, ?, ?, ?, ?, ?, ?)`,
  ).run(
    input.actor,
    input.companySlug,
    input.resourceType,
    input.resourceId,
    input.outcome,
    input.reasonCode,
    requestId,
  );
}

export type WorkspaceAuthorizationAuditInput = {
  actor: string;
  method: string;
  routeTemplate: string;
  permission: string;
  companySlug?: string | null;
  requestId?: string | null;
};

const SAFE_AUTHORIZATION_ROUTE = /^\/api\/[A-Za-z0-9_/:.-]{1,235}$/;
const SAFE_AUTHORIZATION_PERMISSION = /^[a-z0-9][a-z0-9._-]{2,79}$/;

/** Records only a route template and authorization decision—never raw URL, query, body or object id. */
export function insertWorkspaceAuthorizationAudit(
  db: Database,
  input: WorkspaceAuthorizationAuditInput,
): void {
  const method = input.method.trim().toUpperCase();
  const companySlug = input.companySlug?.trim() || null;
  const requestId = input.requestId?.trim() || null;
  if (!SAFE_ACCESS_ACTOR.test(input.actor) || !["GET", "POST", "PATCH", "PUT", "DELETE"].includes(method)) {
    throw new Error("invalid authorization audit identity");
  }
  if (!SAFE_AUTHORIZATION_ROUTE.test(input.routeTemplate) || !SAFE_AUTHORIZATION_PERMISSION.test(input.permission)) {
    throw new Error("invalid authorization audit route");
  }
  if (companySlug !== null && !SAFE_ACCESS_SLUG.test(companySlug)) {
    throw new Error("invalid authorization audit company");
  }
  if (requestId !== null && !SAFE_ACCESS_REQUEST_ID.test(requestId)) {
    throw new Error("invalid authorization audit request id");
  }
  db.query(`INSERT INTO rm_workspace_authorization_events
    (actor, method, route_template, permission, company_slug, outcome, reason_code, request_id)
    VALUES (?, ?, ?, ?, ?, 'denied', 'authorization_denied', ?)`).run(
    input.actor,
    method,
    input.routeTemplate,
    input.permission,
    companySlug,
    requestId,
  );
}

export function readWorkspaceControlMigrations(db: Database): WorkspaceMigrationRow[] {
  return db.query(
    "SELECT id, name, checksum FROM workspace_schema_migrations ORDER BY id",
  ).all() as WorkspaceMigrationRow[];
}
