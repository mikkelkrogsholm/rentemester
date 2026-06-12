// Tests: src/server/router.ts (GET /api/rules dispatch + handler).
//
// #347 — Lovgrundlag-viewer. Endpointet eksponerer bundler, regler og
// retsinformation-citationer fra `rules/dk/` + `sources/legal-sources.json`,
// så cockpittet kan vise SMB-ejeren *hvilke* regler der styrer bogføringen og
// *hvor* de er hentet fra. Read-only — regler kan kun ændres via PR i
// `rules/dk/`.
import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { handleRequest } from "../../src/server/router";
import { type ServerConfig } from "../../src/server/config";
import { initWorkspace } from "../../src/core/workspace";

function makeWorkspace(label: string) {
  const root = mkdtempSync(join(tmpdir(), `rentemester-${label}-`));
  initWorkspace(root);
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

describe("#347 — GET /api/rules (Lovgrundlag-viewer)", () => {
  test("returnerer bundler, regler og legal-sources", async () => {
    const ws = makeWorkspace("rules-viewer");
    try {
      const body = await fetchJson<{
        ok: boolean;
        ruleBundles: Array<{ name: string; version: string; ruleCount: number }>;
        rules: Array<{
          ruleId: string;
          bundle: string;
          sourceId: string;
          provisions: Array<{ ref: string; textHash: string }>;
        }>;
        legalSources: Array<{ id: string; url: string; title: string }>;
      }>(config(ws), "/api/rules");

      expect(body.ok).toBe(true);
      // Mindst én bundle og hver bundle har en SemVer-version + et ikke-tomt navn.
      expect(body.ruleBundles.length).toBeGreaterThan(0);
      for (const b of body.ruleBundles) {
        expect(b.name.length).toBeGreaterThan(0);
        // Versionsstrengen er `<bundle-key>-v<semver>` per `currentRuleBundleVersion`.
        expect(b.version).toMatch(/v\d+\.\d+\.\d+/);
        expect(b.ruleCount).toBeGreaterThanOrEqual(0);
      }
      // Hver regel har en SHA-256-citation pr. provision.
      expect(body.rules.length).toBeGreaterThan(0);
      const ruleWithProv = body.rules.find((r) => r.provisions.length > 0);
      expect(ruleWithProv).toBeDefined();
      for (const p of ruleWithProv!.provisions) {
        expect(p.ref.length).toBeGreaterThan(0);
        // Citationerne præfikses med "sha256:" så algoritmen er eksplicit.
        expect(p.textHash).toMatch(/^sha256:[a-f0-9]{64}$/);
      }
      // Hver regel der har en source_id deklareret skal pege på en kilde der
      // findes i legal-sources. Tomme source_id'er er ikke en fejl her: nogle
      // regler citerer kun via inline-provisions (textHash) uden en samlet
      // top-level kilde-reference.
      const sourceIds = new Set(body.legalSources.map((s) => s.id));
      for (const r of body.rules) {
        if (r.sourceId && r.sourceId.length > 0) {
          expect(sourceIds.has(r.sourceId)).toBe(true);
        }
      }
      // Legal-sources har URL'er til retsinformation.dk (eller anden auth-kilde).
      for (const s of body.legalSources) {
        expect(s.url).toMatch(/^https?:\/\//);
        expect(s.title.length).toBeGreaterThan(0);
      }
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("rute-kataloget annoncerer /api/rules", async () => {
    const ws = makeWorkspace("rules-catalog");
    try {
      const body = await fetchJson<{
        routes: Array<{ method: string; pattern: string }>;
      }>(config(ws), "/api/health");
      const patterns = body.routes.map((r) => `${r.method} ${r.pattern}`);
      expect(patterns).toContain("GET /api/rules");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("POST afvises med 405", async () => {
    const ws = makeWorkspace("rules-405");
    try {
      const res = await handleRequest(
        new Request("http://localhost/api/rules", { method: "POST" }),
        config(ws),
      );
      expect(res.status).toBe(405);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});
