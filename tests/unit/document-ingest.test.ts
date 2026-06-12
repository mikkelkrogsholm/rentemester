// Tests: src/core/documents.ts (document ingestion)
import { describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync, existsSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { ingestDocument, validateDocumentMetadata } from "../../src/core/documents";

import { cleanupDir } from "../helpers/cleanup";
describe("document ingest", () => {
  test("rejects purchase/sale document metadata missing statutory fields", () => {
    const result = validateDocumentMetadata({
      source: "email",
      issueDate: "2026-05-16",
      amountIncVat: 1250,
      currency: "DKK",
    });

    expect(result.ok).toBe(false);
    expect(result.errors).toContain("deliveryDescription is required");
    expect(result.errors).not.toContain("paymentDetails is required");
  });

  test("accepts purchase/sale metadata without payment details", () => {
    const result = validateDocumentMetadata({
      source: "email",
      issueDate: "2026-05-16",
      invoiceNo: "INV-1001",
      deliveryDescription: "Kontorartikler",
      amountIncVat: 125,
      currency: "DKK",
      sender: { name: "Leverandør ApS", address: "Sælgervej 1", vatOrCvr: "DK11223344" },
      recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      vatAmount: 25,
    });

    expect(result.ok).toBe(true);
  });

  test("accepts foreign-currency purchase/sale metadata when statutory fields are present", () => {
    const result = validateDocumentMetadata({
      source: "email",
      issueDate: "2026-05-16",
      invoiceNo: "EUR-1001",
      deliveryDescription: "Cloud subscription",
      amountIncVat: 100,
      currency: "EUR",
      sender: { name: "Cloud Vendor GmbH", address: "Berlin", vatOrCvr: "DE123456789" },
      recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      vatAmount: 0,
    });

    expect(result.ok).toBe(true);
  });

  test("accepts foreign-currency cash-register receipts with original currency preserved", () => {
    const companyRoot = mkdtempSync(join(tmpdir(), "rentemester-company-cash-"));
    const inboxRoot = mkdtempSync(join(tmpdir(), "rentemester-inbox-cash-"));
    const sourceFile = join(inboxRoot, "coffee-receipt.txt");
    writeFileSync(sourceFile, "Coffee receipt\n12.00 EUR\n");

    const validation = validateDocumentMetadata({
      source: "photo-upload",
      documentType: "cash_register_receipt",
      currency: "EUR",
    });
    expect(validation.ok).toBe(true);
    expect(validation.appliedRules).toContain("DK-DOCUMENT-CASH-RECEIPT-001");

    const paths = ensureCompanyDirs(companyRoot);
    const db = openDb(paths.db);
    migrate(db);

    const result = ingestDocument(db, companyRoot, sourceFile, {
      source: "photo-upload",
      documentType: "cash_register_receipt",
      currency: "EUR",
    });

    expect(result.ok).toBe(true);
    const row = db.query("SELECT document_type, currency, exemption_code, invoice_date, vat_amount FROM documents WHERE id = ?").get(result.documentId!) as any;
    expect(row.document_type).toBe("cash_register_receipt");
    expect(row.currency).toBe("EUR");
    expect(row.exemption_code).toBeNull();
    expect(row.invoice_date).toBeNull();
    expect(row.vat_amount).toBeNull();

    db.close();
    cleanupDir(companyRoot);
    cleanupDir(inboxRoot);
  });

  test("accepts foreign physical-only receipts outside Denmark with original EUR currency preserved", () => {
    const companyRoot = mkdtempSync(join(tmpdir(), "rentemester-company-foreign-"));
    const inboxRoot = mkdtempSync(join(tmpdir(), "rentemester-inbox-foreign-"));
    const sourceFile = join(inboxRoot, "metro-ticket.txt");
    writeFileSync(sourceFile, "Metro ticket\n8.50 EUR\n");

    const validation = validateDocumentMetadata({
      source: "mobile-scan",
      currency: "EUR",
      exemptionCode: "FOREIGN_PHYSICAL_ONLY",
    });
    expect(validation.ok).toBe(true);
    expect(validation.appliedRules).toContain("DK-DOCUMENT-FOREIGN-PHYSICAL-001");

    const paths = ensureCompanyDirs(companyRoot);
    const db = openDb(paths.db);
    migrate(db);

    const result = ingestDocument(db, companyRoot, sourceFile, {
      source: "mobile-scan",
      currency: "EUR",
      exemptionCode: "FOREIGN_PHYSICAL_ONLY",
    });

    expect(result.ok).toBe(true);
    const row = db.query("SELECT document_type, currency, exemption_code, invoice_date, vat_amount FROM documents WHERE id = ?").get(result.documentId!) as any;
    expect(row.document_type).toBe("purchase_sale");
    expect(row.currency).toBe("EUR");
    expect(row.exemption_code).toBe("FOREIGN_PHYSICAL_ONLY");
    expect(row.invoice_date).toBeNull();
    expect(row.vat_amount).toBeNull();

    db.close();
    cleanupDir(companyRoot);
    cleanupDir(inboxRoot);
  });

  test("numbers ingested documents by metadata year and resets per year", () => {
    const companyRoot = mkdtempSync(join(tmpdir(), "rentemester-company-docyear-"));
    const inboxRoot = mkdtempSync(join(tmpdir(), "rentemester-inbox-docyear-"));
    const firstFile = join(inboxRoot, "vendor-2024.txt");
    const secondFile = join(inboxRoot, "vendor-2025.txt");
    writeFileSync(firstFile, "Invoice 2024\nAmount 1250 DKK\n");
    writeFileSync(secondFile, "Invoice 2025\nAmount 1250 DKK\n");

    const db = openDb(ensureCompanyDirs(companyRoot).db);
    migrate(db);

    const first = ingestDocument(db, companyRoot, firstFile, {
      source: "email",
      issueDate: "2024-12-31",
      invoiceNo: "INV-2024-1",
      deliveryDescription: "Bogføring",
      amountIncVat: 1250,
      currency: "DKK",
      sender: { name: "Leverandør ApS", address: "Sælgervej 1", vatOrCvr: "DK11223344" },
      recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      vatAmount: 250,
    });
    const second = ingestDocument(db, companyRoot, secondFile, {
      source: "email",
      issueDate: "2025-01-01",
      invoiceNo: "INV-2025-1",
      deliveryDescription: "Bogføring",
      amountIncVat: 1250,
      currency: "DKK",
      sender: { name: "Leverandør ApS", address: "Sælgervej 1", vatOrCvr: "DK11223344" },
      recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      vatAmount: 250,
    });

    expect(first.documentNo).toBe("DOC-2024-000001");
    expect(second.documentNo).toBe("DOC-2025-000001");

    db.close();
    cleanupDir(companyRoot);
    cleanupDir(inboxRoot);
  });

  test("uses configured fiscal year labels for document numbers", () => {
    const companyRoot = mkdtempSync(join(tmpdir(), "rentemester-company-docfiscal-"));
    const inboxRoot = mkdtempSync(join(tmpdir(), "rentemester-inbox-docfiscal-"));
    const firstFile = join(inboxRoot, "vendor-2026.txt");
    const secondFile = join(inboxRoot, "vendor-2027.txt");
    writeFileSync(firstFile, "Invoice July 2026\nAmount 1250 DKK\n");
    writeFileSync(secondFile, "Invoice July 2027\nAmount 1250 DKK\n");

    const db = openDb(ensureCompanyDirs(companyRoot).db);
    migrate(db);
    db.run(
      `INSERT INTO companies (id, name, cvr, fiscal_year_start_month, fiscal_year_label_strategy)
       VALUES (1, 'Rentemester ApS', 'DK12345678', 7, 'span')`
    );

    const first = ingestDocument(db, companyRoot, firstFile, {
      source: "email",
      issueDate: "2026-07-15",
      invoiceNo: "INV-2026-7",
      deliveryDescription: "Bogføring",
      amountIncVat: 1250,
      currency: "DKK",
      sender: { name: "Leverandør ApS", address: "Sælgervej 1", vatOrCvr: "DK11223344" },
      recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      vatAmount: 250,
    });
    const second = ingestDocument(db, companyRoot, secondFile, {
      source: "email",
      issueDate: "2027-07-01",
      invoiceNo: "INV-2027-7",
      deliveryDescription: "Bogføring",
      amountIncVat: 1250,
      currency: "DKK",
      sender: { name: "Leverandør ApS", address: "Sælgervej 1", vatOrCvr: "DK11223344" },
      recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      vatAmount: 250,
    });

    expect(first.documentNo).toBe("DOC-2026-27-000001");
    expect(second.documentNo).toBe("DOC-2027-28-000001");

    db.close();
    cleanupDir(companyRoot);
    cleanupDir(inboxRoot);
  });

  test("rejects a file whose bytes contradict its .pdf extension", () => {
    const companyRoot = mkdtempSync(join(tmpdir(), "rentemester-company-mime-"));
    const inboxRoot = mkdtempSync(join(tmpdir(), "rentemester-inbox-mime-"));
    const fakePdf = join(inboxRoot, "invoice.pdf");
    // Plain text bytes, not a PDF — must not be stored as application/pdf.
    writeFileSync(fakePdf, "this is not really a pdf\n");

    const db = openDb(ensureCompanyDirs(companyRoot).db);
    migrate(db);

    const result = ingestDocument(db, companyRoot, fakePdf, {
      source: "email",
      issueDate: "2026-05-16",
      invoiceNo: "INV-FAKE",
      deliveryDescription: "Bogføring",
      amountIncVat: 1250,
      currency: "DKK",
      sender: { name: "Leverandør ApS", address: "Sælgervej 1", vatOrCvr: "DK11223344" },
      recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      vatAmount: 250,
    });

    expect(result.ok).toBe(false);
    expect(result.errors?.[0]).toContain("content does not match");

    db.close();
    cleanupDir(companyRoot);
    cleanupDir(inboxRoot);
  });

  test("ingests a real PDF when the bytes match the .pdf extension", () => {
    const companyRoot = mkdtempSync(join(tmpdir(), "rentemester-company-realpdf-"));
    const inboxRoot = mkdtempSync(join(tmpdir(), "rentemester-inbox-realpdf-"));
    const realPdf = join(inboxRoot, "invoice.pdf");
    writeFileSync(realPdf, "%PDF-1.4\n%minimal pdf body\n");

    const db = openDb(ensureCompanyDirs(companyRoot).db);
    migrate(db);

    const result = ingestDocument(db, companyRoot, realPdf, {
      source: "email",
      issueDate: "2026-05-16",
      invoiceNo: "INV-REAL",
      deliveryDescription: "Bogføring",
      amountIncVat: 1250,
      currency: "DKK",
      sender: { name: "Leverandør ApS", address: "Sælgervej 1", vatOrCvr: "DK11223344" },
      recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      vatAmount: 250,
    });

    expect(result.ok).toBe(true);
    const row = db.query("SELECT mime_type FROM documents WHERE id = ?").get(result.documentId!) as any;
    expect(row.mime_type).toBe("application/pdf");

    db.close();
    cleanupDir(companyRoot);
    cleanupDir(inboxRoot);
  });

  test("ingests a compliant supporting document and blocks duplicate logical supplier invoices unless forced", () => {
    const companyRoot = mkdtempSync(join(tmpdir(), "rentemester-company-"));
    const inboxRoot = mkdtempSync(join(tmpdir(), "rentemester-inbox-"));
    const sourceFile = join(inboxRoot, "vendor-invoice.txt");
    writeFileSync(sourceFile, "Invoice 1001\nAmount 1250 DKK\n");

    const paths = ensureCompanyDirs(companyRoot);
    const db = openDb(paths.db);
    migrate(db);

    const result = ingestDocument(db, companyRoot, sourceFile, {
      source: "email",
      issueDate: "2026-05-16",
      invoiceNo: "INV-1001",
      deliveryDescription: "Bogføring og momsafstemning",
      amountIncVat: 1250,
      currency: "DKK",
      sender: { name: "Leverandør ApS", address: "Sælgervej 1, 2100 København Ø", vatOrCvr: "DK11223344" },
      recipient: { name: "Rentemester ApS", address: "Testvej 1, 2100 København Ø", vatOrCvr: "DK12345678" },
      vatAmount: 250,
    });

    expect(result.ok).toBe(true);
    expect(result.documentNo).toBeDefined();
    expect(existsSync(result.storedPath!)).toBe(true);

    const row = db.query("SELECT document_no, source, invoice_no, amount_inc_vat, vat_amount, payment_details FROM documents WHERE id = ?").get(result.documentId!) as any;
    expect(row.document_no).toBe(result.documentNo);
    expect(row.invoice_no).toBe("INV-1001");
    expect(row.amount_inc_vat).toBe(1250);
    expect(row.vat_amount).toBe(250);
    expect(row.payment_details).toBeNull();

    const dup = ingestDocument(db, companyRoot, sourceFile, {
      source: "email",
      issueDate: "2026-05-16",
      invoiceNo: "INV-1001",
      deliveryDescription: "Bogføring og momsafstemning",
      amountIncVat: 1250,
      currency: "DKK",
      sender: { name: "Leverandør ApS", address: "Sælgervej 1, 2100 København Ø", vatOrCvr: "DK11223344" },
      recipient: { name: "Rentemester ApS", address: "Testvej 1, 2100 København Ø", vatOrCvr: "DK12345678" },
      vatAmount: 250,
    });
    expect(dup.ok).toBe(false);
    expect(dup.errors?.[0]).toContain("duplicate document content already ingested");

    const rescannedFile = join(inboxRoot, "vendor-invoice-rescan.txt");
    writeFileSync(rescannedFile, "Invoice 1001\nAmount 1250 DKK\nrescanned\n");

    const logicalDup = ingestDocument(db, companyRoot, rescannedFile, {
      source: "email-forward",
      issueDate: "2026-05-16",
      invoiceNo: "INV-1001",
      deliveryDescription: "Bogføring og momsafstemning",
      amountIncVat: 1250,
      currency: "DKK",
      sender: { name: "Leverandør ApS", address: "Sælgervej 1, 2100 København Ø", vatOrCvr: "DK11223344" },
      recipient: { name: "Rentemester ApS", address: "Testvej 1, 2100 København Ø", vatOrCvr: "DK12345678" },
      vatAmount: 250,
    });
    expect(logicalDup.ok).toBe(false);
    expect(logicalDup.errors?.[0]).toContain("already ingested as");

    const forcedLogicalDup = ingestDocument(db, companyRoot, rescannedFile, {
      source: "email-forward",
      issueDate: "2026-05-16",
      invoiceNo: "INV-1001",
      deliveryDescription: "Bogføring og momsafstemning",
      amountIncVat: 1250,
      currency: "DKK",
      sender: { name: "Leverandør ApS", address: "Sælgervej 1, 2100 København Ø", vatOrCvr: "DK11223344" },
      recipient: { name: "Rentemester ApS", address: "Testvej 1, 2100 København Ø", vatOrCvr: "DK12345678" },
      vatAmount: 250,
    }, { forceDuplicateLogicalIdentity: true });
    expect(forcedLogicalDup.ok).toBe(true);
    expect(forcedLogicalDup.documentId).toBeDefined();

    db.close();
    cleanupDir(companyRoot);
    cleanupDir(inboxRoot);
  });
});
