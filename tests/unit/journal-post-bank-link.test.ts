// Tests: src/core/ledger.ts (postJournalEntry sourceBankTransactionId path)
//        src/core/exceptions.ts (UNMATCHED_BANK_TRANSACTION resolution)
//
// Issue #520: a securities trade (DR 5850 Depot / CR 2000 Bank) is an
// asset-to-asset entry that cannot use expense book (5850 is not an
// expense account). The journal post payload has always accepted a top-level
// `sourceBankTransactionId`, which both links the entry to the bank txn AND
// auto-resolves the bank txn's UNMATCHED_BANK_TRANSACTION exception. This
// test pins that behavior so the docs-only fix in this PR stays honest.
import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { seedAccounts, postJournalEntry } from "../../src/core/ledger";
import { importBankCsv } from "../../src/core/bank";
import { syncUnmatchedBankTransactionExceptions } from "../../src/core/exceptions";
import { ingestDocument } from "../../src/core/documents";

function makeWorkspace(label: string) {
  const root = mkdtempSync(join(tmpdir(), `rentemester-${label}-`));
  const csv = join(root, "transactions.csv");
  writeFileSync(
    csv,
    [
      "transaction_date,booking_date,text,amount,currency,reference",
      "2026-05-12,2026-05-12,VP-HANDEL NOVO B,-65432.50,DKK,REF-TRADE-1",
    ].join("\n"),
  );
  const db = openDb(ensureCompanyDirs(root).db);
  migrate(db);
  seedAccounts(db);
  const bankResult = importBankCsv(db, root, csv);
  expect(bankResult.ok).toBe(true);
  const bankRow = db
    .query("SELECT id FROM bank_transactions WHERE reference = 'REF-TRADE-1'")
    .get() as { id: number };
  return { root, db, bankTransactionId: bankRow.id };
}

