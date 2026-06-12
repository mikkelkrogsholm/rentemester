// Tests: src/core/liquidity-forecast.ts
import { describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { seedAccounts } from "../../src/core/ledger";
import { setBudget } from "../../src/core/budget";
import { issueInvoice } from "../../src/core/issued-invoices";
import { createRecurringInvoiceTemplate } from "../../src/core/recurring-invoices";
import { buildLiquidityForecast } from "../../src/core/liquidity-forecast";

function freshDb() {
  const root = mkdtempSync(join(tmpdir(), "rentemester-liquidity-"));
  const db = openDb(ensureCompanyDirs(root).db);
  migrate(db);
  seedAccounts(db);
  return { root, db };
}

function bankAccountNo(db: ReturnType<typeof openDb>): string {
  // The forecast treats `%bank%`-named asset accounts as cash; pick that one
  // so a posting the test makes lands in the account the forecast reads.
  return (db
    .query("SELECT account_no FROM accounts WHERE type = 'asset' AND lower(name) LIKE '%bank%' ORDER BY account_no LIMIT 1")
    .get() as { account_no: string }).account_no;
}
function expenseAccountNo(db: ReturnType<typeof openDb>): string {
  return (db.query("SELECT account_no FROM accounts WHERE type = 'expense' ORDER BY account_no LIMIT 1").get() as {
    account_no: string;
  }).account_no;
}

/** Post a balanced entry directly into the ledger. */
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
    .get(`E-${date}-${debitAccountNo}-${creditAccountNo}-${amount}`, date, "test entry") as { id: number };
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

function invoicePayload(overrides: Record<string, unknown> = {}) {
  return {
    invoiceType: "full" as const,
    vatTreatment: "standard" as const,
    seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
    buyer: { name: "Kunde A/S", address: "Købervej 9" },
    lines: [{ description: "Ydelse", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
    totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
    currency: "DKK",
    ...overrides,
  };
}

describe("liquidity forecast input validation", () => {
  test("rejects a malformed start date", () => {
    const { db } = freshDb();
    expect(buildLiquidityForecast(db, { startDate: "not-a-date", months: 3 }).ok).toBe(false);
    db.close();
  });

  test("rejects a non-positive month count", () => {
    const { db } = freshDb();
    expect(buildLiquidityForecast(db, { startDate: "2026-06-01", months: 0 }).ok).toBe(false);
    expect(buildLiquidityForecast(db, { startDate: "2026-06-01", months: -2 }).ok).toBe(false);
    db.close();
  });
});

describe("liquidity forecast projection", () => {
  test("projects the requested number of monthly periods", () => {
    const { db } = freshDb();
    const result = buildLiquidityForecast(db, { startDate: "2026-06-01", months: 3 });
    expect(result.ok).toBe(true);
    expect(result.periods.length).toBe(3);
    expect(result.periods[0]!.period).toBe("2026-06");
    expect(result.periods[1]!.period).toBe("2026-07");
    expect(result.periods[2]!.period).toBe("2026-08");
    db.close();
  });

  test("opening balance of the first period is the booked bank balance as of the day before start", () => {
    const { db } = freshDb();
    const bank = bankAccountNo(db);
    const expense = expenseAccountNo(db);
    // Booked bank movement before the forecast start: 20000 debited into bank.
    postEntry(db, "2026-05-15", bank, expense, 20000);
    const result = buildLiquidityForecast(db, { startDate: "2026-06-01", months: 1 });
    expect(result.openingBalance).toBe(20000);
    expect(result.periods[0]!.openingBalance).toBe(20000);
    db.close();
  });

  test("each period's closing balance carries into the next opening balance", () => {
    const { db } = freshDb();
    const result = buildLiquidityForecast(db, { startDate: "2026-06-01", months: 3 });
    for (let i = 1; i < result.periods.length; i += 1) {
      expect(result.periods[i]!.openingBalance).toBe(result.periods[i - 1]!.closingBalance);
    }
    db.close();
  });

  test("an open invoice due in a period adds its open balance as an inflow", () => {
    const { root, db } = freshDb();
    // Invoice due 2026-07-15 (issue + 30 days). Not paid => open.
    const issued = issueInvoice(db, root, invoicePayload({ issueDate: "2026-06-15", dueDate: "2026-07-15" }));
    expect(issued.ok).toBe(true);

    const result = buildLiquidityForecast(db, { startDate: "2026-06-01", months: 3 });
    const july = result.periods.find((p) => p.period === "2026-07")!;
    expect(july.invoiceInflow).toBe(1250);
    // The invoice inflow lifts July's closing balance.
    expect(july.closingBalance).toBe(july.openingBalance + 1250);
    db.close();
  });

  test("budgeted expenses reduce the projected balance in their period", () => {
    const { db } = freshDb();
    const expense = expenseAccountNo(db);
    setBudget(db, { accountNo: expense, period: "2026-07", amount: 3000 });
    const result = buildLiquidityForecast(db, { startDate: "2026-06-01", months: 3 });
    const july = result.periods.find((p) => p.period === "2026-07")!;
    expect(july.budgetedCostOutflow).toBe(3000);
    expect(july.closingBalance).toBe(july.openingBalance - 3000);
    db.close();
  });

  test("scheduled recurring invoices add a projected inflow in their period", () => {
    const { db } = freshDb();
    const created = createRecurringInvoiceTemplate(db, {
      name: "Retainer",
      interval: "monthly",
      firstIssueDate: "2026-06-01",
      paymentTermsDays: 30,
      invoice: invoicePayload(),
    });
    expect(created.ok).toBe(true);
    // June + July + August templates each project a 1250 inflow when due.
    const result = buildLiquidityForecast(db, { startDate: "2026-06-01", months: 3 });
    const totalRecurring = result.periods.reduce((s, p) => s + p.recurringInflow, 0);
    expect(totalRecurring).toBe(1250 * 3);
    db.close();
  });

  test("a paid invoice contributes no inflow", () => {
    const { root, db } = freshDb();
    const bank = bankAccountNo(db);
    const income = (db.query("SELECT account_no FROM accounts WHERE type = 'income' ORDER BY account_no LIMIT 1").get() as {
      account_no: string;
    }).account_no;
    const issued = issueInvoice(db, root, invoicePayload({ issueDate: "2026-06-15", dueDate: "2026-07-15" }));
    expect(issued.ok).toBe(true);
    // Settle the invoice fully via a payment so it is no longer open. The
    // payment row needs a journal entry (NOT NULL, UNIQUE FK), so post one.
    const accId = (no: string) =>
      (db.query("SELECT id FROM accounts WHERE account_no = ?").get(no) as { id: number }).id;
    const entry = db
      .query(
        `INSERT INTO journal_entries (entry_no, transaction_date, text, rule_version, entry_hash)
         VALUES ('PAY-1', '2026-06-20', 'payment', 'test', 'h') RETURNING id`,
      )
      .get() as { id: number };
    db.run(
      "INSERT INTO journal_lines (journal_entry_id, account_id, debit_amount, credit_amount) VALUES (?, ?, 1250, 0)",
      entry.id,
      accId(bank),
    );
    db.run(
      "INSERT INTO journal_lines (journal_entry_id, account_id, debit_amount, credit_amount) VALUES (?, ?, 0, 1250)",
      entry.id,
      accId(income),
    );
    db.run(
      `INSERT INTO invoice_payments (invoice_document_id, journal_entry_id, payment_date, amount, currency)
       VALUES (?, ?, '2026-06-20', 1250, 'DKK')`,
      issued.documentId,
      entry.id,
    );
    const result = buildLiquidityForecast(db, { startDate: "2026-06-01", months: 3 });
    const july = result.periods.find((p) => p.period === "2026-07")!;
    expect(july.invoiceInflow).toBe(0);
    db.close();
  });

  test("forecast is deterministic: identical inputs yield identical output", () => {
    const { db } = freshDb();
    const expense = expenseAccountNo(db);
    setBudget(db, { accountNo: expense, period: "2026-07", amount: 3000 });
    const a = buildLiquidityForecast(db, { startDate: "2026-06-01", months: 4 });
    const b = buildLiquidityForecast(db, { startDate: "2026-06-01", months: 4 });
    expect(JSON.stringify(a)).toBe(JSON.stringify(b));
    db.close();
  });
});
