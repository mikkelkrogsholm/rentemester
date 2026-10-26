// Tests for the cockpit SPA additions to `rentemester serve` (#171):
//   - PATCH /api/companies/:slug — rename + archive (non-destructive)
//   - static serving of the built React app (with the index.html fallback)
import { describe, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createCompany } from "../../src/core/company";
import { initWorkspace, listWorkspaceCompanies } from "../../src/core/workspace";
import type { ServerConfig } from "../../src/server/config";
import { handleRequest } from "../../src/server/router";

function tmpRoot(label: string) {
  return mkdtempSync(join(tmpdir(), `rentemester-${label}-`));
}

function makeWorkspace(label: string, companyNames: string[] = []) {
  const root = tmpRoot(label);
  initWorkspace(root);
  for (const [index, name] of companyNames.entries()) {
    createCompany(root, { name, cvr: `DK${String(90000200 + index)}` });
  }
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
  // SEC-1-BYPASS: skriveruterne (PATCH/POST /api/companies) håndhæver nu
  // localhost-gaten — en rigtig klient sender altid en loopback-Host, så
  // helperen sætter en som standard (overskrives via init.headers).
  const headers = { host: "127.0.0.1", ...(init?.headers as Record<string, string> | undefined) };
  return handleRequest(new Request(`http://localhost${path}`, { ...init, headers }), cfg);
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
  test("builds root-relative assets that work on direct company deep links", async () => {
    const root = join(import.meta.dir, "..", "..");
    const build = Bun.spawnSync({
      cmd: ["bun", "run", "build"],
      cwd: join(root, "app"),
      stdout: "pipe",
      stderr: "pipe",
    });
    expect(build.exitCode).toBe(0);

    const ws = makeWorkspace("spa-real-build");
    const dist = join(root, "app", "dist");
    try {
      const index = await call(config({ workspaceRoot: ws, staticRoot: dist }), "/");
      expect(index.status).toBe(200);
      const html = await index.text();
      const assets = [...html.matchAll(/(?:src|href)="([^"?]+\.(?:js|css))"/g)].map(
        (match) => match[1],
      );

      expect(assets.some((asset) => asset.endsWith(".js"))).toBe(true);
      expect(assets.some((asset) => asset.endsWith(".css"))).toBe(true);
      expect(assets.every((asset) => asset.startsWith("/"))).toBe(true);

      for (const route of [
        "/",
        "/companies/synthetic-company",
        "/companies/synthetic-company/manage?source=deep-link",
      ]) {
        const response = await call(config({ workspaceRoot: ws, staticRoot: dist }), route);
        expect(response.status).toBe(200);
        expect(response.headers.get("content-type")).toContain("text/html");
        expect(await response.text()).toBe(html);
      }

      for (const asset of assets) {
        const response = await call(config({ workspaceRoot: ws, staticRoot: dist }), asset);
        expect(response.status).toBe(200);
        expect(response.headers.get("content-type")).toContain(
          asset.endsWith(".js") ? "javascript" : "text/css",
        );
        expect(await response.text()).not.toBe(html);
      }
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  }, 20_000);

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
