import { Database } from "bun:sqlite";
import { describe, expect, test } from "bun:test";
import { migrate } from "../../src/core/db";
import { seedAccounts, postJournalEntry } from "../../src/core/ledger";
import {
  applyMigrationOpenItem,
  getMigrationOpenItems,
  recordMigrationOpenItemBatch,
} from "../../src/core/migration-open-items";
import {
  CURRENT_SCHEMA_VERSION,
  MIGRATION_OPEN_ITEMS_MIGRATION_NAME,
  readSchemaMigrations,
  validateSchemaMigrationHistory,
} from "../../src/core/schema-version";

const hash = (letter: string) => letter.repeat(64);

function setup() {
  const db = new Database(":memory:");
  db.exec("PRAGMA foreign_keys = ON");
  migrate(db);
  seedAccounts(db);
  db.query(`INSERT INTO dinero_import_sources
    (id, raw_sha256, raw_size_bytes, canonical_listing_sha256, canonical_listing_count)
    VALUES (1, ?, 1, ?, 0)`).run(hash("a"), hash("b"));
  db.query(`INSERT INTO dinero_import_inventories
    (id, source_id, source_raw_sha256, canonical_listing_sha256, canonical_listing_count, entry_count, total_size_bytes)
    VALUES (1, 1, ?, ?, 0, 0, 0)`).run(hash("a"), hash("b"));
  db.query(`INSERT INTO dinero_import_attempts
    (id, inventory_id, source_id, source_raw_sha256, parser_contract, actor, cutover_date, outcome, result_sha256)
    VALUES (1, 1, 1, ?, 'synthetic-v1', 'agent:test', '2026-01-01', 'accepted', ?)`).run(hash("a"), hash("c"));
  return db;
}

function journal(db: Database, date: string, lines: Array<{ accountNo: string; debitAmount?: number; creditAmount?: number }>, sourceBankTransactionId?: number) {
  const result = postJournalEntry(db, { transactionDate: date, text: "Synthetic migration evidence", sourceBankTransactionId, lines });
  expect(result.ok).toBe(true);
  return Number(result.entryId);
}

function bank(db: Database, id: number, amount: number) {
  db.query("INSERT INTO bank_transactions (id, transaction_date, text, amount) VALUES (?, '2026-02-01', 'Synthetic bank evidence', ?)").run(id, amount);
}

