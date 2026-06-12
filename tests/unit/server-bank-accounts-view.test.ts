// Tests: src/server/router.ts (GET/POST /bank-accounts),
// src/server/data/bank-accounts-view.ts (cockpit-venligt wrap af
// listBankAccounts + de indbyggede CSV-mapping-profiler).
//
// #345 — cockpittet skal kunne vise SMB-ejeren alle registrerede bankkonti
// og de indbyggede mapping-profiler (Lunar, Danske Bank, …) som
// BankImportModal genbruger.
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

describe("#345 — GET /api/companies/:slug/bank-accounts", () => {
  test("returnerer en tom liste i en frisk virksomhed og de indbyggede profiler", async () => {
    const ws = makeWorkspace("bank-accounts-empty", ["Acme ApS"]);
    try {
      const body = await fetchJson<{
        ok: boolean;
        bankAccounts: {
          slug: string;
          company: { name: string };
          accounts: Array<{ id: number; slug: string; name: string }>;
          profiles: Array<{ name: string; bankName?: string }>;
        };
      }>(config(ws), "/api/companies/acme-aps/bank-accounts");
      expect(body.ok).toBe(true);
      expect(body.bankAccounts.slug).toBe("acme-aps");
      // Frisk virksomhed har ingen konti.
      expect(body.bankAccounts.accounts).toEqual([]);
      // De indbyggede profiler skal være tilgængelige (Lunar, Danske Bank
      // m.fl. er hard-coded i bank-profiles.ts).
      expect(body.bankAccounts.profiles.length).toBeGreaterThan(0);
      const profileNames = body.bankAccounts.profiles.map((p) => p.name);
      expect(profileNames.length).toBeGreaterThan(0);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("rute-kataloget annoncerer GET + POST /bank-accounts", async () => {
    const ws = makeWorkspace("bank-accounts-catalog");
    try {
      const body = await fetchJson<{
        routes: Array<{ method: string; pattern: string }>;
      }>(config(ws), "/api/health");
      const patterns = body.routes.map((r) => `${r.method} ${r.pattern}`);
      expect(patterns).toContain("GET /api/companies/:slug/bank-accounts");
      expect(patterns).toContain("POST /api/companies/:slug/bank-accounts");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("ukendt slug giver en safe 404", async () => {
    const ws = makeWorkspace("bank-accounts-404", []);
    try {
      const res = await handleRequest(
        new Request("http://localhost/api/companies/ghost/bank-accounts"),
        config(ws),
      );
      expect(res.status).toBe(404);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});
