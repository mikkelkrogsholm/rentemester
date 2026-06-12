// Tests: src/core/invoice-bad-debt.ts
import { describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { issueInvoice } from "../../src/core/issued-invoices";
import { postIssuedInvoiceToLedger } from "../../src/core/invoice-booking";
import { getInvoiceStatus } from "../../src/core/invoice-payments";
import { writeOffInvoiceBadDebt } from "../../src/core/invoice-bad-debt";
import { buildVatReport } from "../../src/core/vat";
import { seedAccounts, verifyAuditChain } from "../../src/core/ledger";

import { cleanupDir } from "../helpers/cleanup";
describe("invoice bad debt", () => {
  test("writes off an unpaid standard-rated invoice and reduces output VAT deterministically", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-bad-debt-"));
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

    const writeOff = writeOffInvoiceBadDebt(db, {
      invoiceDocumentId: issued.documentId!,
      writeOffDate: "2026-07-01",
    });
    expect(writeOff.ok).toBe(true);
    expect(writeOff.appliedRules).toContain("DK-INVOICE-BAD-DEBT-WRITEOFF-001");
    expect(writeOff.appliedRules).toContain("DK-VAT-BAD-DEBT-001");
    expect(writeOff.grossAmount).toBe(1250);
    expect(writeOff.netAmount).toBe(1000);
    expect(writeOff.vatAmount).toBe(250);
    expect(writeOff.openBalance).toBe(0);

    const status = getInvoiceStatus(db, issued.documentId!, "2026-07-01");
    expect(status.ok).toBe(true);
    expect(status.openBalance).toBe(0);
    expect(status.status).toBe("written_off");
    expect(status.totalBadDebtWrittenOff).toBe(1250);
    expect(status.badDebtWriteOffs).toHaveLength(1);

    const lines = db.query(
      `SELECT a.account_no, jl.debit_amount, jl.credit_amount, jl.vat_code
       FROM journal_lines jl JOIN accounts a ON a.id = jl.account_id
       WHERE jl.journal_entry_id = ? ORDER BY jl.id ASC`
    ).all(writeOff.entryId!) as any[];
    expect(lines).toEqual([
      { account_no: "3080", debit_amount: 1000, credit_amount: 0, vat_code: "DK_BAD_DEBT_25" },
      { account_no: "1200", debit_amount: 250, credit_amount: 0, vat_code: null },
      { account_no: "1100", debit_amount: 0, credit_amount: 1250, vat_code: null },
    ]);

    const vat = buildVatReport(db, "2026-05-01", "2026-07-31");
    expect(vat.ok).toBe(true);
    expect(vat.outputVat).toBe(0);
    expect(vat.badDebtReliefBase25).toBe(1000);

    expect(verifyAuditChain(db).ok).toBe(true);
    db.close();
    cleanupDir(root);
  });

  test("writes off a non-DKK invoice using stored DKK totals for ledger relief", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-bad-debt-fx-"));
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
      buyer: { name: "Kunde GmbH", address: "Berlin", vatOrCvr: "DE123456789" },
      lines: [{ description: "Consulting", quantity: 1, unitPriceExVat: 100, lineTotalExVat: 100 }],
      totals: { netAmount: 100, vatRate: 0.25, vatAmount: 25, grossAmount: 125, fxRateToDkk: 7.46, netAmountDkk: 746, vatAmountDkk: 186.5, grossAmountDkk: 932.5 },
      currency: "EUR"
    });
    expect(issued.ok).toBe(true);
    expect(postIssuedInvoiceToLedger(db, { invoiceDocumentId: issued.documentId! }).ok).toBe(true);

    const writeOff = writeOffInvoiceBadDebt(db, {
      invoiceDocumentId: issued.documentId!,
      writeOffDate: "2026-07-01",
    });
    expect(writeOff.ok).toBe(true);
    expect(writeOff.grossAmount).toBe(125);
    expect(writeOff.netAmount).toBe(100);
    expect(writeOff.vatAmount).toBe(25);

    const entry = db.query("SELECT currency, amount_foreign, amount_dkk, fx_rate_to_dkk FROM journal_entries WHERE id = ?").get(writeOff.entryId!) as any;
    expect(entry).toEqual({ currency: "EUR", amount_foreign: 125, amount_dkk: 932.5, fx_rate_to_dkk: 7.46 });

    const lines = db.query(
      `SELECT a.account_no, jl.debit_amount, jl.credit_amount, jl.vat_code
       FROM journal_lines jl JOIN accounts a ON a.id = jl.account_id
       WHERE jl.journal_entry_id = ? ORDER BY jl.id ASC`
    ).all(writeOff.entryId!) as any[];
    expect(lines).toEqual([
      { account_no: "3080", debit_amount: 746, credit_amount: 0, vat_code: "DK_BAD_DEBT_25" },
      { account_no: "1200", debit_amount: 186.5, credit_amount: 0, vat_code: null },
      { account_no: "1100", debit_amount: 0, credit_amount: 932.5, vat_code: null },
    ]);

    const status = getInvoiceStatus(db, issued.documentId!, "2026-07-01");
    expect(status.openBalance).toBe(0);
    expect(status.status).toBe("written_off");
    expect(status.totalBadDebtWrittenOff).toBe(125);

    expect(verifyAuditChain(db).ok).toBe(true);
    db.close();
    cleanupDir(root);
  });

  test("blocks bad-debt write-off above open principal balance", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-bad-debt-over-"));
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

    const writeOff = writeOffInvoiceBadDebt(db, {
      invoiceDocumentId: issued.documentId!,
      writeOffDate: "2026-07-01",
      grossAmount: 1300,
    });
    expect(writeOff.ok).toBe(false);
    expect(writeOff.errors[0]).toContain("exceeds open principal balance");

    db.close();
    cleanupDir(root);
  });
});
