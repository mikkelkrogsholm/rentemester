// Tests: src/core/accruals.ts (periodeafgrænsningsposter — prepaid expense,
// accrued expense, deferred revenue, multi-period recognition schedule).
import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { seedAccounts } from "../../src/core/ledger";
import { verifyAuditChain } from "../../src/core/ledger";
import { sumDkk } from "../../src/core/money";
import { ingestDocument } from "../../src/core/documents";
import {
  registerAccrual,
  recognizeAccrualPeriod,
  computeAccrualSchedule,
  buildAccrualRegisterReport,
  listDueAccrualRecognitionPeriods,
} from "../../src/core/accruals";

function setup(label: string) {
  const root = mkdtempSync(join(tmpdir(), `rentemester-${label}-`));
  const inbox = mkdtempSync(join(tmpdir(), `rentemester-${label}-inbox-`));
  const sourceFile = join(inbox, "accrual.txt");
  writeFileSync(sourceFile, `Accrual invoice ${label}\n`);
  const db = openDb(ensureCompanyDirs(root).db);
  migrate(db);
  seedAccounts(db);
  const doc = ingestDocument(db, root, sourceFile, {
    source: "email",
    issueDate: "2026-01-05",
    invoiceNo: `ACC-${label}`,
    deliveryDescription: "Forsikring helår",
    amountIncVat: 12000,
    currency: "DKK",
    sender: { name: "Forsikring ApS", address: "Vej 1", vatOrCvr: "DK11223344" },
    recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
    vatAmount: 0,
    paymentDetails: "Bank transfer",
  });
  expect(doc.ok).toBe(true);
  const cleanup = () => {
    db.close();
    rmSync(root, { recursive: true, force: true });
    rmSync(inbox, { recursive: true, force: true });
  };
  return { root, db, documentId: doc.documentId!, cleanup };
}

function lines(db: any, entryId: number) {
  return db.query(
    `SELECT a.account_no, jl.debit_amount, jl.credit_amount
     FROM journal_lines jl JOIN accounts a ON a.id = jl.account_id
     WHERE jl.journal_entry_id = ? ORDER BY jl.id ASC`,
  ).all(entryId);
}

describe("accrual recognition schedule", () => {
  test("splits the amount evenly with the remainder on the final period", () => {
    const schedule = computeAccrualSchedule({
      totalAmount: 12000,
      recognitionPeriods: 12,
      firstRecognitionDate: "2026-01-31",
    });
    expect(schedule.length).toBe(12);
    expect(sumDkk(schedule.map((p) => p.amount))).toBe(12000);
    expect(schedule[0].amount).toBe(1000);
    expect(schedule[11].amount).toBe(1000);
  });

  test("the final period carries the øre rounding remainder so the total reconciles exactly", () => {
    // 10000 / 3 = 3333.333... — each øre matters.
    const schedule = computeAccrualSchedule({
      totalAmount: 10000,
      recognitionPeriods: 3,
      firstRecognitionDate: "2026-01-15",
    });
    expect(schedule.map((p) => p.amount)).toEqual([3333.33, 3333.33, 3333.34]);
    expect(sumDkk(schedule.map((p) => p.amount))).toBe(10000);
  });

  test("steps the recognition date by whole months, clamping to month-end", () => {
    const schedule = computeAccrualSchedule({
      totalAmount: 300,
      recognitionPeriods: 3,
      firstRecognitionDate: "2026-01-31",
    });
    expect(schedule.map((p) => p.recognitionDate)).toEqual([
      "2026-01-31",
      "2026-02-28", // clamped — February has no 31st
      "2026-03-31",
    ]);
  });

  test("supports a multi-month period step (e.g. quarterly recognition)", () => {
    const schedule = computeAccrualSchedule({
      totalAmount: 900,
      recognitionPeriods: 4,
      firstRecognitionDate: "2026-01-01",
      periodStepMonths: 3,
    });
    expect(schedule.map((p) => p.recognitionDate)).toEqual([
      "2026-01-01",
      "2026-04-01",
      "2026-07-01",
      "2026-10-01",
    ]);
  });

  test("is deterministic on repeated calls", () => {
    const args = { totalAmount: 7777.77, recognitionPeriods: 7, firstRecognitionDate: "2026-03-10" };
    expect(computeAccrualSchedule(args)).toEqual(computeAccrualSchedule(args));
  });
});

