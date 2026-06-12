// Tests: src/server/router.ts (retention-view dispatch),
// src/server/data/retention-view.ts (cockpit-venligt wrap af
// `buildRetentionStatusReport` fra kernen).
//
// #343 — cockpittet skal vise SMB-ejeren den 5-årige opbevaringspligt pr.
// data-domæne (bilag, posteringer, banktransaktioner), så ejeren kan se
// hvad der nærmer sig udløb. Citationen peger på bogføringslovens § 12,
// stk. 1 og deep-linker til Lovgrundlag-viewet (#347).
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

describe("#343 — GET /api/companies/:slug/retention", () => {
  test("returnerer 5-års retention-status pr. data-domæne", async () => {
    const ws = makeWorkspace("retention", ["Acme ApS"]);
    try {
      const body = await fetchJson<{
        ok: boolean;
        retention: {
          slug: string;
          company: { name: string; cvr: string | null };
          report: {
            ok: boolean;
            asOf: string;
            appliedRules: string[];
            rows: Array<{
              table: string;
              total: number;
              expired: number;
              nextExpiry: string | null;
              oldestExpired: string | null;
            }>;
          };
          legalCitation: { sourceId: string; note: string };
        };
      }>(config(ws), "/api/companies/acme-aps/retention");

      expect(body.ok).toBe(true);
      expect(body.retention.slug).toBe("acme-aps");
      expect(body.retention.company.name).toBe("Acme ApS");
      // Tre domæner skal være repræsenteret: documents, journal_entries,
      // bank_transactions — også når de er tomme (total = 0).
      const tables = body.retention.report.rows.map((r) => r.table).sort();
      expect(tables).toEqual([
        "bank_transactions",
        "documents",
        "journal_entries",
      ]);
      for (const row of body.retention.report.rows) {
        expect(row.total).toBe(0);
        expect(row.expired).toBe(0);
        expect(row.nextExpiry).toBeNull();
        expect(row.oldestExpired).toBeNull();
      }
      // Den anvendte regel kommer fra rules/dk-pipelinen.
      expect(body.retention.report.appliedRules).toContain(
        "DK-BOOKKEEPING-RETENTION-001",
      );
      // Legal-citationen deep-linker til Lovgrundlag-viewet via sourceId.
      expect(body.retention.legalCitation.sourceId).toBe(
        "DK-BOGFORINGSLOVEN-2022-700",
      );
      expect(body.retention.legalCitation.note).toContain("§ 12");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("ukendt slug giver en safe 404", async () => {
    const ws = makeWorkspace("retention-404", []);
    try {
      const res = await handleRequest(
        new Request("http://localhost/api/companies/ghost/retention"),
        config(ws),
      );
      expect(res.status).toBe(404);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("POST afvises med 405", async () => {
    const ws = makeWorkspace("retention-405", ["Acme ApS"]);
    try {
      const res = await handleRequest(
        new Request("http://localhost/api/companies/acme-aps/retention", {
          method: "POST",
        }),
        config(ws),
      );
      expect(res.status).toBe(405);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("rute-kataloget annoncerer /retention", async () => {
    const ws = makeWorkspace("retention-catalog");
    try {
      const body = await fetchJson<{
        routes: Array<{ method: string; pattern: string }>;
      }>(config(ws), "/api/health");
      const patterns = body.routes.map((r) => r.pattern);
      expect(patterns).toContain("/api/companies/:slug/retention");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});
