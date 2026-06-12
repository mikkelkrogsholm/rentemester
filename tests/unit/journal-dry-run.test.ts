// Tests: src/core/ledger.ts (dryRunJournalEntry — non-binding posting preview)
// Companion of journal-post.test.ts.
import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { dryRunJournalEntry, postJournalEntry, seedAccounts } from "../../src/core/ledger";

function freshDb(label: string) {
  const root = mkdtempSync(join(tmpdir(), `rentemester-${label}-`));
  const db = openDb(ensureCompanyDirs(root).db);
  migrate(db);
  seedAccounts(db);
  return { root, db };
}

describe("journal dry run", () => {
  test("previews a valid entry without writing to the ledger", () => {
    const { root, db } = freshDb("dryrun-clean");

    const preview = dryRunJournalEntry(db, {
      transactionDate: "2026-05-16",
      text: "Owner contribution",
      lines: [
        { accountNo: "2000", debitAmount: 1000 },
        { accountNo: "5020", creditAmount: 1000 },
      ],
    });

    expect(preview.ok).toBe(true);
    expect(preview.entryNo).toBe("2026-00001");
    expect(preview.previousHash).toBe("GENESIS");
    expect(preview.entryHash).toMatch(/^[0-9a-f]{64}$/);
    expect(preview.accountEffects).toEqual([
      { accountNo: "2000", accountName: "Bank", balanceBefore: 0, balanceAfter: 1000, delta: 1000 },
      { accountNo: "5020", accountName: "Privat indskud", balanceBefore: 0, balanceAfter: -1000, delta: -1000 },
    ]);

    // The dry run must not touch the append-only ledger at all.
    const entryCount = db.query("SELECT COUNT(*) AS n FROM journal_entries").get() as { n: number };
    expect(entryCount.n).toBe(0);
    const auditCount = db.query("SELECT COUNT(*) AS n FROM audit_log WHERE event_type = 'journal_post'").get() as { n: number };
    expect(auditCount.n).toBe(0);

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("does not burn a journal number — a later real post still gets 2026-00001", () => {
    const { root, db } = freshDb("dryrun-seq");

    dryRunJournalEntry(db, {
      transactionDate: "2026-05-16",
      text: "Dry run only",
      lines: [
        { accountNo: "2000", debitAmount: 500 },
        { accountNo: "5020", creditAmount: 500 },
      ],
    });

    const sequence = db.query("SELECT value FROM sequences WHERE kind = 'journal_entry'").get() as { value: number } | null;
    expect(sequence).toBeNull();

    const posted = postJournalEntry(db, {
      transactionDate: "2026-05-16",
      text: "First real post",
      lines: [
        { accountNo: "2000", debitAmount: 500 },
        { accountNo: "5020", creditAmount: 500 },
      ],
    });
    expect(posted.ok).toBe(true);
    expect(posted.entryNo).toBe("2026-00001");

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("reports validation errors for an invalid entry without an entry preview", () => {
    const { root, db } = freshDb("dryrun-invalid");

    const preview = dryRunJournalEntry(db, {
      transactionDate: "2026-05-16",
      text: "Broken posting",
      lines: [
        { accountNo: "2000", debitAmount: 1000 },
        { accountNo: "5020", creditAmount: 900 },
      ],
    });

    expect(preview.ok).toBe(false);
    expect(preview.errors).toContain("journal entry must balance: debit 1000 != credit 900");
    expect(preview.entryNo).toBeUndefined();
    expect(preview.entryHash).toBeUndefined();
    expect(preview.accountEffects).toBeUndefined();

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("prediction matches the real post that follows it", () => {
    const { root, db } = freshDb("dryrun-faithful");

    const payload = {
      transactionDate: "2026-05-16",
      text: "Owner contribution",
      lines: [
        { accountNo: "2000", debitAmount: 1000 },
        { accountNo: "5020", creditAmount: 1000 },
      ],
    };

    const preview = dryRunJournalEntry(db, payload);
    const posted = postJournalEntry(db, payload);

    expect(posted.ok).toBe(true);
    expect(preview.entryNo).toBe(posted.entryNo);
    expect(preview.entryId).toBe(posted.entryId);
    expect(preview.entryHash).toBe(posted.entryHash);

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("account effects reflect balances already in the ledger", () => {
    const { root, db } = freshDb("dryrun-effects");

    postJournalEntry(db, {
      transactionDate: "2026-05-16",
      text: "First contribution",
      lines: [
        { accountNo: "2000", debitAmount: 1000 },
        { accountNo: "5020", creditAmount: 1000 },
      ],
    });

    const preview = dryRunJournalEntry(db, {
      transactionDate: "2026-05-17",
      text: "Second contribution",
      lines: [
        { accountNo: "2000", debitAmount: 500 },
        { accountNo: "5020", creditAmount: 500 },
      ],
    });

    expect(preview.ok).toBe(true);
    expect(preview.previousHash).not.toBe("GENESIS");
    expect(preview.accountEffects).toEqual([
      { accountNo: "2000", accountName: "Bank", balanceBefore: 1000, balanceAfter: 1500, delta: 500 },
      { accountNo: "5020", accountName: "Privat indskud", balanceBefore: -1000, balanceAfter: -1500, delta: -500 },
    ]);

    db.close();
    rmSync(root, { recursive: true, force: true });
  });
});
