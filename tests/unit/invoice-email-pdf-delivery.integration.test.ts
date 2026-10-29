import { describe, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { preflightApprovedInvoice } from "../../src/core/approved-invoice-preflight";
import { migrate, openDb } from "../../src/core/db";
import { sendInvoiceEmail, type EmailMessage, type EmailTransport } from "../../src/core/email";
import { issueInvoice } from "../../src/core/issued-invoices";
import { ensureCompanyDirs } from "../../src/core/paths";
import { renderDocumentPdf } from "../../src/design/pdf-render";

const payload = {
  invoiceType: "full" as const,
  vatTreatment: "standard" as const,
  issueDate: "2026-05-16",
  dueDate: "2026-06-16",
  seller: { name: "Synthetic Seller", address: "Testvej 1", vatOrCvr: "DK12345678" },
  buyer: { name: "Synthetic Buyer", address: "Testvej 2", vatOrCvr: "DK87654321" },
  lines: [
    { description: "Consulting", quantity: 1, unitPriceExVat: 100, lineTotalExVat: 100, taxClassification: "taxable" as const, vatRate: 0.25 },
    { description: "Training", quantity: 2, unitPriceExVat: 125, lineTotalExVat: 250, taxClassification: "taxable" as const, vatRate: 0.25 },
  ],
  totals: { netAmount: 350, vatRate: 0.25, vatAmount: 87.5, grossAmount: 437.5 },
  currency: "DKK",
};
const smtp = { host: "smtp.example.test", port: 587, fromAddress: "seller@example.test" };

async function reviewedInvoice() {
  const root = mkdtempSync(join(tmpdir(), "rentemester-reviewed-email-delivery-"));
  const db = openDb(ensureCompanyDirs(root).db);
  const close = () => { db.close(); rmSync(root, { recursive: true, force: true }); };
  try {
    migrate(db);
    db.run("INSERT INTO companies (id,name,cvr) VALUES (1,'Synthetic Seller','DK12345678')");
    const approvedBytes = renderDocumentPdf({
      title: "Synthetic approved invoice", date: payload.issueDate, footer: "",
      html: "<main><p>2026-0001 2026-05-16 2026-06-16 Synthetic Seller Testvej 1 DK12345678 Synthetic Buyer Testvej 2 DK87654321</p><p>Consulting 1 100,00 Training 2 125,00 250,00 DKK 350,00 25% 87,50 437,50</p></main>",
    });
    const review = await preflightApprovedInvoice(db, {
      payload, pdfBytes: approvedBytes, transport: "email", createdBy: "user:synthetic-reviewer",
    });
    if (!review.ok) throw new Error(review.errors.join(", "));
    expect(review).toMatchObject({ transport: "email", requiresVisualReview: true });
    expect(db.query("SELECT COUNT(*) AS n FROM email_send_log").get()).toEqual({ n: 0 });
    const issued = issueInvoice(db, root, payload, { createdBy: "user:synthetic-issuer" }, {
      transport: "email", preflightId: review.preflightId,
      pdfBytes: approvedBytes, visualReviewConfirmed: true,
    });
    if (!issued.ok) throw new Error(issued.errors.join(", "));
    expect(issued.pdfSha256).toBe(review.pdfSha256);
    expect(readFileSync(issued.pdfStoredPath!)).toEqual(approvedBytes);
    const send = (transport: EmailTransport) => sendInvoiceEmail(db, root, {
      invoiceDocumentId: issued.documentId!, kind: "invoice", to: "buyer@example.test", smtp, transport,
    });
    return { db, issued, approvedBytes, send, close };
  } catch (error) { close(); throw error; }
}

function captureTransport() {
  const messages: EmailMessage[] = [];
  const transport: EmailTransport = {
    send(message) { messages.push(message); return { ok: true, messageId: message.messageId }; },
  };
  return { transport, messages };
}

function attachedPdf(message: EmailMessage) {
  const boundary = message.rawMessage.match(/boundary="([^"]+)"/)?.[1];
  expect(boundary).toBeDefined();
  const attachments = message.rawMessage.split(`--${boundary}`)
    .filter(part => part.includes("Content-Type: application/pdf"));
  expect(attachments).toHaveLength(1);
  expect(attachments[0]).toContain('filename="2026-0001.pdf"');
  return Buffer.from(attachments[0]!.split("\r\n\r\n")[1]!.replace(/\s/g, ""), "base64");
}

