// Tests: src/core/invoice-booking.ts, src/core/credit-notes.ts, src/core/vat.ts,
//        src/core/vat-filing.ts, src/core/vat-vies-list.ts
//
// JUR-2/KODE-2: domestic reverse charge (momsloven §46, e.g. mobile phones,
// CPUs, metal scrap) must NOT land in rubrik B (EU sales, cross-checked against
// the VIES list). It belongs in rubrik C ("værdi af andet salg uden moms").
// Only FOREIGN reverse charge (EU B2B) belongs in rubrik B and on the VIES list.
import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { issueInvoice } from "../../src/core/issued-invoices";
import { postIssuedInvoiceToLedger } from "../../src/core/invoice-booking";
import { issueCreditNote } from "../../src/core/credit-notes";
import { seedAccounts, verifyAuditChain } from "../../src/core/ledger";
import { buildVatReport } from "../../src/core/vat";
import { buildVatFiling } from "../../src/core/vat-filing";
import { recordVatFilingEvidence } from "../../src/core/vat-filing-evidence";
import { buildViesRecapitulativeStatement } from "../../src/core/vat-vies-list";
import { closeAccountingPeriod } from "../helpers/close-period";
import { storeViesValidation } from "../../src/core/vies";

function newDb() {
  const root = mkdtempSync(join(tmpdir(), "rentemester-domestic-rc-"));
  const db = openDb(ensureCompanyDirs(root).db);
  migrate(db);
  seedAccounts(db);
  return { root, db };
}

function cacheVies(db: ReturnType<typeof openDb>) {
  storeViesValidation(db, {
    vatOrCvr: "DE123456789",
    valid: true,
    rawResponse: JSON.stringify({ valid: true }),
  });
}

function issueDomestic(db: ReturnType<typeof openDb>, root: string, invoiceNumber: string, net: number) {
  return issueInvoice(db, root, {
    invoiceType: "full",
    vatTreatment: "domestic_reverse_charge",
    reverseChargeBasis: "DK_MOMSLOVEN_§46_STK_1_NR_6",
    reverseChargeNote: "Indenlandsk omvendt betalingspligt",
    issueDate: "2026-02-16",
    invoiceNumber,
    seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
    buyer: { name: "Dansk Kunde ApS", address: "København", vatOrCvr: "DK87654321" },
    lines: [{ description: "Mobiltelefoner", quantity: 1, unitPriceExVat: net, lineTotalExVat: net }],
    totals: { netAmount: net, grossAmount: net },
    currency: "DKK",
  });
}

function issueForeign(db: ReturnType<typeof openDb>, root: string, invoiceNumber: string, net: number) {
  return issueInvoice(db, root, {
    invoiceType: "full",
    vatTreatment: "foreign_reverse_charge",
    reverseChargeBasis: "EU_MOMSDIREKTIV_ART_196",
    reverseChargeNote: "Reverse charge",
    issueDate: "2026-02-16",
    invoiceNumber,
    seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
    buyer: { name: "Kunde GmbH", address: "Berlin", vatOrCvr: "DE123456789" },
    lines: [{ description: "Consulting", quantity: 1, unitPriceExVat: net, lineTotalExVat: net }],
    totals: { netAmount: net, grossAmount: net },
    currency: "DKK",
  });
}

function closeQuarter(db: ReturnType<typeof openDb>) {
  const closed = closeAccountingPeriod(db, {
    kind: "vat_quarter",
    periodStart: "2026-01-01",
    periodEnd: "2026-03-31",
  });
  expect(closed.ok).toBe(true);
}

