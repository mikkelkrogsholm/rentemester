// Tests: src/core/public-einvoice.ts
import { describe, expect, test } from "bun:test";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { issueInvoice } from "../../src/core/issued-invoices";
import {
  exportPublicEInvoiceOioUbl,
  exportPublicEInvoicePreview,
  submitPublicEInvoicePeppol,
  transmitPublicEInvoicePeppol,
  resumePublicEInvoicePeppolSubmission,
  type PeppolTransmitter,
} from "../../src/core/public-einvoice";
import { digisenseAccessPointIdentity } from "../../src/core/efaktura/digisense-wiring";
import { createDigisenseTransmitter } from "../../src/core/efaktura/digisense-transmitter";
import type { DigisenseClient } from "../../src/core/efaktura/digisense-client";
import { buildInvoiceList } from "../../src/core/invoice-list";
import { wrapCoreResult } from "../../src/mcp/envelope";

const PUBLIC_INVOICE = {
  invoiceType: "full" as const,
  vatTreatment: "standard" as const,
  issueDate: "2026-05-20",
  invoiceNumber: "2026-0001",
  seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
  buyer: {
    name: "Københavns Kommune",
    address: "Rådhuset, 1599 København V",
    publicRecipient: true,
    eanNumber: "5790000000001",
  },
  lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1500, lineTotalExVat: 1500 }],
  totals: { netAmount: 1500, vatRate: 0.25, vatAmount: 375, grossAmount: 1875 },
  currency: "DKK",
  dueDate: "2026-06-19",
};

const ACCESS_POINT = {
  accessPointId: "ap-nemhandel-test",
  endpointUrl: "https://access-point.example.dk/peppol",
  senderEndpointId: "0184:DK12345678",
};

describe("public e-invoice preview export", () => {
  test("exports a deterministic preview artifact for public-recipient invoices", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-public-einvoice-"));
    const outPath = join(root, "public-invoice.xml");
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);

    const issued = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-20",
      invoiceNumber: "2026-0001",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: {
        name: "Københavns Kommune",
        address: "Rådhuset, 1599 København V",
        publicRecipient: true,
        eanNumber: "5790000000001",
      },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1500, lineTotalExVat: 1500 }],
      totals: { netAmount: 1500, vatRate: 0.25, vatAmount: 375, grossAmount: 1875 },
      currency: "DKK",
      dueDate: "2026-06-19",
    });

    expect(issued.ok).toBe(true);
    const first = exportPublicEInvoicePreview(db, { invoiceDocumentId: issued.documentId!, outPath });
    const second = exportPublicEInvoicePreview(db, { invoiceDocumentId: issued.documentId! });

    expect(first.ok).toBe(true);
    expect(second.ok).toBe(true);
    expect(first.sha256).toBe(second.sha256);
    expect(first.xml).toBe(second.xml);
    expect(readFileSync(outPath, "utf8")).toBe(first.xml);
    expect(first.xml).toContain("<EanNumber>5790000000001</EanNumber>");
    expect(first.xml).toContain("<Transport>out_of_scope_peppol_access_point_required</Transport>");

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("rejects export for invoices that are not marked as public-recipient invoices", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-public-einvoice-nonpublic-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);

    const issued = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-20",
      invoiceNumber: "2026-0001",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Privat Kunde", address: "Købervej 9" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1500, lineTotalExVat: 1500 }],
      totals: { netAmount: 1500, vatRate: 0.25, vatAmount: 375, grossAmount: 1875 },
      currency: "DKK",
    });

    expect(issued.ok).toBe(true);
    const exported = exportPublicEInvoicePreview(db, { invoiceDocumentId: issued.documentId! });

    expect(exported.ok).toBe(false);
    expect(exported.errors).toContain("invoice 2026-0001 is not marked as a public-recipient e-invoice");

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("exports a deterministic OIOUBL handoff artifact and records audit metadata", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-public-oioubl-"));
    const outPath = join(root, "public-invoice-oioubl.xml");
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);

    const issued = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-20",
      invoiceNumber: "2026-0001",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: {
        name: "Københavns Kommune",
        address: "Rådhuset, 1599 København V",
        publicRecipient: true,
        eanNumber: "5790000000001",
      },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1500, lineTotalExVat: 1500 }],
      totals: { netAmount: 1500, vatRate: 0.25, vatAmount: 375, grossAmount: 1875 },
      currency: "DKK",
      dueDate: "2026-06-19",
    });

    expect(issued.ok).toBe(true);

    const first = exportPublicEInvoiceOioUbl(db, { invoiceDocumentId: issued.documentId!, outPath });
    const second = exportPublicEInvoiceOioUbl(db, { invoiceDocumentId: issued.documentId! });

    expect(first.ok).toBe(true);
    expect(second.ok).toBe(true);
    expect(first.sha256).toBe(second.sha256);
    expect(first.xml).toBe(second.xml);
    expect(readFileSync(outPath, "utf8")).toBe(first.xml);
    expect(first.xml).toContain("<cbc:CustomizationID>OIOUBL-2.02</cbc:CustomizationID>");
    expect(first.xml).toContain('schemeID="urn:oioubl:id:profileid-1.2" schemeAgencyID="320">Procurement-BilSim-1.0</cbc:ProfileID>');
    expect(first.xml).toContain('<cbc:EndpointID schemeID="GLN">5790000000001</cbc:EndpointID>');
    expect(first.xml).toContain('<cbc:EndpointID schemeID="DK:CVR">DK12345678</cbc:EndpointID>');
    expect(first.xml).toContain(
      '<cac:PartyTaxScheme>\n        <cbc:CompanyID schemeID="DK:SE">DK12345678</cbc:CompanyID>',
    );
    expect(first.xml).toContain(
      '<cac:PartyLegalEntity>\n        <cbc:RegistrationName>Rentemester ApS</cbc:RegistrationName>\n        <cbc:CompanyID schemeID="DK:CVR">DK12345678</cbc:CompanyID>',
    );
    // BuyerReference (BT-10) is mandatory for public recipients (PEPPOL-EN16931-R003).
    expect(first.xml).toContain("<cbc:BuyerReference>");
    expect(first.xml).toContain('listID="urn:oioubl:codelist:addressformatcode-1.1" listAgencyID="320">Unstructured</cbc:AddressFormatCode>');
    expect(first.xml).toContain("<cbc:Name>Københavns Kommune</cbc:Name>");
    // VAT percent follows the canonical 0..1→×100 contract: a 0.25 rate renders
    // as "25" (BT-119/BT-152), never "0.25" — and a 1.0 rate would be "100",
    // never "1" (the old heuristic's bug).
    expect(first.xml).toContain("<cbc:Percent>25</cbc:Percent>");

    const auditRows = db.query(
      "SELECT event_type, entity_type, entity_id, message FROM audit_log WHERE event_type = 'public_einvoice_oioubl_export' ORDER BY id ASC",
    ).all() as Array<{ event_type: string; entity_type: string; entity_id: string; message: string }>;

    expect(auditRows).toHaveLength(2);
    expect(auditRows[0]).toEqual({
      event_type: "public_einvoice_oioubl_export",
      entity_type: "document",
      entity_id: String(issued.documentId),
      message: `Generated public OIOUBL handoff artifact for invoice 2026-0001 (sha256 ${first.sha256})`,
    });

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("renders a 100% VAT rate as cbc:Percent 100, not the old heuristic's 1", () => {
    // totals.vatRate is the canonical 0..1 fraction, so 1.0 means 100%. The old
    // `Number.isInteger(value) ? String(value) : ...` heuristic emitted "1" for
    // a 1.0 rate (1% on the transmitted Peppol line, disagreeing with the
    // invoice's own TaxAmount). The canonical ×100 rule renders "100".
    const root = mkdtempSync(join(tmpdir(), "rentemester-public-oioubl-100pct-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);

    const issued = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-20",
      invoiceNumber: "2026-0001",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: {
        name: "Københavns Kommune",
        address: "Rådhuset, 1599 København V",
        publicRecipient: true,
        eanNumber: "5790000000001",
      },
      lines: [{ description: "Ydelse", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 1.0, vatAmount: 1000, grossAmount: 2000 },
      currency: "DKK",
      dueDate: "2026-06-19",
    });
    expect(issued.ok).toBe(true);

    const exported = exportPublicEInvoiceOioUbl(db, { invoiceDocumentId: issued.documentId! });
    expect(exported.ok).toBe(true);
    expect(exported.xml).toContain("<cbc:Percent>100</cbc:Percent>");
    expect(exported.xml).not.toContain("<cbc:Percent>1</cbc:Percent>");

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("rejects OIOUBL export when required public-recipient handoff metadata is missing", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-public-oioubl-missing-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);

    const issued = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-20",
      invoiceNumber: "2026-0001",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: {
        name: "Københavns Kommune",
        address: "Rådhuset, 1599 København V",
        publicRecipient: true,
        eanNumber: "5790000000001",
      },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1500, lineTotalExVat: 1500 }],
      totals: { netAmount: 1500, vatRate: 0.25, vatAmount: 375, grossAmount: 1875 },
      currency: "DKK",
    });

    expect(issued.ok).toBe(true);

    const exported = exportPublicEInvoiceOioUbl(db, { invoiceDocumentId: issued.documentId! });

    expect(exported.ok).toBe(false);
    expect(exported.errors).toContain("invoice 2026-0001 is missing dueDate required for OIOUBL handoff");

    db.close();
    rmSync(root, { recursive: true, force: true });
  });
});

