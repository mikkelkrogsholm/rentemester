// Tests: src/core/documents.ts (document ingestion)
import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync, existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { ingestDocument, validateDocumentMetadata } from "../../src/core/documents";

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

  test("#529 ingests and reads a US SaaS supplier without an EU VAT or CVR identifier", () => {
    const companyRoot = mkdtempSync(join(tmpdir(), "rentemester-us-saas-"));
    const inboxRoot = mkdtempSync(join(tmpdir(), "rentemester-us-saas-inbox-"));
    const sourceFile = join(inboxRoot, "us-saas.txt");
    writeFileSync(sourceFile, "US SaaS invoice\n100 USD\n");
    const db = openDb(ensureCompanyDirs(companyRoot).db);
    migrate(db);
    const metadata = {
      source: "email", issueDate: "2026-07-18", invoiceNo: "US-529", deliveryDescription: "US SaaS subscription", amountIncVat: 100, currency: "USD",
      sender: { name: "US SaaS Inc.", address: "New York, US", countryCode: "US", identifierKind: "non_eu" as const },
      recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" }, vatAmount: 0,
    };
    const result = ingestDocument(db, companyRoot, sourceFile, metadata);
    expect(result.ok).toBe(true);
    expect(db.query("SELECT supplier_country_code, supplier_identifier_kind, sender_vat_cvr, supplier_identity_status FROM documents WHERE id = ?").get(result.documentId!) as object)
      .toEqual({ supplier_country_code: "US", supplier_identifier_kind: "non_eu", sender_vat_cvr: null, supplier_identity_status: "resolved" });
    const rescan = join(inboxRoot, "us-saas-rescan.txt");
    writeFileSync(rescan, "US SaaS invoice rescanned\n100 USD\n");
    expect(ingestDocument(db, companyRoot, rescan, metadata).errors?.[0]).toContain("US:US SaaS Inc.");
    const enrichedScan = join(inboxRoot, "us-saas-enriched.txt");
    writeFileSync(enrichedScan, "US SaaS invoice enriched with EIN\n100 USD\n");
    expect(ingestDocument(db, companyRoot, enrichedScan, {
      ...metadata,
      sender: { ...metadata.sender, vatOrCvr: "US-EIN-12-3456789" },
    }).errors?.[0]).toContain("US-EIN-12-3456789");

    const enrichedFirstFile = join(inboxRoot, "us-saas-enriched-first.txt");
    writeFileSync(enrichedFirstFile, "US SaaS enriched-first invoice\n200 USD\n");
    const enrichedFirst = {
      ...metadata,
      invoiceNo: "US-529-ENRICHED-FIRST",
      amountIncVat: 200,
      sender: { ...metadata.sender, vatOrCvr: "US-EIN-98-7654321" },
    };
    expect(ingestDocument(db, companyRoot, enrichedFirstFile, enrichedFirst).ok).toBe(true);
    const identityRemovedScan = join(inboxRoot, "us-saas-identity-removed.txt");
    writeFileSync(identityRemovedScan, "US SaaS enriched-first invoice rescanned without EIN\n200 USD\n");
    expect(ingestDocument(db, companyRoot, identityRemovedScan, {
      ...enrichedFirst,
      sender: metadata.sender,
    }).errors?.[0]).toContain("US:US SaaS Inc.");
    expect(ingestDocument(db, companyRoot, rescan, metadata, { forceDuplicateLogicalIdentity: true }).ok).toBe(true);
    db.close(); rmSync(companyRoot, { recursive: true, force: true }); rmSync(inboxRoot, { recursive: true, force: true });
  });

  test("#529 fails closed for contradictory or missing supplier country evidence", () => {
    const base = { source: "email", issueDate: "2026-07-18", deliveryDescription: "SaaS", amountIncVat: 100, sender: { name: "Supplier", address: "Address", vatOrCvr: "DE123456789", countryCode: "US", identifierKind: "eu_vat" as const }, recipient: { name: "Buyer", address: "Address", vatOrCvr: "DK12345678" }, vatAmount: 0 };
    expect(validateDocumentMetadata(base).errors.join(" ")).toContain("human_resolution");
    expect(validateDocumentMetadata({ ...base, sender: { name: "Supplier", address: "Address", vatOrCvr: "unverified foreign text" } }).errors.join(" ")).toContain("human resolution");
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
    rmSync(companyRoot, { recursive: true, force: true });
    rmSync(inboxRoot, { recursive: true, force: true });
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
    rmSync(companyRoot, { recursive: true, force: true });
    rmSync(inboxRoot, { recursive: true, force: true });
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
    rmSync(companyRoot, { recursive: true, force: true });
    rmSync(inboxRoot, { recursive: true, force: true });
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
    rmSync(companyRoot, { recursive: true, force: true });
    rmSync(inboxRoot, { recursive: true, force: true });
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
    rmSync(companyRoot, { recursive: true, force: true });
    rmSync(inboxRoot, { recursive: true, force: true });
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
    rmSync(companyRoot, { recursive: true, force: true });
    rmSync(inboxRoot, { recursive: true, force: true });
  });

  describe("#566 — attachmentless EML as immutable evidence", () => {
    const EML = [
      "From: bog@leverandoer.dk",
      "To: konto@firma.dk",
      "Subject: Ordrebekraeftigelse ORD-77341",
      "Date: Sat, 16 May 2026 10:00:00 +0200",
      "Message-ID: <ord-77341@leverandoer.dk>",
      "MIME-Version: 1.0",
      "Content-Type: text/html; charset=utf-8",
      "",
      "<html><body><p>Ordre ORD-77341, i alt 1250,00 DKK incl. 250,00 moms.</p></body></html>",
      "",
    ].join("\r\n");
    const metadata = {
      source: "email",
      issueDate: "2026-05-16",
      invoiceNo: "ORD-77341",
      deliveryDescription: "Kontorartikler",
      amountIncVat: 1250,
      currency: "DKK",
      sender: { name: "Leverandør ApS", address: "Sælgervej 1", vatOrCvr: "DK11223344" },
      recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      vatAmount: 250,
    };

    test("ingests an attachmentless EML as an immutable evidence document", () => {
      const companyRoot = mkdtempSync(join(tmpdir(), "rentemester-company-eml-"));
      const inboxRoot = mkdtempSync(join(tmpdir(), "rentemester-inbox-eml-"));
      const emlFile = join(inboxRoot, "order-confirmation.eml");
      writeFileSync(emlFile, EML);

      const db = openDb(ensureCompanyDirs(companyRoot).db);
      migrate(db);

      const result = ingestDocument(db, companyRoot, emlFile, metadata);
      expect(result.ok).toBe(true);
      const row = db.query("SELECT mime_type, sha256_hash, stored_path FROM documents WHERE id = ?").get(result.documentId!) as any;
      expect(row.mime_type).toBe("message/rfc822");
      expect(row.sha256_hash).toBe(new Bun.CryptoHasher("sha256").update(EML).digest("hex"));
      expect(existsSync(row.stored_path)).toBe(true);
      expect(readFileSync(row.stored_path, "utf8")).toBe(EML);

      db.close();
      rmSync(companyRoot, { recursive: true, force: true });
      rmSync(inboxRoot, { recursive: true, force: true });
    });

    test("rejects duplicate intake of the same EML bytes", () => {
      const companyRoot = mkdtempSync(join(tmpdir(), "rentemester-company-emldup-"));
      const inboxRoot = mkdtempSync(join(tmpdir(), "rentemester-inbox-emldup-"));
      const emlFile = join(inboxRoot, "order-confirmation.eml");
      writeFileSync(emlFile, EML);

      const db = openDb(ensureCompanyDirs(companyRoot).db);
      migrate(db);

      const first = ingestDocument(db, companyRoot, emlFile, metadata);
      const second = ingestDocument(db, companyRoot, emlFile, metadata);
      expect(first.ok).toBe(true);
      expect(second.ok).toBe(false);
      expect(second.errors?.[0]).toContain("duplicate");

      db.close();
      rmSync(companyRoot, { recursive: true, force: true });
      rmSync(inboxRoot, { recursive: true, force: true });
    });

    test("rejects an altered EML with the same supplier and invoice identity", () => {
      const companyRoot = mkdtempSync(join(tmpdir(), "rentemester-company-emlmut-"));
      const inboxRoot = mkdtempSync(join(tmpdir(), "rentemester-inbox-emlmut-"));
      const original = join(inboxRoot, "original.eml");
      const altered = join(inboxRoot, "altered.eml");
      writeFileSync(original, EML);
      writeFileSync(altered, EML.replace("1250,00 DKK", "990,00 DKK"));

      const db = openDb(ensureCompanyDirs(companyRoot).db);
      migrate(db);

      const first = ingestDocument(db, companyRoot, original, metadata);
      const second = ingestDocument(db, companyRoot, altered, metadata);
      expect(first.ok).toBe(true);
      expect(second.ok).toBe(false);
      expect(second.errors?.[0]).toContain("already ingested");

      db.close();
      rmSync(companyRoot, { recursive: true, force: true });
      rmSync(inboxRoot, { recursive: true, force: true });
    });

    test("rejects a text file masquerading as .eml without an RFC 5322 header block", () => {
      const companyRoot = mkdtempSync(join(tmpdir(), "rentemester-company-emlfake-"));
      const inboxRoot = mkdtempSync(join(tmpdir(), "rentemester-inbox-emlfake-"));
      const fakeEml = join(inboxRoot, "fake.eml");
      writeFileSync(fakeEml, "not an email, just plain text with a\nfloating colon :\n");

      const db = openDb(ensureCompanyDirs(companyRoot).db);
      migrate(db);

      const result = ingestDocument(db, companyRoot, fakeEml, metadata);
      expect(result.ok).toBe(false);
      expect(result.errors?.[0]).toContain("does not look like an RFC 5322");

      db.close();
      rmSync(companyRoot, { recursive: true, force: true });
      rmSync(inboxRoot, { recursive: true, force: true });
    });
  });

  describe("#566 slice 3 — deterministic EML evidence rendering", () => {
    const EML = [
      "From: bog@leverandoer.dk",
      "To: konto@firma.dk",
      "Subject: Ordrebekraeftigelse ORD-88241",
      "Date: Sat, 16 May 2026 10:00:00 +0200",
      "Message-ID: <ord-88241@leverandoer.dk>",
      "MIME-Version: 1.0",
      "Content-Type: text/plain; charset=utf-8",
      "",
      "Ordre ORD-88241, i alt 1250,00 DKK incl. 250,00 moms.",
      "",
    ].join("\r\n");
    const metadata = {
      source: "email",
      issueDate: "2026-05-16",
      invoiceNo: "ORD-88241",
      deliveryDescription: "Kontorartikler",
      amountIncVat: 1250,
      currency: "DKK",
      sender: { name: "Leverandør ApS", address: "Sælgervej 1", vatOrCvr: "DK11223344" },
      recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      vatAmount: 250,
    };

    function ingestedEmlRendering(emlText: string, tag: string): { storedPath: string; sha256: string; rendering: string } {
      const companyRoot = mkdtempSync(join(tmpdir(), `rentemester-company-${tag}-`));
      const inboxRoot = mkdtempSync(join(tmpdir(), `rentemester-inbox-${tag}-`));
      const emlFile = join(inboxRoot, "receipt.eml");
      writeFileSync(emlFile, emlText);
      const db = openDb(ensureCompanyDirs(companyRoot).db);
      migrate(db);
      const result = ingestDocument(db, companyRoot, emlFile, metadata);
      expect(result.ok).toBe(true);
      const row = db.query("SELECT sha256_hash, stored_path FROM documents WHERE id = ?").get(result.documentId!) as any;
      const renderingPath = `${row.stored_path}.rendered.html`;
      expect(existsSync(renderingPath)).toBe(true);
      const rendering = readFileSync(renderingPath, "utf8");
      db.close();
      rmSync(companyRoot, { recursive: true, force: true });
      rmSync(inboxRoot, { recursive: true, force: true });
      return { storedPath: row.stored_path, sha256: row.sha256_hash, rendering };
    }

    test("writes a rendering beside the evidence, derived from the EML itself", () => {
      const { sha256, rendering } = ingestedEmlRendering(EML, "rnd1");
      expect(rendering).toContain(sha256);
      expect(rendering).toContain("Ordrebekraeftigelse ORD-88241");
      expect(rendering).toContain("bog@leverandoer.dk");
      expect(rendering).toContain("Ordre ORD-88241, i alt 1250,00 DKK incl. 250,00 moms.");
    });

    test("rendering is byte-deterministic for identical EML bytes", () => {
      const a = ingestedEmlRendering(EML, "rnd2a");
      const b = ingestedEmlRendering(EML, "rnd2b");
      expect(a.rendering).toBe(b.rendering);
    });

    test("rendering escapes HTML from the message (clearly derived, never executed)", () => {
      const hostile = EML
        .replace("Ordrebekraeftigelse ORD-88241", "Hilsen <script>alert('x')</script>")
        .replace("Ordre ORD-88241", "Ordre <script>alert('x')</script> ORD-88241");
      const { rendering } = ingestedEmlRendering(hostile, "rnd3");
      expect(rendering).not.toContain("<script>");
      expect(rendering).toContain("&lt;script&gt;alert(&#39;x&#39;)&lt;/script&gt;");
    });

    test("a non-EML document gets no rendering", () => {
      const companyRoot = mkdtempSync(join(tmpdir(), "rentemester-company-rnd4-"));
      const inboxRoot = mkdtempSync(join(tmpdir(), "rentemester-inbox-rnd4-"));
      const pdf = join(inboxRoot, "invoice.pdf");
      writeFileSync(pdf, "%PDF-1.4\n%minimal pdf body\n");
      const db = openDb(ensureCompanyDirs(companyRoot).db);
      migrate(db);
      const result = ingestDocument(db, companyRoot, pdf, metadata);
      expect(result.ok).toBe(true);
      const row = db.query("SELECT stored_path FROM documents WHERE id = ?").get(result.documentId!) as any;
      expect(existsSync(`${row.stored_path}.rendered.html`)).toBe(false);
      db.close();
      rmSync(companyRoot, { recursive: true, force: true });
      rmSync(inboxRoot, { recursive: true, force: true });
    });
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
    rmSync(companyRoot, { recursive: true, force: true });
    rmSync(inboxRoot, { recursive: true, force: true });
  });
});
