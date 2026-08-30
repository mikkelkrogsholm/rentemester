import { Database } from "bun:sqlite";
import { describe, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { mkdtempSync, readFileSync, readdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { migrate, openDb } from "../../src/core/db";
import {
  BASELINE_MIGRATION_CHECKSUM,
  BASELINE_MIGRATION_NAME,
  BASELINE_SCHEMA_VERSION,
  CURRENT_SCHEMA_VERSION,
  PEPPOL_SUBMISSION_EVENTS_MIGRATION_CHECKSUM,
  PEPPOL_SUBMISSION_EVENTS_MIGRATION_NAME,
  RECURRING_AUTOMATION_MIGRATION_CHECKSUM,
  RECURRING_AUTOMATION_MIGRATION_NAME,
  DINERO_IMPORT_PROVENANCE_MIGRATION_CHECKSUM,
  DINERO_IMPORT_PROVENANCE_MIGRATION_NAME,
  MIGRATION_OPEN_ITEMS_MIGRATION_CHECKSUM,
  MIGRATION_OPEN_ITEMS_MIGRATION_NAME,
  BANK_JOURNAL_RECONCILIATION_LINKS_MIGRATION_CHECKSUM,
  BANK_JOURNAL_RECONCILIATION_LINKS_MIGRATION_NAME,
  DOCUMENT_SCAN_EVIDENCE_MIGRATION_CHECKSUM,
  DOCUMENT_SCAN_EVIDENCE_MIGRATION_NAME,
  ISSUED_INVOICE_PDF_IMMUTABILITY_MIGRATION_CHECKSUM,
  ISSUED_INVOICE_PDF_IMMUTABILITY_MIGRATION_NAME,
  ACCOUNTING_DRAFT_WORKFLOW_MIGRATION_CHECKSUM,
  ACCOUNTING_DRAFT_WORKFLOW_MIGRATION_NAME,
  INTERNAL_VOUCHER_EVIDENCE_MIGRATION_CHECKSUM,
  INTERNAL_VOUCHER_EVIDENCE_MIGRATION_NAME,
  PURCHASE_VAT_PREFLIGHT_MIGRATION_CHECKSUM,
  PURCHASE_VAT_PREFLIGHT_MIGRATION_NAME,
  POSTING_RULES_MIGRATION_CHECKSUM,
  POSTING_RULES_MIGRATION_NAME,
  BOOKKEEPING_BATCHES_MIGRATION_CHECKSUM,
  BOOKKEEPING_BATCHES_MIGRATION_NAME,
  INVOICE_EXTRACTION_EVIDENCE_MIGRATION_CHECKSUM,
  INVOICE_EXTRACTION_EVIDENCE_MIGRATION_NAME,
  BOOKKEEPING_BATCH_RETRIES_MIGRATION_CHECKSUM,
  BOOKKEEPING_BATCH_RETRIES_MIGRATION_NAME,
  INVOICE_EXTRACTION_ACTORS_MIGRATION_CHECKSUM,
  INVOICE_EXTRACTION_ACTORS_MIGRATION_NAME,
  DOCUMENT_PDF_PARSES_MIGRATION_CHECKSUM,
  DOCUMENT_PDF_PARSES_MIGRATION_NAME,
  DOCUMENT_METADATA_ENRICHMENTS_MIGRATION_CHECKSUM,
  DOCUMENT_METADATA_ENRICHMENTS_MIGRATION_NAME,
  DOCUMENT_COMPANY_CONTEXTS_MIGRATION_CHECKSUM,
  DOCUMENT_COMPANY_CONTEXTS_MIGRATION_NAME,
  MUTATION_IDEMPOTENCY_RECEIPTS_MIGRATION_CHECKSUM,
  MUTATION_IDEMPOTENCY_RECEIPTS_MIGRATION_NAME,
  BOOKKEEPING_BATCH_REVISIONS_MIGRATION_CHECKSUM,
  BOOKKEEPING_BATCH_REVISIONS_MIGRATION_NAME,
  PERIOD_CLOSE_READINESS_MIGRATION_CHECKSUM,
  PERIOD_CLOSE_READINESS_MIGRATION_NAME,
  LOCAL_IDEMPOTENCY_TOMBSTONES_MIGRATION_CHECKSUM,
  LOCAL_IDEMPOTENCY_TOMBSTONES_MIGRATION_NAME,
  BOOKKEEPING_BATCH_PRINCIPALS_MIGRATION_CHECKSUM,
  BOOKKEEPING_BATCH_PRINCIPALS_MIGRATION_NAME,
  PERIOD_CLOSE_REVIEWS_MIGRATION_CHECKSUM,
  PERIOD_CLOSE_REVIEWS_MIGRATION_NAME,
  DOCUMENT_PARTY_LINKS_MIGRATION_CHECKSUM,
  DOCUMENT_PARTY_LINKS_MIGRATION_NAME,
  SUPPLIER_COMMITMENTS_MIGRATION_CHECKSUM,
  SUPPLIER_COMMITMENTS_MIGRATION_NAME,
  ACCOUNTING_DIMENSIONS_MIGRATION_CHECKSUM,
  ACCOUNTING_DIMENSIONS_MIGRATION_NAME,
  DOCUMENT_PARTY_RESOLUTION_MIGRATION_CHECKSUM,
  DOCUMENT_PARTY_RESOLUTION_MIGRATION_NAME,
  ACCOUNTING_DIMENSION_LIFECYCLE_MIGRATION_CHECKSUM,
  ACCOUNTING_DIMENSION_LIFECYCLE_MIGRATION_NAME,
  SUPPLIER_COMMITMENT_OCCURRENCE_MATCHES_MIGRATION_CHECKSUM,
  SUPPLIER_COMMITMENT_OCCURRENCE_MATCHES_MIGRATION_NAME,
  DIMENSION_BUDGET_AND_PROVENANCE_MIGRATION_CHECKSUM,
  DIMENSION_BUDGET_AND_PROVENANCE_MIGRATION_NAME,
  BANK_RECONCILIATION_CORRECTIONS_MIGRATION_CHECKSUM,
  BANK_RECONCILIATION_CORRECTIONS_MIGRATION_NAME,
  DIRECT_BANK_PURCHASE_PAYABLE_CORRECTIONS_MIGRATION_CHECKSUM,
  DIRECT_BANK_PURCHASE_PAYABLE_CORRECTIONS_MIGRATION_NAME,
  BANK_RECONCILIATION_ACCOUNT_ROLE_FALLBACK_MIGRATION_CHECKSUM,
  BANK_RECONCILIATION_ACCOUNT_ROLE_FALLBACK_MIGRATION_NAME,
  IMPORTED_RECEIVABLES_MIGRATION_CHECKSUM,
  IMPORTED_RECEIVABLES_MIGRATION_NAME,
  readSchemaMigrations,
  validateSchemaMigrationHistory,
} from "../../src/core/schema-version";

function seedV2RecurringLedger(db: Database) {
  db.exec(`
    CREATE TABLE schema_migrations (id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE, checksum TEXT NOT NULL, applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP, applied_by_version TEXT NOT NULL, applied_by_commit TEXT);
    INSERT INTO schema_migrations (id,name,checksum,applied_by_version) VALUES
      (1, '${BASELINE_MIGRATION_NAME}', '${BASELINE_MIGRATION_CHECKSUM}', '0.1.0'),
      (2, '${PEPPOL_SUBMISSION_EVENTS_MIGRATION_NAME}', '${PEPPOL_SUBMISSION_EVENTS_MIGRATION_CHECKSUM}', '0.1.0');
    CREATE TABLE recurring_invoice_templates (id INTEGER PRIMARY KEY, name TEXT NOT NULL, interval TEXT NOT NULL CHECK(interval IN ('monthly','quarterly','yearly')), first_issue_date TEXT NOT NULL, next_issue_date TEXT NOT NULL, payment_terms_days INTEGER NOT NULL DEFAULT 30, delivery_period_mode TEXT NOT NULL DEFAULT 'issue_month', payload_json TEXT NOT NULL, notes TEXT, active INTEGER NOT NULL DEFAULT 1, created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);
    CREATE TABLE recurring_invoice_generations (id INTEGER PRIMARY KEY, template_id INTEGER NOT NULL REFERENCES recurring_invoice_templates(id), period_index INTEGER NOT NULL, document_id INTEGER NOT NULL REFERENCES documents(id), invoice_number TEXT NOT NULL, issue_date TEXT NOT NULL, delivery_period_start TEXT, delivery_period_end TEXT, created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP, UNIQUE(template_id, period_index));
  `);
  // The full schema supplies the historic documents table and all unrelated
  // baseline tables; IF NOT EXISTS intentionally preserves the v2 templates.
  db.exec(readFileSync(join(import.meta.dir, "..", "..", "src", "core", "schema.sql"), "utf8"));
  db.exec(`
    INSERT INTO documents (id, source, sha256_hash) VALUES (41, 'rentemester', 'v2-generation-document');
    INSERT INTO recurring_invoice_templates (id,name,interval,first_issue_date,next_issue_date,payment_terms_days,delivery_period_mode,payload_json,active) VALUES (7,'Historic','monthly','2026-01-31','2026-02-28',30,'issue_month','{}',1);
    INSERT INTO recurring_invoice_generations (id,template_id,period_index,document_id,invoice_number,issue_date) VALUES (9,7,0,41,'2026-0001','2026-01-31');
  `);
}

describe("schema version compatibility", () => {
  test("migrates a v2 recurring template and generation losslessly with foreign keys", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-v2-recurring-"));
    const path = join(root, "ledger.sqlite");
    try {
      const legacy = new Database(path);
      legacy.exec("PRAGMA foreign_keys = ON");
      seedV2RecurringLedger(legacy);
      legacy.close();

      const db = openDb(path);
      migrate(db);
      expect(db.query("PRAGMA foreign_keys").get()).toEqual({ foreign_keys: 1 });
      expect(db.query("SELECT id, interval_count, delivery_channel FROM recurring_invoice_templates WHERE id = 7").get())
        .toEqual({ id: 7, interval_count: 1, delivery_channel: "manual" });
      expect(db.query("SELECT id, template_id, document_id FROM recurring_invoice_generations WHERE id = 9").get())
        .toEqual({ id: 9, template_id: 7, document_id: 41 });
      expect(db.query("PRAGMA foreign_key_check").all()).toEqual([]);
      expect(() => db.run("UPDATE recurring_invoice_generations SET invoice_number = 'x' WHERE id = 9")).toThrow("append-only");
      migrate(db);
      expect(() => db.run("UPDATE recurring_invoice_templates SET interval_count = 2 WHERE id = 7")).toThrow("append-only");
      expect(() => db.run("UPDATE recurring_invoice_templates SET delivery_channel = 'email' WHERE id = 7")).toThrow("append-only");
      expect(readSchemaMigrations(db)).toHaveLength(CURRENT_SCHEMA_VERSION);
      db.close();
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
  test("binds the immutable baseline artifact to schema and normalization bytes", () => {
    const core = join(import.meta.dir, "..", "..", "src", "core");
    const artifactPath = join(core, "migrations", "0001-baseline.json");
    const artifact = JSON.parse(readFileSync(artifactPath, "utf8")) as {
      schemaSha256: string;
      normalizationSha256: string;
    };
    const source = readFileSync(join(core, "db.ts"), "utf8");
    const start = "  // BASELINE_MIGRATION_V1_NORMALIZATION_START\n";
    const end = "  // BASELINE_MIGRATION_V1_NORMALIZATION_END\n";
    const normalization = source.split(start)[1]?.split(end)[0];
    expect(normalization).toBeDefined();

    expect(artifact.schemaSha256).toBe(
      createHash("sha256").update(readFileSync(join(core, "schema.sql"))).digest("hex"),
    );
    expect(artifact.normalizationSha256).toBe(
      createHash("sha256").update(normalization!, "utf8").digest("hex"),
    );
    expect(BASELINE_MIGRATION_CHECKSUM).toBe(
      createHash("sha256").update(readFileSync(artifactPath)).digest("hex"),
    );
  });

  test("records the contiguous checksummed migration catalog idempotently", () => {
    const db = new Database(":memory:");
    migrate(db);
    migrate(db);

    expect(readSchemaMigrations(db)).toEqual([
      expect.objectContaining({
        id: BASELINE_SCHEMA_VERSION,
        name: BASELINE_MIGRATION_NAME,
        checksum: BASELINE_MIGRATION_CHECKSUM,
        applied_by_version: "0.2.0",
      }),
      expect.objectContaining({ id: 2, name: PEPPOL_SUBMISSION_EVENTS_MIGRATION_NAME, checksum: PEPPOL_SUBMISSION_EVENTS_MIGRATION_CHECKSUM }),
      expect.objectContaining({ id: 3, name: RECURRING_AUTOMATION_MIGRATION_NAME, checksum: RECURRING_AUTOMATION_MIGRATION_CHECKSUM }),
      expect.objectContaining({ id: 4, name: DINERO_IMPORT_PROVENANCE_MIGRATION_NAME, checksum: DINERO_IMPORT_PROVENANCE_MIGRATION_CHECKSUM }),
      expect.objectContaining({ id: 5, name: MIGRATION_OPEN_ITEMS_MIGRATION_NAME, checksum: MIGRATION_OPEN_ITEMS_MIGRATION_CHECKSUM }),
      expect.objectContaining({ id: 6, name: BANK_JOURNAL_RECONCILIATION_LINKS_MIGRATION_NAME, checksum: BANK_JOURNAL_RECONCILIATION_LINKS_MIGRATION_CHECKSUM }),
      expect.objectContaining({ id: 7, name: DOCUMENT_SCAN_EVIDENCE_MIGRATION_NAME, checksum: DOCUMENT_SCAN_EVIDENCE_MIGRATION_CHECKSUM }),
      expect.objectContaining({ id: 8, name: ISSUED_INVOICE_PDF_IMMUTABILITY_MIGRATION_NAME, checksum: ISSUED_INVOICE_PDF_IMMUTABILITY_MIGRATION_CHECKSUM }),
      expect.objectContaining({ id: 9, name: ACCOUNTING_DRAFT_WORKFLOW_MIGRATION_NAME, checksum: ACCOUNTING_DRAFT_WORKFLOW_MIGRATION_CHECKSUM }),
      expect.objectContaining({ id: 10, name: INTERNAL_VOUCHER_EVIDENCE_MIGRATION_NAME, checksum: INTERNAL_VOUCHER_EVIDENCE_MIGRATION_CHECKSUM }),
      expect.objectContaining({ id: 11, name: PURCHASE_VAT_PREFLIGHT_MIGRATION_NAME, checksum: PURCHASE_VAT_PREFLIGHT_MIGRATION_CHECKSUM }),
      expect.objectContaining({ id: 12, name: POSTING_RULES_MIGRATION_NAME, checksum: POSTING_RULES_MIGRATION_CHECKSUM }),
      expect.objectContaining({ id: 13, name: BOOKKEEPING_BATCHES_MIGRATION_NAME, checksum: BOOKKEEPING_BATCHES_MIGRATION_CHECKSUM }),
      expect.objectContaining({ id: 14, name: INVOICE_EXTRACTION_EVIDENCE_MIGRATION_NAME, checksum: INVOICE_EXTRACTION_EVIDENCE_MIGRATION_CHECKSUM }),
      expect.objectContaining({ id: 15, name: BOOKKEEPING_BATCH_RETRIES_MIGRATION_NAME, checksum: BOOKKEEPING_BATCH_RETRIES_MIGRATION_CHECKSUM }),
      expect.objectContaining({ id: 16, name: INVOICE_EXTRACTION_ACTORS_MIGRATION_NAME, checksum: INVOICE_EXTRACTION_ACTORS_MIGRATION_CHECKSUM }),
      expect.objectContaining({ id: 17, name: DOCUMENT_PDF_PARSES_MIGRATION_NAME, checksum: DOCUMENT_PDF_PARSES_MIGRATION_CHECKSUM }),
      expect.objectContaining({ id: 18, name: DOCUMENT_METADATA_ENRICHMENTS_MIGRATION_NAME, checksum: DOCUMENT_METADATA_ENRICHMENTS_MIGRATION_CHECKSUM }),
      expect.objectContaining({ id: 19, name: DOCUMENT_COMPANY_CONTEXTS_MIGRATION_NAME, checksum: DOCUMENT_COMPANY_CONTEXTS_MIGRATION_CHECKSUM }),
      expect.objectContaining({ id: 20, name: MUTATION_IDEMPOTENCY_RECEIPTS_MIGRATION_NAME, checksum: MUTATION_IDEMPOTENCY_RECEIPTS_MIGRATION_CHECKSUM }),
      expect.objectContaining({ id: 21, name: BOOKKEEPING_BATCH_REVISIONS_MIGRATION_NAME, checksum: BOOKKEEPING_BATCH_REVISIONS_MIGRATION_CHECKSUM }),
      expect.objectContaining({ id: 22, name: PERIOD_CLOSE_READINESS_MIGRATION_NAME, checksum: PERIOD_CLOSE_READINESS_MIGRATION_CHECKSUM }),
      expect.objectContaining({ id: 23, name: LOCAL_IDEMPOTENCY_TOMBSTONES_MIGRATION_NAME, checksum: LOCAL_IDEMPOTENCY_TOMBSTONES_MIGRATION_CHECKSUM }),
      expect.objectContaining({ id: 24, name: BOOKKEEPING_BATCH_PRINCIPALS_MIGRATION_NAME, checksum: BOOKKEEPING_BATCH_PRINCIPALS_MIGRATION_CHECKSUM }),
      expect.objectContaining({ id: 25, name: PERIOD_CLOSE_REVIEWS_MIGRATION_NAME, checksum: PERIOD_CLOSE_REVIEWS_MIGRATION_CHECKSUM }),
      expect.objectContaining({ id: 26, name: DOCUMENT_PARTY_LINKS_MIGRATION_NAME, checksum: DOCUMENT_PARTY_LINKS_MIGRATION_CHECKSUM }),
      expect.objectContaining({ id: 27, name: SUPPLIER_COMMITMENTS_MIGRATION_NAME, checksum: SUPPLIER_COMMITMENTS_MIGRATION_CHECKSUM }),
      expect.objectContaining({ id: 28, name: ACCOUNTING_DIMENSIONS_MIGRATION_NAME, checksum: ACCOUNTING_DIMENSIONS_MIGRATION_CHECKSUM }),
      expect.objectContaining({ id: 29, name: DOCUMENT_PARTY_RESOLUTION_MIGRATION_NAME, checksum: DOCUMENT_PARTY_RESOLUTION_MIGRATION_CHECKSUM }),
      expect.objectContaining({ id: 30, name: ACCOUNTING_DIMENSION_LIFECYCLE_MIGRATION_NAME, checksum: ACCOUNTING_DIMENSION_LIFECYCLE_MIGRATION_CHECKSUM }),
      expect.objectContaining({ id: 31, name: SUPPLIER_COMMITMENT_OCCURRENCE_MATCHES_MIGRATION_NAME, checksum: SUPPLIER_COMMITMENT_OCCURRENCE_MATCHES_MIGRATION_CHECKSUM }),
      expect.objectContaining({ id: 32, name: DIMENSION_BUDGET_AND_PROVENANCE_MIGRATION_NAME, checksum: DIMENSION_BUDGET_AND_PROVENANCE_MIGRATION_CHECKSUM }),
      expect.objectContaining({ id: 33, name: BANK_RECONCILIATION_CORRECTIONS_MIGRATION_NAME, checksum: BANK_RECONCILIATION_CORRECTIONS_MIGRATION_CHECKSUM }),
      expect.objectContaining({ id: 34, name: DIRECT_BANK_PURCHASE_PAYABLE_CORRECTIONS_MIGRATION_NAME, checksum: DIRECT_BANK_PURCHASE_PAYABLE_CORRECTIONS_MIGRATION_CHECKSUM }),
      expect.objectContaining({ id: 35, name: BANK_RECONCILIATION_ACCOUNT_ROLE_FALLBACK_MIGRATION_NAME, checksum: BANK_RECONCILIATION_ACCOUNT_ROLE_FALLBACK_MIGRATION_CHECKSUM }),
      expect.objectContaining({ id: 36, name: IMPORTED_RECEIVABLES_MIGRATION_NAME, checksum: IMPORTED_RECEIVABLES_MIGRATION_CHECKSUM }),
    ]);
    db.close();
  });

  test("makes issued invoice PDF evidence append-only and reasserts its guards", () => {
    const db = new Database(":memory:");
    migrate(db);
    db.run("INSERT INTO documents (source, sha256_hash, document_type, invoice_no) VALUES ('test', 'pdf-evidence', 'issued_invoice_pdf', '2026-0001')");
    const row = db.query("SELECT id FROM documents WHERE document_type = 'issued_invoice_pdf'").get() as { id: number };
    expect(() => db.run("UPDATE documents SET sha256_hash = 'changed' WHERE id = ?", row.id)).toThrow("immutable");
    expect(() => db.run("DELETE FROM documents WHERE id = ?", row.id)).toThrow("immutable");
    db.exec("DROP TRIGGER issued_invoice_pdf_no_update; DROP TRIGGER issued_invoice_pdf_no_delete;");
    migrate(db);
    expect(() => db.run("UPDATE documents SET sha256_hash = 'changed' WHERE id = ?", row.id)).toThrow("immutable");
    db.close();
  });

  test("makes document metadata enrichments append-only and restores guards", () => {
    const db = new Database(":memory:");
    migrate(db);
    db.run("INSERT INTO documents (id, source, sha256_hash) VALUES (1, 'test', 'metadata-enrichment')");
    db.run(`INSERT INTO document_metadata_enrichments
      (document_id, enriched_metadata_json, enriched_metadata_sha256, actor, program)
      VALUES (1, '{}', ?, 'agent:test', 'bun:test')`, "a".repeat(64));
    db.exec("DROP TRIGGER document_metadata_enrichments_no_update; DROP TRIGGER document_metadata_enrichments_no_delete;");
    migrate(db);
    expect(() => db.run("UPDATE document_metadata_enrichments SET actor = 'agent:changed' WHERE document_id = 1")).toThrow("append-only");
    expect(() => db.run("DELETE FROM document_metadata_enrichments WHERE document_id = 1")).toThrow("append-only");
    db.close();
  });

  test("reasserts accounting-draft append-only guards on every open", () => {
    const db = new Database(":memory:");
    migrate(db);
    db.run(
      `INSERT INTO accounting_draft_events
       (id,draft_id,version,event_type,payload_hash,canonical_payload,actor_id,actor_program,event_hash,created_at)
       VALUES (1,'synthetic-draft',1,'created',?,?,'agent:test','unit-test',?,'2026-08-23T00:00:00.000Z')`,
      "a".repeat(64),
      "{}",
      "b".repeat(64),
    );
    db.exec("DROP TRIGGER accounting_draft_events_no_update; DROP TRIGGER accounting_draft_events_no_delete;");
    migrate(db);
    expect(() => db.run("UPDATE accounting_draft_events SET actor_id = 'agent:changed' WHERE id = 1")).toThrow("append-only");
    expect(() => db.run("DELETE FROM accounting_draft_events WHERE id = 1")).toThrow("append-only");
    db.close();
  });

  test("reasserts internal-voucher evidence guards on every open", () => {
    const db = new Database(":memory:");
    migrate(db);
    db.run(
      "INSERT INTO bank_transactions (id, transaction_date, text, amount, transaction_hash) VALUES (1, '2026-07-31', 'Fee', -10, 'stable')",
    );
    db.run(
      "INSERT INTO documents (id, source, sha256_hash, document_type) VALUES (1, 'test', 'voucher', 'internal_voucher')",
    );
    db.run(
      `INSERT INTO internal_voucher_evidence
         (document_id, bank_transaction_id, accounting_rationale, prepared_by, prepared_by_program)
       VALUES (1, 1, 'Synthetic evidence', 'agent:test', 'bun:test')`,
    );
    db.exec(
      "DROP TRIGGER internal_voucher_evidence_no_update; DROP TRIGGER internal_voucher_document_no_update; DROP TRIGGER internal_voucher_bank_no_update;",
    );
    migrate(db);
    expect(() => db.run(
      "UPDATE internal_voucher_evidence SET accounting_rationale = 'changed' WHERE document_id = 1",
    )).toThrow("append-only");
    expect(() => db.run(
      "UPDATE documents SET source = 'changed' WHERE id = 1",
    )).toThrow("append-only");
    expect(() => db.run(
      "UPDATE bank_transactions SET amount = -11 WHERE id = 1",
    )).toThrow("append-only");
    db.close();
  });

  test("opens a pre-change v1 ledger and applies the append-only v2 events migration", () => {
    const db = new Database(":memory:");
    migrate(db);
    db.exec("DROP TABLE peppol_submission_events; DROP TABLE recurring_invoice_delivery_events; DELETE FROM schema_migrations WHERE id >= 2;");
    expect(readSchemaMigrations(db)).toHaveLength(1);
    migrate(db);
    expect(db.query("SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'peppol_submission_events'").get()).not.toBeNull();
    expect(readSchemaMigrations(db)).toHaveLength(CURRENT_SCHEMA_VERSION);
    db.close();
  });

  test("upgrades an isolated v3 schema through the current version without touching a live ledger", () => {
    const db = new Database(":memory:");
    db.exec("PRAGMA foreign_keys = ON");
    migrate(db);
    for (const table of ["migration_open_item_batches", "migration_open_items", "migration_open_item_applications"]) {
      db.exec(`DROP TRIGGER ${table}_no_update; DROP TRIGGER ${table}_no_delete;`);
    }
    db.exec(`DROP TABLE migration_open_item_applications; DROP TABLE migration_open_items; DROP TABLE migration_open_item_batches;`);
    for (const table of ["sources", "inventories", "inventory_entries", "attempts", "archive_evidence", "document_links"]) {
      db.exec(`DROP TRIGGER dinero_import_${table}_no_update; DROP TRIGGER dinero_import_${table}_no_delete;`);
    }
    db.exec(`
      DROP TABLE dinero_import_document_links;
      DROP TABLE dinero_import_archive_evidence;
      DROP TABLE dinero_import_attempts;
      DROP TABLE dinero_import_inventory_entries;
      DROP TABLE dinero_import_inventories;
      DROP TABLE dinero_import_sources;
      DROP INDEX idx_documents_id_sha256_hash;
      DELETE FROM schema_migrations WHERE id >= 4;
    `);
    expect(readSchemaMigrations(db)).toHaveLength(3);
    migrate(db);
    expect(readSchemaMigrations(db)).toHaveLength(CURRENT_SCHEMA_VERSION);
    expect(db.query("SELECT 1 FROM sqlite_master WHERE type = 'index' AND name = 'idx_documents_id_sha256_hash'").get()).not.toBeNull();
    expect(db.query("PRAGMA foreign_key_check").all()).toEqual([]);
    db.close();
  });

  test("accepts a complete append-only history when a future runtime supports it", () => {
    const supported = [
      {
        id: BASELINE_SCHEMA_VERSION,
        name: BASELINE_MIGRATION_NAME,
        checksum: BASELINE_MIGRATION_CHECKSUM,
      },
      { id: 2, name: "rentemester-schema-v2", checksum: "v2-checksum" },
    ];
    const applied = [
      {
        id: BASELINE_SCHEMA_VERSION,
        name: BASELINE_MIGRATION_NAME,
        checksum: BASELINE_MIGRATION_CHECKSUM,
      },
      { id: 2, name: "rentemester-schema-v2", checksum: "v2-checksum" },
    ];

    expect(() => validateSchemaMigrationHistory(applied, supported)).not.toThrow();
    expect(() => validateSchemaMigrationHistory(applied.slice(1), supported)).toThrow(
      "complete append-only prefix",
    );
  });

  test("rejects a database made by newer software before creating business tables", () => {
    const db = new Database(":memory:");
    db.exec(`
      CREATE TABLE schema_migrations (
        id INTEGER PRIMARY KEY,
        name TEXT NOT NULL UNIQUE,
        checksum TEXT NOT NULL,
        applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
        applied_by_version TEXT NOT NULL,
        applied_by_commit TEXT
      );
      INSERT INTO schema_migrations
        (id, name, checksum, applied_by_version)
      VALUES (${CURRENT_SCHEMA_VERSION + 1}, 'future', 'abc', '0.2.0');
    `);

    expect(() => migrate(db)).toThrow(`newer than supported version ${CURRENT_SCHEMA_VERSION}`);
    expect(
      db.query("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'companies'").get(),
    ).toBeNull();
    db.close();
  });

  test("rejects a modified baseline checksum", () => {
    const db = new Database(":memory:");
    migrate(db);
    db.query("UPDATE schema_migrations SET checksum = 'modified' WHERE id = 1").run();
    expect(() => migrate(db)).toThrow("checksum mismatch");
    db.close();
  });

  test("adopts a checksum only from a legacy table that never had the column", () => {
    const db = new Database(":memory:");
    db.exec(`
      CREATE TABLE schema_migrations (
        id INTEGER PRIMARY KEY,
        name TEXT NOT NULL UNIQUE,
        applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
      );
      INSERT INTO schema_migrations (id, name)
      VALUES (1, '${BASELINE_MIGRATION_NAME}');
    `);

    migrate(db);
    expect(readSchemaMigrations(db)[0]?.checksum).toBe(BASELINE_MIGRATION_CHECKSUM);
    const columns = db.query("PRAGMA table_info(schema_migrations)").all() as Array<{
      name: string;
      notnull: number;
    }>;
    expect(columns.find((column) => column.name === "checksum")?.notnull).toBe(1);
    expect(columns.find((column) => column.name === "applied_by_version")?.notnull).toBe(1);
    db.close();
  });

  test("rejects a null checksum once the checksum column exists", () => {
    const db = new Database(":memory:");
    db.exec(`
      CREATE TABLE schema_migrations (
        id INTEGER PRIMARY KEY,
        name TEXT NOT NULL UNIQUE,
        checksum TEXT,
        applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
        applied_by_version TEXT
      );
      INSERT INTO schema_migrations (id, name, checksum, applied_by_version)
      VALUES (1, '${BASELINE_MIGRATION_NAME}', NULL, '0.1.0');
    `);

    expect(() => migrate(db)).toThrow("missing checksum");
    expect(
      db.query("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'companies'").get(),
    ).toBeNull();
    db.close();
  });

  test("openDb rejects a newer on-disk database without changing its bytes or sidecars", () => {
    const directory = mkdtempSync(join(tmpdir(), "rentemester-schema-preflight-"));
    const path = join(directory, "ledger.sqlite");
    try {
      const db = new Database(path);
      db.exec(`
        CREATE TABLE schema_migrations (
          id INTEGER PRIMARY KEY,
          name TEXT NOT NULL UNIQUE,
          checksum TEXT NOT NULL,
          applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
          applied_by_version TEXT NOT NULL,
          applied_by_commit TEXT
        );
        INSERT INTO schema_migrations
          (id, name, checksum, applied_by_version)
        VALUES (${CURRENT_SCHEMA_VERSION + 1}, 'future', 'abc', '0.2.0');
      `);
      db.close();
      const beforeBytes = readFileSync(path);
      const beforeFiles = readdirSync(directory).sort();

      expect(() => openDb(path)).toThrow(`newer than supported version ${CURRENT_SCHEMA_VERSION}`);
      expect(readFileSync(path)).toEqual(beforeBytes);
      expect(readdirSync(directory).sort()).toEqual(beforeFiles);
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  });
});
