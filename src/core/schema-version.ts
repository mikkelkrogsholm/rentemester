import type { Database } from "bun:sqlite";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { getBuildIdentity } from "./build-identity";

export const BASELINE_SCHEMA_VERSION = 1;
export const BASELINE_MIGRATION_NAME = "rentemester-schema-baseline-v1";

// The ledger checksum is derived from an immutable, reviewable migration
// artifact. Its own tests bind the artifact to the exact schema.sql bytes and
// baseline-normalization body in db.ts.
const BASELINE_MIGRATION_ARTIFACT = readFileSync(
  join(import.meta.dir, "migrations", "0001-baseline.json"),
);
export const BASELINE_MIGRATION_CHECKSUM = createHash("sha256")
  .update(BASELINE_MIGRATION_ARTIFACT)
  .digest("hex");
const PEPPOL_SUBMISSION_EVENTS_MIGRATION_ARTIFACT = readFileSync(
  join(import.meta.dir, "migrations", "0002-peppol-submission-events.json"),
);
export const PEPPOL_SUBMISSION_EVENTS_MIGRATION_CHECKSUM = createHash("sha256")
  .update(PEPPOL_SUBMISSION_EVENTS_MIGRATION_ARTIFACT)
  .digest("hex");
export const PEPPOL_SUBMISSION_EVENTS_MIGRATION_NAME = "rentemester-peppol-submission-events-v2";
const RECURRING_AUTOMATION_MIGRATION_ARTIFACT = readFileSync(
  join(import.meta.dir, "migrations", "0003-recurring-automation.json"),
);
export const RECURRING_AUTOMATION_MIGRATION_CHECKSUM = createHash("sha256")
  .update(RECURRING_AUTOMATION_MIGRATION_ARTIFACT)
  .digest("hex");
export const RECURRING_AUTOMATION_MIGRATION_NAME = "rentemester-recurring-automation-v3";
const DINERO_IMPORT_PROVENANCE_MIGRATION_ARTIFACT = readFileSync(
  join(import.meta.dir, "migrations", "0004-dinero-import-provenance.json"),
);
export const DINERO_IMPORT_PROVENANCE_MIGRATION_CHECKSUM = createHash("sha256")
  .update(DINERO_IMPORT_PROVENANCE_MIGRATION_ARTIFACT)
  .digest("hex");
export const DINERO_IMPORT_PROVENANCE_MIGRATION_NAME = "rentemester-dinero-import-provenance-v4";
const MIGRATION_OPEN_ITEMS_MIGRATION_ARTIFACT = readFileSync(
  join(import.meta.dir, "migrations", "0005-migration-open-items.json"),
);
export const MIGRATION_OPEN_ITEMS_MIGRATION_CHECKSUM = createHash("sha256")
  .update(MIGRATION_OPEN_ITEMS_MIGRATION_ARTIFACT)
  .digest("hex");
export const MIGRATION_OPEN_ITEMS_MIGRATION_NAME = "rentemester-migration-open-items-v5";
const BANK_JOURNAL_RECONCILIATION_LINKS_MIGRATION_ARTIFACT = readFileSync(
  join(import.meta.dir, "migrations", "0006-bank-journal-reconciliation-links.json"),
);
export const BANK_JOURNAL_RECONCILIATION_LINKS_MIGRATION_CHECKSUM = createHash("sha256")
  .update(BANK_JOURNAL_RECONCILIATION_LINKS_MIGRATION_ARTIFACT)
  .digest("hex");
export const BANK_JOURNAL_RECONCILIATION_LINKS_MIGRATION_NAME = "rentemester-bank-journal-reconciliation-links-v6";
const DOCUMENT_SCAN_EVIDENCE_MIGRATION_ARTIFACT = readFileSync(
  join(import.meta.dir, "migrations", "0007-document-scan-evidence.json"),
);
export const DOCUMENT_SCAN_EVIDENCE_MIGRATION_CHECKSUM = createHash("sha256")
  .update(DOCUMENT_SCAN_EVIDENCE_MIGRATION_ARTIFACT)
  .digest("hex");
export const DOCUMENT_SCAN_EVIDENCE_MIGRATION_NAME = "rentemester-document-scan-evidence-v7";
const ISSUED_INVOICE_PDF_IMMUTABILITY_MIGRATION_ARTIFACT = readFileSync(
  join(import.meta.dir, "migrations", "0008-issued-invoice-pdf-immutability.json"),
);
export const ISSUED_INVOICE_PDF_IMMUTABILITY_MIGRATION_CHECKSUM = createHash("sha256")
  .update(ISSUED_INVOICE_PDF_IMMUTABILITY_MIGRATION_ARTIFACT)
  .digest("hex");
export const ISSUED_INVOICE_PDF_IMMUTABILITY_MIGRATION_NAME = "rentemester-issued-invoice-pdf-immutability-v8";
const ACCOUNTING_DRAFT_WORKFLOW_MIGRATION_ARTIFACT = readFileSync(
  join(import.meta.dir, "migrations", "0009-accounting-draft-workflow.json"),
);
export const ACCOUNTING_DRAFT_WORKFLOW_MIGRATION_CHECKSUM = createHash("sha256")
  .update(ACCOUNTING_DRAFT_WORKFLOW_MIGRATION_ARTIFACT)
  .digest("hex");
export const ACCOUNTING_DRAFT_WORKFLOW_MIGRATION_NAME = "rentemester-accounting-draft-workflow-v9";
const INTERNAL_VOUCHER_EVIDENCE_MIGRATION_ARTIFACT = readFileSync(
  join(import.meta.dir, "migrations", "0010-internal-voucher-evidence.json"),
);
export const INTERNAL_VOUCHER_EVIDENCE_MIGRATION_CHECKSUM = createHash("sha256")
  .update(INTERNAL_VOUCHER_EVIDENCE_MIGRATION_ARTIFACT)
  .digest("hex");