describe("prepaid expense accrual (forudbetalt omkostning)", () => {
  test("registration parks the cost on the asset account; recognition releases it to expense", () => {
    const { db, documentId, cleanup } = setup("accrual-prepaid");
    const reg = registerAccrual(db, {
      accrualType: "prepaid_expense",
      description: "Årsforsikring 2026",
      totalAmount: 12000,
      recognitionPeriods: 12,
      firstRecognitionDate: "2026-01-31",
      registrationDate: "2026-01-05",
      resultAccountNo: "3150", // Forsikringer (expense)
      documentId,
    });
    expect(reg.ok).toBe(true);
    expect(reg.accrualId).toBeGreaterThan(0);
    expect(reg.totalPeriods).toBe(12);
    expect(reg.periodAmount).toBe(1000);

    // Registration: debit 1300 (asset), credit 2000 (bank).
    expect(lines(db, reg.entryId!)).toEqual([
      { account_no: "1300", debit_amount: 12000, credit_amount: 0 },
      { account_no: "2000", debit_amount: 0, credit_amount: 12000 },
    ]);

    const period1 = recognizeAccrualPeriod(db, { accrualId: reg.accrualId!, periodIndex: 1 });
    expect(period1.ok).toBe(true);
    expect(period1.periodAmount).toBe(1000);
    expect(period1.recognizedPeriods).toBe(1);
    expect(period1.fullyRecognized).toBe(false);
    // Recognition: debit 3150 (expense), credit 1300 (release prepaid asset).
    expect(lines(db, period1.entryId!)).toEqual([
      { account_no: "3150", debit_amount: 1000, credit_amount: 0 },
      { account_no: "1300", debit_amount: 0, credit_amount: 1000 },
    ]);
    cleanup();
  });

  test("recognising every period unwinds the asset to zero and reconciles to the registered amount", () => {
    const { db, documentId, cleanup } = setup("accrual-prepaid-full");
    const reg = registerAccrual(db, {
      accrualType: "prepaid_expense",
      description: "Husleje forud kvartal",
      totalAmount: 10000,
      recognitionPeriods: 3,
      firstRecognitionDate: "2026-01-31",
      registrationDate: "2026-01-05",
      resultAccountNo: "3100", // Husleje (expense)
      documentId,
    });
    expect(reg.ok).toBe(true);

    let last: any;
    for (let p = 1; p <= 3; p += 1) {
      last = recognizeAccrualPeriod(db, { accrualId: reg.accrualId!, periodIndex: p });
      expect(last.ok).toBe(true);
    }
    expect(last.fullyRecognized).toBe(true);
    expect(last.recognizedPeriods).toBe(3);
    // Last period carries the remainder: 10000 / 3 → 3333.34 on period 3.
    expect(last.periodAmount).toBe(3333.34);

    // The prepaid-asset account 1300 nets to exactly zero after full recognition.
    const balance = db.query(
      `SELECT COALESCE(SUM(jl.debit_amount),0) - COALESCE(SUM(jl.credit_amount),0) AS net
       FROM journal_lines jl JOIN accounts a ON a.id = jl.account_id
       WHERE a.account_no = '1300'`,
    ).get() as { net: number };
    expect(balance.net).toBe(0);

    const report = buildAccrualRegisterReport(db);
    const row = report.accruals.find((a) => a.accrualId === reg.accrualId);
    expect(row!.recognizedAmount).toBe(10000);
    expect(row!.remainingAmount).toBe(0);
    expect(row!.fullyRecognized).toBe(true);
    cleanup();
  });
});

describe("accrued expense accrual (skyldig omkostning)", () => {
  test("registration recognises the expense and parks a liability; recognition settles it", () => {
    const { db, documentId, cleanup } = setup("accrual-accrued");
    const reg = registerAccrual(db, {
      accrualType: "accrued_expense",
      description: "Skyldig revisorhonorar",
      totalAmount: 8000,
      recognitionPeriods: 2,
      firstRecognitionDate: "2026-02-28",
      registrationDate: "2026-01-31",
      resultAccountNo: "3160", // Revisor og bogføring (expense)
      documentId,
    });
    expect(reg.ok).toBe(true);
    // Registration: debit 3160 (expense recognised now), credit 7300 (liability).
    expect(lines(db, reg.entryId!)).toEqual([
      { account_no: "3160", debit_amount: 8000, credit_amount: 0 },
      { account_no: "7300", debit_amount: 0, credit_amount: 8000 },
    ]);

    const period1 = recognizeAccrualPeriod(db, { accrualId: reg.accrualId!, periodIndex: 1 });
    expect(period1.ok).toBe(true);
    // Recognition: debit 7300 (settle liability), credit 2000 (payment).
    expect(lines(db, period1.entryId!)).toEqual([
      { account_no: "7300", debit_amount: 4000, credit_amount: 0 },
      { account_no: "2000", debit_amount: 0, credit_amount: 4000 },
    ]);
    cleanup();
  });
});