// OIOUBL 2.02 conformance: BuyerReference,
// a tax category derived from the VAT treatment (not hardcoded "S"), a
// configurable unit code, and the seller EndpointID under schemeID 0184 as a
// bare 8-digit CVR.
describe("public e-invoice OIOUBL 2.02 conformance", () => {
  test("maps OIOUBL TaxExclusiveAmount to the document VAT total", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-oioubl-f-inv127-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    const payload = {
      ...PUBLIC_INVOICE,
      invoiceNumber: "2026-F-INV127",
      lines: [{ description: "Syntetisk testlinje", quantity: 1, unitPriceExVat: 1, lineTotalExVat: 1 }],
      totals: { netAmount: 1, vatRate: 0.25, vatAmount: 0.25, grossAmount: 1.25 },
    };
    db.run(
      `INSERT INTO documents (source, sha256_hash, invoice_no, invoice_date, document_type, payload_json)
       VALUES ('test', ?, ?, ?, 'issued_invoice', ?)`,
      "oioubl-f-inv127-1-00",
      payload.invoiceNumber,
      payload.issueDate,
      JSON.stringify(payload),
    );
    const documentId = Number((db.query("SELECT last_insert_rowid() AS id").get() as { id: number }).id);

    const exported = exportPublicEInvoiceOioUbl(db, { invoiceDocumentId: documentId });
    expect(exported.ok).toBe(true);
    expect(exported.xml).toContain('<cbc:LineExtensionAmount currencyID="DKK">1.00</cbc:LineExtensionAmount>');
    expect(exported.xml).toContain('<cac:TaxTotal>\n    <cbc:TaxAmount currencyID="DKK">0.25</cbc:TaxAmount>');
    expect(exported.xml).toContain('<cbc:TaxExclusiveAmount currencyID="DKK">0.25</cbc:TaxExclusiveAmount>');
    expect(exported.xml).toContain('<cbc:TaxInclusiveAmount currencyID="DKK">1.25</cbc:TaxInclusiveAmount>');
    expect(exported.xml).toContain('<cbc:PayableAmount currencyID="DKK">1.25</cbc:PayableAmount>');

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("emits BuyerReference, OrderReference and a configurable unit code", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-jur9-ref-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);

    const { invoiceNumber: _drop, ...base } = PUBLIC_INVOICE;
    const issued = issueInvoice(db, root, {
      ...base,
      unitCode: "DAY",
      buyer: {
        ...PUBLIC_INVOICE.buyer,
        buyerReference: "EAN-REF-12345",
        orderReference: "ORDRE-987",
      },
    });
    expect(issued.ok).toBe(true);

    const exported = exportPublicEInvoiceOioUbl(db, { invoiceDocumentId: issued.documentId! });
    expect(exported.ok).toBe(true);
    expect(exported.xml).toContain("<cbc:BuyerReference>EAN-REF-12345</cbc:BuyerReference>");
    expect(exported.xml).toContain("<cac:OrderReference>");
    expect(exported.xml).toContain("<cbc:ID>ORDRE-987</cbc:ID>");
    // The configurable unit code overrides the H87 default.
    expect(exported.xml).toContain('<cbc:InvoicedQuantity unitCode="DAY">');
    expect(exported.xml).toContain('<cbc:EndpointID schemeID="DK:CVR">DK12345678</cbc:EndpointID>');

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("keeps tax category S for a standard-rated line", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-jur9-standard-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);

    const { invoiceNumber: _drop, ...base } = PUBLIC_INVOICE;
    const issued = issueInvoice(db, root, {
      ...base,
      buyer: { ...PUBLIC_INVOICE.buyer, buyerReference: "EAN-REF-STD" },
    });
    expect(issued.ok).toBe(true);

    const exported = exportPublicEInvoiceOioUbl(db, { invoiceDocumentId: issued.documentId! });
    expect(exported.ok).toBe(true);
    expect(exported.xml).toContain(">StandardRated</cbc:ID>");
    expect(exported.xml).toContain("<cbc:Percent>25</cbc:Percent>");
    // JUR-9: a standard-rated invoice must NOT carry an exemption reason
    // (BR-S-* forbids BT-120/BT-121 on category S).
    expect(exported.xml).not.toContain("cbc:TaxExemptionReason");

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("derives tax category AE for a domestic reverse-charge invoice", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-jur9-rc-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);

    const issued = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "domestic_reverse_charge",
      reverseChargeBasis: "DK_MOMSLOVEN_§46_STK_1_NR_6",
      issueDate: "2026-05-20",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: {
        name: "Københavns Kommune",
        address: "Rådhuset, 1599 København V",
        publicRecipient: true,
        eanNumber: "5790000000001",
        buyerReference: "EAN-REF-RC",
      },
      lines: [{ description: "Byggeydelse", quantity: 1, unitPriceExVat: 1500, lineTotalExVat: 1500 }],
      totals: { netAmount: 1500, grossAmount: 1500 },
      currency: "DKK",
      dueDate: "2026-06-19",
    });
    expect(issued.ok).toBe(true);

    const exported = exportPublicEInvoiceOioUbl(db, { invoiceDocumentId: issued.documentId! });
    expect(exported.ok).toBe(true);
    // Reverse charge => category AE, and the invoice still validates/exports
    // despite carrying no VAT amount/rate.
    expect(exported.xml).toContain(">ReverseCharge</cbc:ID>");
    expect(exported.xml).toContain("<cbc:BuyerReference>EAN-REF-RC</cbc:BuyerReference>");
    // TaxAmount renders as 0.00 so cac:TaxTotal stays well-formed.
    expect(exported.xml).toContain('<cbc:TaxAmount currencyID="DKK">0.00</cbc:TaxAmount>');
    // JUR-9: reverse charge (AE) must carry an exemption reason (BR-AE-10).
    // The code is the only AE-valid VATEX entry, plus a free-text reason that
    // carries the documented reverse-charge basis from the payload.
    expect(exported.xml).toContain(
      "<cbc:TaxExemptionReasonCode>VATEX-EU-AE</cbc:TaxExemptionReasonCode>",
    );
    expect(exported.xml).toContain("<cbc:TaxExemptionReason>");
    expect(exported.xml).toContain("DK_MOMSLOVEN_§46_STK_1_NR_6");

    // Historical reverse-charge rows without line classifications are still
    // subject to the OIOUBL arithmetic boundary. They must not be exported
    // merely because the seller VAT is zero.
    const legacyPayload = JSON.parse(
      (db.query("SELECT payload_json FROM documents WHERE id = ?").get(issued.documentId) as { payload_json: string }).payload_json,
    );
    legacyPayload.invoiceNumber = "2026-RC-LEGACY-BAD";
    legacyPayload.totals = { ...legacyPayload.totals, netAmount: 1000, grossAmount: 1000 };
    db.run(
      `INSERT INTO documents (source, sha256_hash, invoice_no, invoice_date, document_type, payload_json)
       VALUES ('test', ?, ?, ?, 'issued_invoice', ?)`,
      "jur9-rc-legacy-bad",
      legacyPayload.invoiceNumber,
      legacyPayload.issueDate,
      JSON.stringify(legacyPayload),
    );
    const contradictoryDocumentId = Number(
      (db.query("SELECT last_insert_rowid() AS id").get() as { id: number }).id,
    );
    const contradictory = exportPublicEInvoiceOioUbl(db, { invoiceDocumentId: contradictoryDocumentId });
    expect(contradictory.ok).toBe(false);
    expect(contradictory.errors).toContain(
      "invoice 2026-RC-LEGACY-BAD totals.netAmount must equal rounded OIOUBL line bases (1500)",
    );
    expect(contradictory.errors).toContain(
      "invoice 2026-RC-LEGACY-BAD totals.grossAmount must equal rounded OIOUBL line totals (1500)",
    );

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("emits an exemption reason for an exempt (E) 0%/no-VAT invoice", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-jur9-exempt-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);

    // issueInvoice rejects a 0% standard invoice (it requires a positive VAT
    // rate/amount), so the exempt (E) export branch is exercised by storing the
    // issued-invoice document row directly with a 0% payload. The export only
    // reads id/invoice_no/invoice_date/document_type/payload_json.
    const payload = {
      invoiceNumber: "2026-EXEMPT",
      issueDate: "2026-05-20",
      dueDate: "2026-06-19",
      currency: "DKK",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: {
        name: "Københavns Kommune",
        address: "Rådhuset, 1599 København V",
        publicRecipient: true,
        eanNumber: "5790000000001",
        buyerReference: "EAN-REF-EXEMPT",
      },
      lines: [{ description: "Momsfri ydelse", quantity: 1, unitPriceExVat: 1500, lineTotalExVat: 1500 }],
      totals: { netAmount: 1500, vatRate: 0, vatAmount: 0, grossAmount: 1500 },
    };
    db.run(
      `INSERT INTO documents (source, sha256_hash, invoice_no, invoice_date, document_type, payload_json)
       VALUES ('test', ?, ?, ?, 'issued_invoice', ?)`,
      `jur9-exempt-${payload.invoiceNumber}`,
      payload.invoiceNumber,
      payload.issueDate,
      JSON.stringify(payload),
    );
    const documentId = Number(
      (db.query("SELECT last_insert_rowid() AS id").get() as { id: number }).id,
    );

    const exported = exportPublicEInvoiceOioUbl(db, { invoiceDocumentId: documentId });
    expect(exported.ok).toBe(true);
    // A 0% line is treated as exempt (E); BR-E-10 requires an exemption reason.
    expect(exported.xml).toContain(">ZeroRated</cbc:ID>");
    expect(exported.xml).toContain("<cbc:TaxExemptionReason>");

    // The backwards-compatible interpretation is deliberately limited to
    // payloads without an explicit line classification. A source that says
    // "taxable" at 0% remains contradictory and must fail closed.
    const contradictoryPayload = {
      ...payload,
      invoiceNumber: "2026-EXEMPT-CONTRADICTORY",
      lines: payload.lines.map((line) => ({ ...line, taxClassification: "taxable" as const })),
    };
    db.run(
      `INSERT INTO documents (source, sha256_hash, invoice_no, invoice_date, document_type, payload_json)
       VALUES ('test', ?, ?, ?, 'issued_invoice', ?)`,
      `jur9-exempt-${contradictoryPayload.invoiceNumber}`,
      contradictoryPayload.invoiceNumber,
      contradictoryPayload.issueDate,
      JSON.stringify(contradictoryPayload),
    );
    const contradictoryDocumentId = Number(
      (db.query("SELECT last_insert_rowid() AS id").get() as { id: number }).id,
    );
    const contradictory = exportPublicEInvoiceOioUbl(db, { invoiceDocumentId: contradictoryDocumentId });
    expect(contradictory.ok).toBe(false);
    expect(contradictory.errors).toContain(
      `invoice ${contradictoryPayload.invoiceNumber} lines[0].vatRate is required for taxable lines`,
    );

    db.close();
    rmSync(root, { recursive: true, force: true });
  });
});

