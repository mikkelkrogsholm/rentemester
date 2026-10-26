// Tests: src/core/expense-booking.ts
import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { seedAccounts, verifyAuditChain } from "../../src/core/ledger";
import { importBankCsv } from "../../src/core/bank";
import { ingestDocument } from "../../src/core/documents";
import { buildBankReconciliationReport } from "../../src/core/reconciliation";
import { bookExpenseFromBank, previewBookExpenseFromBank } from "../../src/core/expense-booking";
import { storeViesValidation } from "../../src/core/vies";
import { buildVatReport, postRepresentationPurchase } from "../../src/core/vat";
import { registerPayable } from "../../src/core/payables";

describe("expense booking", () => {
  // ---------------------------------------------------------------------
  // #514 — non_deductible: VAT-charged bilag at NOT VAT-registered
  // companies (a holding ApS, en frivilligt momsfritaget virksomhed eller en
  // mikrovirksomhed under § 48-tærsklen). The VAT is absorbed into the cost
  // basis under § 37 — no 4000 line, no momsangivelse contribution.
  // ---------------------------------------------------------------------
  function markCompanyNotVatRegistered(db: import("bun:sqlite").Database) {
    // bookExpenseFromBank reads getCompanySettings which falls back to
    // DEFAULT_COMPANY_SETTINGS (vatPeriodType: 'quarter') when no row exists.
    // Insert a row with vat_period_type = NULL so the company is explicitly
    // not VAT-registered for the purpose of this test.
    db.run(
      "INSERT INTO companies (id, name, vat_period_type) VALUES (1, 'TEST HOLDING ApS', NULL) " +
        "ON CONFLICT(id) DO UPDATE SET vat_period_type = NULL",
    );
  }

  test("non_deductible posts gross to expense + payment, no 4000 line, at non-registered company", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-non-deductible-"));
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-non-deductible-inbox-"));
    const csv = join(root, "transactions.csv");
    const sourceFile = join(inbox, "depotgebyr.txt");
    // Danske Bank Depotgebyr — the canonical motivating example: net 42,31 +
    // moms 10,58 = brutto 52,89. The whole brutto-beløb hits 3300 because the
    // momsen ikke kan løftes under § 37.
    writeFileSync(csv, [
      "transaction_date,booking_date,text,amount,currency,reference",
      "2026-05-16,2026-05-16,DANSKE BANK DEPOTGEBYR,-52.89,DKK,REF-NDF-1",
    ].join("\n"));
    writeFileSync(sourceFile, "Depotgebyr\n52,89 DKK\n");

    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);
    markCompanyNotVatRegistered(db);

    const bank = importBankCsv(db, root, csv);
    expect(bank.ok).toBe(true);

    const doc = ingestDocument(db, root, sourceFile, {
      source: "email",
      issueDate: "2026-05-16",
      invoiceNo: "DEP-2026-05",
      deliveryDescription: "Depotgebyr",
      amountIncVat: 52.89,
      currency: "DKK",
      sender: { name: "Danske Bank A/S", address: "Bremerholm 1", vatOrCvr: "DK61126228" },
      recipient: { name: "TEST HOLDING ApS", address: "Holdingvej 1", vatOrCvr: "DK12345678" },
      vatAmount: 10.58,
      paymentDetails: "Træk fra depotkonto",
    });
    expect(doc.ok).toBe(true);

    const bankRow = db
      .query("SELECT id FROM bank_transactions WHERE reference = 'REF-NDF-1'")
      .get() as { id: number };
    const booked = bookExpenseFromBank(db, {
      documentId: doc.documentId!,
      bankTransactionId: bankRow.id,
      expenseAccountNo: "3300",
      vatTreatment: "non_deductible",
    });

    expect({ ok: booked.ok, errors: booked.errors }).toEqual({ ok: true, errors: [] });
    expect(booked.grossAmount).toBe(52.89);
    expect(booked.vatTreatment).toBe("non_deductible");
    // netAmount carries the gross figure too — the whole bilag IS the cost.
    expect(booked.netAmount).toBe(52.89);
    expect(booked.vatAmount).toBe(0);

    const lines = db
      .query(
        `SELECT a.account_no, jl.debit_amount, jl.credit_amount, jl.vat_code
           FROM journal_lines jl
           JOIN accounts a ON a.id = jl.account_id
          WHERE jl.journal_entry_id = ?
          ORDER BY jl.id ASC`,
      )
      .all(booked.entryId!) as Array<{
      account_no: string;
      debit_amount: number;
      credit_amount: number;
      vat_code: string | null;
    }>;
    // Exactly two lines: gross debit on the expense account, gross credit on
    // the payment account. No 4000 Købsmoms line, no vat_code on any line —
    // the booking must never feed a momsangivelse rubrik.
    expect(lines).toEqual([
      { account_no: "3300", debit_amount: 52.89, credit_amount: 0, vat_code: null },
      { account_no: "2000", debit_amount: 0, credit_amount: 52.89, vat_code: null },
    ]);

    db.close();
    rmSync(root, { recursive: true, force: true });
    rmSync(inbox, { recursive: true, force: true });
  });

  test("non_deductible remains available at a VAT-registered company", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-non-deductible-reg-"));
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-non-deductible-reg-inbox-"));
    const csv = join(root, "transactions.csv");
    const sourceFile = join(inbox, "vendor.txt");
    writeFileSync(csv, [
      "transaction_date,booking_date,text,amount,currency,reference",
      "2026-05-16,2026-05-16,SOFTWARE APS,-1250,DKK,REF-NDF-REG-1",
    ].join("\n"));
    writeFileSync(sourceFile, "Invoice\n1250 DKK\n");

    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);
    // No markCompanyNotVatRegistered — the default settings carry the
    // historical 'quarter' cadence so the company IS VAT-registered.

    expect(importBankCsv(db, root, csv).ok).toBe(true);
    const doc = ingestDocument(db, root, sourceFile, {
      source: "email",
      issueDate: "2026-05-16",
      invoiceNo: "V-NDF-REG",
      deliveryDescription: "Softwareabonnement",
      amountIncVat: 1250,
      currency: "DKK",
      sender: { name: "Software ApS", address: "SaaSvej 1", vatOrCvr: "DK11223344" },
      recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      vatAmount: 250,
      paymentDetails: "Bank transfer",
    });
    expect(doc.ok).toBe(true);

    const bankRow = db
      .query("SELECT id FROM bank_transactions WHERE reference = 'REF-NDF-REG-1'")
      .get() as { id: number };
    const booked = bookExpenseFromBank(db, {
      documentId: doc.documentId!,
      bankTransactionId: bankRow.id,
      expenseAccountNo: "3000",
      vatTreatment: "non_deductible",
    });

    expect(booked.ok).toBe(true);
    expect(db.query(
      `SELECT a.account_no, jl.debit_amount, jl.credit_amount, jl.vat_code
         FROM journal_lines jl JOIN accounts a ON a.id = jl.account_id
        WHERE jl.journal_entry_id = ? ORDER BY jl.id`,
    ).all(booked.entryId!)).toEqual([
      { account_no: "3000", debit_amount: 1250, credit_amount: 0, vat_code: null },
      { account_no: "2000", debit_amount: 0, credit_amount: 1250, vat_code: null },
    ]);

    db.close();
    rmSync(root, { recursive: true, force: true });
    rmSync(inbox, { recursive: true, force: true });
  });

  test("an identity-less cash receipt cannot claim Danish input VAT", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-cash-receipt-vat-evidence-"));
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-cash-receipt-vat-evidence-inbox-"));
    const csv = join(root, "transactions.csv");
    const sourceFile = join(inbox, "receipt.txt");
    writeFileSync(csv, [
      "transaction_date,booking_date,text,amount,currency,reference",
      "2026-05-16,2026-05-16,UNKNOWN RECEIPT,-1250,DKK,REF-CASH-VAT-1",
    ].join("\n"));
    writeFileSync(sourceFile, "Receipt\n1250 DKK\n");
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);
    expect(importBankCsv(db, root, csv).ok).toBe(true);
    const doc = ingestDocument(db, root, sourceFile, {
      source: "photo-upload",
      documentType: "cash_register_receipt",
      amountIncVat: 1250,
      vatAmount: 250,
      currency: "DKK",
    });
    expect(doc.ok).toBe(true);
    const bank = db.query("SELECT id FROM bank_transactions WHERE reference = 'REF-CASH-VAT-1'").get() as { id: number };
    const standard = bookExpenseFromBank(db, { documentId: doc.documentId!, bankTransactionId: bank.id, expenseAccountNo: "3000", vatTreatment: "standard" });
    expect(standard.ok).toBe(false);
    expect(standard.errors.join(" ")).toContain("resolved Danish supplier identity");
    const payable = registerPayable(db, { documentId: doc.documentId!, billDate: "2026-05-16", dueDate: "2026-06-16", expenseAccountNo: "3000", vatTreatment: "standard" });
    expect(payable.ok).toBe(false);
    expect(payable.errors.join(" ")).toContain("resolved Danish supplier identity");
    expect(db.query("SELECT COUNT(*) AS n FROM journal_entries").get()).toEqual({ n: 0 });
    const gross = bookExpenseFromBank(db, { documentId: doc.documentId!, bankTransactionId: bank.id, expenseAccountNo: "3000", vatTreatment: "non_deductible" });
    expect(gross.ok).toBe(true);
    db.close();
    rmSync(root, { recursive: true, force: true });
    rmSync(inbox, { recursive: true, force: true });
  });

  test("non_deductible accepts vat_amount = 0 too (superset of exempt at non-registered company)", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-non-deductible-vat0-"));
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-non-deductible-vat0-inbox-"));
    const csv = join(root, "transactions.csv");
    const sourceFile = join(inbox, "vendor.txt");
    writeFileSync(csv, [
      "transaction_date,booking_date,text,amount,currency,reference",
      "2026-05-16,2026-05-16,EXEMPT SUPPLIER,-500,DKK,REF-NDF-V0-1",
    ].join("\n"));
    writeFileSync(sourceFile, "Invoice\n500 DKK\n");

    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);
    markCompanyNotVatRegistered(db);

    expect(importBankCsv(db, root, csv).ok).toBe(true);
    const doc = ingestDocument(db, root, sourceFile, {
      source: "email",
      issueDate: "2026-05-16",
      invoiceNo: "EX-1001",
      deliveryDescription: "Momsfritaget ydelse",
      amountIncVat: 500,
      currency: "DKK",
      sender: { name: "Exempt Supplier", address: "Et sted", vatOrCvr: "DK22334455" },
      recipient: { name: "TEST HOLDING ApS", address: "Holdingvej 1", vatOrCvr: "DK12345678" },
      vatAmount: 0,
      paymentDetails: "Bank transfer",
    });
    expect(doc.ok).toBe(true);

    const bankRow = db
      .query("SELECT id FROM bank_transactions WHERE reference = 'REF-NDF-V0-1'")
      .get() as { id: number };
    const booked = bookExpenseFromBank(db, {
      documentId: doc.documentId!,
      bankTransactionId: bankRow.id,
      expenseAccountNo: "3300",
      vatTreatment: "non_deductible",
    });

    expect({ ok: booked.ok, errors: booked.errors }).toEqual({ ok: true, errors: [] });
    expect(booked.vatTreatment).toBe("non_deductible");
    // Two-line booking unchanged when vat_amount is 0 — the shape is identical
    // to a vat-charged bilag, just with the gross == net coincidence.
    const lines = db
      .query(
        `SELECT a.account_no, jl.debit_amount, jl.credit_amount
           FROM journal_lines jl
           JOIN accounts a ON a.id = jl.account_id
          WHERE jl.journal_entry_id = ?
          ORDER BY jl.id ASC`,
      )
      .all(booked.entryId!) as Array<{
      account_no: string;
      debit_amount: number;
      credit_amount: number;
    }>;
    expect(lines).toEqual([
      { account_no: "3300", debit_amount: 500, credit_amount: 0 },
      { account_no: "2000", debit_amount: 0, credit_amount: 500 },
    ]);

    db.close();
    rmSync(root, { recursive: true, force: true });
    rmSync(inbox, { recursive: true, force: true });
  });

  test("inference picks non_deductible for DK_PURCHASE_25 account when company is not VAT-registered", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-non-deductible-infer-"));
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-non-deductible-infer-inbox-"));
    const csv = join(root, "transactions.csv");
    const sourceFile = join(inbox, "vendor.txt");
    writeFileSync(csv, [
      "transaction_date,booking_date,text,amount,currency,reference",
      "2026-05-16,2026-05-16,SOFTWARE APS,-1250,DKK,REF-NDF-INF-1",
    ].join("\n"));
    writeFileSync(sourceFile, "Invoice\n1250 DKK\n");

    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);
    markCompanyNotVatRegistered(db);

    expect(importBankCsv(db, root, csv).ok).toBe(true);
    const doc = ingestDocument(db, root, sourceFile, {
      source: "email",
      issueDate: "2026-05-16",
      invoiceNo: "V-NDF-INF",
      deliveryDescription: "Softwareabonnement",
      amountIncVat: 1250,
      currency: "DKK",
      sender: { name: "Software ApS", address: "SaaSvej 1", vatOrCvr: "DK11223344" },
      recipient: { name: "TEST HOLDING ApS", address: "Holdingvej 1", vatOrCvr: "DK12345678" },
      vatAmount: 250,
      paymentDetails: "Bank transfer",
    });
    expect(doc.ok).toBe(true);

    const bankRow = db
      .query("SELECT id FROM bank_transactions WHERE reference = 'REF-NDF-INF-1'")
      .get() as { id: number };
    // 3000 (Software og SaaS) has DK_PURCHASE_25 as default_vat_code. With no
    // explicit --vat-treatment, the inference must pick non_deductible
    // for a not-VAT-registered company — never `standard`, which would park
    // the moms on 4000 Købsmoms where it can never be deducted.
    const booked = bookExpenseFromBank(db, {
      documentId: doc.documentId!,
      bankTransactionId: bankRow.id,
      expenseAccountNo: "3000",
    });

    expect({ ok: booked.ok, errors: booked.errors }).toEqual({ ok: true, errors: [] });
    expect(booked.vatTreatment).toBe("non_deductible");

    db.close();
    rmSync(root, { recursive: true, force: true });
    rmSync(inbox, { recursive: true, force: true });
  });

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
    rmSync(root, { recursive: true, force: true });
    rmSync(inbox, { recursive: true, force: true });
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
    rmSync(root, { recursive: true, force: true });
    rmSync(inbox, { recursive: true, force: true });
  });

  test("books a foreign-currency purchase settled by a DKK bank transaction and preserves FX basis", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-expense-book-fx-"));
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-expense-book-fx-inbox-"));
    const csv = join(root, "transactions.csv");
    const sourceFile = join(inbox, "vendor.txt");
    writeFileSync(csv, [
      "transaction_date,booking_date,text,amount,currency,reference",
      "2026-05-16,2026-05-16,CLOUD VENDOR,-746,DKK,REF-FX-1"
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
    const auditCountBeforePreview = (db.query("SELECT COUNT(*) AS count FROM audit_log").get() as { count: number }).count;
    const preview = previewBookExpenseFromBank(db, {
      documentId: doc.documentId!,
      bankTransactionId: bankRow.id,
      expenseAccountNo: "3000",
      vatTreatment: "exempt"
    });
    const repeatedPreview = previewBookExpenseFromBank(db, {
      documentId: doc.documentId!,
      bankTransactionId: bankRow.id,
      expenseAccountNo: "3000",
      vatTreatment: "exempt"
    });

    expect(preview).toMatchObject({ ok: true, entryNo: "2026-00001", grossAmountDkk: 746, fxRateToDkk: 7.46, fxRateSource: "derived_dkk_settlement", fxReconstructionDifferenceDkk: 0 });
    expect(repeatedPreview).toEqual(preview);
    expect(db.query("SELECT COUNT(*) AS count FROM journal_entries").get()).toEqual({ count: 0 });
    expect(db.query("SELECT COUNT(*) AS count FROM audit_log").get()).toEqual({ count: auditCountBeforePreview });
    expect(db.query("SELECT fx_rate_to_dkk FROM bank_transactions WHERE id = ?").get(bankRow.id)).toEqual({ fx_rate_to_dkk: null });

    db.query("UPDATE bank_transactions SET fx_rate_to_dkk = 0 WHERE id = ?").run(bankRow.id);
    const invalidImportedRate = previewBookExpenseFromBank(db, {
      documentId: doc.documentId!,
      bankTransactionId: bankRow.id,
      expenseAccountNo: "3000",
      vatTreatment: "exempt"
    });
    expect(invalidImportedRate).toMatchObject({ ok: false, errors: [`bank transaction ${bankRow.id} fx_rate_to_dkk must be positive when provided`] });
    db.query("UPDATE bank_transactions SET fx_rate_to_dkk = NULL WHERE id = ?").run(bankRow.id);

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
    expect(booked).toMatchObject({ grossAmountForeign: 100, grossAmountDkk: 746, netAmountDkk: 746, vatAmountDkk: 0, fxRateToDkk: 7.46, fxRateSource: "derived_dkk_settlement", fxReconstructionDifferenceDkk: 0 });

    const entry = db.query("SELECT currency, amount_foreign, amount_dkk, fx_rate_to_dkk FROM journal_entries WHERE id = ?").get(booked.entryId!) as any;
    expect(entry).toEqual({ currency: "EUR", amount_foreign: 100, amount_dkk: 746, fx_rate_to_dkk: 7.46 });
    expect(db.query("SELECT COUNT(*) AS count FROM audit_log WHERE entity_type = 'journal_entry'").get()).toEqual({ count: 1 });
    db.close();
    const reopened = openDb(ensureCompanyDirs(root).db);
    expect(verifyAuditChain(reopened).ok).toBe(true);
    expect(reopened.query("SELECT currency, amount_foreign, amount_dkk, fx_rate_to_dkk FROM journal_entries WHERE id = ?").get(booked.entryId!)).toEqual({ currency: "EUR", amount_foreign: 100, amount_dkk: 746, fx_rate_to_dkk: 7.46 });
    expect(bookExpenseFromBank(reopened, { documentId: doc.documentId!, bankTransactionId: bankRow.id, expenseAccountNo: "3000", vatTreatment: "exempt" }).ok).toBe(false);
    expect(reopened.query("SELECT COUNT(*) AS count FROM journal_entries").get()).toEqual({ count: 1 });
    reopened.close();
    rmSync(root, { recursive: true, force: true });
    rmSync(inbox, { recursive: true, force: true });
  });

  test("books a signed foreign-currency bank row using the absolute DKK valuation", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-expense-book-fx-signed-"));
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-expense-book-fx-signed-inbox-"));
    const csv = join(root, "transactions.csv");
    const sourceFile = join(inbox, "vendor.txt");
    writeFileSync(csv, [
      "transaction_date,booking_date,text,amount,currency,amount_dkk,fx_rate_to_dkk,reference",
      "2026-05-16,2026-05-16,CLOUD VENDOR,-100,EUR,-746,7.46,REF-FX-SIGNED-1",
    ].join("\n"));
    writeFileSync(sourceFile, "Invoice\n100 EUR\n");
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);
    expect(importBankCsv(db, root, csv).ok).toBe(true);
    const doc = ingestDocument(db, root, sourceFile, {
      source: "email",
      issueDate: "2026-05-16",
      invoiceNo: "V-FX-SIGNED-1",
      deliveryDescription: "Cloud service",
      amountIncVat: 100,
      currency: "EUR",
      sender: { name: "Cloud Vendor", address: "Berlin", vatOrCvr: "DE123456789" },
      recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      vatAmount: 0,
    });
    expect(doc.ok).toBe(true);
    const bankRow = db.query("SELECT id, amount_dkk FROM bank_transactions WHERE reference = 'REF-FX-SIGNED-1'").get() as { id: number; amount_dkk: number };
    expect(bankRow.amount_dkk).toBe(-746);
    const booked = bookExpenseFromBank(db, { documentId: doc.documentId!, bankTransactionId: bankRow.id, expenseAccountNo: "3000", vatTreatment: "exempt" });
    expect(booked.ok).toBe(true);
    expect(booked).toMatchObject({ grossAmountForeign: 100, grossAmountDkk: 746, netAmountDkk: 746, vatAmountDkk: 0, fxRateToDkk: 7.46, fxRateSource: "imported_bank", fxReconstructionDifferenceDkk: 0 });
    expect(db.query("SELECT amount_foreign, amount_dkk, fx_rate_to_dkk FROM journal_entries WHERE id = ?").get(booked.entryId!)).toEqual({ amount_foreign: 100, amount_dkk: 746, fx_rate_to_dkk: 7.46 });
    db.close();
    rmSync(root, { recursive: true, force: true });
    rmSync(inbox, { recursive: true, force: true });
  });

  test("fails closed when a DKK settlement cannot be reconstructed at øre precision from a six-decimal derived FX rate", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-expense-book-fx-missing-"));
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-expense-book-fx-missing-inbox-"));
    const csv = join(root, "transactions.csv");
    const sourceFile = join(inbox, "vendor.txt");
    writeFileSync(csv, [
      "transaction_date,booking_date,text,amount,currency,reference",
      "2026-05-16,2026-05-16,CLOUD VENDOR,-7460000.01,DKK,REF-FX-MISSING"
    ].join("\n"));
    writeFileSync(sourceFile, "Invoice\n1000000 EUR\n");

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
      amountIncVat: 1000000,
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
    expect(booked.errors.join(" ")).toContain("derived fx_rate_to_dkk 7.46 cannot reconstruct DKK settlement 7460000.01");

    db.close();
    rmSync(root, { recursive: true, force: true });
    rmSync(inbox, { recursive: true, force: true });
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
    rmSync(root, { recursive: true, force: true });
    rmSync(inbox, { recursive: true, force: true });
  });

  test("uses reverse-charge flow for an EU service with no Danish invoice VAT and a derived DKK-settlement rate", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-expense-book-rc-"));
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-expense-book-rc-inbox-"));
    const csv = join(root, "transactions.csv");
    const sourceFile = join(inbox, "vendor.txt");
    writeFileSync(csv, [
      "transaction_date,booking_date,text,amount,currency,reference",
      "2026-05-16,2026-05-16,EU SUPPLIER,-746,DKK,REF-EU-1"
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
      invoiceNo: "EU-1001",
      deliveryDescription: "EU software service",
      amountIncVat: 100,
      currency: "EUR",
      sender: { name: "EU Supplier GmbH", address: "Berlin", vatOrCvr: "DE123456789" },
      recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      vatAmount: 0,
      paymentDetails: "Bank transfer"
    });
    expect(doc.ok).toBe(true);

    storeViesValidation(db, {
      vatOrCvr: "DE123456789",
      valid: true,
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
    expect(booked).toMatchObject({ grossAmountDkk: 746, fxRateToDkk: 7.46, fxRateSource: "derived_dkk_settlement", fxReconstructionDifferenceDkk: 0 });
    const report = buildBankReconciliationReport(db, "2026-05-01", "2026-05-31");
    expect(report.matchedCount).toBe(1);
    expect(report.unmatchedCount).toBe(0);

    db.close();
    rmSync(root, { recursive: true, force: true });
    rmSync(inbox, { recursive: true, force: true });
  });

  test("#529 ingests a US service without an EU ID but requires complete invoice evidence before VAT deduction", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-expense-book-us-rc-"));
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-expense-book-us-rc-inbox-"));
    const csv = join(root, "transactions.csv");
    const sourceFile = join(inbox, "us-saas-no-registration.txt");
    const mismatchedSourceFile = join(inbox, "us-saas-wrong-buyer.txt");
    const evidencedSourceFile = join(inbox, "us-saas-evidenced.txt");
    writeFileSync(csv, [
      "transaction_date,booking_date,text,amount,currency,reference",
      "2026-05-16,2026-05-16,US SAAS,-1000,DKK,REF-US-RC-1",
      "2026-05-17,2026-05-17,US SAAS WRONG BUYER,-1000,DKK,REF-US-RC-MISMATCH",
      "2026-05-17,2026-05-17,US SAAS EVIDENCED,-1000,DKK,REF-US-RC-2",
    ].join("\n"));
    writeFileSync(sourceFile, "US SaaS invoice\n1000 DKK\n");
    writeFileSync(mismatchedSourceFile, "US SaaS invoice addressed to another buyer\n1000 DKK\nReverse charge\n");
    writeFileSync(evidencedSourceFile, "US SaaS evidenced invoice\n1000 DKK\nReverse charge\n");
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    db.run("INSERT INTO companies (id, name, cvr, vat_period_type) VALUES (1, 'Rentemester ApS', 'DK12345678', 'quarter')");
    seedAccounts(db);
    expect(importBankCsv(db, root, csv).ok).toBe(true);
    const doc = ingestDocument(db, root, sourceFile, {
      source: "email",
      issueDate: "2026-05-16",
      invoiceNo: "US-RC-529",
      deliveryDescription: "US SaaS subscription",
      amountIncVat: 1000,
      currency: "DKK",
      sender: { name: "US SaaS Inc.", address: "New York", countryCode: "US", identifierKind: "non_eu" },
      recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      vatAmount: 0,
    });
    expect(doc.ok).toBe(true);
    const bankRow = db.query("SELECT id FROM bank_transactions WHERE reference = 'REF-US-RC-1'").get() as { id: number };
    const refused = bookExpenseFromBank(db, {
      documentId: doc.documentId!,
      bankTransactionId: bankRow.id,
      expenseAccountNo: "3010",
    });
    expect(refused.ok).toBe(false);
    expect(refused.errors.join(" ")).toContain("supplier's home-country registration number");
    expect(refused.errors.join(" ")).toContain("confirmed reverse-charge wording");
    expect(db.query("SELECT COUNT(*) AS n FROM journal_entries").get()).toEqual({ n: 0 });

    const mismatchedDoc = ingestDocument(db, root, mismatchedSourceFile, {
      source: "email",
      issueDate: "2026-05-17",
      invoiceNo: "US-RC-529-WRONG-BUYER",
      deliveryDescription: "US SaaS subscription",
      amountIncVat: 1000,
      currency: "DKK",
      sender: { name: "US SaaS Inc.", address: "New York", vatOrCvr: "US-EIN-12-3456789", countryCode: "US", identifierKind: "non_eu" },
      recipient: { name: "Another ApS", address: "Anden vej 1", vatOrCvr: "DK87654321" },
      vatAmount: 0,
      reverseChargeWordingEvidence: { excerpt: "Reverse charge", location: "page 1" },
    });
    expect(mismatchedDoc.ok).toBe(true);
    const mismatchedBank = db.query("SELECT id FROM bank_transactions WHERE reference = 'REF-US-RC-MISMATCH'").get() as { id: number };
    const mismatchedBooking = bookExpenseFromBank(db, { documentId: mismatchedDoc.documentId!, bankTransactionId: mismatchedBank.id, expenseAccountNo: "3010" });
    expect(mismatchedBooking.ok).toBe(false);
    expect(mismatchedBooking.errors.join(" ")).toContain("does not match the ledger company's configured VAT identifier");
    expect(db.query("SELECT COUNT(*) AS n FROM journal_entries").get()).toEqual({ n: 0 });

    const evidencedDoc = ingestDocument(db, root, evidencedSourceFile, {
      source: "email",
      issueDate: "2026-05-17",
      invoiceNo: "US-RC-529-EVIDENCED",
      deliveryDescription: "US SaaS subscription",
      amountIncVat: 1000,
      currency: "DKK",
      sender: { name: "US SaaS Inc.", address: "New York", vatOrCvr: "US-EIN-12-3456789", countryCode: "US", identifierKind: "non_eu" },
      recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      vatAmount: 0,
      reverseChargeWordingEvidence: { excerpt: "Reverse charge", location: "page 1" },
    });
    expect(evidencedDoc.ok).toBe(true);
    const evidencedBankRow = db.query("SELECT id FROM bank_transactions WHERE reference = 'REF-US-RC-2'").get() as { id: number };
    const booked = bookExpenseFromBank(db, {
      documentId: evidencedDoc.documentId!,
      bankTransactionId: evidencedBankRow.id,
      expenseAccountNo: "3010",
    });
    expect({ ok: booked.ok, errors: booked.errors, vatTreatment: booked.vatTreatment }).toEqual({ ok: true, errors: [], vatTreatment: "reverse_charge" });
    const lines = db.query(
      `SELECT a.account_no, jl.debit_amount, jl.credit_amount, jl.vat_code
         FROM journal_lines jl JOIN accounts a ON a.id = jl.account_id
        WHERE jl.journal_entry_id = ? ORDER BY jl.id`,
    ).all(booked.entryId!) as any[];
    expect(lines[0]).toMatchObject({ account_no: "3010", debit_amount: 1000, vat_code: "NON_EU_SERVICE_REVERSE_CHARGE" });
    expect(lines[1]).toMatchObject({ account_no: "4000", debit_amount: 250 });
    expect(lines[3]).toMatchObject({ account_no: "1200", credit_amount: 250 });
    const report = buildVatReport(db, "2026-05-01", "2026-05-31");
    expect(report).toMatchObject({ ok: true, outputVat: 250, inputVat: 250, reverseChargePurchaseBase: 0, nonEuServiceReverseChargePurchaseBase: 1000, reverseChargePurchaseOutputVat: 250 });
    expect(report.rubrikker.rubrikAVarer).toBe(0);
    expect(report.rubrikker.rubrikAYdelser).toBe(0);
    expect(report.rubrikker.momsAfYdelseskobUdland).toBe(250);
    db.close();
    rmSync(root, { recursive: true, force: true });
    rmSync(inbox, { recursive: true, force: true });
  });

  test("#529 foreign local tax cannot be claimed as Danish input VAT through bank, payable, or representation", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-foreign-tax-gate-"));
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-foreign-tax-gate-inbox-"));
    const csv = join(root, "transactions.csv");
    const sourceFile = join(inbox, "foreign-tax.txt");
    writeFileSync(csv, [
      "transaction_date,booking_date,text,amount,currency,reference",
      "2026-05-18,2026-05-18,US PURCHASE WITH LOCAL TAX,-1250,DKK,REF-US-TAX-1",
    ].join("\n"));
    writeFileSync(sourceFile, "US purchase with local tax\n1250 DKK\n");
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);
    expect(importBankCsv(db, root, csv).ok).toBe(true);
    const doc = ingestDocument(db, root, sourceFile, {
      source: "email",
      issueDate: "2026-05-18",
      invoiceNo: "US-LOCAL-TAX-529",
      deliveryDescription: "Foreign purchase with local sales tax",
      amountIncVat: 1250,
      currency: "DKK",
      sender: { name: "US Vendor Inc.", address: "New York", vatOrCvr: "US-EIN-12-3456789", countryCode: "US", identifierKind: "non_eu" },
      recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      vatAmount: 250,
    });
    expect(doc.ok).toBe(true);
    const bank = db.query("SELECT id FROM bank_transactions WHERE reference = 'REF-US-TAX-1'").get() as { id: number };

    const standard = bookExpenseFromBank(db, { documentId: doc.documentId!, bankTransactionId: bank.id, expenseAccountNo: "3000", vatTreatment: "standard" });
    expect(standard.ok).toBe(false);
    expect(standard.errors.join(" ")).toContain("non_eu/US");
    const representation = bookExpenseFromBank(db, { documentId: doc.documentId!, bankTransactionId: bank.id, expenseAccountNo: "3070", vatTreatment: "representation" });
    expect(representation.ok).toBe(false);
    expect(representation.errors.join(" ")).toContain("non_eu/US");
    const directRepresentation = postRepresentationPurchase(db, { transactionDate: "2026-05-18", text: "Foreign representation", documentId: doc.documentId!, netAmount: 1000 });
    expect(directRepresentation.ok).toBe(false);
    expect(directRepresentation.errors.join(" ")).toContain("non_eu/US");
    const payable = registerPayable(db, { documentId: doc.documentId!, billDate: "2026-05-18", dueDate: "2026-06-18", expenseAccountNo: "3000", vatTreatment: "standard" });
    expect(payable.ok).toBe(false);
    expect(payable.errors.join(" ")).toContain("non_eu/US");
    expect(db.query("SELECT COUNT(*) AS n FROM journal_entries").get()).toEqual({ n: 0 });

    const grossCost = bookExpenseFromBank(db, { documentId: doc.documentId!, bankTransactionId: bank.id, expenseAccountNo: "3000", vatTreatment: "non_deductible" });
    expect(grossCost.ok).toBe(true);
    expect(db.query(
      `SELECT a.account_no, jl.debit_amount, jl.credit_amount, jl.vat_code
         FROM journal_lines jl JOIN accounts a ON a.id = jl.account_id
        WHERE jl.journal_entry_id = ? ORDER BY jl.id`,
    ).all(grossCost.entryId!)).toEqual([
      { account_no: "3000", debit_amount: 1250, credit_amount: 0, vat_code: null },
      { account_no: "2000", debit_amount: 0, credit_amount: 1250, vat_code: null },
    ]);

    db.close();
    rmSync(root, { recursive: true, force: true });
    rmSync(inbox, { recursive: true, force: true });
  });

  // §37 + §50b: how a NOT VAT-registered company's representation / EU-service
  // bilag are handled. Representation is absorbed into the cost (non_deductible
  // — the § 42 partial deduction is a registered-business relief). EU-service
  // reverse charge is REFUSED: for a non-registered company it triggers a
  // separate § 50 b erhvervelsesmoms registration that is out of scope, so we
  // must not silently book a forbidden 4000 deduction nor hide owed VAT.
  function bootstrapNonRegistered(
    label: string,
    senderVat: string,
    vatAmount: number,
  ) {
    const root = mkdtempSync(join(tmpdir(), `rentemester-${label}-`));
    const inbox = mkdtempSync(join(tmpdir(), `rentemester-${label}-inbox-`));
    const csv = join(root, "transactions.csv");
    const sourceFile = join(inbox, "bilag.txt");
    writeFileSync(
      csv,
      [
        "transaction_date,booking_date,text,amount,currency,reference",
        `2026-05-16,2026-05-16,LEVERANDOER,-1250,DKK,REF-${label}`,
      ].join("\n"),
    );
    writeFileSync(sourceFile, "Bilag\n1250 DKK\n");
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);
    markCompanyNotVatRegistered(db);
    expect(importBankCsv(db, root, csv).ok).toBe(true);
    const doc = ingestDocument(db, root, sourceFile, {
      source: "email",
      issueDate: "2026-05-16",
      invoiceNo: `INV-${label}`,
      deliveryDescription: "Ydelse",
      amountIncVat: 1250,
      currency: "DKK",
      sender: { name: "Leverandør", address: "Vej 1", vatOrCvr: senderVat },
      recipient: { name: "TEST HOLDING ApS", address: "Holdingvej 1", vatOrCvr: "DK12345678" },
      vatAmount,
      paymentDetails: "Bankoverførsel",
    });
    expect(doc.ok).toBe(true);
    const bankRow = db
      .query("SELECT id FROM bank_transactions WHERE reference = ?")
      .get(`REF-${label}`) as { id: number };
    return { db, root, inbox, documentId: doc.documentId!, bankTransactionId: bankRow.id };
  }

  function lineRows(db: import("bun:sqlite").Database, entryId: number) {
    return db
      .query(
        `SELECT a.account_no, jl.debit_amount, jl.credit_amount, jl.vat_code
           FROM journal_lines jl JOIN accounts a ON a.id = jl.account_id
          WHERE jl.journal_entry_id = ? ORDER BY jl.id ASC`,
      )
      .all(entryId);
  }

  test("representation bilag at a non-registered company is absorbed as non_deductible (no partial deduction, no 4000 line)", () => {
    // A REPRESENTATION_SPECIAL account (3070) with vat on the bilag.
    const { db, root, inbox, documentId, bankTransactionId } = bootstrapNonRegistered(
      "repr-nonreg",
      "DK99887766",
      250,
    );
    const booked = bookExpenseFromBank(db, { documentId, bankTransactionId, expenseAccountNo: "3070" });
    expect({ ok: booked.ok, errors: booked.errors }).toEqual({ ok: true, errors: [] });
    expect(booked.vatTreatment).toBe("non_deductible");
    expect(lineRows(db, booked.entryId!)).toEqual([
      { account_no: "3070", debit_amount: 1250, credit_amount: 0, vat_code: null },
      { account_no: "2000", debit_amount: 0, credit_amount: 1250, vat_code: null },
    ]);
    db.close();
    rmSync(root, { recursive: true, force: true });
    rmSync(inbox, { recursive: true, force: true });
  });

  test("explicit vatTreatment=representation is refused at a non-registered company (points at non_deductible)", () => {
    const { db, root, inbox, documentId, bankTransactionId } = bootstrapNonRegistered(
      "repr-explicit-nonreg",
      "DK99887766",
      250,
    );
    const booked = bookExpenseFromBank(db, {
      documentId,
      bankTransactionId,
      expenseAccountNo: "3070",
      vatTreatment: "representation",
    });
    expect(booked.ok).toBe(false);
    expect(booked.errors.join(" ")).toContain("ikke momsregistreret");
    expect(booked.errors.join(" ")).toContain("non_deductible");
    db.close();
    rmSync(root, { recursive: true, force: true });
    rmSync(inbox, { recursive: true, force: true });
  });

  test("EU-service reverse_charge is refused at a non-registered company (§ 50 b erhvervelsesmoms out of scope)", () => {
    // EU-service invoice carries no Danish VAT (vat_amount = 0); the 3010
    // account's EU_SERVICE_REVERSE_CHARGE default infers reverse_charge.
    const { db, root, inbox, documentId, bankTransactionId } = bootstrapNonRegistered(
      "eu-nonreg",
      "DE811234567",
      0,
    );
    const booked = bookExpenseFromBank(db, { documentId, bankTransactionId, expenseAccountNo: "3010" });
    expect(booked.ok).toBe(false);
    expect(booked.errors.join(" ")).toContain("ikke momsregistreret");
    expect(booked.errors.join(" ")).toContain("§ 50 b");
    // Nothing was booked — no journal entry links the bank transaction.
    const linked = db
      .query("SELECT COUNT(*) AS n FROM journal_entries WHERE source_bank_transaction_id = ?")
      .get(bankTransactionId) as { n: number };
    expect(linked.n).toBe(0);
    db.close();
    rmSync(root, { recursive: true, force: true });
    rmSync(inbox, { recursive: true, force: true });
  });
});
