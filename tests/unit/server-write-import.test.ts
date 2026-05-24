// Tests: src/server/write-handlers.ts (handleDataImport) and the
// POST /api/companies/:slug/import route in src/server/router.ts — the
// cockpit's generic, source-recognising file-import.
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

function makeWorkspace(label: string, companyName = "Acme ApS") {
  const root = mkdtempSync(join(tmpdir(), `rentemester-${label}-`));
  initWorkspace(root);
  const created = createCompany(root, { name: companyName });
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

async function post(cfg: ServerConfig, path: string, body?: unknown) {
  const init: RequestInit = {
    method: "POST",
    headers: { host: "127.0.0.1" },
  };
  if (body !== undefined) init.body = JSON.stringify(body);
  const res = await handleRequest(new Request(`http://localhost${path}`, init), cfg);
  return { status: res.status, body: await res.json() };
}

// A Dinero "Kontakter" export — one vendor (purchase history) and one
// customer (sales history).
const DINERO_CONTACTS_CSV = [
  "Kontaktnavn;Adresse;Postnummer;By;Landekode;CVR-nummer;EAN-nummer;" +
    "Telefon;E-mail;Att. person;Hjemmeside;Betalings metode;" +
    "Betalingsfrist i dage;Total salg;Total køb;Kontakttype",
  "Leverandør ApS;Vej 1;1000;København;DK;12345678;;;;;;Netto;8;0;500;Company",
  "Kunde A/S;Gade 2;2000;Frederiksberg;DK;87654321;;;;;;Netto;14;1200;0;Company",
].join("\n");

describe("Cockpit write — generic file-import", () => {
  test("recognises and imports a Dinero Kontakter CSV", async () => {
    const { root: ws, slug } = makeWorkspace("import-ok");
    try {
      const res = await post(config(ws), `/api/companies/${slug}/import`, {
        fileName: "Kontakter.csv",
        content: DINERO_CONTACTS_CSV,
        confirm: true,
      });
      expect(res.status).toBe(200);
      expect(res.body.ok).toBe(true);
      expect(res.body.import.detected.id).toBe("dinero-contacts");
      expect(res.body.import.detected.system).toBe("Dinero");
      expect(res.body.import.summary.vendorsCreated).toBe(1);
      expect(res.body.import.summary.customersCreated).toBe(1);

      // The contacts now live in the company's master data.
      const companyRoot = companyRootForSlug(ws, slug);
      const db = openDb(companyPaths(companyRoot).db);
      try {
        migrate(db);
        const customers = db
          .query("SELECT COUNT(*) AS n FROM customers")
          .get() as { n: number };
        const vendors = db
          .query("SELECT COUNT(*) AS n FROM vendors")
          .get() as { n: number };
        expect(customers.n).toBe(1);
        expect(vendors.n).toBe(1);
      } finally {
        db.close();
      }
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a re-import is idempotent — existing contacts are skipped", async () => {
    const { root: ws, slug } = makeWorkspace("import-idem");
    try {
      const cfg = config(ws);
      await post(cfg, `/api/companies/${slug}/import`, {
        fileName: "Kontakter.csv",
        content: DINERO_CONTACTS_CSV,
        confirm: true,
      });
      const res = await post(cfg, `/api/companies/${slug}/import`, {
        fileName: "Kontakter.csv",
        content: DINERO_CONTACTS_CSV,
        confirm: true,
      });
      expect(res.status).toBe(200);
      expect(res.body.import.summary.vendorsCreated).toBe(0);
      expect(res.body.import.summary.customersCreated).toBe(0);
      expect(res.body.import.summary.skipped).toBe(2);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("an unrecognised file is a 400 with the supported-formats list", async () => {
    const { root: ws, slug } = makeWorkspace("import-unknown");
    try {
      const res = await post(config(ws), `/api/companies/${slug}/import`, {
        fileName: "mystery.csv",
        content: "a,b,c\n1,2,3",
        confirm: true,
      });
      expect(res.status).toBe(400);
      expect(res.body.errors[0]).toContain("ikke genkendt");
      expect(res.body.errors[0]).toContain("Dinero");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("import without confirm is refused", async () => {
    const { root: ws, slug } = makeWorkspace("import-noconfirm");
    try {
      const res = await post(config(ws), `/api/companies/${slug}/import`, {
        fileName: "Kontakter.csv",
        content: DINERO_CONTACTS_CSV,
      });
      expect(res.status).toBe(400);
      expect(res.body.errors[0]).toContain("confirm");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("import for an unknown slug is a safe 404", async () => {
    const { root: ws } = makeWorkspace("import-404");
    try {
      const res = await post(config(ws), "/api/companies/ghost/import", {
        fileName: "Kontakter.csv",
        content: DINERO_CONTACTS_CSV,
        confirm: true,
      });
      expect(res.status).toBe(404);
      expect(res.body.code).toBe("not_found");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});
