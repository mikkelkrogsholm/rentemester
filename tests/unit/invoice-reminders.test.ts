// Tests: src/core/invoice-reminders.ts
import { describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { issueInvoice } from "../../src/core/issued-invoices";
import { postInvoiceReminderToLedger, registerInvoiceReminder } from "../../src/core/invoice-reminders";
import { getInvoiceStatus } from "../../src/core/invoice-payments";
import { seedAccounts, verifyAuditChain } from "../../src/core/ledger";

import { cleanupDir } from "../helpers/cleanup";
function failingReminderPostingDb(realDb: any) {
  let failed = false;
  return new Proxy(realDb, {
    get(target, prop, receiver) {
      if (prop === "run") {
        return (sql: string, ...args: any[]) => {
          if (!failed && typeof sql === "string" && sql.includes("INSERT INTO invoice_reminder_postings")) {
            failed = true;
            throw new Error("simulated reminder posting link failure");
          }
          return target.run(sql, ...args);
        };
      }
      const value = Reflect.get(target, prop, receiver);
      return typeof value === "function" ? value.bind(target) : value;
    },
  }) as any;
}

describe("invoice reminders", () => {
  test("registers a statutory reminder fee on an overdue invoice", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-reminder-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);

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

    const reminder = registerInvoiceReminder(db, {
      invoiceDocumentId: issued.documentId!,
      reminderDate: "2026-06-26",
    });
    expect(reminder.ok).toBe(true);
    expect(reminder.reminderSequence).toBe(1);
    expect(reminder.feeAmount).toBe(100);
    expect(reminder.totalReminderFees).toBe(100);

    const status = getInvoiceStatus(db, issued.documentId!, "2026-06-26");
    expect(status.ok).toBe(true);
    expect(status.totalReminderFees).toBe(100);
    expect(status.reminders).toHaveLength(1);
    expect(status.reminders?.[0]?.feeAmount).toBe(100);
    expect(status.reminders?.[0]?.journalEntryId).toBe(null);

    db.close();
    cleanupDir(root);
  });

  test("posts a registered reminder fee once to receivables and non-VAT claim income", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-reminder-post-"));
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
    const reminder = registerInvoiceReminder(db, {
      invoiceDocumentId: issued.documentId!,
      reminderDate: "2026-06-26",
    });
    expect(reminder.ok).toBe(true);

    const posted = postInvoiceReminderToLedger(db, { invoiceDocumentId: issued.documentId! });
    expect(posted.ok).toBe(true);
    expect(posted.feeAmount).toBe(100);
    expect(posted.appliedRules).toContain("DK-INVOICE-REMINDER-FEE-BOOKKEEPING-001");

    const lines = db.query(
      `SELECT a.account_no, jl.debit_amount, jl.credit_amount, jl.vat_code
       FROM journal_lines jl JOIN accounts a ON a.id = jl.account_id
       WHERE jl.journal_entry_id = ? ORDER BY jl.id ASC`
    ).all(posted.entryId!) as any[];
    expect(lines).toEqual([
      { account_no: "1100", debit_amount: 100, credit_amount: 0, vat_code: null },
      { account_no: "1010", debit_amount: 0, credit_amount: 100, vat_code: null },
    ]);

    const status = getInvoiceStatus(db, issued.documentId!, "2026-06-26");
    expect(status.ok).toBe(true);
    expect(status.reminders?.[0]?.journalEntryId).toBe(posted.entryId);

    const second = postInvoiceReminderToLedger(db, { invoiceDocumentId: issued.documentId! });
    expect(second.ok).toBe(false);
    expect(second.errors[0]).toContain("already posted");

    const chain = verifyAuditChain(db);
    expect(chain.ok).toBe(true);

    db.close();
    cleanupDir(root);
  });

  test("rolls back the journal entry if reminder posting link creation fails", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-reminder-atomic-"));
    const realDb = openDb(ensureCompanyDirs(root).db);
    migrate(realDb);
    seedAccounts(realDb);
    const db = failingReminderPostingDb(realDb);

    const issued = issueInvoice(realDb, root, {
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
    expect(registerInvoiceReminder(realDb, { invoiceDocumentId: issued.documentId!, reminderDate: "2026-06-26" }).ok).toBe(true);

    const failed = postInvoiceReminderToLedger(db, { invoiceDocumentId: issued.documentId! });
    expect(failed.ok).toBe(false);
    expect(failed.errors[0]).toContain("simulated reminder posting link failure");
    expect(realDb.query("SELECT COUNT(*) AS n FROM journal_entries").get()).toEqual({ n: 0 });
    expect(realDb.query("SELECT COUNT(*) AS n FROM invoice_reminder_postings").get()).toEqual({ n: 0 });

    const retry = postInvoiceReminderToLedger(realDb, { invoiceDocumentId: issued.documentId! });
    expect(retry.ok).toBe(true);
    expect(realDb.query("SELECT COUNT(*) AS n FROM journal_entries").get()).toEqual({ n: 1 });
    expect(realDb.query("SELECT COUNT(*) AS n FROM invoice_reminder_postings").get()).toEqual({ n: 1 });

    realDb.close();
    cleanupDir(root);
  });

  test("blocks a fourth reminder and reminders sent too close together", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-reminder-limits-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);

    const issued = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-16",
      dueDate: "2026-06-01",
      invoiceNumber: "2026-0001",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9", vatOrCvr: "DK87654321" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
      currency: "DKK"
    });
    expect(issued.ok).toBe(true);

    expect(registerInvoiceReminder(db, { invoiceDocumentId: issued.documentId!, reminderDate: "2026-06-11" }).ok).toBe(true);
    const tooSoon = registerInvoiceReminder(db, { invoiceDocumentId: issued.documentId!, reminderDate: "2026-06-20" });
    expect(tooSoon.ok).toBe(false);
    expect(tooSoon.errors[0]).toContain("at least 10 days");

    db.close();
    cleanupDir(root);
  });
});
