import { expect, test, describe } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { Database } from "bun:sqlite";
import { migrate } from "../../src/core/db";
import { seedAccounts } from "../../src/core/ledger";
import { initialiseCompanyVolume } from "../../src/core/company";
import {
  parseBillyContacts,
  importBillyContacts,
} from "../../src/core/import/billy-contacts";
import { listCustomers, listVendors } from "../../src/core/master-data";

function freshCompany(): { root: string; db: Database } {
  const root = mkdtempSync(join(tmpdir(), "rentemester-billy-contacts-"));
  initialiseCompanyVolume(root, {});
  const db = new Database(join(root, "data", "ledger.sqlite"));
  migrate(db);
  seedAccounts(db);
  return { root, db };
}

describe("Billy contacts parser", () => {
  test("parses valid contacts JSON", () => {
    const result = parseBillyContacts(
      JSON.stringify([
        { id: "c1", name: "Acme ApS", isCustomer: true, isSupplier: false },
        { id: "c2", name: "Vendor Corp", isCustomer: false, isSupplier: true },
      ]),
    );
    expect(result.ok).toBe(true);
    expect(result.contacts.length).toBe(2);
  });

  test("rejects invalid JSON", () => {
    const result = parseBillyContacts("not json");
    expect(result.ok).toBe(false);
    expect(result.errors[0]).toContain("invalid JSON");
  });

  test("rejects non-array JSON", () => {
    const result = parseBillyContacts('{"not": "an array"}');
    expect(result.ok).toBe(false);
    expect(result.errors[0]).toContain("expected a JSON array");
  });
});

describe("Billy contacts import", () => {
  test("imports customers and vendors based on isCustomer/isSupplier", () => {
    const { root, db } = freshCompany();
    try {
      const result = importBillyContacts(
        db,
        JSON.stringify([
          {
            id: "c1",
            name: "Customer ApS",
            isCustomer: true,
            isSupplier: false,
            countryId: "DK",
            registrationNo: "12345678",
          },
          {
            id: "c2",
            name: "Supplier A/S",
            isCustomer: false,
            isSupplier: true,
            street: "Testvej 1",
            zipcodeText: "5000",
            cityText: "Odense",
          },
          {
            id: "c3",
            name: "Both Corp",
            isCustomer: true,
            isSupplier: true,
          },
        ]),
      );

      expect(result.ok).toBe(true);
      expect(result.summary.customersCreated).toBe(2);
      expect(result.summary.vendorsCreated).toBe(2);

      const customers = listCustomers(db);
      expect(customers.rows.length).toBe(2);
      const custApS = customers.rows.find((c) => c.name === "Customer ApS")!;
      expect(custApS.vatOrCvr).toBe("DK12345678");

      const vendors = listVendors(db);
      expect(vendors.rows.length).toBe(2);
      const supplier = vendors.rows.find((v) => v.name === "Supplier A/S")!;
      expect(supplier.address).toContain("Testvej 1");
      expect(supplier.address).toContain("5000 Odense");

      db.close();
    } finally {
      try { rmSync(root, { recursive: true, force: true }); } catch {}
    }
  });

  test("is idempotent — re-import skips existing contacts", () => {
    const { root, db } = freshCompany();
    try {
      const contacts = JSON.stringify([
        { id: "c1", name: "Test Customer", isCustomer: true },
      ]);

      const first = importBillyContacts(db, contacts);
      expect(first.summary.customersCreated).toBe(1);

      const second = importBillyContacts(db, contacts);
      expect(second.summary.customersCreated).toBe(0);
      expect(second.summary.skipped).toBe(1);

      expect(listCustomers(db).rows.length).toBe(1);
      db.close();
    } finally {
      try { rmSync(root, { recursive: true, force: true }); } catch {}
    }
  });

  test("skips archived contacts", () => {
    const { root, db } = freshCompany();
    try {
      const result = importBillyContacts(
        db,
        JSON.stringify([
          { id: "c1", name: "Active", isCustomer: true, isArchived: false },
          { id: "c2", name: "Archived", isCustomer: true, isArchived: true },
        ]),
      );

      expect(result.summary.customersCreated).toBe(1);
      expect(result.summary.archivedSkipped).toBe(1);
      db.close();
    } finally {
      try { rmSync(root, { recursive: true, force: true }); } catch {}
    }
  });

  test("falls back to defaultRole when neither flag is set", () => {
    const { root, db } = freshCompany();
    try {
      const result = importBillyContacts(
        db,
        JSON.stringify([{ id: "c1", name: "Ambiguous Contact" }]),
        { defaultRole: "customer" },
      );

      expect(result.summary.customersCreated).toBe(1);
      expect(result.summary.vendorsCreated).toBe(0);
      db.close();
    } finally {
      try { rmSync(root, { recursive: true, force: true }); } catch {}
    }
  });

  test("skips contacts without a name", () => {
    const { root, db } = freshCompany();
    try {
      const result = importBillyContacts(
        db,
        JSON.stringify([
          { id: "c1", name: "", isCustomer: true },
          { id: "c2", name: "   ", isCustomer: true },
        ]),
      );

      expect(result.summary.customersCreated).toBe(0);
      expect(result.errors.length).toBe(2);
      db.close();
    } finally {
      try { rmSync(root, { recursive: true, force: true }); } catch {}
    }
  });
});
