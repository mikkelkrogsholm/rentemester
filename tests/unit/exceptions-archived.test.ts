// Tests: src/core/exceptions.ts — archived-period exclusion in listExceptions.
// An exception whose subject (a bank transaction / document) falls before the
// primobalance cut-over date, or inside a closed accounting period, is an
// archived artifact — not an open task — and must not be counted as one.
import { describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { seedAccounts } from "../../src/core/ledger";
import { recordException, listExceptions } from "../../src/core/exceptions";
import { postOpeningBalance } from "../../src/core/opening-balance";

import { cleanupDir } from "../helpers/cleanup";
function freshCompany(prefix: string) {
  const root = mkdtempSync(join(tmpdir(), prefix));
  const db = openDb(ensureCompanyDirs(root).db);
  migrate(db);
  seedAccounts(db);
  return { root, db };
}

/** Insert a bank transaction on `date`, return its id. */
function bankTxn(db: ReturnType<typeof openDb>, date: string): number {
  const row = db
    .query(
      `INSERT INTO bank_transactions (transaction_date, text, amount)
       VALUES (?, ?, ?) RETURNING id`,
    )
    .get(date, `Post ${date}`, 100) as { id: number };
  return row.id;
}

function unmatchedException(db: ReturnType<typeof openDb>, bankTransactionId: number) {
  recordException(db, {
    type: "UNMATCHED_BANK_TRANSACTION",
    relatedBankTransactionId: bankTransactionId,
    message: `Bank transaction ${bankTransactionId} is still unmatched`,
  });
}

describe("listExceptions — archived periods", () => {
  test("excludes exceptions before the primobalance cut-over date", () => {
    const { root, db } = freshCompany("rentemester-exc-cutover-");
    try {
      postOpeningBalance(db, {
        cutOverDate: "2026-01-01",
        lines: [
          { accountNo: "2000", debitAmount: 50000 },
          { accountNo: "5000", creditAmount: 50000 },
        ],
        createdBy: "user:tester",
      });

      unmatchedException(db, bankTxn(db, "2025-06-01")); // pre-cut-over → archived
      unmatchedException(db, bankTxn(db, "2026-06-01")); // live year → open task
      // An exception with no dated subject is never archived.
      recordException(db, { type: "MANUAL_REVIEW", message: "needs a human" });

      const open = listExceptions(db, { status: "open" });
      expect(open.count).toBe(2); // the 2026 one + the dateless one
      expect(open.rows.every((r) => r.archived === false)).toBe(true);

      const all = listExceptions(db, { status: "open", includeArchived: true });
      expect(all.count).toBe(3);
      expect(all.rows.filter((r) => r.archived).length).toBe(1);

      db.close();
    } finally {
      cleanupDir(root);
    }
  });

  test("excludes exceptions inside a closed accounting period", () => {
    const { root, db } = freshCompany("rentemester-exc-closed-");
    try {
      // No primobalance — this isolates the closed-period branch.
      db.run(
        `INSERT INTO accounting_periods (period_start, period_end, kind, status)
         VALUES ('2025-01-01', '2025-12-31', 'fiscal_year', 'closed')`,
      );

      unmatchedException(db, bankTxn(db, "2025-06-01")); // inside closed period
      unmatchedException(db, bankTxn(db, "2026-06-01")); // outside → open task

      const open = listExceptions(db, { status: "open" });
      expect(open.count).toBe(1);

      const all = listExceptions(db, { status: "open", includeArchived: true });
      expect(all.count).toBe(2);
      const archived = all.rows.filter((r) => r.archived);
      expect(archived).toHaveLength(1);

      db.close();
    } finally {
      cleanupDir(root);
    }
  });

  test("with no cut-over and no closed period, nothing is archived", () => {
    const { root, db } = freshCompany("rentemester-exc-none-");
    try {
      unmatchedException(db, bankTxn(db, "2019-06-01"));
      unmatchedException(db, bankTxn(db, "2026-06-01"));

      const open = listExceptions(db, { status: "open" });
      expect(open.count).toBe(2);
      expect(open.rows.every((r) => r.archived === false)).toBe(true);

      db.close();
    } finally {
      cleanupDir(root);
    }
  });
});
