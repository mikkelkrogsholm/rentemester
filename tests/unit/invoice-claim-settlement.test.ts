// Tests: src/core/invoice-claim-settlement.ts
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
import { settleInvoiceClaimsFromBank } from "../../src/core/invoice-claim-settlement";
import { registerInvoiceReminder, postInvoiceReminderToLedger } from "../../src/core/invoice-reminders";
import { registerInvoiceLateCompensation, postInvoiceLateCompensationToLedger } from "../../src/core/invoice-compensation";
import { registerInvoiceLateInterest, postInvoiceLateInterestToLedger } from "../../src/core/invoice-interest";
import { seedAccounts, verifyAuditChain } from "../../src/core/ledger";

import { cleanupDir } from "../helpers/cleanup";
describe("invoice claim settlement", () => {
  test("settles booked claim receivables from an imported bank receipt after principal is cleared", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-claim-settle-"));
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

    const principalCsv = join(root, "bank-principal.csv");
    writeFileSync(principalCsv, "transaction_date,booking_date,text,amount,currency,reference\n2026-05-20,2026-05-20,Customer payment,1250,DKK,INV-0990\n");
    expect(importBankCsv(db, root, principalCsv).ok).toBe(true);
    const principalTx = db.query("SELECT id FROM bank_transactions WHERE reference = 'INV-0990'").get() as { id: number };
    expect(settleInvoiceFromBank(db, { invoiceDocumentId: issued.documentId!, bankTransactionId: principalTx.id }).ok).toBe(true);

    const claimCsv = join(root, "bank-claim.csv");
    writeFileSync(claimCsv, "transaction_date,booking_date,text,amount,currency,reference\n2026-06-28,2026-06-28,Claim payment,411.75,DKK,INV-0990-CLAIM\n");
    expect(importBankCsv(db, root, claimCsv).ok).toBe(true);
    const claimTx = db.query("SELECT id FROM bank_transactions WHERE reference = 'INV-0990-CLAIM'").get() as { id: number };

    const settled = settleInvoiceClaimsFromBank(db, {
      invoiceDocumentId: issued.documentId!,
      bankTransactionId: claimTx.id,
    });
    expect(settled.ok).toBe(true);
    expect(settled.appliedRules).toContain("DK-INVOICE-CLAIM-SETTLEMENT-001");
    expect(settled.remainingClaimOpenBalance).toBe(0);

    const status = getInvoiceStatus(db, issued.documentId!);
    expect(status.ok).toBe(true);
    expect(status.openBalance).toBe(0);
    expect(status.claimOpenBalance).toBe(0);
    expect(status.totalClaimPayments).toBe(411.75);
    expect(status.claimPayments).toHaveLength(1);
    expect(status.claimPayments?.[0]?.amount).toBe(411.75);

    const lines = db.query(
      `SELECT a.account_no, jl.debit_amount, jl.credit_amount
       FROM journal_lines jl JOIN accounts a ON a.id = jl.account_id
       WHERE jl.journal_entry_id = ? ORDER BY jl.id ASC`
    ).all(settled.entryId!) as any[];
    expect(lines).toEqual([
      { account_no: "2000", debit_amount: 411.75, credit_amount: 0 },
      { account_no: "1100", debit_amount: 0, credit_amount: 411.75 },
    ]);

    const chain = verifyAuditChain(db);
    expect(chain.ok).toBe(true);

    db.close();
    cleanupDir(root);
  });

  test("requires explicit bank transaction selection for claim settlement", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-claim-settle-explicit-bank-"));
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

    const principalCsv = join(root, "bank-principal.csv");
    writeFileSync(principalCsv, "transaction_date,booking_date,text,amount,currency,reference\n2026-05-20,2026-05-20,Customer payment,1250,DKK,INV-0990B\n");
    expect(importBankCsv(db, root, principalCsv).ok).toBe(true);
    const principalTx = db.query("SELECT id FROM bank_transactions WHERE reference = 'INV-0990B'").get() as { id: number };
    expect(settleInvoiceFromBank(db, { invoiceDocumentId: issued.documentId!, bankTransactionId: principalTx.id }).ok).toBe(true);

    const strayClaimCsv = join(root, "bank-claim.csv");
    writeFileSync(strayClaimCsv, "transaction_date,booking_date,text,amount,currency,reference\n2026-06-28,2026-06-28,Unrelated claim payment,100,DKK,OTHER-CLAIM\n");
    expect(importBankCsv(db, root, strayClaimCsv).ok).toBe(true);

    const settled = settleInvoiceClaimsFromBank(db, { invoiceDocumentId: issued.documentId! });
    expect(settled.ok).toBe(false);
    expect(settled.errors[0]).toBe("bankTransactionId or bankTransactionReference is required");

    db.close();
    cleanupDir(root);
  });

  test("blocks claim settlement before principal is cleared", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-claim-settle-blocked-"));
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

    const claimCsv = join(root, "bank-claim.csv");
    writeFileSync(claimCsv, "transaction_date,booking_date,text,amount,currency,reference\n2026-06-28,2026-06-28,Claim payment,100,DKK,INV-0991-CLAIM\n");
    expect(importBankCsv(db, root, claimCsv).ok).toBe(true);
    const claimTx = db.query("SELECT id FROM bank_transactions WHERE reference = 'INV-0991-CLAIM'").get() as { id: number };

    const settled = settleInvoiceClaimsFromBank(db, {
      invoiceDocumentId: issued.documentId!,
      bankTransactionId: claimTx.id,
    });
    expect(settled.ok).toBe(false);
    expect(settled.errors[0]).toContain("settle principal before claim receipts");

    db.close();
    cleanupDir(root);
  });
});
