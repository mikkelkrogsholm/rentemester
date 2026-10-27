import { describe, expect, test } from "bun:test";
import { mkdtempSync, readFileSync, rmSync, writeFileSync, symlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createHash } from "node:crypto";
import { openDb, migrate } from "../../src/core/db";
import { ensureCompanyDirs } from "../../src/core/paths";
import { approvedInvoiceTextErrors, preflightApprovedInvoice, snapshotApprovedInvoiceSource } from "../../src/core/approved-invoice-preflight";
import { issueInvoice } from "../../src/core/issued-invoices";
import type { DigisenseClient } from "../../src/core/efaktura/digisense-client";
import { renderDocumentPdf } from "../../src/design/pdf-render";
import { renderIssuedInvoicePdf } from "../../src/core/invoice-pdf";
import { postIssuedInvoiceToLedger } from "../../src/core/invoice-booking";
import { seedAccounts } from "../../src/core/ledger";
import { buildPublicEInvoiceOioUblXml, exportPublicEInvoiceOioUbl, transmitPublicEInvoicePeppol } from "../../src/core/public-einvoice";

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
  async lookupParticipant() { return { ok: true, status: 200, data: { participantAbleToReceive: true, matchedServiceEndpointUrls: ["https://example.invalid"], matchedDocumentTypes: [{ scheme: "busdox-docid-qns", value: "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2::Invoice##OIOUBL-2.1::2.1" }] } }; },
};

