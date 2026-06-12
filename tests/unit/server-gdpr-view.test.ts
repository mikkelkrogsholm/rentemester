// Tests: src/server/router.ts (GDPR export/erase dispatch),
// src/server/data/gdpr-view.ts (cockpit-venligt wrap af
// buildGdprSubjectExport).
//
// #334 — cockpittet skal kunne svare på en GDPR-indsigtsanmodning ved at
// finde personoplysninger for en data-subject (CVR eller navn) og
// derefter anonymisere data via en separat write-handler.
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

describe("#334 — GDPR-view", () => {
  test("GET /gdpr/export uden cvr/name afvises med 400", async () => {
    const ws = makeWorkspace("gdpr-no-key", ["Acme ApS"]);
    try {
      const res = await handleRequest(
        new Request("http://localhost/api/companies/acme-aps/gdpr/export"),
        config(ws),
      );
      expect(res.status).toBe(400);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("GET /gdpr/export?cvr=… returnerer en tom indsigtsrapport for ukendt subject", async () => {
    const ws = makeWorkspace("gdpr-empty", ["Acme ApS"]);
    try {
      const body = await fetchJson<{
        ok: boolean;
        gdpr: {
          slug: string;
          company: { name: string };
          export: {
            ok: boolean;
            asOf: string;
            subject: { cvr: string | null; name: string | null };
            records: any[];
            appliedRules: string[];
          };
        };
      }>(
        config(ws),
        "/api/companies/acme-aps/gdpr/export?cvr=DK99999999",
      );
      expect(body.ok).toBe(true);
      expect(body.gdpr.export.ok).toBe(true);
      expect(body.gdpr.export.subject.cvr).toBe("DK99999999");
      expect(body.gdpr.export.records).toEqual([]);
      // appliedRules skal komme fra rules/dk pipelinen
      expect(body.gdpr.export.appliedRules.length).toBeGreaterThan(0);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("POST /gdpr/erase uden cvr/name afvises med 400", async () => {
    const ws = makeWorkspace("gdpr-erase-400", ["Acme ApS"]);
    try {
      const res = await handleRequest(
        new Request("http://localhost/api/companies/acme-aps/gdpr/erase", {
          method: "POST",
          // host: 127.0.0.1 — write-pipelinens localhost-gate (#213).
          headers: {
            "content-type": "application/json",
            host: "127.0.0.1",
          },
          body: JSON.stringify({}),
        }),
        config(ws),
      );
      expect(res.status).toBe(400);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("rute-kataloget annoncerer /gdpr/export og /gdpr/erase", async () => {
    const ws = makeWorkspace("gdpr-catalog");
    try {
      const body = await fetchJson<{
        routes: Array<{ method: string; pattern: string }>;
      }>(config(ws), "/api/health");
      const patterns = body.routes.map((r) => `${r.method} ${r.pattern}`);
      expect(patterns).toContain("GET /api/companies/:slug/gdpr/export");
      expect(patterns).toContain("POST /api/companies/:slug/gdpr/erase");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});