describe("migration open items v5", () => {
  test("migrates a fresh database through the current schema and preserves v4 provenance when upgrading", () => {
    const db = setup();
    // The current append-only migration catalogue is the authority.  This
    // regression deliberately protects the documented v4 provenance path,
    // not a stale historical schema number.
    expect(CURRENT_SCHEMA_VERSION).toBeGreaterThanOrEqual(5);
    expect(readSchemaMigrations(db)).toContainEqual(expect.objectContaining({ id: 5, name: MIGRATION_OPEN_ITEMS_MIGRATION_NAME }));
    db.exec(`
      DROP VIEW bank_journal_reconciliations;
      DROP TRIGGER bank_journal_reconciliation_links_guard_insert;
      DROP TRIGGER bank_journal_reconciliation_links_no_update;
      DROP TRIGGER bank_journal_reconciliation_links_no_delete;
      DROP TRIGGER journal_entries_bank_reconciliation_link_conflict;
      DROP TABLE bank_journal_reconciliation_links;
      DROP TRIGGER migration_open_item_batches_no_update;
      DROP TRIGGER migration_open_item_batches_no_delete;
      DROP TRIGGER migration_open_items_no_update;
      DROP TRIGGER migration_open_items_no_delete;
      DROP TRIGGER migration_open_item_applications_no_update;
      DROP TRIGGER migration_open_item_applications_no_delete;
      DROP TABLE migration_open_item_applications;
      DROP TABLE migration_open_items;
      DROP TABLE migration_open_item_batches;
      DELETE FROM schema_migrations WHERE id >= 5;
    `);
    expect(readSchemaMigrations(db).at(-1)?.id).toBe(4);
    migrate(db);
    const migrations = readSchemaMigrations(db);
    // This upgrade is deliberately checked as an ordered, checksummed prefix
    // rather than merely by count: a restore must never silently skip or
    // rewrite an immutable migration artifact.
    expect(() => validateSchemaMigrationHistory(migrations)).not.toThrow();
    expect(migrations).toHaveLength(CURRENT_SCHEMA_VERSION);
    expect(db.query("SELECT id FROM dinero_import_attempts WHERE id = 1").get()).toEqual({ id: 1 });
    expect(db.query("SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'migration_open_items'").get()).not.toBeNull();
    db.close();
  });

  test("records an exact opening batch and derives open balances without posting journals", () => {
    const db = setup();
    const opening = journal(db, "2026-01-01", [
      { accountNo: "1100", debitAmount: 125 }, { accountNo: "5000", creditAmount: 125 },
    ]);
    const before = db.query("SELECT COUNT(*) AS count FROM journal_entries").get() as { count: number };
    const recorded = recordMigrationOpenItemBatch(db, {
      dineroImportAttemptId: 1,
      controlAccountNo: "1100",
      kind: "receivable",
      sourceControlAmount: 125,
      openingJournalEntryId: opening,
      items: [
        { externalRef: "AR-1", counterpartyName: "Customer one", issueDate: "2026-01-02", dueDate: "2026-02-01", originalAmount: 100, openAmountAtImport: 100, sourceKind: "opening" },
        { externalRef: "AR-2", counterpartyName: "Customer two", issueDate: "2026-01-03", originalAmount: 25, openAmountAtImport: 25, sourceKind: "opening" },
      ],
    });
    expect(recorded).toEqual({ ok: true, batchId: expect.any(Number), errors: [] });
    expect(db.query("SELECT COUNT(*) AS count FROM journal_entries").get()).toEqual(before);
    expect(getMigrationOpenItems(db, recorded.batchId).rows).toEqual([
      expect.objectContaining({ externalRef: "AR-1", openBalance: 100, status: "open", sourceKind: "opening", applications: [] }),
      expect.objectContaining({ externalRef: "AR-2", openBalance: 25, status: "open", sourceKind: "opening", applications: [] }),
    ]);
    db.close();
  });

  test("preserves an exact unallocated control balance without inventing item-level evidence", () => {
    const db = setup();
    const recorded = recordMigrationOpenItemBatch(db, {
      dineroImportAttemptId: 1,
      controlAccountNo: "1100",
      kind: "receivable",
      sourceControlAmount: 510_648.75,
      items: [{
        externalRef: "UNALLOCATED:1100",
        originalAmount: 510_648.75,
        openAmountAtImport: 510_648.75,
        sourceKind: "control_balance",
        resolutionStatus: "unallocated",
      }],
    });
    expect(recorded.ok).toBe(true);
    expect(getMigrationOpenItems(db, recorded.batchId).rows[0]).toEqual(expect.objectContaining({
      externalRef: "UNALLOCATED:1100",
      counterpartyName: null,
      issueDate: null,
      openBalance: 510_648.75,
      sourceKind: "control_balance",
      resolutionStatus: "unallocated",
    }));
    const dishonest = recordMigrationOpenItemBatch(db, {
      dineroImportAttemptId: 1,
      controlAccountNo: "7000",
      kind: "payable",
      sourceControlAmount: 1,
      items: [{
        externalRef: "UNALLOCATED:7000",
        counterpartyName: "Invented vendor",
        originalAmount: 1,
        openAmountAtImport: 1,
        sourceKind: "control_balance",
        resolutionStatus: "unallocated",
      }],
    });
    expect(dishonest.errors.join(" ")).toContain("cannot claim item-level evidence");
    db.close();
  });

  test("represents a current-journal item and a journal-evidenced settlement without replaying it", () => {
    const db = setup();
    const recognition = journal(db, "2026-01-10", [
      { accountNo: "1100", debitAmount: 45 }, { accountNo: "5000", creditAmount: 45 },
    ]);
    const recorded = recordMigrationOpenItemBatch(db, {
      dineroImportAttemptId: 1, controlAccountNo: "1100", kind: "receivable", sourceControlAmount: 45,
      items: [{ externalRef: "AR-current", counterpartyName: "Customer current", issueDate: "2026-01-10", originalAmount: 45, openAmountAtImport: 45, sourceRecognitionJournalEntryId: recognition, sourceKind: "journal" }],
    });
    expect(recorded.ok).toBe(true);
    const item = getMigrationOpenItems(db, recorded.batchId).rows[0]!;
    const settlement = journal(db, "2026-01-20", [
      { accountNo: "2000", debitAmount: 45 }, { accountNo: "1100", creditAmount: 45 },
    ]);
    expect(applyMigrationOpenItem(db, { itemId: item.itemId, amount: 45, applicationDate: "2026-01-20", sourceJournalEntryId: settlement })).toEqual({ ok: true, itemId: item.itemId, applicationId: expect.any(Number), errors: [] });
    expect(getMigrationOpenItems(db, recorded.batchId).rows[0]).toEqual(expect.objectContaining({ appliedAmount: 45, openBalance: 0, status: "settled", sourceRecognitionJournalEntryId: recognition }));
    db.close();
  });

  test("fails closed on one-øre mismatches, duplicate references, and wrong control-account journal effects", () => {
    const db = setup();
    const oneOre = recordMigrationOpenItemBatch(db, {
      dineroImportAttemptId: 1, controlAccountNo: "1100", kind: "receivable", sourceControlAmount: 10,
      items: [{ externalRef: "AR-1", counterpartyName: "Customer", issueDate: "2026-01-01", originalAmount: 9.99, openAmountAtImport: 9.99, sourceKind: "opening" }],
    });
    expect(oneOre.ok).toBe(false);
    expect(oneOre.errors.join(" ")).toContain("exactly reconcile");
    const duplicate = recordMigrationOpenItemBatch(db, {
      dineroImportAttemptId: 1, controlAccountNo: "1100", kind: "receivable", sourceControlAmount: 20,
      items: [
        { externalRef: "same", counterpartyName: "Customer", issueDate: "2026-01-01", originalAmount: 10, openAmountAtImport: 10, sourceKind: "opening" },
        { externalRef: "same", counterpartyName: "Another", issueDate: "2026-01-01", originalAmount: 10, openAmountAtImport: 10, sourceKind: "opening" },
      ],
    });
    expect(duplicate.ok).toBe(false);
    expect(duplicate.errors.join(" ")).toContain("repeats external reference");
    const wrongEffect = journal(db, "2026-01-01", [
      { accountNo: "1100", debitAmount: 9 }, { accountNo: "5000", creditAmount: 9 },
    ]);
    const wrong = recordMigrationOpenItemBatch(db, {
      dineroImportAttemptId: 1, controlAccountNo: "1100", kind: "receivable", sourceControlAmount: 10,
      items: [{ externalRef: "wrong", counterpartyName: "Customer", issueDate: "2026-01-01", originalAmount: 10, openAmountAtImport: 10, sourceRecognitionJournalEntryId: wrongEffect, sourceKind: "journal" }],
    });
    expect(wrong.ok).toBe(false);
    expect(wrong.errors.join(" ")).toContain("wrong control-account effect");
    db.close();
  });

  test("rejects over-application and reusing journal or bank evidence", () => {
    const db = setup();
    const opening = journal(db, "2026-01-01", [
      { accountNo: "1100", debitAmount: 30 }, { accountNo: "5000", creditAmount: 30 },
    ]);
    const recorded = recordMigrationOpenItemBatch(db, {
      dineroImportAttemptId: 1, controlAccountNo: "1100", kind: "receivable", sourceControlAmount: 30, openingJournalEntryId: opening,
      items: [
        { externalRef: "AR-a", counterpartyName: "Customer A", issueDate: "2026-01-01", originalAmount: 10, openAmountAtImport: 10, sourceKind: "opening" },
        { externalRef: "AR-b", counterpartyName: "Customer B", issueDate: "2026-01-01", originalAmount: 10, openAmountAtImport: 10, sourceKind: "opening" },
        { externalRef: "AR-c", counterpartyName: "Customer C", issueDate: "2026-01-01", originalAmount: 10, openAmountAtImport: 10, sourceKind: "opening" },
      ],
    });
    const [first, second, third] = getMigrationOpenItems(db, recorded.batchId).rows;
    const settle = journal(db, "2026-01-02", [
      { accountNo: "2000", debitAmount: 10 }, { accountNo: "1100", creditAmount: 10 },
    ]);
    expect(applyMigrationOpenItem(db, { itemId: first!.itemId, amount: 10, applicationDate: "2026-01-02", sourceJournalEntryId: settle }).ok).toBe(true);
    expect(applyMigrationOpenItem(db, { itemId: first!.itemId, amount: 0.01, applicationDate: "2026-01-02", sourceJournalEntryId: settle }).errors.join(" ")).toContain("over-apply");
    expect(applyMigrationOpenItem(db, { itemId: second!.itemId, amount: 10, applicationDate: "2026-01-02", sourceJournalEntryId: settle }).ok).toBe(false);
    bank(db, 1, 10);
    const bankSettlement = journal(db, "2026-01-03", [
      { accountNo: "2000", debitAmount: 10 }, { accountNo: "1100", creditAmount: 10 },
    ], 1);
    expect(applyMigrationOpenItem(db, { itemId: second!.itemId, amount: 10, applicationDate: "2026-01-03", sourceJournalEntryId: bankSettlement, bankTransactionId: 1 }).ok).toBe(true);
    const anotherSettlement = journal(db, "2026-01-03", [
      { accountNo: "2000", debitAmount: 10 }, { accountNo: "1100", creditAmount: 10 },
    ]);
    expect(() => db.query(`INSERT INTO migration_open_item_applications
      (item_id, amount, application_date, source_journal_entry_id, bank_transaction_id)
      VALUES (?, 10, '2026-01-03', ?, 1)`).run(third!.itemId, anotherSettlement)).toThrow();
    expect(() => db.query(`INSERT INTO migration_open_item_applications
      (item_id, amount, application_date, source_journal_entry_id)
      VALUES (?, 10.01, '2026-01-03', ?)`).run(third!.itemId, anotherSettlement)).toThrow("over-apply");
    db.close();
  });

  test("requires a shared opening journal only for opening items and validates mixed batches by the opening subset", () => {
    const db = setup();
    const sharedOpening = journal(db, "2026-01-01", [
      { accountNo: "1100", debitAmount: 30 }, { accountNo: "7000", creditAmount: 10 }, { accountNo: "5000", creditAmount: 20 },
    ]);
    const current = journal(db, "2026-01-02", [
      { accountNo: "1100", debitAmount: 20 }, { accountNo: "5000", creditAmount: 20 },
    ]);
    const receivables = recordMigrationOpenItemBatch(db, {
      dineroImportAttemptId: 1, controlAccountNo: "1100", kind: "receivable", sourceControlAmount: 50, openingJournalEntryId: sharedOpening,
      items: [
        { externalRef: "opening-ar", counterpartyName: "Opening customer", issueDate: "2026-01-01", originalAmount: 30, openAmountAtImport: 30, sourceKind: "opening" },
        { externalRef: "current-ar", counterpartyName: "Current customer", issueDate: "2026-01-02", originalAmount: 20, openAmountAtImport: 20, sourceRecognitionJournalEntryId: current, sourceKind: "journal" },
      ],
    });
    expect(receivables.ok).toBe(true);
    const payables = recordMigrationOpenItemBatch(db, {
      dineroImportAttemptId: 1, controlAccountNo: "7000", kind: "payable", sourceControlAmount: 10, openingJournalEntryId: sharedOpening,
      items: [{ externalRef: "opening-ap", counterpartyName: "Opening vendor", issueDate: "2026-01-01", originalAmount: 10, openAmountAtImport: 10, sourceKind: "opening" }],
    });
    expect(payables.ok).toBe(true);
    const noOpeningJournal = recordMigrationOpenItemBatch(db, {
      dineroImportAttemptId: 1, controlAccountNo: "1100", kind: "receivable", sourceControlAmount: 20,
      items: [{ externalRef: "missing-opening", counterpartyName: "Customer", issueDate: "2026-01-01", originalAmount: 20, openAmountAtImport: 20, sourceKind: "opening" }],
    });
    expect(noOpeningJournal.errors.join(" ")).toContain("needs opening journal evidence");
    db.close();
  });

  test("restores append-only guards on every migrate", () => {
    const db = setup();
    db.exec("DROP TRIGGER migration_open_items_no_update; CREATE TRIGGER migration_open_items_no_update BEFORE UPDATE ON migration_open_items BEGIN SELECT 1; END;");
    migrate(db);
    const opening = journal(db, "2026-01-01", [
      { accountNo: "1100", debitAmount: 1 }, { accountNo: "5000", creditAmount: 1 },
    ]);
    const batch = recordMigrationOpenItemBatch(db, {
      dineroImportAttemptId: 1, controlAccountNo: "1100", kind: "receivable", sourceControlAmount: 1, openingJournalEntryId: opening,
      items: [{ externalRef: "guard", counterpartyName: "Customer", issueDate: "2026-01-01", originalAmount: 1, openAmountAtImport: 1, sourceKind: "opening" }],
    });
    const item = getMigrationOpenItems(db, batch.batchId).rows[0]!;
    expect(() => db.run("UPDATE migration_open_items SET external_ref = 'changed' WHERE id = ?", item.itemId)).toThrow("append-only");
    expect(() => db.run("DELETE FROM migration_open_item_batches WHERE id = ?", batch.batchId)).toThrow("append-only");
    db.close();
  });
});
