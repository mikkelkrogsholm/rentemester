// Tests: src/core/company.ts (company profile), src/core/issued-invoices.ts,
// src/cli/init.ts, src/cli/company.ts — #221 captures the company's own
// identity + payment details once so they flow onto every issued invoice.
import { describe, expect, test } from "bun:test";
import { mkdtempSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs, companyPaths } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import {
  getCompanySettings,
  setCompanyProfile,
  initialiseCompanyVolume,
} from "../../src/core/company";
import { issueInvoice } from "../../src/core/issued-invoices";
import { readIssuedInvoicePdfText } from "../../src/core/invoice-pdf";

import { cleanupDir } from "../helpers/cleanup";
/** Extract every PDF literal-string draw operation `( ... ) Tj` so a test can
 *  assert on the rendered text regardless of positioning. */
function pdfStrings(pdf: Uint8Array | Buffer): string[] {
  const text = Buffer.from(pdf).toString("latin1");
  const out: string[] = [];
  const re = /\(((?:[^()\\]|\\.)*)\) Tj/g;
  let match: RegExpExecArray | null;
  while ((match = re.exec(text)) !== null) {
    out.push(match[1].replace(/\\([()\\])/g, "$1"));
  }
  return out;
}

function tmpRoot(label: string) {
  return mkdtempSync(join(tmpdir(), `rentemester-${label}-`));
}

async function runCli(args: string[], env?: Record<string, string>) {
  const proc = Bun.spawn(["bun", "run", "src/cli.ts", ...args], {
    cwd: process.cwd(),
    stdout: "pipe",
    stderr: "pipe",
    env: { ...process.env, RENTEMESTER_COMPANY: "", RENTEMESTER_WORKSPACE: "", ...env },
  });
  const stdout = await new Response(proc.stdout).text();
  const stderr = await new Response(proc.stderr).text();
  const exitCode = await proc.exited;
  return { stdout, stderr, exitCode };
}

