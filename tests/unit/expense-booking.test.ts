// Tests: src/core/expense-booking.ts
import { describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { seedAccounts } from "../../src/core/ledger";
import { importBankCsv } from "../../src/core/bank";
import { ingestDocument } from "../../src/core/documents";
import { buildBankReconciliationReport } from "../../src/core/reconciliation";
import { bookExpenseFromBank } from "../../src/core/expense-booking";
import { storeViesValidation } from "../../src/core/vies";

import { cleanupDir } from "../helpers/cleanup";
describe("expense booking", () => {
  test("books a standard vendor expense from document + bank transaction and reconciles it", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-expense-book-"));
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-expense-book-inbox-"));
    const csv = join(root, "transactions.csv");
    const sourceFile = join(inbox, "vendor.txt");
    writeFileSync(csv, [
      "transaction_date,booking_date,text,amount,currency,reference",
      "2026-05-16,2026-05-16,SOFTWARE APS,-1250,DKK,REF-EXP-1"
    ].join("\n"));
    writeFileSync(sourceFile, "Invoice\n1250 DKK\n");

    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    const bank = importBankCsv(db, root, csv);
    expect(bank.ok).toBe(true);

    const doc = ingestDocument(db, root, sourceFile, {
      source: "email",
      issueDate: "2026-05-16",
      invoiceNo: "V-1001",
      deliveryDescription: "Softwareabonnement",
      amountIncVat: 1250,
      currency: "DKK",
      sender: { name: "Software ApS", address: "SaaSvej 1", vatOrCvr: "DK11223344" },
      recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      vatAmount: 250,
      paymentDetails: "Bank transfer"
    });
    expect(doc.ok).toBe(true);

    const bankRow = db.query("SELECT id FROM bank_transactions WHERE reference = 'REF-EXP-1'").get() as { id: number };
    const booked = bookExpenseFromBank(db, {
      documentId: doc.documentId!,
      bankTransactionId: bankRow.id,
      expenseAccountNo: "3000"
    });

    expect(booked.ok).toBe(true);
    expect(booked.grossAmount).toBe(1250);
    expect(booked.netAmount).toBe(1000);
    expect(booked.vatAmount).toBe(250);
    expect(booked.vatTreatment).toBe("standard");

    const lines = db.query(
      `SELECT a.account_no, jl.debit_amount, jl.credit_amount, jl.vat_code
       FROM journal_lines jl
       JOIN accounts a ON a.id = jl.account_id
       WHERE jl.journal_entry_id = ?
       ORDER BY jl.id ASC`
    ).all(booked.entryId!) as any[];
    expect(lines).toEqual([
      { account_no: "3000", debit_amount: 1000, credit_amount: 0, vat_code: "DK_PURCHASE_25" },
      { account_no: "4000", debit_amount: 250, credit_amount: 0, vat_code: null },
      { account_no: "2000", debit_amount: 0, credit_amount: 1250, vat_code: null },
    ]);

    const report = buildBankReconciliationReport(db, "2026-05-01", "2026-05-31");
    expect(report.matchedCount).toBe(1);
    expect(report.unmatchedCount).toBe(0);
    expect(report.matched[0].bankTransactionId).toBe(bankRow.id);

    db.close();
    cleanupDir(root);
    cleanupDir(inbox);
  });

  test("rejects a standard expense whose document vat_amount is inconsistent with the 25% rate (#143)", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-expense-book-badvat-"));
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-expense-book-badvat-inbox-"));
    const csv = join(root, "transactions.csv");
    const sourceFile = join(inbox, "vendor.txt");
    writeFileSync(csv, [
      "transaction_date,booking_date,text,amount,currency,reference",
      "2026-05-16,2026-05-16,SOFTWARE APS,-1250,DKK,REF-BADVAT-1"
    ].join("\n"));
    writeFileSync(sourceFile, "Invoice\n1250 DKK\n");

    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    const bank = importBankCsv(db, root, csv);
    expect(bank.ok).toBe(true);

    // Document carries a garbled vat_amount (251) — gross 1250, so net would
    // be 999 and 25% of 999 = 249.75 → 250, not 251. Must be rejected.
    const doc = ingestDocument(db, root, sourceFile, {
      source: "email",
      issueDate: "2026-05-16",
      invoiceNo: "V-BADVAT-1",
      deliveryDescription: "Softwareabonnement",
      amountIncVat: 1250,
      currency: "DKK",
      sender: { name: "Software ApS", address: "SaaSvej 1", vatOrCvr: "DK11223344" },
      recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      vatAmount: 251,
      paymentDetails: "Bank transfer"
    });
    expect(doc.ok).toBe(true);

    const bankRow = db.query("SELECT id FROM bank_transactions WHERE reference = 'REF-BADVAT-1'").get() as { id: number };
    const booked = bookExpenseFromBank(db, {
      documentId: doc.documentId!,
      bankTransactionId: bankRow.id,
      expenseAccountNo: "3000"
    });

    expect(booked.ok).toBe(false);
    expect(booked.errors.join(" ")).toContain("vat_amount");
    expect(booked.entryId).toBeUndefined();

    db.close();
    cleanupDir(root);
    cleanupDir(inbox);
  });

  test("books a foreign-currency purchase settled by a DKK bank transaction and preserves FX basis", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-expense-book-fx-"));
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-expense-book-fx-inbox-"));
    const csv = join(root, "transactions.csv");
    const sourceFile = join(inbox, "vendor.txt");
    writeFileSync(csv, [
      "transaction_date,booking_date,text,amount,currency,fx_rate_to_dkk,reference",
      "2026-05-16,2026-05-16,CLOUD VENDOR,-746,DKK,7.46,REF-FX-1"
    ].join("\n"));
    writeFileSync(sourceFile, "Invoice\n100 EUR\n");

    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    const bank = importBankCsv(db, root, csv);
    expect(bank.ok).toBe(true);

    const doc = ingestDocument(db, root, sourceFile, {
      source: "email",
      issueDate: "2026-05-16",
      invoiceNo: "FX-1001",
      deliveryDescription: "Cloud subscription",
      amountIncVat: 100,
      currency: "EUR",
      sender: { name: "Cloud Vendor GmbH", address: "Berlin", vatOrCvr: "DE123456789" },
      recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      vatAmount: 0,
      paymentDetails: "Card payment"
    });
    expect(doc.ok).toBe(true);

    const bankRow = db.query("SELECT id FROM bank_transactions WHERE reference = 'REF-FX-1'").get() as { id: number };
    const booked = bookExpenseFromBank(db, {
      documentId: doc.documentId!,
      bankTransactionId: bankRow.id,
      expenseAccountNo: "3000",
      vatTreatment: "exempt"
    });

    expect(booked.ok).toBe(true);
    expect(booked.grossAmount).toBe(100);
    expect(booked.netAmount).toBe(746);
    expect(booked.vatAmount).toBe(0);
    expect(booked.vatTreatment).toBe("exempt");

    const entry = db.query("SELECT currency, amount_foreign, amount_dkk, fx_rate_to_dkk FROM journal_entries WHERE id = ?").get(booked.entryId!) as any;
    expect(entry).toEqual({ currency: "EUR", amount_foreign: 100, amount_dkk: 746, fx_rate_to_dkk: 7.46 });

    db.close();
    cleanupDir(root);
    cleanupDir(inbox);
  });

  test("blocks foreign-currency expense booking when the DKK settlement lacks FX basis", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-expense-book-fx-missing-"));
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-expense-book-fx-missing-inbox-"));
    const csv = join(root, "transactions.csv");
    const sourceFile = join(inbox, "vendor.txt");
    writeFileSync(csv, [
      "transaction_date,booking_date,text,amount,currency,reference",
      "2026-05-16,2026-05-16,CLOUD VENDOR,-746,DKK,REF-FX-MISSING"
    ].join("\n"));
    writeFileSync(sourceFile, "Invoice\n100 EUR\n");

    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    const bank = importBankCsv(db, root, csv);
    expect(bank.ok).toBe(true);

    const doc = ingestDocument(db, root, sourceFile, {
      source: "email",
      issueDate: "2026-05-16",
      invoiceNo: "FX-1002",
      deliveryDescription: "Cloud subscription",
      amountIncVat: 100,
      currency: "EUR",
      sender: { name: "Cloud Vendor GmbH", address: "Berlin", vatOrCvr: "DE123456789" },
      recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      vatAmount: 0,
      paymentDetails: "Card payment"
    });
    expect(doc.ok).toBe(true);

    const bankRow = db.query("SELECT id FROM bank_transactions WHERE reference = 'REF-FX-MISSING'").get() as { id: number };
    const booked = bookExpenseFromBank(db, {
      documentId: doc.documentId!,
      bankTransactionId: bankRow.id,
      expenseAccountNo: "3000",
      vatTreatment: "exempt"
    });

    expect(booked.ok).toBe(false);
    expect(booked.errors).toContain("foreign-currency expense booking requires bank fx_rate_to_dkk for DKK-settled payments");

    db.close();
    cleanupDir(root);
    cleanupDir(inbox);
  });

  test("surfaces an error instead of silently booking an unmapped VAT code as exempt (#153)", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-expense-book-unknown-"));
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-expense-book-unknown-inbox-"));
    const csv = join(root, "transactions.csv");
    const sourceFile = join(inbox, "vendor.txt");
    writeFileSync(csv, [
      "transaction_date,booking_date,text,amount,currency,reference",
      "2026-05-16,2026-05-16,VENDOR APS,-1250,DKK,REF-UNKNOWN-1"
    ].join("\n"));
    writeFileSync(sourceFile, "Invoice\n1250 DKK\n");

    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    const bank = importBankCsv(db, root, csv);
    expect(bank.ok).toBe(true);

    const doc = ingestDocument(db, root, sourceFile, {
      source: "email",
      issueDate: "2026-05-16",
      invoiceNo: "V-UNKNOWN-1",
      deliveryDescription: "Diverse",
      amountIncVat: 1250,
      currency: "DKK",
      sender: { name: "Vendor ApS", address: "Vej 1", vatOrCvr: "DK11223344" },
      recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      vatAmount: 250,
      paymentDetails: "Bank transfer"
    });
    expect(doc.ok).toBe(true);

    // Account 3080 has default_vat_code DK_BAD_DEBT_25, which inferVatTreatment
    // does not map. It must not be silently treated as exempt; the caller must
    // be forced to pass an explicit vatTreatment.
    const bankRow = db.query("SELECT id FROM bank_transactions WHERE reference = 'REF-UNKNOWN-1'").get() as { id: number };
    const booked = bookExpenseFromBank(db, {
      documentId: doc.documentId!,
      bankTransactionId: bankRow.id,
      expenseAccountNo: "3080"
    });

    expect(booked.ok).toBe(false);
    expect(booked.errors.join(" ")).toContain("DK_BAD_DEBT_25");
    expect(booked.entryId).toBeUndefined();

    // With an explicit vatTreatment the booking proceeds.
    const explicit = bookExpenseFromBank(db, {
      documentId: doc.documentId!,
      bankTransactionId: bankRow.id,
      expenseAccountNo: "3080",
      vatTreatment: "standard"
    });
    expect(explicit.ok).toBe(true);

    db.close();
    cleanupDir(root);
    cleanupDir(inbox);
  });

  test("uses reverse-charge flow when the expense account defaults to EU reverse charge", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-expense-book-rc-"));
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-expense-book-rc-inbox-"));
    const csv = join(root, "transactions.csv");
    const sourceFile = join(inbox, "vendor.txt");
    writeFileSync(csv, [
      "transaction_date,booking_date,text,amount,currency,reference",
      "2026-05-16,2026-05-16,EU SUPPLIER,-1000,DKK,REF-EU-1"
    ].join("\n"));
    writeFileSync(sourceFile, "Invoice\n1000 DKK\n");

    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    const bank = importBankCsv(db, root, csv);
    expect(bank.ok).toBe(true);

    const doc = ingestDocument(db, root, sourceFile, {
      source: "email",
      issueDate: "2026-05-16",
      invoiceNo: "EU-1001",
      deliveryDescription: "EU software service",
      amountIncVat: 1000,
      currency: "DKK",
      sender: { name: "EU Supplier GmbH", address: "Berlin", vatOrCvr: "DE123456789" },
      recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      vatAmount: 0,
      paymentDetails: "Bank transfer"
    });
    expect(doc.ok).toBe(true);

    storeViesValidation(db, {
      vatOrCvr: "DE123456789",
      valid: true,
      validatedAt: "2026-05-15T00:00:00.000Z",
      expiresAt: "2026-08-15T00:00:00.000Z",
      rawResponse: JSON.stringify({ valid: true })
    });

    const bankRow = db.query("SELECT id FROM bank_transactions WHERE reference = 'REF-EU-1'").get() as { id: number };
    const booked = bookExpenseFromBank(db, {
      documentId: doc.documentId!,
      bankTransactionId: bankRow.id,
      expenseAccountNo: "3010"
    });

    expect(booked.ok).toBe(true);
    expect(booked.vatTreatment).toBe("reverse_charge");
    const report = buildBankReconciliationReport(db, "2026-05-01", "2026-05-31");
    expect(report.matchedCount).toBe(1);
    expect(report.unmatchedCount).toBe(0);

    db.close();
    cleanupDir(root);
    cleanupDir(inbox);
  });
});