describe("reviewed email PDF issuance through the existing SMTP adapter", () => {
  test("delivers the exact approved multiline 25% VAT PDF once and preserves the immutable snapshot", async () => {
    const f = await reviewedInvoice();
    try {
      const fake = captureTransport();
      const snapshot = readFileSync(f.issued.storedPath!);
      expect(f.send(fake.transport)).toMatchObject({ ok: true, duplicate: false, invoiceNumber: "2026-0001" });
      expect(fake.messages).toHaveLength(1);
      expect(fake.messages[0]!.to).toBe("buyer@example.test");
      const delivered = attachedPdf(fake.messages[0]!);
      expect(delivered).toEqual(f.approvedBytes);
      expect(delivered).toEqual(readFileSync(f.issued.pdfStoredPath!));
      expect(createHash("sha256").update(delivered).digest("hex")).toBe(f.issued.pdfSha256!);
      expect(f.send(fake.transport)).toMatchObject({ ok: true, duplicate: true, messageId: fake.messages[0]!.messageId });
      expect(fake.messages).toHaveLength(1);
      expect(f.db.query("SELECT invoice_document_id, message_id FROM email_send_log").all()).toEqual([
        { invoice_document_id: f.issued.documentId!, message_id: fake.messages[0]!.messageId },
      ]);
      expect(readFileSync(f.issued.storedPath!)).toEqual(snapshot);
      expect(() => f.db.run("UPDATE email_invoice_pdf_reviews SET pdf_sha256=?", ["a".repeat(64)])).toThrow("immutable");
    } finally { f.close(); }
  });

  for (const damage of ["tampered", "missing"] as const) {
    test(`${damage} issued evidence is rejected before any transport or receipt`, async () => {
      const f = await reviewedInvoice();
      try {
        const fake = captureTransport();
        const path = f.issued.pdfStoredPath!;
        const changed = Buffer.concat([f.approvedBytes, Buffer.from("\nchanged after issuance")]);
        if (damage === "tampered") writeFileSync(path, changed);
        else rmSync(path);
        const result = f.send(fake.transport);
        expect(result.ok).toBe(false);
        expect(result.errors.join(" ")).toContain("integrity verification");
        expect(fake.messages).toHaveLength(0);
        expect(f.db.query("SELECT COUNT(*) AS n FROM email_send_log").get()).toEqual({ n: 0 });
        expect(f.db.query("SELECT COUNT(*) AS n FROM audit_log WHERE event_type='invoice_email_send'").get()).toEqual({ n: 0 });
        if (damage === "tampered") expect(readFileSync(path)).toEqual(changed);
        else expect(existsSync(path)).toBe(false);
      } finally { f.close(); }
    });
  }

  test("fake SMTP rejection records no success; a retry still attaches the approved evidence", async () => {
    const f = await reviewedInvoice();
    try {
      const rejected: EmailMessage[] = [];
      const failure = f.send({ send(message) {
        rejected.push(message);
        return { ok: false, error: "synthetic transport refusal" };
      } });
      expect(failure.ok).toBe(false);
      expect(failure.errors.join(" ")).toContain("synthetic transport refusal");
      expect(rejected).toHaveLength(1);
      expect(attachedPdf(rejected[0]!)).toEqual(f.approvedBytes);
      expect(f.db.query("SELECT COUNT(*) AS n FROM email_send_log").get()).toEqual({ n: 0 });
      const fake = captureTransport();
      expect(f.send(fake.transport)).toMatchObject({ ok: true, duplicate: false });
      expect(fake.messages).toHaveLength(1);
      expect(fake.messages[0]!.rawMessage).toBe(rejected[0]!.rawMessage);
      expect(attachedPdf(fake.messages[0]!)).toEqual(f.approvedBytes);
    } finally { f.close(); }
  });
});
