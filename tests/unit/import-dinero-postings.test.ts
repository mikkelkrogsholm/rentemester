// Tests: src/core/import/dinero-postings.ts — year-to-date postings replayed
// from a Dinero export's cut-over-year `Posteringer.csv` (#195).
//
// After the opening balance (#194) the company sits at the cut-over date. The
// cut-over year's `Posteringer.csv` also holds the year-to-date activity: every
// non-Primobeholdning row. #195 groups those rows by `Bilag` (voucher) into
// balanced journal entries and replays them into the live ledger, marked as
// imported migration postings.
//
// Tests run against the synthetic fixture in examples/import-dinero/ — the real
// Dinero export is private and is never committed.
import { describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { seedAccounts, verifyAuditChain } from "../../src/core/ledger";
import { resolveSource } from "../../src/core/import/source";
import { dineroParser } from "../../src/core/import/dinero";
import {
  parseDineroPostings,
  IMPORT_POSTINGS_PROGRAM,
} from "../../src/core/import/dinero-postings";
import { runImport, runImportFromSource } from "../../src/core/import/framework";
import { buildTrialBalance } from "../../src/core/financial-statements";

import { cleanupDir } from "../helpers/cleanup";
const FIXTURE = join(import.meta.dir, "../../examples/import-dinero");

function freshCompany(prefix: string) {
  const root = mkdtempSync(join(tmpdir(), prefix));
  const db = openDb(ensureCompanyDirs(root).db);
  migrate(db);
  seedAccounts(db);
  return { root, db };
}

function posteringerText() {
  return resolveSource(FIXTURE).files["2025/Posteringer.csv"]!.text;
}

describe("Dinero parser: Posteringer.csv -> year-to-date vouchers", () => {
  test("groups non-primo rows by Bilag into balanced vouchers", () => {
    const errors: string[] = [];
    const vouchers = parseDineroPostings(posteringerText(), "Posteringer.csv", errors);
    expect(errors).toEqual([]);
    // The fixture has five non-primo Bilag numbers (1..5).
    expect(vouchers.map((v) => v.bilag)).toEqual(["1", "2", "3", "4", "5"]);
    // Every voucher balances (debits == credits, kroner).
    for (const voucher of vouchers) {
      let debit = 0;
      let credit = 0;
      for (const line of voucher.lines) {
        debit += line.debitAmount ?? 0;
        credit += line.creditAmount ?? 0;
      }
      expect(debit).toBeCloseTo(credit, 6);
      expect(voucher.lines.length).toBeGreaterThanOrEqual(2);
    }
  });

  test("skips the Primobeholdning rows — those are #194's job", () => {
    const vouchers = parseDineroPostings(posteringerText(), "Posteringer.csv", []);
    // Bilag 0 is never produced.
    expect(vouchers.find((v) => v.bilag === "0")).toBeUndefined();
  });

  test("applies the sign convention and carries Dato/Bilagstype/Momstype", () => {
    const vouchers = parseDineroPostings(posteringerText(), "Posteringer.csv", []);
    const bilag1 = vouchers.find((v) => v.bilag === "1")!;
    expect(bilag1.transactionDate).toBe("2025-02-10");
    expect(bilag1.voucherType).toBe("Køb");
    // 3000 Vareforbrug: Beløb 5000 -> debit, in kroner (not 500000).
    const debitLine = bilag1.lines.find((l) => l.accountNo === "3000")!;
    expect(debitLine.debitAmount).toBe(5000);
    expect(debitLine.creditAmount).toBeUndefined();
    expect(debitLine.vatCode).toContain("I25");
    // 5510 Bankkonto: Beløb -5000 -> credit (absolute value).
    const creditLine = bilag1.lines.find((l) => l.accountNo === "5510")!;
    expect(creditLine.creditAmount).toBe(5000);
    expect(creditLine.debitAmount).toBeUndefined();
  });
});

describe("Dinero import: year-to-date postings via runImport", () => {
  test("the parsed source carries the year-to-date vouchers as historicalEntries", () => {
    const source = dineroParser.parseSource!(resolveSource(FIXTURE)).source!;
    expect(source.historicalEntries).toBeDefined();
    expect(source.historicalEntries!.length).toBe(5);
    expect(source.historicalEntries!.map((e) => e.voucherRef)).toEqual([
      "1",
      "2",
      "3",
      "4",
      "5",
    ]);
  });

  test("replays the vouchers as journal entries after the primobalance", () => {
    const { root, db } = freshCompany("rentemester-dinero-ytd-");
    try {
      const source = dineroParser.parseSource!(resolveSource(FIXTURE)).source!;
      const result = runImport(db, source, { createdBy: "user:tester" });
      expect(result.errors).toEqual([]);
      expect(result.ok).toBe(true);
      // The primobalance + five year-to-date vouchers.
      const count = db.query("SELECT COUNT(*) AS n FROM journal_entries").get() as { n: number };
      expect(count.n).toBe(6);
      expect(result.historicalEntriesPosted!.length).toBe(5);
      expect(result.historicalEntriesSkipped).toBe(0);
      expect(verifyAuditChain(db).ok).toBe(true);
    } finally {
      db.close();
      cleanupDir(root);
    }
  });

  test("posted entries are marked as imported migration postings", () => {
    const { root, db } = freshCompany("rentemester-dinero-ytd-mark-");
    try {
      const source = dineroParser.parseSource!(resolveSource(FIXTURE)).source!;
      const result = runImport(db, source, { createdBy: "user:tester" });
      expect(result.ok).toBe(true);
      for (const posted of result.historicalEntriesPosted!) {
        const entry = db
          .query("SELECT created_by_program, text FROM journal_entries WHERE id = ?")
          .get(posted.entryId) as { created_by_program: string; text: string };
        expect(entry.created_by_program).toBe(IMPORT_POSTINGS_PROGRAM);
        expect(entry.text).toContain("Import:");
        expect(entry.text).toContain(`bilag ${posted.voucherRef}`);
      }
    } finally {
      db.close();
      cleanupDir(root);
    }
  });

  test("posts amounts in kroner — not 100x off", () => {
    const { root, db } = freshCompany("rentemester-dinero-ytd-amount-");
    try {
      const source = dineroParser.parseSource!(resolveSource(FIXTURE)).source!;
      const result = runImport(db, source, { createdBy: "user:tester" });
      expect(result.ok).toBe(true);
      // Bilag 2: customer payment of 30000,00 kr -> bank debit 30000, not 3000000.
      const bilag2 = result.historicalEntriesPosted!.find((p) => p.voucherRef === "2")!;
      const bankLine = db
        .query(
          `SELECT jl.debit_amount AS debit, jl.credit_amount AS credit
             FROM journal_lines jl
             JOIN accounts a ON a.id = jl.account_id
            WHERE jl.journal_entry_id = ? AND a.account_no = ?`,
        )
        .get(bilag2.entryId, "5510") as { debit: number; credit: number };
      expect(bankLine.debit).toBe(30000);
      expect(bankLine.credit).toBe(0);
    } finally {
      db.close();
      cleanupDir(root);
    }
  });

  test("the trial balance reflects opening balance plus year-to-date activity", () => {
    const { root, db } = freshCompany("rentemester-dinero-ytd-tb-");
    try {
      const source = dineroParser.parseSource!(resolveSource(FIXTURE)).source!;
      const result = runImport(db, source, { createdBy: "user:tester" });
      expect(result.ok).toBe(true);
      const tb = buildTrialBalance(db, "2025-01-01", "2025-12-31");
      expect(tb.ok).toBe(true);
      expect(tb.balanced).toBe(true);
      const acct = (no: string) => tb.accounts.find((a) => a.accountNo === no)!;
      // Bankkonto 5510: primo 88200,50 - 5000 (b1) + 30000 (b2) - 2400 (b4)
      //                 - 8000 (b5) = 102800,50 net debit.
      const bank = acct("5510");
      expect(bank.debit - bank.credit).toBeCloseTo(102800.5, 6);
      // Vareforbrug 3000: 5000 expense debit from voucher 1.
      expect(acct("3000").debit - acct("3000").credit).toBeCloseTo(5000, 6);
      // Salg af ydelser 1000: 12500 income credit from voucher 3.
      expect(acct("1000").credit - acct("1000").debit).toBeCloseTo(12500, 6);
    } finally {
      db.close();
      cleanupDir(root);
    }
  });

  test("end-to-end runImportFromSource posts primobalance and year-to-date", () => {
    const { root, db } = freshCompany("rentemester-dinero-ytd-e2e-");
    try {
      const result = runImportFromSource(db, dineroParser, FIXTURE, {
        createdBy: "user:tester",
      });
      expect(result.errors).toEqual([]);
      expect(result.ok).toBe(true);
      expect(result.historicalEntriesPosted!.length).toBe(5);
      const count = db.query("SELECT COUNT(*) AS n FROM journal_entries").get() as { n: number };
      expect(count.n).toBe(6);
      expect(verifyAuditChain(db).ok).toBe(true);
    } finally {
      db.close();
      cleanupDir(root);
    }
  });

  test("rejects the whole batch when a voucher does not balance", () => {
    const { root, db } = freshCompany("rentemester-dinero-ytd-unbal-");
    try {
      const source = dineroParser.parseSource!(resolveSource(FIXTURE)).source!;
      // Corrupt one voucher so its debits != credits.
      source.historicalEntries![0]!.lines[0]!.debitAmount = 9999;
      const result = runImport(db, source, { createdBy: "user:tester" });
      expect(result.ok).toBe(false);
      expect(result.errors.join(" ").toLowerCase()).toContain("balance");
      // Nothing posted — not even the primobalance.
      const count = db.query("SELECT COUNT(*) AS n FROM journal_entries").get() as { n: number };
      expect(count.n).toBe(0);
    } finally {
      db.close();
      cleanupDir(root);
    }
  });
});
