// Tests: src/core/ledger.ts (audit-chain verification)
import { describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { hashEntry, postJournalEntry, seedAccounts, verifyAuditChain } from "../../src/core/ledger";

import { cleanupDir } from "../helpers/cleanup";
type ManualLine = {
  account_no: string;
  debit_amount: number;
  credit_amount: number;
  vat_code: string | null;
  text: string;
};

function insertManualEntry(db: ReturnType<typeof openDb>, input: {
  entryNo: string;
  previousHash: string;
  transactionDate: string;
  text: string;
  lines: ManualLine[];
  sourceBankTransactionId?: number | null;
  status?: "posted" | "reversed";
  reversalOfEntryId?: number | null;
}) {
  const entry = {
    entry_no: input.entryNo,
    transaction_date: input.transactionDate,
    text: input.text,
    source_bank_transaction_id: input.sourceBankTransactionId ?? null,
    document_id: null,
    currency: "DKK",
    amount_foreign: null,
    amount_dkk: null,
    fx_rate_to_dkk: null,
    rule_version: "dk-v0.0.1",
    created_by: "system",
    created_by_program: "rentemester",
    status: input.status ?? "posted",
    reversal_of_entry_id: input.reversalOfEntryId ?? null,
  };
  // The audit chain binds the row id and per-line ordinal, so the hash must be
  // computed with the id this row will receive once inserted.
  const predictedId = ((db.query("SELECT COALESCE(MAX(id), 0) AS n FROM journal_entries").get() as { n: number }).n) + 1;
  const canonical = {
    id: predictedId,
    ...entry,
    lines: input.lines.map((line, ordinal) => ({
      ordinal,
      account_no: line.account_no,
      debit_amount: line.debit_amount,
      credit_amount: line.credit_amount,
      vat_code: line.vat_code ?? null,
      text: line.text ?? null,
    })),
  };
  const entryHash = hashEntry(canonical, input.previousHash);

  db.run(
    `INSERT INTO journal_entries (
      id, entry_no, transaction_date, text, source_bank_transaction_id, document_id,
      currency, amount_foreign, amount_dkk, fx_rate_to_dkk,
      rule_version, created_by, created_by_program, status, reversal_of_entry_id, previous_hash, entry_hash
    ) VALUES (?, ?, ?, ?, ?, NULL, ?, NULL, NULL, NULL, ?, ?, ?, ?, ?, ?, ?)`,
    predictedId,
    entry.entry_no,
    entry.transaction_date,
    entry.text,
    entry.source_bank_transaction_id,
    entry.currency,
    entry.rule_version,
    entry.created_by,
    entry.created_by_program,
    entry.status,
    entry.reversal_of_entry_id,
    input.previousHash,
    entryHash,
  );

  const inserted = db.query("SELECT id FROM journal_entries WHERE entry_no = ?").get(entry.entry_no) as { id: number };
  for (const line of input.lines) {
    const account = db.query("SELECT id FROM accounts WHERE account_no = ?").get(line.account_no) as { id: number };
    db.run(
      `INSERT INTO journal_lines (journal_entry_id, account_id, debit_amount, credit_amount, vat_code, currency, text)
       VALUES (?, ?, ?, ?, ?, 'DKK', ?)`,
      inserted.id,
      account.id,
      line.debit_amount,
      line.credit_amount,
      line.vat_code,
      line.text,
    );
  }

  return { id: inserted.id, entryHash };
}

describe("audit verify", () => {
  test("flags an unbalanced journal entry even when the stored hash chain matches", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-audit-verify-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    insertManualEntry(db, {
      entryNo: "2026-00001",
      previousHash: "GENESIS",
      transactionDate: "2026-05-16",
      text: "Corrupt unbalanced entry",
      lines: [
        { account_no: "2000", debit_amount: 100, credit_amount: 0, vat_code: null, text: "Bank" },
        { account_no: "1000", debit_amount: 0, credit_amount: 90, vat_code: null, text: "Income" },
      ],
    });

    const audit = verifyAuditChain(db);
    expect(audit.ok).toBe(false);
    expect(audit.errors.some((error) => error.includes("entry is unbalanced"))).toBe(true);

    db.close();
    cleanupDir(root);
  });

  test("flags duplicate use of the same source bank transaction across journal entries", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-audit-verify-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    db.run(
      `INSERT INTO bank_transactions (transaction_date, text, amount, currency, transaction_hash, status)
       VALUES ('2026-05-16', 'Customer payment', 1250, 'DKK', 'dup-bank-hash', 'imported')`
    );
    const bankTransaction = db.query("SELECT id FROM bank_transactions WHERE transaction_hash = 'dup-bank-hash'").get() as { id: number };

    const first = insertManualEntry(db, {
      entryNo: "2026-00001",
      previousHash: "GENESIS",
      transactionDate: "2026-05-16",
      text: "First settlement",
      sourceBankTransactionId: bankTransaction.id,
      lines: [
        { account_no: "2000", debit_amount: 1250, credit_amount: 0, vat_code: null, text: "Bank" },
        { account_no: "1100", debit_amount: 0, credit_amount: 1250, vat_code: null, text: "Receivable" },
      ],
    });
    insertManualEntry(db, {
      entryNo: "2026-00002",
      previousHash: first.entryHash,
      transactionDate: "2026-05-16",
      text: "Duplicate settlement marked reversed without reversal link",
      sourceBankTransactionId: bankTransaction.id,
      status: "reversed",
      lines: [
        { account_no: "2000", debit_amount: 1250, credit_amount: 0, vat_code: null, text: "Bank again" },
        { account_no: "1100", debit_amount: 0, credit_amount: 1250, vat_code: null, text: "Receivable again" },
      ],
    });

    const audit = verifyAuditChain(db);
    expect(audit.ok).toBe(false);
    expect(audit.errors.some((error) => error.includes("duplicate source_bank_transaction_id"))).toBe(true);

    db.close();
    cleanupDir(root);
  });

  test("detects tail truncation of the most recent journal entries", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-audit-truncate-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    for (let i = 0; i < 3; i++) {
      const posted = postJournalEntry(db, {
        transactionDate: "2026-05-16",
        text: `Balanced entry ${i}`,
        lines: [
          { accountNo: "2000", debitAmount: 1000 },
          { accountNo: "5000", creditAmount: 1000 }
        ]
      });
      expect(posted.ok).toBe(true);
    }
    expect(verifyAuditChain(db).ok).toBe(true);

    // Drop the append-only protection and truncate the most recent entry.
    const lastId = (db.query("SELECT MAX(id) AS id FROM journal_entries").get() as { id: number }).id;
    db.run("DROP TRIGGER journal_lines_no_delete");
    db.run("DROP TRIGGER journal_entries_no_delete");
    db.run("DELETE FROM journal_lines WHERE journal_entry_id = ?", lastId);
    db.run("DELETE FROM journal_entries WHERE id = ?", lastId);

    const result = verifyAuditChain(db);
    expect(result.ok).toBe(false);
    expect(result.errors.some((error) => error.includes("missing"))).toBe(true);

    db.close();
    cleanupDir(root);
  });

  test("binds the row id into the entry hash so swapped rows fail verification", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-audit-id-bind-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    const first = insertManualEntry(db, {
      entryNo: "2026-00001",
      previousHash: "GENESIS",
      transactionDate: "2026-05-16",
      text: "First entry",
      lines: [
        { account_no: "2000", debit_amount: 100, credit_amount: 0, vat_code: null, text: "Bank" },
        { account_no: "5000", debit_amount: 0, credit_amount: 100, vat_code: null, text: "Equity" },
      ],
    });
    insertManualEntry(db, {
      entryNo: "2026-00002",
      previousHash: first.entryHash,
      transactionDate: "2026-05-16",
      text: "Second entry",
      lines: [
        { account_no: "2000", debit_amount: 200, credit_amount: 0, vat_code: null, text: "Bank" },
        { account_no: "5000", debit_amount: 0, credit_amount: 200, vat_code: null, text: "Equity" },
      ],
    });
    expect(verifyAuditChain(db).ok).toBe(true);

    // Swap the entry_no values between the two rows. The chain walks by id, so
    // each row keeps a valid previous_hash link, but the id-bound hash no longer
    // matches its row identity.
    db.run("DROP TRIGGER journal_entries_no_update");
    db.run("UPDATE journal_entries SET entry_no = '2026-TMP' WHERE entry_no = '2026-00001'");
    db.run("UPDATE journal_entries SET entry_no = '2026-00001' WHERE entry_no = '2026-00002'");
    db.run("UPDATE journal_entries SET entry_no = '2026-00002' WHERE entry_no = '2026-TMP'");

    const result = verifyAuditChain(db);
    expect(result.ok).toBe(false);
    expect(result.errors.some((error) => error.includes("entry_hash mismatch"))).toBe(true);

    db.close();
    cleanupDir(root);
  });

  test("allows a reversal pair to share the same source bank transaction without audit failure", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-audit-verify-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    db.run(
      `INSERT INTO bank_transactions (transaction_date, text, amount, currency, transaction_hash, status)
       VALUES ('2026-05-16', 'Customer payment', 1250, 'DKK', 'reversal-bank-hash', 'imported')`
    );
    const bankTransaction = db.query("SELECT id FROM bank_transactions WHERE transaction_hash = 'reversal-bank-hash'").get() as { id: number };

    const original = insertManualEntry(db, {
      entryNo: "2026-00001",
      previousHash: "GENESIS",
      transactionDate: "2026-05-16",
      text: "Settlement",
      sourceBankTransactionId: bankTransaction.id,
      lines: [
        { account_no: "2000", debit_amount: 1250, credit_amount: 0, vat_code: null, text: "Bank" },
        { account_no: "1100", debit_amount: 0, credit_amount: 1250, vat_code: null, text: "Receivable" },
      ],
    });
    insertManualEntry(db, {
      entryNo: "2026-00002",
      previousHash: original.entryHash,
      transactionDate: "2026-05-17",
      text: "Reversal of settlement",
      sourceBankTransactionId: bankTransaction.id,
      status: "reversed",
      reversalOfEntryId: original.id,
      lines: [
        { account_no: "2000", debit_amount: 0, credit_amount: 1250, vat_code: null, text: "Bank reversal" },
        { account_no: "1100", debit_amount: 1250, credit_amount: 0, vat_code: null, text: "Receivable reversal" },
      ],
    });

    const audit = verifyAuditChain(db);
    expect(audit.ok).toBe(true);
    expect(audit.errors).toHaveLength(0);

    db.close();
    cleanupDir(root);
  });
});
