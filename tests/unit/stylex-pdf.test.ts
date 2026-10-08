import { describe, expect, test } from "bun:test";
import { mkdtempSync, readdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { buildIssuedInvoicePdf } from "../../src/core/invoice-pdf";
import { buildStatementPdf } from "../../src/server/data/statement-pdf";
import { renderDocumentPdf } from "../../src/design/pdf-render";
import { parsePdfBytes } from "../../src/core/document-pdf-parser";
import { issueInvoice } from "../../src/core/issued-invoices";
import { ensureCompanyDirs } from "../../src/core/paths";
import { migrate, openDb } from "../../src/core/db";

const payload = {
  invoiceType: "full" as const, vatTreatment: "standard" as const,
  issueDate: "2026-05-16", currency: "DKK", seller: { name: "Synthetic ApS", address: "Æblevej 3, København Ø", vatOrCvr: "DK12345678" },
  buyer: { name: "Køber A/S", address: "Bærvej 9, Århus" },
  lines: [{ description: "Rådgivning", quantity: 1, unitPriceExVat: 100, lineTotalExVat: 100 }],
  totals: { netAmount: 100, vatRate: 0.25, vatAmount: 25, grossAmount: 125 },
};

describe("StyleX Takumi PDF boundary", () => {
  test("local Latin-Extended faces preserve legal names and unsupported scripts fail closed", async () => {
    const bytes = buildIssuedInvoicePdf({ ...payload, seller: { ...payload.seller, name: "Żółć ApS" }, buyer: { ...payload.buyer, name: "Łódź S.A." } });
    const parsed = await parsePdfBytes(bytes);
    const text = parsed.pages.map(page => page.text).join("\n");
    expect(text).toContain("Żółć ApS");
    expect(text).toContain("Łódź S.A.");
    for (const name of ["李商", "Ж"]) {
      expect(() => buildIssuedInvoicePdf({ ...payload, seller: { ...payload.seller, name } })).toThrow("PDF renderer failed");
    }
  });
  test("repeats table headers, wraps full descriptions and preserves legal trailing content on multiple pages", async () => {
    const longDescription = "Lang rådgivningstekst med æ ø å, der skal ombrydes uden afkortning. ".repeat(5);
    const input = { ...payload, invoiceNumber: "2026-0001", lines: Array.from({ length: 75 }, (_, index) => ({ ...payload.lines[0]!, description: index === 0 ? longDescription : `Linje ${index + 1}` })), reverseChargeNote: "Omvendt betalingspligt — køber afregner moms", deliveryPeriodStart: "2026-05-01", deliveryPeriodEnd: "2026-05-15" };
    const bytes = buildIssuedInvoicePdf(input);
    expect(bytes.equals(buildIssuedInvoicePdf(input))).toBe(true);
    const parsed = await parsePdfBytes(bytes);
    expect(parsed.status).toBe("ok");
    expect(parsed.pages.length).toBeGreaterThan(2);
    const text = parsed.pages.map(page => page.text).join("\n");
    expect(text.replace(/\s+/g, " ")).toContain(longDescription.trim().replace(/\s+/g, " "));
    expect(text).toContain("Linje 75");
    expect(text).toContain("125,00 DKK");
    expect(text).toContain("Leveringsperiode: 2026-05-01 – 2026-05-15");
    expect(text).toContain("Omvendt betalingspligt — køber afregner moms");
    const tablePages = parsed.pages.filter(page => /Linje \d+/.test(page.text));
    for (const page of tablePages) expect(page.text).toContain("Beskrivelse Antal Stykpris Beløb");
    for (const page of parsed.pages) expect(page.text).toContain(`Side ${page.pageNumber} af ${parsed.pages.length}`);
  });

  test("statements preserve every row across pages and deterministic metadata", async () => {
    const input = { title: "Resultatopgørelse", company: { name: "Synthetic Økonomi ApS", cvr: "12345678", currency: "DKK" }, yearLabel: "2025/2026", generatedAtIsoDate: "2026-05-16", rows: Array.from({ length: 130 }, (_, i) => ({ kind: i === 129 ? "total" as const : "line" as const, label: `Regnskabslinje ${i + 1}`, amount: `${i + 1},00 DKK` })) };
    const bytes = buildStatementPdf(input);
    expect(bytes.equals(buildStatementPdf(input))).toBe(true);
    const parsed = await parsePdfBytes(bytes);
    expect(parsed.status).toBe("ok");
    expect(parsed.pages.length).toBeGreaterThan(2);
    expect(parsed.pages.map(page => page.text).join("\n")).toContain("Regnskabslinje 130 130,00 DKK");
  });

  test("timeouts kill the child and invalid input cannot become PDF output", () => {
    expect(() => renderDocumentPdf({ html: "hello", title: "Synthetic", date: "2026-05-16", footer: "" }, { timeoutMs: 1 })).toThrow("timed out");
    expect(() => renderDocumentPdf({ html: "hello", title: "Synthetic", date: "invalid", footer: "" })).toThrow("failed");
    expect(() => renderDocumentPdf({ html: "hello", title: "Synthetic", date: "2026-05-16", footer: "" }, { timeoutMs: 0 })).toThrow("timeout");
  });

  test("renderer failure rolls back invoice documents, artifact publication and number reservation", () => {
    const root = mkdtempSync(join(tmpdir(), "stylex-pdf-rollback-"));
    const paths = ensureCompanyDirs(root), db = openDb(paths.db);
    try {
      migrate(db);
      expect(() => issueInvoice(db, root, { ...payload, lines: [{ ...payload.lines[0]!, description: "x".repeat(8 * 1024 * 1024) }] })).toThrow("PDF render input exceeds");
      expect(db.query("SELECT COUNT(*) AS n FROM documents").get()).toEqual({ n: 0 });
      expect(readdirSync(paths.invoicesIssued)).toHaveLength(0);
      const next = issueInvoice(db, root, payload);
      expect(next.ok).toBe(true);
      expect(next.invoiceNumber).toBe("2026-0001");
    } finally { db.close(); rmSync(root, { recursive: true, force: true }); }
  });
});
