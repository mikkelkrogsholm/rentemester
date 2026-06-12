// Tests: GET /api/companies/:slug/documents/:id/file — the cockpit's
// read route that serves the stored bilag file so a human can open it.
import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { handleRequest } from "../../src/server/router";
import type { ServerConfig } from "../../src/server/config";
import { createCompany } from "../../src/core/company";
import { initWorkspace, companyRootForSlug } from "../../src/core/workspace";
import { companyPaths } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { ingestDocument } from "../../src/core/documents";

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

/** Ingests examples/vendor-invoice.txt into the company; returns its id. */
function ingestSample(ws: string, slug: string): number {
  const companyRoot = companyRootForSlug(ws, slug);
  const db = openDb(companyPaths(companyRoot).db);
  try {
    migrate(db);
    const metadata = JSON.parse(
      readFileSync("examples/vendor-invoice.metadata.json", "utf8"),
    );
    const res = ingestDocument(
      db,
      companyRoot,
      "examples/vendor-invoice.txt",
      metadata,
    );
    if (!res.ok) {
      throw new Error(`ingest failed: ${(res.errors ?? []).join("; ")}`);
    }
    return Number(res.documentId);
  } finally {
    db.close();
  }
}

async function getRaw(cfg: ServerConfig, path: string): Promise<Response> {
  return handleRequest(
    new Request(`http://localhost${path}`, { headers: { host: "127.0.0.1" } }),
    cfg,
  );
}

describe("cockpit API — document file (GET .../documents/:id/file)", () => {
  test("serves the stored bilag file with its content type", async () => {
    const { root: ws, slug } = makeWorkspace("docfile-ok");
    try {
      const id = ingestSample(ws, slug);
      const res = await getRaw(
        config(ws),
        `/api/companies/${slug}/documents/${id}/file`,
      );
      expect(res.status).toBe(200);
      expect(res.headers.get("content-type")).toContain("text/plain");
      const body = await res.text();
      expect(body).toBe(readFileSync("examples/vendor-invoice.txt", "utf8"));
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("an unknown document id is a safe 404", async () => {
    const { root: ws, slug } = makeWorkspace("docfile-404");
    try {
      const res = await getRaw(
        config(ws),
        `/api/companies/${slug}/documents/9999/file`,
      );
      expect(res.status).toBe(404);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("an unknown company is a safe 404", async () => {
    const { root: ws } = makeWorkspace("docfile-co404");
    try {
      const res = await getRaw(
        config(ws),
        "/api/companies/ghost/documents/1/file",
      );
      expect(res.status).toBe(404);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a non-GET method is rejected", async () => {
    const { root: ws, slug } = makeWorkspace("docfile-method");
    try {
      const res = await handleRequest(
        new Request(
          `http://localhost/api/companies/${slug}/documents/1/file`,
          { method: "POST", headers: { host: "127.0.0.1" } },
        ),
        config(ws),
      );
      expect(res.status).toBe(405);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a text bilag is served as a download, not rendered inline", async () => {
    const { root: ws, slug } = makeWorkspace("docfile-disp");
    try {
      const id = ingestSample(ws, slug);
      const res = await getRaw(
        config(ws),
        `/api/companies/${slug}/documents/${id}/file`,
      );
      expect(res.status).toBe(200);
      expect(res.headers.get("content-disposition")).toContain("attachment");
      expect(res.headers.get("x-content-type-options")).toBe("nosniff");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});
