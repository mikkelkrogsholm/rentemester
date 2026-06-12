// Tests: src/server/router.ts (GET /exceptions),
// src/server/data/exceptions-list.ts (cockpit-venligt wrap af
// `listExceptions`).
//
// #332 — cockpittet skal kunne vise SMB-ejeren en undtagelses-kø
// (unmatched bank-rows, blokerede write-flows osv.) med filter pr.
// status (open/resolved/all). POST-resolve er en separat handler.
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
import { recordException } from "../../src/core/exceptions";

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

let exceptionTag = 0;
function seedException(
  ws: string,
  slug: string,
  severity: "low" | "medium" | "high",
) {
  exceptionTag += 1;
  const companyRoot = companyRootForSlug(ws, slug);
  const dbPath = companyPaths(companyRoot).db;
  const db = openDb(dbPath);
  try {
    migrate(db);
    // `recordException` dedupes på (type, message, related ids, required_action).
    // En unik message-suffix sikrer at hver indkaldelse opretter en separat række.
    recordException(db, {
      type: "TEST_EXCEPTION",
      severity,
      message: `synthetic ${severity} exception #${exceptionTag}`,
      requiredAction: "verify in tests",
    });
  } finally {
    db.close();
  }
}

describe("#332 — GET /api/companies/:slug/exceptions", () => {
  test("returnerer åbne undtagelser med severity-tæller", async () => {
    const ws = makeWorkspace("exceptions-list", ["Acme ApS"]);
    try {
      seedException(ws, "acme-aps", "high");
      seedException(ws, "acme-aps", "medium");
      seedException(ws, "acme-aps", "medium");

      const body = await fetchJson<{
        ok: boolean;
        exceptions: {
          slug: string;
          company: { name: string };
          status: string;
          count: number;
          rows: Array<{ id: number; severity: string; status: string; message: string }>;
          bySeverity: { high: number; medium: number; low: number };
        };
      }>(config(ws), "/api/companies/acme-aps/exceptions");

      expect(body.ok).toBe(true);
      expect(body.exceptions.slug).toBe("acme-aps");
      expect(body.exceptions.status).toBe("open");
      expect(body.exceptions.count).toBe(3);
      expect(body.exceptions.bySeverity).toEqual({
        high: 1,
        medium: 2,
        low: 0,
      });
      for (const row of body.exceptions.rows) {
        expect(row.status).toBe("open");
        expect(row.message).toMatch(/synthetic/);
      }
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("?status=all viser også resolved undtagelser", async () => {
    const ws = makeWorkspace("exceptions-all", ["Acme ApS"]);
    try {
      seedException(ws, "acme-aps", "low");

      const body = await fetchJson<{
        exceptions: { count: number; status: string };
      }>(config(ws), "/api/companies/acme-aps/exceptions?status=all");

      expect(body.exceptions.status).toBe("all");
      expect(body.exceptions.count).toBe(1);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("?status=garbage afvises med 400", async () => {
    const ws = makeWorkspace("exceptions-400", ["Acme ApS"]);
    try {
      const res = await handleRequest(
        new Request(
          "http://localhost/api/companies/acme-aps/exceptions?status=garbage",
        ),
        config(ws),
      );
      expect(res.status).toBe(400);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("rute-kataloget annoncerer /exceptions", async () => {
    const ws = makeWorkspace("exceptions-catalog");
    try {
      const body = await fetchJson<{
        routes: Array<{ method: string; pattern: string }>;
      }>(config(ws), "/api/health");
      const patterns = body.routes.map((r) => `${r.method} ${r.pattern}`);
      expect(patterns).toContain("GET /api/companies/:slug/exceptions");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});
