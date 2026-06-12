// Tests: src/core/ledger.ts (journal entry validation, balancing, entry numbering, transactions)
// Companion of journal-post-fx.test.ts and ledger-hardening.test.ts.
import { describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { postJournalEntry, reverseJournalEntry, seedAccounts } from "../../src/core/ledger";

import { cleanupDir } from "../helpers/cleanup";
function failingJournalInsertDb(realDb: any) {
  return new Proxy(realDb, {
    get(target, prop, receiver) {
      if (prop === "query") {
        return (sql: string) => {
          const statement = target.query(sql);
          if (sql.includes("INSERT INTO journal_entries")) {
            return { get() { throw new Error("simulated journal insert failure"); } };
          }
          return statement;
        };
      }
      const value = Reflect.get(target, prop, receiver);
      return typeof value === "function" ? value.bind(target) : value;
    },
  }) as any;
}

describe("journal posting", () => {
  test("rejects unbalanced journal entries", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-journal-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    const result = postJournalEntry(db, {
      transactionDate: "2026-05-16",
      text: "Broken posting",
      lines: [
        { accountNo: "2000", debitAmount: 1000 },
        { accountNo: "5000", creditAmount: 900 }
      ]
    });

    expect(result.ok).toBe(false);
    expect(result.errors).toContain("journal entry must balance: debit 1000 != credit 900");

    db.close();
    cleanupDir(root);
  });

  test("rejects journal lines with negative debit or credit amounts", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-journal-negative-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    const result = postJournalEntry(db, {
      transactionDate: "2026-05-16",
      text: "Negative-amount posting",
      lines: [
        { accountNo: "2000", debitAmount: -500 },
        { accountNo: "5000", creditAmount: -500 }
      ]
    });

    expect(result.ok).toBe(false);
    expect(result.errors.some((error) => error.includes("must not be negative"))).toBe(true);

    const positive = postJournalEntry(db, {
      transactionDate: "2026-05-16",
      text: "Valid positive posting",
      lines: [
        { accountNo: "2000", debitAmount: 500 },
        { accountNo: "5000", creditAmount: 500 }
      ]
    });
    expect(positive.ok).toBe(true);

    db.close();
    cleanupDir(root);
  });

  test("numbers journal entries from transaction year and resets per year", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-journal-entryno-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    const first2024 = postJournalEntry(db, {
      transactionDate: "2024-12-31",
      text: "Year-end entry",
      lines: [
        { accountNo: "2000", debitAmount: 1000 },
        { accountNo: "5000", creditAmount: 1000 }
      ]
    });
    const second2024 = postJournalEntry(db, {
      transactionDate: "2024-01-01",
      text: "Opening correction",
      lines: [
        { accountNo: "2000", debitAmount: 500 },
        { accountNo: "5000", creditAmount: 500 }
      ]
    });
    const first2025 = postJournalEntry(db, {
      transactionDate: "2025-01-01",
      text: "New year entry",
      lines: [
        { accountNo: "2000", debitAmount: 750 },
        { accountNo: "5000", creditAmount: 750 }
      ]
    });

    expect(first2024.entryNo).toBe("2024-00001");
    expect(second2024.entryNo).toBe("2024-00002");
    expect(first2025.entryNo).toBe("2025-00001");

    db.close();
    cleanupDir(root);
  });

  test("uses configured fiscal year labels for journal entry numbers", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-journal-fiscal-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);
    db.run(
      `INSERT INTO companies (id, name, cvr, fiscal_year_start_month, fiscal_year_label_strategy)
       VALUES (1, 'Rentemester ApS', 'DK12345678', 7, 'end-year')`
    );

    const first = postJournalEntry(db, {
      transactionDate: "2024-07-01",
      text: "Opening fiscal entry",
      lines: [
        { accountNo: "2000", debitAmount: 1000 },
        { accountNo: "5000", creditAmount: 1000 }
      ]
    });
    const second = postJournalEntry(db, {
      transactionDate: "2025-06-30",
      text: "Fiscal year close",
      lines: [
        { accountNo: "2000", debitAmount: 500 },
        { accountNo: "5000", creditAmount: 500 }
      ]
    });
    const next = postJournalEntry(db, {
      transactionDate: "2025-07-01",
      text: "Next fiscal year",
      lines: [
        { accountNo: "2000", debitAmount: 750 },
        { accountNo: "5000", creditAmount: 750 }
      ]
    });

    expect(first.entryNo).toBe("2025-00001");
    expect(second.entryNo).toBe("2025-00002");
    expect(next.entryNo).toBe("2026-00001");

    db.close();
    cleanupDir(root);
  });

  test("respects the highest existing journal number when a stale sequence row lags behind", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-journal-stale-seq-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    const bank = db.query("SELECT id FROM accounts WHERE account_no = '2000'").get() as { id: number };
    const equity = db.query("SELECT id FROM accounts WHERE account_no = '5000'").get() as { id: number };
    db.run(
      `INSERT INTO journal_entries (
        id, entry_no, transaction_date, text, rule_version, created_by, created_by_program, status, previous_hash, entry_hash, retain_until
      ) VALUES (1, '2026-00005', '2026-05-15', 'Legacy imported entry', 'legacy-import', 'legacy', 'restore', 'posted', 'GENESIS', 'legacy-hash', '2031-12-31')`
    );
    db.run(`INSERT INTO journal_lines (journal_entry_id, account_id, debit_amount, credit_amount, currency, text) VALUES (1, ?, 1000, 0, 'DKK', 'legacy debit')`, bank.id);
    db.run(`INSERT INTO journal_lines (journal_entry_id, account_id, debit_amount, credit_amount, currency, text) VALUES (1, ?, 0, 1000, 'DKK', 'legacy credit')`, equity.id);
    db.run(`INSERT INTO sequences (kind, scope, value) VALUES ('journal_entry', 'company-1:2026', 1)`);

    const posted = postJournalEntry(db, {
      transactionDate: "2026-05-16",
      text: "Entry after stale restore sequence",
      lines: [
        { accountNo: "2000", debitAmount: 500 },
        { accountNo: "5000", creditAmount: 500 }
      ]
    });

    expect(posted.ok).toBe(true);
    expect(posted.entryNo).toBe("2026-00006");

    db.close();
    cleanupDir(root);
  });

  test("does not burn a journal number when insert fails after allocation", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-journal-rollback-seq-"));
    const realDb = openDb(ensureCompanyDirs(root).db);
    migrate(realDb);
    seedAccounts(realDb);
    const failingDb = failingJournalInsertDb(realDb);

    expect(() => postJournalEntry(failingDb, {
      transactionDate: "2026-05-16",
      text: "Should roll back sequence",
      lines: [
        { accountNo: "2000", debitAmount: 1000 },
        { accountNo: "5000", creditAmount: 1000 }
      ]
    })).toThrow("simulated journal insert failure");

    const sequence = realDb.query("SELECT value FROM sequences WHERE kind = 'journal_entry' AND scope = 'company-1:2026'").get() as { value: number } | null;
    expect(sequence).toBeNull();

    const posted = postJournalEntry(realDb, {
      transactionDate: "2026-05-16",
      text: "First surviving entry",
      lines: [
        { accountNo: "2000", debitAmount: 1000 },
        { accountNo: "5000", creditAmount: 1000 }
      ]
    });
    expect(posted.ok).toBe(true);
    expect(posted.entryNo).toBe("2026-00001");

    realDb.close();
    cleanupDir(root);
  });

  test("uses immediate transactions for journal writes and reversals", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-journal-immediate-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    const seenOptions: any[] = [];
    const instrumentedDb = new Proxy(db, {
      get(target, prop, receiver) {
        if (prop === "transaction") {
          return (fn: (...args: any[]) => any, options?: any) => {
            seenOptions.push(options ?? null);
            return target.transaction(fn, options);
          };
        }
        const value = Reflect.get(target, prop, receiver);
        return typeof value === "function" ? value.bind(target) : value;
      },
    }) as any;

    const posted = postJournalEntry(instrumentedDb, {
      transactionDate: "2026-05-16",
      text: "Immediate transaction proof",
      lines: [
        { accountNo: "2000", debitAmount: 1000 },
        { accountNo: "5000", creditAmount: 1000 }
      ]
    });
    expect(posted.ok).toBe(true);

    const reversed = reverseJournalEntry(instrumentedDb, {
      entryId: posted.entryId!,
      transactionDate: "2026-05-17",
      reason: "Proof"
    });
    expect(reversed.ok).toBe(true);
    expect(seenOptions.filter((options) => options?.immediate === true)).toHaveLength(2);

    db.close();
    cleanupDir(root);
  });
});
