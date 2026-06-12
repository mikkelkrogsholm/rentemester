// Tests: src/core/issued-invoices.ts
import { describe, expect, test } from "bun:test";
import { existsSync, mkdtempSync, readFileSync, readdirSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs, companyPaths } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { issueInvoice } from "../../src/core/issued-invoices";
import { issueCreditNote } from "../../src/core/credit-notes";
import { storeViesValidation } from "../../src/core/vies";
import { readIssuedInvoicePdfText, renderIssuedInvoicePdf } from "../../src/core/invoice-pdf";

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

describe("invoice issue", () => {
  test("persists the currency in canonical form even when given lower-case", () => {
    // validateInvoice only checks the currency after normalize(), so 'dkk'
    // passes. The persisted documents.currency must be the canonical 'DKK' so
    // downstream raw-string guards (e.g. the reminder flow's DKK check) do not
    // wrongly reject a legitimate DKK invoice.
    const root = mkdtempSync(join(tmpdir(), "rentemester-issue-currency-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);

    const result = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-16",
      invoiceNumber: "2026-0001",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
      currency: "dkk",
    });
    expect(result.errors).toEqual([]);
    expect(result.ok).toBe(true);

    const row = db.query("SELECT currency FROM documents WHERE id = ?").get(result.documentId!) as { currency: string };
    expect(row.currency).toBe("DKK");

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("issues a validated invoice as immutable persisted snapshot", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-issue-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);

    const result = issueInvoice(db, root, {
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

    expect(result.ok).toBe(true);
    expect(result.invoiceNumber).toBe("2026-0001");
    expect(result.appliedRules).toContain("DK-INVOICE-ISSUE-001");
    expect(existsSync(result.storedPath!)).toBe(true);
    expect(existsSync(result.pdfStoredPath!)).toBe(true);

    const stored = JSON.parse(readFileSync(result.storedPath!, "utf8"));
    expect(stored.status).toBe("issued");
    expect(stored.issuedAt).toBeTruthy();
    expect(stored.invoiceNumber).toBe("2026-0001");

    const row = db.query("SELECT document_type, invoice_no, status, payload_json FROM documents WHERE id = ?").get(result.documentId!) as any;
    expect(row.document_type).toBe("issued_invoice");
    expect(row.invoice_no).toBe("2026-0001");
    expect(row.status).toBe("issued");
    expect(JSON.parse(row.payload_json).invoiceNumber).toBe("2026-0001");

    const pdfRow = db.query("SELECT document_type, invoice_no, mime_type, stored_path FROM documents WHERE id = ?").get(result.pdfDocumentId!) as any;
    expect(pdfRow).toEqual({
      document_type: "issued_invoice_pdf",
      invoice_no: "2026-0001",
      mime_type: "application/pdf",
      stored_path: result.pdfStoredPath,
    });
    const pdfText = readIssuedInvoicePdfText(result.pdfStoredPath!);
    expect(pdfText.startsWith("%PDF-")).toBe(true);
    expect(pdfText).toContain("2026-0001");
    expect(pdfText).toContain("DK12345678");
    // #225: customer-facing amounts use Danish number format (1.000,00).
    expect(pdfText).toContain("250,00 DKK");

    const rerender = renderIssuedInvoicePdf(db, root, { invoiceDocumentId: result.documentId! });
    expect(rerender.ok).toBe(true);
    expect(rerender.sha256).toBe(result.pdfSha256);

    expect(() => db.run("UPDATE documents SET status = 'changed' WHERE id = ?", result.documentId!)).toThrow();

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("persists non-DKK issued invoices with deterministic DKK totals in the snapshot payload", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-issue-fx-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);

    const result = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-16",
      invoiceNumber: "2026-0001",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde GmbH", address: "Berlin" },
      lines: [{ description: "Consulting", quantity: 1, unitPriceExVat: 100, lineTotalExVat: 100 }],
      totals: { netAmount: 100, vatRate: 0.25, vatAmount: 25, grossAmount: 125, fxRateToDkk: 7.46, netAmountDkk: 746, vatAmountDkk: 186.5, grossAmountDkk: 932.5 },
      currency: "EUR"
    });

    expect(result.ok).toBe(true);
    const row = db.query("SELECT amount_inc_vat, currency, vat_amount, payload_json FROM documents WHERE id = ?").get(result.documentId!) as any;
    expect(row.amount_inc_vat).toBe(125);
    expect(row.currency).toBe("EUR");
    expect(row.vat_amount).toBe(25);
    expect(JSON.parse(row.payload_json).totals.grossAmountDkk).toBe(932.5);

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("requires cached VIES validation for foreign reverse-charge invoices", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-issue-vies-required-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);

    const missing = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "foreign_reverse_charge",
      issueDate: "2026-05-16",
      invoiceNumber: "2026-0001",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "EU Kunde GmbH", address: "Berlin", vatOrCvr: "DE123456789" },
      lines: [{ description: "EU consulting", quantity: 1, unitPriceExVat: 8000, lineTotalExVat: 8000 }],
      totals: { netAmount: 8000, grossAmount: 8000 },
      reverseChargeBasis: "EU_MOMSDIREKTIV_ART_196",
      reverseChargeNote: "VAT reverse charge — VAT to be accounted by the recipient",
      currency: "DKK"
    });
    expect(missing.ok).toBe(false);
    expect(missing.errors[0]).toContain("VIES lookup not yet performed");

    storeViesValidation(db, {
      vatOrCvr: "DE123456789",
      valid: true,
      name: "EU Kunde GmbH",
      address: "Berlin",
      validatedAt: "2026-05-15T00:00:00.000Z",
      expiresAt: "2026-08-15T00:00:00.000Z",
      rawResponse: JSON.stringify({ valid: true })
    });

    const result = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "foreign_reverse_charge",
      issueDate: "2026-05-16",
      invoiceNumber: "2026-0001",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "EU Kunde GmbH", address: "Berlin", vatOrCvr: "DE123456789" },
      lines: [{ description: "EU consulting", quantity: 1, unitPriceExVat: 8000, lineTotalExVat: 8000 }],
      totals: { netAmount: 8000, grossAmount: 8000 },
      reverseChargeBasis: "EU_MOMSDIREKTIV_ART_196",
      reverseChargeNote: "VAT reverse charge — VAT to be accounted by the recipient",
      deliveryPeriodStart: "2026-05-01",
      deliveryPeriodEnd: "2026-05-15",
      currency: "DKK"
    });

    expect(result.ok).toBe(true);

    const row = db.query("SELECT delivery_description, exemption_code, payload_json FROM documents WHERE id = ?").get(result.documentId!) as any;
    expect(row.delivery_description).toBe("Delivery period 2026-05-01..2026-05-15");
    expect(row.exemption_code).toBe("EU_MOMSDIREKTIV_ART_196");
    expect(JSON.parse(row.payload_json).deliveryPeriodEnd).toBe("2026-05-15");
    expect(JSON.parse(row.payload_json).viesValidation.normalized).toBe("DE123456789");

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("stores structured delivery and reverse-charge basis fields on issued invoices", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-issue-structured-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    storeViesValidation(db, {
      vatOrCvr: "DE123456789",
      valid: true,
      validatedAt: "2026-05-15T00:00:00.000Z",
      expiresAt: "2026-08-15T00:00:00.000Z",
      rawResponse: JSON.stringify({ valid: true })
    });

    const result = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "foreign_reverse_charge",
      issueDate: "2026-05-16",
      invoiceNumber: "2026-0001",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "EU Kunde GmbH", address: "Berlin", vatOrCvr: "DE123456789" },
      lines: [{ description: "EU consulting", quantity: 1, unitPriceExVat: 8000, lineTotalExVat: 8000 }],
      totals: { netAmount: 8000, grossAmount: 8000 },
      reverseChargeBasis: "EU_MOMSDIREKTIV_ART_196",
      reverseChargeNote: "VAT reverse charge — VAT to be accounted by the recipient",
      deliveryPeriodStart: "2026-05-01",
      deliveryPeriodEnd: "2026-05-15",
      currency: "DKK"
    });

    expect(result.ok).toBe(true);

    const row = db.query("SELECT delivery_description, exemption_code, payload_json FROM documents WHERE id = ?").get(result.documentId!) as any;
    expect(row.delivery_description).toBe("Delivery period 2026-05-01..2026-05-15");
    expect(row.exemption_code).toBe("EU_MOMSDIREKTIV_ART_196");
    expect(JSON.parse(row.payload_json).deliveryPeriodEnd).toBe("2026-05-15");

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("auto-generates invoice numbers from issue year and resets per year", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-issue-auto-no-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);

    const first2024 = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2024-12-31",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
      currency: "DKK"
    });
    const second2024 = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2024-01-02",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde B ApS", address: "Købervej 10" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 500, lineTotalExVat: 500 }],
      totals: { netAmount: 500, vatRate: 0.25, vatAmount: 125, grossAmount: 625 },
      currency: "DKK"
    });
    const first2025 = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2025-01-03",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde C ApS", address: "Købervej 11" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 400, lineTotalExVat: 400 }],
      totals: { netAmount: 400, vatRate: 0.25, vatAmount: 100, grossAmount: 500 },
      currency: "DKK"
    });

    expect(first2024.invoiceNumber).toBe("2024-0001");
    expect(second2024.invoiceNumber).toBe("2024-0002");
    expect(first2025.invoiceNumber).toBe("2025-0001");

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("uses configured fiscal year labels for invoice numbers", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-issue-fiscal-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    db.run(
      `INSERT INTO companies (id, name, cvr, fiscal_year_start_month, fiscal_year_label_strategy)
       VALUES (1, 'Rentemester ApS', 'DK12345678', 7, 'end-year')`
    );

    const first = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-07-15",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
      currency: "DKK"
    });
    const second = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2027-06-30",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde B ApS", address: "Købervej 10" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 500, lineTotalExVat: 500 }],
      totals: { netAmount: 500, vatRate: 0.25, vatAmount: 125, grossAmount: 625 },
      currency: "DKK"
    });
    const next = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2027-07-01",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde C ApS", address: "Købervej 11" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 400, lineTotalExVat: 400 }],
      totals: { netAmount: 400, vatRate: 0.25, vatAmount: 100, grossAmount: 500 },
      currency: "DKK"
    });

    expect(first.invoiceNumber).toBe("2027-0001");
    expect(second.invoiceNumber).toBe("2027-0002");
    expect(next.invoiceNumber).toBe("2028-0001");

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  // #251: a manual invoice number that matches the next sequence value is
  // accepted and stored in the canonical four-digit format — regardless of how
  // many digits the caller padded it to. A five-digit `2026-00001` and a
  // four-digit `2026-0001` therefore both persist as `2026-0001`, so the
  // issued series is one unbroken, consistent fortløbende nummer.
  test("canonicalises a matching manual invoice number to the four-digit format", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-issue-manual-seq-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);

    const first = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-16",
      invoiceNumber: "2026-00001",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
      currency: "DKK"
    });
    expect(first.ok).toBe(true);
    // A five-digit manual number is re-padded to the canonical four-digit form.
    expect(first.invoiceNumber).toBe("2026-0001");
    // The persisted snapshot and documents row carry the same canonical number.
    expect(JSON.parse(readFileSync(first.storedPath!, "utf8")).invoiceNumber).toBe("2026-0001");
    const firstRow = db
      .query("SELECT invoice_no FROM documents WHERE id = ?")
      .get(first.documentId!) as { invoice_no: string };
    expect(firstRow.invoice_no).toBe("2026-0001");

    const second = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-17",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde B ApS", address: "Købervej 10" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 500, lineTotalExVat: 500 }],
      totals: { netAmount: 500, vatRate: 0.25, vatAmount: 125, grossAmount: 625 },
      currency: "DKK"
    });
    expect(second.ok).toBe(true);
    expect(second.invoiceNumber).toBe("2026-0002");

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  // #251: `invoice issue` (manual, from example JSON) and `invoice create`
  // (auto-numbered) must produce the identical string for the same sequence
  // value, so the two built-in paths never collide in one ledger.
  test("manual and auto-numbered first invoices resolve to the same canonical number", () => {
    const autoRoot = mkdtempSync(join(tmpdir(), "rentemester-issue-auto-fmt-"));
    const autoDb = openDb(ensureCompanyDirs(autoRoot).db);
    migrate(autoDb);
    const auto = issueInvoice(autoDb, autoRoot, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-16",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
      currency: "DKK"
    });
    autoDb.close();
    rmSync(autoRoot, { recursive: true, force: true });

    const manualRoot = mkdtempSync(join(tmpdir(), "rentemester-issue-manual-fmt-"));
    const manualDb = openDb(ensureCompanyDirs(manualRoot).db);
    migrate(manualDb);
    const manual = issueInvoice(manualDb, manualRoot, {
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
    manualDb.close();
    rmSync(manualRoot, { recursive: true, force: true });

    expect(auto.ok).toBe(true);
    expect(manual.ok).toBe(true);
    expect(auto.invoiceNumber).toBe("2026-0001");
    expect(manual.invoiceNumber).toBe(auto.invoiceNumber);
  });

  test("rejects canonical manual invoice numbers that skip the next sequence value", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-issue-manual-gap-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);

    const result = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-16",
      invoiceNumber: "2026-00099",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
      currency: "DKK"
    });

    expect(result.ok).toBe(false);
    expect(result.errors[0]).toContain("næste fortløbende nummer 2026-0001");

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("routes non-five-digit manual invoice numbers through sequence reservation", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-issue-manual-short-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);

    // 4-digit suffix that skips the next sequence value (floor 0 -> next is 1).
    const gap = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-16",
      invoiceNumber: "2026-0500",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
      currency: "DKK"
    });
    expect(gap.ok).toBe(false);
    expect(gap.errors[0]).toContain("næste fortløbende nummer");

    // 4-digit suffix matching the next sequence value is accepted and stored verbatim.
    const first = issueInvoice(db, root, {
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
    expect(first.ok).toBe(true);
    expect(first.invoiceNumber).toBe("2026-0001");

    // Re-using the same manual number must collide on the reserved sequence value.
    const duplicate = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-16",
      invoiceNumber: "2026-0001",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde B ApS", address: "Købervej 10" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 500, lineTotalExVat: 500 }],
      totals: { netAmount: 500, vatRate: 0.25, vatAmount: 125, grossAmount: 625 },
      currency: "DKK"
    });
    expect(duplicate.ok).toBe(false);

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("rejects manual invoice numbers that are not <scope>-<digits>", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-issue-manual-bad-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);

    const result = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-16",
      invoiceNumber: "INV-A",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
      currency: "DKK"
    });
    expect(result.ok).toBe(false);
    expect(result.errors[0]).toContain("INV-A");

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("rejects canonical manual invoice numbers from the wrong fiscal scope", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-issue-manual-scope-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);

    const result = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-16",
      invoiceNumber: "2099-00001",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
      currency: "DKK"
    });

    expect(result.ok).toBe(false);
    expect(result.errors[0]).toContain("does not match current fiscal scope 2026");

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("does not burn an auto-numbered invoice sequence when insert fails", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-issue-auto-rollback-"));
    const realDb = openDb(ensureCompanyDirs(root).db);
    migrate(realDb);
    const failingDb = failingDocumentInsertDb(realDb);

    expect(() => issueInvoice(failingDb, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-16",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
      currency: "DKK"
    })).toThrow("simulated insert failure");

    const sequence = realDb.query("SELECT value FROM sequences WHERE kind = 'issued_invoice' AND scope = 'company-1:2026'").get() as { value: number } | null;
    expect(sequence).toBeNull();

    const retried = issueInvoice(realDb, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-16",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
      currency: "DKK"
    });

    expect(retried.ok).toBe(true);
    expect(retried.invoiceNumber).toBe("2026-0001");

    realDb.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("does not leave an orphan invoice file when document insert fails", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-issue-fail-"));
    const realDb = openDb(ensureCompanyDirs(root).db);
    migrate(realDb);
    const failingDb = failingDocumentInsertDb(realDb);

    expect(() => issueInvoice(failingDb, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-16",
      invoiceNumber: "2026-0001",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
      currency: "DKK"
    })).toThrow("simulated insert failure");

    expect(readdirSync(companyPaths(root).invoicesIssued)).toEqual([]);

    realDb.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("does not leave an orphan credit-note file when document insert fails", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-credit-fail-"));
    const realDb = openDb(ensureCompanyDirs(root).db);
    migrate(realDb);

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
    const result = issueCreditNote(failingDb, root, {
      originalInvoiceDocumentId: issued.documentId!,
      issueDate: "2026-05-17",
      creditNoteNumber: "CN-2026-0001",
      reason: "Test rollback",
    });

    expect(result.ok).toBe(false);
    expect(result.errors[0]).toContain("simulated insert failure");
    expect(readdirSync(companyPaths(root).invoicesIssued).sort()).toEqual(["2026-0001.json", "2026-0001.pdf"].sort());

    realDb.close();
    rmSync(root, { recursive: true, force: true });
  });
});