export const INTERNAL_VOUCHER_EVIDENCE_MIGRATION_NAME = "rentemester-internal-voucher-evidence-v10";
const PURCHASE_VAT_PREFLIGHT_MIGRATION_ARTIFACT = readFileSync(
  join(import.meta.dir, "migrations", "0011-purchase-vat-preflight.json"),
);
export const PURCHASE_VAT_PREFLIGHT_MIGRATION_CHECKSUM = createHash("sha256")
  .update(PURCHASE_VAT_PREFLIGHT_MIGRATION_ARTIFACT)
  .digest("hex");
export const PURCHASE_VAT_PREFLIGHT_MIGRATION_NAME = "rentemester-purchase-vat-preflight-v11";
const POSTING_RULES_MIGRATION_ARTIFACT = readFileSync(
  join(import.meta.dir, "migrations", "0012-posting-rules.json"),
);
export const POSTING_RULES_MIGRATION_CHECKSUM = createHash("sha256")
  .update(POSTING_RULES_MIGRATION_ARTIFACT)
  .digest("hex");
export const POSTING_RULES_MIGRATION_NAME = "rentemester-posting-rules-v12";
const BOOKKEEPING_BATCHES_MIGRATION_ARTIFACT = readFileSync(join(import.meta.dir, "migrations", "0013-bookkeeping-batches.json"));
export const BOOKKEEPING_BATCHES_MIGRATION_CHECKSUM = createHash("sha256").update(BOOKKEEPING_BATCHES_MIGRATION_ARTIFACT).digest("hex");
export const BOOKKEEPING_BATCHES_MIGRATION_NAME = "rentemester-bookkeeping-batches-v13";
const INVOICE_EXTRACTION_EVIDENCE_MIGRATION_ARTIFACT = readFileSync(join(import.meta.dir, "migrations", "0014-invoice-extraction-evidence.json"));
export const INVOICE_EXTRACTION_EVIDENCE_MIGRATION_CHECKSUM = createHash("sha256").update(INVOICE_EXTRACTION_EVIDENCE_MIGRATION_ARTIFACT).digest("hex");
export const INVOICE_EXTRACTION_EVIDENCE_MIGRATION_NAME = "rentemester-invoice-extraction-evidence-v14";
const BOOKKEEPING_BATCH_RETRIES_MIGRATION_ARTIFACT = readFileSync(join(import.meta.dir, "migrations", "0015-bookkeeping-batch-retries.json"));
export const BOOKKEEPING_BATCH_RETRIES_MIGRATION_CHECKSUM = createHash("sha256").update(BOOKKEEPING_BATCH_RETRIES_MIGRATION_ARTIFACT).digest("hex");
export const BOOKKEEPING_BATCH_RETRIES_MIGRATION_NAME = "rentemester-bookkeeping-batch-retries-v15";
const INVOICE_EXTRACTION_ACTORS_MIGRATION_ARTIFACT = readFileSync(join(import.meta.dir, "migrations", "0016-invoice-extraction-actors.json"));
export const INVOICE_EXTRACTION_ACTORS_MIGRATION_CHECKSUM = createHash("sha256").update(INVOICE_EXTRACTION_ACTORS_MIGRATION_ARTIFACT).digest("hex");
export const INVOICE_EXTRACTION_ACTORS_MIGRATION_NAME = "rentemester-invoice-extraction-actors-v16";
const DOCUMENT_PDF_PARSES_MIGRATION_ARTIFACT = readFileSync(join(import.meta.dir, "migrations", "0017-document-pdf-parses.json"));
export const DOCUMENT_PDF_PARSES_MIGRATION_CHECKSUM = createHash("sha256").update(DOCUMENT_PDF_PARSES_MIGRATION_ARTIFACT).digest("hex");
export const DOCUMENT_PDF_PARSES_MIGRATION_NAME = "rentemester-document-pdf-parses-v17";
const DOCUMENT_METADATA_ENRICHMENTS_MIGRATION_ARTIFACT = readFileSync(join(import.meta.dir, "migrations", "0018-document-metadata-enrichments.json"));
export const DOCUMENT_METADATA_ENRICHMENTS_MIGRATION_CHECKSUM = createHash("sha256").update(DOCUMENT_METADATA_ENRICHMENTS_MIGRATION_ARTIFACT).digest("hex");
export const DOCUMENT_METADATA_ENRICHMENTS_MIGRATION_NAME = "rentemester-document-metadata-enrichments-v18";
const DOCUMENT_COMPANY_CONTEXTS_MIGRATION_ARTIFACT = readFileSync(join(import.meta.dir, "migrations", "0019-document-company-contexts.json"));
export const DOCUMENT_COMPANY_CONTEXTS_MIGRATION_CHECKSUM = createHash("sha256").update(DOCUMENT_COMPANY_CONTEXTS_MIGRATION_ARTIFACT).digest("hex");
export const DOCUMENT_COMPANY_CONTEXTS_MIGRATION_NAME = "rentemester-document-company-contexts-v19";
const MUTATION_IDEMPOTENCY_RECEIPTS_MIGRATION_ARTIFACT = readFileSync(join(import.meta.dir, "migrations", "0020-mutation-idempotency-receipts.json"));
export const MUTATION_IDEMPOTENCY_RECEIPTS_MIGRATION_CHECKSUM = createHash("sha256").update(MUTATION_IDEMPOTENCY_RECEIPTS_MIGRATION_ARTIFACT).digest("hex");
export const MUTATION_IDEMPOTENCY_RECEIPTS_MIGRATION_NAME = "rentemester-mutation-idempotency-foundation-v20";
const BOOKKEEPING_BATCH_REVISIONS_MIGRATION_ARTIFACT = readFileSync(join(import.meta.dir, "migrations", "0021-bookkeeping-batch-revisions.json"));
export const BOOKKEEPING_BATCH_REVISIONS_MIGRATION_CHECKSUM = createHash("sha256").update(BOOKKEEPING_BATCH_REVISIONS_MIGRATION_ARTIFACT).digest("hex");
export const BOOKKEEPING_BATCH_REVISIONS_MIGRATION_NAME = "rentemester-bookkeeping-batch-revisions-v21";
const PERIOD_CLOSE_READINESS_MIGRATION_ARTIFACT = readFileSync(join(import.meta.dir, "migrations", "0022-period-close-readiness.json"));
export const PERIOD_CLOSE_READINESS_MIGRATION_CHECKSUM = createHash("sha256").update(PERIOD_CLOSE_READINESS_MIGRATION_ARTIFACT).digest("hex");
export const PERIOD_CLOSE_READINESS_MIGRATION_NAME = "rentemester-period-close-readiness-v22";
const LOCAL_IDEMPOTENCY_TOMBSTONES_MIGRATION_ARTIFACT = readFileSync(join(import.meta.dir, "migrations", "0023-local-idempotency-tombstones.json"));
export const LOCAL_IDEMPOTENCY_TOMBSTONES_MIGRATION_CHECKSUM = createHash("sha256").update(LOCAL_IDEMPOTENCY_TOMBSTONES_MIGRATION_ARTIFACT).digest("hex");
export const LOCAL_IDEMPOTENCY_TOMBSTONES_MIGRATION_NAME = "rentemester-local-idempotency-tombstones-v23";
const BOOKKEEPING_BATCH_PRINCIPALS_MIGRATION_ARTIFACT = readFileSync(join(import.meta.dir, "migrations", "0024-bookkeeping-batch-principals.json"));
export const BOOKKEEPING_BATCH_PRINCIPALS_MIGRATION_CHECKSUM = createHash("sha256").update(BOOKKEEPING_BATCH_PRINCIPALS_MIGRATION_ARTIFACT).digest("hex");
export const BOOKKEEPING_BATCH_PRINCIPALS_MIGRATION_NAME = "rentemester-bookkeeping-batch-principals-v24";
const PERIOD_CLOSE_REVIEWS_MIGRATION_ARTIFACT = readFileSync(join(import.meta.dir, "migrations", "0025-period-close-reviews.json"));
export const PERIOD_CLOSE_REVIEWS_MIGRATION_CHECKSUM = createHash("sha256").update(PERIOD_CLOSE_REVIEWS_MIGRATION_ARTIFACT).digest("hex");
export const PERIOD_CLOSE_REVIEWS_MIGRATION_NAME = "rentemester-period-close-reviews-v25";
const DOCUMENT_PARTY_LINKS_MIGRATION_ARTIFACT = readFileSync(join(import.meta.dir, "migrations", "0026-document-party-links.json"));
export const DOCUMENT_PARTY_LINKS_MIGRATION_CHECKSUM = createHash("sha256").update(DOCUMENT_PARTY_LINKS_MIGRATION_ARTIFACT).digest("hex");
export const DOCUMENT_PARTY_LINKS_MIGRATION_NAME = "rentemester-document-party-links-v26";
const SUPPLIER_COMMITMENTS_MIGRATION_ARTIFACT = readFileSync(join(import.meta.dir, "migrations", "0027-supplier-commitments.json"));
export const SUPPLIER_COMMITMENTS_MIGRATION_CHECKSUM = createHash("sha256").update(SUPPLIER_COMMITMENTS_MIGRATION_ARTIFACT).digest("hex");
export const SUPPLIER_COMMITMENTS_MIGRATION_NAME = "rentemester-supplier-commitments-v27";
const ACCOUNTING_DIMENSIONS_MIGRATION_ARTIFACT = readFileSync(join(import.meta.dir, "migrations", "0028-accounting-dimensions.json"));
export const ACCOUNTING_DIMENSIONS_MIGRATION_CHECKSUM = createHash("sha256").update(ACCOUNTING_DIMENSIONS_MIGRATION_ARTIFACT).digest("hex");
export const ACCOUNTING_DIMENSIONS_MIGRATION_NAME = "rentemester-accounting-dimensions-v28";
const DOCUMENT_PARTY_RESOLUTION_MIGRATION_ARTIFACT = readFileSync(join(import.meta.dir, "migrations", "0029-document-party-resolution.json"));
export const DOCUMENT_PARTY_RESOLUTION_MIGRATION_CHECKSUM = createHash("sha256").update(DOCUMENT_PARTY_RESOLUTION_MIGRATION_ARTIFACT).digest("hex");
export const DOCUMENT_PARTY_RESOLUTION_MIGRATION_NAME = "rentemester-document-party-resolution-v29";
const ACCOUNTING_DIMENSION_LIFECYCLE_MIGRATION_ARTIFACT = readFileSync(join(import.meta.dir, "migrations", "0030-accounting-dimension-lifecycle.json"));
export const ACCOUNTING_DIMENSION_LIFECYCLE_MIGRATION_CHECKSUM = createHash("sha256").update(ACCOUNTING_DIMENSION_LIFECYCLE_MIGRATION_ARTIFACT).digest("hex");
export const ACCOUNTING_DIMENSION_LIFECYCLE_MIGRATION_NAME = "rentemester-accounting-dimension-lifecycle-v30";
const SUPPLIER_COMMITMENT_OCCURRENCE_MATCHES_MIGRATION_ARTIFACT = readFileSync(join(import.meta.dir, "migrations", "0031-supplier-commitment-occurrence-matches.json"));
export const SUPPLIER_COMMITMENT_OCCURRENCE_MATCHES_MIGRATION_CHECKSUM = createHash("sha256").update(SUPPLIER_COMMITMENT_OCCURRENCE_MATCHES_MIGRATION_ARTIFACT).digest("hex");
export const SUPPLIER_COMMITMENT_OCCURRENCE_MATCHES_MIGRATION_NAME = "rentemester-supplier-commitment-occurrence-matches-v31";
const DIMENSION_BUDGET_AND_PROVENANCE_MIGRATION_ARTIFACT = readFileSync(join(import.meta.dir, "migrations", "0032-dimension-budget-and-provenance.json"));
export const DIMENSION_BUDGET_AND_PROVENANCE_MIGRATION_CHECKSUM = createHash("sha256").update(DIMENSION_BUDGET_AND_PROVENANCE_MIGRATION_ARTIFACT).digest("hex");
export const DIMENSION_BUDGET_AND_PROVENANCE_MIGRATION_NAME = "rentemester-dimension-budget-and-provenance-v32";
const BANK_RECONCILIATION_CORRECTIONS_MIGRATION_ARTIFACT = readFileSync(join(import.meta.dir, "migrations", "0033-bank-reconciliation-corrections.json"));
export const BANK_RECONCILIATION_CORRECTIONS_MIGRATION_CHECKSUM = createHash("sha256").update(BANK_RECONCILIATION_CORRECTIONS_MIGRATION_ARTIFACT).digest("hex");
export const BANK_RECONCILIATION_CORRECTIONS_MIGRATION_NAME = "rentemester-bank-reconciliation-corrections-v33";
const DIRECT_BANK_PURCHASE_PAYABLE_CORRECTIONS_MIGRATION_ARTIFACT = readFileSync(join(import.meta.dir, "migrations", "0034-direct-bank-purchase-payable-corrections.json"));
export const DIRECT_BANK_PURCHASE_PAYABLE_CORRECTIONS_MIGRATION_CHECKSUM = createHash("sha256").update(DIRECT_BANK_PURCHASE_PAYABLE_CORRECTIONS_MIGRATION_ARTIFACT).digest("hex");
export const DIRECT_BANK_PURCHASE_PAYABLE_CORRECTIONS_MIGRATION_NAME = "rentemester-direct-bank-purchase-payable-corrections-v34";
const BANK_RECONCILIATION_ACCOUNT_ROLE_FALLBACK_MIGRATION_ARTIFACT = readFileSync(join(import.meta.dir, "migrations", "0035-bank-reconciliation-account-role-fallback.json"));
export const BANK_RECONCILIATION_ACCOUNT_ROLE_FALLBACK_MIGRATION_CHECKSUM = createHash("sha256").update(BANK_RECONCILIATION_ACCOUNT_ROLE_FALLBACK_MIGRATION_ARTIFACT).digest("hex");
export const BANK_RECONCILIATION_ACCOUNT_ROLE_FALLBACK_MIGRATION_NAME = "rentemester-bank-reconciliation-account-role-fallback-v35";
const IMPORTED_RECEIVABLES_MIGRATION_ARTIFACT = readFileSync(join(import.meta.dir, "migrations", "0036-imported-receivables.json"));
export const IMPORTED_RECEIVABLES_MIGRATION_CHECKSUM = createHash("sha256").update(IMPORTED_RECEIVABLES_MIGRATION_ARTIFACT).digest("hex");
export const IMPORTED_RECEIVABLES_MIGRATION_NAME = "rentemester-imported-receivables-v36";

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

