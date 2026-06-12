// Tests: src/core/budget.ts
import { describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { seedAccounts } from "../../src/core/ledger";
import {
  setBudget,
  listBudget,
  buildBudgetVsActual,
} from "../../src/core/budget";

function freshDb() {
  const root = mkdtempSync(join(tmpdir(), "rentemester-budget-"));
  const db = openDb(ensureCompanyDirs(root).db);
  migrate(db);
  seedAccounts(db);
  return { root, db };
}

/**
 * Post a minimal balanced journal entry directly so the budget-vs-actual
 * report has real ledger movement to compare against. The chart of accounts
 * is seeded by migrate(): 1000-range bank assets, 1100 income, 2200 expense
 * vary by chart — resolve account ids by type instead of hardcoding numbers.
 */
function postEntry(
  db: ReturnType<typeof openDb>,
  date: string,
  debitAccountNo: string,
  creditAccountNo: string,
  amount: number,
) {
  const accId = (no: string) =>
    (db.query("SELECT id FROM accounts WHERE account_no = ?").get(no) as { id: number }).id;
  const entry = db
    .query(
      `INSERT INTO journal_entries (entry_no, transaction_date, text, rule_version, entry_hash)
       VALUES (?, ?, ?, 'test', 'h') RETURNING id`,
    )
    .get(`E-${date}-${debitAccountNo}-${creditAccountNo}-${amount}`, date, "test entry") as {
    id: number;
  };
  db.run(
    "INSERT INTO journal_lines (journal_entry_id, account_id, debit_amount, credit_amount) VALUES (?, ?, ?, 0)",
    entry.id,
    accId(debitAccountNo),
    amount,
  );
  db.run(
    "INSERT INTO journal_lines (journal_entry_id, account_id, debit_amount, credit_amount) VALUES (?, ?, 0, ?)",
    entry.id,
    accId(creditAccountNo),
    amount,
  );
}

function expenseAccountNo(db: ReturnType<typeof openDb>): string {
  return (db.query("SELECT account_no FROM accounts WHERE type = 'expense' ORDER BY account_no LIMIT 1").get() as {
    account_no: string;
  }).account_no;
}
function incomeAccountNo(db: ReturnType<typeof openDb>): string {
  return (db.query("SELECT account_no FROM accounts WHERE type = 'income' ORDER BY account_no LIMIT 1").get() as {
    account_no: string;
  }).account_no;
}
function bankAccountNo(db: ReturnType<typeof openDb>): string {
  return (db.query("SELECT account_no FROM accounts WHERE type = 'asset' ORDER BY account_no LIMIT 1").get() as {
    account_no: string;
  }).account_no;
}

describe("budget definition", () => {
  test("setBudget rejects an unknown account", () => {
    const { db } = freshDb();
    const res = setBudget(db, { accountNo: "999999", period: "2026-06", amount: 1000 });
    expect(res.ok).toBe(false);
    expect(res.errors.join(" ")).toContain("999999");
    db.close();
  });

  test("setBudget rejects a malformed period", () => {
    const { db } = freshDb();
    const acc = expenseAccountNo(db);
    expect(setBudget(db, { accountNo: acc, period: "2026-13", amount: 100 }).ok).toBe(false);
    expect(setBudget(db, { accountNo: acc, period: "2026", amount: 100 }).ok).toBe(false);
    expect(setBudget(db, { accountNo: acc, period: "2026-06-01", amount: 100 }).ok).toBe(false);
    db.close();
  });

  test("setBudget stores a budget line and listBudget returns it", () => {
    const { db } = freshDb();
    const acc = expenseAccountNo(db);
    const res = setBudget(db, { accountNo: acc, period: "2026-06", amount: 5000 });
    expect(res.ok).toBe(true);

    const listed = listBudget(db);
    expect(listed.ok).toBe(true);
    expect(listed.count).toBe(1);
    expect(listed.rows[0]!.accountNo).toBe(acc);
    expect(listed.rows[0]!.period).toBe("2026-06");
    expect(listed.rows[0]!.amount).toBe(5000);
    db.close();
  });

  test("setBudget is append-only: the latest revision wins per account/period", () => {
    const { db } = freshDb();
    const acc = expenseAccountNo(db);
    expect(setBudget(db, { accountNo: acc, period: "2026-06", amount: 5000 }).ok).toBe(true);
    expect(setBudget(db, { accountNo: acc, period: "2026-06", amount: 7000 }).ok).toBe(true);

    const listed = listBudget(db);
    // listBudget collapses to the effective (latest) line per account/period.
    expect(listed.count).toBe(1);
    expect(listed.rows[0]!.amount).toBe(7000);
    db.close();
  });

  test("listBudget can filter by period", () => {
    const { db } = freshDb();
    const acc = expenseAccountNo(db);
    setBudget(db, { accountNo: acc, period: "2026-06", amount: 1000 });
    setBudget(db, { accountNo: acc, period: "2026-07", amount: 2000 });
    const june = listBudget(db, { period: "2026-06" });
    expect(june.count).toBe(1);
    expect(june.rows[0]!.amount).toBe(1000);
    db.close();
  });
});

describe("budget vs actual", () => {
  test("rejects a malformed period range", () => {
    const { db } = freshDb();
    expect(buildBudgetVsActual(db, "2026-07", "2026-06").ok).toBe(false);
    expect(buildBudgetVsActual(db, "2026-13", "2026-12").ok).toBe(false);
    db.close();
  });

  test("compares a budgeted expense account against ledger actuals", () => {
    const { db } = freshDb();
    const expense = expenseAccountNo(db);
    const bank = bankAccountNo(db);
    setBudget(db, { accountNo: expense, period: "2026-06", amount: 5000 });
    // Two expense postings in June: 3000 + 1000 = 4000 actual.
    postEntry(db, "2026-06-05", expense, bank, 3000);
    postEntry(db, "2026-06-20", expense, bank, 1000);

    const report = buildBudgetVsActual(db, "2026-06", "2026-06");
    expect(report.ok).toBe(true);
    expect(report.periodStart).toBe("2026-06");
    expect(report.periodEnd).toBe("2026-06");
    const line = report.lines.find((l) => l.accountNo === expense && l.period === "2026-06");
    expect(line).toBeDefined();
    expect(line!.budget).toBe(5000);
    expect(line!.actual).toBe(4000);
    // variance = budget - actual for an expense (positive => under budget).
    expect(line!.variance).toBe(1000);
    db.close();
  });

  test("income variance is actual - budget (positive => over target)", () => {
    const { db } = freshDb();
    const income = incomeAccountNo(db);
    const bank = bankAccountNo(db);
    setBudget(db, { accountNo: income, period: "2026-06", amount: 10000 });
    // Income posting: credit income 12000.
    postEntry(db, "2026-06-10", bank, income, 12000);

    const report = buildBudgetVsActual(db, "2026-06", "2026-06");
    const line = report.lines.find((l) => l.accountNo === income)!;
    expect(line.budget).toBe(10000);
    expect(line.actual).toBe(12000);
    // For income a positive variance means beating the target.
    expect(line.variance).toBe(2000);
    db.close();
  });

  test("surfaces an actual with no budget line (budget 0)", () => {
    const { db } = freshDb();
    const expense = expenseAccountNo(db);
    const bank = bankAccountNo(db);
    postEntry(db, "2026-06-05", expense, bank, 2500);
    const report = buildBudgetVsActual(db, "2026-06", "2026-06");
    const line = report.lines.find((l) => l.accountNo === expense)!;
    expect(line.budget).toBe(0);
    expect(line.actual).toBe(2500);
    expect(line.variance).toBe(-2500);
    db.close();
  });

  test("surfaces a budget line with no actual (actual 0)", () => {
    const { db } = freshDb();
    const expense = expenseAccountNo(db);
    setBudget(db, { accountNo: expense, period: "2026-06", amount: 4000 });
    const report = buildBudgetVsActual(db, "2026-06", "2026-06");
    const line = report.lines.find((l) => l.accountNo === expense)!;
    expect(line.budget).toBe(4000);
    expect(line.actual).toBe(0);
    expect(line.variance).toBe(4000);
    db.close();
  });

  test("totals sum budget and actual across the range", () => {
    const { db } = freshDb();
    const expense = expenseAccountNo(db);
    const bank = bankAccountNo(db);
    setBudget(db, { accountNo: expense, period: "2026-06", amount: 5000 });
    setBudget(db, { accountNo: expense, period: "2026-07", amount: 5000 });
    postEntry(db, "2026-06-05", expense, bank, 3000);
    postEntry(db, "2026-07-05", expense, bank, 6000);

    const report = buildBudgetVsActual(db, "2026-06", "2026-07");
    expect(report.totalBudget).toBe(10000);
    expect(report.totalActual).toBe(9000);
    expect(report.lines.length).toBe(2);
    db.close();
  });
});
