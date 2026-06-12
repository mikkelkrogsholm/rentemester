// Tests: src/core/vat-vies-list.ts (EU-salg uden moms-liste / VIES recapitulative statement)
import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { seedAccounts } from "../../src/core/ledger";
import { issueInvoice } from "../../src/core/issued-invoices";
import { storeViesValidation } from "../../src/core/vies";
import { buildViesRecapitulativeStatement } from "../../src/core/vat-vies-list";

function newCompany(prefix: string) {
  const root = mkdtempSync(join(tmpdir(), prefix));
  const db = openDb(ensureCompanyDirs(root).db);
  migrate(db);
  seedAccounts(db);
  return { root, db };
}

function foreignReverseChargeInvoice(
  db: ReturnType<typeof openDb>,
  root: string,
  opts: { issueDate: string; net: number; buyerName: string; buyerVat: string },
) {
  // The buyer's EU VAT number must be VIES-cached for a foreign reverse-charge
  // invoice to issue.
  storeViesValidation(db, { vatOrCvr: opts.buyerVat, valid: true, name: opts.buyerName });
  const result = issueInvoice(db, root, {
    invoiceType: "full",
    vatTreatment: "foreign_reverse_charge",
    reverseChargeBasis: "EU_MOMSDIREKTIV_ART_196",
    issueDate: opts.issueDate,
    seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
    buyer: { name: opts.buyerName, address: "EU-vej 1", vatOrCvr: opts.buyerVat },
    lines: [{ description: "Konsulentydelse", quantity: 1, unitPriceExVat: opts.net, lineTotalExVat: opts.net }],
    totals: { netAmount: opts.net, grossAmount: opts.net },
    currency: "DKK",
  });
  expect(result.ok).toBe(true);
  return result;
}

describe("EU-salg uden moms-liste (VIES recapitulative statement)", () => {
  test("groups foreign reverse-charge sales per customer VAT number", () => {
    const { root, db } = newCompany("rentemester-vieslist-");
    foreignReverseChargeInvoice(db, root, { issueDate: "2026-02-10", net: 1000, buyerName: "Kunde DE", buyerVat: "DE123456789" });
    foreignReverseChargeInvoice(db, root, { issueDate: "2026-03-05", net: 2500, buyerName: "Kunde DE", buyerVat: "DE123456789" });
    foreignReverseChargeInvoice(db, root, { issueDate: "2026-03-20", net: 700, buyerName: "Kunde SE", buyerVat: "SE556677889901" });

    const statement = buildViesRecapitulativeStatement(db, "2026-01-01", "2026-03-31");
    expect(statement.ok).toBe(true);
    // Two distinct customers in the period.
    expect(statement.customers.length).toBe(2);
    const de = statement.customers.find((c) => c.vatNumber === "DE123456789");
    expect(de).toBeDefined();
    // 1000 + 2500 summed for the same customer.
    expect(de!.totalValue).toBe(3500);
    expect(de!.invoiceCount).toBe(2);
    const se = statement.customers.find((c) => c.vatNumber === "SE556677889901");
    expect(se!.totalValue).toBe(700);
    expect(statement.totalValue).toBe(4200);

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("only includes invoices issued within the period", () => {
    const { root, db } = newCompany("rentemester-vieslist-period-");
    foreignReverseChargeInvoice(db, root, { issueDate: "2026-03-15", net: 1000, buyerName: "Kunde DE", buyerVat: "DE123456789" });
    // Out of the Q1 period.
    foreignReverseChargeInvoice(db, root, { issueDate: "2026-04-02", net: 9000, buyerName: "Kunde DE", buyerVat: "DE123456789" });

    const statement = buildViesRecapitulativeStatement(db, "2026-01-01", "2026-03-31");
    expect(statement.ok).toBe(true);
    expect(statement.customers.length).toBe(1);
    expect(statement.customers[0]!.totalValue).toBe(1000);

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("excludes standard domestic VAT sales from the listing", () => {
    const { root, db } = newCompany("rentemester-vieslist-domestic-");
    // A standard domestic invoice must not appear on the VIES list.
    const domestic = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-03-10",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Dansk Kunde", address: "Danmarksvej 1", vatOrCvr: "DK87654321" },
      lines: [{ description: "Ydelse", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
      currency: "DKK",
    });
    expect(domestic.ok).toBe(true);

    const statement = buildViesRecapitulativeStatement(db, "2026-01-01", "2026-03-31");
    expect(statement.ok).toBe(true);
    expect(statement.customers.length).toBe(0);
    expect(statement.totalValue).toBe(0);

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("rejects invalid period dates", () => {
    const { root, db } = newCompany("rentemester-vieslist-bad-");
    const statement = buildViesRecapitulativeStatement(db, "not-a-date", "2026-03-31");
    expect(statement.ok).toBe(false);
    expect(statement.errors.length).toBeGreaterThan(0);
    db.close();
    rmSync(root, { recursive: true, force: true });
  });
});
