// Tests: src/server/router.ts (integrity-view dispatch),
// src/server/data/integrity-view.ts (cockpit-venligt wrap af
// `verifyAuditChain` + `getBackupComplianceStatus` + `listBackupDestinations`).
//
// #333 — cockpittet skal vise SMB-ejeren et "Integritet & backup"-panel: er
// hash-kæden hel, hvor er den brudt, hvornår sidst backup, og hvilke
// destinationer er konfigureret. Endpointet er idempotent — verifikationen
// er read-only.
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

describe("#333 — GET /api/companies/:slug/integrity", () => {
  test("returnerer audit chain, backup-status og destinations", async () => {
    const ws = makeWorkspace("integrity", ["Acme ApS"]);
    try {
      const body = await fetchJson<{
        ok: boolean;
        integrity: {
          slug: string;
          company: { name: string; cvr: string | null };
          auditChain: { ok: boolean; entries: number; errors: string[] };
          backup: {
            ok: boolean;
            latestBackupAt: string | null;
            backupDue: boolean;
            checkedAt: string;
          };
          destinations: Array<{ id: string; label: string }>;
          legalCitation: { sourceId: string; note: string };
        };
      }>(config(ws), "/api/companies/acme-aps/integrity");

      expect(body.ok).toBe(true);
      expect(body.integrity.slug).toBe("acme-aps");
      expect(body.integrity.company.name).toBe("Acme ApS");

      // En frisk workspace har ingen postings ⇒ audit-chain er triviel hel.
      expect(body.integrity.auditChain.ok).toBe(true);
      expect(body.integrity.auditChain.entries).toBe(0);
      expect(body.integrity.auditChain.errors).toEqual([]);

      // Ingen backup endnu ⇒ backupDue er sand (bogføringsloven kræver én).
      expect(body.integrity.backup.latestBackupAt).toBeNull();
      expect(typeof body.integrity.backup.checkedAt).toBe("string");

      // Ingen destinationer konfigureret endnu.
      expect(body.integrity.destinations).toEqual([]);

      // Legal-citation deep-linker til Lovgrundlag-viewet (#347).
      expect(body.integrity.legalCitation.sourceId).toBe(
        "DK-BOGFORINGSLOVEN-2022-700",
      );
      expect(body.integrity.legalCitation.note).toContain("§ 14");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("er idempotent — to kald giver samme audit-chain-resultat", async () => {
    const ws = makeWorkspace("integrity-idem", ["Acme ApS"]);
    try {
      const a = await fetchJson<{
        integrity: { auditChain: { ok: boolean; entries: number } };
      }>(config(ws), "/api/companies/acme-aps/integrity");
      const b = await fetchJson<{
        integrity: { auditChain: { ok: boolean; entries: number } };
      }>(config(ws), "/api/companies/acme-aps/integrity");
      expect(a.integrity.auditChain).toEqual(b.integrity.auditChain);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("ukendt slug giver en safe 404", async () => {
    const ws = makeWorkspace("integrity-404", []);
    try {
      const res = await handleRequest(
        new Request("http://localhost/api/companies/ghost/integrity"),
        config(ws),
      );
      expect(res.status).toBe(404);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("POST afvises med 405", async () => {
    const ws = makeWorkspace("integrity-405", ["Acme ApS"]);
    try {
      const res = await handleRequest(
        new Request("http://localhost/api/companies/acme-aps/integrity", {
          method: "POST",
        }),
        config(ws),
      );
      expect(res.status).toBe(405);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("rute-kataloget annoncerer /integrity", async () => {
    const ws = makeWorkspace("integrity-catalog");
    try {
      const body = await fetchJson<{
        routes: Array<{ method: string; pattern: string }>;
      }>(config(ws), "/api/health");
      const patterns = body.routes.map((r) => r.pattern);
      expect(patterns).toContain("/api/companies/:slug/integrity");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});
