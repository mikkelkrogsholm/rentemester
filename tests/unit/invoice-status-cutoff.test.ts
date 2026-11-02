import { expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { seedAccounts } from "../../src/core/ledger";
import { issueInvoice } from "../../src/core/issued-invoices";
import { postIssuedInvoiceToLedger } from "../../src/core/invoice-booking";
import { applyInvoicePayment, getInvoiceStatus } from "../../src/core/invoice-payments";
import { issueCreditNote } from "../../src/core/credit-notes";
import { buildInvoiceList } from "../../src/core/invoice-list";
import { writeOffInvoiceBadDebt } from "../../src/core/invoice-bad-debt";
import { importBankCsv } from "../../src/core/bank";
import { refundInvoiceToBank } from "../../src/core/invoice-refunds";
import { registerInvoiceReminder } from "../../src/core/invoice-reminders";
import { registerInvoiceLateCompensation } from "../../src/core/invoice-compensation";
import { registerInvoiceLateInterest } from "../../src/core/invoice-interest";

function fixture() {
  const root = mkdtempSync(join(tmpdir(), "rentemester-status-cutoff-"));
  const db = openDb(ensureCompanyDirs(root).db);
  migrate(db); seedAccounts(db);
  const issued = issueInvoice(db, root, {
    invoiceType: "full", vatTreatment: "standard", issueDate: "2026-05-16", dueDate: "2026-06-15", invoiceNumber: "2026-0001",
    seller: { name: "Synthetic Seller", address: "Example 1", vatOrCvr: "DK12345678" },
    buyer: { name: "Synthetic Buyer", address: "Example 2", vatOrCvr: "DK87654321" },
    lines: [{ description: "Service", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
    totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 }, currency: "DKK",
  });
  expect(issued.ok).toBe(true);
  const invoiceDocumentId = issued.documentId!;
  expect(postIssuedInvoiceToLedger(db, { invoiceDocumentId }).ok).toBe(true);
  return { db, root, invoiceDocumentId, close() { db.close(); rmSync(root, { recursive: true, force: true }); } };
}

test("historical invoice lifecycle agrees with the debtor ledger at inclusive cutoffs", () => {
  const f = fixture(); const { db, invoiceDocumentId } = f;
  try {
    expect(applyInvoicePayment(db, { invoiceDocumentId, paymentDate: "2026-05-20", amount: 500 }).ok).toBe(true);
    expect(issueCreditNote(db, f.root, { originalInvoiceDocumentId: invoiceDocumentId, issueDate: "2026-05-21", reason: "Partial cancellation", grossAmount: 250 }).ok).toBe(true);
    expect(applyInvoicePayment(db, { invoiceDocumentId, paymentDate: "2026-05-22", amount: 500 }).ok).toBe(true);
    for (const [date, expected] of [["2026-05-16", 1250], ["2026-05-20", 750], ["2026-05-21", 500], ["2026-05-22", 0]] as const) {
      const status = getInvoiceStatus(db, invoiceDocumentId, date);
      expect(status.ok).toBe(true);
      expect(status.openBalance).toBe(expected);
      const ledger = db.query(`SELECT ROUND(SUM(l.debit_amount-l.credit_amount),2) AS balance
        FROM journal_lines l JOIN journal_entries j ON j.id=l.journal_entry_id
        JOIN accounts a ON a.id=l.account_id WHERE a.account_no='1100' AND j.transaction_date<=?`).get(date) as { balance: number };
      expect(status.openBalance).toBe(ledger.balance);
      expect(status.payments!.every(p => p.paymentDate <= date)).toBe(true);
      expect(status.creditNotes!.every(c => c.issueDate! <= date)).toBe(true);
    }
    expect(getInvoiceStatus(db, invoiceDocumentId).openBalance).toBe(0);
    expect(getInvoiceStatus(db, invoiceDocumentId, "2026-05-15").ok).toBe(false);
    expect(getInvoiceStatus(db, invoiceDocumentId, "2026-02-30").ok).toBe(false);
    expect(buildInvoiceList(db, { asOfDate: "2026-05-20" }).rows[0].openBalance).toBe(750);
    expect(buildInvoiceList(db, { asOfDate: "2026-05-15" }).rows).toHaveLength(0);
    // Historical reporting must not relax current mutation caps.
    expect(applyInvoicePayment(db, { invoiceDocumentId, paymentDate: "2026-05-20", amount: 1 }).ok).toBe(false);
    expect(writeOffInvoiceBadDebt(db, { invoiceDocumentId, writeOffDate: "2026-05-20", grossAmount: 1 }).ok).toBe(false);
  } finally { f.close(); }
});

test("historical status excludes later refunds and includes them on their date", () => {
  const f = fixture(); const { db, invoiceDocumentId } = f;
  try {
    expect(applyInvoicePayment(db, { invoiceDocumentId, paymentDate: "2026-05-20", amount: 1250 }).ok).toBe(true);
    expect(issueCreditNote(db, f.root, { originalInvoiceDocumentId: invoiceDocumentId, issueDate: "2026-05-21", reason: "Cancellation" }).ok).toBe(true);
    const csv = join(f.root, "refund.csv");
    writeFileSync(csv, "transaction_date,booking_date,text,amount,currency,reference\n2026-05-22,2026-05-22,Synthetic refund,-1250,DKK,REFUND\n");
    expect(importBankCsv(db, f.root, csv).ok).toBe(true);
    expect(refundInvoiceToBank(db, { invoiceDocumentId, bankTransactionReference: "REFUND" }).ok).toBe(true);
    expect(getInvoiceStatus(db, invoiceDocumentId, "2026-05-21").openBalance).toBe(-1250);
    expect(getInvoiceStatus(db, invoiceDocumentId, "2026-05-21").refunds).toHaveLength(0);
    expect(getInvoiceStatus(db, invoiceDocumentId, "2026-05-22").openBalance).toBe(0);
  } finally { f.close(); }
});

test("historical status excludes later write-offs and fee claims", () => {
  const f = fixture(); const { db, invoiceDocumentId } = f;
  try {
    expect(registerInvoiceReminder(db, { invoiceDocumentId, reminderDate: "2026-06-26" }).ok).toBe(true);
    expect(registerInvoiceLateCompensation(db, { invoiceDocumentId, asOfDate: "2026-06-27" }).ok).toBe(true);
    expect(registerInvoiceLateInterest(db, { invoiceDocumentId, asOfDate: "2026-06-28", referenceRatePercent: 2.2 }).ok).toBe(true);
    expect(writeOffInvoiceBadDebt(db, { invoiceDocumentId, writeOffDate: "2026-07-01", grossAmount: 1250 }).ok).toBe(true);
    const before = getInvoiceStatus(db, invoiceDocumentId, "2026-06-25");
    expect(before.ok).toBe(true);
    expect(before.openBalance).toBe(1250);
    expect(before.claimOpenBalance).toBe(1250);
    expect(before.reminders).toHaveLength(0);
    expect(before.compensationClaims).toHaveLength(0);
    expect(before.interestClaims).toHaveLength(0);
    expect(before.badDebtWriteOffs).toHaveLength(0);
    expect(getInvoiceStatus(db, invoiceDocumentId, "2026-06-26").reminders).toHaveLength(1);
    expect(getInvoiceStatus(db, invoiceDocumentId, "2026-06-27").compensationClaims).toHaveLength(1);
    expect(getInvoiceStatus(db, invoiceDocumentId, "2026-06-28").interestClaims).toHaveLength(1);
    expect(getInvoiceStatus(db, invoiceDocumentId, "2026-07-01").openBalance).toBe(0);
  } finally { f.close(); }
});
