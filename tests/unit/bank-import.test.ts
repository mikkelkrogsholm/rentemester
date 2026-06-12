// Tests: src/core/bank.ts (bank CSV import)
import { describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { importBankCsv } from "../../src/core/bank";

import { cleanupDir } from "../helpers/cleanup";
describe("bank import", () => {
  test("stores null FX columns for DKK-only rows and skips deterministic duplicates", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-bank-"));
    const csv = join(root, "transactions.csv");
    writeFileSync(csv, [
      "transaction_date,booking_date,text,amount,currency,reference",
      "2026-05-16,2026-05-17,Card payment,-1250,DKK,REF-1",
      "2026-05-18,2026-05-18,Customer payment,2500,DKK,REF-2"
    ].join("\n"));

    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);

    const first = importBankCsv(db, root, csv);
    expect(first.ok).toBe(true);
    expect(first.imported).toBe(2);
    expect(first.skippedDuplicates).toBe(0);

    const second = importBankCsv(db, root, csv);
    expect(second.ok).toBe(true);
    expect(second.imported).toBe(0);
    expect(second.skippedDuplicates).toBe(2);

    const rows = db.query("SELECT transaction_date, text, amount, amount_dkk, fx_rate_to_dkk, import_batch_id, transaction_hash FROM bank_transactions ORDER BY id ASC").all() as any[];
    expect(rows).toHaveLength(2);
    expect(rows[0].transaction_date).toBe("2026-05-16");
    expect(rows[0].transaction_hash).toBeTruthy();
    expect(rows[0].amount_dkk).toBeNull();
    expect(rows[0].fx_rate_to_dkk).toBeNull();
    expect(rows[1].text).toBe("Customer payment");
    expect(rows[1].amount_dkk).toBeNull();
    expect(rows[1].fx_rate_to_dkk).toBeNull();

    db.close();
    cleanupDir(root);
  });

  test("imports non-DKK rows when DKK amount and FX rate are supplied", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-bank-fx-"));
    const csv = join(root, "fx.csv");
    writeFileSync(csv, [
      "transaction_date,booking_date,text,amount,currency,amount_dkk,fx_rate_to_dkk,reference",
      "2026-05-19,2026-05-19,Stripe payout,100,EUR,746,7.46,EUR-REF-1"
    ].join("\n"));

    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);

    const result = importBankCsv(db, root, csv);
    expect(result.ok).toBe(true);
    const rows = db.query("SELECT currency, amount, amount_dkk, fx_rate_to_dkk FROM bank_transactions ORDER BY id ASC").all() as any[];
    expect(rows).toHaveLength(1);
    expect(rows[0]).toEqual({ currency: "EUR", amount: 100, amount_dkk: 746, fx_rate_to_dkk: 7.46 });

    db.close();
    cleanupDir(root);
  });

  test("rejects impossible calendar dates in bank rows", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-bank-baddate-"));
    const csv = join(root, "bad-date.csv");
    writeFileSync(csv, [
      "transaction_date,booking_date,text,amount,currency,reference",
      "2026-02-30,2026-04-31,Customer payment,2500,DKK,REF-1"
    ].join("\n"));

    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    const result = importBankCsv(db, root, csv);
    expect(result.ok).toBe(false);
    expect(result.errors).toContain("rows[0].transactionDate must be YYYY-MM-DD");
    expect(result.errors).toContain("rows[0].bookingDate must be YYYY-MM-DD when present");

    db.close();
    cleanupDir(root);
  });

  test("imports quoted fields with commas and escaped quotes", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-bank-quotes-"));
    const csv = join(root, "quoted.csv");
    writeFileSync(csv, [
      "transaction_date,booking_date,text,amount,currency,reference",
      "2026-05-16,2026-05-17,\"Nordea, faktura \"\"A-42\"\"\",-1250,DKK,REF-1"
    ].join("\n"));

    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    const result = importBankCsv(db, root, csv);
    expect(result.ok).toBe(true);
    expect(result.imported).toBe(1);
    const row = db.query("SELECT text, amount FROM bank_transactions ORDER BY id ASC LIMIT 1").get() as any;
    expect(row).toEqual({ text: "Nordea, faktura \"A-42\"", amount: -1250 });

    db.close();
    cleanupDir(root);
  });

  test("imports Danish bank headers with semicolon delimiter and European amounts", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-bank-danish-"));
    const csv = join(root, "nordea.csv");
    writeFileSync(csv, [
      "Bogføringsdato;Rentedato;Tekst;Beløb;Valuta;Reference",
      "17-05-2026;16-05-2026;\"Kunde, faktura 100\";\"1.234,56\";DKK;N-100"
    ].join("\n"));

    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    const result = importBankCsv(db, root, csv);
    expect(result.ok).toBe(true);
    expect(result.imported).toBe(1);
    const row = db.query("SELECT transaction_date, booking_date, text, amount, reference FROM bank_transactions ORDER BY id ASC LIMIT 1").get() as any;
    expect(row).toEqual({ transaction_date: "2026-05-17", booking_date: "2026-05-16", text: "Kunde, faktura 100", amount: 1234.56, reference: "N-100" });

    db.close();
    cleanupDir(root);
  });

  test("returns header and row-shape errors close to the CSV root cause", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-bank-header-"));
    const csv = join(root, "bad-header.csv");
    writeFileSync(csv, [
      "Dato,Tekst,Saldo",
      "2026-05-16,Payment,999,EXTRA"
    ].join("\n"));

    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    const result = importBankCsv(db, root, csv);
    expect(result.ok).toBe(false);
    expect(result.errors).toContain("CSV header missing required column: amount (accepted: amount, beløb, belob)");
    expect(result.errors).toContain("CSV row 2 has 4 fields, header has 3");

    db.close();
    cleanupDir(root);
  });

  test("parses Danish thousands-only amounts as whole kroner (issue #130)", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-bank-thousands-"));
    const csv = join(root, "thousands.csv");
    writeFileSync(csv, [
      "transaction_date,booking_date,text,amount,currency,reference",
      "2026-05-16,2026-05-17,Card payment,1.234,DKK,REF-1"
    ].join("\n"));

    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    const result = importBankCsv(db, root, csv);
    expect(result.ok).toBe(true);
    const row = db.query("SELECT amount FROM bank_transactions ORDER BY id ASC LIMIT 1").get() as any;
    expect(row.amount).toBe(1234);

    db.close();
    cleanupDir(root);
  });

  test("rejects hex and scientific-notation amounts (issue #130)", () => {
    for (const bad of ["0xff", "1e3"]) {
      const root = mkdtempSync(join(tmpdir(), "rentemester-bank-garbage-"));
      const csv = join(root, "garbage.csv");
      writeFileSync(csv, [
        "transaction_date,booking_date,text,amount,currency,reference",
        `2026-05-16,2026-05-17,Card payment,${bad},DKK,REF-1`
      ].join("\n"));

      const db = openDb(ensureCompanyDirs(root).db);
      migrate(db);
      const result = importBankCsv(db, root, csv);
      expect(result.ok).toBe(false);
      expect(result.errors.some((e) => e.includes("amount"))).toBe(true);

      db.close();
      cleanupDir(root);
    }
  });

  test("rejects ambiguous dd-mm/mm-dd dates rather than guessing (issue #137)", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-bank-ambig-"));
    const csv = join(root, "ambig.csv");
    writeFileSync(csv, [
      "transaction_date,booking_date,text,amount,currency,reference",
      "05/04/2026,05/04/2026,Card payment,-1250,DKK,REF-1"
    ].join("\n"));

    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    const result = importBankCsv(db, root, csv);
    expect(result.ok).toBe(false);
    expect(result.errors.some((e) => e.includes("transactionDate"))).toBe(true);

    db.close();
    cleanupDir(root);
  });

  test("keeps reformatting unambiguous DD-MM-YYYY dates (issue #137)", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-bank-unambig-"));
    const csv = join(root, "unambig.csv");
    writeFileSync(csv, [
      "transaction_date,booking_date,text,amount,currency,reference",
      "17-05-2026,16-05-2026,Card payment,-1250,DKK,REF-1"
    ].join("\n"));

    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    const result = importBankCsv(db, root, csv);
    expect(result.ok).toBe(true);
    const row = db.query("SELECT transaction_date, booking_date FROM bank_transactions ORDER BY id ASC LIMIT 1").get() as any;
    expect(row).toEqual({ transaction_date: "2026-05-17", booking_date: "2026-05-16" });

    db.close();
    cleanupDir(root);
  });

  test("rejects CSV with duplicate canonical headers (issue #150)", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-bank-duphdr-"));
    const csv = join(root, "dup.csv");
    // both "dato" and "date" canonicalise to transaction_date
    writeFileSync(csv, [
      "dato,date,text,amount,currency,reference",
      "2026-05-16,2026-05-17,Card payment,-1250,DKK,REF-1"
    ].join("\n"));

    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    const result = importBankCsv(db, root, csv);
    expect(result.ok).toBe(false);
    expect(result.errors.some((e) => e.includes("duplicate canonical column: transaction_date"))).toBe(true);

    db.close();
    cleanupDir(root);
  });

  test("imports two legitimately-distinct identical same-day fees, still dedups re-import (issue #155)", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-bank-dedup-"));
    const csv = join(root, "fees.csv");
    writeFileSync(csv, [
      "transaction_date,booking_date,text,amount,currency,reference",
      "2026-05-16,2026-05-16,Fee,-50,DKK,",
      "2026-05-16,2026-05-16,Fee,-50,DKK,"
    ].join("\n"));

    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    const first = importBankCsv(db, root, csv);
    expect(first.ok).toBe(true);
    expect(first.imported).toBe(2);
    expect(first.skippedDuplicates).toBe(0);
    expect(first.skippedDuplicateRows ?? []).toEqual([]);

    // re-importing the same file must still dedup deterministically
    const second = importBankCsv(db, root, csv);
    expect(second.ok).toBe(true);
    expect(second.imported).toBe(0);
    expect(second.skippedDuplicates).toBe(2);
    expect((second.skippedDuplicateRows ?? []).length).toBe(2);

    db.close();
    cleanupDir(root);
  });

  test("rejects malformed bank rows", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-bank-bad-"));
    const csv = join(root, "bad.csv");
    writeFileSync(csv, [
      "transaction_date,booking_date,text,amount,currency,amount_dkk,fx_rate_to_dkk,reference",
      "32-05-2026,,,-abc,EUR,,,REF-1"
    ].join("\n"));

    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    const result = importBankCsv(db, root, csv);
    expect(result.ok).toBe(false);
    expect(result.errors.some((e) => e.includes("transactionDate"))).toBe(true);
    expect(result.errors.some((e) => e.includes("amount"))).toBe(true);
    expect(result.errors.some((e) => e.includes("amountDkk"))).toBe(true);
    expect(result.errors.some((e) => e.includes("fxRateToDkk"))).toBe(true);

    db.close();
    cleanupDir(root);
  });
});
