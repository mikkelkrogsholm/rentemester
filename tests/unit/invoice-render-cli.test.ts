// Tests: src/cli/invoice.ts, src/cli.ts (invoice render CLI)
import { describe, expect, test } from "bun:test";
import { existsSync, mkdtempSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { buildIssuedInvoicePdf, renderIssuedInvoicePdf } from "../../src/core/invoice-pdf";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { issueInvoice } from "../../src/core/issued-invoices";
import { addBankAccount } from "../../src/core/bank";

import { cleanupDir } from "../helpers/cleanup";
/** Extract every PDF literal-string draw operation `( ... ) Tj` from a content
 *  stream so tests can assert on the rendered text regardless of positioning. */
function pdfStrings(pdf: Uint8Array): string[] {
  const text = Buffer.from(pdf).toString("latin1");
  const out: string[] = [];
  const re = /\(((?:[^()\\]|\\.)*)\) Tj/g;
  let match: RegExpExecArray | null;
  while ((match = re.exec(text)) !== null) {
    out.push(match[1].replace(/\\([()\\])/g, "$1"));
  }
  return out;
}

describe("invoice PDF rendering", () => {
  test("paginates long invoices without dropping totals or the reverse-charge note", () => {
    const lines = Array.from({ length: 80 }, (_, i) => ({
      description: `Linje ${i + 1}`,
      quantity: 1,
      unitPriceExVat: 100,
      lineTotalExVat: 100,
    }));
    const pdf = buildIssuedInvoicePdf({
      invoiceNumber: "2026-9999",
      issueDate: "2026-05-16",
      dueDate: "2026-06-15",
      currency: "DKK",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9", vatOrCvr: "DK87654321" },
      lines,
      totals: { netAmount: 8000, vatAmount: 2000, grossAmount: 10000 },
      reverseChargeNote: "Omvendt betalingspligt - koeber afregner moms",
    } as any);
    const strings = pdfStrings(pdf);

    // Trailing legally-required content must survive into the rendered PDF.
    expect(strings).toContain("Total");
    // #225: customer-facing amounts use Danish number format (10.000,00).
    expect(strings).toContain("10.000,00 DKK");
    expect(strings.join("\n")).toContain("Omvendt betalingspligt");
    // Pagination produces more than one page object.
    const count = Buffer.from(pdf).toString("latin1").match(/\/Count (\d+)/);
    expect(count).not.toBeNull();
    expect(Number(count![1])).toBeGreaterThan(1);
  });

  test("renders Danish characters correctly via WinAnsi encoding", () => {
    const pdf = buildIssuedInvoicePdf({
      invoiceNumber: "2026-0007",
      issueDate: "2026-05-16",
      currency: "DKK",
      seller: { name: "Smør & Brød ApS", address: "Æblevej 3, 2100 København Ø", vatOrCvr: "DK11112222" },
      buyer: { name: "Køber A/S", address: "Bærvej 9, 8000 Århus", vatOrCvr: "DK33334444" },
      lines: [{ description: "Rådgivning på dansk", quantity: 1, unitPriceExVat: 500, lineTotalExVat: 500 }],
      totals: { netAmount: 500, vatRate: 0.25, vatAmount: 125, grossAmount: 625 },
    } as any);
    const text = Buffer.from(pdf).toString("latin1");

    // Headings keep their real Danish letters — no ASCII mangling.
    expect(text).toContain("SÆLGER");
    expect(text).toContain("KØBER");
    expect(text).not.toContain("Saelger");
    expect(text).not.toContain("Koeber");
    // æ ø å Æ Ø Å survive into the content stream as single WinAnsi bytes.
    const strings = pdfStrings(pdf).join("\n");
    expect(strings).toContain("Smør & Brød ApS");
    expect(strings).toContain("Æblevej 3, 2100 København Ø");
    expect(strings).toContain("Rådgivning på dansk");
    // The fonts must declare WinAnsiEncoding so viewers map those bytes right.
    expect(text).toContain("/Encoding /WinAnsiEncoding");
  });

  test("includes payment details (account number / IBAN) when supplied", () => {
    const pdf = buildIssuedInvoicePdf({
      invoiceNumber: "2026-0008",
      issueDate: "2026-05-16",
      currency: "DKK",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9", vatOrCvr: "DK87654321" },
      lines: [{ description: "Ydelse", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
      payment: {
        bankName: "Danske Bank",
        registrationNo: "1234",
        accountNo: "0001234567",
        iban: "DK5000400440116243",
      },
    } as any);
    const strings = pdfStrings(pdf).join("\n");

    expect(strings).toContain("BETALING");
    expect(strings).toContain("Danske Bank");
    expect(strings).toContain("Reg.nr. 1234");
    expect(strings).toContain("Kontonr. 0001234567");
    expect(strings).toContain("IBAN: DK5000400440116243");
  });

  test("omits the payment block when no payment details are supplied", () => {
    const pdf = buildIssuedInvoicePdf({
      invoiceNumber: "2026-0009",
      issueDate: "2026-05-16",
      currency: "DKK",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9", vatOrCvr: "DK87654321" },
      lines: [{ description: "Ydelse", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
    } as any);
    expect(pdfStrings(pdf).join("\n")).not.toContain("BETALING");
  });

  test("renders line and total amounts in Danish number format (#225)", () => {
    const pdf = buildIssuedInvoicePdf({
      invoiceNumber: "2026-0011",
      issueDate: "2026-05-16",
      currency: "DKK",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9", vatOrCvr: "DK87654321" },
      lines: [{ description: "Konsulentydelse", quantity: 8, unitPriceExVat: 1250.5, lineTotalExVat: 10004 }],
      totals: { netAmount: 10004, vatRate: 0.25, vatAmount: 2501, grossAmount: 12505 },
    } as any);
    const strings = pdfStrings(pdf);

    // Danish format: thousands grouped with '.', decimals after ','.
    expect(strings).toContain("1.250,50"); // unit price
    expect(strings).toContain("10.004,00"); // line total
    expect(strings).toContain("12.505,00 DKK"); // gross total
    // The English "1000.00" / "10004.00" form must NOT appear.
    expect(strings.join("\n")).not.toContain("10004.00");
    expect(strings.join("\n")).not.toContain("1250.50");
  });

  test("the PDF footer uses an ASCII separator — no mojibake (#225)", () => {
    const pdf = buildIssuedInvoicePdf({
      invoiceNumber: "2026-0012",
      issueDate: "2026-05-16",
      currency: "DKK",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9", vatOrCvr: "DK87654321" },
      lines: [{ description: "Ydelse", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
    } as any);
    const strings = pdfStrings(pdf);
    const footer = strings.find((s) => s.includes("Side"));
    expect(footer).toBe("Faktura 2026-0012 - Side 1 af 1");
    // No broken/exotic glyph in the footer.
    expect(footer).not.toContain("?");
    expect(footer).not.toContain("·");
  });

  test("is deterministic: identical payloads produce byte-identical PDFs", () => {
    const payload = {
      invoiceNumber: "2026-0010",
      issueDate: "2026-05-16",
      dueDate: "2026-06-15",
      currency: "DKK",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9", vatOrCvr: "DK87654321" },
      lines: [{ description: "Ydelse", quantity: 2, unitPriceExVat: 750, lineTotalExVat: 1500 }],
      totals: { netAmount: 1500, vatRate: 0.25, vatAmount: 375, grossAmount: 1875 },
      payment: { bankName: "Danske Bank", registrationNo: "1234", accountNo: "0001234567" },
    } as any;
    const a = buildIssuedInvoicePdf(payload);
    const b = buildIssuedInvoicePdf(payload);
    expect(Buffer.from(a).equals(Buffer.from(b))).toBe(true);
  });
});

describe("renderIssuedInvoicePdf — ledger payment details", () => {
  test("sources bank account / IBAN from the ledger's bank_accounts table", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-render-pay-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);

    addBankAccount(db, {
      name: "Drift",
      bankName: "Danske Bank",
      registrationNo: "1234",
      accountNo: "0001234567",
      iban: "DK5000400440116243",
      currency: "DKK",
    });

    const issued = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-16",
      invoiceNumber: "2026-0001",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
      currency: "DKK",
    });
    expect(issued.ok).toBe(true);

    const render = renderIssuedInvoicePdf(db, root, { invoiceDocumentId: issued.documentId! });
    expect(render.ok).toBe(true);

    const strings = pdfStrings(readFileSync(render.storedPath!)).join("\n");
    expect(strings).toContain("BETALING");
    expect(strings).toContain("Danske Bank");
    expect(strings).toContain("Reg.nr. 1234");
    expect(strings).toContain("Kontonr. 0001234567");
    expect(strings).toContain("IBAN: DK5000400440116243");

    db.close();
    cleanupDir(root);
  });
});

describe("invoice render CLI", () => {
  test("renders a deterministic PDF for an issued invoice", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-render-cli-"));
    const company = join(root, "company");

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    const issue = Bun.spawn(["bun", "run", "src/cli.ts", "invoice", "issue", "--company", company, "--input", "examples/full-invoice.dk.json"], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });
    const issueStdout = await new Response(issue.stdout).text();
    const issueExitCode = await issue.exited;
    expect(issueExitCode).toBe(0);
    const issued = JSON.parse(issueStdout);
    const originalPdf = readFileSync(issued.pdfStoredPath, "latin1");
    expect(existsSync(issued.pdfStoredPath)).toBe(true);

    const render = Bun.spawn(["bun", "run", "src/cli.ts", "invoice", "render", "--company", company, "--invoice-number", issued.invoiceNumber], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });
    const stdout = await new Response(render.stdout).text();
    const stderr = await new Response(render.stderr).text();
    const exitCode = await render.exited;
    const rerenderedPdf = readFileSync(issued.pdfStoredPath, "latin1");

    cleanupDir(root);
    expect({ exitCode, stderr }).toEqual({ exitCode: 0, stderr: "" });
    const parsed = JSON.parse(stdout);
    expect(parsed.ok).toBe(true);
    expect(parsed.invoiceNumber).toBe(issued.invoiceNumber);
    expect(rerenderedPdf).toBe(originalPdf);
  });
});
