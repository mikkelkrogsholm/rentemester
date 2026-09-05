import type { Database } from "bun:sqlite";
import { join } from "node:path";
import { getBuildIdentity } from "./build-identity";

export const BASELINE_SCHEMA_VERSION = 1;
import { loadMigrationCatalog } from "./migration-catalog";

const MIGRATIONS = loadMigrationCatalog(join(import.meta.dir, "migrations"), {
  16: "rentemester-invoice-extraction-actors-v16",
  35: "rentemester-bank-reconciliation-account-role-fallback-v35",
});
const migration = (id: number) => MIGRATIONS[id - 1]!;
const artifact = (id: number) => migration(id).artifact;
export const BASELINE_MIGRATION_NAME = MIGRATIONS[0]!.name;
export const BASELINE_MIGRATION_CHECKSUM = MIGRATIONS[0]!.checksum;

// Kept for callers that import a migration's historical named identity.
// Every value is read from the immutable catalogue above; this is not a
// second artifact list or migration registration mechanism.
export const PEPPOL_SUBMISSION_EVENTS_MIGRATION_CHECKSUM = migration(2).checksum;
export const PEPPOL_SUBMISSION_EVENTS_MIGRATION_NAME = migration(2).name;
export const RECURRING_AUTOMATION_MIGRATION_CHECKSUM = migration(3).checksum;
export const RECURRING_AUTOMATION_MIGRATION_NAME = migration(3).name;
export const DINERO_IMPORT_PROVENANCE_MIGRATION_CHECKSUM = migration(4).checksum;
export const DINERO_IMPORT_PROVENANCE_MIGRATION_NAME = migration(4).name;
export const MIGRATION_OPEN_ITEMS_MIGRATION_CHECKSUM = migration(5).checksum;
export const MIGRATION_OPEN_ITEMS_MIGRATION_NAME = migration(5).name;
export const BANK_JOURNAL_RECONCILIATION_LINKS_MIGRATION_CHECKSUM = migration(6).checksum;
export const BANK_JOURNAL_RECONCILIATION_LINKS_MIGRATION_NAME = migration(6).name;
export const DOCUMENT_SCAN_EVIDENCE_MIGRATION_CHECKSUM = migration(7).checksum;
export const DOCUMENT_SCAN_EVIDENCE_MIGRATION_NAME = migration(7).name;
export const ISSUED_INVOICE_PDF_IMMUTABILITY_MIGRATION_CHECKSUM = migration(8).checksum;
export const ISSUED_INVOICE_PDF_IMMUTABILITY_MIGRATION_NAME = migration(8).name;
export const ACCOUNTING_DRAFT_WORKFLOW_MIGRATION_CHECKSUM = migration(9).checksum;
export const ACCOUNTING_DRAFT_WORKFLOW_MIGRATION_NAME = migration(9).name;
export const INTERNAL_VOUCHER_EVIDENCE_MIGRATION_CHECKSUM = migration(10).checksum;
export const INTERNAL_VOUCHER_EVIDENCE_MIGRATION_NAME = migration(10).name;
export const PURCHASE_VAT_PREFLIGHT_MIGRATION_CHECKSUM = migration(11).checksum;
export const PURCHASE_VAT_PREFLIGHT_MIGRATION_NAME = migration(11).name;
export const POSTING_RULES_MIGRATION_CHECKSUM = migration(12).checksum;
export const POSTING_RULES_MIGRATION_NAME = migration(12).name;
export const BOOKKEEPING_BATCHES_MIGRATION_CHECKSUM = migration(13).checksum;
export const BOOKKEEPING_BATCHES_MIGRATION_NAME = migration(13).name;
export const INVOICE_EXTRACTION_EVIDENCE_MIGRATION_CHECKSUM = migration(14).checksum;
export const INVOICE_EXTRACTION_EVIDENCE_MIGRATION_NAME = migration(14).name;
export const BOOKKEEPING_BATCH_RETRIES_MIGRATION_CHECKSUM = migration(15).checksum;
export const BOOKKEEPING_BATCH_RETRIES_MIGRATION_NAME = migration(15).name;
export const INVOICE_EXTRACTION_ACTORS_MIGRATION_CHECKSUM = migration(16).checksum;
export const INVOICE_EXTRACTION_ACTORS_MIGRATION_NAME = migration(16).name;
export const DOCUMENT_PDF_PARSES_MIGRATION_CHECKSUM = migration(17).checksum;
export const DOCUMENT_PDF_PARSES_MIGRATION_NAME = migration(17).name;
export const DOCUMENT_METADATA_ENRICHMENTS_MIGRATION_CHECKSUM = migration(18).checksum;
export const DOCUMENT_METADATA_ENRICHMENTS_MIGRATION_NAME = migration(18).name;
export const DOCUMENT_COMPANY_CONTEXTS_MIGRATION_CHECKSUM = migration(19).checksum;
export const DOCUMENT_COMPANY_CONTEXTS_MIGRATION_NAME = migration(19).name;
export const MUTATION_IDEMPOTENCY_RECEIPTS_MIGRATION_CHECKSUM = migration(20).checksum;
export const MUTATION_IDEMPOTENCY_RECEIPTS_MIGRATION_NAME = migration(20).name;
export const BOOKKEEPING_BATCH_REVISIONS_MIGRATION_CHECKSUM = migration(21).checksum;
export const BOOKKEEPING_BATCH_REVISIONS_MIGRATION_NAME = migration(21).name;
export const PERIOD_CLOSE_READINESS_MIGRATION_CHECKSUM = migration(22).checksum;
export const PERIOD_CLOSE_READINESS_MIGRATION_NAME = migration(22).name;
export const LOCAL_IDEMPOTENCY_TOMBSTONES_MIGRATION_CHECKSUM = migration(23).checksum;
export const LOCAL_IDEMPOTENCY_TOMBSTONES_MIGRATION_NAME = migration(23).name;
export const BOOKKEEPING_BATCH_PRINCIPALS_MIGRATION_CHECKSUM = migration(24).checksum;
export const BOOKKEEPING_BATCH_PRINCIPALS_MIGRATION_NAME = migration(24).name;
export const PERIOD_CLOSE_REVIEWS_MIGRATION_CHECKSUM = migration(25).checksum;
export const PERIOD_CLOSE_REVIEWS_MIGRATION_NAME = migration(25).name;
export const DOCUMENT_PARTY_LINKS_MIGRATION_CHECKSUM = migration(26).checksum;
export const DOCUMENT_PARTY_LINKS_MIGRATION_NAME = migration(26).name;
export const SUPPLIER_COMMITMENTS_MIGRATION_CHECKSUM = migration(27).checksum;
export const SUPPLIER_COMMITMENTS_MIGRATION_NAME = migration(27).name;
export const ACCOUNTING_DIMENSIONS_MIGRATION_CHECKSUM = migration(28).checksum;
export const ACCOUNTING_DIMENSIONS_MIGRATION_NAME = migration(28).name;
export const DOCUMENT_PARTY_RESOLUTION_MIGRATION_CHECKSUM = migration(29).checksum;
export const DOCUMENT_PARTY_RESOLUTION_MIGRATION_NAME = migration(29).name;
export const ACCOUNTING_DIMENSION_LIFECYCLE_MIGRATION_CHECKSUM = migration(30).checksum;
export const ACCOUNTING_DIMENSION_LIFECYCLE_MIGRATION_NAME = migration(30).name;
export const SUPPLIER_COMMITMENT_OCCURRENCE_MATCHES_MIGRATION_CHECKSUM = migration(31).checksum;
export const SUPPLIER_COMMITMENT_OCCURRENCE_MATCHES_MIGRATION_NAME = migration(31).name;
export const DIMENSION_BUDGET_AND_PROVENANCE_MIGRATION_CHECKSUM = migration(32).checksum;
export const DIMENSION_BUDGET_AND_PROVENANCE_MIGRATION_NAME = migration(32).name;
export const BANK_RECONCILIATION_CORRECTIONS_MIGRATION_CHECKSUM = migration(33).checksum;
export const BANK_RECONCILIATION_CORRECTIONS_MIGRATION_NAME = migration(33).name;
export const DIRECT_BANK_PURCHASE_PAYABLE_CORRECTIONS_MIGRATION_CHECKSUM = migration(34).checksum;
export const DIRECT_BANK_PURCHASE_PAYABLE_CORRECTIONS_MIGRATION_NAME = migration(34).name;
export const BANK_RECONCILIATION_ACCOUNT_ROLE_FALLBACK_MIGRATION_CHECKSUM = migration(35).checksum;
export const BANK_RECONCILIATION_ACCOUNT_ROLE_FALLBACK_MIGRATION_NAME = migration(35).name;
export const IMPORTED_RECEIVABLES_MIGRATION_CHECKSUM = migration(36).checksum;
export const IMPORTED_RECEIVABLES_MIGRATION_NAME = migration(36).name;
export const IMPORTED_RECEIVABLE_BOUNDARIES_MIGRATION_CHECKSUM = migration(37).checksum;
export const IMPORTED_RECEIVABLE_BOUNDARIES_MIGRATION_NAME = migration(37).name;
export const LEGACY_IMPORTED_RECEIVABLE_BACKFILLS_MIGRATION_CHECKSUM = migration(38).checksum;
export const LEGACY_IMPORTED_RECEIVABLE_BACKFILLS_MIGRATION_NAME = migration(38).name;
export const NON_CASH_BALANCE_CORRECTIONS_MIGRATION_CHECKSUM = migration(39).checksum;
export const NON_CASH_BALANCE_CORRECTIONS_MIGRATION_NAME = migration(39).name;
export const BANK_STATEMENT_ORDER_MIGRATION_CHECKSUM = migration(40).checksum;
export const BANK_STATEMENT_ORDER_MIGRATION_NAME = migration(40).name;
export const LEGACY_OPENING_CREDITOR_RECLASSIFICATION_MIGRATION_CHECKSUM = migration(41).checksum;
export const LEGACY_OPENING_CREDITOR_RECLASSIFICATION_MIGRATION_NAME = migration(41).name;
export const LEGACY_BANK_PAYABLE_BACKFILLS_MIGRATION_CHECKSUM = migration(42).checksum;
export const LEGACY_BANK_PAYABLE_BACKFILLS_MIGRATION_NAME = migration(42).name;
export const VAT_FILING_EVIDENCE_MIGRATION_CHECKSUM = migration(43).checksum;
export const VAT_FILING_EVIDENCE_MIGRATION_NAME = migration(43).name;
export const IMPORTED_RECEIVABLE_BANK_SETTLEMENTS_MIGRATION_CHECKSUM = migration(44).checksum;
export const IMPORTED_RECEIVABLE_BANK_SETTLEMENTS_MIGRATION_NAME = migration(44).name;