describe("deferred revenue accrual (forudbetalt indtægt)", () => {
  test("registration parks cash as a liability; recognition releases it to income", () => {
    const { db, documentId, cleanup } = setup("accrual-deferred");
    const reg = registerAccrual(db, {
      accrualType: "deferred_revenue",
      description: "Forudbetalt support-aftale",
      totalAmount: 6000,
      recognitionPeriods: 6,
      firstRecognitionDate: "2026-01-31",
      registrationDate: "2026-01-02",
      resultAccountNo: "1000", // Omsætning, ydelser (income)
      documentId,
    });
    expect(reg.ok).toBe(true);
    // Registration: debit 2000 (cash received), credit 7310 (deferred-revenue liability).
    expect(lines(db, reg.entryId!)).toEqual([
      { account_no: "2000", debit_amount: 6000, credit_amount: 0 },
      { account_no: "7310", debit_amount: 0, credit_amount: 6000 },
    ]);

    const period1 = recognizeAccrualPeriod(db, { accrualId: reg.accrualId!, periodIndex: 1 });
    expect(period1.ok).toBe(true);
    // Recognition: debit 7310 (release liability), credit 1000 (income earned).
    expect(lines(db, period1.entryId!)).toEqual([
      { account_no: "7310", debit_amount: 1000, credit_amount: 0 },
      { account_no: "1000", debit_amount: 0, credit_amount: 1000 },
    ]);
    cleanup();
  });
});

