// Tests: src/core/workspace.ts, src/core/company.ts (workspace model + createCompany)
import { describe, expect, test } from "bun:test";
import { existsSync, mkdtempSync, mkdirSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import {
  WORKSPACE_MANIFEST_FILE,
  adoptCompanyDir,
  initWorkspace,
  isCompanyInsideWorkspace,
  listWorkspaceCompanies,
  loadWorkspaceManifest,
  registerCompanyDirIntoWorkspace,
  resolveWorkspaceSlug,
  saveWorkspaceManifest,
  slugifyCompanyName,
  workspaceExists,
} from "../../src/core/workspace";
import { createCompany, initialiseCompanyVolume } from "../../src/core/company";
import { companyPaths } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";

import { cleanupDir } from "../helpers/cleanup";
function tmpRoot(label: string) {
  return mkdtempSync(join(tmpdir(), `rentemester-${label}-`));
}

describe("workspace model", () => {
  test("detects when no workspace exists yet", () => {
    const root = tmpRoot("ws-detect");
    try {
      expect(workspaceExists(root)).toBe(false);
      initWorkspace(root);
      expect(workspaceExists(root)).toBe(true);
    } finally {
      cleanupDir(root);
    }
  });

  test("initWorkspace creates an empty manifest at the root", () => {
    const root = tmpRoot("ws-init");
    try {
      initWorkspace(root);
      expect(existsSync(join(root, WORKSPACE_MANIFEST_FILE))).toBe(true);
      const manifest = loadWorkspaceManifest(root);
      expect(manifest.companies).toEqual([]);
    } finally {
      cleanupDir(root);
    }
  });

  test("manifest round-trips through save/load deterministically", () => {
    const root = tmpRoot("ws-roundtrip");
    try {
      initWorkspace(root);
      const manifest = {
        version: 1 as const,
        companies: [
          { slug: "acme", name: "Acme ApS", createdAt: "2026-05-20T00:00:00.000Z", archived: false },
          { slug: "beta", name: "Beta IVS", createdAt: "2026-05-20T01:00:00.000Z", archived: true },
        ],
      };
      saveWorkspaceManifest(root, manifest);
      expect(loadWorkspaceManifest(root)).toEqual(manifest);
    } finally {
      cleanupDir(root);
    }
  });

  test("slugifyCompanyName produces filesystem-safe deterministic slugs", () => {
    expect(slugifyCompanyName("Acme ApS")).toBe("acme-aps");
    expect(slugifyCompanyName("  Bla  Bla  ")).toBe("bla-bla");
    expect(slugifyCompanyName("Æbleskive Smør Ø")).toBe("aebleskive-smoer-oe");
    expect(slugifyCompanyName("Acme ApS")).toBe(slugifyCompanyName("Acme ApS"));
  });
});

describe("createCompany", () => {
  test("creates the full directory structure and an initialised ledger DB", () => {
    const root = tmpRoot("create-company");
    try {
      initWorkspace(root);
      const result = createCompany(root, { name: "Acme ApS", cvr: "DK12345678" });
      expect(result.slug).toBe("acme-aps");

      const p = companyPaths(result.companyRoot);
      expect(existsSync(p.db)).toBe(true);
      expect(existsSync(p.documentsInbox)).toBe(true);
      expect(existsSync(p.config)).toBe(true);
      expect(existsSync(join(p.config, "policy.yaml"))).toBe(true);

      const db = openDb(p.db);
      migrate(db);
      const accounts = db.query("SELECT COUNT(*) AS n FROM accounts").get() as { n: number };
      expect(accounts.n).toBeGreaterThan(0);
      const company = db.query("SELECT cvr FROM companies WHERE id = 1").get() as { cvr: string };
      expect(company.cvr).toBe("DK12345678");
      db.close();
    } finally {
      cleanupDir(root);
    }
  });

  test("registers the new company in the workspace manifest", () => {
    const root = tmpRoot("create-company-manifest");
    try {
      initWorkspace(root);
      createCompany(root, { name: "Acme ApS" });
      const companies = listWorkspaceCompanies(root);
      expect(companies.map((c) => c.slug)).toEqual(["acme-aps"]);
      expect(companies[0]!.name).toBe("Acme ApS");
      expect(companies[0]!.archived).toBe(false);
    } finally {
      cleanupDir(root);
    }
  });

  test("rejects a duplicate slug", () => {
    const root = tmpRoot("create-company-dup");
    try {
      initWorkspace(root);
      createCompany(root, { name: "Acme ApS" });
      expect(() => createCompany(root, { slug: "acme-aps", name: "Other" })).toThrow();
    } finally {
      cleanupDir(root);
    }
  });

  test("honours an explicit slug and fiscal-year settings", () => {
    const root = tmpRoot("create-company-explicit");
    try {
      initWorkspace(root);
      const result = createCompany(root, {
        slug: "my-co",
        name: "My Co",
        fiscalYearStartMonth: 7,
        fiscalYearLabelStrategy: "span",
      });
      expect(result.slug).toBe("my-co");
      const db = openDb(companyPaths(result.companyRoot).db);
      migrate(db);
      const row = db.query(
        "SELECT fiscal_year_start_month AS m, fiscal_year_label_strategy AS s FROM companies WHERE id = 1",
      ).get() as { m: number; s: string };
      expect(row).toEqual({ m: 7, s: "span" });
      db.close();
    } finally {
      cleanupDir(root);
    }
  });
});

describe("slug resolution", () => {
  test("resolves a known slug to its company directory", () => {
    const root = tmpRoot("ws-resolve");
    try {
      initWorkspace(root);
      const created = createCompany(root, { name: "Acme ApS" });
      const resolved = resolveWorkspaceSlug(root, "acme-aps");
      expect(resolved).toBe(created.companyRoot);
      expect(resolved).toBe(join(root, "acme-aps"));
    } finally {
      cleanupDir(root);
    }
  });

  test("returns null for an unknown slug", () => {
    const root = tmpRoot("ws-resolve-miss");
    try {
      initWorkspace(root);
      expect(resolveWorkspaceSlug(root, "ghost")).toBeNull();
    } finally {
      cleanupDir(root);
    }
  });
});

describe("adoption of an unlisted company directory", () => {
  test("adopts a present-but-unlisted company directory into the manifest", () => {
    const root = tmpRoot("ws-adopt");
    try {
      initWorkspace(root);
      // A company directory that exists on disk but is not in the manifest.
      const orphanRoot = join(root, "orphan-co");
      const p = companyPaths(orphanRoot);
      mkdirSync(p.data, { recursive: true });
      const db = openDb(p.db);
      migrate(db);
      db.run("INSERT INTO companies (id, name) VALUES (1, 'Orphan Co') ON CONFLICT(id) DO NOTHING");
      db.close();

      expect(listWorkspaceCompanies(root).map((c) => c.slug)).toEqual([]);
      const adopted = adoptCompanyDir(root, "orphan-co");
      expect(adopted.slug).toBe("orphan-co");
      expect(listWorkspaceCompanies(root).map((c) => c.slug)).toEqual(["orphan-co"]);
    } finally {
      cleanupDir(root);
    }
  });

  test("refuses to adopt a directory without a ledger", () => {
    const root = tmpRoot("ws-adopt-empty");
    try {
      initWorkspace(root);
      mkdirSync(join(root, "not-a-company"), { recursive: true });
      expect(() => adoptCompanyDir(root, "not-a-company")).toThrow();
    } finally {
      cleanupDir(root);
    }
  });
});

describe("registerCompanyDirIntoWorkspace (#216 init/cockpit unification)", () => {
  test("detects whether a company directory sits inside a workspace", () => {
    const ws = tmpRoot("ws-inside");
    try {
      expect(isCompanyInsideWorkspace(ws, join(ws, "acme"))).toBe(true);
      expect(isCompanyInsideWorkspace(ws, join(ws, "acme", "nested"))).toBe(false);
      expect(isCompanyInsideWorkspace(ws, ws)).toBe(false);
      expect(isCompanyInsideWorkspace(ws, "/tmp/some-other-place")).toBe(false);
    } finally {
      cleanupDir(ws);
    }
  });

  test("registers an init-created company so the workspace can see it", () => {
    const ws = tmpRoot("ws-register-init");
    try {
      const companyRoot = join(ws, "acme-aps");
      initialiseCompanyVolume(companyRoot, { name: "Acme ApS" });

      // Before registration the workspace manifest does not know the company.
      expect(listWorkspaceCompanies(ws).map((c) => c.slug)).toEqual([]);

      const result = registerCompanyDirIntoWorkspace(ws, companyRoot);
      expect(result).toEqual({ status: "registered", slug: "acme-aps" });

      const listed = listWorkspaceCompanies(ws);
      expect(listed.map((c) => c.slug)).toEqual(["acme-aps"]);
      expect(listed[0]?.name).toBe("Acme ApS");
    } finally {
      cleanupDir(ws);
    }
  });

  test("is idempotent — re-registering the same company is a no-op", () => {
    const ws = tmpRoot("ws-register-idem");
    try {
      const companyRoot = join(ws, "acme-aps");
      initialiseCompanyVolume(companyRoot, { name: "Acme ApS" });
      expect(registerCompanyDirIntoWorkspace(ws, companyRoot).status).toBe("registered");
      expect(registerCompanyDirIntoWorkspace(ws, companyRoot)).toEqual({
        status: "already-registered",
        slug: "acme-aps",
      });
      expect(listWorkspaceCompanies(ws)).toHaveLength(1);
    } finally {
      cleanupDir(ws);
    }
  });

  test("a company directory outside the workspace is left untouched", () => {
    const ws = tmpRoot("ws-register-outside");
    const elsewhere = tmpRoot("ws-register-elsewhere");
    try {
      const companyRoot = join(elsewhere, "company");
      initialiseCompanyVolume(companyRoot, { name: "Outside Co" });
      initWorkspace(ws);
      expect(registerCompanyDirIntoWorkspace(ws, companyRoot)).toEqual({
        status: "outside-workspace",
      });
      expect(listWorkspaceCompanies(ws)).toEqual([]);
    } finally {
      cleanupDir(ws);
      cleanupDir(elsewhere);
    }
  });
});
