// Tests: src/core/bank-suggest-matches.ts (refund / credit-note matching, #182)
import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { seedAccounts } from "../../src/core/ledger";
import { issueInvoice } from "../../src/core/issued-invoices";
import { issueCreditNote } from "../../src/core/credit-notes";
import { ingestDocument } from "../../src/core/documents";
import { importBankCsv } from "../../src/core/bank";
import { suggestBankMatches } from "../../src/core/bank-suggest-matches";

function invoicePayload(overrides: Record<string, unknown> = {}) {
  return {
    invoiceType: "full",
    vatTreatment: "standard",
    issueDate: "2026-05-16",
    invoiceNumber: "2026-0001",
    seller: { name: "Rentemester ApS", address: "Testvej 1, 2100 København Ø", vatOrCvr: "DK12345678" },
    buyer: { name: "Kunde A/S", address: "Købervej 9, 8000 Aarhus C", vatOrCvr: "DK87654321" },
    lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
    totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
    currency: "DKK",
    dueDate: "2026-06-15",
    ...overrides,
  };
}

function setup(prefix: string) {
  const root = mkdtempSync(join(tmpdir(), prefix));
  const db = openDb(ensureCompanyDirs(root).db);
  migrate(db);
  seedAccounts(db);
  return { root, db };
}

describe("cross-currency suggestion guard", () => {
  test("a foreign-currency bank row does not match a same-amount DKK invoice", () => {
    const { root, db } = setup("rentemester-suggest-fx-");
    const invoice = issueInvoice(db, root, invoicePayload({ invoiceNumber: "2026-0001" }));
    expect(invoice.ok).toBe(true);

    // An incoming bank row of 1250 — but in EUR, not DKK. The amount coincides
    // with the DKK invoice gross (1250) and the text names the invoice, so
    // WITHOUT the currency guard equalsDkk(1250, 1250) would surface a confident
    // but unactionable suggestion (the apply path rejects a currency mismatch).
    const csv = join(root, "fx.csv");
    writeFileSync(
      csv,
      [
        "transaction_date,booking_date,text,amount,currency,reference,fx_rate_to_dkk,amount_dkk",
        "2026-05-22,2026-05-22,Betaling 2026-0001 Kunde A/S,1250,EUR,FX-1,7.46,9325",
      ].join("\n"),
    );
    expect(importBankCsv(db, root, csv).ok).toBe(true);

    const result = suggestBankMatches(db, {});
    const row = result.rows.find((r) => r.reference === "FX-1")!;
    expect(row).toBeDefined();
    expect(row.suggestions.some((s) => s.kind === "issued_invoice")).toBe(false);

    db.close();
    rmSync(root, { recursive: true, force: true });
  });
});

