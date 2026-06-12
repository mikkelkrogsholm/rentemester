// Tests: src/core/import/framework.ts — the import framework (#185).
//
// The framework maps a normalised intermediate representation (the parser
// contract output) onto the #179 primobalance target via `postOpeningBalance`.
// It validates (balanced, accounts exist) and produces a deterministic audit
// trail. Per-system parsers (e-conomic, Billy, Dinero #173) plug into the
// `SourceParser` contract; the framework itself is parser-agnostic.
import { describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { seedAccounts, verifyAuditChain } from "../../src/core/ledger";
import { getOpeningBalance, OPENING_BALANCE_TEXT } from "../../src/core/opening-balance";
import { runImport } from "../../src/core/import/framework";
import type { ImportSource } from "../../src/core/import/types";

import { cleanupDir } from "../helpers/cleanup";
function freshCompany(prefix: string) {
  const root = mkdtempSync(join(tmpdir(), prefix));
  const db = openDb(ensureCompanyDirs(root).db);
  migrate(db);
  seedAccounts(db);
  return { root, db };
}

// A minimal, balanced normalised source — what a per-system parser returns.
function balancedSource(): ImportSource {
  return {
    sourceSystem: "test-system",
    cutOverDate: "2026-01-01",
    chartOfAccounts: [
      { accountNo: "2000", name: "Bank" },
      { accountNo: "5000", name: "Egenkapital" },
    ],
    openingBalances: [
      { accountNo: "2000", debitAmount: 80000 },
      { accountNo: "5000", creditAmount: 80000 },
    ],
  };
}

describe("import framework: normalised source -> primobalance", () => {
  test("maps a balanced normalised source onto an audited primobalance", () => {
    const { root, db } = freshCompany("rentemester-import-ok-");
    try {
      const result = runImport(db, balancedSource(), { createdBy: "user:tester" });
      expect(result.errors).toEqual([]);
      expect(result.ok).toBe(true);
      expect(result.entryNo).toBeTruthy();
      expect(result.sourceSystem).toBe("test-system");
      expect(result.openingBalanceLineCount).toBe(2);

      // The framework landed the result through postOpeningBalance: a single
      // flagged primobalance entry exists and the audit chain is intact.
      const entry = db
        .query("SELECT text, transaction_date FROM journal_entries WHERE id = ?")
        .get(result.entryId!) as { text: string; transaction_date: string };
      expect(entry.transaction_date).toBe("2026-01-01");
      expect(entry.text.startsWith(OPENING_BALANCE_TEXT)).toBe(true);
      expect(verifyAuditChain(db).ok).toBe(true);

      const marker = getOpeningBalance(db);
      expect(marker!.cutOverDate).toBe("2026-01-01");
    } finally {
      db.close();
      cleanupDir(root);
    }
  });

  test("rejects an unbalanced source before posting anything", () => {
    const { root, db } = freshCompany("rentemester-import-unbal-");
    try {
      const source = balancedSource();
      source.openingBalances = [
        { accountNo: "2000", debitAmount: 80000 },
        { accountNo: "5000", creditAmount: 70000 },
      ];
      const result = runImport(db, source, { createdBy: "user:tester" });
      expect(result.ok).toBe(false);
      expect(result.errors.join(" ").toLowerCase()).toContain("balance");
      // Nothing was posted.
      expect(getOpeningBalance(db)).toBeNull();
      const count = db.query("SELECT COUNT(*) AS n FROM journal_entries").get() as { n: number };
      expect(count.n).toBe(0);
    } finally {
      db.close();
      cleanupDir(root);
    }
  });

  test("rejects an opening balance referencing an account not in the chart", () => {
    const { root, db } = freshCompany("rentemester-import-chart-");
    try {
      const source = balancedSource();
      source.openingBalances = [
        { accountNo: "2000", debitAmount: 80000 },
        { accountNo: "9999", creditAmount: 80000 },
      ];
      const result = runImport(db, source, { createdBy: "user:tester" });
      expect(result.ok).toBe(false);
      expect(result.errors.join(" ")).toContain("9999");
      expect(getOpeningBalance(db)).toBeNull();
    } finally {
      db.close();
      cleanupDir(root);
    }
  });

  test("rejects a line carrying both a debit and a credit amount", () => {
    const { root, db } = freshCompany("rentemester-import-twosided-");
    try {
      const source = balancedSource();
      source.openingBalances = [
        { accountNo: "2000", debitAmount: 80000, creditAmount: 10000 },
        { accountNo: "5000", creditAmount: 70000 },
      ];
      const result = runImport(db, source, { createdBy: "user:tester" });
      expect(result.ok).toBe(false);
      expect(result.errors.join(" ").toLowerCase()).toContain("debit");
      expect(getOpeningBalance(db)).toBeNull();
    } finally {
      db.close();
      cleanupDir(root);
    }
  });

  test("rejects a missing cut-over date", () => {
    const { root, db } = freshCompany("rentemester-import-date-");
    try {
      const source = balancedSource();
      source.cutOverDate = "";
      const result = runImport(db, source, { createdBy: "user:tester" });
      expect(result.ok).toBe(false);
      expect(result.errors.join(" ").toLowerCase()).toContain("cut-over");
    } finally {
      db.close();
      cleanupDir(root);
    }
  });

  test("rejects a source with no opening balances", () => {
    const { root, db } = freshCompany("rentemester-import-empty-");
    try {
      const source = balancedSource();
      source.openingBalances = [];
      const result = runImport(db, source, { createdBy: "user:tester" });
      expect(result.ok).toBe(false);
      expect(result.errors.length).toBeGreaterThan(0);
    } finally {
      db.close();
      cleanupDir(root);
    }
  });

  test("is deterministic: line order and audit trail are stable", () => {
    const { root: rootA, db: dbA } = freshCompany("rentemester-import-detA-");
    const { root: rootB, db: dbB } = freshCompany("rentemester-import-detB-");
    try {
      const a = runImport(dbA, balancedSource(), { createdBy: "user:tester" });
      const b = runImport(dbB, balancedSource(), { createdBy: "user:tester" });
      expect(a.ok).toBe(true);
      expect(b.ok).toBe(true);
      expect(a.entryNo).toBe(b.entryNo);
      expect(a.auditTrail).toEqual(b.auditTrail);
      expect(a.openingBalanceLineCount).toBe(b.openingBalanceLineCount);
    } finally {
      dbA.close();
      dbB.close();
      cleanupDir(rootA);
      cleanupDir(rootB);
    }
  });

  test("is idempotent: a second import is rejected (one primobalance per company)", () => {
    const { root, db } = freshCompany("rentemester-import-idem-");
    try {
      const first = runImport(db, balancedSource(), { createdBy: "user:tester" });
      expect(first.ok).toBe(true);
      const second = runImport(db, balancedSource(), { createdBy: "user:tester" });
      expect(second.ok).toBe(false);
      expect(second.errors.join(" ").toLowerCase()).toContain("already");
      const count = db.query("SELECT COUNT(*) AS n FROM journal_entries").get() as { n: number };
      expect(count.n).toBe(1);
    } finally {
      db.close();
      cleanupDir(root);
    }
  });
});
