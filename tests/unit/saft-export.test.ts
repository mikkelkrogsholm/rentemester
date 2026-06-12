// Tests: src/core/saft-export.ts
import { describe, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { issueInvoice } from "../../src/core/issued-invoices";
import { postIssuedInvoiceToLedger } from "../../src/core/invoice-booking";
import { seedAccounts } from "../../src/core/ledger";
import { exportSaftPackage } from "../../src/core/saft-export";
import { createCustomer, createVendor } from "../../src/core/master-data";

function sha256(path: string) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

describe("SAF-T export", () => {
  test("exports a deterministic first-slice SAF-T package with ledger and sales invoices", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-saft-export-"));
    const companyRoot = join(root, "company");
    const exportRoot = join(root, "exports");
    const paths = ensureCompanyDirs(companyRoot);
    const db = openDb(paths.db);
    migrate(db);
    seedAccounts(db);
    db.run("INSERT INTO companies (id, name, country, currency, cvr, fiscal_year_start_month, fiscal_year_label_strategy) VALUES (1, 'Rentemester ApS', 'DK', 'DKK', 'DK12345678', 1, 'end-year')");

    const issued = issueInvoice(db, companyRoot, JSON.parse(readFileSync(join(process.cwd(), "examples/full-invoice.dk.json"), "utf8")));
    expect(issued.ok).toBe(true);
    const posted = postIssuedInvoiceToLedger(db, { invoiceDocumentId: issued.documentId! });
    expect(posted.ok).toBe(true);

    const first = exportSaftPackage(db, companyRoot, {
      periodStart: "2026-05-01",
      periodEnd: "2026-05-31",
      outputDir: exportRoot,
      generatedAt: "2026-05-17T02:24:00.000Z",
    });

    expect(first.ok).toBe(true);
    expect(existsSync(first.manifestPath!)).toBe(true);
    expect(existsSync(first.saftXmlPath!)).toBe(true);

    const second = exportSaftPackage(db, companyRoot, {
      periodStart: "2026-05-01",
      periodEnd: "2026-05-31",
      outputDir: exportRoot,
      generatedAt: "2026-05-17T02:24:00.000Z",
    });

    expect(second.ok).toBe(true);
    expect(second.exportDir).toBe(first.exportDir);
    expect(sha256(first.manifestPath!)).toBe(sha256(second.manifestPath!));
    expect(sha256(first.saftXmlPath!)).toBe(sha256(second.saftXmlPath!));

    const manifest = JSON.parse(readFileSync(first.manifestPath!, "utf8"));
    expect(manifest.packageType).toBe("saft_export");
    expect(manifest.profileId).toBe("rentemester-dk-saft-v3-ledger-sales-purchases-masterfiles");
    expect(manifest.counts.journalEntries).toBe(1);
    expect(manifest.counts.salesInvoices).toBe(1);
    expect(manifest.files.saftXml).toBe("saft.xml");
    expect(manifest.outOfScope).toContain("bank_statement_transport");

    const xml = readFileSync(first.saftXmlPath!, "utf8");
    expect(xml).toContain("<AuditFile");
    expect(xml).toContain("<ProfileID>rentemester-dk-saft-v3-ledger-sales-purchases-masterfiles</ProfileID>");
    expect(xml).toContain("<RegistrationNumber>DK12345678</RegistrationNumber>");
    expect(xml).toContain("<AccountID>1000</AccountID>");
    expect(xml).toContain("<TransactionID>2026-00001</TransactionID>");
    expect(xml).toContain("<InvoiceNo>2026-0001</InvoiceNo>");
    // Quantity is a plain count, not a money value: integer 1 renders as "1",
    // never the 2-decimal "1.00" the øre money formatter would emit.
    expect(xml).toContain("<Quantity>1</Quantity>");
    expect(xml).not.toContain("<Quantity>1.00</Quantity>");

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("fails clearly when required SAF-T source fields are missing", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-saft-export-missing-"));
    const companyRoot = join(root, "company");
    const exportRoot = join(root, "exports");
    const paths = ensureCompanyDirs(companyRoot);
    const db = openDb(paths.db);
    migrate(db);
    seedAccounts(db);
    db.run("INSERT INTO companies (id, name, country, currency) VALUES (1, 'Rentemester ApS', 'DK', 'DKK')");

    const result = exportSaftPackage(db, companyRoot, {
      periodStart: "2026-05-01",
      periodEnd: "2026-05-31",
      outputDir: exportRoot,
      generatedAt: "2026-05-17T02:24:00.000Z",
    });

    expect(result.ok).toBe(false);
    expect(result.errors).toContain("company cvr is required for SAF-T export");

    db.close();
    rmSync(root, { recursive: true, force: true });
  });
});

// ===== Second SAF-T slice: purchase records, VAT summary, document references (#127) =====

function setupCompanyWithSalesAndPurchase() {
  const root = mkdtempSync(join(tmpdir(), "rentemester-saft-export-v2-"));
  const companyRoot = join(root, "company");
  const exportRoot = join(root, "exports");
  const paths = ensureCompanyDirs(companyRoot);
  const db = openDb(paths.db);
  migrate(db);
  seedAccounts(db);
  db.run(
    "INSERT INTO companies (id, name, country, currency, cvr, fiscal_year_start_month, fiscal_year_label_strategy) VALUES (1, 'Rentemester ApS', 'DK', 'DKK', 'DK12345678', 1, 'end-year')",
  );

  const issued = issueInvoice(
    db,
    companyRoot,
    JSON.parse(readFileSync(join(process.cwd(), "examples/full-invoice.dk.json"), "utf8")),
  );
  expect(issued.ok).toBe(true);
  const posted = postIssuedInvoiceToLedger(db, { invoiceDocumentId: issued.documentId! });
  expect(posted.ok).toBe(true);

  return { root, companyRoot, exportRoot, db };
}

// A purchase document is the deterministic SAF-T source for the second slice.
// It mirrors the `documents` row shape produced by ingestDocument for
// document_type = 'purchase_sale' (no file ingestion needed for the export).
function insertPurchaseDocument(
  db: ReturnType<typeof openDb>,
  fields: {
    documentNo: string;
    invoiceNo: string | null;
    invoiceDate: string;
    supplierName: string | null;
    supplierVatCvr: string | null;
    amountIncVat: number | null;
    vatAmount: number | null;
    sha256: string;
  },
) {
  db.run(
    `INSERT INTO documents (
       document_no, source, sha256_hash, document_type, status,
       supplier_name, invoice_no, invoice_date, amount_inc_vat, currency, vat_amount,
       sender_name, sender_vat_cvr, recipient_name, recipient_vat_cvr
     ) VALUES (?, 'email', ?, 'purchase_sale', 'ingested', ?, ?, ?, ?, 'DKK', ?, ?, ?, 'Rentemester ApS', 'DK12345678')`,
    [
      fields.documentNo,
      fields.sha256,
      fields.supplierName,
      fields.invoiceNo,
      fields.invoiceDate,
      fields.amountIncVat,
      fields.vatAmount,
      fields.supplierName,
      fields.supplierVatCvr,
    ],
  );
}

describe("SAF-T export second slice (purchases, VAT summary, document references)", () => {
  test("exports purchase records, a VAT summary, and bumps the profile/version identifier", () => {
    const { root, companyRoot, exportRoot, db } = setupCompanyWithSalesAndPurchase();

    insertPurchaseDocument(db, {
      documentNo: "DOC-2026-000001",
      invoiceNo: "LEV-1001",
      invoiceDate: "2026-05-12",
      supplierName: "Leverandør ApS",
      supplierVatCvr: "DK11223344",
      amountIncVat: 1250,
      vatAmount: 250,
      sha256: "a".repeat(64),
    });

    const result = exportSaftPackage(db, companyRoot, {
      periodStart: "2026-05-01",
      periodEnd: "2026-05-31",
      outputDir: exportRoot,
      generatedAt: "2026-05-17T02:24:00.000Z",
    });

    expect(result.ok).toBe(true);
    expect(result.purchaseInvoiceCount).toBe(1);

    const manifest = JSON.parse(readFileSync(result.manifestPath!, "utf8"));
    expect(manifest.profileId).toBe("rentemester-dk-saft-v3-ledger-sales-purchases-masterfiles");
    expect(manifest.counts.purchaseInvoices).toBe(1);
    expect(manifest.counts.vatSummaryCodes).toBeGreaterThan(0);
    // purchase_documents must no longer be advertised as out of scope
    expect(manifest.outOfScope).not.toContain("purchase_documents");

    const xml = readFileSync(result.saftXmlPath!, "utf8");
    expect(xml).toContain("<ProfileID>rentemester-dk-saft-v3-ledger-sales-purchases-masterfiles</ProfileID>");
    expect(xml).toContain("<AuditFileVersion>3.0</AuditFileVersion>");
    expect(xml).toContain("<PurchaseInvoices>");
    expect(xml).toContain("<InvoiceNo>LEV-1001</InvoiceNo>");
    expect(xml).toContain("<SupplierName>Leverandør ApS</SupplierName>");
    expect(xml).toContain("<SupplierTaxID>DK11223344</SupplierTaxID>");
    // VAT summary section aggregated from deterministic journal-line tax codes
    expect(xml).toContain("<TaxSummary>");
    expect(xml).toContain("<TaxCode>");

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("emits document references linking ledger transactions to source documents", () => {
    const { root, companyRoot, exportRoot, db } = setupCompanyWithSalesAndPurchase();

    const result = exportSaftPackage(db, companyRoot, {
      periodStart: "2026-05-01",
      periodEnd: "2026-05-31",
      outputDir: exportRoot,
      generatedAt: "2026-05-17T02:24:00.000Z",
    });

    expect(result.ok).toBe(true);
    const xml = readFileSync(result.saftXmlPath!, "utf8");
    // the sales invoice posting carries a document_id -> document reference
    expect(xml).toContain("<DocumentReference>");
    expect(xml).toContain("<SourceDocumentID>");

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("export remains byte-stable across reruns with purchase records", () => {
    const { root, companyRoot, exportRoot, db } = setupCompanyWithSalesAndPurchase();

    insertPurchaseDocument(db, {
      documentNo: "DOC-2026-000001",
      invoiceNo: "LEV-1001",
      invoiceDate: "2026-05-12",
      supplierName: "Leverandør ApS",
      supplierVatCvr: "DK11223344",
      amountIncVat: 1250,
      vatAmount: 250,
      sha256: "b".repeat(64),
    });

    const first = exportSaftPackage(db, companyRoot, {
      periodStart: "2026-05-01",
      periodEnd: "2026-05-31",
      outputDir: exportRoot,
      generatedAt: "2026-05-17T02:24:00.000Z",
    });
    const second = exportSaftPackage(db, companyRoot, {
      periodStart: "2026-05-01",
      periodEnd: "2026-05-31",
      outputDir: exportRoot,
      generatedAt: "2026-05-17T02:24:00.000Z",
    });

    expect(first.ok).toBe(true);
    expect(second.ok).toBe(true);
    expect(second.exportDir).toBe(first.exportDir);
    expect(sha256(first.manifestPath!)).toBe(sha256(second.manifestPath!));
    expect(sha256(first.saftXmlPath!)).toBe(sha256(second.saftXmlPath!));

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("fails clearly when a purchase document in the period is missing required SAF-T fields", () => {
    const { root, companyRoot, exportRoot, db } = setupCompanyWithSalesAndPurchase();

    // supplier tax ID and gross amount are required for the purchase profile
    insertPurchaseDocument(db, {
      documentNo: "DOC-2026-000002",
      invoiceNo: "LEV-9999",
      invoiceDate: "2026-05-12",
      supplierName: "Leverandør ApS",
      supplierVatCvr: null,
      amountIncVat: null,
      vatAmount: null,
      sha256: "c".repeat(64),
    });

    const result = exportSaftPackage(db, companyRoot, {
      periodStart: "2026-05-01",
      periodEnd: "2026-05-31",
      outputDir: exportRoot,
      generatedAt: "2026-05-17T02:24:00.000Z",
    });

    expect(result.ok).toBe(false);
    expect(result.errors.some((e) => e.includes("DOC-2026-000002"))).toBe(true);
    expect(result.errors.some((e) => e.includes("supplier"))).toBe(true);

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  // ===== Third slice (master files): customers + suppliers in MasterFiles =====
  test("emits Customers and Suppliers under MasterFiles and counts them in the manifest", () => {
    const { root, companyRoot, exportRoot, db } = setupCompanyWithSalesAndPurchase();
    createCustomer(db, {
      name: "Master Kunde A/S",
      vatOrCvr: "DK11111111",
      email: "kunde@example.com",
      address: "Kundevej 1, 1000 København K",
    });
    createVendor(db, {
      name: "Master Leverandør ApS",
      vatOrCvr: "DK22222222",
      email: "lev@example.com",
      address: "Leverandørvej 2, 8000 Aarhus",
    });

    const result = exportSaftPackage(db, companyRoot, {
      periodStart: "2026-05-01",
      periodEnd: "2026-05-31",
      outputDir: exportRoot,
      generatedAt: "2026-05-17T02:24:00.000Z",
    });
    expect(result.ok).toBe(true);
    expect(result.customerCount).toBe(1);
    expect(result.supplierCount).toBe(1);

    const xml = readFileSync(result.saftXmlPath!, "utf8");
    expect(xml).toContain("<Customers>");
    expect(xml).toContain("<CustomerName>Master Kunde A/S</CustomerName>");
    expect(xml).toContain("<CustomerTaxID>DK11111111</CustomerTaxID>");
    expect(xml).toContain("<Suppliers>");
    expect(xml).toContain("<SupplierName>Master Leverandør ApS</SupplierName>");
    expect(xml).toContain("<SupplierTaxID>DK22222222</SupplierTaxID>");

    const manifest = JSON.parse(readFileSync(result.manifestPath!, "utf8"));
    expect(manifest.counts.customers).toBe(1);
    expect(manifest.counts.suppliers).toBe(1);

    db.close();
    rmSync(root, { recursive: true, force: true });
  });
});