export type SupportedSchemaMigration = {
  id: number;
  name: string;
  checksum: string;
};

export type SchemaMigrationIdentity = {
  id: number;
  name: string;
  checksum?: string | null;
};

const SUPPORTED_SCHEMA_MIGRATIONS: readonly SupportedSchemaMigration[] = MIGRATIONS.map(({ id, name, checksum }) => ({ id, name, checksum }));
/* legacy registration removed: artifacts are the source of identity. */

export const CURRENT_SCHEMA_VERSION = SUPPORTED_SCHEMA_MIGRATIONS.at(-1)!.id;

/** Immutable migration catalogue for read-only compatibility inspection. */
export function supportedSchemaMigrations(): readonly SupportedSchemaMigration[] {
  return SUPPORTED_SCHEMA_MIGRATIONS;
}

type MigrationRow = {
  id: number;
  name: string;
  checksum?: string | null;
  applied_at: string;
  applied_by_version?: string | null;
  applied_by_commit?: string | null;
};

type MigrationColumn = {
  name: string;
  notnull: number;
};

function tableExists(db: Database): boolean {
  return db
    .query(
      "SELECT 1 AS present FROM sqlite_master WHERE type = 'table' AND name = 'schema_migrations'",
    )
    .get() != null;
}

function migrationColumnInfo(db: Database): MigrationColumn[] {
  if (!tableExists(db)) return [];
  return db.query("PRAGMA table_info(schema_migrations)").all() as MigrationColumn[];
}

