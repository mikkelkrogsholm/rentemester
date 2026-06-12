// Tests: src/core/financial-statements.ts
// Trial balance (saldobalance), profit & loss (resultatopgørelse) and
// balance sheet (balance) computed deterministically from the ledger.
import { describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { ingestDocument } from "../../src/core/documents";
import { postJournalEntry, reverseJournalEntry, seedAccounts } from "../../src/core/ledger";
import { cleanupDir } from "../helpers/cleanup";
import {
  buildBalanceSheet,
  buildProfitAndLoss,
  buildTrialBalance,
} from "../../src/core/financial-statements";

/**
 * Seed a fixed company with two postings inside May 2026:
 *  - a 1.250 sale (bank 1250 debit / income 1000 credit / output VAT 250 credit)
 *  - a 1.250 software purchase (expense 1000 debit / input VAT 250 debit / bank 1250 credit)
 * Net bank position after both = 0. Income 1000, expense 1000.
 */
function seedFixedBooks() {
  const root = mkdtempSync(join(tmpdir(), "rentemester-fs-"));
  const inbox = mkdtempSync(join(tmpdir(), "rentemester-fs-inbox-"));
  const sourceFile = join(inbox, "invoice.txt");
  writeFileSync(sourceFile, "Invoice\n1250 DKK\n");

  const db = openDb(ensureCompanyDirs(root).db);
  migrate(db);
  seedAccounts(db);

  const doc = ingestDocument(db, root, sourceFile, {
    source: "email",
    issueDate: "2026-05-16",
    invoiceNo: "INV-FS-1",
    deliveryDescription: "Softwareabonnement",
    amountIncVat: 1250,
    currency: "DKK",
    sender: { name: "Leverandør ApS", address: "Sælgervej 1", vatOrCvr: "DK11223344" },
    recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
    vatAmount: 250,
    paymentDetails: "Bankoverførsel",
  });
  if (!doc.ok) throw new Error("document ingest failed: " + (doc.errors ?? []).join("; "));

  const sale = postJournalEntry(db, {
    transactionDate: "2026-05-10",
    text: "Consulting sale",
    documentId: doc.documentId,
    lines: [
      { accountNo: "2000", debitAmount: 1250 },
      { accountNo: "1000", creditAmount: 1000, vatCode: "DK_SALE_25" },
      { accountNo: "1200", creditAmount: 250 },
    ],
  });
  if (!sale.ok) throw new Error("sale post failed: " + sale.errors.join("; "));

  const purchase = postJournalEntry(db, {
    transactionDate: "2026-05-16",
    text: "Software purchase",
    documentId: doc.documentId,
    lines: [
      { accountNo: "3000", debitAmount: 1000, vatCode: "DK_PURCHASE_25" },
      { accountNo: "4000", debitAmount: 250 },
      { accountNo: "2000", creditAmount: 1250 },
    ],
  });
  if (!purchase.ok) throw new Error("purchase post failed: " + purchase.errors.join("; "));

  return { db, root, inbox, doc, sale, purchase };
}

describe("trial balance (saldobalance)", () => {
  test("computes exact per-account debit/credit totals and net balances for a period", () => {
    const { db, root, inbox } = seedFixedBooks();

    const tb = buildTrialBalance(db, "2026-05-01", "2026-05-31");
    expect(tb.ok).toBe(true);
    expect(tb.errors).toEqual([]);
    expect(tb.periodStart).toBe("2026-05-01");
    expect(tb.periodEnd).toBe("2026-05-31");

    const byAccount = new Map(tb.accounts.map((a) => [a.accountNo, a]));

    // Bank: 1250 debit (sale) + 1250 credit (purchase) → net 0
    expect(byAccount.get("2000")!.debit).toBe(1250);
    expect(byAccount.get("2000")!.credit).toBe(1250);
    expect(byAccount.get("2000")!.balance).toBe(0);
    expect(byAccount.get("2000")!.type).toBe("asset");

    // Income 1000: credit-normal account → negative net (credit - debit convention)
    expect(byAccount.get("1000")!.credit).toBe(1000);
    expect(byAccount.get("1000")!.debit).toBe(0);
    expect(byAccount.get("1000")!.balance).toBe(-1000);

    // Expense 1000: debit-normal → positive net
    expect(byAccount.get("3000")!.debit).toBe(1000);
    expect(byAccount.get("3000")!.balance).toBe(1000);

    // Output VAT 1200 credit 250, input VAT 4000 debit 250
    expect(byAccount.get("1200")!.credit).toBe(250);
    expect(byAccount.get("4000")!.debit).toBe(250);

    // Trial balance must balance: total debit == total credit
    expect(tb.totalDebit).toBe(2500);
    expect(tb.totalCredit).toBe(2500);
    expect(tb.balanced).toBe(true);

    db.close();
    cleanupDir(root);
    cleanupDir(inbox);
  });

  test("excludes accounts with no movement in the period and only lists touched accounts", () => {
    const { db, root, inbox } = seedFixedBooks();
    const tb = buildTrialBalance(db, "2026-05-01", "2026-05-31");
    const touched = tb.accounts.map((a) => a.accountNo).sort();
    expect(touched).toEqual(["1000", "1200", "2000", "3000", "4000"]);
    // Accounts are sorted by account_no ascending.
    expect(tb.accounts.map((a) => a.accountNo)).toEqual(["1000", "1200", "2000", "3000", "4000"]);
    db.close();
    cleanupDir(root);
    cleanupDir(inbox);
  });

  test("rejects an inverted period and an invalid date", () => {
    const { db, root, inbox } = seedFixedBooks();
    const inverted = buildTrialBalance(db, "2026-05-31", "2026-05-01");
    expect(inverted.ok).toBe(false);
    expect(inverted.errors.length).toBeGreaterThan(0);

    const bad = buildTrialBalance(db, "not-a-date", "2026-05-31");
    expect(bad.ok).toBe(false);
    db.close();
    cleanupDir(root);
    cleanupDir(inbox);
  });
});

describe("period boundary filtering", () => {
  test("includes postings on the exact period start/end and excludes the days outside", () => {
    const { db, root, inbox } = seedFixedBooks();

    // Sale is 2026-05-10, purchase 2026-05-16.
    // Period covering only the sale day picks up the sale, not the purchase.
    const saleOnly = buildTrialBalance(db, "2026-05-10", "2026-05-15");
    expect(saleOnly.ok).toBe(true);
    expect(saleOnly.accounts.map((a) => a.accountNo).sort()).toEqual(["1000", "1200", "2000"]);
    expect(saleOnly.totalDebit).toBe(1250);
    expect(saleOnly.totalCredit).toBe(1250);

    // A period that ends the day before the purchase still excludes it.
    const beforePurchase = buildTrialBalance(db, "2026-05-01", "2026-05-15");
    expect(beforePurchase.accounts.find((a) => a.accountNo === "3000")).toBeUndefined();

    // A period starting exactly on the purchase day includes it.
    const fromPurchase = buildTrialBalance(db, "2026-05-16", "2026-05-31");
    expect(fromPurchase.accounts.find((a) => a.accountNo === "3000")!.debit).toBe(1000);

    // A period entirely before any posting is empty but valid.
    const empty = buildTrialBalance(db, "2026-01-01", "2026-01-31");
    expect(empty.ok).toBe(true);
    expect(empty.accounts).toEqual([]);
    expect(empty.totalDebit).toBe(0);
    expect(empty.totalCredit).toBe(0);
    expect(empty.balanced).toBe(true);

    db.close();
    cleanupDir(root);
    cleanupDir(inbox);
  });
});

describe("profit & loss (resultatopgørelse)", () => {
  test("profit = income - expense for the period", () => {
    const { db, root, inbox } = seedFixedBooks();
    const pl = buildProfitAndLoss(db, "2026-05-01", "2026-05-31");
    expect(pl.ok).toBe(true);
    expect(pl.totalIncome).toBe(1000);
    expect(pl.totalExpense).toBe(1000);
    expect(pl.result).toBe(0);

    // Income and expense lines are present.
    expect(pl.income.find((l) => l.accountNo === "1000")!.amount).toBe(1000);
    expect(pl.expense.find((l) => l.accountNo === "3000")!.amount).toBe(1000);
    // VAT/asset accounts must not leak into the P&L.
    expect(pl.income.find((l) => l.accountNo === "1200")).toBeUndefined();
    expect(pl.expense.find((l) => l.accountNo === "2000")).toBeUndefined();

    db.close();
    cleanupDir(root);
    cleanupDir(inbox);
  });

  test("a second sale produces a positive result equal to income minus expense", () => {
    const { db, root, inbox, doc } = seedFixedBooks();
    const sale2 = postJournalEntry(db, {
      transactionDate: "2026-05-20",
      text: "Second consulting sale",
      documentId: doc.documentId,
      lines: [
        { accountNo: "2000", debitAmount: 2500 },
        { accountNo: "1000", creditAmount: 2000, vatCode: "DK_SALE_25" },
        { accountNo: "1200", creditAmount: 500 },
      ],
    });
    expect(sale2.ok).toBe(true);

    const pl = buildProfitAndLoss(db, "2026-05-01", "2026-05-31");
    expect(pl.totalIncome).toBe(3000);
    expect(pl.totalExpense).toBe(1000);
    expect(pl.result).toBe(2000);

    db.close();
    cleanupDir(root);
    cleanupDir(inbox);
  });
});

describe("balance sheet (balance)", () => {
  test("balances: assets = liabilities + equity at a date", () => {
    const { db, root, inbox } = seedFixedBooks();

    // As of end of May the books only have the sale + purchase.
    // Assets: bank net 0, debtors 0. The 250 input VAT (4000) is an asset-type
    // account, the 250 output VAT (1200) is a vat account.
    // The P&L result of 0 (income 1000 - expense 1000) flows into equity so the
    // sheet balances.
    const bs = buildBalanceSheet(db, "2026-05-31");
    expect(bs.ok).toBe(true);
    expect(bs.errors).toEqual([]);
    expect(bs.asOfDate).toBe("2026-05-31");
    expect(bs.balanced).toBe(true);
    expect(bs.totalAssets).toBe(bs.totalLiabilitiesAndEquity);

    db.close();
    cleanupDir(root);
    cleanupDir(inbox);
  });

  test("a retained-earnings posting still balances and exposes the result line", () => {
    const { db, root, inbox, doc } = seedFixedBooks();
    // Add an asymmetric sale so income > expense; equity must absorb the result.
    const sale2 = postJournalEntry(db, {
      transactionDate: "2026-05-21",
      text: "Profitable sale",
      documentId: doc.documentId,
      lines: [
        { accountNo: "2000", debitAmount: 6250 },
        { accountNo: "1000", creditAmount: 5000, vatCode: "DK_SALE_25" },
        { accountNo: "1200", creditAmount: 1250 },
      ],
    });
    expect(sale2.ok).toBe(true);

    const bs = buildBalanceSheet(db, "2026-05-31");
    expect(bs.ok).toBe(true);
    expect(bs.balanced).toBe(true);
    expect(bs.totalAssets).toBe(bs.totalLiabilitiesAndEquity);
    // The period result (income - expense before this date) is carried in equity.
    expect(bs.periodResult).toBe(5000);

    db.close();
    cleanupDir(root);
    cleanupDir(inbox);
  });

  test("rejects an invalid as-of date", () => {
    const { db, root, inbox } = seedFixedBooks();
    const bad = buildBalanceSheet(db, "2026-13-99");
    expect(bad.ok).toBe(false);
    expect(bad.errors.length).toBeGreaterThan(0);
    db.close();
    cleanupDir(root);
    cleanupDir(inbox);
  });
});

describe("reversal handling", () => {
  test("a reversed sale and its reversal net to zero in a spanning period", () => {
    const { db, root, inbox, sale } = seedFixedBooks();
    const reversed = reverseJournalEntry(db, {
      entryId: sale.entryId!,
      transactionDate: "2026-05-25",
      reason: "Booked in error",
    });
    expect(reversed.ok).toBe(true);

    // May still contains the original sale, its reversal and the purchase.
    const tb = buildTrialBalance(db, "2026-05-01", "2026-05-31");
    expect(tb.balanced).toBe(true);
    const bank = tb.accounts.find((a) => a.accountNo === "2000")!;
    // sale +1250, purchase -1250, reversal -1250 → net -1250
    expect(bank.balance).toBe(-1250);

    // Income: sale credit 1000, reversal debit 1000 → net 0.
    const pl = buildProfitAndLoss(db, "2026-05-01", "2026-05-31");
    expect(pl.income.find((l) => l.accountNo === "1000")!.amount).toBe(0);

    db.close();
    cleanupDir(root);
    cleanupDir(inbox);
  });
});
