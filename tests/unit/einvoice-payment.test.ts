import { describe, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { openDb, migrate } from "../../src/core/db";
import { ensureCompanyDirs } from "../../src/core/paths";
import { issueInvoice } from "../../src/core/issued-invoices";
import { buildPublicEInvoiceOioUblXml, exportPublicEInvoiceOioUbl, transmitPublicEInvoicePeppol, validateOioUblPayload } from "../../src/core/public-einvoice";

const account = { registrationNo: "1234", accountNo: "5678901234", iban: "DK6712345678901234", bic: "SYNFDKKK" };
const invoice = {
  invoiceType: "full" as const, issueDate: "2026-05-16", dueDate: "2026-06-16", currency: "DKK",
  seller: { name: "Synthetic Seller", address: "Synthetic Road 1", vatOrCvr: "DK12345678" },
  buyer: { name: "Synthetic Buyer", address: "Synthetic Road 2", eanNumber: "5790000000013", publicRecipient: true, buyerReference: "BUYER-ABC", orderReference: "ORDER-789" },
  lines: [{ description: "Synthetic test work", quantity: 1, unitPriceExVat: 100, lineTotalExVat: 100 }],
  totals: { netAmount: 100, vatRate: 0.25, vatAmount: 25, grossAmount: 125 },
};
const accessPoint = { accessPointId: "synthetic", endpointUrl: "https://example.invalid", senderEndpointId: "synthetic" };

describe("OIOUBL payment from issued evidence", () => {
  for (const version of ["2.1", "2.02"] as const) for (const exempt of [false, true]) {
    test(`${version}: ${exempt ? "exempt" : "25%"} has exact payment and references`, () => {
      const payload = { ...invoice, payment: account, ...(exempt ? { totals: { netAmount: 100, vatRate: 0, vatAmount: 0, grossAmount: 100 } } : {}) };
      expect(validateOioUblPayload("2026-0001", payload, payload.buyer.eanNumber)).toEqual([]);
      const xml = buildPublicEInvoiceOioUblXml("2026-0001", payload, undefined, version);
      const payment = xml.match(/<cac:PaymentMeans>([\s\S]*?)<\/cac:PaymentMeans>/)![1]!;
      expect(payment).toContain(">42</cbc:PaymentMeansCode>");
      expect(payment).toContain(">DK:BANK</cbc:PaymentChannelCode>");
      expect(payment).toContain("<cbc:PaymentDueDate>2026-06-16</cbc:PaymentDueDate>");
      expect(payment).toContain("<cac:PayeeFinancialAccount>\n      <cbc:ID>5678901234</cbc:ID>");
      expect(payment).toContain("<cac:FinancialInstitutionBranch>\n        <cbc:ID>1234</cbc:ID>");
      if (version === "2.1") expect(xml).toContain("<cbc:BuyerReference>BUYER-ABC</cbc:BuyerReference>");
      else expect(xml).not.toContain("<cbc:BuyerReference>");
      expect(xml).toContain("<cac:Contact>\n        <cbc:ID>BUYER-ABC</cbc:ID>");
      expect(xml).toContain("<cac:OrderReference>\n    <cbc:ID>ORDER-789</cbc:ID>");
      expect(xml).not.toContain("<cbc:ID>TEST</cbc:ID>");
      expect(xml).toContain(`<cbc:TaxAmount currencyID="DKK">${exempt ? "0.00" : "25.00"}</cbc:TaxAmount>`);
      expect(xml).toContain(`<cbc:PayableAmount currencyID="DKK">${exempt ? "100.00" : "125.00"}</cbc:PayableAmount>`);
    });
  }
  test("short Danish account numbers are left padded without changing the account", () => {
    const payload = { ...invoice, payment: { registrationNo: "1234", accountNo: "456789" } };
    expect(validateOioUblPayload("2026-0001", payload, payload.buyer.eanNumber)).toEqual([]);
    expect(buildPublicEInvoiceOioUblXml("2026-0001", payload)).toContain("<cbc:ID>0000456789</cbc:ID>");
  });
  test("IBAN and BIC map to international transfer without a branch ID", () => {
    const payload = { ...invoice, payment: { iban: account.iban, bic: account.bic } };
    expect(validateOioUblPayload("2026-0001", payload, payload.buyer.eanNumber)).toEqual([]);
    for (const version of ["2.1", "2.02"] as const) {
      const xml = buildPublicEInvoiceOioUblXml("2026-0001", payload, undefined, version);
      const payment = xml.match(/<cac:PaymentMeans>([\s\S]*?)<\/cac:PaymentMeans>/)![1]!;
      expect(payment).toContain(">31</cbc:PaymentMeansCode>");
      expect(payment).toContain(">IBAN</cbc:PaymentChannelCode>");
      expect(payment).toContain(`<cbc:ID>${account.iban}</cbc:ID>`);
      expect(payment).toContain(`<cac:FinancialInstitution>\n          <cbc:ID>${account.bic}</cbc:ID>`);
      expect(payment).not.toContain("<cac:FinancialInstitutionBranch>\n        <cbc:ID>");
    }
  });
  test("invalid payment and identity stop before provider, including legacy rows", async () => {
    const root = mkdtempSync(join(tmpdir(), "synthetic-payment-boundary-"));
    const db = openDb(ensureCompanyDirs(root).db); migrate(db);
    try {
      const bad = [
        { ...invoice },
        { ...invoice, payment: { registrationNo: "123", accountNo: account.accountNo } },
        { ...invoice, payment: { ...account, iban: "DK0012345678901234" } },
        { ...invoice, payment: { ...account, accountNo: "5678901235" } },
        { ...invoice, payment: { iban: account.iban } },
        { ...invoice, payment: { ...account, type: "FIK" } },
        { ...invoice, payment: { registrationNo: "1234", accountNo: "12345678901" } },
        { ...invoice, payment: account, buyer: { ...invoice.buyer, buyerReference: "TEST" } },
        { ...invoice, payment: account, buyer: { ...invoice.buyer, buyerReference: "", orderReference: "ORDER-789" } },
      ];
      let sends = 0;
      for (const [i, value] of bad.entries()) {
        const number = `2026-BAD-${i}`;
        const id = (db.query("INSERT INTO documents (source,sha256_hash,invoice_no,invoice_date,document_type,payload_json) VALUES ('synthetic',?,?,?,'issued_invoice',?) RETURNING id").get(`bad-${i}`, number, invoice.issueDate, JSON.stringify(value)) as { id: number }).id;
        const result = await transmitPublicEInvoicePeppol(db, { invoiceDocumentId: id, accessPoint }, async () => { sends++; return { ok: true, transmissionId: "never", transmittedAt: "2026-05-16T00:00:00Z" }; });
        expect(result.ok).toBe(false);
        expect(result.errors.length).toBeGreaterThan(0);
      }
      expect(sends).toBe(0);
    } finally { db.close(); rmSync(root, { recursive: true, force: true }); }
  });
  test("previously exported legacy XML without payment cannot bypass transmit validation", async () => {
    const root = mkdtempSync(join(tmpdir(), "synthetic-legacy-transmit-"));
    const db = openDb(ensureCompanyDirs(root).db); migrate(db);
    try {
      const id = (db.query("INSERT INTO documents (source,sha256_hash,invoice_no,invoice_date,document_type,payload_json) VALUES ('synthetic','legacy','2026-LEGACY',?,'issued_invoice',?) RETURNING id").get(invoice.issueDate, JSON.stringify(invoice)) as { id: number }).id;
      const oldXml = buildPublicEInvoiceOioUblXml("2026-LEGACY", invoice);
      expect(oldXml).not.toContain("<cac:PaymentMeans>");
      const hash = createHash("sha256").update(oldXml).digest("hex");
      db.run("INSERT INTO peppol_submissions(invoice_document_id,invoice_no,idempotency_key,submission_reference,access_point_id,receiver_endpoint_id,oioubl_sha256,envelope_sha256,envelope_xml,status) VALUES (?,?,?,?,?,?,?,?,?,'prepared')", [id, "2026-LEGACY", "legacy-key", "legacy-ref", "synthetic", "GLN:5790000000013", hash, "b".repeat(64), "legacy evidence"]);
      let sends = 0;
      const result = await transmitPublicEInvoicePeppol(db, { invoiceDocumentId: id, accessPoint }, async () => { sends++; return { ok: true, transmissionId: "never", transmittedAt: "2026-05-16T00:00:00Z" }; });
      expect(result.ok).toBe(false); expect(result.errors.join(" ")).toContain("payment"); expect(sends).toBe(0);
    } finally { db.close(); rmSync(root, { recursive: true, force: true }); }
  });
  test("snapshot survives changed bank settings; old submission hash blocks a second transport", async () => {
    const root = mkdtempSync(join(tmpdir(), "synthetic-payment-snapshot-"));
    const db = openDb(ensureCompanyDirs(root).db); migrate(db);
    try {
      db.run("INSERT INTO bank_accounts (slug,name,currency,registration_no,account_no,iban,bic) VALUES ('synthetic','Synthetic Bank','DKK',?,?,?,?)", [account.registrationNo, account.accountNo, account.iban, account.bic]);
      const issued = issueInvoice(db, root, invoice);
      expect(issued.ok).toBe(true);
      db.run("UPDATE bank_accounts SET registration_no='2222',account_no='3333333333',iban=NULL,bic=NULL");
      const xml = exportPublicEInvoiceOioUbl(db, { invoiceDocumentId: issued.documentId! });
      expect(xml.ok).toBe(true);
      expect(xml.xml).toContain("<cbc:ID>5678901234</cbc:ID>");
      expect(xml.xml).not.toContain("3333333333");
      expect(xml.sha256).toBe(createHash("sha256").update(xml.xml!).digest("hex"));
      db.run("INSERT INTO peppol_submissions(invoice_document_id,invoice_no,idempotency_key,submission_reference,access_point_id,receiver_endpoint_id,oioubl_sha256,envelope_sha256,envelope_xml,status) VALUES (?,?,?,?,?,?,?,?,?,'acknowledged')", [issued.documentId!, issued.invoiceNumber!, "old-key", "old-ref", "synthetic", "GLN:5790000000013", "a".repeat(64), "b".repeat(64), "old evidence"]);
      let sends = 0;
      const result = await transmitPublicEInvoicePeppol(db, { invoiceDocumentId: issued.documentId!, accessPoint }, async () => { sends++; return { ok: true, transmissionId: "never", transmittedAt: "2026-05-16T00:00:00Z" }; });
      expect(result.ok).toBe(false); expect(result.errors.join(" ")).toContain("different XML artifact"); expect(sends).toBe(0);
    } finally { db.close(); rmSync(root, { recursive: true, force: true }); }
  });
});