describe("approved invoice evidence", () => {
  test("validates and binds the actual 2.02 XML for a recipient registered only for that version", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-recipient-legacy-"));
    const db = openDb(ensureCompanyDirs(root).db); migrate(db);
    db.run("INSERT INTO companies (id, name, cvr) VALUES (1, 'Synthetic Seller', 'DK12345678')");
    try {
      const pdf = renderDocumentPdf({ title: "Synthetic", date: "2026-05-16", footer: "", html: "<main><p>2026-0001 2026-05-16 2026-06-16 Synthetic Seller Testvej 1 DK12345678 Synthetic Buyer Testvej 2 5790000000013 Reference ABC Consulting DKK 100,00 25% 25,00 125,00</p></main>" });
      const xmls: string[] = [];
      const legacyClient = {
        async validateDocument(xml: string) { xmls.push(xml); return { ok: true as const, status: 200, data: { statusCode: 200, success: true, errors: [] } }; },
        async lookupParticipant(query: { documentType?: string }) { return query.documentType?.includes("OIOUBL-2.1::2.1") ? { ok: false as const, error: { status: 404, message: "not found" } } : { ok: true as const, status: 200, data: { participantAbleToReceive: true, matchedServiceEndpointUrls: ["https://example.invalid"], matchedDocumentTypes: [{ scheme: "busdox-docid-qns", value: "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2::Invoice##OIOUBL-2.02::2.0" }] } }; },
      };
      const pre = await preflightApprovedInvoice(db, { payload, pdfBytes: pdf, bilateralAttachmentAgreement: true, providerBinding }, legacyClient);
      if (!pre.ok) throw new Error(pre.errors.join(", "));
      expect(xmls).toHaveLength(2);
      expect(xmls[0]).toContain("<cbc:CustomizationID>OIOUBL-2.1</cbc:CustomizationID>");
      expect(xmls[1]).toContain("<cbc:CustomizationID>OIOUBL-2.02</cbc:CustomizationID>");
      expect(pre.xmlSha256).toBe(createHash("sha256").update(xmls[1]).digest("hex"));
      const issued = issueInvoice(db, root, payload, {}, { preflightId: pre.preflightId, pdfBytes: pdf, visualReviewConfirmed: true, providerBinding });
      expect(issued.ok).toBe(true);
      expect(exportPublicEInvoiceOioUbl(db, { invoiceDocumentId: issued.documentId!, companyRoot: root }).xml).toBe(xmls[1]);
    } finally { db.close(); rmSync(root, { recursive: true, force: true }); }
  });
  test("legacy approved 2.02 XML remains byte-identical after new preflights move to 2.1", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-legacy-approved-"));
    const db = openDb(ensureCompanyDirs(root).db); migrate(db);
    db.run("INSERT INTO companies (id, name, cvr) VALUES (1, 'Synthetic Seller', 'DK12345678')");
    try {
      const pdf = renderDocumentPdf({ title: "Synthetic", date: "2026-05-16", footer: "", html: "<main><p>2026-0001 2026-05-16 2026-06-16 Synthetic Seller Testvej 1 DK12345678 Synthetic Buyer Testvej 2 5790000000013 Reference ABC Consulting DKK 100,00 25% 25,00 125,00</p></main>" });
      const pre = await preflightApprovedInvoice(db, { payload, pdfBytes: pdf, bilateralAttachmentAgreement: true, providerBinding }, client);
      if (!pre.ok) throw new Error(pre.errors.join(", "));
      const legacyXml = buildPublicEInvoiceOioUblXml(pre.invoiceNumber, pre.payload, pdf, "2.02");
      const legacyHash = createHash("sha256").update(legacyXml).digest("hex");
      expect(legacyHash).not.toBe(pre.xmlSha256);
      db.run("INSERT INTO approved_invoice_preflights(id,invoice_no,payload_sha256,pdf_sha256,xml_sha256,buyer_ean,provider_binding,actor) SELECT 'legacy',invoice_no,payload_sha256,pdf_sha256,?,buyer_ean,provider_binding,actor FROM approved_invoice_preflights WHERE id=?", [legacyHash, pre.preflightId]);
      const issued = issueInvoice(db, root, payload, {}, { preflightId: "legacy", pdfBytes: pdf, visualReviewConfirmed: true, providerBinding });
      expect(issued.ok).toBe(true);
      const exported = exportPublicEInvoiceOioUbl(db, { invoiceDocumentId: issued.documentId!, companyRoot: root });
      expect(exported.xml).toBe(legacyXml);
      expect(exported.sha256).toBe(legacyHash);
    } finally { db.close(); rmSync(root, { recursive: true, force: true }); }
  });
  test("accepts separated text fields but still rejects missing words and amounts", () => {
    const separated = "2026-0001 2026-05-16 2026-06-16 Synthetic Testvej Buyer Seller 1 DK12345678 2 5790000000013 Reference ABC Consulting DKK 100,00 25% 25,00 125,00";
    expect(approvedInvoiceTextErrors(separated, payload, "2026-0001")).toEqual([]);
    expect(approvedInvoiceTextErrors(separated.replace("Buyer", ""), payload, "2026-0001")).toContain("PDF is missing buyer name");
    expect(approvedInvoiceTextErrors(separated.replace("125,00", ""), payload, "2026-0001")).toContain("PDF is missing gross amount");
  });
  test("flags small differences in long descriptions for visual review and rejects missing numbers", () => {
    const description = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau upsilon phi chi psi omega";
    const data = { ...payload, lines: [{ ...payload.lines[0], description }] };
    const text = `2026-0001 2026-05-16 2026-06-16 Synthetic Seller Testvej 1 DK12345678 Synthetic Buyer Testvej 2 5790000000013 Reference ABC DKK 100,00 25% 25,00 125,00 ${description.replace("omega", "")}`;
    const warnings: string[] = [];
    expect(approvedInvoiceTextErrors(text, data, "2026-0001", warnings)).toEqual([]);
    expect(warnings).toEqual(["PDF line description requires visual review; words not found: omega"]);
    const withNumber = { ...data, lines: [{ ...data.lines[0], description: `${description} 89123` }] };
    expect(approvedInvoiceTextErrors(text, withNumber, "2026-0001")).toContain("PDF is missing line description");
  });
  test("description differences require explicit review before creating preflight evidence", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-description-review-"));
    const db = openDb(ensureCompanyDirs(root).db); migrate(db);
    db.run("INSERT INTO companies (id, name, cvr) VALUES (1, 'Synthetic Seller', 'DK12345678')");
    try {
      const description = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau upsilon phi chi psi omega";
      const data = { ...payload, lines: [{ ...payload.lines[0], description }] };
      const pdf = renderDocumentPdf({ title: "Synthetic", date: "2026-05-16", footer: "", html: `<main><p>2026-0001 2026-05-16 2026-06-16 Synthetic Seller Testvej 1 DK12345678 Synthetic Buyer Testvej 2 5790000000013 Reference ABC DKK 100,00 25% 25,00 125,00 ${description.replace("omega", "")}</p></main>` });
      const base = { payload: data, pdfBytes: pdf, bilateralAttachmentAgreement: true, providerBinding };
      const refused = await preflightApprovedInvoice(db, base, client);
      expect(refused.ok).toBe(false);
      expect(refused.warnings).toContain("PDF line description requires visual review; words not found: omega");
      expect(db.query("SELECT COUNT(*) AS n FROM approved_invoice_preflights").get()).toEqual({ n: 0 });
      const accepted = await preflightApprovedInvoice(db, { ...base, descriptionDifferencesReviewed: true }, client);
      expect(accepted.ok).toBe(true);
      expect(accepted.warnings).toContain("PDF line description requires visual review; words not found: omega");
      expect((await preflightApprovedInvoice(db, { ...base, bilateralAttachmentAgreement: false, descriptionDifferencesReviewed: true }, client)).ok).toBe(false);
    } finally { db.close(); rmSync(root, { recursive: true, force: true }); }
  });
  test("an entirely exempt invoice keeps its approved PDF through issue, posting and delivery", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-exempt-approved-"));
    const db = openDb(ensureCompanyDirs(root).db); migrate(db); seedAccounts(db);
    db.run("INSERT INTO companies (id, name, cvr) VALUES (1, 'Synthetic Seller', 'DK12345678')");
    try {
      const exempt = { ...payload, lines: [{ description: "Exempt synthetic service", quantity: 1, unitPriceExVat: 100, lineTotalExVat: 100, taxClassification: "exempt" as const }], totals: { netAmount: 100, vatAmount: 0, grossAmount: 100 } };
      const pdf = renderDocumentPdf({ title: "Synthetic", date: "2026-05-16", footer: "", html: "<main><p>2026-0001 2026-05-16 2026-06-16 Synthetic Seller Testvej 1 DK12345678 Synthetic Buyer Testvej 2 5790000000013 Reference ABC Exempt synthetic service DKK 100,00 0,00 momsfritaget</p></main>" });
      const pre = await preflightApprovedInvoice(db, { payload: exempt, pdfBytes: pdf, bilateralAttachmentAgreement: true, providerBinding }, client);
      if (!pre.ok) throw new Error(pre.errors.join(", "));
      const issued = issueInvoice(db, root, exempt, {}, { preflightId: pre.preflightId, pdfBytes: pdf, visualReviewConfirmed: true, providerBinding });
      expect(issued.ok).toBe(true);
      const posted = postIssuedInvoiceToLedger(db, { invoiceDocumentId: issued.documentId! });
      if (!posted.ok) throw new Error(posted.errors.join(", "));
      expect(db.query("SELECT vat_code, credit_amount FROM journal_lines WHERE vat_code IS NOT NULL").all()).toEqual([{ vat_code: "DK_SALE_EXEMPT", credit_amount: 100 }]);
      const exported = exportPublicEInvoiceOioUbl(db, { invoiceDocumentId: issued.documentId!, companyRoot: root });
      expect(exported.ok).toBe(true);
      expect(exported.xml).toContain("<cbc:UBLVersionID>2.1</cbc:UBLVersionID>");
      expect(exported.xml).toContain("<cbc:CustomizationID>OIOUBL-2.1</cbc:CustomizationID>");
      expect(exported.sha256).toBe(pre.xmlSha256);
      expect(exported.xml).toContain("ZeroRated");
      expect(Buffer.from(exported.xml!.match(/<cbc:EmbeddedDocumentBinaryObject[^>]*>([^<]+)</)![1], "base64")).toEqual(pdf);
      const sent = await transmitPublicEInvoicePeppol(db, { invoiceDocumentId: issued.documentId!, companyRoot: root, providerBinding, accessPoint: { accessPointId: "synthetic", endpointUrl: "https://example.invalid", senderEndpointId: "synthetic-sender" } }, async ({ oioublXml }) => {
        expect(oioublXml).toBe(exported.xml);
        return { ok: true, transmissionId: "synthetic-delivery", transmittedAt: "2026-05-16T10:00:00Z" };
      });
      expect(sent.ok).toBe(true);
      expect(sent.envelope).toContain("<Format>OIOUBL-2.1</Format>");
    } finally { db.close(); rmSync(root, { recursive: true, force: true }); }
  });
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