function migrationColumns(db: Database): Set<string> {
  return new Set(migrationColumnInfo(db).map((row) => row.name));
}

/** Validate a complete append-only prefix of the migrations this runtime knows. */
export function validateSchemaMigrationHistory(
  rows: readonly SchemaMigrationIdentity[],
  supported: readonly SupportedSchemaMigration[] = SUPPORTED_SCHEMA_MIGRATIONS,
  checksumsRequired = true,
): void {
  if (supported.some((migration, index) => migration.id !== index + 1)) {
    throw new Error("runtime schema migration catalog must be contiguous from version 1");
  }
  const newestSupported = supported.at(-1)?.id ?? 0;
  const newestApplied = rows.at(-1)?.id ?? 0;
  if (newestApplied > newestSupported) {
    throw new Error(
      `database schema version ${newestApplied} is newer than supported version ${newestSupported}; upgrade Rentemester before opening this ledger`,
    );
  }

  for (let index = 0; index < rows.length; index += 1) {
    const row = rows[index]!;
    const expected = supported[index];
    if (!expected || row.id !== expected.id) {
      throw new Error("schema migration history is not a complete append-only prefix");
    }
    if (row.name !== expected.name) {
      throw new Error(`schema migration ${row.id} has unexpected name '${row.name}'`);
    }
    if (checksumsRequired && !row.checksum) {
      throw new Error("schema migration history contains a missing checksum");
    }
    if (row.checksum && row.checksum !== expected.checksum) {
      throw new Error(
        `schema migration ${row.id} checksum mismatch; the ledger migration history may have been modified`,
      );
    }
  }
}

/** Reject a ledger created by newer or altered software before mutation. */
export function assertSchemaCompatibility(db: Database): void {
  if (!tableExists(db)) return;

  const columns = migrationColumns(db);
  const selectChecksum = columns.has("checksum") ? ", checksum" : "";
  const rows = db
    .query(`SELECT id, name, applied_at${selectChecksum} FROM schema_migrations ORDER BY id`)
    .all() as MigrationRow[];
  validateSchemaMigrationHistory(rows, SUPPORTED_SCHEMA_MIGRATIONS, columns.has("checksum"));
}

function createStrictMigrationTable(db: Database, name: string): void {
  db.exec(`
    CREATE TABLE ${name} (
      id INTEGER PRIMARY KEY,
      name TEXT NOT NULL UNIQUE,
      checksum TEXT NOT NULL,
      applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
      applied_by_version TEXT NOT NULL,
      applied_by_commit TEXT
    );
  `);
}

/**
 * Stamp a successfully normalised schema and upgrade the legacy migration
 * ledger to strict NOT NULL provenance. A missing checksum is adopted only
 * when the legacy table never had a checksum column; once that column exists,
 * null means corrupt/incomplete history and assertSchemaCompatibility rejects it.
 */
export function recordSchemaBaseline(db: Database): void {
  const columns = migrationColumns(db);
  const columnInfo = migrationColumnInfo(db);
  const build = getBuildIdentity();
  const hasChecksumColumn = columns.has("checksum");
  const select = [
    "id",
    "name",
    "applied_at",
    hasChecksumColumn ? "checksum" : "NULL AS checksum",
    columns.has("applied_by_version")
      ? "applied_by_version"
      : "NULL AS applied_by_version",
    columns.has("applied_by_commit")
      ? "applied_by_commit"
      : "NULL AS applied_by_commit",
  ].join(", ");
  const rows = db
    .query(`SELECT ${select} FROM schema_migrations ORDER BY id`)
    .all() as MigrationRow[];

  validateSchemaMigrationHistory(rows, SUPPORTED_SCHEMA_MIGRATIONS, hasChecksumColumn);

  const strictColumns = new Map(columnInfo.map((column) => [column.name, column.notnull]));
  const isStrict =
    strictColumns.get("name") === 1 &&
    strictColumns.get("checksum") === 1 &&
    strictColumns.get("applied_at") === 1 &&
    strictColumns.get("applied_by_version") === 1 &&
    strictColumns.has("applied_by_commit");

  if (!isStrict) {
    db.transaction(() => {
      db.exec("DROP TABLE IF EXISTS schema_migrations_strict;");
      createStrictMigrationTable(db, "schema_migrations_strict");
      for (const row of rows) {
        db.query(
          `INSERT INTO schema_migrations_strict
             (id, name, checksum, applied_at, applied_by_version, applied_by_commit)
           VALUES (?, ?, ?, ?, ?, ?)`,
        ).run(
          row.id,
          row.name,
          row.checksum ?? SUPPORTED_SCHEMA_MIGRATIONS[row.id - 1]!.checksum,
          row.applied_at,
          row.applied_by_version ?? build.version,
          row.applied_by_commit ?? build.gitCommit,
        );
      }
      db.exec(`
        DROP TABLE schema_migrations;
        ALTER TABLE schema_migrations_strict RENAME TO schema_migrations;
      `);
    })();
  }

  const existing = db
    .query("SELECT id FROM schema_migrations WHERE id = ?")
    .get(BASELINE_SCHEMA_VERSION);
  if (!existing) {
    db.query(
      `INSERT INTO schema_migrations
         (id, name, checksum, applied_by_version, applied_by_commit)
       VALUES (?, ?, ?, ?, ?)`,
    ).run(
      BASELINE_SCHEMA_VERSION,
      BASELINE_MIGRATION_NAME,
      BASELINE_MIGRATION_CHECKSUM,
      build.version,
      build.gitCommit,
    );
  }
}

