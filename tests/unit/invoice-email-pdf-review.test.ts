import { describe, expect, test } from "bun:test";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { openDb, migrate } from "../../src/core/db";
import { ensureCompanyDirs } from "../../src/core/paths";
import { seedAccounts } from "../../src/core/ledger";
import { preflightApprovedInvoice } from "../../src/core/approved-invoice-preflight";
import { issueInvoice } from "../../src/core/issued-invoices";
import { renderIssuedInvoicePdf } from "../../src/core/invoice-pdf";
import { postIssuedInvoiceToLedger } from "../../src/core/invoice-booking";
import { exportPublicEInvoiceOioUbl } from "../../src/core/public-einvoice";
import { renderDocumentPdf } from "../../src/design/pdf-render";
import { registerInvoiceIssuanceTools } from "../../src/mcp/tools/invoice/issuance";

const payload = {
  invoiceType: "full" as const, vatTreatment: "standard" as const,
  issueDate: "2026-05-16", dueDate: "2026-06-16",
  seller: { name: "Synthetic Seller", address: "Testvej 1", vatOrCvr: "DK12345678" },
  buyer: { name: "Synthetic Buyer", address: "Testvej 2", vatOrCvr: "DK87654321", buyerReference: "Reference ABC" },
  lines: [
    { description: "Consulting", quantity: 1, unitPriceExVat: 100, lineTotalExVat: 100, taxClassification: "taxable" as const, vatRate: 0.25 },
    { description: "Training", quantity: 2, unitPriceExVat: 125, lineTotalExVat: 250, taxClassification: "taxable" as const, vatRate: 0.25 },
  ],
  totals: { netAmount: 350, vatRate: 0.25, vatAmount: 87.5, grossAmount: 437.5 }, currency: "DKK",
};
const pdf = () => renderDocumentPdf({ title: "Synthetic faktura", date: payload.issueDate, footer: "", html: "<main><p>2026-0001 2026-05-16 2026-06-16 Synthetic Seller Testvej 1 DK12345678 Synthetic Buyer Testvej 2 DK87654321 Reference ABC</p><p>Consulting 1 100,00 Training 2 125,00 250,00 DKK 350,00 25% 87,50 437,50</p></main>" });
function fixture() {
  const root = mkdtempSync(join(tmpdir(), "rentemester-email-pdf-"));
  const db = openDb(ensureCompanyDirs(root).db); migrate(db); seedAccounts(db);
  db.run("INSERT INTO companies (id,name,cvr) VALUES (1,'Synthetic Seller','DK12345678')");
  return { root, db, close() { db.close(); rmSync(root, { recursive: true, force: true }); } };
}

