// Tests: src/server/router.ts, src/server/data.ts — the CVR cockpit endpoints
// GET /api/companies/:slug/company and POST /api/companies/:slug/sync-cvr.
import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { handleRequest } from "../../src/server/router";
import type { ServerConfig } from "../../src/server/config";
import { createCompany } from "../../src/core/company";
import { initWorkspace } from "../../src/core/workspace";

function config(workspaceRoot: string): ServerConfig {
  return { host: "127.0.0.1", port: 0, authRequired: false, authToken: null, workspaceRoot };
}

async function call(cfg: ServerConfig, path: string, init?: RequestInit) {
  const res = await handleRequest(new Request(`http://localhost${path}`, init), cfg);
  return { status: res.status, body: await res.json() };
}

describe("cockpit API — CVR endpoints", () => {
  test("GET /api/companies/:slug/company returns the full settings row", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-srv-cvr-"));
    initWorkspace(root);
    createCompany(root, { name: "Acme", cvr: "DK12345678" });
    const cfg = config(root);

    const { status, body } = await call(cfg, "/api/companies/acme/company");
    expect(status).toBe(200);
    expect(body.ok).toBe(true);
    expect(body.company.cvr).toBe("DK12345678");
    // The CVR stamdata columns exist and are null until a sync runs.
    expect(body.company.address).toBeNull();
    expect(body.company.cvrSyncedAt).toBeNull();

    rmSync(root, { recursive: true, force: true });
  });

  test("GET .../company for an unknown slug is a safe 404", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-srv-cvr-404-"));
    initWorkspace(root);
    const { status, body } = await call(config(root), "/api/companies/ghost/company");
    expect(status).toBe(404);
    expect(body.ok).toBe(false);
    rmSync(root, { recursive: true, force: true });
  });

  test("a GET on the sync-cvr route is 405 — it is POST-only", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-srv-cvr-405-"));
    initWorkspace(root);
    createCompany(root, { name: "Acme" });
    const { status } = await call(config(root), "/api/companies/acme/sync-cvr");
    expect(status).toBe(405);
    rmSync(root, { recursive: true, force: true });
  });

  test("POST .../sync-cvr without a registered CVR reports the failure inside sync.ok", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-srv-cvr-nocvr-"));
    initWorkspace(root);
    createCompany(root, { name: "Acme" }); // no CVR number

    const { status, body } = await call(config(root), "/api/companies/acme/sync-cvr", {
      method: "POST",
    });
    // The HTTP call succeeds; the CVR failure is carried structurally.
    expect(status).toBe(200);
    expect(body.ok).toBe(true);
    expect(body.sync.ok).toBe(false);
    expect(body.sync.errors[0]).toContain("CVR-nummer");

    rmSync(root, { recursive: true, force: true });
  });

  // #402 — the cockpit needs a way to tell the owner whether the CVR
  // credentials are configured *before* they click "Hent fra CVR" and get a
  // silent failure. The cockpit reads this endpoint and disables the button
  // (and hides the developer-language hint) when `configured` is false.
  test("GET /api/system/cvr-status reflects env credentials", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-srv-cvr-status-"));
    initWorkspace(root);
    const cfg = config(root);

    const prevUser = process.env.CVR_USERNAME;
    const prevPass = process.env.CVR_PASSWORD;
    try {
      delete process.env.CVR_USERNAME;
      delete process.env.CVR_PASSWORD;
      const missing = await call(cfg, "/api/system/cvr-status");
      expect(missing.status).toBe(200);
      expect(missing.body.ok).toBe(true);
      expect(missing.body.cvrStatus.configured).toBe(false);

      process.env.CVR_USERNAME = "u";
      process.env.CVR_PASSWORD = "p";
      const present = await call(cfg, "/api/system/cvr-status");
      expect(present.status).toBe(200);
      expect(present.body.ok).toBe(true);
      expect(present.body.cvrStatus.configured).toBe(true);
    } finally {
      if (prevUser === undefined) delete process.env.CVR_USERNAME;
      else process.env.CVR_USERNAME = prevUser;
      if (prevPass === undefined) delete process.env.CVR_PASSWORD;
      else process.env.CVR_PASSWORD = prevPass;
    }

    rmSync(root, { recursive: true, force: true });
  });
});
