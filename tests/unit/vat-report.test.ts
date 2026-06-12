// Tests: src/core/vat.ts (VAT report)
import { describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { ingestDocument } from "../../src/core/documents";
import { buildVatReport, vatFilingDeadline } from "../../src/core/vat";
import { postJournalEntry, reverseJournalEntry, seedAccounts } from "../../src/core/ledger";

import { cleanupDir } from "../helpers/cleanup";
describe("vatFilingDeadline (#236)", () => {
  test("returns the 1st of the third month after the period ends", () => {
    // Q1 ends 31-03 → due 1 June.
    expect(vatFilingDeadline("2026-03-31")).toBe("2026-06-01");
    // Q2 ends 30-06 → due 1 September.
    expect(vatFilingDeadline("2026-06-30")).toBe("2026-09-01");
    // Q3 ends 30-09 → due 1 December.
    expect(vatFilingDeadline("2026-09-30")).toBe("2026-12-01");
  });

  test("rolls into the next year for a Q4 period", () => {
    // Q4 ends 31-12 → due 1 March the following year.
    expect(vatFilingDeadline("2026-12-31")).toBe("2027-03-01");
    expect(vatFilingDeadline("2026-11-30")).toBe("2027-02-01");
  });

  test("returns null for an invalid period-end date", () => {
    expect(vatFilingDeadline("not-a-date")).toBeNull();
    expect(vatFilingDeadline("")).toBeNull();
  });
});

describe("vat report", () => {
  test("builds deterministic VAT totals from journal entries in a period", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-vat-"));
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-vat-inbox-"));
    const sourceFile = join(inbox, "invoice.txt");
    writeFileSync(sourceFile, "Invoice\n1250 DKK\n");

    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    const doc = ingestDocument(db, root, sourceFile, {
      source: "email",
      issueDate: "2026-05-16",
      invoiceNo: "INV-VAT-1",
      deliveryDescription: "Softwareabonnement",
      amountIncVat: 1250,
      currency: "DKK",
      sender: { name: "Leverandør ApS", address: "Sælgervej 1", vatOrCvr: "DK11223344" },
      recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      vatAmount: 250,
      paymentDetails: "Bankoverførsel"
    });
    expect(doc.ok).toBe(true);

    const sale = postJournalEntry(db, {
      transactionDate: "2026-05-10",
      text: "Consulting sale",
      documentId: doc.documentId,
      lines: [
        { accountNo: "2000", debitAmount: 1250 },
        { accountNo: "1000", creditAmount: 1000, vatCode: "DK_SALE_25" },
        { accountNo: "1200", creditAmount: 250 }
      ]
    });
    expect(sale.ok).toBe(true);

    const purchase = postJournalEntry(db, {
      transactionDate: "2026-05-16",
      text: "Software purchase",
      documentId: doc.documentId,
      lines: [
        { accountNo: "3000", debitAmount: 1000, vatCode: "DK_PURCHASE_25" },
        { accountNo: "4000", debitAmount: 250 },
        { accountNo: "2000", creditAmount: 1250 }
      ]
    });
    expect(purchase.ok).toBe(true);

    const may = buildVatReport(db, "2026-05-01", "2026-05-31");
    expect(may.ok).toBe(true);
    expect(may.outputVat).toBe(250);
    expect(may.inputVat).toBe(250);
    expect(may.netVatPayable).toBe(0);
    expect(may.salesBase25).toBe(1000);
    expect(may.purchaseBase25).toBe(1000);
    expect(may.badDebtReliefBase25).toBe(0);
    expect(may.journalEntryCount).toBe(2);
    expect(may.totalJournalEntryCount).toBe(2);
    expect(may.warnings).toEqual([]);

    const reversed = reverseJournalEntry(db, {
      entryId: sale.entryId!,
      transactionDate: "2026-06-01",
      reason: "Sale moved to next month"
    });
    expect(reversed.ok).toBe(true);

    const june = buildVatReport(db, "2026-06-01", "2026-06-30");
    expect(june.ok).toBe(true);
    expect(june.outputVat).toBe(-250);
    expect(june.salesBase25).toBe(-1000);
    expect(june.netVatPayable).toBe(-250);
    expect(june.journalEntryCount).toBe(0);
    expect(june.reversalJournalEntryCount).toBe(1);
    expect(june.reversedJournalEntryCount).toBe(0);
    expect(june.totalJournalEntryCount).toBe(1);
    expect(june.linesConsidered).toBe(0);
    expect(june.reversalLinesConsidered).toBe(3);
    expect(june.totalLinesConsidered).toBe(3);

    db.close();
    cleanupDir(root);
    cleanupDir(inbox);
  });

  test("does not warn on aggregate-vs-per-line rounding of correctly-booked odd-ore VAT (#142)", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-vat-rounding-"));
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-vat-rounding-inbox-"));
    const sourceFile = join(inbox, "invoice.txt");
    writeFileSync(sourceFile, "Invoice\n4.16 DKK\n");

    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    const doc = ingestDocument(db, root, sourceFile, {
      source: "email",
      issueDate: "2026-05-16",
      invoiceNo: "INV-VAT-ODD",
      deliveryDescription: "Odd-ore purchase",
      amountIncVat: 4.16,
      currency: "DKK",
      sender: { name: "Leverandør ApS", address: "Sælgervej 1", vatOrCvr: "DK11223344" },
      recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      vatAmount: 0.83,
      paymentDetails: "Bankoverførsel"
    });
    expect(doc.ok).toBe(true);

    // Three purchases each with net 3.33 → per-entry VAT 0.83 (3.33 × 25%
    // rounds to 0.8325 → 0.83). Booked input VAT total = 2.49.
    // 25% of the summed base 9.99 = 2.4975 → 2.50. The 1-øre aggregate gap
    // is a correct ledger and must NOT raise a reconciliation warning.
    for (let i = 0; i < 3; i++) {
      const purchase = postJournalEntry(db, {
        transactionDate: "2026-05-16",
        text: `Odd-ore purchase ${i + 1}`,
        documentId: doc.documentId,
        lines: [
          { accountNo: "3000", debitAmount: 3.33, vatCode: "DK_PURCHASE_25" },
          { accountNo: "4000", debitAmount: 0.83 },
          { accountNo: "2000", creditAmount: 4.16 }
        ]
      });
      expect(purchase.ok).toBe(true);
    }

    const report = buildVatReport(db, "2026-05-01", "2026-05-31");
    expect(report.ok).toBe(true);
    expect(report.purchaseBase25).toBe(9.99);
    expect(report.inputVat).toBe(2.49);
    expect(report.warnings).toEqual([]);

    db.close();
    cleanupDir(root);
    cleanupDir(inbox);
  });

  test("warns when VAT base and booked VAT do not reconcile and distinguishes reversed entries in-period", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-vat-warning-"));
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-vat-warning-inbox-"));
    const sourceFile = join(inbox, "invoice.txt");
    writeFileSync(sourceFile, "Invoice\n1250 DKK\n");

    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    const doc = ingestDocument(db, root, sourceFile, {
      source: "email",
      issueDate: "2026-05-16",
      invoiceNo: "INV-VAT-2",
      deliveryDescription: "Softwareabonnement",
      amountIncVat: 1250,
      currency: "DKK",
      sender: { name: "Leverandør ApS", address: "Sælgervej 1", vatOrCvr: "DK11223344" },
      recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      vatAmount: 250,
      paymentDetails: "Bankoverførsel"
    });
    expect(doc.ok).toBe(true);

    const brokenPurchase = postJournalEntry(db, {
      transactionDate: "2026-05-12",
      text: "Broken VAT booking",
      documentId: doc.documentId,
      lines: [
        { accountNo: "3000", debitAmount: 1000, vatCode: "DK_PURCHASE_25" },
        { accountNo: "2000", creditAmount: 1000 }
      ]
    });
    expect(brokenPurchase.ok).toBe(true);

    const reversibleSale = postJournalEntry(db, {
      transactionDate: "2026-05-13",
      text: "Sale booked then reversed",
      documentId: doc.documentId,
      lines: [
        { accountNo: "2000", debitAmount: 1250 },
        { accountNo: "1000", creditAmount: 1000, vatCode: "DK_SALE_25" },
        { accountNo: "1200", creditAmount: 250 }
      ]
    });
    expect(reversibleSale.ok).toBe(true);

    const reversed = reverseJournalEntry(db, {
      entryId: reversibleSale.entryId!,
      transactionDate: "2026-05-20",
      reason: "Booked in wrong month"
    });
    expect(reversed.ok).toBe(true);

    const report = buildVatReport(db, "2026-05-01", "2026-05-31");
    expect(report.ok).toBe(true);
    expect(report.purchaseBase25).toBe(1000);
    expect(report.inputVat).toBe(0);
    expect(report.outputVat).toBe(0);
    expect(report.netVatPayable).toBe(0);
    expect(report.journalEntryCount).toBe(1);
    expect(report.reversedJournalEntryCount).toBe(1);
    expect(report.reversalJournalEntryCount).toBe(1);
    expect(report.totalJournalEntryCount).toBe(3);
    expect(report.linesConsidered).toBe(2);
    expect(report.reversedLinesConsidered).toBe(3);
    expect(report.reversalLinesConsidered).toBe(3);
    expect(report.totalLinesConsidered).toBe(8);
    expect(report.warnings).toContain("input VAT mismatch: booked 0, expected from base × rate 250");

    db.close();
    cleanupDir(root);
    cleanupDir(inbox);
  });
});
