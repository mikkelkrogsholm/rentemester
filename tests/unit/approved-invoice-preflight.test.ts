import { describe, expect, test } from "bun:test";
import { mkdtempSync, readFileSync, rmSync, writeFileSync, symlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { openDb, migrate } from "../../src/core/db";
import { ensureCompanyDirs } from "../../src/core/paths";
import { preflightApprovedInvoice, snapshotApprovedInvoiceSource } from "../../src/core/approved-invoice-preflight";
import { issueInvoice } from "../../src/core/issued-invoices";
import type { DigisenseClient } from "../../src/core/efaktura/digisense-client";
import { renderDocumentPdf } from "../../src/design/pdf-render";
import { renderIssuedInvoicePdf } from "../../src/core/invoice-pdf";
import { postIssuedInvoiceToLedger } from "../../src/core/invoice-booking";
import { seedAccounts } from "../../src/core/ledger";
import { exportPublicEInvoiceOioUbl, transmitPublicEInvoicePeppol } from "../../src/core/public-einvoice";

const providerBinding = "a".repeat(64);
const payload = {
  invoiceType: "full" as const, issueDate: "2026-05-16", dueDate: "2026-06-16",
  seller: { name: "Synthetic Seller", address: "Testvej 1", vatOrCvr: "DK12345678" },
  buyer: { name: "Synthetic Buyer", address: "Testvej 2", eanNumber: "5790000000013", publicRecipient: true, buyerReference: "Reference ABC" },
  lines: [{ description: "Consulting", quantity: 1, unitPriceExVat: 100, lineTotalExVat: 100 }],
  totals: { netAmount: 100, vatRate: 0.25, vatAmount: 25, grossAmount: 125 }, currency: "DKK",
};

const client: Pick<DigisenseClient, "validateDocument" | "lookupParticipant"> = {
  async validateDocument() { return { ok: true, status: 200, data: { statusCode: 200, success: true, errors: [] } }; },
  async lookupParticipant() { return { ok: true, status: 200, data: { participantAbleToReceive: true, matchedServiceEndpointUrls: ["https://example.invalid"], matchedDocumentTypes: [{ scheme: "busdox-docid-qns", value: "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2::Invoice##OIOUBL-2.02::2.0" }] } }; },
};

describe("approved invoice evidence", () => {
  test("MCP PDF source cannot escape the company's draft store through a symlink", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-approved-path-"));
    const company = join(root, "company");
    ensureCompanyDirs(company);
    try {
      const outside = join(root, "outside.pdf");
      const inside = join(company, "invoices", "drafts", "inside.pdf");
      const link = join(company, "invoices", "drafts", "escape.pdf");
      writeFileSync(outside, "secret"); writeFileSync(inside, "synthetic"); symlinkSync(outside, link);
      expect(snapshotApprovedInvoiceSource(company, inside).bytes.toString()).toBe("synthetic");
      expect(() => snapshotApprovedInvoiceSource(company, outside)).toThrow();
      expect(() => snapshotApprovedInvoiceSource(company, link)).toThrow();
    } finally { rmSync(root, { recursive: true, force: true }); }
  });
  test("preflight binds the reviewed PDF and payload without reserving a number", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-approved-invoice-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);
    db.run("INSERT INTO companies (id, name, cvr) VALUES (1, 'Synthetic Seller', 'DK12345678')");
    try {
      const pdf = renderDocumentPdf({ title: "Synthetic", date: "2026-05-16", footer: "", html: "<main><p>2026-0001 2026-05-16 2026-06-16 Synthetic Seller Testvej 1 DK12345678 Synthetic Buyer Testvej 2 5790000000013 Reference ABC Consulting DKK 100,00 25% 25,00 125,00</p></main>" });
      const preflight = await preflightApprovedInvoice(db, { payload, pdfBytes: pdf, bilateralAttachmentAgreement: true, providerBinding, createdBy: "user:test" }, client);
      expect(preflight.ok).toBe(true);
      if (!preflight.ok) throw new Error(preflight.errors.join(", "));
      expect(db.query("SELECT COUNT(*) AS n FROM sequences").get()).toEqual({ n: 0 });
      expect(issueInvoice(db, root, { ...payload, approvedPdf: { sha256: "forged" } } as typeof payload).ok).toBe(false);
      expect(issueInvoice(db, root, payload, {}, { preflightId: preflight.preflightId, pdfBytes: pdf, visualReviewConfirmed: false, providerBinding }).ok).toBe(false);
      expect(issueInvoice(db, root, payload, {}, { preflightId: preflight.preflightId, pdfBytes: pdf, visualReviewConfirmed: true, providerBinding: "b".repeat(64) }).ok).toBe(false);
      expect(issueInvoice(db, root, payload, {}, { preflightId: preflight.preflightId, pdfBytes: Buffer.concat([pdf, Buffer.from("tampered")]), visualReviewConfirmed: true, providerBinding }).ok).toBe(false);
      db.run("INSERT INTO approved_invoice_preflights(id,invoice_no,payload_sha256,pdf_sha256,xml_sha256,buyer_ean,provider_binding,actor,created_at) SELECT 'expired',invoice_no,payload_sha256,pdf_sha256,xml_sha256,buyer_ean,provider_binding,actor,'2000-01-01 00:00:00' FROM approved_invoice_preflights WHERE id=?", [preflight.preflightId]);
      expect(issueInvoice(db, root, payload, {}, { preflightId: "expired", pdfBytes: pdf, visualReviewConfirmed: true, providerBinding }).ok).toBe(false);
      const rejected = issueInvoice(db, root, { ...payload, buyer: { ...payload.buyer, name: "Different Buyer" } }, {}, { preflightId: preflight.preflightId, pdfBytes: pdf, visualReviewConfirmed: true, providerBinding });
      expect(rejected.ok).toBe(false);
      expect(db.query("SELECT COUNT(*) AS n FROM sequences").get()).toEqual({ n: 0 });
      const issued = issueInvoice(db, root, payload, {}, { preflightId: preflight.preflightId, pdfBytes: pdf, visualReviewConfirmed: true, providerBinding });
      expect(issued.ok).toBe(true);
      expect(readFileSync(issued.pdfStoredPath!)).toEqual(pdf);
      expect(issued.pdfSha256).toBe(preflight.pdfSha256);
      expect(issueInvoice(db, root, payload, {}, { preflightId: preflight.preflightId, pdfBytes: pdf, visualReviewConfirmed: true, providerBinding }).ok).toBe(false);
      const missingPdfDb = new Proxy(db, { get(target, property, receiver) {
        if (property === "query") return (sql: string) => sql.includes("WHERE document_type = ? AND invoice_no = ?") ? { all: () => [] } : target.query(sql);
        const value = Reflect.get(target, property, receiver); return typeof value === "function" ? value.bind(target) : value;
      } });
      expect(renderIssuedInvoicePdf(missingPdfDb, root, { invoiceDocumentId: issued.documentId! }).ok).toBe(false);
      expect(JSON.parse(readFileSync(issued.storedPath!, "utf8")).approvedPdf).toMatchObject({ preflightId: preflight.preflightId, sha256: preflight.pdfSha256 });
      expect(postIssuedInvoiceToLedger(db, { invoiceDocumentId: issued.documentId! }).ok).toBe(true);
      const exported = exportPublicEInvoiceOioUbl(db, { invoiceDocumentId: issued.documentId!, companyRoot: root });
      expect(exported.ok).toBe(true);
      expect(exported.sha256).toBe(preflight.xmlSha256);
      const encoded = exported.xml!.match(/<cbc:EmbeddedDocumentBinaryObject[^>]*>([^<]+)</)?.[1];
      expect(Buffer.from(encoded!, "base64")).toEqual(pdf);
      let delivered = 0;
      const send = () => transmitPublicEInvoicePeppol(db, { invoiceDocumentId: issued.documentId!, companyRoot: root, providerBinding, accessPoint: { accessPointId: "synthetic", endpointUrl: "https://example.invalid", senderEndpointId: "synthetic-sender" } }, async ({ oioublXml }) => {
        delivered++;
        expect(oioublXml).toBe(exported.xml);
        return { ok: true, transmissionId: "synthetic-delivery", transmittedAt: "2026-05-16T10:00:00Z" };
      });
      const wrongProvider = await transmitPublicEInvoicePeppol(db, { invoiceDocumentId: issued.documentId!, companyRoot: root, providerBinding: "b".repeat(64), accessPoint: { accessPointId: "synthetic", endpointUrl: "https://example.invalid", senderEndpointId: "synthetic-sender" } }, async () => { throw new Error("must not transmit"); });
      expect(wrongProvider.ok).toBe(false);
      expect((await send()).ok).toBe(true);
      expect((await send()).ok).toBe(true);
      expect(delivered).toBe(1);
      writeFileSync(issued.pdfStoredPath!, "tampered");
      expect(exportPublicEInvoiceOioUbl(db, { invoiceDocumentId: issued.documentId!, companyRoot: root }).ok).toBe(false);
    } finally { db.close(); rmSync(root, { recursive: true, force: true }); }
  });

  test("rejects unreadable PDFs and absent recipient agreement before reserving a number", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-approved-invalid-"));
    const db = openDb(ensureCompanyDirs(root).db); migrate(db);
    db.run("INSERT INTO companies (id, name, cvr) VALUES (1, 'Synthetic Seller', 'DK12345678')");
    try {
      expect((await preflightApprovedInvoice(db, { payload, pdfBytes: Buffer.from("not a PDF"), bilateralAttachmentAgreement: false, providerBinding }, client)).ok).toBe(false);
      expect((await preflightApprovedInvoice(db, { payload, pdfBytes: Buffer.from("not a PDF"), bilateralAttachmentAgreement: true, providerBinding }, client)).ok).toBe(false);
      expect(db.query("SELECT COUNT(*) AS n FROM sequences").get()).toEqual({ n: 0 });
      expect(db.query("SELECT COUNT(*) AS n FROM approved_invoice_preflights").get()).toEqual({ n: 0 });
    } finally { db.close(); rmSync(root, { recursive: true, force: true }); }
  });
});
