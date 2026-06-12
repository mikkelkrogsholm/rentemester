// Tests: src/core/credit-notes.ts
import { describe, expect, test } from "bun:test";
import { existsSync, mkdtempSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { issueInvoice } from "../../src/core/issued-invoices";
import { issueCreditNote } from "../../src/core/credit-notes";
import { seedAccounts, verifyAuditChain } from "../../src/core/ledger";
import { postIssuedInvoiceToLedger } from "../../src/core/invoice-booking";
import { storeViesValidation } from "../../src/core/vies";

import { cleanupDir } from "../helpers/cleanup";
function failingDocumentInsertDb(realDb: any) {
  return new Proxy(realDb, {
    get(target, prop, receiver) {
      if (prop === "query") {
        return (sql: string) => {
          const statement = target.query(sql);
          if (sql.includes("INSERT INTO documents")) {
            return { get() { throw new Error("simulated insert failure"); } };
          }
          return statement;
        };
      }
      const value = Reflect.get(target, prop, receiver);
      return typeof value === "function" ? value.bind(target) : value;
    },
  }) as any;
}

describe("credit notes", () => {
  test("mirrors original invoice posting accounts when crediting a custom-booked invoice", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-credit-custom-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);
    db.run("INSERT OR IGNORE INTO accounts (account_no, name, type, normal_balance, default_vat_code) VALUES ('1001', 'Abonnementsomsætning', 'income', 'credit', NULL)");
    db.run("INSERT OR IGNORE INTO accounts (account_no, name, type, normal_balance, default_vat_code) VALUES ('1101', 'Debitorer abonnement', 'asset', 'debit', NULL)");
    db.run("INSERT OR IGNORE INTO accounts (account_no, name, type, normal_balance, default_vat_code) VALUES ('1201', 'Salgsmoms abonnement', 'vat', 'credit', NULL)");

    const issued = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-16",
      invoiceNumber: "2026-0001",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9" },
      lines: [{ description: "Abonnement", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
      currency: "DKK"
    });
    expect(issued.ok).toBe(true);
    expect(postIssuedInvoiceToLedger(db, {
      invoiceDocumentId: issued.documentId!,
      receivableAccountNo: "1101",
      revenueAccountNo: "1001",
      outputVatAccountNo: "1201"
    }).ok).toBe(true);

    const credit = issueCreditNote(db, root, {
      originalInvoiceDocumentId: issued.documentId!,
      issueDate: "2026-05-17",
      reason: "Partial correction",
      grossAmount: 625
    });
    expect(credit.ok).toBe(true);

    const lines = db.query(
      `SELECT a.account_no, jl.debit_amount, jl.credit_amount, jl.vat_code
       FROM journal_lines jl JOIN accounts a ON a.id = jl.account_id
       WHERE jl.journal_entry_id = ? ORDER BY jl.id ASC`
    ).all(credit.journalEntryId!) as any[];
    expect(lines).toEqual([
      { account_no: "1101", debit_amount: 0, credit_amount: 625, vat_code: null },
      { account_no: "1001", debit_amount: 500, credit_amount: 0, vat_code: "DK_SALE_25" },
      { account_no: "1201", debit_amount: 125, credit_amount: 0, vat_code: null },
    ]);

    const chain = verifyAuditChain(db);
    expect(chain.ok).toBe(true);

    db.close();
    cleanupDir(root);
  });

  test("rejects canonical manual credit-note numbers from the wrong fiscal scope", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-credit-scope-"));
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

    const credit = issueCreditNote(db, root, {
      originalInvoiceDocumentId: issued.documentId!,
      issueDate: "2026-05-17",
      creditNoteNumber: "CN-2099-0001",
      reason: "Wrong fiscal scope",
      grossAmount: 625
    });
    expect(credit.ok).toBe(false);
    expect(credit.errors[0]).toContain("does not match current fiscal scope 2026");

    db.close();
    cleanupDir(root);
  });

  test("does not burn an auto-numbered credit-note sequence when insert fails", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-credit-rollback-"));
    const realDb = openDb(ensureCompanyDirs(root).db);
    migrate(realDb);
    seedAccounts(realDb);

    const issued = issueInvoice(realDb, root, {
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

    const failingDb = failingDocumentInsertDb(realDb);
    const failed = issueCreditNote(failingDb, root, {
      originalInvoiceDocumentId: issued.documentId!,
      issueDate: "2026-05-17",
      reason: "Should roll back sequence",
      grossAmount: 625
    });
    expect(failed.ok).toBe(false);
    expect(failed.errors[0]).toContain("simulated insert failure");

    const sequence = realDb.query("SELECT value FROM sequences WHERE kind = 'credit_note' AND scope = 'company-1:2026'").get() as { value: number } | null;
    expect(sequence).toBeNull();

    const retried = issueCreditNote(realDb, root, {
      originalInvoiceDocumentId: issued.documentId!,
      issueDate: "2026-05-17",
      reason: "Retry succeeds",
      grossAmount: 625
    });
    expect(retried.ok).toBe(true);
    expect(retried.creditNoteNumber).toBe("CN-2026-0001");

    realDb.close();
    cleanupDir(root);
  });

  test("uses reverse-charge fallback lines when crediting an unposted reverse-charge invoice", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-credit-reverse-fallback-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);
    storeViesValidation(db, {
      vatOrCvr: "DE123456789",
      valid: true,
      validatedAt: "2026-05-15T00:00:00.000Z",
      expiresAt: "2026-08-15T00:00:00.000Z",
      rawResponse: JSON.stringify({ valid: true })
    });

    const issued = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "foreign_reverse_charge",
      reverseChargeBasis: "EU_MOMSDIREKTIV_ART_196",
      reverseChargeNote: "Reverse charge",
      issueDate: "2026-05-16",
      invoiceNumber: "2026-0001",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde GmbH", address: "Berlin", vatOrCvr: "DE123456789" },
      lines: [{ description: "Consulting", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, grossAmount: 1000 },
      currency: "DKK"
    });
    expect(issued.ok).toBe(true);

    const credit = issueCreditNote(db, root, {
      originalInvoiceDocumentId: issued.documentId!,
      issueDate: "2026-05-17",
      reason: "Cancel reverse-charge invoice"
    });
    expect(credit.ok).toBe(true);
    expect(credit.appliedRules).toContain("DK-INVOICE-BOOKKEEPING-REVERSE-002");

    const lines = db.query(
      `SELECT a.account_no, jl.debit_amount, jl.credit_amount, jl.vat_code
       FROM journal_lines jl JOIN accounts a ON a.id = jl.account_id
       WHERE jl.journal_entry_id = ? ORDER BY jl.id ASC`
    ).all(credit.journalEntryId!) as any[];
    expect(lines).toEqual([
      { account_no: "1000", debit_amount: 1000, credit_amount: 0, vat_code: "REVERSE_CHARGE_EXEMPT" },
      { account_no: "1100", debit_amount: 0, credit_amount: 1000, vat_code: null },
    ]);

    const chain = verifyAuditChain(db);
    expect(chain.ok).toBe(true);

    db.close();
    cleanupDir(root);
  });

  test("issues partial credit notes up to the original invoice amount and posts reversing sales lines", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-credit-"));
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

    const credit = issueCreditNote(db, root, {
      originalInvoiceDocumentId: issued.documentId!,
      issueDate: "2026-05-17",
      reason: "Partial correction",
      grossAmount: 625
    });
    expect(credit.ok).toBe(true);
    expect(credit.appliedRules).toContain("DK-CREDIT-NOTE-001");
    expect(existsSync(credit.storedPath!)).toBe(true);

    const doc = db.query("SELECT document_type, invoice_no, payment_details FROM documents WHERE id = ?").get(credit.documentId!) as any;
    expect(doc.document_type).toBe("credit_note");
    expect(doc.payment_details).toBe("2026-0001");

    const lines = db.query(
      `SELECT a.account_no, jl.debit_amount, jl.credit_amount, jl.vat_code
       FROM journal_lines jl JOIN accounts a ON a.id = jl.account_id
       WHERE jl.journal_entry_id = ? ORDER BY jl.id ASC`
    ).all(credit.journalEntryId!) as any[];
    expect(lines).toEqual([
      { account_no: "1000", debit_amount: 500, credit_amount: 0, vat_code: "DK_SALE_25" },
      { account_no: "1200", debit_amount: 125, credit_amount: 0, vat_code: null },
      { account_no: "1100", debit_amount: 0, credit_amount: 625, vat_code: null },
    ]);

    const second = issueCreditNote(db, root, {
      originalInvoiceDocumentId: issued.documentId!,
      issueDate: "2026-05-18",
      reason: "Final correction"
    });
    expect(second.ok).toBe(true);

    const third = issueCreditNote(db, root, {
      originalInvoiceDocumentId: issued.documentId!,
      issueDate: "2026-05-19",
      reason: "Too much",
      grossAmount: 1
    });
    expect(third.ok).toBe(false);
    expect(third.errors[0]).toContain("already fully credited");

    const chain = verifyAuditChain(db);
    expect(chain.ok).toBe(true);

    db.close();
    cleanupDir(root);
  });
});
