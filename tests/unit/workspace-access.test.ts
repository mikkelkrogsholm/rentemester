import { Database } from "bun:sqlite";
import { describe, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { mkdirSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  activateWorkspaceUser,
  ALL_ROUTE_PERMISSIONS,
  authorizeWorkspaceRoute,
  disableWorkspaceUser,
  getCompanyMembership,
  listActiveCompanyMembershipSlugs,
  getSessionInvalidation,
  grantCompanyMembership,
  invalidateUserSessions,
  invalidateWorkspaceSessions,
  revokeCompanyMembership,
  ROUTE_PERMISSION_POLICY,
} from "../../src/core/workspace-access";
import { openWorkspaceControlDb } from "../../src/core/workspace-control";
import {
  initWorkspace,
  registerWorkspaceCompany,
  setWorkspaceCompanyArchived,
} from "../../src/core/workspace";

function tempWorkspace() {
  return mkdtempSync(join(tmpdir(), "rentemester-workspace-access-"));
}

function actor() {
  return { createdBy: "agent:test", createdByProgram: "unit-test" };
}

function addUser(db: Database, id: string, options?: { verified?: boolean; twoFactor?: boolean }) {
  db.query(
    `INSERT INTO "user" (id, name, email, emailVerified, createdAt, updatedAt, twoFactorEnabled)
     VALUES (?, ?, ?, ?, ?, ?, ?)`,
  ).run(
    id,
    `Synthetic ${id}`,
    `${id}@example.test`,
    options?.verified === false ? 0 : 1,
    "2026-01-01T00:00:00.000Z",
    "2026-01-01T00:00:00.000Z",
    options?.twoFactor === false ? 0 : 1,
  );
}

function registerCompanies(workspace: string) {
  initWorkspace(workspace);
  registerWorkspaceCompany(workspace, {
    slug: "company-a",
    name: "Synthetic Company A",
    createdAt: "2026-01-01T00:00:00.000Z",
    archived: false,
  });
  registerWorkspaceCompany(workspace, {
    slug: "company-b",
    name: "Synthetic Company B",
    createdAt: "2026-01-01T00:00:00.000Z",
    archived: false,
  });
}

describe("workspace access event core", () => {
  test("allows a prepared member only in their granted company and never opens its ledger", () => {
    const workspace = tempWorkspace();
    const ledgerPath = join(workspace, "company-a", "data", "ledger.sqlite");
    try {
      registerCompanies(workspace);
      mkdirSync(join(workspace, "company-a", "data"), { recursive: true });
      const ledger = new Database(ledgerPath, { create: true });
      ledger.exec("CREATE TABLE sentinel (value TEXT NOT NULL); INSERT INTO sentinel VALUES ('unchanged');");
      ledger.close();
      const before = createHash("sha256").update(readFileSync(ledgerPath)).digest("hex");

      const db = openWorkspaceControlDb(workspace);
      addUser(db, "user-a");
      activateWorkspaceUser(db, { userId: "user-a", workspaceRole: "member", ...actor() });
      grantCompanyMembership(db, workspace, {
        userId: "user-a", companySlug: "company-a", role: "bookkeeper", ...actor(),
      });
      expect(listActiveCompanyMembershipSlugs(db, workspace, "user-a")).toEqual(["company-a"]);

      expect(authorizeWorkspaceRoute(db, workspace, {
        userId: "user-a", companySlug: "company-a", permission: "company.ledger.post",
      })).toEqual({ allowed: true });
      expect(authorizeWorkspaceRoute(db, workspace, {
        userId: "user-a", companySlug: "company-a", permission: "company.review",
      })).toEqual({ allowed: false });
      // A changed URL/request slug has no access and gets no disclosure.
      expect(authorizeWorkspaceRoute(db, workspace, {
        userId: "user-a", companySlug: "company-b", permission: "company.read",
      })).toEqual({ allowed: false });
      expect(authorizeWorkspaceRoute(db, workspace, {
        userId: "user-a", companySlug: "company-a", permission: "workspace.manage",
      })).toEqual({ allowed: false });
      db.close();

      expect(createHash("sha256").update(readFileSync(ledgerPath)).digest("hex")).toBe(before);
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });

  test("fails closed until verified email and TOTP are both enabled, and after disable", () => {
    const workspace = tempWorkspace();
    try {
      registerCompanies(workspace);
      const db = openWorkspaceControlDb(workspace);
      addUser(db, "not-ready", { verified: false, twoFactor: false });
      activateWorkspaceUser(db, { userId: "not-ready", workspaceRole: "member", ...actor() });
      grantCompanyMembership(db, workspace, {
        userId: "not-ready", companySlug: "company-a", role: "reader", ...actor(),
      });
      expect(authorizeWorkspaceRoute(db, workspace, {
        userId: "not-ready", companySlug: "company-a", permission: "company.read",
      })).toEqual({ allowed: false });

      db.run('UPDATE "user" SET emailVerified = 1, twoFactorEnabled = 1 WHERE id = ?', ["not-ready"]);
      expect(authorizeWorkspaceRoute(db, workspace, {
        userId: "not-ready", companySlug: "company-a", permission: "company.read",
      })).toEqual({ allowed: true });
      disableWorkspaceUser(db, { userId: "not-ready", ...actor() });
      expect(authorizeWorkspaceRoute(db, workspace, {
        userId: "not-ready", companySlug: "company-a", permission: "company.read",
      })).toEqual({ allowed: false });
      db.close();
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });

  test("reduces grant, revoke and regrant history without updates and validates grant preconditions", () => {
    const workspace = tempWorkspace();
    try {
      registerCompanies(workspace);
      const db = openWorkspaceControlDb(workspace);
      addUser(db, "user-a");
      // Foreign keys protect append-only evidence from orphan injection. The
      // referenced Better Auth user must exist even for direct SQL callers.
      expect(() => db.query(
        "INSERT INTO rm_workspace_user_access_events (user_id, event_type, workspace_role, actor) VALUES (?, 'activate', 'member', 'test')",
      ).run("orphan-user")).toThrow(/FOREIGN KEY/i);
      expect(() => db.query(
        "INSERT INTO rm_company_membership_events (user_id, company_slug, event_type, company_role, actor) VALUES (?, 'company-a', 'grant', 'reader', 'test')",
      ).run("orphan-user")).toThrow(/FOREIGN KEY/i);
      expect(() => db.query(
        "INSERT INTO rm_workspace_security_events (scope, user_id, event_type, actor) VALUES ('user', ?, 'session_invalidate', 'test')",
      ).run("orphan-user")).toThrow(/FOREIGN KEY/i);
      expect(() => grantCompanyMembership(db, workspace, {
        userId: "missing", companySlug: "company-a", role: "reader", ...actor(),
      })).toThrow("Better Auth user does not exist");
      expect(() => grantCompanyMembership(db, workspace, {
        userId: "user-a", companySlug: "missing-company", role: "reader", ...actor(),
      })).toThrow("company is not registered");
      setWorkspaceCompanyArchived(workspace, "company-b", true);
      expect(() => grantCompanyMembership(db, workspace, {
        userId: "user-a", companySlug: "company-b", role: "reader", ...actor(),
      })).toThrow("company is archived");

      grantCompanyMembership(db, workspace, {
        userId: "user-a", companySlug: "company-a", role: "reader", ...actor(),
      });
      expect(revokeCompanyMembership(db, workspace, {
        userId: "user-a", companySlug: "company-a", ...actor(),
      }).status).toBe("revoked");
      expect(revokeCompanyMembership(db, workspace, {
        userId: "user-a", companySlug: "company-a", ...actor(),
      }).status).toBe("already-revoked");
      expect(grantCompanyMembership(db, workspace, {
        userId: "user-a", companySlug: "company-a", role: "reviewer", ...actor(),
      }).status).toBe("granted");
      expect(getCompanyMembership(db, "user-a", "company-a")).toMatchObject({
        active: true,
        role: "reviewer",
      });
      expect(db.query("SELECT event_type FROM rm_company_membership_events ORDER BY id").all()).toEqual([
        { event_type: "grant" }, { event_type: "revoke" }, { event_type: "grant" },
      ]);
      expect(() => db.run("UPDATE rm_company_membership_events SET company_role = 'owner'")).toThrow("append-only");
      expect(() => db.run("DELETE FROM rm_company_membership_events")).toThrow("append-only");
      activateWorkspaceUser(db, { userId: "user-a", workspaceRole: "member", ...actor() });
      expect(() => db.run("UPDATE rm_workspace_user_access_events SET event_type = 'disable'")).toThrow("append-only");
      expect(() => db.run("DELETE FROM rm_workspace_user_access_events")).toThrow("append-only");
      expect(() => db.run('DELETE FROM "user" WHERE id = ?', ["user-a"])).toThrow(/FOREIGN KEY/i);
      db.close();
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });

  test("keeps at least one effective owner for the workspace and every active company", () => {
    const workspace = tempWorkspace();
    try {
      registerCompanies(workspace);
      const db = openWorkspaceControlDb(workspace);
      addUser(db, "owner-a");
      addUser(db, "owner-b");
      for (const userId of ["owner-a", "owner-b"]) {
        activateWorkspaceUser(db, { userId, workspaceRole: "workspace_owner", ...actor() });
        grantCompanyMembership(db, workspace, {
          userId, companySlug: "company-a", role: "owner", ...actor(),
        });
      }

      expect(revokeCompanyMembership(db, workspace, {
        userId: "owner-b", companySlug: "company-a", ...actor(),
      }).status).toBe("revoked");
      const membershipEvents = db.query(
        "SELECT COUNT(*) AS count FROM rm_company_membership_events",
      ).get() as { count: number };
      expect(() => revokeCompanyMembership(db, workspace, {
        userId: "owner-a", companySlug: "company-a", ...actor(),
      })).toThrow("final effective company owner");
      expect(db.query("SELECT COUNT(*) AS count FROM rm_company_membership_events").get())
        .toEqual(membershipEvents);

      expect(activateWorkspaceUser(db, {
        userId: "owner-b", workspaceRole: "member", ...actor(),
      }).status).toBe("role-updated");
      const accessEvents = db.query(
        "SELECT COUNT(*) AS count FROM rm_workspace_user_access_events",
      ).get() as { count: number };
      expect(() => disableWorkspaceUser(db, { userId: "owner-a", ...actor() }))
        .toThrow("final effective workspace owner");
      expect(() => activateWorkspaceUser(db, {
        userId: "owner-a", workspaceRole: "member", ...actor(),
      })).toThrow("final effective workspace owner");
      expect(db.query("SELECT COUNT(*) AS count FROM rm_workspace_user_access_events").get())
        .toEqual(accessEvents);
      db.close();
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });

  test("does not count an unverified or non-MFA identity as the replacement owner", () => {
    const workspace = tempWorkspace();
    try {
      registerCompanies(workspace);
      const db = openWorkspaceControlDb(workspace);
      addUser(db, "ready-owner");
      addUser(db, "pending-owner", { verified: false, twoFactor: false });
      for (const userId of ["ready-owner", "pending-owner"]) {
        activateWorkspaceUser(db, { userId, workspaceRole: "workspace_owner", ...actor() });
        grantCompanyMembership(db, workspace, {
          userId, companySlug: "company-a", role: "owner", ...actor(),
        });
      }
      expect(() => disableWorkspaceUser(db, { userId: "ready-owner", ...actor() }))
        .toThrow("final effective workspace owner");
      expect(() => revokeCompanyMembership(db, workspace, {
        userId: "ready-owner", companySlug: "company-a", ...actor(),
      })).toThrow("final effective company owner");

      db.run(
        'UPDATE "user" SET emailVerified = 1, twoFactorEnabled = 1 WHERE id = ?',
        ["pending-owner"],
      );
      expect(disableWorkspaceUser(db, { userId: "ready-owner", ...actor() }).status)
        .toBe("disabled");
      db.close();
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });

  test("has an exhaustive central permission policy with workspace management only for workspace owners", () => {
    expect(ALL_ROUTE_PERMISSIONS).toEqual([
      "public.read", "public.invitation.claim", "workspace.read", "workspace.tasks.read", "workspace.tasks.manage", "workspace.group.read", "workspace.manage",
      "workspace.members.read", "workspace.members.manage", "company.read", "company.tasks.read", "company.tasks.write", "company.tasks.manage", "company.documents.read",
      "company.documents.upload", "company.master-data", "company.draft.write", "company.ledger.post",
      "company.review", "company.period.force-close", "company.export", "company.external-lookup", "company.external-send", "company.admin",
      "company.knowledge.read", "company.knowledge.manage", "company.ownership.read", "company.ownership.manage",
    ]);
    expect(ROUTE_PERMISSION_POLICY.workspace_owner).toContain("workspace.manage");
    expect(ROUTE_PERMISSION_POLICY.workspace_owner).toContain("workspace.members.manage");
    expect(ROUTE_PERMISSION_POLICY.member).not.toContain("workspace.members.read");
    expect(ROUTE_PERMISSION_POLICY.owner).toContain("company.admin");
    expect(ROUTE_PERMISSION_POLICY.bookkeeper).not.toContain("company.review");
    expect(ROUTE_PERMISSION_POLICY.bookkeeper).not.toContain("company.external-send");
    expect(ROUTE_PERMISSION_POLICY.reviewer).toEqual([
      "company.read", "company.tasks.read", "company.tasks.write", "company.documents.read", "company.review", "company.export",
      "company.knowledge.read", "company.knowledge.manage", "company.ownership.read", "company.ownership.manage",
    ]);
    expect(ROUTE_PERMISSION_POLICY.reader).toEqual([
      "company.read", "company.tasks.read", "company.documents.read", "company.export", "company.knowledge.read", "company.ownership.read",
    ]);
    expect(new Set(Object.values(ROUTE_PERMISSION_POLICY).flat())).toEqual(new Set(ALL_ROUTE_PERMISSIONS));
  });

  test("records monotonic user and workspace security epochs for later session rejection", () => {
    const workspace = tempWorkspace();
    try {
      const db = openWorkspaceControlDb(workspace);
      addUser(db, "user-a");
      expect(getSessionInvalidation(db, "user-a")).toEqual({ epoch: 0, invalidatedAt: null });
      const userFirst = invalidateUserSessions(db, { userId: "user-a", ...actor() });
      const workspaceEpoch = invalidateWorkspaceSessions(db, actor());
      const userLatest = invalidateUserSessions(db, { userId: "user-a", ...actor() });
      expect(userFirst.epoch).toBeGreaterThan(0);
      expect(workspaceEpoch.epoch).toBeGreaterThan(userFirst.epoch);
      expect(userLatest.epoch).toBeGreaterThan(workspaceEpoch.epoch);
      expect(getSessionInvalidation(db, "user-a")).toEqual(userLatest);
      expect(() => db.run("UPDATE rm_workspace_security_events SET scope = 'user'")).toThrow("append-only");
      expect(() => db.run("DELETE FROM rm_workspace_security_events")).toThrow("append-only");
      db.close();
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });
});
