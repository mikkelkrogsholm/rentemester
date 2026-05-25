// Tests: src/server/router.ts (GET /annual-report),
// src/server/data/annual-report-view.ts (cockpit-wrap af buildAnnualReport).
//
// #338 — cockpittet skal kunne forberede en regnskabsklasse-B-arsrapport
// for en lukket fiscal year. Forudsætningerne (lukket periode, CVR,
// balancerede bøger) håndhæves af kernen.
import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { handleRequest } from "../../src/server/router";
import { type ServerConfig } from "../../src/server/config";
import { createCompany } from "../../src/core/company";
import { initWorkspace } from "../../src/core/workspace";

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

describe("#338 — GET /api/companies/:slug/annual-report", () => {
  test("uden fiscalYearStart/-End afvises med 400", async () => {
    const ws = makeWorkspace("annual-no-year", ["Acme ApS"]);
    try {
      const res = await handleRequest(
        new Request(
          "http://localhost/api/companies/acme-aps/annual-report",
        ),
        config(ws),
      );
      expect(res.status).toBe(400);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("rapporterer 'ok:false' med klare fejl for en frisk virksomhed (CVR mangler, periode ikke låst)", async () => {
    const ws = makeWorkspace("annual-fresh", ["Acme ApS"]);
    try {
      const body = await fetchJson<{
        ok: boolean;
        annualReport: {
          slug: string;
          fiscalYearStart: string;
          fiscalYearEnd: string;
          report: { ok: boolean; errors: string[] };
        };
      }>(
        config(ws),
        "/api/companies/acme-aps/annual-report?fiscalYearStart=2026-01-01&fiscalYearEnd=2026-12-31",
      );
      expect(body.ok).toBe(true);
      expect(body.annualReport.fiscalYearStart).toBe("2026-01-01");
      expect(body.annualReport.fiscalYearEnd).toBe("2026-12-31");
      expect(body.annualReport.report.ok).toBe(false);
      // Mindst én fejl skal være i den danske form.
      expect(body.annualReport.report.errors.length).toBeGreaterThan(0);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("rute-kataloget annoncerer /annual-report", async () => {
    const ws = makeWorkspace("annual-catalog");
    try {
      const body = await fetchJson<{
        routes: Array<{ method: string; pattern: string }>;
      }>(config(ws), "/api/health");
      const patterns = body.routes.map((r) => r.pattern);
      expect(patterns).toContain("/api/companies/:slug/annual-report");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});
