import { describe, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { openDb, migrate } from "../../src/core/db";
import { ensureCompanyDirs } from "../../src/core/paths";
import { seedAccounts } from "../../src/core/ledger";
import { saveDigisenseCompany } from "../../src/core/efaktura/digisense-state";
import { saveDigisenseSecretConfig } from "../../src/core/efaktura/digisense-config";
import { renderDocumentPdf } from "../../src/design/pdf-render";
import { registerInvoiceIssuanceTools } from "../../src/mcp/tools/invoice/issuance";
import { registerEfakturaTools } from "../../src/mcp/tools/efaktura";
import { syntheticDigisenseFetch } from "../helpers/synthetic-digisense-fetch";

const payload = {
  invoiceType: "full" as const, issueDate: "2026-05-16", dueDate: "2026-06-16", currency: "DKK",
  seller: { name: "Synthetic Seller", address: "Synthetic Road 1", vatOrCvr: "DK12345678" },
  buyer: { name: "Synthetic Buyer", address: "Synthetic Road 2", eanNumber: "5790000000013", publicRecipient: true, buyerReference: "BUYER-ABC", orderReference: "ORDER-789" },
  lines: [{ description: "Synthetic service", quantity: 1, unitPriceExVat: 100, lineTotalExVat: 100 }],
  totals: { netAmount: 100, vatRate: 0.25, vatAmount: 25, grossAmount: 125 },
};
function setup() {
  const root = mkdtempSync(join(tmpdir(), "synthetic-approved-interface-"));
  const db = openDb(ensureCompanyDirs(root).db); migrate(db); seedAccounts(db);
  db.run("INSERT INTO companies (id,name,cvr) VALUES (1,'Synthetic Seller','DK12345678')");
  db.run("INSERT INTO bank_accounts (slug,name,currency,registration_no,account_no) VALUES ('synthetic','Synthetic bank','DKK','1234','5678901234')");
  saveDigisenseCompany(db, { companyKey: "synthetic-key", companyType: { type: "DK:CVR", id: "DK12345678" }, companyName: "Synthetic Seller" });
  saveDigisenseSecretConfig(root, { apiLicenseKey: "synthetic-key", environment: "test" });
  writeFileSync(join(root, "config", "policy.yaml"), "actor_allowlist:\n  - user:synthetic\n  - agent:synthetic/1\n");
  const pdf = renderDocumentPdf({ title: "Synthetic", date: payload.issueDate, footer: "", html: "<main><p>2026-0001 2026-05-16 2026-06-16 Synthetic Seller Synthetic Road 1 DK12345678 Synthetic Buyer Synthetic Road 2 5790000000013 BUYER-ABC ORDER-789 Synthetic service DKK 100,00 25% 25,00 125,00 1234 5678901234</p></main>" });
  const path = join(root, "invoices", "drafts", "synthetic.pdf"); writeFileSync(path, pdf);
  return { root, db, pdf, path, close() { db.close(); rmSync(root, { recursive: true, force: true }); } };
}
function checkArtifact(xml: string, hash: string) {
  expect(createHash("sha256").update(xml).digest("hex")).toBe(hash);
  expect(xml).toContain("<cbc:ID>5678901234</cbc:ID>");
  expect(xml).toContain("<cbc:ID>1234</cbc:ID>");
  expect(xml).toContain("<cbc:ID>BUYER-ABC</cbc:ID>");
  expect(xml).toContain("<cbc:ID>ORDER-789</cbc:ID>");
  expect(xml).not.toContain("EmbeddedDocumentBinaryObject");
}

describe("synthetic approved DigiSense interfaces", () => {
  test("CLI reviews, issues, posts and transmits the same hash-bound XML", async () => {
    const f = setup();
    try {
      const input = join(f.root, "invoices", "drafts", "invoice.json"), log = join(f.root, "fake-network.jsonl");
      writeFileSync(input, JSON.stringify(payload));
      const invoke = async (args: string[]) => {
        const proc = Bun.spawn(["bun", "run", "--preload", "./tests/helpers/synthetic-digisense-preload.ts", "src/cli.ts", ...args, "--company", f.root, "--actor", "user:synthetic"], { env: { ...process.env, SYNTHETIC_DIGISENSE_LOG: log }, stdout: "pipe", stderr: "pipe" });
        const [out, err, code] = await Promise.all([new Response(proc.stdout).text(), new Response(proc.stderr).text(), proc.exited]);
        if (code !== 0) throw new Error(`${out} ${err}`);
        return JSON.parse(out);
      };
      const pre = await invoke(["invoice", "preflight-approved-pdf", "--input", input, "--approved-pdf", f.path, "--attachment-policy", "none", "--confirm", "yes"]);
      const issued = await invoke(["invoice", "issue", "--input", input, "--approved-pdf", f.path, "--preflight-id", pre.preflightId, "--pdf-reviewed", "yes", "--confirm", "yes"]);
      await invoke(["invoice", "post", "--invoice-number", issued.invoiceNumber]);
      await invoke(["invoice", "transmit-digisense", "--invoice-number", issued.invoiceNumber, "--confirm", "yes"]);
      expect(readFileSync(issued.pdfStoredPath)).toEqual(f.pdf);
      const calls = readFileSync(log, "utf8").trim().split("\n").map((line) => JSON.parse(line));
      const validations = calls.filter((call) => call.route.endsWith("/validate-document"));
      const deliveries = calls.filter((call) => call.route.endsWith("/deliver-document"));
      expect(validations).toHaveLength(2); expect(deliveries).toHaveLength(1);
      expect(validations[0].body).toBe(deliveries[0].body); expect(validations[1].body).toBe(deliveries[0].body);
      checkArtifact(deliveries[0].body, pre.xmlSha256);
      expect((f.db.query("SELECT oioubl_sha256 FROM peppol_submissions").get() as any).oioubl_sha256).toBe(pre.xmlSha256);
    } finally { f.close(); }
  });
  test("MCP reviews, issues, posts and transmits the same hash-bound XML", async () => {
    const f = setup(); const originalFetch = globalThis.fetch;
    const calls: Array<{ route: string; body: string }> = [];
    globalThis.fetch = syntheticDigisenseFetch((route, body) => calls.push({ route, body })) as typeof fetch;
    try {
      const server = new McpServer({ name: "synthetic", version: "1" });
      server.server.getClientVersion = () => ({ name: "synthetic", version: "1" });
      registerInvoiceIssuanceTools(server); registerEfakturaTools(server);
      const registered = (server as any)._registeredTools;
      const call = async (name: string, args: object) => (await registered[name].handler({ company: f.root, ...args }, { signal: new AbortController().signal })).structuredContent;
      const pre = await call("invoice_preflight_approved_pdf", { payload, approvedPdfPath: f.path, attachmentPolicy: "none", confirm: true });
      if (!pre.ok) throw new Error(JSON.stringify(pre));
      const issued = await call("invoice_issue", { payload, approvedPdfPath: f.path, preflightId: pre.data.preflightId, pdfReviewed: true, confirm: true });
      if (!issued.ok) throw new Error(JSON.stringify(issued));
      const posted = await call("invoice_post", { documentId: issued.data.documentId, confirm: true });
      if (!posted.ok) throw new Error(JSON.stringify(posted));
      const sent = await call("efaktura_send", { documentId: issued.data.documentId, confirm: true });
      if (!sent.ok) throw new Error(JSON.stringify(sent));
      expect(readFileSync(issued.data.pdfStoredPath)).toEqual(f.pdf);
      const validations = calls.filter((call) => call.route.endsWith("/validate-document"));
      const deliveries = calls.filter((call) => call.route.endsWith("/deliver-document"));
      expect(validations).toHaveLength(2); expect(deliveries).toHaveLength(1);
      expect(validations[0].body).toBe(deliveries[0].body); expect(validations[1].body).toBe(deliveries[0].body);
      checkArtifact(deliveries[0].body, pre.data.xmlSha256);
      expect((f.db.query("SELECT oioubl_sha256 FROM peppol_submissions").get() as any).oioubl_sha256).toBe(pre.data.xmlSha256);
    } finally { globalThis.fetch = originalFetch; f.close(); }
  });
});