describe("company profile — captured once, flows onto every invoice (#221)", () => {
  test("a profile set once supplies seller identity on the issued invoice without re-typing", () => {
    const root = tmpRoot("profile-seller");
    try {
      // Company initialised, then the profile edited once.
      initialiseCompanyVolume(root, { name: "Rentemester ApS" });
      const db = openDb(companyPaths(root).db);

      const profile = setCompanyProfile(db, {
        name: "Bogholderiet ApS",
        cvr: "DK12345678",
        address: "Hovedgaden 1",
        postalCode: "2100",
        city: "København Ø",
        paymentTermsDays: 8,
      });
      expect(profile.ok).toBe(true);
      expect(profile.updatedFields).toContain("cvr");
      expect(profile.updatedFields).toContain("address");

      // The owner issues an invoice with NO seller block at all.
      const issued = issueInvoice(db, root, {
        invoiceType: "full",
        vatTreatment: "standard",
        issueDate: "2026-05-16",
        buyer: { name: "Kunde A/S", address: "Købervej 9" },
        lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
        totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
        currency: "DKK",
      });
      expect(issued.ok).toBe(true);

      // Seller identity was filled from the stored profile — not re-typed.
      const stored = JSON.parse(readFileSync(issued.storedPath!, "utf8"));
      expect(stored.seller.name).toBe("Bogholderiet ApS");
      expect(stored.seller.vatOrCvr).toBe("DK12345678");
      expect(stored.seller.address).toBe("Hovedgaden 1, 2100 København Ø");
      // The due date defaulted from the company's payment terms (issue + 8).
      expect(stored.dueDate).toBe("2026-05-24");

      db.close();
    } finally {
      cleanupDir(root);
    }
  });

  test("an explicit seller value on the payload always wins over the profile", () => {
    const root = tmpRoot("profile-seller-override");
    try {
      initialiseCompanyVolume(root, {
        name: "Profile ApS",
        cvr: "DK11112222",
        address: "Profilvej 1",
      });
      const db = openDb(companyPaths(root).db);

      const issued = issueInvoice(db, root, {
        invoiceType: "full",
        vatTreatment: "standard",
        issueDate: "2026-05-16",
        seller: { name: "Explicit ApS", address: "Andenvej 9", vatOrCvr: "DK99998888" },
        buyer: { name: "Kunde A/S", address: "Købervej 9" },
        lines: [{ description: "Ydelse", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
        totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
        currency: "DKK",
      });
      expect(issued.ok).toBe(true);

      const stored = JSON.parse(readFileSync(issued.storedPath!, "utf8"));
      expect(stored.seller).toEqual({
        name: "Explicit ApS",
        address: "Andenvej 9",
        vatOrCvr: "DK99998888",
      });
      db.close();
    } finally {
      cleanupDir(root);
    }
  });

  test("payment details captured at init appear on the at-issue invoice PDF", () => {
    const root = tmpRoot("profile-payment-init");
    try {
      // init captures the company's bank account once.
      initialiseCompanyVolume(root, {
        name: "Rentemester ApS",
        cvr: "DK12345678",
        address: "Testvej 1",
        payment: {
          bankName: "Danske Bank",
          registrationNo: "1234",
          accountNo: "0001234567",
          iban: "DK5000400440116243",
        },
      });
      const db = openDb(companyPaths(root).db);

      const issued = issueInvoice(db, root, {
        invoiceType: "full",
        vatTreatment: "standard",
        issueDate: "2026-05-16",
        buyer: { name: "Kunde A/S", address: "Købervej 9" },
        lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
        totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
        currency: "DKK",
      });
      expect(issued.ok).toBe(true);

      // The PDF built AT ISSUE TIME — not only `invoice render` — carries the
      // BETALING block so the customer knows where to pay.
      const pdfText = readIssuedInvoicePdfText(issued.pdfStoredPath!);
      const strings = pdfStrings(readFileSync(issued.pdfStoredPath!)).join("\n");
      expect(pdfText.startsWith("%PDF-")).toBe(true);
      expect(strings).toContain("BETALING");
      expect(strings).toContain("Danske Bank");
      expect(strings).toContain("Reg.nr. 1234  Kontonr. 0001234567");
      expect(strings).toContain("IBAN: DK5000400440116243");

      // The persisted snapshot also carries the payment details so a later
      // `invoice render` reproduces the same payment block.
      const stored = JSON.parse(readFileSync(issued.storedPath!, "utf8"));
      expect(stored.payment.bankName).toBe("Danske Bank");
      expect(stored.payment.accountNo).toBe("0001234567");

      db.close();
    } finally {
      cleanupDir(root);
    }
  });

  test("payment details added via the editable profile flow onto a later invoice + its PDF", () => {
    const root = tmpRoot("profile-payment-edit");
    try {
      initialiseCompanyVolume(root, { name: "Rentemester ApS", cvr: "DK12345678", address: "Testvej 1" });
      const db = openDb(companyPaths(root).db);

      const profile = setCompanyProfile(db, {
        payment: { bankName: "Nordea", registrationNo: "5678", accountNo: "0009876543" },
      });
      expect(profile.ok).toBe(true);
      expect(profile.updatedFields).toContain("payment");

      const issued = issueInvoice(db, root, {
        invoiceType: "full",
        vatTreatment: "standard",
        issueDate: "2026-05-16",
        buyer: { name: "Kunde A/S", address: "Købervej 9" },
        lines: [{ description: "Ydelse", quantity: 1, unitPriceExVat: 500, lineTotalExVat: 500 }],
        totals: { netAmount: 500, vatRate: 0.25, vatAmount: 125, grossAmount: 625 },
        currency: "DKK",
      });
      expect(issued.ok).toBe(true);

      const strings = pdfStrings(readFileSync(issued.pdfStoredPath!)).join("\n");
      expect(strings).toContain("BETALING");
      expect(strings).toContain("Nordea");
      expect(strings).toContain("Reg.nr. 5678  Kontonr. 0009876543");

      db.close();
    } finally {
      cleanupDir(root);
    }
  });

  test("setCompanyProfile rejects an invalid CVR and an out-of-range payment term", () => {
    const root = tmpRoot("profile-invalid");
    try {
      initialiseCompanyVolume(root, { name: "Rentemester ApS" });
      const db = openDb(companyPaths(root).db);

      const badCvr = setCompanyProfile(db, { cvr: "12" });
      expect(badCvr.ok).toBe(false);
      expect(badCvr.errors[0]).toContain("CVR");

      const badTerms = setCompanyProfile(db, { paymentTermsDays: 999 });
      expect(badTerms.ok).toBe(false);
      expect(badTerms.errors[0]).toContain("0 and 365");

      // A rejected update never mutates the profile.
      expect(getCompanySettings(db).cvr).toBeNull();
      db.close();
    } finally {
      cleanupDir(root);
    }
  });

  test("init --name/--cvr/--address/--bank-* captures the profile in one command", async () => {
    const root = tmpRoot("profile-init-cli");
    try {
      const company = join(root, "company");
      const res = await runCli([
        "init",
        "--company", company,
        "--name", "Min Virksomhed ApS",
        "--cvr", "DK12345678",
        "--address", "Hovedgaden 1",
        "--postal-code", "2100",
        "--city", "København Ø",
        "--payment-terms", "10",
        "--bank-name", "Danske Bank",
        "--bank-reg", "1234",
        "--bank-account", "0001234567",
      ]);
      expect({ exitCode: res.exitCode, stderr: res.stderr }).toEqual({ exitCode: 0, stderr: "" });

      const db = openDb(companyPaths(company).db);
      migrate(db);
      const settings = getCompanySettings(db);
      expect(settings.name).toBe("Min Virksomhed ApS");
      expect(settings.cvr).toBe("DK12345678");
      expect(settings.address).toBe("Hovedgaden 1");
      expect(settings.paymentTermsDays).toBe(10);
      const bank = db.query("SELECT bank_name, account_no FROM bank_accounts WHERE slug = 'primaer'").get() as any;
      expect(bank.bank_name).toBe("Danske Bank");
      expect(bank.account_no).toBe("0001234567");
      db.close();
    } finally {
      cleanupDir(root);
    }
  });

  test("company set-profile edits the profile and the next issued invoice picks it up", async () => {
    const root = tmpRoot("profile-set-cli");
    try {
      const company = join(root, "company");
      const initRes = await runCli(["init", "--company", company]);
      expect(initRes.exitCode).toBe(0);

      const setRes = await runCli([
        "company", "set-profile",
        "--company", company,
        "--name", "Opdateret ApS",
        "--cvr", "DK87654321",
        "--address", "Nyvej 5",
        "--bank-name", "Jyske Bank",
        "--bank-account", "0005554443",
        "--format", "json",
      ]);
      expect({ exitCode: setRes.exitCode, stderr: setRes.stderr }).toEqual({ exitCode: 0, stderr: "" });
      const setParsed = JSON.parse(setRes.stdout);
      expect(setParsed.ok).toBe(true);

      const issueRes = await runCli([
        "invoice", "create",
        "--company", company,
        "--issue-date", "2026-05-16",
        "--line", "Ydelse|1|1000",
        "--buyer-name", "Kunde A/S",
        "--buyer-address", "Købervej 9",
        "--format", "json",
      ]);
      expect(issueRes.exitCode).toBe(0);
      const issued = JSON.parse(issueRes.stdout);
      expect(issued.ok).toBe(true);

      const stored = JSON.parse(readFileSync(issued.storedPath, "utf8"));
      expect(stored.seller.name).toBe("Opdateret ApS");
      expect(stored.seller.vatOrCvr).toBe("DK87654321");
      expect(stored.seller.address).toBe("Nyvej 5");

      const strings = pdfStrings(readFileSync(issued.pdfStoredPath)).join("\n");
      expect(strings).toContain("BETALING");
      expect(strings).toContain("Jyske Bank");
    } finally {
      cleanupDir(root);
    }
  });
});
