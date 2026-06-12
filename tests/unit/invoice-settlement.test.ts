// Tests: src/core/invoice-settlement.ts
import { describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { importBankCsv } from "../../src/core/bank";
import { issueInvoice } from "../../src/core/issued-invoices";
import { getInvoiceStatus } from "../../src/core/invoice-payments";
import { postIssuedInvoiceToLedger } from "../../src/core/invoice-booking";
import { settleInvoiceFromBank } from "../../src/core/invoice-settlement";
import { registerInvoiceReminder, postInvoiceReminderToLedger } from "../../src/core/invoice-reminders";
import { registerInvoiceLateCompensation, postInvoiceLateCompensationToLedger } from "../../src/core/invoice-compensation";
import { registerInvoiceLateInterest, postInvoiceLateInterestToLedger } from "../../src/core/invoice-interest";
import { seedAccounts, verifyAuditChain } from "../../src/core/ledger";

import { cleanupDir } from "../helpers/cleanup";
describe("invoice bank settlement", () => {
  test("settles an issued invoice from an imported bank receipt and clears receivables", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-settle-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    const issued = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-16",
      invoiceNumber: "2026-0001",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
      currency: "DKK"
    });
    expect(issued.ok).toBe(true);
    const salesPost = postIssuedInvoiceToLedger(db, { invoiceDocumentId: issued.documentId! });
    expect(salesPost.ok).toBe(true);

    const csvPath = join(root, "bank.csv");
    writeFileSync(csvPath, "transaction_date,booking_date,text,amount,currency,reference\n2026-05-20,2026-05-20,Customer payment,1250,DKK,INV-0900\n");
    const bank = importBankCsv(db, root, csvPath);
    expect(bank.ok).toBe(true);

    const bankTx = db.query("SELECT id FROM bank_transactions LIMIT 1").get() as { id: number };
    const settled = settleInvoiceFromBank(db, {
      invoiceDocumentId: issued.documentId!,
      bankTransactionId: bankTx.id,
    });
    expect(settled.ok).toBe(true);
    expect(settled.appliedRules).toContain("DK-INVOICE-SETTLEMENT-001");
    expect(settled.openBalance).toBe(0);

    const status = getInvoiceStatus(db, issued.documentId!);
    expect(status.status).toBe("paid");

    const lines = db.query(
      `SELECT a.account_no, jl.debit_amount, jl.credit_amount
       FROM journal_lines jl JOIN accounts a ON a.id = jl.account_id
       WHERE jl.journal_entry_id = ? ORDER BY jl.id ASC`
    ).all(settled.entryId!) as any[];
    expect(lines).toEqual([
      { account_no: "2000", debit_amount: 1250, credit_amount: 0 },
      { account_no: "1100", debit_amount: 0, credit_amount: 1250 },
    ]);

    const chain = verifyAuditChain(db);
    expect(chain.ok).toBe(true);

    db.close();
    cleanupDir(root);
  });

  test("settles a non-DKK issued invoice from an imported FX bank receipt using DKK ledger amounts", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-settle-fx-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    const issued = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-16",
      invoiceNumber: "2026-0001",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde GmbH", address: "Berlin" },
      lines: [{ description: "Consulting", quantity: 1, unitPriceExVat: 100, lineTotalExVat: 100 }],
      totals: { netAmount: 100, vatRate: 0.25, vatAmount: 25, grossAmount: 125, fxRateToDkk: 7.46, netAmountDkk: 746, vatAmountDkk: 186.5, grossAmountDkk: 932.5 },
      currency: "EUR"
    });
    expect(issued.ok).toBe(true);
    expect(postIssuedInvoiceToLedger(db, { invoiceDocumentId: issued.documentId! }).ok).toBe(true);

    const csvPath = join(root, "bank-fx.csv");
    writeFileSync(csvPath, "transaction_date,booking_date,text,amount,currency,amount_dkk,fx_rate_to_dkk,reference\n2026-05-20,2026-05-20,Customer payment,125,EUR,932.5,7.46,INV-0900-EUR\n");
    expect(importBankCsv(db, root, csvPath).ok).toBe(true);

    const bankTx = db.query("SELECT id FROM bank_transactions LIMIT 1").get() as { id: number };
    const settled = settleInvoiceFromBank(db, {
      invoiceDocumentId: issued.documentId!,
      bankTransactionId: bankTx.id,
    });
    expect(settled.ok).toBe(true);
    expect(settled.openBalance).toBe(0);

    const status = getInvoiceStatus(db, issued.documentId!);
    expect(status.status).toBe("paid");

    const entry = db.query("SELECT currency, amount_foreign, amount_dkk, fx_rate_to_dkk FROM journal_entries WHERE id = ?").get(settled.entryId!) as any;
    expect(entry).toEqual({ currency: "EUR", amount_foreign: 125, amount_dkk: 932.5, fx_rate_to_dkk: 7.46 });

    const lines = db.query(
      `SELECT a.account_no, jl.debit_amount, jl.credit_amount
       FROM journal_lines jl JOIN accounts a ON a.id = jl.account_id
       WHERE jl.journal_entry_id = ? ORDER BY jl.id ASC`
    ).all(settled.entryId!) as any[];
    expect(lines).toEqual([
      { account_no: "2000", debit_amount: 932.5, credit_amount: 0 },
      { account_no: "1100", debit_amount: 0, credit_amount: 932.5 },
    ]);

    db.close();
    cleanupDir(root);
  });

  test("blocks combined settlement until included claims are ledger-posted", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-settle-combined-unposted-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    const issued = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-16",
      dueDate: "2026-06-15",
      invoiceNumber: "2026-0001",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9", vatOrCvr: "DK87654321" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
      currency: "DKK"
    });
    expect(issued.ok).toBe(true);
    expect(postIssuedInvoiceToLedger(db, { invoiceDocumentId: issued.documentId! }).ok).toBe(true);
    expect(registerInvoiceReminder(db, { invoiceDocumentId: issued.documentId!, reminderDate: "2026-06-26" }).ok).toBe(true);

    const csvPath = join(root, "bank-combined-unposted.csv");
    writeFileSync(csvPath, "transaction_date,booking_date,text,amount,currency,reference\n2026-06-28,2026-06-28,Customer payment incl claims,1350,DKK,INV-0900B-COMBINED\n");
    expect(importBankCsv(db, root, csvPath).ok).toBe(true);

    const bankTx = db.query("SELECT id FROM bank_transactions WHERE reference = 'INV-0900B-COMBINED'").get() as { id: number };
    const settled = settleInvoiceFromBank(db, {
      invoiceDocumentId: issued.documentId!,
      bankTransactionId: bankTx.id,
    });
    expect(settled.ok).toBe(false);
    expect(settled.errors[0]).toContain("combined settlement requires all included claims to be ledger-posted first");
    expect(db.query("SELECT COUNT(*) AS n FROM invoice_claim_payments").get()).toEqual({ n: 0 });

    db.close();
    cleanupDir(root);
  });

  test("settles principal and booked claim balance from one combined bank receipt", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-settle-combined-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    const issued = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-16",
      dueDate: "2026-06-15",
      invoiceNumber: "2026-0001",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9", vatOrCvr: "DK87654321" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
      currency: "DKK"
    });
    expect(issued.ok).toBe(true);
    expect(postIssuedInvoiceToLedger(db, { invoiceDocumentId: issued.documentId! }).ok).toBe(true);
    expect(registerInvoiceReminder(db, { invoiceDocumentId: issued.documentId!, reminderDate: "2026-06-26" }).ok).toBe(true);
    expect(postInvoiceReminderToLedger(db, { invoiceDocumentId: issued.documentId! }).ok).toBe(true);
    expect(registerInvoiceLateCompensation(db, { invoiceDocumentId: issued.documentId!, asOfDate: "2026-06-20" }).ok).toBe(true);
    expect(postInvoiceLateCompensationToLedger(db, { invoiceDocumentId: issued.documentId! }).ok).toBe(true);
    expect(registerInvoiceLateInterest(db, { invoiceDocumentId: issued.documentId!, asOfDate: "2026-06-20", referenceRatePercent: 2.2 }).ok).toBe(true);
    expect(postInvoiceLateInterestToLedger(db, { invoiceDocumentId: issued.documentId! }).ok).toBe(true);

    const csvPath = join(root, "bank-combined.csv");
    writeFileSync(csvPath, "transaction_date,booking_date,text,amount,currency,reference\n2026-06-28,2026-06-28,Customer payment incl claims,1661.75,DKK,INV-0901-COMBINED\n");
    expect(importBankCsv(db, root, csvPath).ok).toBe(true);

    const bankTx = db.query("SELECT id FROM bank_transactions WHERE reference = 'INV-0901-COMBINED'").get() as { id: number };
    const settled = settleInvoiceFromBank(db, {
      invoiceDocumentId: issued.documentId!,
      bankTransactionId: bankTx.id,
    });
    expect(settled.ok).toBe(true);
    expect(settled.appliedRules).toContain("DK-INVOICE-SETTLEMENT-001");
    expect(settled.appliedRules).toContain("DK-INVOICE-COMBINED-SETTLEMENT-001");
    expect(settled.principalAmount).toBe(1250);
    expect(settled.claimAmount).toBe(411.75);
    expect(settled.openBalance).toBe(0);
    expect(settled.claimOpenBalance).toBe(0);
    expect(settled.claimPaymentId).toBeDefined();

    const status = getInvoiceStatus(db, issued.documentId!);
    expect(status.ok).toBe(true);
    expect(status.openBalance).toBe(0);
    expect(status.claimOpenBalance).toBe(0);
    expect(status.totalClaimPayments).toBe(411.75);

    const lines = db.query(
      `SELECT a.account_no, jl.debit_amount, jl.credit_amount
       FROM journal_lines jl JOIN accounts a ON a.id = jl.account_id
       WHERE jl.journal_entry_id = ? ORDER BY jl.id ASC`
    ).all(settled.entryId!) as any[];
    expect(lines).toEqual([
      { account_no: "2000", debit_amount: 1661.75, credit_amount: 0 },
      { account_no: "1100", debit_amount: 0, credit_amount: 1661.75 },
    ]);

    db.close();
    cleanupDir(root);
  });

  test("requires explicit bank transaction selection for settlement", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-settle-explicit-bank-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    const issued = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-16",
      invoiceNumber: "2026-0001",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
      currency: "DKK"
    });
    expect(issued.ok).toBe(true);
    expect(postIssuedInvoiceToLedger(db, { invoiceDocumentId: issued.documentId! }).ok).toBe(true);

    const csvPath = join(root, "bank.csv");
    writeFileSync(csvPath, "transaction_date,booking_date,text,amount,currency,reference\n2026-05-20,2026-05-20,Wrong payment,1250,DKK,OTHER-INVOICE\n");
    expect(importBankCsv(db, root, csvPath).ok).toBe(true);

    const settled = settleInvoiceFromBank(db, { invoiceDocumentId: issued.documentId! });
    expect(settled.ok).toBe(false);
    expect(settled.errors[0]).toBe("bankTransactionId or bankTransactionReference is required");

    db.close();
    cleanupDir(root);
  });
});
