// Tests for the cockpit's Anlæg routes (#336):
//   - GET    /api/companies/:slug/assets
//   - POST   /api/companies/:slug/assets
//   - GET    /api/companies/:slug/assets/:id/next-depreciation
//   - POST   /api/companies/:slug/assets/:id/depreciate
//   - POST   /api/companies/:slug/assets/write-off
//
// The cockpit becomes a THIRD caller of `src/core/assets.ts` alongside the
// CLI's `asset` sub-commands and the MCP `asset_*` tools — no depreciation
// arithmetic is reimplemented at the HTTP layer. These tests pin the
// happy-path responses, the confirm/validation guards, and the read-side
// register/write-off aggregation.

import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { handleRequest } from "../../src/server/router";
import type { ServerConfig } from "../../src/server/config";
import { createCompany } from "../../src/core/company";
import { initWorkspace, companyRootForSlug } from "../../src/core/workspace";
import { companyPaths } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { ingestDocument } from "../../src/core/documents";

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

/**
 * Seeds a purchase document the cockpit's register-asset call can reference.
 * Mirrors `tests/unit/asset-depreciation.test.ts` — `ingestDocument` produces
 * a real `documents` row so the FK in `assets.purchase_document_id` is valid.
 */
function seedPurchaseDocument(
  workspaceRoot: string,
  slug: string,
  label: string,
  cost: number,
): number {
  const companyRoot = companyRootForSlug(workspaceRoot, slug);
  const inbox = mkdtempSync(join(tmpdir(), `rentemester-${label}-inbox-`));
  const sourceFile = join(inbox, "asset.txt");
  writeFileSync(sourceFile, `Asset invoice ${label}\n`);
  const db = openDb(companyPaths(companyRoot).db);
  try {
    migrate(db);
    const doc = ingestDocument(db, companyRoot, sourceFile, {
      source: "email",
      issueDate: "2026-01-10",
      invoiceNo: `ASSET-${label}`,
      deliveryDescription: "Anlægsaktiv",
      amountIncVat: cost,
      currency: "DKK",
      sender: {
        name: "Hardware ApS",
        address: "Vej 1",
        vatOrCvr: "DK11223344",
      },
      recipient: {
        name: "Rentemester ApS",
        address: "Testvej 1",
        vatOrCvr: "DK12345678",
      },
      vatAmount: 0,
      paymentDetails: "Bank transfer",
    });
    if (!doc.ok || !doc.documentId) {
      throw new Error(`failed to seed purchase document: ${doc.errors?.join("; ")}`);
    }
    return doc.documentId;
  } finally {
    db.close();
    rmSync(inbox, { recursive: true, force: true });
  }
}