describe("accrual guardrails", () => {
  test("blocks recognising the same period twice", () => {
    const { db, documentId, cleanup } = setup("accrual-dup");
    const reg = registerAccrual(db, {
      accrualType: "prepaid_expense",
      description: "Dobbelt",
      totalAmount: 1200,
      recognitionPeriods: 12,
      firstRecognitionDate: "2026-01-31",
      registrationDate: "2026-01-05",
      resultAccountNo: "3150",
      documentId,
    });
    expect(reg.ok).toBe(true);
    const first = recognizeAccrualPeriod(db, { accrualId: reg.accrualId!, periodIndex: 1 });
    expect(first.ok).toBe(true);
    const dup = recognizeAccrualPeriod(db, { accrualId: reg.accrualId!, periodIndex: 1 });
    expect(dup.ok).toBe(false);
    expect(dup.errors.join(" ")).toContain("already recognized");
    expect(dup.entryId).toBeUndefined();
    cleanup();
  });

  test("rejects a period index outside the schedule", () => {
    const { db, documentId, cleanup } = setup("accrual-range");
    const reg = registerAccrual(db, {
      accrualType: "prepaid_expense",
      description: "Udenfor",
      totalAmount: 600,
      recognitionPeriods: 6,
      firstRecognitionDate: "2026-01-31",
      registrationDate: "2026-01-05",
      resultAccountNo: "3150",
      documentId,
    });
    expect(reg.ok).toBe(true);
    const tooHigh = recognizeAccrualPeriod(db, { accrualId: reg.accrualId!, periodIndex: 7 });
    expect(tooHigh.ok).toBe(false);
    expect(tooHigh.errors.join(" ")).toContain("outside the accrual schedule");
    cleanup();
  });

  test("rejects a result account whose type contradicts the accrual type", () => {
    const { db, documentId, cleanup } = setup("accrual-acctype");
    // deferred_revenue must recognise on an income account, not an expense one.
    const bad = registerAccrual(db, {
      accrualType: "deferred_revenue",
      description: "Forkert konto",
      totalAmount: 1000,
      recognitionPeriods: 2,
      firstRecognitionDate: "2026-01-31",
      resultAccountNo: "3150", // expense — wrong for deferred revenue
      documentId,
    });
    expect(bad.ok).toBe(false);
    expect(bad.errors.join(" ")).toContain("income account");
    cleanup();
  });

  test("requires document evidence because the registration touches an expense account", () => {
    const { db, cleanup } = setup("accrual-nodoc");
    const noDoc = registerAccrual(db, {
      accrualType: "prepaid_expense",
      description: "Uden bilag",
      totalAmount: 1000,
      recognitionPeriods: 2,
      firstRecognitionDate: "2026-01-31",
      resultAccountNo: "3150",
      // documentId omitted — the prepaid registration debits an asset only, so
      // the registration entry itself does not require a document; but the
      // recognition entry hits an expense account, which does.
    });
    // Prepaid registration (asset + bank) needs no document and succeeds.
    expect(noDoc.ok).toBe(true);
    const rec = recognizeAccrualPeriod(db, { accrualId: noDoc.accrualId!, periodIndex: 1 });
    expect(rec.ok).toBe(false);
    expect(rec.errors.join(" ")).toContain("documentId is required");
    cleanup();
  });

  test("the whole accrual lifecycle leaves the audit chain valid", () => {
    const { db, documentId, cleanup } = setup("accrual-audit");
    const reg = registerAccrual(db, {
      accrualType: "prepaid_expense",
      description: "Audit-kæde",
      totalAmount: 9000,
      recognitionPeriods: 3,
      firstRecognitionDate: "2026-01-31",
      registrationDate: "2026-01-05",
      resultAccountNo: "3150",
      documentId,
    });
    expect(reg.ok).toBe(true);
    for (let p = 1; p <= 3; p += 1) {
      expect(recognizeAccrualPeriod(db, { accrualId: reg.accrualId!, periodIndex: p }).ok).toBe(true);
    }
    const verify = verifyAuditChain(db);
    expect(verify.ok).toBe(true);
    expect(verify.errors).toEqual([]);
    cleanup();
  });

  test("lists the recognition periods that are due/overdue and not yet posted", () => {
    const { db, documentId, cleanup } = setup("accrual-due");
    // 3-month prepaid expense, recognised on the last of Jan/Feb/Mar 2026.
    const reg = registerAccrual(db, {
      accrualType: "prepaid_expense",
      description: "Forsikring Q1",
      totalAmount: 9000,
      recognitionPeriods: 3,
      firstRecognitionDate: "2026-01-31",
      registrationDate: "2026-01-05",
      resultAccountNo: "3150",
      documentId,
    });
    expect(reg.ok).toBe(true);

    // As of 2026-02-15: period 1 (31-01) is due, period 2 (28-02) is not yet,
    // period 3 (31-03) is in the future — only period 1 must surface.
    const due = listDueAccrualRecognitionPeriods(db, "2026-02-15");
    expect(due.ok).toBe(true);
    expect(due.periods.length).toBe(1);
    expect(due.periods[0]!.accrualId).toBe(reg.accrualId!);
    expect(due.periods[0]!.periodIndex).toBe(1);
    expect(due.periods[0]!.recognitionDate).toBe("2026-01-31");
    expect(due.periods[0]!.amount).toBe(3000);
    expect(due.totalDueAmount).toBe(3000);

    // Post period 1; now nothing is due as of the same date (idempotent).
    expect(recognizeAccrualPeriod(db, { accrualId: reg.accrualId!, periodIndex: 1 }).ok).toBe(true);
    const afterPost = listDueAccrualRecognitionPeriods(db, "2026-02-15");
    expect(afterPost.periods.length).toBe(0);

    // As of 2026-04-30 the remaining periods 2 and 3 are both overdue.
    const allDue = listDueAccrualRecognitionPeriods(db, "2026-04-30");
    expect(allDue.periods.length).toBe(2);
    expect(allDue.periods.map((p) => p.periodIndex)).toEqual([2, 3]);
    // Period 2 is 61 days overdue (28-02 → 30-04), surfaced as overdue.
    expect(allDue.periods[0]!.overdueDays).toBeGreaterThan(0);
    expect(allDue.totalDueAmount).toBe(6000);
    cleanup();
  });

  test("accrual rows are append-only — a raw UPDATE is rejected", () => {
    const { db, documentId, cleanup } = setup("accrual-immutable");
    const reg = registerAccrual(db, {
      accrualType: "prepaid_expense",
      description: "Immutabel",
      totalAmount: 1000,
      recognitionPeriods: 2,
      firstRecognitionDate: "2026-01-31",
      registrationDate: "2026-01-05",
      resultAccountNo: "3150",
      documentId,
    });
    expect(reg.ok).toBe(true);
    expect(() =>
      db.query("UPDATE accruals SET total_amount = 5000 WHERE id = ?").run(reg.accrualId!),
    ).toThrow();
    cleanup();
  });
});
