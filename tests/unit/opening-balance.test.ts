// Tests: src/core/opening-balance.ts (primobalance flow, #179)
import { describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { seedAccounts, verifyAuditChain } from "../../src/core/ledger";
import { cleanupDir } from "../helpers/cleanup";
import {
  postOpeningBalance,
  getOpeningBalance,
  OPENING_BALANCE_TEXT,
} from "../../src/core/opening-balance";

function freshCompany(prefix: string) {
  const root = mkdtempSync(join(tmpdir(), prefix));
  const db = openDb(ensureCompanyDirs(root).db);
  migrate(db);
  seedAccounts(db);
  return { root, db };
}

describe("opening balance (primobalance)", () => {
  test("posts a balanced primobalance as an audited opening journal entry", () => {
    const { root, db } = freshCompany("rentemester-opening-balance-");
    try {
      const result = postOpeningBalance(db, {
        cutOverDate: "2026-01-01",
        lines: [
          { accountNo: "2000", debitAmount: 50000 },
          { accountNo: "1100", debitAmount: 12000 },
          { accountNo: "5000", creditAmount: 62000 },
        ],
        createdBy: "user:tester",
      });
      expect(result.errors).toEqual([]);
      expect(result.ok).toBe(true);
      expect(result.entryId).toBeGreaterThan(0);
      expect(typeof result.entryNo).toBe("string");

      // The opening entry is explicitly flagged as the opening entry.
      const entry = db
        .query("SELECT id, text, transaction_date FROM journal_entries WHERE id = ?")
        .get(result.entryId) as { id: number; text: string; transaction_date: string };
      expect(entry.transaction_date).toBe("2026-01-01");
      expect(entry.text.startsWith(OPENING_BALANCE_TEXT)).toBe(true);

      // Ledger integrity is preserved.
      expect(verifyAuditChain(db).ok).toBe(true);
    } finally {
      db.close();
      cleanupDir(root);
    }
  });

  test("rejects an unbalanced primobalance (debits != credits)", () => {
    const { root, db } = freshCompany("rentemester-opening-balance-unbal-");
    try {
      const result = postOpeningBalance(db, {
        cutOverDate: "2026-01-01",
        lines: [
          { accountNo: "2000", debitAmount: 50000 },
          { accountNo: "5000", creditAmount: 40000 },
        ],
        createdBy: "user:tester",
      });
      expect(result.ok).toBe(false);
      expect(result.errors.join(" ")).toContain("balance");
      // Nothing was recorded.
      expect(getOpeningBalance(db)).toBeNull();
      const count = db.query("SELECT COUNT(*) AS n FROM journal_entries").get() as { n: number };
      expect(count.n).toBe(0);
    } finally {
      db.close();
      cleanupDir(root);
    }
  });

  test("is idempotent: a second primobalance is rejected", () => {
    const { root, db } = freshCompany("rentemester-opening-balance-dup-");
    try {
      const lines = [
        { accountNo: "2000", debitAmount: 50000 },
        { accountNo: "5000", creditAmount: 50000 },
      ];
      const first = postOpeningBalance(db, { cutOverDate: "2026-01-01", lines, createdBy: "user:tester" });
      expect(first.ok).toBe(true);

      const second = postOpeningBalance(db, { cutOverDate: "2026-01-01", lines, createdBy: "user:tester" });
      expect(second.ok).toBe(false);
      expect(second.errors.join(" ").toLowerCase()).toContain("already");

      // Exactly one journal entry was posted.
      const count = db.query("SELECT COUNT(*) AS n FROM journal_entries").get() as { n: number };
      expect(count.n).toBe(1);
    } finally {
      db.close();
      cleanupDir(root);
    }
  });

  test("records an audit trail for the opening balance", () => {
    const { root, db } = freshCompany("rentemester-opening-balance-audit-");
    try {
      const result = postOpeningBalance(db, {
        cutOverDate: "2026-01-01",
        lines: [
          { accountNo: "2000", debitAmount: 50000 },
          { accountNo: "5000", creditAmount: 50000 },
        ],
        createdBy: "user:auditor",
      });
      expect(result.ok).toBe(true);

      const audit = db
        .query("SELECT event_type, message FROM audit_log WHERE event_type = 'opening_balance_post'")
        .all() as Array<{ event_type: string; message: string }>;
      expect(audit.length).toBe(1);
      expect(audit[0]!.message).toContain("2026-01-01");

      const marker = getOpeningBalance(db);
      expect(marker).not.toBeNull();
      expect(marker!.cutOverDate).toBe("2026-01-01");
      expect(marker!.journalEntryId).toBe(result.entryId!);
    } finally {
      db.close();
      cleanupDir(root);
    }
  });

  test("rejects lines referencing a non-existent account", () => {
    const { root, db } = freshCompany("rentemester-opening-balance-acct-");
    try {
      const result = postOpeningBalance(db, {
        cutOverDate: "2026-01-01",
        lines: [
          { accountNo: "9999", debitAmount: 50000 },
          { accountNo: "5000", creditAmount: 50000 },
        ],
        createdBy: "user:tester",
      });
      expect(result.ok).toBe(false);
      expect(result.errors.join(" ")).toContain("must reference an existing account");
      // No primobalance marker is recorded when an account is invalid.
      expect(getOpeningBalance(db)).toBeNull();
    } finally {
      db.close();
      cleanupDir(root);
    }
  });

  test("rejects an invalid cut-over date", () => {
    const { root, db } = freshCompany("rentemester-opening-balance-date-");
    try {
      const result = postOpeningBalance(db, {
        cutOverDate: "not-a-date",
        lines: [
          { accountNo: "2000", debitAmount: 50000 },
          { accountNo: "5000", creditAmount: 50000 },
        ],
        createdBy: "user:tester",
      });
      expect(result.ok).toBe(false);
      expect(result.errors.join(" ").toLowerCase()).toContain("cut-over");
    } finally {
      db.close();
      cleanupDir(root);
    }
  });

  test("financial statements reflect the opening balance from the cut-over date", () => {
    const { root, db } = freshCompany("rentemester-opening-balance-stmt-");
    try {
      const result = postOpeningBalance(db, {
        cutOverDate: "2026-01-01",
        lines: [
          { accountNo: "2000", debitAmount: 75000 },
          { accountNo: "5000", creditAmount: 75000 },
        ],
        createdBy: "user:tester",
      });
      expect(result.ok).toBe(true);

      // Aggregate ledger movement per account — what any statement is built on.
      const bank = db
        .query(
          `SELECT COALESCE(SUM(jl.debit_amount),0) - COALESCE(SUM(jl.credit_amount),0) AS net
             FROM journal_lines jl
             JOIN accounts a ON a.id = jl.account_id
            WHERE a.account_no = '2000'`,
        )
        .get() as { net: number };
      const equity = db
        .query(
          `SELECT COALESCE(SUM(jl.credit_amount),0) - COALESCE(SUM(jl.debit_amount),0) AS net
             FROM journal_lines jl
             JOIN accounts a ON a.id = jl.account_id
            WHERE a.account_no = '5000'`,
        )
        .get() as { net: number };
      expect(bank.net).toBe(75000);
      expect(equity.net).toBe(75000);
    } finally {
      db.close();
      cleanupDir(root);
    }
  });
});