describe("domestic vs foreign reverse-charge VAT rubrik placement (JUR-2/KODE-2)", () => {
  test("domestic reverse-charge sale books DOMESTIC_REVERSE_CHARGE_EXEMPT, lands in rubrik C not B, and stays off the VIES list", () => {
    const { root, db } = newDb();

    const issued = issueDomestic(db, root, "2026-0001", 1000);
    expect(issued.ok).toBe(true);

    const posted = postIssuedInvoiceToLedger(db, { invoiceDocumentId: issued.documentId! });
    expect(posted.ok).toBe(true);

    const lines = db.query(
      `SELECT a.account_no, jl.debit_amount, jl.credit_amount, jl.vat_code
       FROM journal_lines jl JOIN accounts a ON a.id = jl.account_id
       WHERE jl.journal_entry_id = ? ORDER BY jl.id ASC`,
    ).all(posted.entryId!) as any[];
    expect(lines).toEqual([
      { account_no: "1100", debit_amount: 1000, credit_amount: 0, vat_code: null },
      { account_no: "1000", debit_amount: 0, credit_amount: 1000, vat_code: "DOMESTIC_REVERSE_CHARGE_EXEMPT" },
    ]);

    const report = buildVatReport(db, "2026-01-01", "2026-03-31");
    expect(report.ok).toBe(true);
    expect(report.domesticReverseChargeSalesBase).toBe(1000);
    expect(report.foreignReverseChargeSalesBase).toBe(0);

    closeQuarter(db);
    const filing = buildVatFiling(db, "2026-01-01", "2026-03-31");
    expect(filing.ok).toBe(true);
    // Domestic §46 -> rubrik C, NOT rubrik B.
    expect(filing.rubrikker.rubrikBVarerEuSalesList).toBe(0);
    expect(filing.rubrikker.rubrikC).toBe(1000);
    // Momsfri salg: påvirker ikke tilsvaret.
    expect(filing.rubrikker.momsIAlt).toBe(report.netVatPayable);

    const vies = buildViesRecapitulativeStatement(db, "2026-01-01", "2026-03-31");
    expect(vies.ok).toBe(true);
    expect(vies.invoiceCount).toBe(0);
    expect(vies.customers).toEqual([]);

    expect(verifyAuditChain(db).ok).toBe(true);
    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("foreign reverse-charge sale stays in rubrik B and on the VIES list (regression)", () => {
    const { root, db } = newDb();
    cacheVies(db);

    const issued = issueForeign(db, root, "2026-0001", 2000);
    expect(issued.ok).toBe(true);
    const posted = postIssuedInvoiceToLedger(db, { invoiceDocumentId: issued.documentId! });
    expect(posted.ok).toBe(true);

    const report = buildVatReport(db, "2026-01-01", "2026-03-31");
    expect(report.foreignReverseChargeSalesBase).toBe(2000);
    expect(report.domesticReverseChargeSalesBase).toBe(0);

    closeQuarter(db);
    expect(buildVatFiling(db, "2026-01-01", "2026-03-31")).toMatchObject({ ok: false, errors: [expect.stringContaining("classification")] });
    expect(recordVatFilingEvidence(db, { periodStart: "2026-01-01", periodEnd: "2026-03-31", fieldName: "rubrikBVarerEuSalesList", amountDkk: 2000, evidenceRef: "synthetic EU sales-list evidence", actor: "agent:test", principal: "user:test", confirm: true }).ok).toBe(true);
    const filing = buildVatFiling(db, "2026-01-01", "2026-03-31");
    expect(filing.ok).toBe(true);
    expect(filing.rubrikker.rubrikBVarerEuSalesList).toBe(2000);
    expect(filing.rubrikker.rubrikC).toBe(0);
    expect(filing.rubrikker.momsIAlt).toBe(report.netVatPayable);

    const vies = buildViesRecapitulativeStatement(db, "2026-01-01", "2026-03-31");
    expect(vies.invoiceCount).toBe(1);
    expect(vies.customers).toHaveLength(1);
    expect(vies.customers[0]).toMatchObject({ vatNumber: "DE123456789", totalValue: 2000 });

    expect(verifyAuditChain(db).ok).toBe(true);
    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("EU-sales-list evidence cannot be hidden in the non-list B-goods field", () => {
    const { root, db } = newDb();
    cacheVies(db);
    const issued = issueForeign(db, root, "2026-0001", 2000);
    expect(issued.ok).toBe(true);
    expect(postIssuedInvoiceToLedger(db, { invoiceDocumentId: issued.documentId! }).ok).toBe(true);
    closeQuarter(db);

    expect(recordVatFilingEvidence(db, {
      periodStart: "2026-01-01", periodEnd: "2026-03-31",
      fieldName: "rubrikBVarerIkkeEuSalesList", amountDkk: 2000,
      evidenceRef: "synthetic non-list classification", actor: "agent:test", principal: "user:test", confirm: true,
    }).ok).toBe(true);

    expect(buildVatFiling(db, "2026-01-01", "2026-03-31")).toMatchObject({
      ok: false,
      errors: [expect.stringContaining("EU-sales-list total")],
    });
    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("mixed period splits correctly: domestic -> rubrik C, foreign -> rubrik B + VIES; momstilsvar == netVatPayable", () => {
    const { root, db } = newDb();
    cacheVies(db);

    const dom = issueDomestic(db, root, "2026-0001", 1000);
    expect(dom.ok).toBe(true);
    expect(postIssuedInvoiceToLedger(db, { invoiceDocumentId: dom.documentId! }).ok).toBe(true);

    const for1 = issueForeign(db, root, "2026-0002", 3000);
    expect(for1.ok).toBe(true);
    expect(postIssuedInvoiceToLedger(db, { invoiceDocumentId: for1.documentId! }).ok).toBe(true);

    const report = buildVatReport(db, "2026-01-01", "2026-03-31");
    expect(report.domesticReverseChargeSalesBase).toBe(1000);
    expect(report.foreignReverseChargeSalesBase).toBe(3000);
    // Backwards-compatible combined base = sum of the two.
    expect(report.reverseChargeSalesBase).toBe(4000);

    closeQuarter(db);
    expect(recordVatFilingEvidence(db, { periodStart: "2026-01-01", periodEnd: "2026-03-31", fieldName: "rubrikBVarerEuSalesList", amountDkk: 3000, evidenceRef: "synthetic EU sales-list evidence", actor: "agent:test", principal: "user:test", confirm: true }).ok).toBe(true);
    const filing = buildVatFiling(db, "2026-01-01", "2026-03-31");
    expect(filing.ok).toBe(true);
    expect(filing.rubrikker.rubrikBVarerEuSalesList).toBe(3000);
    expect(filing.rubrikker.rubrikC).toBe(1000);
    expect(filing.rubrikker.momsIAlt).toBe(report.netVatPayable);
    expect(filing.rubrikker.momsIAlt).toBe(0);

    const vies = buildViesRecapitulativeStatement(db, "2026-01-01", "2026-03-31");
    expect(vies.invoiceCount).toBe(1);
    expect(vies.totalValue).toBe(3000);

    expect(verifyAuditChain(db).ok).toBe(true);
    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("credit note for a domestic reverse-charge sale reverses with DOMESTIC_REVERSE_CHARGE_EXEMPT and reduces rubrik C", () => {
    const { root, db } = newDb();

    const issued = issueDomestic(db, root, "2026-0001", 1000);
    expect(issued.ok).toBe(true);
    expect(postIssuedInvoiceToLedger(db, { invoiceDocumentId: issued.documentId! }).ok).toBe(true);

    const credit = issueCreditNote(db, root, {
      originalInvoiceDocumentId: issued.documentId!,
      issueDate: "2026-02-17",
      reason: "Cancel domestic reverse-charge invoice",
    });
    expect(credit.ok).toBe(true);
    expect(credit.appliedRules).toContain("DK-INVOICE-BOOKKEEPING-REVERSE-002");

    const lines = db.query(
      `SELECT a.account_no, jl.debit_amount, jl.credit_amount, jl.vat_code
       FROM journal_lines jl JOIN accounts a ON a.id = jl.account_id
       WHERE jl.journal_entry_id = ? ORDER BY jl.id ASC`,
    ).all(credit.journalEntryId!) as any[];
    // Original invoice was posted, so the credit note reverses its booked lines
    // verbatim (receivable then revenue), carrying the same vat_code.
    expect(lines).toEqual([
      { account_no: "1100", debit_amount: 0, credit_amount: 1000, vat_code: null },
      { account_no: "1000", debit_amount: 1000, credit_amount: 0, vat_code: "DOMESTIC_REVERSE_CHARGE_EXEMPT" },
    ]);

    // Credit note offsets the sale: domestic base nets to 0, still nothing in
    // rubrik B and nothing on the VIES list.
    const report = buildVatReport(db, "2026-01-01", "2026-03-31");
    expect(report.domesticReverseChargeSalesBase).toBe(0);
    expect(report.foreignReverseChargeSalesBase).toBe(0);

    expect(verifyAuditChain(db).ok).toBe(true);
    db.close();
    rmSync(root, { recursive: true, force: true });
  });
});
