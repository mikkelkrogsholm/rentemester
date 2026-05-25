// Tests: src/server/router.ts (GET /periods),
// src/server/data/periods-view.ts (cockpit-venligt wrap af
// accounting_periods + effectivePeriodState).
//
// #342 — cockpittet skal vise SMB-ejeren alle perioder med deres
// effective status (open/closed/reported). Close/reopen er separate
// write-handlers der allerede eksisterer for CLI.
import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { handleRequest } from "../../src/server/router";
import { type ServerConfig } from "../../src/server/config";
import { createCompany } from "../../src/core/company";
import { initWorkspace, companyRootForSlug } from "../../src/core/workspace";
import { companyPaths } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { closeAccountingPeriod } from "../../src/core/periods";

function makeWorkspace(label: string, companyNames: string[] = []) {
  const root = mkdtempSync(join(tmpdir(), `rentemester-${label}-`));
  initWorkspace(root);
  for (const name of companyNames) createCompany(root, { name });
  return root;
}

function config(workspaceRoot: string): ServerConfig {
  return {
    host: "127.0.0.1",
    port: 0,
    workspaceRoot,
    authRequired: false,
    authToken: null,
  };
}

async function fetchJson<T>(cfg: ServerConfig, path: string): Promise<T> {
  const res = await handleRequest(new Request(`http://localhost${path}`), cfg);
  return (await res.json()) as T;
}

function closeQuarterlyVatPeriod(
  ws: string,
  slug: string,
  start: string,
  end: string,
) {
  const dbPath = companyPaths(companyRootForSlug(ws, slug)).db;
  const db = openDb(dbPath);
  try {
    migrate(db);
    const r = closeAccountingPeriod(db, {
      periodStart: start,
      periodEnd: end,
      kind: "vat_quarter",
      createdBy: "user:test",
      createdByProgram: "rentemester-test",
    });
    if (!r.ok) throw new Error("close failed: " + r.errors.join("; "));
  } finally {
    db.close();
  }
}

describe("#342 — GET /api/companies/:slug/periods", () => {
  test("returnerer tom liste i en frisk virksomhed", async () => {
    const ws = makeWorkspace("periods-empty", ["Acme ApS"]);
    try {
      const body = await fetchJson<{
        ok: boolean;
        periods: {
          slug: string;
          company: { name: string };
          periods: any[];
          byStatus: { open: number; closed: number; reported: number };
        };
      }>(config(ws), "/api/companies/acme-aps/periods");
      expect(body.ok).toBe(true);
      expect(body.periods.periods).toEqual([]);
      expect(body.periods.byStatus).toEqual({
        open: 0,
        closed: 0,
        reported: 0,
      });
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("viser en lukket periode med effective status closed", async () => {
    const ws = makeWorkspace("periods-closed", ["Acme ApS"]);
    try {
      closeQuarterlyVatPeriod(ws, "acme-aps", "2026-01-01", "2026-03-31");
      const body = await fetchJson<{
        periods: {
          periods: Array<{
            periodStart: string;
            periodEnd: string;
            kind: string;
            rowStatus: string;
            effectiveStatus: string;
          }>;
          byStatus: { open: number; closed: number; reported: number };
        };
      }>(config(ws), "/api/companies/acme-aps/periods");

      expect(body.periods.periods.length).toBe(1);
      const p = body.periods.periods[0]!;
      expect(p.periodStart).toBe("2026-01-01");
      expect(p.periodEnd).toBe("2026-03-31");
      expect(p.kind).toBe("vat_quarter");
      expect(p.rowStatus).toBe("closed");
      expect(p.effectiveStatus).toBe("closed");
      expect(body.periods.byStatus.closed).toBe(1);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("rute-kataloget annoncerer GET /periods, POST /periods/close og /periods/reopen", async () => {
    const ws = makeWorkspace("periods-catalog");
    try {
      const body = await fetchJson<{
        routes: Array<{ method: string; pattern: string }>;
      }>(config(ws), "/api/health");
      const patterns = body.routes.map((r) => `${r.method} ${r.pattern}`);
      expect(patterns).toContain("GET /api/companies/:slug/periods");
      expect(patterns).toContain("POST /api/companies/:slug/periods/close");
      expect(patterns).toContain("POST /api/companies/:slug/periods/reopen");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("POST /periods afvises (write-routerne hænger på /periods/close og /periods/reopen)", async () => {
    const ws = makeWorkspace("periods-405", ["Acme ApS"]);
    try {
      const res = await handleRequest(
        new Request("http://localhost/api/companies/acme-aps/periods", {
          method: "POST",
        }),
        config(ws),
      );
      expect(res.status).toBe(405);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});
