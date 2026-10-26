import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { importBankCsv } from "../../src/core/bank";
import { actualBankBalanceAsOf, resolveActualBankBalanceAsOf } from "../../src/server/data/bank";
import { createSystemBackup } from "../../src/core/system-backups";
import { restoreSystemBackup } from "../../src/core/system-restore";

function setup() {
  const root = mkdtempSync(join(tmpdir(), "rentemester-statement-order-"));
  const db = openDb(ensureCompanyDirs(root).db);
  migrate(db);
  return { root, db };
}

const header = "date,text,amount,currency,balance_after";

describe("authoritative bank statement order (#596)", () => {
  test("uses persisted descending source order for a same-date closing balance and replays unchanged", () => {
    const { root, db } = setup();
    const source = join(root, "reverse.csv");
    // Common newest-first export: row one is the statement closing row.
    writeFileSync(source, [header,
      "2026-08-31,final,100.00,DKK,1000.00",
      "2026-08-31,middle,100.00,DKK,900.00",
      "2026-08-31,opening,100.00,DKK,800.00",
    ].join("\n"));
    const first = importBankCsv(db, root, source, { statementOrder: "descending" });
    expect(first).toMatchObject({ ok: true, imported: 3 });
    const resolution = resolveActualBankBalanceAsOf(db, "2026-08-31");
    expect(resolution.status).toBe("known");
    expect(resolution.balance).toBe(1000);
    expect(resolution.provenance).toEqual([expect.objectContaining({ sourceOrder: "descending", transactionDate: "2026-08-31" })]);
    expect(actualBankBalanceAsOf(db, "2026-08-31")).toBe(1000);
    const replay = importBankCsv(db, root, source, { statementOrder: "ascending" });
    expect(replay).toMatchObject({ ok: true, imported: 0, skippedDuplicates: 3 });
    expect(resolveActualBankBalanceAsOf(db, "2026-08-31").balance).toBe(1000);
    expect(() => db.run("UPDATE bank_transactions SET statement_order='ascending' WHERE id=1")).toThrow("immutable");
    db.close(); rmSync(root, { recursive: true, force: true });
  });

  test("supports ascending metadata and fails closed for an unknown same-date chain or inconsistent balances", () => {
    const { root, db } = setup();
    const source = join(root, "ascending.csv");
    writeFileSync(source, [header,
      "2026-08-30,opening,100.00,DKK,800.00",
      "2026-08-30,middle,100.00,DKK,900.00",
      "2026-08-30,final,100.00,DKK,1000.00",
    ].join("\n"));
    expect(importBankCsv(db, root, source, { statementOrder: "ascending" }).ok).toBe(true);
    expect(resolveActualBankBalanceAsOf(db, "2026-08-30")).toMatchObject({ status: "known", balance: 1000 });
    db.run("INSERT INTO bank_transactions(transaction_date,text,amount,currency,balance_after,transaction_hash) VALUES('2026-08-31','unknown A',1,'DKK',1001,'u-a')");
    db.run("INSERT INTO bank_transactions(transaction_date,text,amount,currency,balance_after,transaction_hash) VALUES('2026-08-31','unknown B',1,'DKK',1002,'u-b')");
    const ambiguous = resolveActualBankBalanceAsOf(db, "2026-08-31");
    expect(ambiguous.status).toBe("ambiguous");
    expect(ambiguous.balance).toBeNull();
    expect(ambiguous.diagnostics.join(" ")).toContain("ambiguous statement order");
    db.close(); rmSync(root, { recursive: true, force: true });
  });

  test("fails closed for a mixed known/unknown multi-account total and colliding sources", () => {
    const { root, db } = setup();
    db.run("INSERT INTO bank_accounts(slug,name,currency) VALUES('a','A','DKK')");
    db.run("INSERT INTO bank_accounts(slug,name,currency) VALUES('b','B','DKK')");
    db.run("INSERT INTO bank_transactions(transaction_date,text,amount,currency,balance_after,transaction_hash,bank_account_id) VALUES('2026-08-31','known',1,'DKK',101,'known',1)");
    db.run("INSERT INTO bank_transactions(transaction_date,text,amount,currency,transaction_hash,bank_account_id) VALUES('2026-08-31','missing',1,'DKK','missing',2)");
    expect(resolveActualBankBalanceAsOf(db, "2026-08-31")).toMatchObject({ status: "ambiguous", balance: null });
    db.close(); rmSync(root, { recursive: true, force: true });
    const second = setup();
    const one = join(second.root, "one.csv");
    const two = join(second.root, "two.csv");
    writeFileSync(one, [header, "2026-08-31,source one,1,DKK,100"].join("\n"));
    writeFileSync(two, [header, "2026-08-31,source two,1,DKK,101"].join("\n"));
    expect(importBankCsv(second.db, second.root, one, { statementOrder: "ascending" }).ok).toBe(true);
    expect(importBankCsv(second.db, second.root, two, { statementOrder: "ascending" }).ok).toBe(true);
    const collision = resolveActualBankBalanceAsOf(second.db, "2026-08-31");
    expect(collision.status).toBe("ambiguous");
    expect(collision.diagnostics.join(" ")).toContain("ambiguous statement order");
    second.db.close(); rmSync(second.root, { recursive: true, force: true });
  });

  test("keeps source-order semantics after reopen and verified backup restore", () => {
    const { root, db } = setup();
    const source = join(root, "statement.csv");
    writeFileSync(source, [header, "2026-08-31,final,1,DKK,101", "2026-08-31,opening,1,DKK,100"].join("\n"));
    expect(importBankCsv(db, root, source, { statementOrder: "descending" }).ok).toBe(true);
    const backup = createSystemBackup(db, root, { createdAt: "2026-08-31T12:00:00.000Z" });
    expect(backup.ok).toBe(true);
    db.close();
    const reopened = openDb(ensureCompanyDirs(root).db);
    migrate(reopened);
    expect(resolveActualBankBalanceAsOf(reopened, "2026-08-31")).toMatchObject({ status: "known", balance: 101 });
    reopened.close();
    const restoredRoot = mkdtempSync(join(tmpdir(), "rentemester-statement-order-restored-"));
    const restored = restoreSystemBackup({ backupDir: backup.backupDir!, targetCompanyRoot: restoredRoot });
    expect(restored.ok, restored.errors.join("; ")).toBe(true);
    const restoredDb = openDb(restored.restoredDbPath!);
    expect(resolveActualBankBalanceAsOf(restoredDb, "2026-08-31")).toMatchObject({ status: "known", balance: 101 });
    restoredDb.close(); rmSync(root, { recursive: true, force: true }); rmSync(restoredRoot, { recursive: true, force: true });
  });
});
