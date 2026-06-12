// Tests: src/core/retention.ts
import { describe, expect, test } from "bun:test";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { ingestDocument } from "../../src/core/documents";
import { postJournalEntry, seedAccounts } from "../../src/core/ledger";
import { importBankCsv } from "../../src/core/bank";
import { buildRetentionStatusReport, retainUntilForDate } from "../../src/core/retention";
import { exportAuthorityPackage } from "../../src/core/authority-export";

import { cleanupDir } from "../helpers/cleanup";
describe("retention tracking", () => {
  test("computes retain_until from fiscal year end plus five years", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-retention-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    db.run(`INSERT INTO companies (id, name, cvr, fiscal_year_start_month, fiscal_year_label_strategy) VALUES (1, 'Rentemester ApS', 'DK12345678', 7, 'end-year')`);

    expect(retainUntilForDate(db, "2026-07-15")).toBe("2032-06-30");
    expect(retainUntilForDate(db, "2027-06-30")).toBe("2032-06-30");

    db.close();
    cleanupDir(root);
  });

  test("runtime migrate tolerates legacy journal rows with NULL retain_until under append-only triggers", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-retention-legacy-journal-"));
    const companyRoot = join(root, "company");
    const db = openDb(ensureCompanyDirs(companyRoot).db);
    migrate(db);
    seedAccounts(db);
    db.run(`INSERT INTO companies (id, name, cvr, fiscal_year_start_month, fiscal_year_label_strategy) VALUES (1, 'Rentemester ApS', 'DK12345678', 7, 'end-year')`);

    const posted = postJournalEntry(db, {
      transactionDate: "2025-07-18",
      text: "Legacy retention gap",
      lines: [
        { accountNo: "2000", debitAmount: 1250 },
        { accountNo: "5000", creditAmount: 1250 },
      ],
    });
    expect(posted.ok).toBe(true);

    db.exec("DROP TRIGGER IF EXISTS journal_entries_no_update");
    db.run("UPDATE journal_entries SET retain_until = NULL WHERE id = ?", posted.entryId!);
    db.exec(`CREATE TRIGGER journal_entries_no_update BEFORE UPDATE ON journal_entries BEGIN SELECT RAISE(ABORT, 'journal_entries are append-only; create reversal instead'); END;`);

    expect(() => migrate(db)).not.toThrow();
    const stored = db.query("SELECT retain_until FROM journal_entries WHERE id = ?").get(posted.entryId!) as { retain_until: string | null };
    expect(stored.retain_until).toBeNull();

    const report = buildRetentionStatusReport(db, "2031-07-01");
    expect(report.ok).toBe(true);
    expect(report.rows.find((row) => row.table === "journal_entries")).toEqual({
      table: "journal_entries",
      total: 1,
      expired: 1,
      nextExpiry: null,
      oldestExpired: "2031-06-30",
    });

    const authority = exportAuthorityPackage(db, companyRoot, { periodStart: "2025-07-01", periodEnd: "2025-07-31", outputDir: join(root, "exports"), requestedAt: "2026-05-17T03:00:00.000Z" });
    expect(authority.ok).toBe(true);
    const exportedJournal = JSON.parse(readFileSync(join(authority.exportDir!, "machine-readable", "journal-entries.json"), "utf8"));
    expect(exportedJournal[0].retainUntil).toBe("2031-06-30");

    db.close();
    cleanupDir(root);
  });

  test("stores and reports retention deadlines for documents journals and bank transactions", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-retention-status-"));
    const companyRoot = join(root, "company");
    const csv = join(root, "bank.csv");
    const docFile = join(root, "vendor.txt");
    writeFileSync(csv, [
      "transaction_date,booking_date,text,amount,currency,reference",
      "2025-07-16,2025-07-17,Customer payment,1250,DKK,RET-1"
    ].join("\n"));
    writeFileSync(docFile, "Vendor invoice\n");

    const db = openDb(ensureCompanyDirs(companyRoot).db);
    migrate(db);
    seedAccounts(db);
    db.run(`INSERT INTO companies (id, name, cvr, fiscal_year_start_month, fiscal_year_label_strategy) VALUES (1, 'Rentemester ApS', 'DK12345678', 7, 'end-year')`);

    const ingested = ingestDocument(db, companyRoot, docFile, {
      source: "email",
      issueDate: "2025-07-15",
      invoiceNo: "RET-INV-1",
      deliveryDescription: "Bogføring",
      amountIncVat: 1250,
      currency: "DKK",
      sender: { name: "Leverandør ApS", address: "Sælgervej 1", vatOrCvr: "DK11223344" },
      recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      vatAmount: 250,
    });
    expect(ingested.ok).toBe(true);

    const posted = postJournalEntry(db, {
      transactionDate: "2025-07-18",
      text: "Retention expense",
      documentId: ingested.documentId,
      lines: [
        { accountNo: "3000", debitAmount: 1000, vatCode: "DK_PURCHASE_25" },
        { accountNo: "4000", debitAmount: 250 },
        { accountNo: "2000", creditAmount: 1250 },
      ],
    });
    expect(posted.ok).toBe(true);

    const imported = importBankCsv(db, companyRoot, csv);
    expect(imported.ok).toBe(true);

    const retainRows = db.query("SELECT invoice_date, retain_until FROM documents ORDER BY id ASC").all() as any[];
    expect(retainRows.some((row) => row.invoice_date === "2025-07-15" && row.retain_until === "2031-06-30")).toBe(true);
    const journalRow = db.query("SELECT transaction_date, retain_until FROM journal_entries ORDER BY id ASC LIMIT 1").get() as any;
    expect(journalRow).toEqual({ transaction_date: "2025-07-18", retain_until: "2031-06-30" });
    const bankRow = db.query("SELECT booking_date, retain_until FROM bank_transactions ORDER BY id ASC LIMIT 1").get() as any;
    expect(bankRow).toEqual({ booking_date: "2025-07-17", retain_until: "2031-06-30" });

    const report = buildRetentionStatusReport(db, "2031-07-01");
    expect(report.ok).toBe(true);
    expect(report.rows).toEqual([
      { table: "documents", total: 1, expired: 1, nextExpiry: null, oldestExpired: "2031-06-30" },
      { table: "journal_entries", total: 1, expired: 1, nextExpiry: null, oldestExpired: "2031-06-30" },
      { table: "bank_transactions", total: 1, expired: 1, nextExpiry: null, oldestExpired: "2031-06-30" },
    ]);

    db.close();
    cleanupDir(root);
  });
});