describe("refund / credit-note matching (#182)", () => {
  test("an outgoing customer-refund bank row matches its credit note", () => {
    const { root, db } = setup("rentemester-refund-cn-");
    const invoice = issueInvoice(db, root, invoicePayload({ invoiceNumber: "2026-0001" }));
    expect(invoice.ok).toBe(true);

    // Credit note CN-2026-0001 against the invoice — a 1250 credit note.
    const cn = issueCreditNote(db, root, {
      originalInvoiceDocumentId: invoice.documentId!,
      issueDate: "2026-05-20",
      reason: "Returvare",
    });
    expect(cn.ok).toBe(true);

    // The outgoing bank row paying the customer back, naming the credit note.
    const csv = join(root, "tx.csv");
    writeFileSync(csv, [
      "transaction_date,booking_date,text,amount,currency,reference",
      `2026-05-22,2026-05-22,Refusion ${cn.creditNoteNumber} Kunde A/S,-1250,DKK,RFND-1`,
    ].join("\n"));
    expect(importBankCsv(db, root, csv).ok).toBe(true);

    const result = suggestBankMatches(db, {});
    const row = result.rows.find((r) => r.reference === "RFND-1")!;
    expect(row.suggestions.length).toBeGreaterThan(0);
    const top = row.suggestions[0];
    expect(top.kind).toBe("credit_note_refund");
    expect(top.invoiceNo).toBe(cn.creditNoteNumber);
    expect(top.confidence).toBeGreaterThanOrEqual(0.5);

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("an incoming supplier credit-note refund matches its purchase", () => {
    const { root, db } = setup("rentemester-refund-supplier-");
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-refund-inbox-"));
    const sourceFile = join(inbox, "vendor.txt");
    writeFileSync(sourceFile, "Software invoice\n1250 DKK\n");
    const purchase = ingestDocument(db, root, sourceFile, {
      source: "email",
      issueDate: "2026-05-16",
      invoiceNo: "SUP-1001",
      deliveryDescription: "Software subscription",
      amountIncVat: 1250,
      currency: "DKK",
      sender: { name: "Stripe Payments Ltd", address: "1 Market St", vatOrCvr: "DK11223344" },
      recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      vatAmount: 250,
      paymentDetails: "Stripe payout",
    });
    expect(purchase.ok).toBe(true);

    // An INCOMING bank row: the supplier refunds the purchase via a credit
    // note. Names the supplier and the purchase invoice number.
    const csv = join(root, "tx.csv");
    writeFileSync(csv, [
      "transaction_date,booking_date,text,amount,currency,reference",
      "2026-05-25,2026-05-25,Kreditnota refusion SUP-1001 Stripe Payments Ltd,1250,DKK,SUP-RFND-1",
    ].join("\n"));
    expect(importBankCsv(db, root, csv).ok).toBe(true);

    const result = suggestBankMatches(db, {});
    const row = result.rows.find((r) => r.reference === "SUP-RFND-1")!;
    expect(row.suggestions.length).toBeGreaterThan(0);
    const top = row.suggestions[0];
    expect(top.kind).toBe("supplier_credit_refund");
    expect(top.invoiceNo).toBe("SUP-1001");
    expect(top.supplierName).toBe("Stripe Payments Ltd");

    db.close();
    rmSync(root, { recursive: true, force: true });
    rmSync(inbox, { recursive: true, force: true });
  });

  test("a refund with no corroboration (amount only) yields no crossing-threshold suggestion", () => {
    const { root, db } = setup("rentemester-refund-amtonly-");
    const invoice = issueInvoice(db, root, invoicePayload({ invoiceNumber: "2026-0001" }));
    expect(invoice.ok).toBe(true);
    const cn = issueCreditNote(db, root, {
      originalInvoiceDocumentId: invoice.documentId!,
      issueDate: "2026-05-20",
      reason: "Returvare",
    });
    expect(cn.ok).toBe(true);

    // Outgoing row equal to the credit-note gross, but no CN number and no
    // customer-name overlap — an amount-only tie.
    const csv = join(root, "tx.csv");
    writeFileSync(csv, [
      "transaction_date,booking_date,text,amount,currency,reference",
      "2026-05-22,2026-05-22,Udbetaling,-1250,DKK,RFND-X",
    ].join("\n"));
    expect(importBankCsv(db, root, csv).ok).toBe(true);

    const result = suggestBankMatches(db, {});
    const row = result.rows.find((r) => r.reference === "RFND-X")!;
    expect(row.suggestions).toHaveLength(0);

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("a normal incoming customer payment still matches its issued invoice, not a supplier refund", () => {
    const { root, db } = setup("rentemester-refund-regression-");
    const invoice = issueInvoice(db, root, invoicePayload({ invoiceNumber: "2026-0001" }));
    expect(invoice.ok).toBe(true);

    const csv = join(root, "tx.csv");
    writeFileSync(csv, [
      "transaction_date,booking_date,text,amount,currency,reference",
      "2026-05-20,2026-05-20,Customer payment 2026-0001 Kunde A/S,1250,DKK,INV-PAY-1",
    ].join("\n"));
    expect(importBankCsv(db, root, csv).ok).toBe(true);

    const result = suggestBankMatches(db, {});
    const row = result.rows.find((r) => r.reference === "INV-PAY-1")!;
    expect(row.suggestions[0].kind).toBe("issued_invoice");
    expect(row.suggestions[0].invoiceNo).toBe("2026-0001");

    db.close();
    rmSync(root, { recursive: true, force: true });
  });
});
