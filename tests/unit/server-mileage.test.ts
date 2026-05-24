// Tests: src/server/router.ts + src/server/data/company-views.ts (#335 —
// Cockpit Kørsel view). Covers the read route (`GET .../mileage`) and the
// write route (`POST .../mileage`), including the confirm gate, the
// per-month summary card and the rejection of bad inputs.

import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { handleRequest } from "../../src/server/router";
import type { ServerConfig } from "../../src/server/config";
import { createCompany } from "../../src/core/company";
import { initWorkspace } from "../../src/core/workspace";

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
  const res = await handleRequest(
    new Request(`http://localhost${path}`, init),
    cfg,
  );
  return { status: res.status, body: await res.json() };
}

const VALID_TRIP = {
  tripDate: "2026-03-15",
  purpose: "Kundebesøg Aarhus",
  fromLocation: "København",
  toLocation: "Aarhus",
  kilometers: 312,
  vehicle: "Privat bil",
  driver: "Owner",
  ratePerKm: 3.79,
  // The rate basis is a free-text source-backed note the caller confirms — the
  // mileage core deliberately does not own a tax rate.
  rateBasis: "SKAT befordringsfradrag 2026 (lav)",
  confirm: true,
};

describe("Cockpit Kørsel-routes (#335)", () => {
  test("GET /mileage returns an empty year before any trip is registered", async () => {
    const { root, slug } = makeWorkspace("mileage-empty");
    try {
      const cfg = config(root);
      const res = await send(
        cfg,
        "GET",
        `/api/companies/${slug}/mileage?year=2026`,
      );
      expect(res.status).toBe(200);
      expect(res.body.ok).toBe(true);
      const m = res.body.mileage;
      expect(m.selectedYear).toBe("2026");
      expect(m.entries).toEqual([]);
      expect(m.tripCount).toBe(0);
      expect(m.totalKilometers).toBe(0);
      expect(m.totalAmountBasis).toBe(0);
      // Twelve months, jan..dec, all zero.
      expect(m.months.length).toBe(12);
      expect(m.months[0].label).toBe("jan");
      expect(m.months[0].tripCount).toBe(0);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  test("POST /mileage registers a trip and the GET surfaces it with the per-month totals", async () => {
    const { root, slug } = makeWorkspace("mileage-create");
    try {
      const cfg = config(root);
      const created = await send(
        cfg,
        "POST",
        `/api/companies/${slug}/mileage`,
        VALID_TRIP,
      );
      expect(created.status).toBe(200);
      expect(created.body.ok).toBe(true);
      expect(created.body.mileage.entryNo).toMatch(/^MIL-2026-\d{6}$/);
      // 312 km × 3.79 kr/km = 1182.48 kr — the core rounds with the shared
      // integer-øre money helpers.
      expect(created.body.mileage.amountBasis).toBeCloseTo(1182.48, 2);

      const list = await send(
        cfg,
        "GET",
        `/api/companies/${slug}/mileage?year=2026`,
      );
      expect(list.status).toBe(200);
      const m = list.body.mileage;
      expect(m.entries.length).toBe(1);
      expect(m.entries[0].purpose).toBe("Kundebesøg Aarhus");
      expect(m.entries[0].kilometers).toBe(312);
      expect(m.tripCount).toBe(1);
      expect(m.totalKilometers).toBe(312);
      expect(m.totalAmountBasis).toBeCloseTo(1182.48, 2);
      // March is months[2] (0-indexed) — the trip's month.
      const march = m.months[2];
      expect(march.label).toBe("mar");
      expect(march.tripCount).toBe(1);
      expect(march.kilometers).toBe(312);
      // Other months stay at zero.
      expect(m.months[0].tripCount).toBe(0);
      expect(m.months[11].tripCount).toBe(0);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  test("POST /mileage without confirm:true is rejected — write-irreversibility gate", async () => {
    const { root, slug } = makeWorkspace("mileage-no-confirm");
    try {
      const cfg = config(root);
      const { confirm: _confirm, ...withoutConfirm } = VALID_TRIP;
      const res = await send(
        cfg,
        "POST",
        `/api/companies/${slug}/mileage`,
        withoutConfirm,
      );
      expect(res.status).toBe(400);
      expect(res.body.ok).toBe(false);
      expect(String(res.body.error.message)).toMatch(/confirm/i);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  test("POST /mileage with missing required fields surfaces core validation as a 400", async () => {
    const { root, slug } = makeWorkspace("mileage-bad-input");
    try {
      const cfg = config(root);
      const res = await send(
        cfg,
        "POST",
        `/api/companies/${slug}/mileage`,
        { ...VALID_TRIP, kilometers: 0 },
      );
      expect(res.status).toBe(400);
      expect(res.body.ok).toBe(false);
      expect(String(res.body.error.message)).toMatch(/positive/i);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  test("GET /mileage isolates entries by fiscal year — last year's trip is not in this year's list", async () => {
    const { root, slug } = makeWorkspace("mileage-year-isolated");
    try {
      const cfg = config(root);
      // One trip in 2025, one in 2026.
      await send(cfg, "POST", `/api/companies/${slug}/mileage`, {
        ...VALID_TRIP,
        tripDate: "2025-06-01",
      });
      await send(cfg, "POST", `/api/companies/${slug}/mileage`, VALID_TRIP);

      const r2025 = await send(
        cfg,
        "GET",
        `/api/companies/${slug}/mileage?year=2025`,
      );
      const r2026 = await send(
        cfg,
        "GET",
        `/api/companies/${slug}/mileage?year=2026`,
      );
      expect(r2025.body.mileage.entries.length).toBe(1);
      expect(r2025.body.mileage.entries[0].tripDate).toBe("2025-06-01");
      expect(r2026.body.mileage.entries.length).toBe(1);
      expect(r2026.body.mileage.entries[0].tripDate).toBe("2026-03-15");
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  test("the route catalog advertises both mileage routes", async () => {
    const { root, slug: _slug } = makeWorkspace("mileage-catalog");
    try {
      const cfg = config(root);
      const res = await send(cfg, "GET", "/api/health");
      expect(res.status).toBe(200);
      const routes = res.body.routes as Array<{ method: string; pattern: string }>;
      expect(
        routes.some(
          (r) =>
            r.method === "GET" && r.pattern === "/api/companies/:slug/mileage",
        ),
      ).toBe(true);
      expect(
        routes.some(
          (r) =>
            r.method === "POST" && r.pattern === "/api/companies/:slug/mileage",
        ),
      ).toBe(true);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
});