describe("email PDF review", () => {
  test("no EAN/provider: multiline 25% VAT, immutable review, canonical readback and posting", async () => {
    const f = fixture();
    try {
      const bytes = pdf();
      let calls = 0;
      const forbidden = async () => { calls++; throw new Error("provider must not be called"); };
      const pre = await preflightApprovedInvoice(f.db, { payload, pdfBytes: bytes, transport: "email", bilateralAttachmentAgreement: false, createdBy: "user:reviewer" }, { validateDocument: forbidden, lookupParticipant: forbidden });
      if (!pre.ok) throw new Error(pre.errors.join(", "));
      expect(calls).toBe(0);
      expect(Object.hasOwn(pre, "xmlSha256")).toBe(false);
      expect(Object.hasOwn(pre, "oioublVersion")).toBe(false);
      expect(pre).toMatchObject({ transport: "email", attachmentPolicy: "none", requiresVisualReview: true, invoiceNumber: "2026-0001" });
      expect(f.db.query("SELECT COUNT(*) AS n FROM sequences").get()).toEqual({ n: 0 });
      expect(f.db.query("SELECT actor FROM email_invoice_pdf_reviews WHERE id=?").get(pre.preflightId)).toMatchObject({ actor: expect.stringContaining("user:reviewer") });
      expect(() => f.db.run("UPDATE email_invoice_pdf_reviews SET pdf_sha256=? WHERE id=?", ["a".repeat(64), pre.preflightId])).toThrow("immutable");
      expect(() => f.db.run("DELETE FROM email_invoice_pdf_reviews WHERE id=?", [pre.preflightId])).toThrow("append-only");
      const approved = { transport: "email" as const, preflightId: pre.preflightId, pdfBytes: bytes, visualReviewConfirmed: true };
      expect(issueInvoice(f.db, f.root, payload, {}, { ...approved, pdfBytes: Buffer.concat([bytes, Buffer.from("changed")]) }).ok).toBe(false);
      expect(issueInvoice(f.db, f.root, { ...payload, buyer: { ...payload.buyer, name: "Changed Buyer" } }, {}, approved).ok).toBe(false);
      expect(issueInvoice(f.db, f.root, payload, {}, { ...approved, visualReviewConfirmed: false }).ok).toBe(false);
      expect(issueInvoice(f.db, f.root, payload, {}, { ...approved, transport: "digisense", providerBinding: "a".repeat(64) }).ok).toBe(false);
      f.db.run("INSERT INTO email_invoice_pdf_reviews SELECT 'expired',invoice_no,payload_sha256,pdf_sha256,actor,'2000-01-01 00:00:00' FROM email_invoice_pdf_reviews WHERE id=?", [pre.preflightId]);
      expect(issueInvoice(f.db, f.root, payload, {}, { ...approved, preflightId: "expired" }).ok).toBe(false);
      const issued = issueInvoice(f.db, f.root, payload, { createdBy: "user:issuer" }, approved);
      expect(issued.ok).toBe(true);
      expect(issued.pdfSha256).toBe(pre.pdfSha256);
      expect(readFileSync(issued.pdfStoredPath!)).toEqual(bytes);
      expect(JSON.parse(readFileSync(issued.storedPath!, "utf8")).approvedPdf).toEqual({ preflightId: pre.preflightId, sha256: pre.pdfSha256, transport: "email" });
      expect(renderIssuedInvoicePdf(f.db, f.root, { invoiceDocumentId: issued.documentId! }).ok).toBe(true);
      expect(postIssuedInvoiceToLedger(f.db, { invoiceDocumentId: issued.documentId! }).ok).toBe(true);
      expect(issueInvoice(f.db, f.root, payload, {}, approved).ok).toBe(false);
      expect(exportPublicEInvoiceOioUbl(f.db, { invoiceDocumentId: issued.documentId!, companyRoot: f.root }).errors.join(" ")).toContain("Email PDF review");
      expect(f.db.query("SELECT actor,message FROM audit_log WHERE event_type='invoice_issue'").get()).toMatchObject({ actor: expect.stringContaining("user:issuer"), message: expect.stringContaining(pre.preflightId) });
      writeFileSync(issued.pdfStoredPath!, "tampered");
      expect(renderIssuedInvoicePdf(f.db, f.root, { invoiceDocumentId: issued.documentId! }).ok).toBe(false);
      expect(readFileSync(issued.pdfStoredPath!, "utf8")).toBe("tampered");
    } finally { f.close(); }
  });

  test("transport, embedded policy and legacy EAN gates fail closed", async () => {
    const f = fixture();
    try {
      const base = { payload, pdfBytes: pdf(), bilateralAttachmentAgreement: false };
      expect((await preflightApprovedInvoice(f.db, base)).ok).toBe(false);
      expect((await preflightApprovedInvoice(f.db, { ...base, transport: "email", attachmentPolicy: "embedded", bilateralAttachmentAgreement: true })).ok).toBe(false);
      expect((await preflightApprovedInvoice(f.db, { ...base, transport: "smtp" as "email" })).ok).toBe(false);
      expect((await preflightApprovedInvoice(f.db, { ...base, transport: "email", pdfBytes: Buffer.from("invalid") })).ok).toBe(false);
      const client = { async validateDocument() { throw new Error("invalid EAN must fail first"); }, async lookupParticipant() { throw new Error("invalid EAN must fail first"); } };
      const legacy = await preflightApprovedInvoice(f.db, { ...base, attachmentPolicy: "none", providerBinding: "a".repeat(64) }, client);
      expect(legacy.ok).toBe(false);
      if (!legacy.ok) expect(legacy.errors.join(" ")).toContain("buyer.eanNumber");
    } finally { f.close(); }
  });

  test("email review with an existing EAN never grants OIOUBL approval", async () => {
    const f = fixture();
    try {
      const withEan = { ...payload, buyer: { ...payload.buyer, eanNumber: "5790000000013", publicRecipient: true } };
      const bytes = renderDocumentPdf({ title: "Synthetic", date: payload.issueDate, footer: "", html: "<main><p>2026-0001 2026-05-16 2026-06-16 Synthetic Seller Testvej 1 DK12345678 Synthetic Buyer Testvej 2 DK87654321 5790000000013 Reference ABC Consulting Training DKK 350,00 25% 87,50 437,50</p></main>" });
      const pre = await preflightApprovedInvoice(f.db, { payload: withEan, pdfBytes: bytes, transport: "email", bilateralAttachmentAgreement: false });
      if (!pre.ok) throw new Error(pre.errors.join(", "));
      const issued = issueInvoice(f.db, f.root, withEan, {}, { transport: "email", preflightId: pre.preflightId, pdfBytes: bytes, visualReviewConfirmed: true });
      expect(issued.ok).toBe(true);
      expect(exportPublicEInvoiceOioUbl(f.db, { invoiceDocumentId: issued.documentId!, companyRoot: f.root }).errors.join(" ")).toContain("Email PDF review");
    } finally { f.close(); }
  });

  test("competing issuance invalidates an unreserved review number", async () => {
    const f = fixture();
    try {
      const bytes = pdf();
      const pre = await preflightApprovedInvoice(f.db, { payload, pdfBytes: bytes, transport: "email", bilateralAttachmentAgreement: false });
      if (!pre.ok) throw new Error(pre.errors.join(", "));
      const otherDb = openDb(join(f.root, "data", "ledger.sqlite"));
      try { expect(issueInvoice(otherDb, f.root, payload).ok).toBe(true); } finally { otherDb.close(); }
      expect(issueInvoice(f.db, f.root, payload, {}, { transport: "email", preflightId: pre.preflightId, pdfBytes: bytes, visualReviewConfirmed: true }).ok).toBe(false);
      expect(f.db.query("SELECT COUNT(*) AS n FROM documents WHERE document_type='issued_invoice'").get()).toEqual({ n: 1 });
    } finally { f.close(); }
  });

  test("CLI concurrent approved issuance publishes exactly one canonical PDF", async () => {
    const f = fixture();
    try {
      writeFileSync(join(f.root, "config", "policy.yaml"), "actor_allowlist:\n  - user:synthetic\n");
      const input = join(f.root, "invoices", "drafts", "input.json"), pdfPath = join(f.root, "invoices", "drafts", "approved.pdf");
      const bytes = pdf(); writeFileSync(input, JSON.stringify(payload)); writeFileSync(pdfPath, bytes);
      const pre = await preflightApprovedInvoice(f.db, { payload, pdfBytes: bytes, transport: "email", bilateralAttachmentAgreement: false });
      if (!pre.ok) throw new Error(pre.errors.join(", "));
      const args = ["bun", "run", "src/cli.ts", "invoice", "issue", "--company", f.root, "--actor", "user:synthetic", "--input", input, "--approved-pdf", pdfPath, "--transport", "email", "--preflight-id", pre.preflightId, "--pdf-reviewed", "yes", "--confirm", "yes"];
      const invoke = async () => {
        const proc = Bun.spawn(args, { stdout: "pipe", stderr: "pipe" });
        const [out, err, code] = await Promise.all([new Response(proc.stdout).text(), new Response(proc.stderr).text(), proc.exited]);
        if (!out) throw new Error(err);
        return { code, result: JSON.parse(out) };
      };
      const results = await Promise.all([invoke(), invoke()]);
      expect(results.map((r) => r.code).sort()).toEqual([0, 1]);
      const success = results.find((r) => r.result.ok)!.result;
      expect(readFileSync(success.pdfStoredPath)).toEqual(bytes);
      expect(f.db.query("SELECT COUNT(*) AS n FROM documents WHERE document_type='issued_invoice'").get()).toEqual({ n: 1 });
    } finally { f.close(); }
  });

  test("CLI and MCP expose local email preflight, review confirmation and canonical issuance", async () => {
    const f = fixture();
    try {
      writeFileSync(join(f.root, "config", "policy.yaml"), "actor_allowlist:\n  - user:synthetic\n  - agent:synthetic/1\n");
      const input = join(f.root, "invoices", "drafts", "input.json"), pdfPath = join(f.root, "invoices", "drafts", "approved.pdf");
      writeFileSync(input, JSON.stringify(payload)); writeFileSync(pdfPath, pdf());
      const invoke = async (args: string[]) => {
        const proc = Bun.spawn(["bun", "run", "src/cli.ts", ...args, "--company", f.root, "--actor", "user:synthetic"], { stdout: "pipe", stderr: "pipe" });
        const [out, err, code] = await Promise.all([new Response(proc.stdout).text(), new Response(proc.stderr).text(), proc.exited]);
        if (!out) throw new Error(err);
        return { code, result: JSON.parse(out) };
      };
      const cli = await invoke(["invoice", "preflight-approved-pdf", "--input", input, "--approved-pdf", pdfPath, "--transport", "email", "--confirm", "yes"]);
      expect(cli.code).toBe(0); expect(cli.result.transport).toBe("email");
      const server = new McpServer({ name: "synthetic", version: "1" });
      server.server.getClientVersion = () => ({ name: "synthetic", version: "1" });
      registerInvoiceIssuanceTools(server);
      const registered = (server as any)._registeredTools;
      const call = async (name: string, args: object) => (await registered[name].handler({ company: f.root, ...args }, { signal: new AbortController().signal })).structuredContent;
      for (const name of ["invoice_issue", "invoice_preflight_approved_pdf"]) {
        expect(registered[name].inputSchema.shape.transport.safeParse("email").success).toBe(true);
        expect(registered[name].inputSchema.shape.transport.safeParse("smtp").success).toBe(false);
      }
      const args = { payload, approvedPdfPath: pdfPath, transport: "email", confirm: true };
      expect((await call("invoice_preflight_approved_pdf", { ...args, confirm: false })).ok).toBe(false);
      const pre = await call("invoice_preflight_approved_pdf", args);
      expect(pre.ok).toBe(true);
      const approved = { ...args, preflightId: pre.data.preflightId, pdfReviewed: true };
      expect((await call("invoice_issue", { ...approved, pdfReviewed: false })).ok).toBe(false);
      const issued = await call("invoice_issue", approved);
      expect(issued.ok).toBe(true);
      expect(readFileSync(issued.data.pdfStoredPath)).toEqual(readFileSync(pdfPath));
      expect((await invoke(["invoice", "issue", "--input", input, "--approved-pdf", pdfPath, "--transport", "email", "--preflight-id", cli.result.preflightId, "--pdf-reviewed", "yes", "--confirm", "yes"])).result.ok).toBe(false);
    } finally { f.close(); }
  });
});
