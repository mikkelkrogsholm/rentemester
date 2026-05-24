// Tests for the cockpit SPA additions to `rentemester serve` (#171):
//   - PATCH /api/companies/:slug — rename + archive (non-destructive)
//   - static serving of the built React app (with the index.html fallback)
import { describe, expect, test } from "bun:test";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { handleRequest } from "../../src/server/router";
import type { ServerConfig } from "../../src/server/config";
import { createCompany } from "../../src/core/company";
import { initWorkspace, listWorkspaceCompanies } from "../../src/core/workspace";

function tmpRoot(label: string) {
  return mkdtempSync(join(tmpdir(), `rentemester-${label}-`));
}

function makeWorkspace(label: string, companyNames: string[] = []) {
  const root = tmpRoot(label);
  initWorkspace(root);
  for (const name of companyNames) createCompany(root, { name });
  return root;
}

function config(overrides: Partial<ServerConfig> & { workspaceRoot: string }): ServerConfig {
  return {
    host: "127.0.0.1",
    port: 0,
    authRequired: false,
    authToken: null,
    ...overrides,
  };
}

async function call(cfg: ServerConfig, path: string, init?: RequestInit) {
  return handleRequest(new Request(`http://localhost${path}`, init), cfg);
}

async function json(cfg: ServerConfig, path: string, init?: RequestInit) {
  const res = await call(cfg, path, init);
  return { status: res.status, body: await res.json() };
}

describe("cockpit — company management (PATCH /api/companies/:slug)", () => {
  test("renames a company's display name without touching its slug", async () => {
    const ws = makeWorkspace("patch-rename", ["Acme ApS"]);
    try {
      const res = await json(config({ workspaceRoot: ws }), "/api/companies/acme-aps", {
        method: "PATCH",
        body: JSON.stringify({ name: "Acme Holding ApS" }),
      });
      expect(res.status).toBe(200);
      expect(res.body.company).toMatchObject({
        slug: "acme-aps",
        name: "Acme Holding ApS",
      });
      const entry = listWorkspaceCompanies(ws).find((c) => c.slug === "acme-aps");
      expect(entry?.name).toBe("Acme Holding ApS");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("archives a company non-destructively (ledger stays on disk)", async () => {
    const ws = makeWorkspace("patch-archive", ["Acme ApS"]);
    try {
      const res = await json(config({ workspaceRoot: ws }), "/api/companies/acme-aps", {
        method: "PATCH",
        body: JSON.stringify({ archived: true }),
      });
      expect(res.status).toBe(200);
      expect(res.body.company.archived).toBe(true);
      // The dashboard still resolves — the ledger was not deleted.
      const dash = await json(config({ workspaceRoot: ws }), "/api/companies/acme-aps/dashboard");
      expect(dash.status).toBe(200);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("an unknown slug is a safe 404", async () => {
    const ws = makeWorkspace("patch-404", ["Acme ApS"]);
    try {
      const res = await json(config({ workspaceRoot: ws }), "/api/companies/ghost", {
        method: "PATCH",
        body: JSON.stringify({ name: "X" }),
      });
      expect(res.status).toBe(404);
      expect(res.body.code).toBe("not_found");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("an empty PATCH body is a 400", async () => {
    const ws = makeWorkspace("patch-empty", ["Acme ApS"]);
    try {
      const res = await json(config({ workspaceRoot: ws }), "/api/companies/acme-aps", {
        method: "PATCH",
        body: JSON.stringify({}),
      });
      expect(res.status).toBe(400);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a non-PATCH method on the company route is 405", async () => {
    const ws = makeWorkspace("patch-405", ["Acme ApS"]);
    try {
      const res = await json(config({ workspaceRoot: ws }), "/api/companies/acme-aps", {
        method: "DELETE",
      });
      expect(res.status).toBe(405);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});

describe("cockpit — static SPA serving", () => {
  function makeStaticRoot(label: string) {
    const root = tmpRoot(label);
    const dist = join(root, "dist");
    mkdirSync(join(dist, "assets"), { recursive: true });
    writeFileSync(join(dist, "index.html"), "<!doctype html><div id=root></div>");
    writeFileSync(join(dist, "assets", "app.js"), "console.log('cockpit')");
    return dist;
  }

  test("serves index.html for the app root", async () => {
    const ws = makeWorkspace("spa-root");
    const dist = makeStaticRoot("spa-root-dist");
    try {
      const res = await call(config({ workspaceRoot: ws, staticRoot: dist }), "/");
      expect(res.status).toBe(200);
      expect(res.headers.get("content-type")).toContain("text/html");
      expect(await res.text()).toContain("id=root");
    } finally {
      rmSync(ws, { recursive: true, force: true });
      rmSync(dist, { recursive: true, force: true });
    }
  });

  test("serves a real asset with its content type", async () => {
    const ws = makeWorkspace("spa-asset");
    const dist = makeStaticRoot("spa-asset-dist");
    try {
      const res = await call(
        config({ workspaceRoot: ws, staticRoot: dist }),
        "/assets/app.js",
      );
      expect(res.status).toBe(200);
      expect(res.headers.get("content-type")).toContain("javascript");
    } finally {
      rmSync(ws, { recursive: true, force: true });
      rmSync(dist, { recursive: true, force: true });
    }
  });

  test("falls back to index.html for a client-side route (deep link)", async () => {
    const ws = makeWorkspace("spa-fallback");
    const dist = makeStaticRoot("spa-fallback-dist");
    try {
      const res = await call(
        config({ workspaceRoot: ws, staticRoot: dist }),
        "/companies/acme-aps/manage",
      );
      expect(res.status).toBe(200);
      expect(await res.text()).toContain("id=root");
    } finally {
      rmSync(ws, { recursive: true, force: true });
      rmSync(dist, { recursive: true, force: true });
    }
  });

  test("a path-traversal attempt is contained to index.html, never escapes the root", async () => {
    const ws = makeWorkspace("spa-traversal");
    const dist = makeStaticRoot("spa-traversal-dist");
    try {
      const res = await call(
        config({ workspaceRoot: ws, staticRoot: dist }),
        "/../../../../etc/passwd",
      );
      // Either a contained fallback or a 404 — never a leak of an outside file.
      const text = await res.text();
      expect(text).not.toContain("root:");
    } finally {
      rmSync(ws, { recursive: true, force: true });
      rmSync(dist, { recursive: true, force: true });
    }
  });

  test("the JSON API still works alongside static serving", async () => {
    const ws = makeWorkspace("spa-api", ["Acme ApS"]);
    const dist = makeStaticRoot("spa-api-dist");
    try {
      const res = await json(
        config({ workspaceRoot: ws, staticRoot: dist }),
        "/api/companies",
      );
      expect(res.status).toBe(200);
      expect(res.body.count).toBe(1);
    } finally {
      rmSync(ws, { recursive: true, force: true });
      rmSync(dist, { recursive: true, force: true });
    }
  });

  test("an unknown /api route is still a JSON 404 even with a SPA configured", async () => {
    const ws = makeWorkspace("spa-api404");
    const dist = makeStaticRoot("spa-api404-dist");
    try {
      const res = await json(
        config({ workspaceRoot: ws, staticRoot: dist }),
        "/api/nope",
      );
      expect(res.status).toBe(404);
      expect(res.body.code).toBe("not_found");
    } finally {
      rmSync(ws, { recursive: true, force: true });
      rmSync(dist, { recursive: true, force: true });
    }
  });
});
