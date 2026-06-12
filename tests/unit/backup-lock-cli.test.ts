// Tests: src/cli.ts (opt-in backup lock guard)
import { describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { migrate, openDb } from "../../src/core/db";

import { cleanupDir } from "../helpers/cleanup";
describe("backup lock CLI guard", () => {
  test("blocks bookkeeping mutations when enforced and overdue, but never system commands", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-lock-cli-"));
    const company = join(root, "company");
    try {
      await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();

      // Old bookkeeping activity with no backup -> a weekly backup is overdue.
      const db = openDb(join(company, "data", "ledger.sqlite"));
      migrate(db);
      db.run(
        "INSERT INTO bank_transactions (transaction_date, booking_date, text, amount, currency, reference, import_batch_id, source_file_hash, transaction_hash) VALUES (?, ?, ?, ?, 'DKK', ?, ?, ?, ?)",
        "2026-01-01",
        "2026-01-01",
        "Old activity",
        100,
        "lock-cli-ref",
        "lock-cli-batch",
        "lock-cli-hash",
        "lock-cli-tx",
      );
      db.close();

      await Bun.$`bun run src/cli.ts system backup-lock --company ${company} --enforce true --grace-days 0`.quiet();

      // A bookkeeping mutation is refused with the lock message. The lock is
      // a *business rejection* (the call was well-formed; a precondition — a
      // fresh backup — is missing), so per docs/cli-contract.md it must exit
      // 1 with an { ok:false, errors:[...] } envelope on stdout — NOT exit 2
      // (which the contract reserves for "fix the call"). (#258)
      const blocked = Bun.spawn(
        ["bun", "run", "src/cli.ts", "journal", "post", "--company", company, "--input", "examples/journal-entry.expense.json", "--json"],
        { cwd: process.cwd(), stdout: "pipe", stderr: "pipe" },
      );
      const blockedOut = await new Response(blocked.stdout).text();
      const blockedExit = await blocked.exited;
      expect(blockedExit).toBe(1);
      const blockedResult = JSON.parse(blockedOut) as { ok: boolean; errors: string[] };
      expect(blockedResult.ok).toBe(false);
      expect(blockedResult.errors.join("\n")).toContain("Bogføring er låst");

      // `system backup` is exempt — backing up is the only way out of the lock.
      const backup = Bun.spawn(
        ["bun", "run", "src/cli.ts", "system", "backup", "--company", company, "--at", "2026-05-21T10:00:00.000Z"],
        { cwd: process.cwd(), stdout: "pipe", stderr: "pipe" },
      );
      const backupOut = await new Response(backup.stdout).text();
      const backupExit = await backup.exited;
      expect(backupExit).toBe(0);
      expect(JSON.parse(backupOut).ok).toBe(true);
    } finally {
      cleanupDir(root);
    }
  });
});
