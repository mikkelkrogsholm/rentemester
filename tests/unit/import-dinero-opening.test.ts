// Tests: src/core/import/dinero.ts — opening balance (primobalance) from a
// Dinero export's `<year>/Posteringer.csv` Primobeholdning rows (#194).
//
// A Dinero `<year>/Posteringer.csv` opens with the fiscal year's opening
// balances: rows with `Bilag = 0`, `Tekst = Primobeholdning`, dated the first
// day of the year. `Beløb` is signed (comma decimal) — positive = debit,
// negative = credit — and the balance-sheet rows sum to zero. This is exactly
// the import framework's `ImportSource.openingBalances`; the existing
// `runImport` -> `postOpeningBalance` path posts it.
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
import { runImport, runImportFromSource } from "../../src/core/import/framework";
import { getOpeningBalance, OPENING_BALANCE_TEXT } from "../../src/core/opening-balance";
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

describe("Dinero parser: Posteringer.csv -> opening balances", () => {
  test("produces a non-empty primobalance and a real cut-over date", () => {
    const parsed = dineroParser.parseSource!(resolveSource(FIXTURE));
    expect(parsed.errors).toEqual([]);
    expect(parsed.ok).toBe(true);
    const source = parsed.source!;
    // The cut-over year's fiscal-year start.
    expect(source.cutOverDate).toBe("2025-01-01");
    // One opening-balance line per Primobeholdning row.
    expect(source.openingBalances.length).toBe(8);
    // Every opening-balance line references an account in the chart.
    const chartNos = new Set(source.chartOfAccounts.map((a) => a.accountNo));
    for (const line of source.openingBalances) {
      expect(chartNos.has(line.accountNo)).toBe(true);
    }
  });

  test("applies the sign convention: positive Beløb -> debit, negative -> credit", () => {
    const source = dineroParser.parseSource!(resolveSource(FIXTURE)).source!;
    const by = (no: string) => source.openingBalances.find((l) => l.accountNo === no)!;
    // 5510 Bankkonto: Beløb 88200,50 -> debit, no credit.
    expect(by("5510").debitAmount).toBe(88200.5);
    expect(by("5510").creditAmount).toBeUndefined();
    // 60000 Registreret kapital: Beløb -40000 -> credit (absolute), no debit.
    expect(by("60000").creditAmount).toBe(40000);
    expect(by("60000").debitAmount).toBeUndefined();
  });

  test("the opening balances balance (debits == credits)", () => {
    const source = dineroParser.parseSource!(resolveSource(FIXTURE)).source!;
    let debit = 0;
    let credit = 0;
    for (const line of source.openingBalances) {
      debit += line.debitAmount ?? 0;
      credit += line.creditAmount ?? 0;
    }
    expect(debit).toBeCloseTo(credit, 6);
    expect(debit).toBeCloseTo(195200.5, 6);
  });
});