describe("public e-invoice PEPPOL submission", () => {
  test("produces a deterministic submission envelope on top of the OIOUBL handoff artifact", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-peppol-submit-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);

    const issued = issueInvoice(db, root, { ...PUBLIC_INVOICE });
    expect(issued.ok).toBe(true);

    const first = submitPublicEInvoicePeppol(db, {
      invoiceDocumentId: issued.documentId!,
      accessPoint: ACCESS_POINT,
    });
    const second = submitPublicEInvoicePeppol(db, {
      invoiceDocumentId: issued.documentId!,
      accessPoint: ACCESS_POINT,
    });

    expect(first.ok).toBe(true);
    expect(second.ok).toBe(true);
    // Deterministic envelope + idempotency key.
    expect(first.envelopeSha256).toBe(second.envelopeSha256);
    expect(first.envelope).toBe(second.envelope);
    expect(first.idempotencyKey).toBe(second.idempotencyKey);
    // Envelope embeds the OIOUBL handoff artifact hash, not a mutated payload.
    const oioubl = exportPublicEInvoiceOioUbl(db, { invoiceDocumentId: issued.documentId! });
    expect(first.oioublSha256).toBe(oioubl.sha256);
    expect(first.envelope).toContain(oioubl.sha256!);
    expect(first.envelope).toContain("ap-nemhandel-test");
    expect(first.appliedRules).toContain("DK-PEPPOL-SUBMIT-001");

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("is idempotent: a duplicate submission reuses the existing attempt record", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-peppol-idempotent-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);

    const issued = issueInvoice(db, root, { ...PUBLIC_INVOICE });
    expect(issued.ok).toBe(true);

    const first = submitPublicEInvoicePeppol(db, {
      invoiceDocumentId: issued.documentId!,
      accessPoint: ACCESS_POINT,
    });
    const second = submitPublicEInvoicePeppol(db, {
      invoiceDocumentId: issued.documentId!,
      accessPoint: ACCESS_POINT,
    });

    expect(first.ok).toBe(true);
    expect(first.duplicate).toBe(false);
    expect(second.ok).toBe(true);
    expect(second.duplicate).toBe(true);
    expect(second.submissionReference).toBe(first.submissionReference);

    const rows = db
      .query("SELECT id FROM peppol_submissions WHERE idempotency_key = ?")
      .all(first.idempotencyKey!) as Array<{ id: number }>;
    expect(rows).toHaveLength(1);

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("records an audit event linking invoice to submission attempt", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-peppol-audit-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);

    const issued = issueInvoice(db, root, { ...PUBLIC_INVOICE });
    expect(issued.ok).toBe(true);

    const result = submitPublicEInvoicePeppol(db, {
      invoiceDocumentId: issued.documentId!,
      accessPoint: ACCESS_POINT,
    });
    expect(result.ok).toBe(true);

    const auditRows = db
      .query(
        "SELECT event_type, entity_type, entity_id, message FROM audit_log WHERE event_type = 'public_einvoice_peppol_submission' ORDER BY id ASC",
      )
      .all() as Array<{ event_type: string; entity_type: string; entity_id: string; message: string }>;
    // One submission attempt -> exactly one audit event (idempotent re-runs do not append).
    expect(auditRows).toHaveLength(1);
    expect(auditRows[0]!.entity_type).toBe("document");
    expect(auditRows[0]!.entity_id).toBe(String(issued.documentId));
    expect(auditRows[0]!.message).toContain(result.submissionReference!);

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("fails clearly when access-point config is missing", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-peppol-noconfig-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);

    const issued = issueInvoice(db, root, { ...PUBLIC_INVOICE });
    expect(issued.ok).toBe(true);

    const result = submitPublicEInvoicePeppol(db, {
      invoiceDocumentId: issued.documentId!,
      accessPoint: { accessPointId: "", endpointUrl: "", senderEndpointId: "" },
    });

    expect(result.ok).toBe(false);
    expect(result.errors.join(" ")).toContain("access-point");

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("fails clearly when required public-recipient OIOUBL metadata is missing", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-peppol-missing-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);

    // Omit dueDate -> OIOUBL handoff validation fails -> submission must fail too.
    const { dueDate, ...withoutDueDate } = PUBLIC_INVOICE;
    const issued = issueInvoice(db, root, withoutDueDate);
    expect(issued.ok).toBe(true);

    const result = submitPublicEInvoicePeppol(db, {
      invoiceDocumentId: issued.documentId!,
      accessPoint: ACCESS_POINT,
    });

    expect(result.ok).toBe(false);
    expect(result.errors.join(" ")).toContain("dueDate");
    // No submission row written on failure.
    const rows = db.query("SELECT id FROM peppol_submissions").all() as Array<{ id: number }>;
    expect(rows).toHaveLength(0);

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("records a transport acknowledgement when one is supplied", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-peppol-ack-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);

    const issued = issueInvoice(db, root, { ...PUBLIC_INVOICE });
    expect(issued.ok).toBe(true);

    const result = submitPublicEInvoicePeppol(db, {
      invoiceDocumentId: issued.documentId!,
      accessPoint: ACCESS_POINT,
      acknowledgement: { transmissionId: "tx-9001", acknowledgedAt: "2026-05-20T10:00:00Z" },
    });

    expect(result.ok).toBe(true);
    expect(result.status).toBe("acknowledged");

    const row = db
      .query("SELECT status, transmission_id FROM peppol_submissions WHERE idempotency_key = ?")
      .get(result.idempotencyKey!) as { status: string; transmission_id: string | null };
    expect(row.status).toBe("acknowledged");
    expect(row.transmission_id).toBe("tx-9001");

    db.close();
    rmSync(root, { recursive: true, force: true });
  });
});