describe("Cockpit Anlæg routes (#336)", () => {
  test("GET /assets on a fresh ledger returns an empty register", async () => {
    const { root: ws, slug } = makeWorkspace("anl-empty");
    try {
      const cfg = config(ws);
      const res = await send(cfg, "GET", `/api/companies/${slug}/assets`);
      expect(res.status).toBe(200);
      expect(res.body.ok).toBe(true);
      expect(res.body.assets.assets).toEqual([]);
      expect(res.body.assets.writeOffs).toEqual([]);
      expect(res.body.assets.totals.cost).toBe(0);
      expect(res.body.assets.totals.activeCount).toBe(0);
      expect(res.body.assets.totals.fullyDepreciatedCount).toBe(0);
      expect(res.body.assets.totals.writeOffCount).toBe(0);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("POST /assets registers a capitalised asset and it shows up in GET /assets", async () => {
    const { root: ws, slug } = makeWorkspace("anl-register");
    try {
      const cfg = config(ws);
      const docId = seedPurchaseDocument(ws, slug, "register", 48000);

      const created = await send(cfg, "POST", `/api/companies/${slug}/assets`, {
        name: "MacBook Pro",
        category: "hardware",
        acquisitionDate: "2026-01-10",
        cost: 48000,
        usefulLifeMonths: 36,
        purchaseDocumentId: docId,
        confirm: true,
      });
      expect(created.status).toBe(200);
      expect(created.body.ok).toBe(true);
      expect(created.body.asset.assetId).toBeGreaterThan(0);
      expect(created.body.asset.totalPeriods).toBe(36);
      expect(typeof created.body.asset.periodAmount).toBe("number");

      const list = await send(cfg, "GET", `/api/companies/${slug}/assets`);
      expect(list.status).toBe(200);
      const rows = list.body.assets.assets as Array<{
        assetId: number;
        name: string;
        cost: number;
        status: string;
        remainingPeriods: number;
        postedPeriods: number;
        netBookValue: number;
      }>;
      expect(rows.length).toBe(1);
      const row = rows[0]!;
      expect(row.name).toBe("MacBook Pro");
      expect(row.cost).toBe(48000);
      expect(row.postedPeriods).toBe(0);
      expect(row.remainingPeriods).toBe(36);
      expect(row.status).toBe("active");
      expect(row.netBookValue).toBe(48000);
      expect(list.body.assets.totals.cost).toBe(48000);
      expect(list.body.assets.totals.activeCount).toBe(1);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("POST /assets without confirm: true is a 400", async () => {
    const { root: ws, slug } = makeWorkspace("anl-confirm");
    try {
      const cfg = config(ws);
      const docId = seedPurchaseDocument(ws, slug, "confirm", 12000);
      const res = await send(cfg, "POST", `/api/companies/${slug}/assets`, {
        name: "Skærm",
        category: "hardware",
        acquisitionDate: "2026-02-01",
        cost: 12000,
        usefulLifeMonths: 24,
        purchaseDocumentId: docId,
      });
      expect(res.status).toBe(400);
      expect(res.body.ok).toBe(false);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("POST /assets/:id/depreciate posts the next period and shows it in the register", async () => {
    const { root: ws, slug } = makeWorkspace("anl-depr");
    try {
      const cfg = config(ws);
      const docId = seedPurchaseDocument(ws, slug, "depr", 24000);
      const created = await send(cfg, "POST", `/api/companies/${slug}/assets`, {
        name: "Serverhardware",
        category: "hardware",
        acquisitionDate: "2026-01-10",
        cost: 24000,
        usefulLifeMonths: 12,
        purchaseDocumentId: docId,
        confirm: true,
      });
      const assetId = created.body.asset.assetId as number;

      const next = await send(
        cfg,
        "GET",
        `/api/companies/${slug}/assets/${assetId}/next-depreciation`,
      );
      expect(next.status).toBe(200);
      expect(next.body.nextDepreciation.nextPeriodIndex).toBe(1);
      expect(next.body.nextDepreciation.totalPeriods).toBe(12);
      expect(next.body.nextDepreciation.remainingPeriods).toBe(12);

      const posted = await send(
        cfg,
        "POST",
        `/api/companies/${slug}/assets/${assetId}/depreciate`,
        { transactionDate: "2026-01-31", confirm: true },
      );
      expect(posted.status).toBe(200);
      expect(posted.body.depreciation.periodIndex).toBe(1);
      expect(typeof posted.body.depreciation.periodAmount).toBe("number");

      const list = await send(cfg, "GET", `/api/companies/${slug}/assets`);
      const row = (list.body.assets.assets as Array<{
        assetId: number;
        postedPeriods: number;
        accumulatedDepreciation: number;
        remainingPeriods: number;
      }>).find((r) => r.assetId === assetId);
      expect(row?.postedPeriods).toBe(1);
      expect(row?.remainingPeriods).toBe(11);
      expect(row?.accumulatedDepreciation).toBeGreaterThan(0);

      const nextAfter = await send(
        cfg,
        "GET",
        `/api/companies/${slug}/assets/${assetId}/next-depreciation`,
      );
      expect(nextAfter.body.nextDepreciation.postedPeriods).toBe(1);
      expect(nextAfter.body.nextDepreciation.nextPeriodIndex).toBe(2);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("POST /assets/:id/depreciate without confirm: true is a 400", async () => {
    const { root: ws, slug } = makeWorkspace("anl-depr-confirm");
    try {
      const cfg = config(ws);
      const docId = seedPurchaseDocument(ws, slug, "depr-confirm", 12000);
      const created = await send(cfg, "POST", `/api/companies/${slug}/assets`, {
        name: "Anlæg",
        category: "hardware",
        acquisitionDate: "2026-01-10",
        cost: 12000,
        usefulLifeMonths: 12,
        purchaseDocumentId: docId,
        confirm: true,
      });
      const assetId = created.body.asset.assetId as number;
      const res = await send(
        cfg,
        "POST",
        `/api/companies/${slug}/assets/${assetId}/depreciate`,
        { transactionDate: "2026-01-31" },
      );
      expect(res.status).toBe(400);
      expect(res.body.ok).toBe(false);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("POST /assets/write-off books a straksafskrivning and shows it in GET /assets", async () => {
    const { root: ws, slug } = makeWorkspace("anl-wo");
    try {
      const cfg = config(ws);
      const docId = seedPurchaseDocument(ws, slug, "wo", 8000);
      const res = await send(cfg, "POST", `/api/companies/${slug}/assets/write-off`, {
        name: "Mobil",
        category: "smaaanskaffelser",
        acquisitionDate: "2026-02-01",
        transactionDate: "2026-02-01",
        cost: 8000,
        purchaseDocumentId: docId,
        expenseAccountNo: "3000",
        thresholdRuleSource: "AL §6 stk. 1 nr. 2 — småanskaffelser",
        confirm: true,
      });
      expect(res.status).toBe(200);
      expect(res.body.ok).toBe(true);
      expect(res.body.writeOff.writeOffId).toBeGreaterThan(0);
      expect(res.body.writeOff.cost).toBe(8000);

      const list = await send(cfg, "GET", `/api/companies/${slug}/assets`);
      const writeOffs = list.body.assets.writeOffs as Array<{
        id: number;
        name: string;
        cost: number;
        thresholdRuleSource: string;
      }>;
      expect(writeOffs.length).toBe(1);
      expect(writeOffs[0]!.name).toBe("Mobil");
      expect(writeOffs[0]!.cost).toBe(8000);
      expect(writeOffs[0]!.thresholdRuleSource).toMatch(/småanskaffelser/);
      expect(list.body.assets.totals.writeOffCount).toBe(1);
      expect(list.body.assets.totals.writeOffTotal).toBe(8000);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("POST /assets/write-off without confirm: true is a 400", async () => {
    const { root: ws, slug } = makeWorkspace("anl-wo-confirm");
    try {
      const cfg = config(ws);
      const docId = seedPurchaseDocument(ws, slug, "wo-confirm", 5000);
      const res = await send(cfg, "POST", `/api/companies/${slug}/assets/write-off`, {
        name: "Tastatur",
        category: "smaaanskaffelser",
        acquisitionDate: "2026-02-01",
        transactionDate: "2026-02-01",
        cost: 5000,
        purchaseDocumentId: docId,
        expenseAccountNo: "3000",
        thresholdRuleSource: "AL §6 stk. 1 nr. 2",
      });
      expect(res.status).toBe(400);
      expect(res.body.ok).toBe(false);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("POST /assets rejects a bogus purchase document with a 4xx", async () => {
    const { root: ws, slug } = makeWorkspace("anl-bad-doc");
    try {
      const cfg = config(ws);
      const res = await send(cfg, "POST", `/api/companies/${slug}/assets`, {
        name: "Phantom",
        category: "hardware",
        acquisitionDate: "2026-01-01",
        cost: 10000,
        usefulLifeMonths: 24,
        purchaseDocumentId: 999999,
        confirm: true,
      });
      // Core surfaces "purchase document … does not exist" — server maps that
      // family of messages to a 409 conflict via withCompanyMutation.
      expect([400, 409]).toContain(res.status);
      expect(res.body.ok).toBe(false);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("ROUTE_CATALOG lists the four new asset routes", async () => {
    const { root: ws } = makeWorkspace("anl-catalog");
    try {
      const cfg = config(ws);
      const res = await send(cfg, "GET", "/api/health");
      expect(res.status).toBe(200);
      const patterns = (res.body.routes as Array<{ pattern: string }>).map(
        (r) => r.pattern,
      );
      expect(patterns).toContain("/api/companies/:slug/assets");
      expect(patterns).toContain("/api/companies/:slug/assets/:id/depreciate");
      expect(patterns).toContain("/api/companies/:slug/assets/write-off");
      expect(patterns).toContain(
        "/api/companies/:slug/assets/:id/next-depreciation",
      );
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});