const SUPPORTED_SCHEMA_MIGRATIONS: readonly SupportedSchemaMigration[] = [
  {
    id: BASELINE_SCHEMA_VERSION,
    name: BASELINE_MIGRATION_NAME,
    checksum: BASELINE_MIGRATION_CHECKSUM,
  },
  {
    id: 2,
    name: PEPPOL_SUBMISSION_EVENTS_MIGRATION_NAME,
    checksum: PEPPOL_SUBMISSION_EVENTS_MIGRATION_CHECKSUM,
  },
  { id: 3, name: RECURRING_AUTOMATION_MIGRATION_NAME, checksum: RECURRING_AUTOMATION_MIGRATION_CHECKSUM },
  { id: 4, name: DINERO_IMPORT_PROVENANCE_MIGRATION_NAME, checksum: DINERO_IMPORT_PROVENANCE_MIGRATION_CHECKSUM },
  { id: 5, name: MIGRATION_OPEN_ITEMS_MIGRATION_NAME, checksum: MIGRATION_OPEN_ITEMS_MIGRATION_CHECKSUM },
  { id: 6, name: BANK_JOURNAL_RECONCILIATION_LINKS_MIGRATION_NAME, checksum: BANK_JOURNAL_RECONCILIATION_LINKS_MIGRATION_CHECKSUM },
  { id: 7, name: DOCUMENT_SCAN_EVIDENCE_MIGRATION_NAME, checksum: DOCUMENT_SCAN_EVIDENCE_MIGRATION_CHECKSUM },
  { id: 8, name: ISSUED_INVOICE_PDF_IMMUTABILITY_MIGRATION_NAME, checksum: ISSUED_INVOICE_PDF_IMMUTABILITY_MIGRATION_CHECKSUM },
  { id: 9, name: ACCOUNTING_DRAFT_WORKFLOW_MIGRATION_NAME, checksum: ACCOUNTING_DRAFT_WORKFLOW_MIGRATION_CHECKSUM },
  { id: 10, name: INTERNAL_VOUCHER_EVIDENCE_MIGRATION_NAME, checksum: INTERNAL_VOUCHER_EVIDENCE_MIGRATION_CHECKSUM },
  { id: 11, name: PURCHASE_VAT_PREFLIGHT_MIGRATION_NAME, checksum: PURCHASE_VAT_PREFLIGHT_MIGRATION_CHECKSUM },
  { id: 12, name: POSTING_RULES_MIGRATION_NAME, checksum: POSTING_RULES_MIGRATION_CHECKSUM },
  { id: 13, name: BOOKKEEPING_BATCHES_MIGRATION_NAME, checksum: BOOKKEEPING_BATCHES_MIGRATION_CHECKSUM },
  { id: 14, name: INVOICE_EXTRACTION_EVIDENCE_MIGRATION_NAME, checksum: INVOICE_EXTRACTION_EVIDENCE_MIGRATION_CHECKSUM },
  { id: 15, name: BOOKKEEPING_BATCH_RETRIES_MIGRATION_NAME, checksum: BOOKKEEPING_BATCH_RETRIES_MIGRATION_CHECKSUM },
  { id: 16, name: INVOICE_EXTRACTION_ACTORS_MIGRATION_NAME, checksum: INVOICE_EXTRACTION_ACTORS_MIGRATION_CHECKSUM },
  { id: 17, name: DOCUMENT_PDF_PARSES_MIGRATION_NAME, checksum: DOCUMENT_PDF_PARSES_MIGRATION_CHECKSUM },
  { id: 18, name: DOCUMENT_METADATA_ENRICHMENTS_MIGRATION_NAME, checksum: DOCUMENT_METADATA_ENRICHMENTS_MIGRATION_CHECKSUM },
  { id: 19, name: DOCUMENT_COMPANY_CONTEXTS_MIGRATION_NAME, checksum: DOCUMENT_COMPANY_CONTEXTS_MIGRATION_CHECKSUM },
  { id: 20, name: MUTATION_IDEMPOTENCY_RECEIPTS_MIGRATION_NAME, checksum: MUTATION_IDEMPOTENCY_RECEIPTS_MIGRATION_CHECKSUM },
  { id: 21, name: BOOKKEEPING_BATCH_REVISIONS_MIGRATION_NAME, checksum: BOOKKEEPING_BATCH_REVISIONS_MIGRATION_CHECKSUM },
  { id: 22, name: PERIOD_CLOSE_READINESS_MIGRATION_NAME, checksum: PERIOD_CLOSE_READINESS_MIGRATION_CHECKSUM },
  { id: 23, name: LOCAL_IDEMPOTENCY_TOMBSTONES_MIGRATION_NAME, checksum: LOCAL_IDEMPOTENCY_TOMBSTONES_MIGRATION_CHECKSUM },
  { id: 24, name: BOOKKEEPING_BATCH_PRINCIPALS_MIGRATION_NAME, checksum: BOOKKEEPING_BATCH_PRINCIPALS_MIGRATION_CHECKSUM },
  { id: 25, name: PERIOD_CLOSE_REVIEWS_MIGRATION_NAME, checksum: PERIOD_CLOSE_REVIEWS_MIGRATION_CHECKSUM },
  { id: 26, name: DOCUMENT_PARTY_LINKS_MIGRATION_NAME, checksum: DOCUMENT_PARTY_LINKS_MIGRATION_CHECKSUM },
  { id: 27, name: SUPPLIER_COMMITMENTS_MIGRATION_NAME, checksum: SUPPLIER_COMMITMENTS_MIGRATION_CHECKSUM },
  { id: 28, name: ACCOUNTING_DIMENSIONS_MIGRATION_NAME, checksum: ACCOUNTING_DIMENSIONS_MIGRATION_CHECKSUM },
  { id: 29, name: DOCUMENT_PARTY_RESOLUTION_MIGRATION_NAME, checksum: DOCUMENT_PARTY_RESOLUTION_MIGRATION_CHECKSUM },
  { id: 30, name: ACCOUNTING_DIMENSION_LIFECYCLE_MIGRATION_NAME, checksum: ACCOUNTING_DIMENSION_LIFECYCLE_MIGRATION_CHECKSUM },
  { id: 31, name: SUPPLIER_COMMITMENT_OCCURRENCE_MATCHES_MIGRATION_NAME, checksum: SUPPLIER_COMMITMENT_OCCURRENCE_MATCHES_MIGRATION_CHECKSUM },
  { id: 32, name: DIMENSION_BUDGET_AND_PROVENANCE_MIGRATION_NAME, checksum: DIMENSION_BUDGET_AND_PROVENANCE_MIGRATION_CHECKSUM },
  { id: 33, name: BANK_RECONCILIATION_CORRECTIONS_MIGRATION_NAME, checksum: BANK_RECONCILIATION_CORRECTIONS_MIGRATION_CHECKSUM },
  { id: 34, name: DIRECT_BANK_PURCHASE_PAYABLE_CORRECTIONS_MIGRATION_NAME, checksum: DIRECT_BANK_PURCHASE_PAYABLE_CORRECTIONS_MIGRATION_CHECKSUM },
  { id: 35, name: BANK_RECONCILIATION_ACCOUNT_ROLE_FALLBACK_MIGRATION_NAME, checksum: BANK_RECONCILIATION_ACCOUNT_ROLE_FALLBACK_MIGRATION_CHECKSUM },
  { id: 36, name: IMPORTED_RECEIVABLES_MIGRATION_NAME, checksum: IMPORTED_RECEIVABLES_MIGRATION_CHECKSUM },
];
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
  const migrations = [
    { id: 2, name: PEPPOL_SUBMISSION_EVENTS_MIGRATION_NAME, checksum: PEPPOL_SUBMISSION_EVENTS_MIGRATION_CHECKSUM, artifact: PEPPOL_SUBMISSION_EVENTS_MIGRATION_ARTIFACT },
    { id: 3, name: RECURRING_AUTOMATION_MIGRATION_NAME, checksum: RECURRING_AUTOMATION_MIGRATION_CHECKSUM, artifact: RECURRING_AUTOMATION_MIGRATION_ARTIFACT },
    { id: 4, name: DINERO_IMPORT_PROVENANCE_MIGRATION_NAME, checksum: DINERO_IMPORT_PROVENANCE_MIGRATION_CHECKSUM, artifact: DINERO_IMPORT_PROVENANCE_MIGRATION_ARTIFACT },
    { id: 5, name: MIGRATION_OPEN_ITEMS_MIGRATION_NAME, checksum: MIGRATION_OPEN_ITEMS_MIGRATION_CHECKSUM, artifact: MIGRATION_OPEN_ITEMS_MIGRATION_ARTIFACT },
    { id: 6, name: BANK_JOURNAL_RECONCILIATION_LINKS_MIGRATION_NAME, checksum: BANK_JOURNAL_RECONCILIATION_LINKS_MIGRATION_CHECKSUM, artifact: BANK_JOURNAL_RECONCILIATION_LINKS_MIGRATION_ARTIFACT },
    { id: 7, name: DOCUMENT_SCAN_EVIDENCE_MIGRATION_NAME, checksum: DOCUMENT_SCAN_EVIDENCE_MIGRATION_CHECKSUM, artifact: DOCUMENT_SCAN_EVIDENCE_MIGRATION_ARTIFACT },
    { id: 8, name: ISSUED_INVOICE_PDF_IMMUTABILITY_MIGRATION_NAME, checksum: ISSUED_INVOICE_PDF_IMMUTABILITY_MIGRATION_CHECKSUM, artifact: ISSUED_INVOICE_PDF_IMMUTABILITY_MIGRATION_ARTIFACT },
    { id: 9, name: ACCOUNTING_DRAFT_WORKFLOW_MIGRATION_NAME, checksum: ACCOUNTING_DRAFT_WORKFLOW_MIGRATION_CHECKSUM, artifact: ACCOUNTING_DRAFT_WORKFLOW_MIGRATION_ARTIFACT },
    { id: 10, name: INTERNAL_VOUCHER_EVIDENCE_MIGRATION_NAME, checksum: INTERNAL_VOUCHER_EVIDENCE_MIGRATION_CHECKSUM, artifact: INTERNAL_VOUCHER_EVIDENCE_MIGRATION_ARTIFACT },
    { id: 11, name: PURCHASE_VAT_PREFLIGHT_MIGRATION_NAME, checksum: PURCHASE_VAT_PREFLIGHT_MIGRATION_CHECKSUM, artifact: PURCHASE_VAT_PREFLIGHT_MIGRATION_ARTIFACT },
    { id: 12, name: POSTING_RULES_MIGRATION_NAME, checksum: POSTING_RULES_MIGRATION_CHECKSUM, artifact: POSTING_RULES_MIGRATION_ARTIFACT },
    { id: 13, name: BOOKKEEPING_BATCHES_MIGRATION_NAME, checksum: BOOKKEEPING_BATCHES_MIGRATION_CHECKSUM, artifact: BOOKKEEPING_BATCHES_MIGRATION_ARTIFACT },
    { id: 14, name: INVOICE_EXTRACTION_EVIDENCE_MIGRATION_NAME, checksum: INVOICE_EXTRACTION_EVIDENCE_MIGRATION_CHECKSUM, artifact: INVOICE_EXTRACTION_EVIDENCE_MIGRATION_ARTIFACT },
    { id: 15, name: BOOKKEEPING_BATCH_RETRIES_MIGRATION_NAME, checksum: BOOKKEEPING_BATCH_RETRIES_MIGRATION_CHECKSUM, artifact: BOOKKEEPING_BATCH_RETRIES_MIGRATION_ARTIFACT },
    { id: 16, name: INVOICE_EXTRACTION_ACTORS_MIGRATION_NAME, checksum: INVOICE_EXTRACTION_ACTORS_MIGRATION_CHECKSUM, artifact: INVOICE_EXTRACTION_ACTORS_MIGRATION_ARTIFACT },
    { id: 17, name: DOCUMENT_PDF_PARSES_MIGRATION_NAME, checksum: DOCUMENT_PDF_PARSES_MIGRATION_CHECKSUM, artifact: DOCUMENT_PDF_PARSES_MIGRATION_ARTIFACT },
    { id: 18, name: DOCUMENT_METADATA_ENRICHMENTS_MIGRATION_NAME, checksum: DOCUMENT_METADATA_ENRICHMENTS_MIGRATION_CHECKSUM, artifact: DOCUMENT_METADATA_ENRICHMENTS_MIGRATION_ARTIFACT },
    { id: 19, name: DOCUMENT_COMPANY_CONTEXTS_MIGRATION_NAME, checksum: DOCUMENT_COMPANY_CONTEXTS_MIGRATION_CHECKSUM, artifact: DOCUMENT_COMPANY_CONTEXTS_MIGRATION_ARTIFACT },
    { id: 20, name: MUTATION_IDEMPOTENCY_RECEIPTS_MIGRATION_NAME, checksum: MUTATION_IDEMPOTENCY_RECEIPTS_MIGRATION_CHECKSUM, artifact: MUTATION_IDEMPOTENCY_RECEIPTS_MIGRATION_ARTIFACT },
    { id: 21, name: BOOKKEEPING_BATCH_REVISIONS_MIGRATION_NAME, checksum: BOOKKEEPING_BATCH_REVISIONS_MIGRATION_CHECKSUM, artifact: BOOKKEEPING_BATCH_REVISIONS_MIGRATION_ARTIFACT },
    { id: 22, name: PERIOD_CLOSE_READINESS_MIGRATION_NAME, checksum: PERIOD_CLOSE_READINESS_MIGRATION_CHECKSUM, artifact: PERIOD_CLOSE_READINESS_MIGRATION_ARTIFACT },
    { id: 23, name: LOCAL_IDEMPOTENCY_TOMBSTONES_MIGRATION_NAME, checksum: LOCAL_IDEMPOTENCY_TOMBSTONES_MIGRATION_CHECKSUM, artifact: LOCAL_IDEMPOTENCY_TOMBSTONES_MIGRATION_ARTIFACT },
    { id: 24, name: BOOKKEEPING_BATCH_PRINCIPALS_MIGRATION_NAME, checksum: BOOKKEEPING_BATCH_PRINCIPALS_MIGRATION_CHECKSUM, artifact: BOOKKEEPING_BATCH_PRINCIPALS_MIGRATION_ARTIFACT },
    { id: 25, name: PERIOD_CLOSE_REVIEWS_MIGRATION_NAME, checksum: PERIOD_CLOSE_REVIEWS_MIGRATION_CHECKSUM, artifact: PERIOD_CLOSE_REVIEWS_MIGRATION_ARTIFACT },
    { id: 26, name: DOCUMENT_PARTY_LINKS_MIGRATION_NAME, checksum: DOCUMENT_PARTY_LINKS_MIGRATION_CHECKSUM, artifact: DOCUMENT_PARTY_LINKS_MIGRATION_ARTIFACT },
    { id: 27, name: SUPPLIER_COMMITMENTS_MIGRATION_NAME, checksum: SUPPLIER_COMMITMENTS_MIGRATION_CHECKSUM, artifact: SUPPLIER_COMMITMENTS_MIGRATION_ARTIFACT },
    { id: 28, name: ACCOUNTING_DIMENSIONS_MIGRATION_NAME, checksum: ACCOUNTING_DIMENSIONS_MIGRATION_CHECKSUM, artifact: ACCOUNTING_DIMENSIONS_MIGRATION_ARTIFACT },
    { id: 29, name: DOCUMENT_PARTY_RESOLUTION_MIGRATION_NAME, checksum: DOCUMENT_PARTY_RESOLUTION_MIGRATION_CHECKSUM, artifact: DOCUMENT_PARTY_RESOLUTION_MIGRATION_ARTIFACT },
    { id: 30, name: ACCOUNTING_DIMENSION_LIFECYCLE_MIGRATION_NAME, checksum: ACCOUNTING_DIMENSION_LIFECYCLE_MIGRATION_CHECKSUM, artifact: ACCOUNTING_DIMENSION_LIFECYCLE_MIGRATION_ARTIFACT },
    { id: 31, name: SUPPLIER_COMMITMENT_OCCURRENCE_MATCHES_MIGRATION_NAME, checksum: SUPPLIER_COMMITMENT_OCCURRENCE_MATCHES_MIGRATION_CHECKSUM, artifact: SUPPLIER_COMMITMENT_OCCURRENCE_MATCHES_MIGRATION_ARTIFACT },
    { id: 32, name: DIMENSION_BUDGET_AND_PROVENANCE_MIGRATION_NAME, checksum: DIMENSION_BUDGET_AND_PROVENANCE_MIGRATION_CHECKSUM, artifact: DIMENSION_BUDGET_AND_PROVENANCE_MIGRATION_ARTIFACT },
    { id: 33, name: BANK_RECONCILIATION_CORRECTIONS_MIGRATION_NAME, checksum: BANK_RECONCILIATION_CORRECTIONS_MIGRATION_CHECKSUM, artifact: BANK_RECONCILIATION_CORRECTIONS_MIGRATION_ARTIFACT },
  { id: 34, name: DIRECT_BANK_PURCHASE_PAYABLE_CORRECTIONS_MIGRATION_NAME, checksum: DIRECT_BANK_PURCHASE_PAYABLE_CORRECTIONS_MIGRATION_CHECKSUM, artifact: DIRECT_BANK_PURCHASE_PAYABLE_CORRECTIONS_MIGRATION_ARTIFACT },
  { id: 35, name: BANK_RECONCILIATION_ACCOUNT_ROLE_FALLBACK_MIGRATION_NAME, checksum: BANK_RECONCILIATION_ACCOUNT_ROLE_FALLBACK_MIGRATION_CHECKSUM, artifact: BANK_RECONCILIATION_ACCOUNT_ROLE_FALLBACK_MIGRATION_ARTIFACT },
  { id: 36, name: IMPORTED_RECEIVABLES_MIGRATION_NAME, checksum: IMPORTED_RECEIVABLES_MIGRATION_CHECKSUM, artifact: IMPORTED_RECEIVABLES_MIGRATION_ARTIFACT },
  ];
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
    const parsed = JSON.parse(DINERO_IMPORT_PROVENANCE_MIGRATION_ARTIFACT.toString("utf8")) as { sql: string };
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
    const parsed = JSON.parse(MIGRATION_OPEN_ITEMS_MIGRATION_ARTIFACT.toString("utf8")) as { sql: string };
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
    const parsed = JSON.parse(BANK_JOURNAL_RECONCILIATION_LINKS_MIGRATION_ARTIFACT.toString("utf8")) as { sql: string };
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
    const parsed = JSON.parse(ISSUED_INVOICE_PDF_IMMUTABILITY_MIGRATION_ARTIFACT.toString("utf8")) as { sql: string };
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
    const parsed = JSON.parse(ACCOUNTING_DRAFT_WORKFLOW_MIGRATION_ARTIFACT.toString("utf8")) as { sql: string };
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
    const parsed = JSON.parse(INTERNAL_VOUCHER_EVIDENCE_MIGRATION_ARTIFACT.toString("utf8")) as { sql: string };
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
    const parsed = JSON.parse(PURCHASE_VAT_PREFLIGHT_MIGRATION_ARTIFACT.toString("utf8")) as { sql: string };
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
    const parsed = JSON.parse(POSTING_RULES_MIGRATION_ARTIFACT.toString("utf8")) as { sql: string };
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
    const parsed = JSON.parse(BOOKKEEPING_BATCHES_MIGRATION_ARTIFACT.toString("utf8")) as { sql: string };
    const triggers = parsed.sql.match(/CREATE TRIGGER[\s\S]*?END;/gi) ?? [];
    db.transaction(() => { for (const statement of triggers) { const name = /CREATE TRIGGER\s+([A-Za-z_][A-Za-z0-9_]*)/i.exec(statement)?.[1]; if (name) { db.exec(`DROP TRIGGER IF EXISTS ${name};`); db.exec(statement); } } }).immediate();
  }
  if (db.query("SELECT id FROM schema_migrations WHERE id = 14").get()) {
    const parsed = JSON.parse(INVOICE_EXTRACTION_EVIDENCE_MIGRATION_ARTIFACT.toString("utf8")) as { sql: string };
    const triggers = parsed.sql.match(/CREATE TRIGGER[\s\S]*?END;/gi) ?? [];
    db.transaction(() => { for (const statement of triggers) { const name = /CREATE TRIGGER\s+([A-Za-z_][A-Za-z0-9_]*)/i.exec(statement)?.[1]; if (name) { db.exec(`DROP TRIGGER IF EXISTS ${name};`); db.exec(statement); } } }).immediate();
  }
  if (db.query("SELECT id FROM schema_migrations WHERE id = 17").get()) {
    const parsed = JSON.parse(DOCUMENT_PDF_PARSES_MIGRATION_ARTIFACT.toString("utf8")) as { sql: string };
    const triggers = parsed.sql.match(/CREATE TRIGGER[\s\S]*?END;/gi) ?? [];
    db.transaction(() => { for (const statement of triggers) { const name = /CREATE TRIGGER\s+([A-Za-z_][A-Za-z0-9_]*)/i.exec(statement)?.[1]; if (name) { db.exec(`DROP TRIGGER IF EXISTS ${name};`); db.exec(statement); } } }).immediate();
  }
  if (db.query("SELECT id FROM schema_migrations WHERE id = 18").get()) {
    const parsed = JSON.parse(DOCUMENT_METADATA_ENRICHMENTS_MIGRATION_ARTIFACT.toString("utf8")) as { sql: string };
    const triggers = parsed.sql.match(/CREATE TRIGGER[\s\S]*?END;/gi) ?? [];
    db.transaction(() => { for (const statement of triggers) { const name = /CREATE TRIGGER\s+([A-Za-z_][A-Za-z0-9_]*)/i.exec(statement)?.[1]; if (name) { db.exec(`DROP TRIGGER IF EXISTS ${name};`); db.exec(statement); } } }).immediate();
  }
  if (db.query("SELECT id FROM schema_migrations WHERE id = 19").get()) {
    const parsed = JSON.parse(DOCUMENT_COMPANY_CONTEXTS_MIGRATION_ARTIFACT.toString("utf8")) as { sql: string };
    const triggers = parsed.sql.match(/CREATE TRIGGER[\s\S]*?END;/gi) ?? [];
    db.transaction(() => { for (const statement of triggers) { const name = /CREATE TRIGGER\s+([A-Za-z_][A-Za-z0-9_]*)/i.exec(statement)?.[1]; if (name) { db.exec(`DROP TRIGGER IF EXISTS ${name};`); db.exec(statement); } } }).immediate();
  }
  if (db.query("SELECT id FROM schema_migrations WHERE id = 22").get()) {
    const parsed = JSON.parse(PERIOD_CLOSE_READINESS_MIGRATION_ARTIFACT.toString("utf8")) as { sql: string };
    const triggers = parsed.sql.match(/CREATE TRIGGER[\s\S]*?END;/gi) ?? [];
    db.transaction(() => { for (const statement of triggers) { const name = /CREATE TRIGGER\s+([A-Za-z_][A-Za-z0-9_]*)/i.exec(statement)?.[1]; if (name) { db.exec(`DROP TRIGGER IF EXISTS ${name};`); db.exec(statement); } } }).immediate();
  }
  if (db.query("SELECT id FROM schema_migrations WHERE id = 34").get()) {
    const parsed = JSON.parse(DIRECT_BANK_PURCHASE_PAYABLE_CORRECTIONS_MIGRATION_ARTIFACT.toString("utf8")) as { sql: string };
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
    const parsed = JSON.parse(BANK_RECONCILIATION_ACCOUNT_ROLE_FALLBACK_MIGRATION_ARTIFACT.toString("utf8")) as { sql: string };
    const triggers = parsed.sql.match(/CREATE TRIGGER[\s\S]*?END;/gi) ?? [];
    db.transaction(() => { for (const statement of triggers) { const name = /CREATE TRIGGER\s+([A-Za-z_][A-Za-z0-9_]*)/i.exec(statement)?.[1]; if (name) { db.exec(`DROP TRIGGER IF EXISTS ${name};`); db.exec(statement); } } }).immediate();
  }
  if (db.query("SELECT id FROM schema_migrations WHERE id = 36").get()) {
    const parsed = JSON.parse(IMPORTED_RECEIVABLES_MIGRATION_ARTIFACT.toString("utf8")) as { sql: string };
    const triggers = parsed.sql.match(/CREATE TRIGGER[\s\S]*?END;/gi) ?? [];
    db.transaction(() => { for (const statement of triggers) { const name = /CREATE TRIGGER(?: IF NOT EXISTS)?\s+([A-Za-z_][A-Za-z0-9_]*)/i.exec(statement)?.[1]; if (name) { db.exec(`DROP TRIGGER IF EXISTS ${name};`); db.exec(statement.replace("CREATE TRIGGER IF NOT EXISTS", "CREATE TRIGGER")); } } }).immediate();
  }
}
