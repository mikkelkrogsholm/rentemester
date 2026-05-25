// Tests: src/server/router.ts (accounts-view dispatch),
// src/server/data/accounts-view.ts (kontoplan-listen).
//
// #344 — cockpittet skal vise SMB-ejeren kontoplanen som read-only liste
// med nummer, navn, type og evt. moms-mapping. Acceptkriterium: bruger
// eksisterende seedAccounts/accounts-tabellen (ingen genimplementering).
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

describe("#344 — GET /api/companies/:slug/accounts", () => {
  test("returnerer den seedede kontoplan sorteret efter kontonummer", async () => {
    const ws = makeWorkspace("accounts", ["Acme ApS"]);
    try {
      const body = await fetchJson<{
        ok: boolean;
        accounts: {
          slug: string;
          company: { name: string };
          accounts: Array<{
            accountNo: string;
            name: string;
            type: string;
            normalBalance: string;
            defaultVatCode: string | null;
            hasPostings: boolean;
          }>;
          byType: Record<string, number>;
        };
      }>(config(ws), "/api/companies/acme-aps/accounts");

      expect(body.ok).toBe(true);
      expect(body.accounts.slug).toBe("acme-aps");
      // Kontoplanen er ikke tom — seedAccounts plant'er omkring 40 konti.
      expect(body.accounts.accounts.length).toBeGreaterThan(20);

      // Sortering efter account_no — første-konto har lavest nummer.
      const firstNo = body.accounts.accounts[0]!.accountNo;
      const sorted = [...body.accounts.accounts]
        .map((a) => a.accountNo)
        .sort();
      expect(body.accounts.accounts.map((a) => a.accountNo)).toEqual(sorted);

      // Indeholder kerne-konti fra seedAccounts.
      const acc1000 = body.accounts.accounts.find(
        (a) => a.accountNo === "1000",
      );
      expect(acc1000).toBeDefined();
      expect(acc1000!.type).toBe("income");

      const acc2000 = body.accounts.accounts.find(
        (a) => a.accountNo === "2000",
      );
      expect(acc2000).toBeDefined();
      expect(acc2000!.type).toBe("asset");

      // Owner's draw + tax account fra #249.
      expect(
        body.accounts.accounts.find((a) => a.accountNo === "5010"),
      ).toBeDefined();
      expect(
        body.accounts.accounts.find((a) => a.accountNo === "7200"),
      ).toBeDefined();

      // byType-summen matcher den totale liste.
      const sum = Object.values(body.accounts.byType).reduce((a, b) => a + b, 0);
      expect(sum).toBe(body.accounts.accounts.length);

      // Ingen konti har bogføringslinjer i en frisk virksomhed.
      for (const a of body.accounts.accounts) {
        expect(a.hasPostings).toBe(false);
      }
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("ukendt slug giver en safe 404", async () => {
    const ws = makeWorkspace("accounts-404", []);
    try {
      const res = await handleRequest(
        new Request("http://localhost/api/companies/ghost/accounts"),
        config(ws),
      );
      expect(res.status).toBe(404);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("POST afvises med 405", async () => {
    const ws = makeWorkspace("accounts-405", ["Acme ApS"]);
    try {
      const res = await handleRequest(
        new Request("http://localhost/api/companies/acme-aps/accounts", {
          method: "POST",
        }),
        config(ws),
      );
      expect(res.status).toBe(405);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("rute-kataloget annoncerer /accounts", async () => {
    const ws = makeWorkspace("accounts-catalog");
    try {
      const body = await fetchJson<{
        routes: Array<{ method: string; pattern: string }>;
      }>(config(ws), "/api/health");
      const patterns = body.routes.map((r) => r.pattern);
      expect(patterns).toContain("/api/companies/:slug/accounts");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});
