import { expect, test, describe } from "bun:test";
import { parseBillyPostings } from "../../src/core/import/billy-postings";

const accountMap = new Map([
  ["acc-bank", "5710"],
  ["acc-expense", "1815"],
  ["acc-vat", "4000"],
  ["acc-income", "1000"],
  ["acc-debtor", "1100"],
]);

describe("Billy postings parser", () => {
  test("groups postings by transactionId into balanced vouchers", () => {
    const errors: string[] = [];
    const entries = parseBillyPostings(
      JSON.stringify([
        {
          id: "p1",
          transactionId: "txn-1",
          entryDate: "2026-01-15",
          text: "Software subscription",
          accountId: "acc-expense",
          amount: 200,
          side: "debit",
          isVoided: false,
          priority: 1,
        },
        {
          id: "p2",
          transactionId: "txn-1",
          entryDate: "2026-01-15",
          text: "Software subscription",
          accountId: "acc-bank",
          amount: 200,
          side: "credit",
          isVoided: false,
          priority: 2,
        },
      ]),
      { cutOverDate: "2026-01-01", accountIdToNo: accountMap },
      errors,
    );

    expect(errors.length).toBe(0);
    expect(entries.length).toBe(1);
    expect(entries[0]!.transactionDate).toBe("2026-01-15");
    expect(entries[0]!.voucherRef).toBe("txn-1");
    expect(entries[0]!.lines.length).toBe(2);

    const debitLine = entries[0]!.lines.find((l) => l.debitAmount !== undefined)!;
    expect(debitLine.accountNo).toBe("1815");
    expect(debitLine.debitAmount).toBe(200);

    const creditLine = entries[0]!.lines.find((l) => l.creditAmount !== undefined)!;
    expect(creditLine.accountNo).toBe("5710");
    expect(creditLine.creditAmount).toBe(200);
  });

  test("filters out postings before cut-over date", () => {
    const errors: string[] = [];
    const entries = parseBillyPostings(
      JSON.stringify([
        {
          id: "p1",
          transactionId: "txn-old",
          entryDate: "2025-12-31",
          text: "Old",
          accountId: "acc-expense",
          amount: 100,
          side: "debit",
          isVoided: false,
          priority: 1,
        },
        {
          id: "p2",
          transactionId: "txn-old",
          entryDate: "2025-12-31",
          text: "Old",
          accountId: "acc-bank",
          amount: 100,
          side: "credit",
          isVoided: false,
          priority: 2,
        },
        {
          id: "p3",
          transactionId: "txn-new",
          entryDate: "2026-01-02",
          text: "New",
          accountId: "acc-expense",
          amount: 50,
          side: "debit",
          isVoided: false,
          priority: 1,
        },
        {
          id: "p4",
          transactionId: "txn-new",
          entryDate: "2026-01-02",
          text: "New",
          accountId: "acc-bank",
          amount: 50,
          side: "credit",
          isVoided: false,
          priority: 2,
        },
      ]),
      { cutOverDate: "2026-01-01", accountIdToNo: accountMap },
      errors,
    );

    expect(entries.length).toBe(1);
    expect(entries[0]!.voucherRef).toBe("txn-new");
  });

  test("skips voided postings", () => {
    const errors: string[] = [];
    const entries = parseBillyPostings(
      JSON.stringify([
        {
          id: "p1",
          transactionId: "txn-1",
          entryDate: "2026-02-01",
          text: "Voided",
          accountId: "acc-expense",
          amount: 100,
          side: "debit",
          isVoided: true,
          priority: 1,
        },
        {
          id: "p2",
          transactionId: "txn-1",
          entryDate: "2026-02-01",
          text: "Voided",
          accountId: "acc-bank",
          amount: 100,
          side: "credit",
          isVoided: true,
          priority: 2,
        },
      ]),
      { cutOverDate: "2026-01-01", accountIdToNo: accountMap },
      errors,
    );

    expect(entries.length).toBe(0);
  });

  test("skips single-line transactions (cannot form balanced entry)", () => {
    const errors: string[] = [];
    const entries = parseBillyPostings(
      JSON.stringify([
        {
          id: "p1",
          transactionId: "txn-orphan",
          entryDate: "2026-03-01",
          text: "Orphan",
          accountId: "acc-expense",
          amount: 50,
          side: "debit",
          isVoided: false,
          priority: 1,
        },
      ]),
      { cutOverDate: "2026-01-01", accountIdToNo: accountMap },
      errors,
    );

    expect(entries.length).toBe(0);
  });

  test("handles multiple transactions correctly", () => {
    const errors: string[] = [];
    const entries = parseBillyPostings(
      JSON.stringify([
        { id: "p1", transactionId: "txn-a", entryDate: "2026-01-10", text: "Purchase A", accountId: "acc-expense", amount: 100, side: "debit", isVoided: false, priority: 1 },
        { id: "p2", transactionId: "txn-a", entryDate: "2026-01-10", text: "Purchase A", accountId: "acc-bank", amount: 100, side: "credit", isVoided: false, priority: 2 },
        { id: "p3", transactionId: "txn-b", entryDate: "2026-01-15", text: "Income B", accountId: "acc-bank", amount: 500, side: "debit", isVoided: false, priority: 1 },
        { id: "p4", transactionId: "txn-b", entryDate: "2026-01-15", text: "Income B", accountId: "acc-income", amount: 500, side: "credit", isVoided: false, priority: 2 },
      ]),
      { cutOverDate: "2026-01-01", accountIdToNo: accountMap },
      errors,
    );

    expect(entries.length).toBe(2);
    expect(entries[0]!.transactionDate).toBe("2026-01-10");
    expect(entries[1]!.transactionDate).toBe("2026-01-15");
  });

  test("rejects invalid JSON", () => {
    const errors: string[] = [];
    const entries = parseBillyPostings("not json", { cutOverDate: "2026-01-01", accountIdToNo: accountMap }, errors);
    expect(entries.length).toBe(0);
    expect(errors[0]).toContain("invalid JSON");
  });

  test("returns empty for empty array", () => {
    const errors: string[] = [];
    const entries = parseBillyPostings("[]", { cutOverDate: "2026-01-01", accountIdToNo: accountMap }, errors);
    expect(entries.length).toBe(0);
    expect(errors.length).toBe(0);
  });

  test("skips postings with unknown accountId", () => {
    const errors: string[] = [];
    const entries = parseBillyPostings(
      JSON.stringify([
        { id: "p1", transactionId: "txn-1", entryDate: "2026-02-01", text: "Unknown", accountId: "unknown-id", amount: 100, side: "debit", isVoided: false, priority: 1 },
        { id: "p2", transactionId: "txn-1", entryDate: "2026-02-01", text: "Unknown", accountId: "acc-bank", amount: 100, side: "credit", isVoided: false, priority: 2 },
      ]),
      { cutOverDate: "2026-01-01", accountIdToNo: accountMap },
      errors,
    );

    // One line has unknown account → only 1 line → filtered as single-line
    expect(entries.length).toBe(0);
  });

  test("uses absolute amounts (negative amounts become positive)", () => {
    const errors: string[] = [];
    const entries = parseBillyPostings(
      JSON.stringify([
        { id: "p1", transactionId: "txn-neg", entryDate: "2026-02-01", text: "Reversal", accountId: "acc-expense", amount: -200, side: "debit", isVoided: false, priority: 1 },
        { id: "p2", transactionId: "txn-neg", entryDate: "2026-02-01", text: "Reversal", accountId: "acc-bank", amount: -200, side: "credit", isVoided: false, priority: 2 },
      ]),
      { cutOverDate: "2026-01-01", accountIdToNo: accountMap },
      errors,
    );

    expect(entries.length).toBe(1);
    const debitLine = entries[0]!.lines.find((l) => l.debitAmount !== undefined)!;
    expect(debitLine.debitAmount).toBe(200);
    const creditLine = entries[0]!.lines.find((l) => l.creditAmount !== undefined)!;
    expect(creditLine.creditAmount).toBe(200);
  });

  test("skips postings with unknown side value", () => {
    const errors: string[] = [];
    const entries = parseBillyPostings(
      JSON.stringify([
        { id: "p1", transactionId: "txn-bad", entryDate: "2026-02-01", text: "Bad", accountId: "acc-expense", amount: 100, side: "unknown", isVoided: false, priority: 1 },
        { id: "p2", transactionId: "txn-bad", entryDate: "2026-02-01", text: "Bad", accountId: "acc-bank", amount: 100, side: "credit", isVoided: false, priority: 2 },
      ]),
      { cutOverDate: "2026-01-01", accountIdToNo: accountMap },
      errors,
    );

    // Unknown side skipped → only 1 valid line → filtered as single-line
    expect(entries.length).toBe(0);
  });

  test("rejects invalid cutOverDate format", () => {
    const errors: string[] = [];
    const entries = parseBillyPostings(
      JSON.stringify([
        { id: "p1", transactionId: "txn-1", entryDate: "2026-01-15", text: "X", accountId: "acc-expense", amount: 100, side: "debit", isVoided: false, priority: 1 },
        { id: "p2", transactionId: "txn-1", entryDate: "2026-01-15", text: "X", accountId: "acc-bank", amount: 100, side: "credit", isVoided: false, priority: 2 },
      ]),
      { cutOverDate: "not-a-date", accountIdToNo: accountMap },
      errors,
    );

    expect(entries.length).toBe(0);
    expect(errors[0]).toContain("not a valid");
  });
});
