import { expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { initialiseCompanyVolume } from "../../src/core/company";
import { openDb } from "../../src/core/db";
import { companyPaths } from "../../src/core/paths";
import { issueInvoice } from "../../src/core/issued-invoices";
import { postIssuedInvoiceToLedger } from "../../src/core/invoice-booking";
import { importBankCsv } from "../../src/core/bank";
import { settleInvoiceFromBank } from "../../src/core/invoice-settlement";
import { refundInvoiceToBank } from "../../src/core/invoice-refunds";
import { settleInvoiceClaimsFromBank } from "../../src/core/invoice-claim-settlement";
import { issueCreditNote } from "../../src/core/credit-notes";
import { registerInvoiceReminder, postInvoiceReminderToLedger } from "../../src/core/invoice-reminders";

for (const kind of ["principal", "refund", "claim"] as const) {
  test(`${kind} requires an unambiguous reference or a consistent explicit bank ID`, () => {
    const root = mkdtempSync(join(tmpdir(), `rentemester-bank-select-${kind}-`));
    initialiseCompanyVolume(root, { cvr: "DK12345678" });
    const db = openDb(companyPaths(root).db);
    try {
      const invoice = issueInvoice(db, root, {
        invoiceType: "full", vatTreatment: "standard", issueDate: "2026-05-16", dueDate: "2026-06-15",
        seller: { name: "Synthetic Seller ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
        buyer: { name: "Synthetic Buyer ApS", address: "Testvej 2" },
        lines: [{ description: "Synthetic service", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
        totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 }, currency: "DKK",
      });
      expect(invoice.ok).toBe(true);
      const invoiceDocumentId = invoice.documentId!;
      expect(postIssuedInvoiceToLedger(db, { invoiceDocumentId }).ok).toBe(true);
      if (kind === "claim") {
        expect(registerInvoiceReminder(db, { invoiceDocumentId, reminderDate: "2026-06-26" }).ok).toBe(true);
        expect(postInvoiceReminderToLedger(db, { invoiceDocumentId }).ok).toBe(true);
      }
      if (kind !== "principal") {
        const paidCsv = join(root, "paid.csv");
        writeFileSync(paidCsv, "transaction_date,text,amount,currency,reference\n2026-06-27,Principal,1250,DKK,PRINCIPAL\n");
        expect(importBankCsv(db, root, paidCsv).ok).toBe(true);
        expect(settleInvoiceFromBank(db, { invoiceDocumentId, bankTransactionReference: "PRINCIPAL" }).ok).toBe(true);
      }
      if (kind === "refund") expect(issueCreditNote(db, root, { originalInvoiceDocumentId: invoiceDocumentId, issueDate: "2026-06-28", reason: "Synthetic cancellation" }).ok).toBe(true);
      const amount = kind === "refund" ? -1250 : kind === "claim" ? 100 : 1250;
      const csv = join(root, "ambiguous.csv");
      writeFileSync(csv, `transaction_date,text,amount,currency,reference\n2026-06-29,First payment,${amount},DKK,DUP-REF\n2026-06-30,Second payment,${amount},DKK,DUP-REF\n`);
      expect(importBankCsv(db, root, csv).ok).toBe(true);
      const operation = kind === "principal" ? settleInvoiceFromBank : kind === "refund" ? refundInvoiceToBank : settleInvoiceClaimsFromBank;
      const snapshot = () => ["journal_entries", "journal_lines", "invoice_payments", "invoice_refunds", "invoice_claim_payments", "bank_journal_reconciliations", "audit_log", "sequences"].map(table => db.query(`SELECT * FROM ${table}`).all());
      const before = snapshot();
      expect(operation(db, { invoiceDocumentId, bankTransactionReference: "DUP-REF" })).toMatchObject({ ok: false, errors: ["bank transaction reference DUP-REF is ambiguous; provide bankTransactionId"] });
      expect(snapshot()).toEqual(before);
      const bank = db.query("SELECT id FROM bank_transactions WHERE reference='DUP-REF' ORDER BY id LIMIT 1").get() as { id: number };
      expect(operation(db, { invoiceDocumentId, bankTransactionId: bank.id, bankTransactionReference: "DIFFERENT" })).toMatchObject({ ok: false, errors: ["bankTransactionId and bankTransactionReference must identify the same bank transaction"] });
      expect(snapshot()).toEqual(before);
      expect(operation(db, { invoiceDocumentId, bankTransactionId: bank.id, bankTransactionReference: "DUP-REF" }).ok).toBe(true);
      expect(db.query("SELECT bank_transaction_id FROM bank_journal_reconciliations WHERE bank_transaction_id=?").get(bank.id)).toEqual({ bank_transaction_id: bank.id });
    } finally { db.close(); rmSync(root, { recursive: true, force: true }); }
  });
}
