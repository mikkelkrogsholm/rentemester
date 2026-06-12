// Tests: src/core/invoice-compensation.ts
import { describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { issueInvoice } from "../../src/core/issued-invoices";
import { applyInvoicePayment, getInvoiceStatus } from "../../src/core/invoice-payments";
import { calculateInvoiceLateCompensation, postInvoiceLateCompensationToLedger, registerInvoiceLateCompensation } from "../../src/core/invoice-compensation";
import { seedAccounts, verifyAuditChain } from "../../src/core/ledger";

import { cleanupDir } from "../helpers/cleanup";
function failingCompensationPostingDb(realDb: any) {
  let failed = false;
  return new Proxy(realDb, {
    get(target, prop, receiver) {
      if (prop === "run") {
        return (sql: string, ...args: any[]) => {
          if (!failed && typeof sql === "string" && sql.includes("INSERT INTO invoice_compensation_postings")) {
            failed = true;
            throw new Error("simulated compensation posting link failure");
          }
          return target.run(sql, ...args);
        };
      }
      const value = Reflect.get(target, prop, receiver);
      return typeof value === "function" ? value.bind(target) : value;
    },
  }) as any;
}

describe("invoice late compensation", () => {
  test("marks overdue commercial invoice as eligible for statutory fixed compensation", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-comp-"));
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
    expect(applyInvoicePayment(db, {
      invoiceDocumentId: issued.documentId!,
      paymentDate: "2026-05-20",
      amount: 1000,
      note: "Partial payment"
    }).ok).toBe(true);

    const result = calculateInvoiceLateCompensation(db, {
      invoiceDocumentId: issued.documentId!,
      asOfDate: "2026-06-20",
    });
    expect(result.ok).toBe(true);
    expect(result.eligible).toBe(true);
    expect(result.compensationAmountDkk).toBe(310);
    expect(result.overdueDays).toBe(5);
    expect(result.appliedRules).toContain("DK-INVOICE-LATE-COMPENSATION-001");

    db.close();
    cleanupDir(root);
  });

  test("posts a registered compensation claim once to receivables and non-VAT claim income", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-comp-post-"));
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
    expect(applyInvoicePayment(db, {
      invoiceDocumentId: issued.documentId!,
      paymentDate: "2026-05-20",
      amount: 1000,
      note: "Partial payment"
    }).ok).toBe(true);
    expect(registerInvoiceLateCompensation(db, {
      invoiceDocumentId: issued.documentId!,
      asOfDate: "2026-06-20",
      note: "Statutory fixed compensation"
    }).ok).toBe(true);

    const posted = postInvoiceLateCompensationToLedger(db, { invoiceDocumentId: issued.documentId! });
    expect(posted.ok).toBe(true);
    expect(posted.compensationAmountDkk).toBe(310);
    expect(posted.appliedRules).toContain("DK-INVOICE-LATE-COMPENSATION-BOOKKEEPING-001");

    const lines = db.query(
      `SELECT a.account_no, jl.debit_amount, jl.credit_amount, jl.vat_code
       FROM journal_lines jl JOIN accounts a ON a.id = jl.account_id
       WHERE jl.journal_entry_id = ? ORDER BY jl.id ASC`
    ).all(posted.entryId!) as any[];
    expect(lines).toEqual([
      { account_no: "1100", debit_amount: 310, credit_amount: 0, vat_code: null },
      { account_no: "1010", debit_amount: 0, credit_amount: 310, vat_code: null },
    ]);

    const status = getInvoiceStatus(db, issued.documentId!, "2026-06-20");
    expect(status.ok).toBe(true);
    expect(status.compensationClaims?.[0]?.journalEntryId).toBe(posted.entryId);

    const second = postInvoiceLateCompensationToLedger(db, { invoiceDocumentId: issued.documentId! });
    expect(second.ok).toBe(false);
    expect(second.errors[0]).toContain("already posted");

    const chain = verifyAuditChain(db);
    expect(chain.ok).toBe(true);

    db.close();
    cleanupDir(root);
  });

  test("rolls back the journal entry if compensation posting link creation fails", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-comp-atomic-"));
    const realDb = openDb(ensureCompanyDirs(root).db);
    migrate(realDb);
    seedAccounts(realDb);
    const db = failingCompensationPostingDb(realDb);

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
    expect(applyInvoicePayment(realDb, {
      invoiceDocumentId: issued.documentId!,
      paymentDate: "2026-05-20",
      amount: 1000,
      note: "Partial payment"
    }).ok).toBe(true);
    expect(registerInvoiceLateCompensation(realDb, {
      invoiceDocumentId: issued.documentId!,
      asOfDate: "2026-06-20",
      note: "Statutory fixed compensation"
    }).ok).toBe(true);

    const failed = postInvoiceLateCompensationToLedger(db, { invoiceDocumentId: issued.documentId! });
    expect(failed.ok).toBe(false);
    expect(failed.errors[0]).toContain("simulated compensation posting link failure");
    expect(realDb.query("SELECT COUNT(*) AS n FROM journal_entries").get()).toEqual({ n: 1 });
    expect(realDb.query("SELECT COUNT(*) AS n FROM invoice_compensation_postings").get()).toEqual({ n: 0 });

    const retry = postInvoiceLateCompensationToLedger(realDb, { invoiceDocumentId: issued.documentId! });
    expect(retry.ok).toBe(true);
    expect(realDb.query("SELECT COUNT(*) AS n FROM journal_entries").get()).toEqual({ n: 2 });
    expect(realDb.query("SELECT COUNT(*) AS n FROM invoice_compensation_postings").get()).toEqual({ n: 1 });

    realDb.close();
    cleanupDir(root);
  });

  test("registers one immutable compensation claim and surfaces it in claim balance", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-comp-register-"));
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
    expect(applyInvoicePayment(db, {
      invoiceDocumentId: issued.documentId!,
      paymentDate: "2026-05-20",
      amount: 1000,
      note: "Partial payment"
    }).ok).toBe(true);

    const registered = registerInvoiceLateCompensation(db, {
      invoiceDocumentId: issued.documentId!,
      asOfDate: "2026-06-20",
      note: "Statutory fixed compensation"
    });
    expect(registered.ok).toBe(true);
    expect(registered.claimId).toBeDefined();
    expect(registered.compensationAmountDkk).toBe(310);
    expect(registered.claimOpenBalance).toBe(560);
    expect(registered.appliedRules).toContain("DK-INVOICE-LATE-COMPENSATION-REGISTER-001");

    const status = getInvoiceStatus(db, issued.documentId!, "2026-06-20");
    expect(status.ok).toBe(true);
    expect(status.openBalance).toBe(250);
    expect(status.claimOpenBalance).toBe(560);
    expect(status.totalCompensationClaims).toBe(310);
    expect(status.compensationClaims).toHaveLength(1);
    expect(status.compensationClaims?.[0]?.amountDkk).toBe(310);
    expect(status.compensationClaims?.[0]?.journalEntryId).toBe(null);

    const second = registerInvoiceLateCompensation(db, {
      invoiceDocumentId: issued.documentId!,
      asOfDate: "2026-06-21",
    });
    expect(second.ok).toBe(false);
    expect(second.errors[0]).toContain("already has a registered compensation claim");

    db.close();
    cleanupDir(root);
  });

  test("rejects a caller-supplied compensation amount above the statutory DKK 310", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-comp-cap-"));
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
    expect(applyInvoicePayment(db, {
      invoiceDocumentId: issued.documentId!,
      paymentDate: "2026-05-20",
      amount: 1000,
      note: "Partial payment"
    }).ok).toBe(true);

    const calc = calculateInvoiceLateCompensation(db, {
      invoiceDocumentId: issued.documentId!,
      asOfDate: "2026-06-20",
      compensationAmountDkk: 9999,
    });
    expect(calc.ok).toBe(false);
    expect(calc.errors[0]).toContain("310");

    const registered = registerInvoiceLateCompensation(db, {
      invoiceDocumentId: issued.documentId!,
      asOfDate: "2026-06-20",
      compensationAmountDkk: 9999,
    });
    expect(registered.ok).toBe(false);
    expect(db.query("SELECT COUNT(*) AS n FROM invoice_compensation_claims").get()).toEqual({ n: 0 });

    db.close();
    cleanupDir(root);
  });

  test("treats a non-CVR-shaped buyer identifier as a non-commercial transaction", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-comp-bad-cvr-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);

    const issued = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-16",
      dueDate: "2026-06-15",
      invoiceNumber: "2026-0001",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Privat Kunde", address: "Købervej 9", vatOrCvr: "ikke-et-cvr" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
      currency: "DKK"
    });
    expect(issued.ok).toBe(true);

    const result = calculateInvoiceLateCompensation(db, {
      invoiceDocumentId: issued.documentId!,
      asOfDate: "2026-06-20",
    });
    expect(result.ok).toBe(true);
    expect(result.isCommercialTransaction).toBe(false);
    expect(result.eligible).toBe(false);

    db.close();
    cleanupDir(root);
  });

  test("does not allow compensation when commercial status is not proven", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-comp-no-buyer-vat-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);

    const issued = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-16",
      dueDate: "2026-06-15",
      invoiceNumber: "2026-0001",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde", address: "Købervej 9" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
      currency: "DKK"
    });
    expect(issued.ok).toBe(true);

    const result = calculateInvoiceLateCompensation(db, {
      invoiceDocumentId: issued.documentId!,
      asOfDate: "2026-06-20",
    });
    expect(result.ok).toBe(true);
    expect(result.eligible).toBe(false);
    expect(result.compensationAmountDkk).toBe(0);
    expect(result.reason).toContain("commercial transaction not proven");

    db.close();
    cleanupDir(root);
  });

  test("does not apply the statutory amount to invoices predating 2013-03-01", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-comp-pre-2013-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);

    const issued = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2013-02-15",
      dueDate: "2013-02-20",
      invoiceNumber: "2013-0001",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9", vatOrCvr: "DK87654321" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
      currency: "DKK"
    });
    expect(issued.ok).toBe(true);

    const result = calculateInvoiceLateCompensation(db, {
      invoiceDocumentId: issued.documentId!,
      asOfDate: "2013-03-10",
    });
    expect(result.ok).toBe(true);
    expect(result.eligible).toBe(false);
    expect(result.compensationAmountDkk).toBe(0);
    expect(result.reason).toContain("predates statutory compensation start date 2013-03-01");

    db.close();
    cleanupDir(root);
  });
});
