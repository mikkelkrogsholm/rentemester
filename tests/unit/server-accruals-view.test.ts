// Tests: src/server/router.ts (GET /accruals),
// src/server/data/accruals-view.ts (cockpit-wrap af
// buildAccrualRegisterReport).
//
// #337 — cockpittet skal kunne vise SMB-ejeren periodiseringer (forudbetalte
// omkostninger, skyldige omkostninger, udskudt omsætning) med remaining-
// balance og recognized-amount.
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

describe("#337 — GET /api/companies/:slug/accruals", () => {
  test("returnerer tomt register for en frisk virksomhed", async () => {
    const ws = makeWorkspace("accruals-empty", ["Acme ApS"]);
    try {
      const body = await fetchJson<{
        ok: boolean;
        accruals: {
          slug: string;
          company: { name: string };
          report: {
            ok: boolean;
            accruals: any[];
            totals: {
              totalAmount: number;
              recognizedAmount: number;
              remainingAmount: number;
            };
          };
        };
      }>(config(ws), "/api/companies/acme-aps/accruals");
      expect(body.ok).toBe(true);
      expect(body.accruals.report.ok).toBe(true);
      expect(body.accruals.report.accruals).toEqual([]);
      expect(body.accruals.report.totals.totalAmount).toBe(0);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("rute-kataloget annoncerer /accruals", async () => {
    const ws = makeWorkspace("accruals-catalog");
    try {
      const body = await fetchJson<{
        routes: Array<{ method: string; pattern: string }>;
      }>(config(ws), "/api/health");
      const patterns = body.routes.map((r) => r.pattern);
      expect(patterns).toContain("/api/companies/:slug/accruals");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("ukendt slug giver en safe 404", async () => {
    const ws = makeWorkspace("accruals-404", []);
    try {
      const res = await handleRequest(
        new Request("http://localhost/api/companies/ghost/accruals"),
        config(ws),
      );
      expect(res.status).toBe(404);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});