export function readSchemaMigrations(db: Database): MigrationRow[] {
  if (!tableExists(db)) return [];
  return db
    .query(
      `SELECT id, name, checksum, applied_at, applied_by_version, applied_by_commit
         FROM schema_migrations
        ORDER BY id`,
    )
    .all() as MigrationRow[];
}

/** True only for a complete, checksummed ledger known by this runtime. */
export function schemaHistoryIsCurrent(db: Database): boolean {
  if (!tableExists(db) || !migrationColumns(db).has("checksum")) return false;
  try {
    const rows = readSchemaMigrations(db);
    validateSchemaMigrationHistory(rows);
    return rows.length === CURRENT_SCHEMA_VERSION;
  } catch { return false; }
}

/** Apply migrations after the immutable v1 normalization has completed. */
export function applySchemaMigrations(db: Database): void {
  const build = getBuildIdentity();
  const migrations = MIGRATIONS.slice(1);

  for (const migration of migrations) {
    if (db.query("SELECT id FROM schema_migrations WHERE id = ?").get(migration.id)) continue;
    const parsed = JSON.parse(migration.artifact.toString("utf8")) as { sql: string };
    db.transaction(() => {
      if (migration.id === 19) {
        db.exec("DROP TRIGGER IF EXISTS document_company_contexts_no_update; DROP TRIGGER IF EXISTS document_company_contexts_no_delete;");
      }
      const recurringAlreadyUpgraded = migration.id === 3 &&
        (db.query("PRAGMA table_info(recurring_invoice_templates)").all() as Array<{ name: string }>)
          .some((column) => column.name === "interval_count");
      if (recurringAlreadyUpgraded) {
        // Recover a ledger whose v3 business tables committed but whose
        // migration row or auxiliary delivery table was lost. Never rebuild
        // the already-upgraded parent/child pair in that state.
        db.exec(`
          CREATE TABLE IF NOT EXISTS recurring_invoice_generation_claims (
            id INTEGER PRIMARY KEY,
            template_id INTEGER NOT NULL REFERENCES recurring_invoice_templates(id),
            period_index INTEGER NOT NULL CHECK(period_index >= 0),
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            UNIQUE(template_id, period_index)
          );
          INSERT OR IGNORE INTO recurring_invoice_generation_claims(template_id, period_index)
            SELECT template_id, period_index FROM recurring_invoice_generations;
          CREATE TABLE IF NOT EXISTS recurring_invoice_delivery_events (
            id INTEGER PRIMARY KEY,
            generation_id INTEGER NOT NULL REFERENCES recurring_invoice_generations(id),
            channel TEXT NOT NULL CHECK(channel IN ('email','digisense')),
            event_type TEXT NOT NULL CHECK(event_type IN ('attempted','acknowledged','accepted_pending','terminal_failed','uncertain','preflight_failed')),
            provider_id TEXT,
            message TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
          );
          CREATE INDEX IF NOT EXISTS idx_recurring_invoice_delivery_events_generation
            ON recurring_invoice_delivery_events(generation_id,id DESC);
        `);
      } else {
        // A damaged migration ledger can be missing the v4 row while its
        // committed tables and guards remain. The migration is deliberately
        // replay-safe: remove only its canonical trigger names, then let the
        // IF NOT EXISTS table definitions preserve the recorded evidence.
        if (migration.id === 4 || migration.id === 5 || migration.id === 6 || migration.id === 7 || migration.id === 8 || migration.id === 9 || migration.id === 10 || migration.id === 11 || migration.id === 12 || migration.id === 13 || migration.id === 14 || migration.id === 15 || migration.id === 17 || migration.id === 18 || migration.id === 22) {
          const triggerStatements = parsed.sql.match(/CREATE TRIGGER[\s\S]*?END;/gi) ?? [];
          for (const statement of triggerStatements) {
            const name = /CREATE TRIGGER\s+([A-Za-z_][A-Za-z0-9_]*)/i.exec(statement)?.[1];
            if (name) db.exec(`DROP TRIGGER IF EXISTS ${name};`);
          }
        }
        // v11 may be replayed against a baseline-shaped ledger after only the
        // migration rows were lost. SQLite has no ADD COLUMN IF NOT EXISTS.
        let sql = migration.id === 11 && (db.query("PRAGMA table_info(exceptions)").all() as Array<{ name: string }>).some((column) => column.name === "resolution_key")
          ? parsed.sql.replace(/ALTER TABLE exceptions ADD COLUMN resolution_key TEXT;\s*/, "")
          : parsed.sql;
        if (migration.id === 16) {
          if ((db.query("PRAGMA table_info(invoice_extraction_attempts)").all() as Array<{ name: string }>).some((column) => column.name === "initiated_by")) sql = sql.replace(/ALTER TABLE invoice_extraction_attempts ADD COLUMN initiated_by TEXT;\s*/, "");
          if ((db.query("PRAGMA table_info(invoice_extraction_results)").all() as Array<{ name: string }>).some((column) => column.name === "initiated_by")) sql = sql.replace(/ALTER TABLE invoice_extraction_results ADD COLUMN initiated_by TEXT;\s*/, "");
        }
        if (migration.id === 32 && (db.query("PRAGMA table_info(accounting_dimension_assignment_events)").all() as Array<{ name: string }>).some((column) => column.name === "source_ref")) {
          // Recovery after a lost migration-ledger row must preserve the
          // already-added provenance column. SQLite has no ADD COLUMN IF NOT
          // EXISTS; the remaining v32 objects are replay-safe.
          sql = sql.replace(/ALTER TABLE accounting_dimension_assignment_events ADD COLUMN source_ref TEXT;\s*/, "");
        }
        if (migration.id === 40) {
          // A restored/lost migration ledger may already have the three
          // additive columns. Preserve their immutable evidence and replay
          // only the index/guards SQLite can safely recreate.
          const columns = new Set((db.query("PRAGMA table_info(bank_transactions)").all() as Array<{ name: string }>).map((column) => column.name));
          if (columns.has("statement_row_index")) sql = sql.replace(/ALTER TABLE bank_transactions ADD COLUMN statement_row_index INTEGER;\s*/, "");
          if (columns.has("statement_order")) sql = sql.replace(/ALTER TABLE bank_transactions ADD COLUMN statement_order TEXT CHECK\(statement_order IN \('ascending','descending'\)\);\s*/, "");
          if (columns.has("statement_order_provenance")) sql = sql.replace(/ALTER TABLE bank_transactions ADD COLUMN statement_order_provenance TEXT;\s*/, "");
        }
        if (migration.id === 17 && db.query("SELECT 1 FROM sqlite_master WHERE type='table' AND name='document_pdf_parse_results'").get()) {
          // Recovery after a migration-ledger loss: retain immutable rows,
          // rebuild just this migration's indexes and append-only guards.
          sql = sql.replace(/^CREATE TABLE document_pdf_parse_attempts[\s\S]*?;\nCREATE TABLE document_pdf_parse_results[\s\S]*?;\nCREATE TABLE document_pdf_parse_pages[\s\S]*?;\n/, "")
            .replace(/CREATE VIEW document_pdf_parses[\s\S]*?;\n?/, "")
            .replaceAll("CREATE INDEX idx_", "CREATE INDEX IF NOT EXISTS idx_");
        }
        if (migration.id === 29 && db.query("SELECT 1 FROM sqlite_master WHERE type='table' AND name='document_party_resolution_events'").get()) {
          // Recovery after a migration-ledger loss: v29's append-only tables,
          // views and guards are already present. Re-running its table rename
          // and CREATE sequence would both fail and risk disturbing evidence;
          // restoring the missing immutable migration row is sufficient.
          sql = "";
        }
        if (migration.id === 47 && db.query("SELECT 1 FROM sqlite_master WHERE type='table' AND name='purchase_case_events'").get()) {
          // A lost migration ledger must never recreate an already present
          // append-only purchase-case table. Later additive migrations still
          // supply any newly introduced evidence columns.
          sql = "";
        }
        if (migration.id === 48 && db.query("SELECT 1 FROM sqlite_master WHERE type='table' AND name='purchase_case_group_events'").get()) {
          sql = "";
        }
        if (migration.id === 49) {
          const columns = new Set((db.query("PRAGMA table_info(purchase_case_events)").all() as Array<{name:string}>).map((column) => column.name));
          if (columns.has("principal_id")) sql = sql.replace(/ALTER TABLE purchase_case_events ADD COLUMN principal_id TEXT;\s*/, "");
          if (columns.has("approval_policy_hash")) sql = sql.replace(/ALTER TABLE purchase_case_events ADD COLUMN approval_policy_hash TEXT;\s*/, "");
          sql = sql.replace("CREATE INDEX purchase_case_creator_principal", "CREATE INDEX IF NOT EXISTS purchase_case_creator_principal");
        }
        if (migration.id === 50) {
          const columns = new Set((db.query("PRAGMA table_info(bookkeeping_batch_revision_approvals)").all() as Array<{name:string}>).map((column) => column.name));
          if (columns.has("approval_policy_hash")) sql = sql.replace(/ALTER TABLE bookkeeping_batch_revision_approvals ADD COLUMN approval_policy_hash TEXT;\s*/, "");
        }
        if (migration.id === 51) {
          const columns = new Set((db.query("PRAGMA table_info(accounting_draft_events)").all() as Array<{name:string}>).map((column) => column.name));
          if (columns.has("principal_id")) sql = sql.replace(/ALTER TABLE accounting_draft_events ADD COLUMN principal_id TEXT;\s*/, "");
          if (columns.has("approval_policy_hash")) sql = sql.replace(/ALTER TABLE accounting_draft_events ADD COLUMN approval_policy_hash TEXT;\s*/, "");
          sql = sql.replace("CREATE INDEX accounting_draft_events_principal", "CREATE INDEX IF NOT EXISTS accounting_draft_events_principal");
        }
        if (sql.trim()) db.exec(sql);
      }
      db.query(`INSERT INTO schema_migrations (id, name, checksum, applied_by_version, applied_by_commit) VALUES (?, ?, ?, ?, ?)`)
        .run(migration.id, migration.name, migration.checksum, build.version, build.gitCommit);
    }).immediate();
  }

  // `migrate()` restores the immutable v1 trigger catalogue before applying
  // post-baseline migrations. On a second open that catalogue would otherwise
  // replace the v3 template guard with the old body that does not protect the
  // new cadence and channel columns. Re-assert the post-migration guards and
  // delivery reservation index on every open, including when v3 was already
  // recorded.
  if (db.query("SELECT id FROM schema_migrations WHERE id = 3").get()) {
    db.transaction(() => {
      db.exec(`
        DROP TRIGGER IF EXISTS recurring_invoice_templates_guard_update;
        CREATE TRIGGER recurring_invoice_templates_guard_update
        BEFORE UPDATE ON recurring_invoice_templates
        WHEN OLD.name != NEW.name
          OR OLD.interval != NEW.interval
          OR OLD.interval_count != NEW.interval_count
          OR OLD.delivery_channel != NEW.delivery_channel
          OR OLD.first_issue_date != NEW.first_issue_date
          OR OLD.payment_terms_days != NEW.payment_terms_days
          OR OLD.delivery_period_mode != NEW.delivery_period_mode
          OR OLD.payload_json != NEW.payload_json
          OR OLD.created_at != NEW.created_at
          OR NEW.next_issue_date < OLD.next_issue_date
          OR (OLD.active = 0 AND NEW.active = 1)
        BEGIN
          SELECT RAISE(ABORT, 'recurring invoice templates are append-only; only next_issue_date may advance and active may be retired');
        END;
        CREATE UNIQUE INDEX IF NOT EXISTS idx_recurring_invoice_delivery_single_attempt
          ON recurring_invoice_delivery_events(generation_id)
          WHERE event_type = 'attempted';
      `);
    }).immediate();
  }

  // These tables are intentionally absent from the immutable v1 schema. Their
  // append-only guards therefore need the same drop+create reassertion on
  // every open as baseline triggers, including after a privileged tamper.
  if (db.query("SELECT id FROM schema_migrations WHERE id = 4").get()) {
    const parsed = JSON.parse(artifact(4).toString("utf8")) as { sql: string };
    const triggerStatements = parsed.sql.match(/CREATE TRIGGER[\s\S]*?END;/gi) ?? [];
    db.transaction(() => {
      for (const statement of triggerStatements) {
        const name = /CREATE TRIGGER\s+([A-Za-z_][A-Za-z0-9_]*)/i.exec(statement)?.[1];
        if (!name) continue;
        db.exec(`DROP TRIGGER IF EXISTS ${name};`);
        db.exec(statement);
      }
      // v11 originally permitted an in-place started→posted update. Reassert
      // a strict append-only application ledger on every open instead; callers
      // reserve and link inside one transaction and insert the final row.
      db.exec("DROP TRIGGER IF EXISTS purchase_posting_applications_no_update;");
      db.exec("CREATE TRIGGER purchase_posting_applications_no_update BEFORE UPDATE ON purchase_posting_applications BEGIN SELECT RAISE(ABORT, 'purchase posting applications are append-only'); END;");
      db.exec("CREATE TRIGGER IF NOT EXISTS purchase_posting_applications_no_delete BEFORE DELETE ON purchase_posting_applications BEGIN SELECT RAISE(ABORT, 'purchase posting applications are append-only'); END;");
    }).immediate();
  }

  if (db.query("SELECT id FROM schema_migrations WHERE id = 5").get()) {
    const parsed = JSON.parse(artifact(5).toString("utf8")) as { sql: string };
    const triggerStatements = parsed.sql.match(/CREATE TRIGGER[\s\S]*?END;/gi) ?? [];
    db.transaction(() => {
      for (const statement of triggerStatements) {
        const name = /CREATE TRIGGER\s+([A-Za-z_][A-Za-z0-9_]*)/i.exec(statement)?.[1];
        if (!name) continue;
        db.exec(`DROP TRIGGER IF EXISTS ${name};`);
        db.exec(statement);
      }
    }).immediate();
  }

  if (db.query("SELECT id FROM schema_migrations WHERE id = 6").get()) {
    const parsed = JSON.parse(artifact(6).toString("utf8")) as { sql: string };
    const triggerStatements = parsed.sql.match(/CREATE TRIGGER[\s\S]*?END;/gi) ?? [];
    db.transaction(() => {
      for (const statement of triggerStatements) {
        const name = /CREATE TRIGGER\s+([A-Za-z_][A-Za-z0-9_]*)/i.exec(statement)?.[1];
        if (!name) continue;
        db.exec(`DROP TRIGGER IF EXISTS ${name};`);
        db.exec(statement);
      }
    }).immediate();
  }

  if (db.query("SELECT id FROM schema_migrations WHERE id = 8").get()) {
    const parsed = JSON.parse(artifact(8).toString("utf8")) as { sql: string };
    const triggerStatements = parsed.sql.match(/CREATE TRIGGER[\s\S]*?END;/gi) ?? [];
    db.transaction(() => {
      for (const statement of triggerStatements) {
        const name = /CREATE TRIGGER\s+([A-Za-z_][A-Za-z0-9_]*)/i.exec(statement)?.[1];
        if (!name) continue;
        db.exec(`DROP TRIGGER IF EXISTS ${name};`);
        db.exec(statement);
      }
    }).immediate();
  }

  if (db.query("SELECT id FROM schema_migrations WHERE id = 9").get()) {
    const parsed = JSON.parse(artifact(9).toString("utf8")) as { sql: string };
    const triggerStatements = parsed.sql.match(/CREATE TRIGGER[\s\S]*?END;/gi) ?? [];
    db.transaction(() => {
      for (const statement of triggerStatements) {
        const name = /CREATE TRIGGER\s+([A-Za-z_][A-Za-z0-9_]*)/i.exec(statement)?.[1];
        if (!name) continue;
        db.exec(`DROP TRIGGER IF EXISTS ${name};`);
        db.exec(statement);
      }
    }).immediate();
  }

  if (db.query("SELECT id FROM schema_migrations WHERE id = 10").get()) {
    const parsed = JSON.parse(artifact(10).toString("utf8")) as { sql: string };
    const triggerStatements = parsed.sql.match(/CREATE TRIGGER[\s\S]*?END;/gi) ?? [];
    db.transaction(() => {
      for (const statement of triggerStatements) {
        const name = /CREATE TRIGGER\s+([A-Za-z_][A-Za-z0-9_]*)/i.exec(statement)?.[1];
        if (!name) continue;
        db.exec(`DROP TRIGGER IF EXISTS ${name};`);
        db.exec(statement);
      }
    }).immediate();
  }
  if (db.query("SELECT id FROM schema_migrations WHERE id = 11").get()) {
    const parsed = JSON.parse(artifact(11).toString("utf8")) as { sql: string };
    const triggerStatements = parsed.sql.match(/CREATE TRIGGER[\s\S]*?END;/gi) ?? [];
    db.transaction(() => {
      for (const statement of triggerStatements) {
        const name = /CREATE TRIGGER\s+([A-Za-z_][A-Za-z0-9_]*)/i.exec(statement)?.[1];
        if (!name) continue;
        db.exec(`DROP TRIGGER IF EXISTS ${name};`);
        db.exec(statement);
      }
    }).immediate();
  }
  if (db.query("SELECT id FROM schema_migrations WHERE id = 12").get()) {
    const parsed = JSON.parse(artifact(12).toString("utf8")) as { sql: string };
    const triggerStatements = parsed.sql.match(/CREATE TRIGGER[\s\S]*?END;/gi) ?? [];
    db.transaction(() => {
      for (const statement of triggerStatements) {
        const name = /CREATE TRIGGER\s+([A-Za-z_][A-Za-z0-9_]*)/i.exec(statement)?.[1];
        if (!name) continue;
        db.exec(`DROP TRIGGER IF EXISTS ${name};`);
        db.exec(statement);
      }
    }).immediate();
  }
  if (db.query("SELECT id FROM schema_migrations WHERE id = 13").get()) {
    const parsed = JSON.parse(artifact(13).toString("utf8")) as { sql: string };
    const triggers = parsed.sql.match(/CREATE TRIGGER[\s\S]*?END;/gi) ?? [];
    db.transaction(() => { for (const statement of triggers) { const name = /CREATE TRIGGER\s+([A-Za-z_][A-Za-z0-9_]*)/i.exec(statement)?.[1]; if (name) { db.exec(`DROP TRIGGER IF EXISTS ${name};`); db.exec(statement); } } }).immediate();
  }
  if (db.query("SELECT id FROM schema_migrations WHERE id = 14").get()) {
    const parsed = JSON.parse(artifact(14).toString("utf8")) as { sql: string };
    const triggers = parsed.sql.match(/CREATE TRIGGER[\s\S]*?END;/gi) ?? [];
    db.transaction(() => { for (const statement of triggers) { const name = /CREATE TRIGGER\s+([A-Za-z_][A-Za-z0-9_]*)/i.exec(statement)?.[1]; if (name) { db.exec(`DROP TRIGGER IF EXISTS ${name};`); db.exec(statement); } } }).immediate();
  }
  if (db.query("SELECT id FROM schema_migrations WHERE id = 17").get()) {
    const parsed = JSON.parse(artifact(17).toString("utf8")) as { sql: string };
    const triggers = parsed.sql.match(/CREATE TRIGGER[\s\S]*?END;/gi) ?? [];
    db.transaction(() => { for (const statement of triggers) { const name = /CREATE TRIGGER\s+([A-Za-z_][A-Za-z0-9_]*)/i.exec(statement)?.[1]; if (name) { db.exec(`DROP TRIGGER IF EXISTS ${name};`); db.exec(statement); } } }).immediate();
  }
  if (db.query("SELECT id FROM schema_migrations WHERE id = 18").get()) {
    const parsed = JSON.parse(artifact(18).toString("utf8")) as { sql: string };
    const triggers = parsed.sql.match(/CREATE TRIGGER[\s\S]*?END;/gi) ?? [];
    db.transaction(() => { for (const statement of triggers) { const name = /CREATE TRIGGER\s+([A-Za-z_][A-Za-z0-9_]*)/i.exec(statement)?.[1]; if (name) { db.exec(`DROP TRIGGER IF EXISTS ${name};`); db.exec(statement); } } }).immediate();
  }
  if (db.query("SELECT id FROM schema_migrations WHERE id = 19").get()) {
    const parsed = JSON.parse(artifact(19).toString("utf8")) as { sql: string };
    const triggers = parsed.sql.match(/CREATE TRIGGER[\s\S]*?END;/gi) ?? [];
    db.transaction(() => { for (const statement of triggers) { const name = /CREATE TRIGGER\s+([A-Za-z_][A-Za-z0-9_]*)/i.exec(statement)?.[1]; if (name) { db.exec(`DROP TRIGGER IF EXISTS ${name};`); db.exec(statement); } } }).immediate();
  }
  if (db.query("SELECT id FROM schema_migrations WHERE id = 22").get()) {
    const parsed = JSON.parse(artifact(22).toString("utf8")) as { sql: string };
    const triggers = parsed.sql.match(/CREATE TRIGGER[\s\S]*?END;/gi) ?? [];
    db.transaction(() => { for (const statement of triggers) { const name = /CREATE TRIGGER\s+([A-Za-z_][A-Za-z0-9_]*)/i.exec(statement)?.[1]; if (name) { db.exec(`DROP TRIGGER IF EXISTS ${name};`); db.exec(statement); } } }).immediate();
  }
  if (db.query("SELECT id FROM schema_migrations WHERE id = 34").get()) {
    const parsed = JSON.parse(artifact(34).toString("utf8")) as { sql: string };
    const triggers = parsed.sql.match(/CREATE TRIGGER[\s\S]*?END;/gi) ?? [];
    db.transaction(() => {
      for (const statement of triggers) {
        const name = /CREATE TRIGGER\s+([A-Za-z_][A-Za-z0-9_]*)/i.exec(statement)?.[1];
        if (name) {
          db.exec(`DROP TRIGGER IF EXISTS ${name};`);
          db.exec(statement);
        }
      }
    }).immediate();
  }
  if (db.query("SELECT id FROM schema_migrations WHERE id = 35").get()) {
    const parsed = JSON.parse(artifact(35).toString("utf8")) as { sql: string };
    const triggers = parsed.sql.match(/CREATE TRIGGER[\s\S]*?END;/gi) ?? [];
    db.transaction(() => { for (const statement of triggers) { const name = /CREATE TRIGGER\s+([A-Za-z_][A-Za-z0-9_]*)/i.exec(statement)?.[1]; if (name) { db.exec(`DROP TRIGGER IF EXISTS ${name};`); db.exec(statement); } } }).immediate();
  }
  if (db.query("SELECT id FROM schema_migrations WHERE id = 36").get()) {
    const parsed = JSON.parse(artifact(36).toString("utf8")) as { sql: string };
    const triggers = parsed.sql.match(/CREATE TRIGGER[\s\S]*?END;/gi) ?? [];
    db.transaction(() => { for (const statement of triggers) { const name = /CREATE TRIGGER(?: IF NOT EXISTS)?\s+([A-Za-z_][A-Za-z0-9_]*)/i.exec(statement)?.[1]; if (name) { db.exec(`DROP TRIGGER IF EXISTS ${name};`); db.exec(statement.replace("CREATE TRIGGER IF NOT EXISTS", "CREATE TRIGGER")); } } }).immediate();
  }
  if (db.query("SELECT id FROM schema_migrations WHERE id = 37").get()) {
    const parsed = JSON.parse(artifact(37).toString("utf8")) as { sql: string };
    const triggers = parsed.sql.match(/CREATE TRIGGER[\s\S]*?END;/gi) ?? [];
    db.transaction(() => { for (const statement of triggers) { const name = /CREATE TRIGGER(?: IF NOT EXISTS)?\s+([A-Za-z_][A-Za-z0-9_]*)/i.exec(statement)?.[1]; if (name) { db.exec(`DROP TRIGGER IF EXISTS ${name};`); db.exec(statement.replace("CREATE TRIGGER IF NOT EXISTS", "CREATE TRIGGER")); } } }).immediate();
  }
  if (db.query("SELECT id FROM schema_migrations WHERE id = 38").get()) {
    const parsed = JSON.parse(artifact(38).toString("utf8")) as { sql: string };
    const triggers = parsed.sql.match(/CREATE TRIGGER[\s\S]*?END;/gi) ?? [];
    db.transaction(() => { for (const statement of triggers) { const name = /CREATE TRIGGER(?: IF NOT EXISTS)?\s+([A-Za-z_][A-Za-z0-9_]*)/i.exec(statement)?.[1]; if (name) { db.exec(`DROP TRIGGER IF EXISTS ${name};`); db.exec(statement.replace("CREATE TRIGGER IF NOT EXISTS", "CREATE TRIGGER")); } } }).immediate();
  }
  if (db.query("SELECT id FROM schema_migrations WHERE id = 39").get()) {
    const parsed = JSON.parse(artifact(39).toString("utf8")) as { sql: string };
    const triggers = parsed.sql.match(/CREATE TRIGGER[\s\S]*?END;/gi) ?? [];
    db.transaction(() => { for (const statement of triggers) { const name = /CREATE TRIGGER(?: IF NOT EXISTS)?\s+([A-Za-z_][A-Za-z0-9_]*)/i.exec(statement)?.[1]; if (name) { db.exec(`DROP TRIGGER IF EXISTS ${name};`); db.exec(statement.replace("CREATE TRIGGER IF NOT EXISTS", "CREATE TRIGGER")); } } }).immediate();
  }
  if (db.query("SELECT id FROM schema_migrations WHERE id = 40").get()) {
    const parsed = JSON.parse(artifact(40).toString("utf8")) as { sql: string };
    const triggers = parsed.sql.match(/CREATE TRIGGER[\s\S]*?END;/gi) ?? [];
    db.transaction(() => { for (const statement of triggers) { const name = /CREATE TRIGGER(?: IF NOT EXISTS)?\s+([A-Za-z_][A-Za-z0-9_]*)/i.exec(statement)?.[1]; if (name) { db.exec(`DROP TRIGGER IF EXISTS ${name};`); db.exec(statement.replace("CREATE TRIGGER IF NOT EXISTS", "CREATE TRIGGER")); } } }).immediate();
  }
  if (db.query("SELECT id FROM schema_migrations WHERE id = 41").get()) {
    const parsed = JSON.parse(artifact(41).toString("utf8")) as { sql: string };
    const triggers = parsed.sql.match(/CREATE TRIGGER[\s\S]*?END;/gi) ?? [];
    db.transaction(() => { for (const statement of triggers) { const name = /CREATE TRIGGER(?: IF NOT EXISTS)?\s+([A-Za-z_][A-Za-z0-9_]*)/i.exec(statement)?.[1]; if (name) { db.exec(`DROP TRIGGER IF EXISTS ${name};`); db.exec(statement.replace("CREATE TRIGGER IF NOT EXISTS", "CREATE TRIGGER")); } } }).immediate();
  }
  if (db.query("SELECT id FROM schema_migrations WHERE id = 42").get()) {
    const parsed = JSON.parse(artifact(42).toString("utf8")) as { sql: string };
    const triggers = parsed.sql.match(/CREATE TRIGGER(?: IF NOT EXISTS)?\s+[\s\S]*?END;/gi) ?? [];
    db.transaction(() => { for (const statement of triggers) { const name = /CREATE TRIGGER(?: IF NOT EXISTS)?\s+([A-Za-z_][A-Za-z0-9_]*)/i.exec(statement)?.[1]; if (name) { db.exec(`DROP TRIGGER IF EXISTS ${name};`); db.exec(statement.replace("CREATE TRIGGER IF NOT EXISTS", "CREATE TRIGGER")); } } }).immediate();
  }
}
