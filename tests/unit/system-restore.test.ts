// Tests: src/core/system-restore.ts
import { describe, expect, test } from "bun:test";
import { Database } from "bun:sqlite";
import { copyFileSync, existsSync, mkdtempSync, mkdirSync, readdirSync, readFileSync, renameSync, rmSync, statSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { createHash, createHmac } from "node:crypto";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { seedAccounts, postJournalEntry, verifyAuditChain } from "../../src/core/ledger";
import { ingestDocument } from "../../src/core/documents";
import { backupManifestKeyPath, createSystemBackup } from "../../src/core/system-backups";
import { restoreSystemBackup } from "../../src/core/system-restore";
import { validateInvoiceJournalEvidence } from "../../src/core/invoice-journal-evidence";
import { BASELINE_MIGRATION_CHECKSUM, CURRENT_SCHEMA_VERSION, readSchemaMigrations, supportedSchemaMigrations } from "../../src/core/schema-version";
import { buildBankReconciliationReport } from "../../src/core/reconciliation";
import { buildTrialBalance } from "../../src/core/financial-statements";
import { buildVatReport } from "../../src/core/vat";

function sha256File(path: string) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

function rewriteSignedManifest(companyRoot: string, backupDir: string, manifest: Record<string, any>) {
  const manifestPath = join(backupDir, "manifest.json");
  const signaturePath = join(backupDir, "manifest.json.hmac");
  const keyHex = readFileSync(backupManifestKeyPath(companyRoot), "utf8").trim();
  const manifestText = `${JSON.stringify(manifest, null, 2)}\n`;
  const signature = createHmac("sha256", Buffer.from(keyHex, "hex")).update(manifestText).digest("hex");
  writeFileSync(manifestPath, manifestText);
  writeFileSync(signaturePath, `${signature}\n`);
}

const preV1InvoiceApplicationsFixture = readFileSync(
  join(process.cwd(), "tests", "fixtures", "legacy-pre-v1", "invoice-application-tables.sql"),
  "utf8",
);

function rewriteSnapshotAsPreV1(snapshotPath: string) {
  const snapshot = new Database(snapshotPath);
  try {
    snapshot.exec("PRAGMA foreign_keys = OFF;");
    snapshot.exec("DROP TABLE invoice_payments; DROP TABLE invoice_refunds; DROP TABLE invoice_claim_payments;");
    snapshot.exec("DROP TABLE schema_migrations;");
    snapshot.exec(preV1InvoiceApplicationsFixture);
  } finally {
    snapshot.close();
  }
}

function signChangedSnapshot(companyRoot: string, backupDir: string) {
  const manifestPath = join(backupDir, "manifest.json");
  const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
  const snapshotPath = join(backupDir, manifest.dbSnapshot.path);
  manifest.dbSnapshot.sha256 = sha256File(snapshotPath);
  manifest.dbSnapshot.sizeBytes = statSync(snapshotPath).size;
  rewriteSignedManifest(companyRoot, backupDir, manifest);
  return { manifest, snapshotPath };
}

function restoreStagingEntries(root: string) {
  return readdirSync(root).filter((entry) => entry.startsWith(".restore-"));
}

const provenanceHash = (letter: string) => letter.repeat(64);

function seedV4Provenance(db: Database) {
  db.query("INSERT INTO documents (id, source, sha256_hash) VALUES (1, 'test', ?)").run(provenanceHash("c"));
  db.query("INSERT INTO dinero_import_sources (id, raw_sha256, raw_size_bytes, canonical_listing_sha256, canonical_listing_count) VALUES (1, ?, 9, ?, 1)").run(provenanceHash("a"), provenanceHash("b"));
  db.query("INSERT INTO dinero_import_inventories (id, source_id, source_raw_sha256, canonical_listing_sha256, canonical_listing_count, entry_count, total_size_bytes) VALUES (1, 1, ?, ?, 1, 1, 9)").run(provenanceHash("a"), provenanceHash("b"));
  db.query("INSERT INTO dinero_import_inventory_entries (inventory_id, entry_path, entry_size_bytes, entry_sha256) VALUES (1, 'docs/a.pdf', 9, ?)").run(provenanceHash("c"));
  db.query("INSERT INTO dinero_import_attempts (id, inventory_id, source_id, source_raw_sha256, parser_contract, actor, cutover_date, outcome, result_sha256) VALUES (1, 1, 1, ?, 'v1', 'agent:test', '2025-01-01', 'accepted', ?)").run(provenanceHash("a"), provenanceHash("e"));
  db.query("INSERT INTO dinero_import_document_links (attempt_id, inventory_id, entry_path, entry_sha256, document_id, disposition) VALUES (1, 1, 'docs/a.pdf', ?, 1, 'linked')").run(provenanceHash("c"));
}

describe("system restore", () => {
  test("migrates a signed genuine pre-v1 snapshot in staging before validation and stamping", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-restore-pre-v1-"));
    const companyRoot = join(root, "company");
    const restoredRoot = join(root, "restored-company");
    const paths = ensureCompanyDirs(companyRoot);
    const db = openDb(paths.db);
    migrate(db);
    const backup = createSystemBackup(db, companyRoot, { createdAt: "2026-05-17T02:39:00.000Z" });
    db.close();
    expect(backup.ok).toBe(true);

    const snapshotPath = join(backup.backupDir!, "ledger.sqlite");
    rewriteSnapshotAsPreV1(snapshotPath);
    const unstamped = new Database(snapshotPath, { readonly: true });
    expect(readSchemaMigrations(unstamped)).toEqual([]);
    unstamped.close();
    const { manifest } = signChangedSnapshot(companyRoot, backup.backupDir!);
    const sourceBytes = readFileSync(snapshotPath);

    const restored = restoreSystemBackup({ backupDir: backup.backupDir!, targetCompanyRoot: restoredRoot });
    expect(restored.ok, restored.errors.join("; ")).toBe(true);
    expect(readFileSync(snapshotPath)).toEqual(sourceBytes);

    const restoredDb = new Database(restored.restoredDbPath!);
    const migrations = readSchemaMigrations(restoredDb);
    expect(migrations).toEqual(supportedSchemaMigrations().map((migration) => expect.objectContaining({
      ...migration,
      applied_by_version: expect.any(String),
    })));
    /*expect(migrations).toEqual([
      expect.objectContaining({
        id: 1,
        name: BASELINE_MIGRATION_NAME,
        checksum: BASELINE_MIGRATION_CHECKSUM,
        applied_by_version: expect.any(String),
      }),
      expect.objectContaining({
        id: 2,
        name: PEPPOL_SUBMISSION_EVENTS_MIGRATION_NAME,
        checksum: PEPPOL_SUBMISSION_EVENTS_MIGRATION_CHECKSUM,
        applied_by_version: expect.any(String),
      }),
      expect.objectContaining({
        id: 3,
        name: RECURRING_AUTOMATION_MIGRATION_NAME,
        checksum: RECURRING_AUTOMATION_MIGRATION_CHECKSUM,
        applied_by_version: expect.any(String),
      }),
      expect.objectContaining({
        id: 4,
        name: DINERO_IMPORT_PROVENANCE_MIGRATION_NAME,
        checksum: DINERO_IMPORT_PROVENANCE_MIGRATION_CHECKSUM,
        applied_by_version: expect.any(String),
      }),
      expect.objectContaining({
        id: 5,
        name: MIGRATION_OPEN_ITEMS_MIGRATION_NAME,
        checksum: MIGRATION_OPEN_ITEMS_MIGRATION_CHECKSUM,
        applied_by_version: expect.any(String),
      }),
      expect.objectContaining({
        id: 6,
        name: BANK_JOURNAL_RECONCILIATION_LINKS_MIGRATION_NAME,
        checksum: BANK_JOURNAL_RECONCILIATION_LINKS_MIGRATION_CHECKSUM,
        applied_by_version: expect.any(String),
      }),
      expect.objectContaining({
        id: 7,
        name: DOCUMENT_SCAN_EVIDENCE_MIGRATION_NAME,
        checksum: DOCUMENT_SCAN_EVIDENCE_MIGRATION_CHECKSUM,
        applied_by_version: expect.any(String),
      }),
      expect.objectContaining({
        id: 8,
        name: ISSUED_INVOICE_PDF_IMMUTABILITY_MIGRATION_NAME,
        checksum: ISSUED_INVOICE_PDF_IMMUTABILITY_MIGRATION_CHECKSUM,
        applied_by_version: expect.any(String),
      }),
      expect.objectContaining({
        id: 9,
        name: ACCOUNTING_DRAFT_WORKFLOW_MIGRATION_NAME,
        checksum: ACCOUNTING_DRAFT_WORKFLOW_MIGRATION_CHECKSUM,
        applied_by_version: expect.any(String),
      }),
      expect.objectContaining({
        id: 10,
        name: INTERNAL_VOUCHER_EVIDENCE_MIGRATION_NAME,
        checksum: INTERNAL_VOUCHER_EVIDENCE_MIGRATION_CHECKSUM,
        applied_by_version: expect.any(String),
      }),
      expect.objectContaining({ id: 11, name: PURCHASE_VAT_PREFLIGHT_MIGRATION_NAME, checksum: PURCHASE_VAT_PREFLIGHT_MIGRATION_CHECKSUM, applied_by_version: expect.any(String) }),
      expect.objectContaining({ id: 12, name: POSTING_RULES_MIGRATION_NAME, checksum: POSTING_RULES_MIGRATION_CHECKSUM, applied_by_version: expect.any(String) }),
      expect.objectContaining({ id: 13, name: BOOKKEEPING_BATCHES_MIGRATION_NAME, checksum: BOOKKEEPING_BATCHES_MIGRATION_CHECKSUM, applied_by_version: expect.any(String) }),
      expect.objectContaining({ id: 14, name: INVOICE_EXTRACTION_EVIDENCE_MIGRATION_NAME, checksum: INVOICE_EXTRACTION_EVIDENCE_MIGRATION_CHECKSUM, applied_by_version: expect.any(String) }),
      expect.objectContaining({ id: 15, name: BOOKKEEPING_BATCH_RETRIES_MIGRATION_NAME, checksum: BOOKKEEPING_BATCH_RETRIES_MIGRATION_CHECKSUM, applied_by_version: expect.any(String) }),
      expect.objectContaining({ id: 16, name: INVOICE_EXTRACTION_ACTORS_MIGRATION_NAME, checksum: INVOICE_EXTRACTION_ACTORS_MIGRATION_CHECKSUM, applied_by_version: expect.any(String) }),
      expect.objectContaining({ id: 17, name: DOCUMENT_PDF_PARSES_MIGRATION_NAME, checksum: DOCUMENT_PDF_PARSES_MIGRATION_CHECKSUM, applied_by_version: expect.any(String) }),
      expect.objectContaining({ id: 18, name: DOCUMENT_METADATA_ENRICHMENTS_MIGRATION_NAME, checksum: DOCUMENT_METADATA_ENRICHMENTS_MIGRATION_CHECKSUM, applied_by_version: expect.any(String) }),
      expect.objectContaining({ id: 19, name: DOCUMENT_COMPANY_CONTEXTS_MIGRATION_NAME, checksum: DOCUMENT_COMPANY_CONTEXTS_MIGRATION_CHECKSUM, applied_by_version: expect.any(String) }),
      expect.objectContaining({ id: 20, name: MUTATION_IDEMPOTENCY_RECEIPTS_MIGRATION_NAME, checksum: MUTATION_IDEMPOTENCY_RECEIPTS_MIGRATION_CHECKSUM, applied_by_version: expect.any(String) }),
      expect.objectContaining({ id: 21, name: BOOKKEEPING_BATCH_REVISIONS_MIGRATION_NAME, checksum: BOOKKEEPING_BATCH_REVISIONS_MIGRATION_CHECKSUM, applied_by_version: expect.any(String) }),
      expect.objectContaining({ id: 22, name: PERIOD_CLOSE_READINESS_MIGRATION_NAME, checksum: PERIOD_CLOSE_READINESS_MIGRATION_CHECKSUM, applied_by_version: expect.any(String) }),
      expect.objectContaining({ id: 23, name: LOCAL_IDEMPOTENCY_TOMBSTONES_MIGRATION_NAME, checksum: LOCAL_IDEMPOTENCY_TOMBSTONES_MIGRATION_CHECKSUM, applied_by_version: expect.any(String) }),
      expect.objectContaining({ id: 24, name: BOOKKEEPING_BATCH_PRINCIPALS_MIGRATION_NAME, checksum: BOOKKEEPING_BATCH_PRINCIPALS_MIGRATION_CHECKSUM, applied_by_version: expect.any(String) }),
      expect.objectContaining({ id: 25, name: PERIOD_CLOSE_REVIEWS_MIGRATION_NAME, checksum: PERIOD_CLOSE_REVIEWS_MIGRATION_CHECKSUM, applied_by_version: expect.any(String) }),
      expect.objectContaining({ id: 26, name: DOCUMENT_PARTY_LINKS_MIGRATION_NAME, checksum: DOCUMENT_PARTY_LINKS_MIGRATION_CHECKSUM, applied_by_version: expect.any(String) }),
      expect.objectContaining({ id: 27, name: SUPPLIER_COMMITMENTS_MIGRATION_NAME, checksum: SUPPLIER_COMMITMENTS_MIGRATION_CHECKSUM, applied_by_version: expect.any(String) }),
      expect.objectContaining({ id: 28, name: ACCOUNTING_DIMENSIONS_MIGRATION_NAME, checksum: ACCOUNTING_DIMENSIONS_MIGRATION_CHECKSUM, applied_by_version: expect.any(String) }),
      expect.objectContaining({ id: 29, name: DOCUMENT_PARTY_RESOLUTION_MIGRATION_NAME, checksum: DOCUMENT_PARTY_RESOLUTION_MIGRATION_CHECKSUM, applied_by_version: expect.any(String) }),
      expect.objectContaining({ id: 30, name: ACCOUNTING_DIMENSION_LIFECYCLE_MIGRATION_NAME, checksum: ACCOUNTING_DIMENSION_LIFECYCLE_MIGRATION_CHECKSUM, applied_by_version: expect.any(String) }),
      expect.objectContaining({ id: 31, name: SUPPLIER_COMMITMENT_OCCURRENCE_MATCHES_MIGRATION_NAME, checksum: SUPPLIER_COMMITMENT_OCCURRENCE_MATCHES_MIGRATION_CHECKSUM, applied_by_version: expect.any(String) }),
      expect.objectContaining({ id: 32, name: DIMENSION_BUDGET_AND_PROVENANCE_MIGRATION_NAME, checksum: DIMENSION_BUDGET_AND_PROVENANCE_MIGRATION_CHECKSUM, applied_by_version: expect.any(String) }),
      expect.objectContaining({ id: 33, name: BANK_RECONCILIATION_CORRECTIONS_MIGRATION_NAME, checksum: BANK_RECONCILIATION_CORRECTIONS_MIGRATION_CHECKSUM, applied_by_version: expect.any(String) }),
      expect.objectContaining({ id: 34, name: DIRECT_BANK_PURCHASE_PAYABLE_CORRECTIONS_MIGRATION_NAME, checksum: DIRECT_BANK_PURCHASE_PAYABLE_CORRECTIONS_MIGRATION_CHECKSUM, applied_by_version: expect.any(String) }),
      expect.objectContaining({ id: 35, name: BANK_RECONCILIATION_ACCOUNT_ROLE_FALLBACK_MIGRATION_NAME, checksum: BANK_RECONCILIATION_ACCOUNT_ROLE_FALLBACK_MIGRATION_CHECKSUM, applied_by_version: expect.any(String) }),
      expect.objectContaining({ id: 36, name: IMPORTED_RECEIVABLES_MIGRATION_NAME, checksum: IMPORTED_RECEIVABLES_MIGRATION_CHECKSUM, applied_by_version: expect.any(String) }),
      expect.objectContaining({ id: 37, name: IMPORTED_RECEIVABLE_BOUNDARIES_MIGRATION_NAME, checksum: IMPORTED_RECEIVABLE_BOUNDARIES_MIGRATION_CHECKSUM, applied_by_version: expect.any(String) }),
      expect.objectContaining({ id: 38, name: LEGACY_IMPORTED_RECEIVABLE_BACKFILLS_MIGRATION_NAME, checksum: LEGACY_IMPORTED_RECEIVABLE_BACKFILLS_MIGRATION_CHECKSUM, applied_by_version: expect.any(String) }),
      expect.objectContaining({ id: 39, name: NON_CASH_BALANCE_CORRECTIONS_MIGRATION_NAME, checksum: NON_CASH_BALANCE_CORRECTIONS_MIGRATION_CHECKSUM, applied_by_version: expect.any(String) }),
      expect.objectContaining({ id: 40, name: BANK_STATEMENT_ORDER_MIGRATION_NAME, checksum: BANK_STATEMENT_ORDER_MIGRATION_CHECKSUM, applied_by_version: expect.any(String) }),
      expect.objectContaining({ id: 41, name: LEGACY_OPENING_CREDITOR_RECLASSIFICATION_MIGRATION_NAME, checksum: LEGACY_OPENING_CREDITOR_RECLASSIFICATION_MIGRATION_CHECKSUM, applied_by_version: expect.any(String) }),
      expect.objectContaining({ id: 42, name: LEGACY_BANK_PAYABLE_BACKFILLS_MIGRATION_NAME, checksum: LEGACY_BANK_PAYABLE_BACKFILLS_MIGRATION_CHECKSUM, applied_by_version: expect.any(String) }),
      expect.objectContaining({ id: 43, name: VAT_FILING_EVIDENCE_MIGRATION_NAME, checksum: VAT_FILING_EVIDENCE_MIGRATION_CHECKSUM, applied_by_version: expect.any(String) }),
      expect.objectContaining({ id: 44, name: IMPORTED_RECEIVABLE_BANK_SETTLEMENTS_MIGRATION_NAME, checksum: IMPORTED_RECEIVABLE_BANK_SETTLEMENTS_MIGRATION_CHECKSUM, applied_by_version: expect.any(String) }),
    ]);*/
    expect(manifest.provenance).toEqual(expect.objectContaining({
      product: expect.objectContaining({ version: expect.any(String) }),
      schema: { version: CURRENT_SCHEMA_VERSION, baselineChecksum: BASELINE_MIGRATION_CHECKSUM },
    }));
    for (const table of ["invoice_payments", "invoice_refunds", "invoice_claim_payments"]) {
      const columns = restoredDb.query(`PRAGMA table_info(${table})`).all() as Array<{ name: string }>;
      expect(columns.some((column) => column.name === "journal_entry_id")).toBe(true);
    }
    expect(validateInvoiceJournalEvidence(restoredDb).ok).toBe(true);
    expect(verifyAuditChain(restoredDb, { companyRoot: restoredRoot }).ok).toBe(true);
    expect(buildBankReconciliationReport(restoredDb, "2026-01-01", "2026-12-31").ok).toBe(true);
    expect(buildTrialBalance(restoredDb, "2026-01-01", "2026-12-31").ok).toBe(true);
    expect(buildVatReport(restoredDb, "2026-01-01", "2026-12-31").ok).toBe(true);
    restoredDb.close();
    expect(restoreStagingEntries(root)).toEqual([]);
    rmSync(root, { recursive: true, force: true });
  });

  test("rejects invalid authentication before staging or migrating a pre-v1 snapshot", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-restore-pre-v1-auth-"));
    const companyRoot = join(root, "company");
    const restoredRoot = join(root, "restored-company");
    const db = openDb(ensureCompanyDirs(companyRoot).db);
    migrate(db);
    const backup = createSystemBackup(db, companyRoot, { createdAt: "2026-05-17T02:39:00.000Z" });
    db.close();
    expect(backup.ok).toBe(true);

    const snapshotPath = join(backup.backupDir!, "ledger.sqlite");
    rewriteSnapshotAsPreV1(snapshotPath);
    signChangedSnapshot(companyRoot, backup.backupDir!);
    writeFileSync(join(backup.backupDir!, "manifest.json.hmac"), "0".repeat(64));

    const restored = restoreSystemBackup({ backupDir: backup.backupDir!, targetCompanyRoot: restoredRoot });
    expect(restored.ok).toBe(false);
    expect(restored.errors[0]).toContain("authenticity");
    const legacy = new Database(snapshotPath, { readonly: true });
    for (const table of ["invoice_payments", "invoice_refunds", "invoice_claim_payments"]) {
      const columns = legacy.query(`PRAGMA table_info(${table})`).all() as Array<{ name: string }>;
      expect(columns.some((column) => column.name === "journal_entry_id")).toBe(false);
    }
    legacy.close();
    expect(existsSync(restoredRoot)).toBe(false);
    expect(restoreStagingEntries(root)).toEqual([]);
    rmSync(root, { recursive: true, force: true });
  });

  test("restores a moved backup into a fresh company root and records a restore audit event", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-restore-"));
    const companyRoot = join(root, "company");
    const movedBackupsRoot = join(root, "moved-backups");
    const restoredRoot = join(root, "restored-company");
    const paths = ensureCompanyDirs(companyRoot);
    const db = openDb(paths.db);
    migrate(db);
    seedAccounts(db);

    const ingested = ingestDocument(db, companyRoot, join(process.cwd(), "examples/vendor-invoice.txt"), JSON.parse(readFileSync(join(process.cwd(), "examples/vendor-invoice.metadata.json"), "utf8")));
    expect(ingested.ok).toBe(true);
    const journal = postJournalEntry(db, JSON.parse(readFileSync(join(process.cwd(), "examples/journal-entry.expense.json"), "utf8")));
    expect(journal.ok).toBe(true);

    const backup = createSystemBackup(db, companyRoot, { createdAt: "2026-05-17T02:39:00.000Z" });
    expect(backup.ok).toBe(true);
    db.close();

    mkdirSync(movedBackupsRoot, { recursive: true });
    const movedBackupDir = join(movedBackupsRoot, "portable-backup");
    renameSync(backup.backupDir!, movedBackupDir);
    const detachedVerificationKey = join(root, "detached-backup.key");
    copyFileSync(backupManifestKeyPath(companyRoot), detachedVerificationKey);
    // A portable restore must validate the files copied into staging. The old
    // source company (and every historical absolute stored_path) may be gone.
    rmSync(companyRoot, { recursive: true, force: true });

    const prevActor = process.env.RENTEMESTER_ACTOR;
    const prevVia = process.env.RENTEMESTER_ACTOR_VIA;
    process.env.RENTEMESTER_ACTOR = "user:mikkel";
    process.env.RENTEMESTER_ACTOR_VIA = "restore-cli";
    const restored = restoreSystemBackup({ backupDir: movedBackupDir, targetCompanyRoot: restoredRoot, verificationKeyPath: detachedVerificationKey });
    if (prevActor === undefined) delete process.env.RENTEMESTER_ACTOR; else process.env.RENTEMESTER_ACTOR = prevActor;
    if (prevVia === undefined) delete process.env.RENTEMESTER_ACTOR_VIA; else process.env.RENTEMESTER_ACTOR_VIA = prevVia;
    expect(restored.ok).toBe(true);
    expect(existsSync(restored.restoredDbPath!)).toBe(true);
    expect(restored.restoredFiles?.documentsOriginals).toBe(1);

    const restoredDb = openDb(join(restoredRoot, "data", "ledger.sqlite"));
    migrate(restoredDb);
    const documentCount = (restoredDb.query("SELECT COUNT(*) AS n FROM documents").get() as { n: number }).n;
    const journalCount = (restoredDb.query("SELECT COUNT(*) AS n FROM journal_entries").get() as { n: number }).n;
    const restoreEvent = restoredDb.query(
      "SELECT event_type, actor, message FROM audit_log WHERE event_type = 'system_restore' ORDER BY id DESC LIMIT 1"
    ).get() as { event_type: string; actor: string | null; message: string } | null;
    restoredDb.close();

    expect(documentCount).toBe(1);
    expect(journalCount).toBe(1);
    expect(restoreEvent?.event_type).toBe("system_restore");
    expect(restoreEvent?.actor).toBe("user:mikkel via restore-cli");
    expect(restoreEvent?.message).toContain("backup-20260517T023900Z");

    rmSync(root, { recursive: true, force: true });
  });

  test("rejects a correctly re-signed v2 manifest whose release provenance is missing", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-restore-provenance-"));
    const companyRoot = join(root, "company");
    const restoredRoot = join(root, "restored-company");
    const paths = ensureCompanyDirs(companyRoot);
    const db = openDb(paths.db);
    migrate(db);
    const backup = createSystemBackup(db, companyRoot, {
      createdAt: "2026-05-17T02:39:00.000Z",
    });
    db.close();
    expect(backup.ok).toBe(true);

    const manifestPath = join(backup.backupDir!, "manifest.json");
    const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
    delete manifest.provenance;
    rewriteSignedManifest(companyRoot, backup.backupDir!, manifest);

    const restored = restoreSystemBackup({
      backupDir: backup.backupDir!,
      targetCompanyRoot: restoredRoot,
    });
    expect(restored.ok).toBe(false);
    expect(restored.errors).toContain(
      `invalid or missing backup manifest in ${backup.backupDir}`,
    );
    expect(existsSync(restoredRoot)).toBe(false);
    rmSync(root, { recursive: true, force: true });
  });

  test("rejects a correctly re-signed backup from a newer schema before target mutation", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-restore-future-schema-"));
    const companyRoot = join(root, "company");
    const restoredRoot = join(root, "restored-company");
    const paths = ensureCompanyDirs(companyRoot);
    const db = openDb(paths.db);
    migrate(db);
    const backup = createSystemBackup(db, companyRoot, {
      createdAt: "2026-05-17T02:39:00.000Z",
    });
    db.close();
    expect(backup.ok).toBe(true);

    const manifestPath = join(backup.backupDir!, "manifest.json");
    const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
    const snapshotPath = join(backup.backupDir!, manifest.dbSnapshot.path);
    const snapshot = new Database(snapshotPath);
    snapshot.query(
      `INSERT INTO schema_migrations
         (id, name, checksum, applied_by_version)
       VALUES (?, 'future', 'future-checksum', '0.4.0')`,
    ).run(CURRENT_SCHEMA_VERSION + 1);
    snapshot.close();
    manifest.dbSnapshot.sha256 = sha256File(snapshotPath);
    manifest.dbSnapshot.sizeBytes = statSync(snapshotPath).size;
    rewriteSignedManifest(companyRoot, backup.backupDir!, manifest);
    const sourceBytes = readFileSync(snapshotPath);
    mkdirSync(restoredRoot, { recursive: true });
    const sentinel = join(restoredRoot, "keep-me.txt");
    writeFileSync(sentinel, "existing target content");

    const restored = restoreSystemBackup({
      backupDir: backup.backupDir!,
      targetCompanyRoot: restoredRoot,
      allowNonEmptyTarget: true,
    });
    expect(restored.ok).toBe(false);
    expect(restored.errors[0]).toContain(`newer than supported version ${CURRENT_SCHEMA_VERSION}`);
    expect(readFileSync(sentinel, "utf8")).toBe("existing target content");
    expect(existsSync(join(restoredRoot, "data", "ledger.sqlite"))).toBe(false);
    expect(readFileSync(snapshotPath)).toEqual(sourceBytes);
    expect(restoreStagingEntries(root)).toEqual([]);
    rmSync(root, { recursive: true, force: true });
  });

  test("rejects re-signed unposted evidence that differs from the document register before target swap", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-restore-evidence-tamper-"));
    const companyRoot = join(root, "company");
    const restoredRoot = join(root, "restored-company");
    const paths = ensureCompanyDirs(companyRoot);
    const db = openDb(paths.db);
    migrate(db);
    seedAccounts(db);
    const ingested = ingestDocument(
      db,
      companyRoot,
      join(process.cwd(), "examples/vendor-invoice.txt"),
      JSON.parse(readFileSync(join(process.cwd(), "examples/vendor-invoice.metadata.json"), "utf8")),
    );
    expect(ingested.ok).toBe(true);
    const sourceEvidenceHash = sha256File(ingested.storedPath!);
    const backup = createSystemBackup(db, companyRoot, { createdAt: "2026-05-17T02:39:00.000Z" });
    expect(backup.ok).toBe(true);
    db.close();

    const manifestPath = join(backup.backupDir!, "manifest.json");
    const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
    const evidenceManifest = manifest.copiedFiles.documentsOriginals[0];
    const backupEvidencePath = join(backup.backupDir!, evidenceManifest.path);
    writeFileSync(backupEvidencePath, "re-signed but ledger-inconsistent evidence");
    evidenceManifest.sha256 = sha256File(backupEvidencePath);
    evidenceManifest.sizeBytes = readFileSync(backupEvidencePath).byteLength;
    rewriteSignedManifest(companyRoot, backup.backupDir!, manifest);

    mkdirSync(restoredRoot, { recursive: true });
    const sentinel = join(restoredRoot, "keep-me.txt");
    writeFileSync(sentinel, "existing target content");
    const restored = restoreSystemBackup({
      backupDir: backup.backupDir!,
      targetCompanyRoot: restoredRoot,
      allowNonEmptyTarget: true,
    });
    expect(restored.ok).toBe(false);
    expect(restored.errors.join(" ")).toContain("stored evidence sha256 does not match");
    expect(readFileSync(sentinel, "utf8")).toBe("existing target content");
    expect(existsSync(join(restoredRoot, "data", "ledger.sqlite"))).toBe(false);
    // The intact source file must not have been consulted as a fallback.
    expect(sha256File(ingested.storedPath!)).toBe(sourceEvidenceHash);

    rmSync(root, { recursive: true, force: true });
  });

  test("rejects a re-signed v4 backup whose linked document digest no longer matches its source entry", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-restore-v4-provenance-tamper-"));
    const companyRoot = join(root, "company");
    const restoredRoot = join(root, "restored-company");
    const db = openDb(ensureCompanyDirs(companyRoot).db);
    migrate(db);
    seedV4Provenance(db);
    const backup = createSystemBackup(db, companyRoot, { createdAt: "2026-05-17T02:39:00.000Z" });
    db.close();
    expect(backup.ok).toBe(true);

    const snapshotPath = join(backup.backupDir!, "ledger.sqlite");
    const snapshot = openDb(snapshotPath, { journalMode: "DELETE" });
    snapshot.exec("PRAGMA foreign_keys = OFF; DROP TRIGGER dinero_import_document_links_no_update;");
    snapshot.query("UPDATE dinero_import_document_links SET entry_sha256 = ? WHERE id = 1").run(provenanceHash("d"));
    snapshot.close();
    signChangedSnapshot(companyRoot, backup.backupDir!);

    const restored = restoreSystemBackup({ backupDir: backup.backupDir!, targetCompanyRoot: restoredRoot });
    expect(restored.ok).toBe(false);
    expect(restored.errors.join(" ")).toContain("FK violations");
    expect(existsSync(join(restoredRoot, "data", "ledger.sqlite"))).toBe(false);
    expect(restoreStagingEntries(root)).toEqual([]);
    rmSync(root, { recursive: true, force: true });
  });

  test("rejects a manifest path that escapes the backup directory", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-restore-escape-"));
    const companyRoot = join(root, "company");
    const restoredRoot = join(root, "restored-company");
    const outsideRoot = join(root, "outside");
    const paths = ensureCompanyDirs(companyRoot);
    const db = openDb(paths.db);
    migrate(db);
    seedAccounts(db);

    const ingested = ingestDocument(db, companyRoot, join(process.cwd(), "examples/vendor-invoice.txt"), JSON.parse(readFileSync(join(process.cwd(), "examples/vendor-invoice.metadata.json"), "utf8")));
    expect(ingested.ok).toBe(true);

    const backup = createSystemBackup(db, companyRoot, { createdAt: "2026-05-17T02:39:00.000Z" });
    expect(backup.ok).toBe(true);
    db.close();

    mkdirSync(outsideRoot, { recursive: true });
    const outsideDb = join(outsideRoot, "ledger.sqlite");
    copyFileSync(join(backup.backupDir!, "ledger.sqlite"), outsideDb);

    const manifestPath = join(backup.backupDir!, "manifest.json");
    const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
    manifest.dbSnapshot.path = outsideDb;
    manifest.dbSnapshot.sha256 = sha256File(outsideDb);
    manifest.dbSnapshot.sizeBytes = readFileSync(outsideDb).byteLength;
    rewriteSignedManifest(companyRoot, backup.backupDir!, manifest);

    const restored = restoreSystemBackup({ backupDir: backup.backupDir!, targetCompanyRoot: restoredRoot });
    expect(restored.ok).toBe(false);
    expect(restored.errors[0]).toContain("manifest path escapes backup dir");

    rmSync(root, { recursive: true, force: true });
  });

  test("rejects a backup whose files and manifest are rewritten without a valid manifest signature", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-restore-tampered-"));
    const companyRoot = join(root, "company");
    const restoredRoot = join(root, "restored-company");
    const paths = ensureCompanyDirs(companyRoot);
    const db = openDb(paths.db);
    migrate(db);
    seedAccounts(db);

    const ingested = ingestDocument(db, companyRoot, join(process.cwd(), "examples/vendor-invoice.txt"), JSON.parse(readFileSync(join(process.cwd(), "examples/vendor-invoice.metadata.json"), "utf8")));
    expect(ingested.ok).toBe(true);

    const backup = createSystemBackup(db, companyRoot, { createdAt: "2026-05-17T02:39:00.000Z" });
    expect(backup.ok).toBe(true);
    db.close();

    const snapshotPath = join(backup.backupDir!, "ledger.sqlite");
    const snapshotDb = openDb(snapshotPath);
    snapshotDb.exec("PRAGMA foreign_keys = OFF");
    snapshotDb.exec("DROP TRIGGER IF EXISTS documents_no_update");
    snapshotDb.run("UPDATE documents SET original_filename = 'tampered.txt' WHERE id = 1");
    snapshotDb.close();

    const manifestPath = join(backup.backupDir!, "manifest.json");
    const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
    manifest.dbSnapshot.sha256 = sha256File(snapshotPath);
    manifest.dbSnapshot.sizeBytes = readFileSync(snapshotPath).byteLength;
    writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);

    const restored = restoreSystemBackup({ backupDir: backup.backupDir!, targetCompanyRoot: restoredRoot });
    expect(restored.ok).toBe(false);
    expect(restored.errors[0]).toContain("authenticity");

    rmSync(root, { recursive: true, force: true });
  });

  test("rejects a backup whose snapshot passes file hash checks but fails audit validation", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-restore-auditfail-"));
    const companyRoot = join(root, "company");
    const restoredRoot = join(root, "restored-company");
    const paths = ensureCompanyDirs(companyRoot);
    const db = openDb(paths.db);
    migrate(db);
    seedAccounts(db);

    const ingested = ingestDocument(db, companyRoot, join(process.cwd(), "examples/vendor-invoice.txt"), JSON.parse(readFileSync(join(process.cwd(), "examples/vendor-invoice.metadata.json"), "utf8")));
    expect(ingested.ok).toBe(true);
    const journal = postJournalEntry(db, JSON.parse(readFileSync(join(process.cwd(), "examples/journal-entry.expense.json"), "utf8")));
    expect(journal.ok).toBe(true);

    const backup = createSystemBackup(db, companyRoot, { createdAt: "2026-05-17T02:39:00.000Z" });
    expect(backup.ok).toBe(true);
    db.close();

    const snapshotDb = openDb(join(backup.backupDir!, "ledger.sqlite"), { journalMode: "DELETE" });
    snapshotDb.exec("PRAGMA foreign_keys = OFF");
    snapshotDb.exec("DROP TRIGGER IF EXISTS journal_entries_no_update");
    snapshotDb.run("UPDATE journal_entries SET previous_hash = 'BROKEN' WHERE id = 1");
    snapshotDb.close();

    const manifestPath = join(backup.backupDir!, "manifest.json");
    const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
    manifest.dbSnapshot.sha256 = sha256File(join(backup.backupDir!, "ledger.sqlite"));
    manifest.dbSnapshot.sizeBytes = readFileSync(join(backup.backupDir!, "ledger.sqlite")).byteLength;
    rewriteSignedManifest(companyRoot, backup.backupDir!, manifest);

    const restored = restoreSystemBackup({ backupDir: backup.backupDir!, targetCompanyRoot: restoredRoot });
    expect(restored.ok).toBe(false);
    expect(restored.errors[0]).toContain("broken audit chain");

    rmSync(root, { recursive: true, force: true });
  });

  test("issue #139: a failed restore leaves the target with no clobbered ledger or document files", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-restore-rollback-"));
    const companyRoot = join(root, "company");
    const restoredRoot = join(root, "restored-company");
    const paths = ensureCompanyDirs(companyRoot);
    const db = openDb(paths.db);
    migrate(db);
    seedAccounts(db);

    const ingested = ingestDocument(db, companyRoot, join(process.cwd(), "examples/vendor-invoice.txt"), JSON.parse(readFileSync(join(process.cwd(), "examples/vendor-invoice.metadata.json"), "utf8")));
    expect(ingested.ok).toBe(true);
    const journal = postJournalEntry(db, JSON.parse(readFileSync(join(process.cwd(), "examples/journal-entry.expense.json"), "utf8")));
    expect(journal.ok).toBe(true);

    const backup = createSystemBackup(db, companyRoot, { createdAt: "2026-05-17T02:39:00.000Z" });
    expect(backup.ok).toBe(true);
    db.close();

    // Corrupt the audit chain inside the snapshot; hashes still match the
    // manifest, so file checks pass but validateRestoredDb must reject it.
    const snapshotDb = openDb(join(backup.backupDir!, "ledger.sqlite"), { journalMode: "DELETE" });
    snapshotDb.exec("PRAGMA foreign_keys = OFF");
    snapshotDb.exec("DROP TRIGGER IF EXISTS journal_entries_no_update");
    snapshotDb.run("UPDATE journal_entries SET previous_hash = 'BROKEN' WHERE id = 1");
    snapshotDb.close();
    const manifest = JSON.parse(readFileSync(join(backup.backupDir!, "manifest.json"), "utf8"));
    manifest.dbSnapshot.sha256 = sha256File(join(backup.backupDir!, "ledger.sqlite"));
    manifest.dbSnapshot.sizeBytes = readFileSync(join(backup.backupDir!, "ledger.sqlite")).byteLength;
    rewriteSignedManifest(companyRoot, backup.backupDir!, manifest);

    const restored = restoreSystemBackup({ backupDir: backup.backupDir!, targetCompanyRoot: restoredRoot });
    expect(restored.ok).toBe(false);
    expect(restored.errors[0]).toContain("broken audit chain");

    // The target must NOT be left half-restored: no ledger DB, no copied
    // document files should have survived the failed validation.
    expect(existsSync(join(restoredRoot, "data", "ledger.sqlite"))).toBe(false);
    const docsDir = join(restoredRoot, "documents", "originals");
    const leakedDocs = existsSync(docsDir) ? readdirSync(docsDir) : [];
    expect(leakedDocs).toEqual([]);
    expect(restoreStagingEntries(root)).toEqual([]);

    rmSync(root, { recursive: true, force: true });
  });
});
