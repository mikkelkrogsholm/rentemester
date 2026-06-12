// Tests: src/core/invoice-refunds.ts
import { describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { seedAccounts, verifyAuditChain } from "../../src/core/ledger";
import { importBankCsv } from "../../src/core/bank";
import { issueInvoice } from "../../src/core/issued-invoices";
import { postIssuedInvoiceToLedger } from "../../src/core/invoice-booking";
import { settleInvoiceFromBank } from "../../src/core/invoice-settlement";
import { issueCreditNote } from "../../src/core/credit-notes";
import { refundInvoiceToBank } from "../../src/core/invoice-refunds";
import { getInvoiceStatus } from "../../src/core/invoice-payments";

import { cleanupDir } from "../helpers/cleanup";
describe("invoice refunds", () => {
  test("requires explicit bank transaction selection for refunds", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-refund-explicit-bank-"));
    const paths = ensureCompanyDirs(root);
    const db = openDb(paths.db);
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

    const paymentCsv = join(root, "payment.csv");
    writeFileSync(paymentCsv, "transaction_date,booking_date,text,amount,currency,reference\n2026-05-20,2026-05-20,Customer payment,1250,DKK,INV-0800B\n");
    expect(importBankCsv(db, root, paymentCsv).ok).toBe(true);
    expect(settleInvoiceFromBank(db, { invoiceDocumentId: issued.documentId!, bankTransactionReference: "INV-0800B" }).ok).toBe(true);

    const credit = issueCreditNote(db, root, {
      originalInvoiceDocumentId: issued.documentId!,
      issueDate: "2026-05-21",
      reason: "Work cancelled"
    });
    expect(credit.ok).toBe(true);

    const refundCsv = join(root, "refund.csv");
    writeFileSync(refundCsv, "transaction_date,booking_date,text,amount,currency,reference\n2026-05-22,2026-05-22,Other customer refund,-1250,DKK,RFND-OTHER\n");
    expect(importBankCsv(db, root, refundCsv).ok).toBe(true);

    const refund = refundInvoiceToBank(db, {
      invoiceDocumentId: issued.documentId!,
    });
    expect(refund.ok).toBe(false);
    expect(refund.errors[0]).toBe("bankTransactionId or bankTransactionReference is required");

    db.close();
    cleanupDir(root);
  });

  test("refunds a paid-and-credited invoice from an outgoing bank transaction", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-refund-"));
    const paths = ensureCompanyDirs(root);
    const db = openDb(paths.db);
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

    const paymentCsv = join(root, "payment.csv");
    writeFileSync(paymentCsv, "transaction_date,booking_date,text,amount,currency,reference\n2026-05-20,2026-05-20,Customer payment,1250,DKK,INV-0800\n");
    expect(importBankCsv(db, root, paymentCsv).ok).toBe(true);
    expect(settleInvoiceFromBank(db, { invoiceDocumentId: issued.documentId!, bankTransactionReference: "INV-0800" }).ok).toBe(true);

    const credit = issueCreditNote(db, root, {
      originalInvoiceDocumentId: issued.documentId!,
      issueDate: "2026-05-21",
      reason: "Work cancelled"
    });
    expect(credit.ok).toBe(true);

    const preRefund = getInvoiceStatus(db, issued.documentId!);
    expect(preRefund.status).toBe("overpaid");
    expect(preRefund.openBalance).toBe(-1250);

    const refundCsv = join(root, "refund.csv");
    writeFileSync(refundCsv, "transaction_date,booking_date,text,amount,currency,reference\n2026-05-22,2026-05-22,Customer refund,-1250,DKK,RFND-0800\n");
    expect(importBankCsv(db, root, refundCsv).ok).toBe(true);

    const refund = refundInvoiceToBank(db, {
      invoiceDocumentId: issued.documentId!,
      bankTransactionReference: "RFND-0800"
    });
    expect(refund.ok).toBe(true);
    expect(refund.remainingCreditBalance).toBe(0);
    expect(refund.appliedRules).toContain("DK-INVOICE-REFUND-001");

    const status = getInvoiceStatus(db, issued.documentId!);
    expect(status.status).toBe("refunded");
    expect(status.openBalance).toBe(0);
    expect(status.refunds).toHaveLength(1);

    const chain = verifyAuditChain(db);
    expect(chain.ok).toBe(true);

    db.close();
    cleanupDir(root);
  });
});
