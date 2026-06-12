// Tests: customers/vendors are ordinary mutable tables (the append-only
// triggers were removed — they were a self-imposed constraint, not law) and
// carry the phone/website contact columns.
import { describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { createCustomer, createVendor, listCustomers, listVendors } from "../../src/core/master-data";

import { cleanupDir } from "../helpers/cleanup";
function freshDb() {
  const root = mkdtempSync(join(tmpdir(), "rentemester-md-mut-"));
  const db = openDb(ensureCompanyDirs(root).db);
  migrate(db);
  return { root, db };
}

describe("customers/vendors are mutable master data", () => {
  test("a customer row can be UPDATEd and DELETEd", () => {
    const { root, db } = freshDb();
    const created = createCustomer(db, { name: "Kunde A/S" });
    expect(created.ok).toBe(true);
    const id = (created as { customerId: number }).customerId;

    // UPDATE — was blocked by the customers_no_update trigger.
    expect(() => db.run("UPDATE customers SET address = ? WHERE id = ?", "Ny vej 1", id)).not.toThrow();
    expect((db.query("SELECT address FROM customers WHERE id = ?").get(id) as { address: string }).address).toBe("Ny vej 1");

    // DELETE — was blocked by the customers_no_delete trigger.
    expect(() => db.run("DELETE FROM customers WHERE id = ?", id)).not.toThrow();
    expect(db.query("SELECT COUNT(*) AS n FROM customers").get()).toEqual({ n: 0 });

    db.close();
    cleanupDir(root);
  });

  test("a vendor row can be UPDATEd and DELETEd", () => {
    const { root, db } = freshDb();
    const created = createVendor(db, { name: "Leverandør ApS" });
    expect(created.ok).toBe(true);
    const id = (created as { vendorId: number }).vendorId;

    expect(() => db.run("UPDATE vendors SET notes = ? WHERE id = ?", "ret", id)).not.toThrow();
    expect(() => db.run("DELETE FROM vendors WHERE id = ?", id)).not.toThrow();

    db.close();
    cleanupDir(root);
  });

  test("phone and website round-trip on customers and vendors", () => {
    const { root, db } = freshDb();
    createCustomer(db, {
      name: "Med kontaktdata ApS",
      phone: "12345678",
      website: "www.eksempel.dk",
    });
    createVendor(db, {
      name: "Leverandør med data",
      email: "faktura@lev.dk",
      phone: "87654321",
      website: "lev.dk",
    });

    const customer = listCustomers(db).rows[0]!;
    expect({ phone: customer.phone, website: customer.website }).toEqual({
      phone: "12345678",
      website: "www.eksempel.dk",
    });

    const vendor = listVendors(db).rows[0]!;
    expect({ email: vendor.email, phone: vendor.phone, website: vendor.website }).toEqual({
      email: "faktura@lev.dk",
      phone: "87654321",
      website: "lev.dk",
    });

    db.close();
    cleanupDir(root);
  });
});
