import { describe, expect, test } from "bun:test";
import {
  companyPaths,
  companyRootForSlug,
  config,
  createCustomer,
  createVendor,
  get,
  issueTestInvoiceForBuyer,
  makeWorkspace,
  migrate,
  openDb,
  rmSync,
} from "./_shared";

describe("cockpit API — contacts (GET .../contacts)", () => {
  test("returns customers and vendors from the master data", async () => {
    const ws = makeWorkspace("con-live", ["Acme ApS"]);
    try {
      const db = openDb(companyPaths(companyRootForSlug(ws, "acme-aps")).db);
      try {
        migrate(db);
        createCustomer(db, { name: "Kunde A/S", vatOrCvr: "DK87654321" });
        createVendor(db, { name: "Leverandør ApS", vatOrCvr: "DK11223344" });
      } finally {
        db.close();
      }
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/contacts",
      );
      expect(res.status).toBe(200);
      const c = res.body.contacts;
      expect(c.slug).toBe("acme-aps");
      expect(c.customers.length).toBe(1);
      expect(c.customers[0].name).toBe("Kunde A/S");
      expect(c.vendors.length).toBe(1);
      expect(c.vendors[0].name).toBe("Leverandør ApS");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a company with no contacts returns empty lists", async () => {
    const ws = makeWorkspace("con-empty", ["Acme ApS"]);
    try {
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/contacts",
      );
      expect(res.status).toBe(200);
      expect(res.body.contacts.customers).toEqual([]);
      expect(res.body.contacts.vendors).toEqual([]);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("contacts for an unknown slug is a safe 404", async () => {
    const ws = makeWorkspace("con-404", ["Acme ApS"]);
    try {
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/ghost/contacts",
      );
      expect(res.status).toBe(404);
      expect(res.body.code).toBe("not_found");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  // #439 — Kontakter-siden skal kunne svare på "hvad skylder den her kunde mig?"
  // direkte. Server-side aggregerer åbne fakturaer pr. kunde fra den samme
  // ledger-kilde som /invoices, så hvert ContactCustomerRow får openBalance,
  // openInvoiceCount og overdueCount. Kunder med forfaldne fakturaer sorteres
  // øverst (mirrors PortfolioView.sortByAttention).
  test("aggregates open + overdue invoices per customer (#439)", async () => {
    const ws = makeWorkspace("con-saldo", ["Acme ApS"]);
    try {
      // Tre kunder i kontaktlisten — én uden fakturaer, én med åben (ikke-
      // forfalden) faktura, én med forfalden faktura. Navn er join-nøglen
      // mellem invoices og customers (samme regel som "Send på mail"-prefill).
      const db = openDb(companyPaths(companyRootForSlug(ws, "acme-aps")).db);
      try {
        migrate(db);
        createCustomer(db, { name: "Ingen Skyld ApS" });
        createCustomer(db, { name: "Åben Saldo ApS" });
        createCustomer(db, { name: "Forfalden Saldo ApS" });
      } finally {
        db.close();
      }
      // En faktura med en udstedelsesdato langt tilbage er forfalden i dag.
      issueTestInvoiceForBuyer(ws, "acme-aps", "Forfalden Saldo ApS", "2020-01-15", 800);
      // En faktura udstedt i går — antagelig ikke forfalden endnu (30 dages
      // standard betalingsfrist på dansk faktura). buildInvoiceList vurderer
      // selv via getInvoiceStatus.
      const today = new Date();
      const yyyy = today.getUTCFullYear();
      const mm = String(today.getUTCMonth() + 1).padStart(2, "0");
      const dd = String(today.getUTCDate()).padStart(2, "0");
      issueTestInvoiceForBuyer(ws, "acme-aps", "Åben Saldo ApS", `${yyyy}-${mm}-${dd}`, 1200);

      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/contacts",
      );
      expect(res.status).toBe(200);
      const customers = res.body.contacts.customers as Array<{
        name: string;
        openBalance: number;
        openInvoiceCount: number;
        overdueCount: number;
      }>;
      expect(customers.length).toBe(3);

      // Kunden med forfaldne fakturaer ligger øverst.
      expect(customers[0].name).toBe("Forfalden Saldo ApS");
      expect(customers[0].overdueCount).toBe(1);
      expect(customers[0].openInvoiceCount).toBe(1);
      expect(customers[0].openBalance).toBeGreaterThan(0);

      // Derefter kunden med åben (men ikke forfalden) faktura.
      expect(customers[1].name).toBe("Åben Saldo ApS");
      expect(customers[1].overdueCount).toBe(0);
      expect(customers[1].openInvoiceCount).toBe(1);
      expect(customers[1].openBalance).toBeGreaterThan(0);

      // Sidst kunden uden udestående.
      expect(customers[2].name).toBe("Ingen Skyld ApS");
      expect(customers[2].overdueCount).toBe(0);
      expect(customers[2].openInvoiceCount).toBe(0);
      expect(customers[2].openBalance).toBe(0);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});
