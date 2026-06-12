// Tests: src/core/bank.ts (bank accounts as a first-class entity, #187)
import { describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { addBankAccount, listBankAccounts, importBankCsv, resolveBankAccount } from "../../src/core/bank";
import { listBankTransactions, buildBankReconciliationReport } from "../../src/core/reconciliation";

import { cleanupDir } from "../helpers/cleanup";
function setup() {
  const root = mkdtempSync(join(tmpdir(), "rentemester-bankacct-"));
  const db = openDb(ensureCompanyDirs(root).db);
  migrate(db);
  return { root, db };
}

function writeCsv(root: string, name: string, lines: string[]) {
  const path = join(root, name);
  writeFileSync(path, lines.join("\n"));
  return path;
}

describe("bank accounts (#187)", () => {
  test("adds a bank account and lists it", () => {
    const { root, db } = setup();
    const result = addBankAccount(db, { name: "Driftskonto DKK", bankName: "Danske Bank" });
    expect(result.ok).toBe(true);
    expect(result.account?.slug).toBe("driftskonto-dkk");
    expect(result.account?.currency).toBe("DKK");

    const listed = listBankAccounts(db);
    expect(listed.count).toBe(1);
    expect(listed.accounts[0].name).toBe("Driftskonto DKK");

    expect(resolveBankAccount(db, "driftskonto-dkk")?.id).toBe(result.account!.id);
    expect(resolveBankAccount(db, result.account!.id)?.slug).toBe("driftskonto-dkk");
    db.close();
    cleanupDir(root);
  });

  test("rejects duplicate slugs", () => {
    const { root, db } = setup();
    expect(addBankAccount(db, { name: "Drift" }).ok).toBe(true);
    const dup = addBankAccount(db, { name: "Drift" });
    expect(dup.ok).toBe(false);
    expect(dup.errors.some((e) => e.includes("already exists"))).toBe(true);
    db.close();
    cleanupDir(root);
  });

  test("importing into two accounts keeps rows separated", () => {
    const { root, db } = setup();
    const drift = addBankAccount(db, { name: "Drift" }).account!;
    const opspar = addBankAccount(db, { name: "Opsparing" }).account!;

    const csv = writeCsv(root, "tx.csv", [
      "transaction_date,booking_date,text,amount,currency,reference",
      "2026-05-16,2026-05-16,Card payment,-1250,DKK,REF-1",
    ]);

    const a = importBankCsv(db, root, csv, { account: drift.id });
    const b = importBankCsv(db, root, csv, { account: opspar.slug });
    expect(a.ok).toBe(true);
    expect(b.ok).toBe(true);
    expect(a.imported).toBe(1);
    // Identical transaction in a DIFFERENT account is not a duplicate.
    expect(b.imported).toBe(1);
    expect(b.skippedDuplicates).toBe(0);

    const driftRows = listBankTransactions(db, { bankAccountId: drift.id });
    const opsparRows = listBankTransactions(db, { bankAccountId: opspar.id });
    expect(driftRows.count).toBe(1);
    expect(opsparRows.count).toBe(1);
    expect(driftRows.rows[0].bankAccountId).toBe(drift.id);
    expect(opsparRows.rows[0].bankAccountId).toBe(opspar.id);
    db.close();
    cleanupDir(root);
  });

  test("re-import into the same account is still deduplicated", () => {
    const { root, db } = setup();
    const drift = addBankAccount(db, { name: "Drift" }).account!;
    const csv = writeCsv(root, "tx.csv", [
      "transaction_date,booking_date,text,amount,currency,reference",
      "2026-05-16,2026-05-16,Card payment,-1250,DKK,REF-1",
    ]);
    expect(importBankCsv(db, root, csv, { account: drift.id }).imported).toBe(1);
    const second = importBankCsv(db, root, csv, { account: drift.id });
    expect(second.imported).toBe(0);
    expect(second.skippedDuplicates).toBe(1);
    db.close();
    cleanupDir(root);
  });

  test("--account filter scopes list and reconcile bank", () => {
    const { root, db } = setup();
    const drift = addBankAccount(db, { name: "Drift" }).account!;
    const opspar = addBankAccount(db, { name: "Opsparing" }).account!;
    importBankCsv(db, root, writeCsv(root, "d.csv", [
      "transaction_date,booking_date,text,amount,currency,reference",
      "2026-05-16,2026-05-16,Drift payment,-1250,DKK,D-1",
    ]), { account: drift.id });
    importBankCsv(db, root, writeCsv(root, "o.csv", [
      "transaction_date,booking_date,text,amount,currency,reference",
      "2026-05-17,2026-05-17,Opspar deposit,500,DKK,O-1",
    ]), { account: opspar.id });

    expect(listBankTransactions(db, {}).count).toBe(2);
    expect(listBankTransactions(db, { bankAccountId: drift.id }).count).toBe(1);

    const report = buildBankReconciliationReport(db, "2026-05-01", "2026-05-31", { bankAccountId: opspar.id });
    expect(report.unmatchedCount).toBe(1);
    expect(report.unmatched[0].text).toBe("Opspar deposit");
    db.close();
    cleanupDir(root);
  });

  test("import rejects an unknown account", () => {
    const { root, db } = setup();
    const csv = writeCsv(root, "tx.csv", [
      "transaction_date,booking_date,text,amount,currency,reference",
      "2026-05-16,2026-05-16,Card payment,-1250,DKK,REF-1",
    ]);
    const result = importBankCsv(db, root, csv, { account: "no-such-account" });
    expect(result.ok).toBe(false);
    expect(result.errors.some((e) => e.includes("does not exist"))).toBe(true);
    db.close();
    cleanupDir(root);
  });
});