describe("public e-invoice PEPPOL transmission", () => {
  const okTransmitter: PeppolTransmitter = () => ({
    ok: true,
    transmissionId: "tx-test-0001",
    transmittedAt: "2026-05-22T10:00:00Z",
  });
  const failTransmitter: PeppolTransmitter = () => ({
    ok: false,
    error: "access point unavailable",
    retryableBeforeDelivery: true,
  });

  test("transmits an invoice and records it as an acknowledged submission", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-peppol-transmit-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    const issued = issueInvoice(db, root, { ...PUBLIC_INVOICE });
    expect(issued.ok).toBe(true);

    const result = await transmitPublicEInvoicePeppol(
      db,
      { invoiceDocumentId: issued.documentId!, accessPoint: ACCESS_POINT },
      okTransmitter,
    );

    expect(result.ok).toBe(true);
    expect(result.status).toBe("acknowledged");
    expect(result.transmissionId).toBe("tx-test-0001");

    const row = db
      .query(
        "SELECT status, transmission_id, acknowledged_at FROM peppol_submissions WHERE invoice_document_id = ?",
      )
      .get(issued.documentId!) as {
      status: string;
      transmission_id: string | null;
      acknowledged_at: string | null;
    };
    expect(row.status).toBe("prepared");
    expect(row.transmission_id).toBeNull();
    expect(row.acknowledged_at).toBeNull();
    expect(db.query("SELECT document_id FROM peppol_submission_events WHERE event_type = 'delivered'").get()).toMatchObject({ document_id: "tx-test-0001" });

    const audit = db
      .query("SELECT message FROM audit_log WHERE event_type = 'public_einvoice_peppol_transmission'")
      .all() as Array<{ message: string }>;
    expect(audit).toHaveLength(1);
    expect(audit[0]!.message).toContain("tx-test-0001");

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("is idempotent: an already-transmitted invoice is not transmitted again", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-peppol-transmit-idem-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    const issued = issueInvoice(db, root, { ...PUBLIC_INVOICE });
    expect(issued.ok).toBe(true);

    let calls = 0;
    const countingTransmitter: PeppolTransmitter = () => {
      calls += 1;
      return { ok: true, transmissionId: "tx-once", transmittedAt: "2026-05-22T10:00:00Z" };
    };

    const first = await transmitPublicEInvoicePeppol(
      db,
      { invoiceDocumentId: issued.documentId!, accessPoint: ACCESS_POINT },
      countingTransmitter,
    );
    const second = await transmitPublicEInvoicePeppol(
      db,
      { invoiceDocumentId: issued.documentId!, accessPoint: ACCESS_POINT },
      countingTransmitter,
    );

    expect(first.ok).toBe(true);
    expect(second.ok).toBe(true);
    expect(second.status).toBe("acknowledged");
    expect(second.transmissionId).toBe("tx-once");
    // The transmitter ran exactly once — the second call short-circuits.
    expect(calls).toBe(1);
    const rows = db.query("SELECT id FROM peppol_submissions").all() as Array<{ id: number }>;
    expect(rows).toHaveLength(1);

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("preserves a legacy row acknowledgement without status lookup or redelivery", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-peppol-legacy-ack-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    const issued = issueInvoice(db, root, { ...PUBLIC_INVOICE });
    expect(issued.ok).toBe(true);
    const prepared = submitPublicEInvoicePeppol(db, { invoiceDocumentId: issued.documentId!, accessPoint: ACCESS_POINT });
    expect(prepared.ok).toBe(true);

    // Model a database written by the pre-event implementation.
    db.run("DROP TRIGGER peppol_submissions_no_update");
    db.run(
      "UPDATE peppol_submissions SET status = 'acknowledged', transmission_id = 'legacy-tx', acknowledged_at = '2026-05-22T10:00:00Z'",
    );

    let deliveryCalls = 0;
    const result = await transmitPublicEInvoicePeppol(
      db,
      { invoiceDocumentId: issued.documentId!, accessPoint: ACCESS_POINT },
      () => { deliveryCalls += 1; return { ok: true, transmissionId: "must-not-send", transmittedAt: "2026-05-23T10:00:00Z" }; },
    );
    expect(result.status).toBe("acknowledged");
    expect(result.transmissionId).toBe("legacy-tx");
    expect(deliveryCalls).toBe(0);

    let statusCalls = 0;
    const resumed = await resumePublicEInvoicePeppolSubmission(
      db,
      { invoiceDocumentId: issued.documentId!, accessPoint: ACCESS_POINT },
      async () => { statusCalls += 1; return { ok: true, status: "delivered" }; },
    );
    expect(resumed.status).toBe("acknowledged");
    expect(statusCalls).toBe(0);
    expect(buildInvoiceList(db).rows[0]?.peppolStatus?.status).toBe("acknowledged");

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("records a failed transmission as retryable append-only evidence", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-peppol-transmit-fail-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    const issued = issueInvoice(db, root, { ...PUBLIC_INVOICE });
    expect(issued.ok).toBe(true);

    const result = await transmitPublicEInvoicePeppol(
      db,
      { invoiceDocumentId: issued.documentId!, accessPoint: ACCESS_POINT },
      failTransmitter,
    );

    expect(result.ok).toBe(false);
    expect(result.errors.join(" ")).toContain("access point unavailable");

    const rows = db.query("SELECT id FROM peppol_submissions").all() as Array<{ id: number }>;
    expect(rows).toHaveLength(1);
    expect(db.query("SELECT event_type FROM peppol_submission_events WHERE event_type = 'delivery_failed'").get()).not.toBeNull();
    expect(buildInvoiceList(db).rows[0]?.peppolStatus?.status).toBe("retryable");

    const audit = db
      .query("SELECT message FROM audit_log WHERE event_type = 'public_einvoice_peppol_transmission'")
      .all() as Array<{ message: string }>;
    expect(audit).toHaveLength(1);
    expect(audit[0]!.message).toContain("failed");

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("a failed transmission can be retried and reach acknowledged", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-peppol-transmit-retry-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    const issued = issueInvoice(db, root, { ...PUBLIC_INVOICE });
    expect(issued.ok).toBe(true);

    const failed = await transmitPublicEInvoicePeppol(
      db,
      { invoiceDocumentId: issued.documentId!, accessPoint: ACCESS_POINT },
      failTransmitter,
    );
    expect(failed.ok).toBe(false);

    const retried = await transmitPublicEInvoicePeppol(
      db,
      { invoiceDocumentId: issued.documentId!, accessPoint: ACCESS_POINT },
      okTransmitter,
    );
    expect(retried.ok).toBe(true);
    expect(retried.status).toBe("acknowledged");

    const rows = db.query("SELECT status FROM peppol_submissions").all() as Array<{ status: string }>;
    expect(rows).toHaveLength(1);
    expect(rows[0]!.status).toBe("prepared");

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("surfaces OIOUBL validation errors without calling the transmitter", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-peppol-transmit-invalid-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    // Omit dueDate -> OIOUBL handoff validation fails before any transport.
    const { dueDate, ...withoutDueDate } = PUBLIC_INVOICE;
    const issued = issueInvoice(db, root, withoutDueDate);
    expect(issued.ok).toBe(true);

    let calls = 0;
    const spyTransmitter: PeppolTransmitter = () => {
      calls += 1;
      return { ok: true, transmissionId: "tx-x", transmittedAt: "2026-05-22T10:00:00Z" };
    };

    const result = await transmitPublicEInvoicePeppol(
      db,
      { invoiceDocumentId: issued.documentId!, accessPoint: ACCESS_POINT },
      spyTransmitter,
    );

    expect(result.ok).toBe(false);
    expect(result.errors.join(" ")).toContain("dueDate");
    expect(calls).toBe(0);

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("treats a thrown transmitter error as uncertain and blocks retry", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-peppol-transmit-throw-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    const issued = issueInvoice(db, root, { ...PUBLIC_INVOICE });
    expect(issued.ok).toBe(true);

    let calls = 0;
    const throwingTransmitter: PeppolTransmitter = () => {
      calls += 1;
      throw new Error("socket reset by access point");
    };

    const result = await transmitPublicEInvoicePeppol(
      db,
      { invoiceDocumentId: issued.documentId!, accessPoint: ACCESS_POINT },
      throwingTransmitter,
    );

    const retry = await transmitPublicEInvoicePeppol(
      db,
      { invoiceDocumentId: issued.documentId!, accessPoint: ACCESS_POINT },
      throwingTransmitter,
    );
    expect(result).toMatchObject({ ok: true, status: "uncertain" });
    expect(retry).toMatchObject({ ok: true, status: "uncertain", duplicate: true });
    expect(calls).toBe(1);

    const rows = db.query("SELECT id FROM peppol_submissions").all() as Array<{ id: number }>;
    expect(rows).toHaveLength(1);

    db.close();
    rmSync(root, { recursive: true, force: true });
  });
});

describe("public e-invoice PEPPOL transmission — Digisense double-send safety", () => {
  // For Digisense the access point IS Digisense (routing on companyKey +
  // license-key); the transmitter ignores accessPoint entirely. The deterministic
  // identity keyed on companyKey is what keeps the idempotency key stable.
  const COMPANY_KEY = "ck-digisense-42";

  test("the Digisense identity makes the idempotency key stable across calls (no double-deliver)", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-digisense-idem-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    const issued = issueInvoice(db, root, { ...PUBLIC_INVOICE });
    expect(issued.ok).toBe(true);

    let calls = 0;
    const deliverOnce: PeppolTransmitter = () => {
      calls += 1;
      return { ok: true, transmissionId: "ds-doc-1", transmittedAt: "2026-05-22T10:00:00Z" };
    };

    // Two transmits of the SAME invoice using the SAME deterministic identity.
    const ap = digisenseAccessPointIdentity(COMPANY_KEY);
    const first = await transmitPublicEInvoicePeppol(db, { invoiceDocumentId: issued.documentId!, accessPoint: ap }, deliverOnce);
    const second = await transmitPublicEInvoicePeppol(db, { invoiceDocumentId: issued.documentId!, accessPoint: ap }, deliverOnce);

    expect(first.ok).toBe(true);
    expect(second.ok).toBe(true);
    // The second call collapses onto the acknowledged row — deliver ran ONCE.
    expect(calls).toBe(1);
    const rows = db.query("SELECT id FROM peppol_submissions").all() as Array<{ id: number }>;
    expect(rows).toHaveLength(1);

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("atomically reserves delivery before an async transport so concurrent callers deliver once", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-digisense-concurrent-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    const issued = issueInvoice(db, root, { ...PUBLIC_INVOICE });
    let calls = 0;
    let release!: () => void;
    const hold = new Promise<void>((resolve) => { release = resolve; });
    const transmitter: PeppolTransmitter = async () => {
      calls += 1;
      await hold;
      return { ok: true, transmissionId: "ds-concurrent-1", transmittedAt: "2026-06-01T00:00:00Z" };
    };
    const ap = digisenseAccessPointIdentity(COMPANY_KEY);
    const first = transmitPublicEInvoicePeppol(db, { invoiceDocumentId: issued.documentId!, accessPoint: ap }, transmitter);
    const second = transmitPublicEInvoicePeppol(db, { invoiceDocumentId: issued.documentId!, accessPoint: ap }, transmitter);
    await Promise.resolve();
    release();
    const [firstResult, secondResult] = await Promise.all([first, second]);
    expect(calls).toBe(1);
    expect(firstResult.ok).toBe(true);
    expect(secondResult.errors.join(" ")).toContain("in progress");
    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("a queued-but-not-delivered timeout records a pending row and refuses to re-deliver", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-digisense-queued-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    const issued = issueInvoice(db, root, { ...PUBLIC_INVOICE });
    expect(issued.ok).toBe(true);

    let deliverCalls = 0;
    // First attempt: the access point ACCEPTED the doc into its queue but we
    // never observed `delivered` — it returns the queued documentId.
    const queuedTimeoutTransmitter: PeppolTransmitter = () => {
      deliverCalls += 1;
      return { ok: false, error: "timed out (still queued)", queuedDocumentId: "ds-queued-7" };
    };

    const ap = digisenseAccessPointIdentity(COMPANY_KEY);
    const first = await transmitPublicEInvoicePeppol(db, { invoiceDocumentId: issued.documentId!, accessPoint: ap }, queuedTimeoutTransmitter);

    expect(first.ok).toBe(true);
    expect(first.status).toBe("prepared");
    expect(first.transmissionId).toBe("ds-queued-7");
    // A pending submission row and append-only queued event were recorded.
    const pendingRow = db
      .query("SELECT status, transmission_id FROM peppol_submissions WHERE invoice_document_id = ?")
      .get(issued.documentId!) as { status: string; transmission_id: string | null };
    expect(pendingRow.status).toBe("prepared");
    expect(pendingRow.transmission_id).toBeNull();
    expect(db.query("SELECT document_id FROM peppol_submission_events WHERE event_type = 'queued'").get()).toMatchObject({ document_id: "ds-queued-7" });
    expect(buildInvoiceList(db).rows[0]?.peppolStatus?.status).toBe("queued");

    // A naive retry MUST NOT call deliver again — that would deliver the invoice
    // a second time. It is a successful pending result for status-only UI mode.
    const retry = await transmitPublicEInvoicePeppol(db, { invoiceDocumentId: issued.documentId!, accessPoint: ap }, queuedTimeoutTransmitter);
    expect(retry.ok).toBe(true);
    expect(retry.status).toBe("prepared");
    expect(retry.transmissionId).toBe("ds-queued-7");
    // deliver ran exactly ONCE across both attempts.
    expect(deliverCalls).toBe(1);
    // Still exactly one submission row.
    const rows = db.query("SELECT id FROM peppol_submissions").all() as Array<{ id: number }>;
    expect(rows).toHaveLength(1);

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("an accepted terminal failure is never re-delivered", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-digisense-terminal-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    const issued = issueInvoice(db, root, { ...PUBLIC_INVOICE });
    let deliverCalls = 0;
    const ap = digisenseAccessPointIdentity(COMPANY_KEY);
    const client = {
      validateDocument: async () => ({
        ok: true as const,
        status: 200,
        data: { statusCode: 200, success: true, errors: [] },
      }),
      deliverDocument: async () => {
        deliverCalls += 1;
        return {
          ok: true as const,
          status: 202,
          data: { statusCode: 202, documentStatus: "queued-for-delivery" as const, documentId: "ds-terminal-1", message: "queued", publicUrl: "" },
        };
      },
      documentStatus: async () => ({
        ok: true as const,
        status: 422,
        data: { statusCode: 422, documentStatus: "unable-to-deliver" as const, documentId: "ds-terminal-1", message: "receiver rejected", publicUrl: "" },
      }),
    } as unknown as DigisenseClient;
    const acceptedThenRejected = createDigisenseTransmitter(client, {
      companyKey: COMPANY_KEY,
      sleep: async () => {},
      maxPollAttempts: 2,
    });

    const first = await transmitPublicEInvoicePeppol(db, { invoiceDocumentId: issued.documentId!, accessPoint: ap }, acceptedThenRejected);
    const retry = await transmitPublicEInvoicePeppol(db, { invoiceDocumentId: issued.documentId!, accessPoint: ap }, acceptedThenRejected);

    expect(first.ok).toBe(true);
    expect(retry.ok).toBe(true);
    expect(first.status).toBe("failed");
    expect(retry.status).toBe("failed");
    expect(wrapCoreResult(first)).toMatchObject({
      ok: true,
      data: { status: "failed", transmissionId: "ds-terminal-1" },
      errors: [],
    });
    expect(deliverCalls).toBe(1);
    expect(buildInvoiceList(db).rows[0]?.peppolStatus).toMatchObject({
      status: "failed",
      transmissionId: "ds-terminal-1",
    });
    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("an ambiguous delivery response becomes uncertain and is never re-delivered", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-digisense-uncertain-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    const issued = issueInvoice(db, root, { ...PUBLIC_INVOICE });
    let deliverCalls = 0;
    const ambiguous: PeppolTransmitter = () => {
      deliverCalls += 1;
      // No explicit pre-delivery proof: generic failures fail closed as
      // uncertain even if the adapter forgot to set deliveryUncertain.
      return { ok: false, error: "transport timed out after POST" };
    };
    const ap = digisenseAccessPointIdentity(COMPANY_KEY);

    const first = await transmitPublicEInvoicePeppol(db, { invoiceDocumentId: issued.documentId!, accessPoint: ap }, ambiguous);
    const retry = await transmitPublicEInvoicePeppol(db, { invoiceDocumentId: issued.documentId!, accessPoint: ap }, ambiguous);

    expect(first).toMatchObject({ ok: true, status: "uncertain" });
    expect(retry).toMatchObject({ ok: true, status: "uncertain", duplicate: true });
    expect(deliverCalls).toBe(1);
    expect(buildInvoiceList(db).rows[0]?.peppolStatus?.status).toBe("uncertain");
    expect(wrapCoreResult(first)).toMatchObject({ ok: true, data: { status: "uncertain" }, errors: [] });
    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("status resume records append-only evidence and makes later transmit acknowledged without re-delivery", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-digisense-resume-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    const issued = issueInvoice(db, root, { ...PUBLIC_INVOICE });
    const ap = digisenseAccessPointIdentity(COMPANY_KEY);
    let deliverCalls = 0;
    await transmitPublicEInvoicePeppol(db, { invoiceDocumentId: issued.documentId!, accessPoint: ap }, () => {
      deliverCalls += 1;
      return { ok: false, error: "queued", queuedDocumentId: "ds-resume-1" };
    });
    const resumed = await resumePublicEInvoicePeppolSubmission(db, { invoiceDocumentId: issued.documentId!, accessPoint: ap }, () => ({ ok: true, status: "delivered", message: "done", observedAt: "2026-06-01T00:00:00Z" }));
    expect(resumed.ok).toBe(true);
    expect(resumed.status).toBe("acknowledged");
    expect(db.query("SELECT id FROM peppol_submission_events WHERE event_type = 'status_observed'").all()).toHaveLength(1);
    const retry = await transmitPublicEInvoicePeppol(db, { invoiceDocumentId: issued.documentId!, accessPoint: ap }, () => {
      deliverCalls += 1;
      return { ok: true, transmissionId: "should-not-run", transmittedAt: "2026-06-01T00:00:00Z" };
    });
    expect(retry.ok).toBe(true);
    expect(retry.status).toBe("acknowledged");
    expect(deliverCalls).toBe(1);
    db.close();
    rmSync(root, { recursive: true, force: true });
  });
});