describe("journal post — sourceBankTransactionId", () => {
  test("posts a source-linked payroll accrual and settles net pay on its actual bank date", () => {
    const { root, db, bankTransactionId } = makeWorkspace("payroll-evidence");
    const source = join(root, "payroll-report.txt");
    writeFileSync(source, "Synthetic payroll report August 2026\n");
    const document = ingestDocument(db, root, source, {
      source: "payroll-export", documentType: "external_accounting_evidence", issueDate: "2026-08-31",
      sender: { name: "Synthetic Payroll Provider" }, recipient: { name: "Example Company ApS" }, vatAmount: 0,
      externalAccountingEvidence: { category: "payroll", accountingPeriod: "2026-08", externalReference: "PAY-SYN-2026-08", totals: { debitAmount: 40200, creditAmount: 40200 } },
    });
    expect(document.ok).toBe(true);
    const accrual = postJournalEntry(db, { transactionDate: "2026-08-31", text: "Synthetic August payroll accrual", documentId: document.documentId!, lines: [
      { accountNo: "3500", debitAmount: 40200 }, { accountNo: "7130", creditAmount: 26000 }, { accountNo: "7100", creditAmount: 14000 }, { accountNo: "7120", creditAmount: 200 },
    ] });
    expect(accrual.ok).toBe(true);
    db.run("UPDATE bank_transactions SET transaction_date='2026-08-28', amount=-26000, transaction_hash='payroll-bank-syn' WHERE id=?", [bankTransactionId]);
    const settlement = postJournalEntry(db, { transactionDate: "2026-08-28", text: "Synthetic net-pay settlement", sourceBankTransactionId: bankTransactionId, lines: [{ accountNo: "7130", debitAmount: 26000 }, { accountNo: "2000", creditAmount: 26000 }] });
    expect(settlement.ok).toBe(true);
    expect(db.query("SELECT COALESCE(SUM(debit_amount-credit_amount),0) AS balance FROM journal_lines jl JOIN accounts a ON a.id=jl.account_id WHERE a.account_no='7130'").get()).toEqual({ balance: 0 });
    expect(db.query("SELECT COALESCE(SUM(debit_amount-credit_amount),0) AS balance FROM journal_lines jl JOIN accounts a ON a.id=jl.account_id WHERE a.account_no='7100'").get()).toEqual({ balance: -14000 });
    db.close(); rmSync(root, { recursive: true, force: true });
  });

  test("asset-to-asset entry posts, persists the link, and clears UNMATCHED_BANK_TRANSACTION (#520)", () => {
    const { root, db, bankTransactionId } = makeWorkspace("journal-bank-link-happy");

    // Sync opens an UNMATCHED_BANK_TRANSACTION for the imported bank txn,
    // mirroring what runs during a normal agent loop after a bank import.
    const sync = syncUnmatchedBankTransactionExceptions(db);
    expect(sync.ok).toBe(true);
    const openBefore = db
      .query(
        `SELECT status FROM exceptions
         WHERE type = 'UNMATCHED_BANK_TRANSACTION'
           AND related_bank_transaction_id = ?`,
      )
      .all(bankTransactionId) as Array<{ status: string }>;
    expect(openBefore).toEqual([{ status: "open" }]);

    // Asset-to-asset post: DR 5800 (Driftsmidler) / CR 2000 (Bank). Stands in
    // for the securities-trade shape (DR 5850 Depot / CR 2000 Bank) using only
    // seeded accounts so the test is self-contained.
    const posted = postJournalEntry(db, {
      transactionDate: "2026-05-12",
      text: "Køb af 100 stk. Novo Nordisk B",
      sourceBankTransactionId: bankTransactionId,
      lines: [
        { accountNo: "5800", debitAmount: 65432.50, text: "Depot (test stand-in)" },
        { accountNo: "2000", creditAmount: 65432.50, text: "Bank, afregning" },
      ],
    });
    expect(posted.ok).toBe(true);

    const entry = db
      .query(
        `SELECT source_bank_transaction_id, status
         FROM journal_entries WHERE id = ?`,
      )
      .get(posted.entryId!) as {
      source_bank_transaction_id: number | null;
      status: string;
    };
    expect(entry.source_bank_transaction_id).toBe(bankTransactionId);
    expect(entry.status).toBe("posted");

    const openAfter = db
      .query(
        `SELECT status FROM exceptions
         WHERE type = 'UNMATCHED_BANK_TRANSACTION'
           AND related_bank_transaction_id = ?`,
      )
      .all(bankTransactionId) as Array<{ status: string }>;
    expect(openAfter).toEqual([{ status: "resolved" }]);

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("a second post linking the same bank txn is refused by the unique index (#520)", () => {
    const { root, db, bankTransactionId } = makeWorkspace("journal-bank-link-dup");

    const first = postJournalEntry(db, {
      transactionDate: "2026-05-12",
      text: "Første postering",
      sourceBankTransactionId: bankTransactionId,
      lines: [
        { accountNo: "5800", debitAmount: 65432.50 },
        { accountNo: "2000", creditAmount: 65432.50 },
      ],
    });
    expect(first.ok).toBe(true);

    // Second post with the same sourceBankTransactionId. The partial unique
    // index on journal_entries(source_bank_transaction_id) WHERE status='posted'
    // (src/core/schema.sql:249-250) rejects this. Whether the rejection
    // surfaces as a thrown SQLite error or an {ok:false} envelope is not the
    // contract we care about — only that the second link does not persist.
    let secondLinked = false;
    try {
      const second = postJournalEntry(db, {
        transactionDate: "2026-05-12",
        text: "Anden postering (skal afvises)",
        sourceBankTransactionId: bankTransactionId,
        lines: [
          { accountNo: "5800", debitAmount: 1.0 },
          { accountNo: "2000", creditAmount: 1.0 },
        ],
      });
      secondLinked = second.ok === true;
    } catch {
      secondLinked = false;
    }
    expect(secondLinked).toBe(false);

    const linkedEntries = db
      .query(
        `SELECT id FROM journal_entries
         WHERE source_bank_transaction_id = ?
           AND status = 'posted'`,
      )
      .all(bankTransactionId) as Array<{ id: number }>;
    expect(linkedEntries).toHaveLength(1);
    expect(linkedEntries[0]!.id).toBe(first.entryId!);

    db.close();
    rmSync(root, { recursive: true, force: true });
  });
});
