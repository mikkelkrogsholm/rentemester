// Tests: handleAccountantExport + the POST /accountant-export route — the
// cockpit's "share with revisor" action. Generates the same accountant-handoff
// package the CLI's `system export-accountant` produces, packed into one
// downloadable .tar so a human can hand it off in one click.
import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { handleRequest } from "../../src/server/router";
import type { ServerConfig } from "../../src/server/config";
import { createCompany } from "../../src/core/company";
import { initWorkspace } from "../../src/core/workspace";
import { readTar } from "../../src/core/tar";

function makeWorkspace(label: string) {
  const root = mkdtempSync(join(tmpdir(), `rentemester-${label}-`));
  initWorkspace(root);
  const created = createCompany(root, { name: "Acme ApS" });
  return { root, slug: created.slug };
}

function config(workspaceRoot: string): ServerConfig {
  return {
    host: "127.0.0.1",
    port: 0,
    authRequired: false,
    authToken: null,
    workspaceRoot,
  };
}

async function post(cfg: ServerConfig, path: string, body?: unknown): Promise<Response> {
  const init: RequestInit = {
    method: "POST",
    headers: { host: "127.0.0.1" },
  };
  if (body !== undefined) init.body = JSON.stringify(body);
  return handleRequest(new Request(`http://localhost${path}`, init), cfg);
}

describe("Cockpit write — accountant export (POST .../accountant-export)", () => {
  test("returns a downloadable tar with the accountant-handoff package", async () => {
    const { root: ws, slug } = makeWorkspace("acct-ok");
    try {
      const res = await post(config(ws), `/api/companies/${slug}/accountant-export`, {
        periodStart: "2026-01-01",
        periodEnd: "2026-12-31",
        confirm: true,
      });
      expect(res.status).toBe(200);
      expect(res.headers.get("content-type")).toBe("application/x-tar");
      expect(res.headers.get("content-disposition")).toContain("attachment");
      expect(res.headers.get("content-disposition")).toContain("revisor-eksport");
      const buf = Buffer.from(await res.arrayBuffer());
      expect(buf.length).toBeGreaterThan(0);
      // The tar contains a manifest — verifies the package was actually built.
      const entries = readTar(buf);
      const names = entries.map((e) => e.path);
      expect(names.some((n) => n.endsWith("manifest.json"))).toBe(true);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("is refused without confirm", async () => {
    const { root: ws, slug } = makeWorkspace("acct-noconfirm");
    try {
      const res = await post(config(ws), `/api/companies/${slug}/accountant-export`, {
        periodStart: "2026-01-01",
        periodEnd: "2026-12-31",
      });
      expect(res.status).toBe(400);
      const body = await res.json();
      expect(body.errors[0]).toContain("confirm");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("an invalid period is a 400", async () => {
    const { root: ws, slug } = makeWorkspace("acct-badperiod");
    try {
      const res = await post(config(ws), `/api/companies/${slug}/accountant-export`, {
        periodStart: "2026-12-31",
        periodEnd: "2026-01-01",
        confirm: true,
      });
      expect(res.status).toBe(400);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("an unknown company is a safe 404", async () => {
    const { root: ws } = makeWorkspace("acct-404");
    try {
      const res = await post(config(ws), "/api/companies/ghost/accountant-export", {
        periodStart: "2026-01-01",
        periodEnd: "2026-12-31",
        confirm: true,
      });
      expect(res.status).toBe(404);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});
