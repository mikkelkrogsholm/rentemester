import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, symlinkSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { createCompany } from "../../src/core/company";
import { isCompanyInsideWorkspace, initWorkspace, loadWorkspaceManifest, resolveWorkspaceSlug, saveWorkspaceManifest } from "../../src/core/workspace";
import { resolveWorkspaceCompany } from "../../src/core/workspace-company-resolver";

function workspace(): string { return mkdtempSync(join(tmpdir(), "rentemester-company-resolver-")); }
const registered = { selection: "registered", archived: "allow", ledger: "optional" } as const;

describe("workspace company resolver", () => {
  test("rejects invalid and unregistered slugs without deriving a filesystem target", () => {
    const root = workspace();
    try {
      initWorkspace(root);
      expect(resolveWorkspaceCompany(root, "../escape", registered)).toEqual({ ok: false, reason: "INVALID_SLUG" });
      expect(resolveWorkspaceCompany(root, "missing", registered)).toEqual({ ok: false, reason: "NOT_REGISTERED" });
    } finally { rmSync(root, { recursive: true, force: true }); }
  });

  test("keeps registered, routable-live and archived selection semantics explicit", () => {
    const root = workspace();
    try {
      initWorkspace(root);
      createCompany(root, { name: "Live ApS" });
      createCompany(root, { name: "Dry Run ApS" });
      createCompany(root, { name: "Archived ApS" });
      const manifest = loadWorkspaceManifest(root);
      saveWorkspaceManifest(root, { ...manifest, companies: manifest.companies.map((entry) =>
        entry.slug === "dry-run-aps" ? { ...entry, purpose: "dry-run" } :
        entry.slug === "archived-aps" ? { ...entry, archived: true } : entry,
      ) });

      expect(resolveWorkspaceCompany(root, "dry-run-aps", registered).ok).toBe(true);
      expect(resolveWorkspaceCompany(root, "dry-run-aps", { selection: "routable-live", archived: "allow", ledger: "optional" })).toEqual({ ok: false, reason: "NOT_ROUTABLE_LIVE" });
      expect(resolveWorkspaceCompany(root, "archived-aps", { selection: "registered", archived: "deny", ledger: "optional" })).toEqual({ ok: false, reason: "ARCHIVED" });
    } finally { rmSync(root, { recursive: true, force: true }); }
  });

  test("returns immutable, direct-child paths and enforces an optional ledger requirement", () => {
    const root = workspace();
    try {
      initWorkspace(root);
      const created = createCompany(root, { name: "Acme ApS" });
      const resolved = resolveWorkspaceCompany(root, created.slug, { selection: "registered", archived: "allow", ledger: "required" });
      expect(resolved.ok).toBe(true);
      if (!resolved.ok) return;
      expect(resolved.company.companyRoot).toBe(created.companyRoot);
      expect(resolved.company.ledgerDbPath).toBe(join(created.companyRoot, "data", "ledger.sqlite"));
      expect(isCompanyInsideWorkspace(root, resolved.company.companyRoot)).toBe(true);
      expect(Object.isFrozen(resolved.company)).toBe(true);
      expect(Object.isFrozen(resolved.company.entry)).toBe(true);

      rmSync(resolved.company.ledgerDbPath);
      expect(resolveWorkspaceCompany(root, created.slug, { selection: "registered", archived: "allow", ledger: "required" })).toEqual({ ok: false, reason: "LEDGER_MISSING" });
      expect(resolveWorkspaceCompany(root, created.slug, registered).ok).toBe(true);
    } finally { rmSync(root, { recursive: true, force: true }); }
  });

  test("rejects a registered direct-child symlink that escapes the workspace", () => {
    const root = workspace();
    const outside = workspace();
    try {
      initWorkspace(root);
      saveWorkspaceManifest(root, {
        version: 2,
        companies: [{ slug: "escape", name: "Escape ApS", createdAt: "2026-01-01T00:00:00.000Z", archived: false, purpose: "live" }],
      });
      symlinkSync(outside, join(root, "escape"));

      expect(resolveWorkspaceSlug(root, "escape")).toBeNull();
      expect(isCompanyInsideWorkspace(root, join(root, "escape"))).toBe(false);
      expect(resolveWorkspaceCompany(root, "escape", registered)).toEqual({
        ok: false,
        reason: "PATH_OUTSIDE_WORKSPACE",
      });
    } finally {
      rmSync(root, { recursive: true, force: true });
      rmSync(outside, { recursive: true, force: true });
    }
  });

  test("rejects another company's symlink alias and a dangling outside target", () => {
    const root = workspace();
    try {
      initWorkspace(root);
      const live = createCompany(root, { name: "Live ApS" });
      const manifest = loadWorkspaceManifest(root);
      saveWorkspaceManifest(root, { ...manifest, companies: [...manifest.companies,
        { slug: "alias", name: "Alias ApS", createdAt: "2026-01-01T00:00:00.000Z", archived: false },
        { slug: "dangling", name: "Dangling ApS", createdAt: "2026-01-01T00:00:00.000Z", archived: false },
      ] });
      symlinkSync(live.companyRoot, join(root, "alias"));
      symlinkSync(join(root, "missing", "outside"), join(root, "dangling"));
      for (const slug of ["alias", "dangling"]) {
        expect(resolveWorkspaceSlug(root, slug)).toBeNull();
        expect(resolveWorkspaceCompany(root, slug, registered)).toEqual({ ok: false, reason: "PATH_OUTSIDE_WORKSPACE" });
      }
      expect(resolveWorkspaceSlug(root, live.slug)).toBe(live.companyRoot);
    } finally { rmSync(root, { recursive: true, force: true }); }
  });
});