describe("Dinero import: Posteringer.csv primobalance posted via runImport", () => {
  test("posts a balanced primobalance journal entry at the cut-over date", () => {
    const { root, db } = freshCompany("rentemester-dinero-primo-");
    try {
      const source = dineroParser.parseSource!(resolveSource(FIXTURE)).source!;
      const result = runImport(db, source, { createdBy: "user:tester" });
      expect(result.errors).toEqual([]);
      expect(result.ok).toBe(true);
      expect(result.cutOverDate).toBe("2025-01-01");
      expect(result.openingBalanceLineCount).toBe(8);

      const entry = db
        .query("SELECT text, transaction_date FROM journal_entries WHERE id = ?")
        .get(result.entryId!) as { text: string; transaction_date: string };
      expect(entry.transaction_date).toBe("2025-01-01");
      expect(entry.text.startsWith(OPENING_BALANCE_TEXT)).toBe(true);
      expect(verifyAuditChain(db).ok).toBe(true);

      const marker = getOpeningBalance(db);
      expect(marker!.cutOverDate).toBe("2025-01-01");
    } finally {
      db.close();
      cleanupDir(root);
    }
  });

  test("posts the correct amounts in kroner — not off by 100x", () => {
    const { root, db } = freshCompany("rentemester-dinero-amounts-");
    try {
      const source = dineroParser.parseSource!(resolveSource(FIXTURE)).source!;
      const result = runImport(db, source, { createdBy: "user:tester" });
      expect(result.ok).toBe(true);

      // The journal lines store the kroner amount verbatim (the ledger keeps
      // øre internally only). 88200,50 kr must land as 88200.5 — not 8820050.
      const lineFor = (accountNo: string) =>
        db
          .query(
            `SELECT jl.debit_amount AS debit_amount, jl.credit_amount AS credit_amount
               FROM journal_lines jl
               JOIN accounts a ON a.id = jl.account_id
              WHERE jl.journal_entry_id = ? AND a.account_no = ?`,
          )
          .get(result.entryId!, accountNo) as { debit_amount: number; credit_amount: number };

      const bank = lineFor("5510");
      expect(bank.debit_amount).toBe(88200.5);
      expect(bank.credit_amount).toBe(0);

      const capital = lineFor("60000");
      expect(capital.credit_amount).toBe(40000);
      expect(capital.debit_amount).toBe(0);
    } finally {
      db.close();
      cleanupDir(root);
    }
  });

  test("the trial balance reflects the opening balance from the cut-over date", () => {
    const { root, db } = freshCompany("rentemester-dinero-tb-");
    try {
      const source = dineroParser.parseSource!(resolveSource(FIXTURE)).source!;
      const result = runImport(db, source, { createdBy: "user:tester" });
      expect(result.ok).toBe(true);

      // Scoped to the cut-over date alone so it isolates the primobalance from
      // the year-to-date activity that the same import also posts (#195).
      const tb = buildTrialBalance(db, "2025-01-01", "2025-01-01");
      expect(tb.ok).toBe(true);
      expect(tb.balanced).toBe(true);
      const acct = (no: string) => tb.accounts.find((a) => a.accountNo === no)!;
      // Bankkonto (asset) carries its debit opening balance.
      expect(acct("5510").debit).toBeCloseTo(88200.5, 6);
      // Registreret kapital (equity) carries its credit opening balance.
      expect(acct("60000").credit).toBeCloseTo(40000, 6);
    } finally {
      db.close();
      cleanupDir(root);
    }
  });

  test("import run --system dinero end-to-end posts the primobalance", () => {
    const { root, db } = freshCompany("rentemester-dinero-e2e-");
    try {
      const result = runImportFromSource(db, dineroParser, FIXTURE, {
        createdBy: "user:tester",
      });
      expect(result.errors).toEqual([]);
      expect(result.ok).toBe(true);
      expect(result.entryNo).toBeTruthy();
      expect(getOpeningBalance(db)!.cutOverDate).toBe("2025-01-01");
    } finally {
      db.close();
      cleanupDir(root);
    }
  });

  test("is idempotent: a second Dinero import is rejected", () => {
    const { root, db } = freshCompany("rentemester-dinero-idem-");
    try {
      const first = runImportFromSource(db, dineroParser, FIXTURE, { createdBy: "user:tester" });
      expect(first.ok).toBe(true);
      // First import landed the primobalance + the year-to-date vouchers (#195).
      const afterFirst = db
        .query("SELECT COUNT(*) AS n FROM journal_entries")
        .get() as { n: number };
      expect(afterFirst.n).toBe(6);
      const second = runImportFromSource(db, dineroParser, FIXTURE, { createdBy: "user:tester" });
      expect(second.ok).toBe(false);
      expect(second.errors.join(" ").toLowerCase()).toContain("already");
      // The rejected second import posted nothing further.
      const count = db.query("SELECT COUNT(*) AS n FROM journal_entries").get() as { n: number };
      expect(count.n).toBe(6);
    } finally {
      db.close();
      cleanupDir(root);
    }
  });
});
