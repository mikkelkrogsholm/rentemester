import { describe, expect, test } from "bun:test";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { initialiseCompanyVolume, setCompanyProfile } from "../../src/core/company";
import { openDb } from "../../src/core/db";
import { issueInvoice } from "../../src/core/issued-invoices";
import { companyPaths } from "../../src/core/paths";

const payload = {
  invoiceType: "full" as const,
  vatTreatment: "standard" as const,
  issueDate: "2026-10-26",
  buyer: { name: "Kunde A/S", address: "Købervej 9" },
  lines: [{ description: "Ydelse", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
  totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
  currency: "DKK",
};

describe("daily invoice series", () => {
  test("rolls back reservation when the PDF cannot be published", () => {
    const root = mkdtempSync(join(tmpdir(), "invoice-daily-rollback-"));
    try {
      initialiseCompanyVolume(root, { name: "Synthetic ApS", cvr: "DK12345678", address: "Testvej 1" });
      const db = openDb(companyPaths(root).db);
      expect(setCompanyProfile(db, { invoiceNumberFormat: "daily" }).ok).toBe(true);
      const blocked = join(companyPaths(root).invoicesIssued, "RM-26-10-26-01.pdf");
      writeFileSync(blocked, "occupied");
      expect(() => issueInvoice(db, root, payload)).toThrow();
      rmSync(blocked);
      expect(issueInvoice(db, root, payload).invoiceNumber).toBe("RM-26-10-26-01");
      db.close();
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
  test("are company-local and date-local and preserve previously issued invoices", () => {
    const root = mkdtempSync(join(tmpdir(), "invoice-daily-"));
    const a = join(root, "a");
    const b = join(root, "b");
    try {
      for (const path of [a, b]) initialiseCompanyVolume(path, { name: "Synthetic ApS", cvr: "DK12345678", address: "Testvej 1" });
      const dbA = openDb(companyPaths(a).db);
      const dbB = openDb(companyPaths(b).db);
      const old = issueInvoice(dbA, a, payload);
      expect(old.invoiceNumber).toBe("2026-0001");
      const oldBytes = readFileSync(old.storedPath!);
      expect(setCompanyProfile(dbA, { invoiceNumberFormat: "daily" }).ok).toBe(true);
      expect(setCompanyProfile(dbB, { invoiceNumberFormat: "daily" }).ok).toBe(true);
      expect(issueInvoice(dbA, a, payload).invoiceNumber).toBe("RM-26-10-26-01");
      expect(issueInvoice(dbA, a, payload).invoiceNumber).toBe("RM-26-10-26-02");
      expect(issueInvoice(dbB, b, payload).invoiceNumber).toBe("RM-26-10-26-01");
      expect(issueInvoice(dbA, a, { ...payload, issueDate: "2026-10-27" }).invoiceNumber).toBe("RM-26-10-27-01");
      expect(issueInvoice(dbA, a, { ...payload, invoiceNumber: "RM-26-10-26-04" }).ok).toBe(false);
      expect(issueInvoice(dbA, a, { ...payload, invoiceNumber: "RM-26-10-26-3" }).invoiceNumber).toBe("RM-26-10-26-03");
      expect(readFileSync(old.storedPath!)).toEqual(oldBytes);
      expect(setCompanyProfile(dbA, { invoiceNumberFormat: "fiscal-year" }).ok).toBe(true);
      expect(issueInvoice(dbA, a, payload).invoiceNumber).toBe("2026-0002");
      dbA.close();
      dbB.close();
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
});
