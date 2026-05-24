// Tests: src/server/write-handlers.ts (#390 — Cockpit can create + edit
// customers/vendors) and the matching routes in src/server/router.ts.

import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { handleRequest } from "../../src/server/router";
import type { ServerConfig } from "../../src/server/config";
import { createCompany } from "../../src/core/company";
import { initWorkspace, companyRootForSlug } from "../../src/core/workspace";
import { companyPaths } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";

function makeWorkspace(label: string) {
  const root = mkdtempSync(join(tmpdir(), `rentemester-${label}-`));
  initWorkspace(root);
  const created = createCompany(root, { name: "Acme ApS" });
  return { root, slug: created.slug };
}

function config(workspaceRoot: string): ServerConfig {
  return {
    host: "127.0.0.1",
    port: 0,
    authRequired: false,
    authToken: null,
    workspaceRoot,
  };
}

async function send(
  cfg: ServerConfig,
  method: string,
  path: string,
  body?: unknown,
) {
  const init: RequestInit = { method, headers: { host: "127.0.0.1" } };
  if (body !== undefined) init.body = JSON.stringify(body);
  const res = await handleRequest(new Request(`http://localhost${path}`, init), cfg);
  return { status: res.status, body: await res.json() };
}

describe("Cockpit write — contacts (#390)", () => {
  test("POST /customers creates a customer reachable from the contacts view", async () => {
    const { root: ws, slug } = makeWorkspace("contact-create-cust");
    try {
      const cfg = config(ws);
      const created = await send(cfg, "POST", `/api/companies/${slug}/customers`, {
        name: "Ny Kunde A/S",
        vatOrCvr: "DK12345678",
        email: "faktura@kunde.dk",
        paymentTermsDays: 14,
        defaultCurrency: "DKK",
      });
      expect(created.status).toBe(200);
      expect(created.body.ok).toBe(true);
      expect(typeof created.body.customer.id).toBe("number");

      const list = await send(cfg, "GET", `/api/companies/${slug}/contacts`);
      expect(list.status).toBe(200);
      const customers = list.body.contacts.customers as Array<{ name: string }>;
      expect(customers.some((c) => c.name === "Ny Kunde A/S")).toBe(true);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("POST /vendors creates a vendor with stamdata fields", async () => {
    const { root: ws, slug } = makeWorkspace("contact-create-vendor");
    try {
      const cfg = config(ws);
      const created = await send(cfg, "POST", `/api/companies/${slug}/vendors`, {
        name: "Leverandør ApS",
        vatOrCvr: "DK87654321",
        defaultExpenseAccount: "3000",
        defaultVatTreatment: "standard",
      });
      expect(created.status).toBe(200);
      expect(created.body.ok).toBe(true);

      const list = await send(cfg, "GET", `/api/companies/${slug}/contacts`);
      const vendors = list.body.contacts.vendors as Array<{
        name: string;
        defaultExpenseAccount: string | null;
      }>;
      const row = vendors.find((v) => v.name === "Leverandør ApS");
      expect(row).toBeDefined();
      expect(row!.defaultExpenseAccount).toBe("3000");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("PATCH /customers/:id updates the customer", async () => {
    const { root: ws, slug } = makeWorkspace("contact-update-cust");
    try {
      const cfg = config(ws);
      const created = await send(cfg, "POST", `/api/companies/${slug}/customers`, {
        name: "Original A/S",
        paymentTermsDays: 30,
      });
      const id = created.body.customer.id as number;

      const patched = await send(
        cfg,
        "PATCH",
        `/api/companies/${slug}/customers/${id}`,
        { name: "Omdøbt A/S", paymentTermsDays: 7, email: "ny@kunde.dk" },
      );
      expect(patched.status).toBe(200);
      expect(patched.body.ok).toBe(true);

      const list = await send(cfg, "GET", `/api/companies/${slug}/contacts`);
      const row = (list.body.contacts.customers as Array<{
        id: number;
        name: string;
        paymentTermsDays: number;
        email: string | null;
      }>).find((c) => c.id === id);
      expect(row?.name).toBe("Omdøbt A/S");
      expect(row?.paymentTermsDays).toBe(7);
      expect(row?.email).toBe("ny@kunde.dk");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("PATCH /vendors/:id updates the vendor's default expense account", async () => {
    const { root: ws, slug } = makeWorkspace("contact-update-vendor");
    try {
      const cfg = config(ws);
      const created = await send(cfg, "POST", `/api/companies/${slug}/vendors`, {
        name: "Lev. ApS",
      });
      const id = created.body.vendor.id as number;

      const patched = await send(
        cfg,
        "PATCH",
        `/api/companies/${slug}/vendors/${id}`,
        { defaultExpenseAccount: "3100", defaultVatTreatment: "exempt" },
      );
      expect(patched.status).toBe(200);
      expect(patched.body.ok).toBe(true);

      const list = await send(cfg, "GET", `/api/companies/${slug}/contacts`);
      const row = (list.body.contacts.vendors as Array<{
        id: number;
        defaultExpenseAccount: string | null;
        defaultVatTreatment: string | null;
      }>).find((v) => v.id === id);
      expect(row?.defaultExpenseAccount).toBe("3100");
      expect(row?.defaultVatTreatment).toBe("exempt");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("POST /customers rejects a missing name with 400", async () => {
    const { root: ws, slug } = makeWorkspace("contact-bad-name");
    try {
      const cfg = config(ws);
      const res = await send(cfg, "POST", `/api/companies/${slug}/customers`, {
        email: "ingen-navn@kunde.dk",
      });
      expect(res.status).toBe(400);
      expect(res.body.ok).toBe(false);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("PATCH /customers/:id on an unknown id is a 409 conflict", async () => {
    const { root: ws, slug } = makeWorkspace("contact-unknown");
    try {
      const cfg = config(ws);
      const res = await send(
        cfg,
        "PATCH",
        `/api/companies/${slug}/customers/99999`,
        { name: "Phantom A/S" },
      );
      expect(res.status).toBe(409);
      expect(res.body.ok).toBe(false);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("GET /cvr-lookup degrades gracefully when credentials are missing", async () => {
    const { root: ws, slug } = makeWorkspace("contact-cvr");
    try {
      const cfg = config(ws);
      // No CVR_USERNAME/CVR_PASSWORD set in this test process — the lookup
      // returns ok:false inside the envelope, not a 500.
      delete process.env.CVR_USERNAME;
      delete process.env.CVR_PASSWORD;
      const res = await send(
        cfg,
        "GET",
        `/api/companies/${slug}/cvr-lookup?cvr=12345678`,
      );
      expect(res.status).toBe(200);
      expect(res.body.ok).toBe(true);
      expect(res.body.cvr.ok).toBe(false);
      expect(Array.isArray(res.body.cvr.errors)).toBe(true);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});
