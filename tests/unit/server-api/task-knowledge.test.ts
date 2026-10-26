import { expect, test } from "bun:test";
import { initWorkspace, mkdtempSync, rmSync, join, tmpdir, handleRequest, type ServerConfig } from "./_shared";
import { registerWorkspaceCompany } from "../../../src/core/workspace";
import { openWorkspaceControlDb } from "../../../src/core/workspace-control";
import { activateWorkspaceUser, grantCompanyMembership, revokeCompanyMembership } from "../../../src/core/workspace-access";
import { createKnowledgePage } from "../../../src/core/knowledge-pages";

test("protected task playbooks honor exact scope, live access and valid short slugs", async () => {
  const root = mkdtempSync(join(tmpdir(), "rm-task-knowledge-"));
  const audit = { createdBy: "agent:test", createdByProgram: "task-knowledge-test" };
  initWorkspace(root);
  const db = openWorkspaceControlDb(root);
  try {
    for (const slug of ["company-a", "company-b"]) registerWorkspaceCompany(root, { slug, name: `Synthetic ${slug}`, createdAt: "2026-01-01T00:00:00Z", archived: false });
    const createdAt = new Date();
    for (const id of ["reader", "owner"]) {
      db.query('INSERT INTO "user" (id,name,email,emailVerified,createdAt,updatedAt,twoFactorEnabled) VALUES (?,?,?,?,?,?,?)').run(id, id, `${id}@example.test`, 1, createdAt.toISOString(), createdAt.toISOString(), 1);
      activateWorkspaceUser(db, { userId: id, workspaceRole: id === "owner" ? "workspace_owner" : "member", ...audit });
      db.query('INSERT INTO "session" (id,expiresAt,token,createdAt,updatedAt,userId) VALUES (?,?,?,?,?,?)').run(`session-${id}`, new Date(createdAt.getTime() + 3_600_000).toISOString(), `opaque-${id}`, createdAt.toISOString(), createdAt.toISOString(), id);
    }
    grantCompanyMembership(db, root, { userId: "reader", companySlug: "company-a", role: "reader", ...audit });
    for (const [pageId, slug, scope, body] of [
      ["page-a", "a-playbook", { kind: "company", companySlug: "company-a" }, "A-SYNTHETIC"],
      ["page-b", "b-playbook", { kind: "company", companySlug: "company-b" }, "B-SECRET-SYNTHETIC"],
      ["page-w", "workspace-playbook", { kind: "workspace" }, "WORKSPACE-SECRET-SYNTHETIC"],
      ["page-short", "ab", { kind: "company", companySlug: "company-a" }, "SHORT-SYNTHETIC"],
    ] as const) createKnowledgePage(db, { pageId, slug, scope, title: pageId, bodyMarkdown: body, provenance: { kind: "user", ref: "synthetic" }, effectiveFrom: "2026-01-01T00:00:00Z", actor: "agent:test", principal: "synthetic" });
    const cfg = (id: string): ServerConfig => ({ host: "127.0.0.1", port: 0, authRequired: false, authToken: null, workspaceRoot: root, betterAuthProvider: { getSession: async () => ({ user: { id }, session: { id: `session-${id}`, createdAt } }), handle: async () => new Response("not supported", { status: 404 }) } });
    const cases = [
      ["reader", "/api/companies/company-a/knowledge-pages/page-a", "GET", 200],
      ["reader", "/api/companies/company-a/knowledge-pages/a-playbook", "GET", 200],
      ["reader", "/api/companies/company-a/knowledge-pages/page-b", "GET", 404],
      ["reader", "/api/companies/company-b/knowledge-pages/page-b", "GET", 401],
      ["reader", "/api/knowledge-pages/page-w", "GET", 401],
      ["owner", "/api/knowledge-pages/page-w", "GET", 200],
      ["owner", "/api/companies/company-b/knowledge-pages/page-b", "GET", 401],
      ["reader", "/api/companies/company-a%2Fcompany-b/knowledge-pages/page-b", "GET", 404],
      ["reader", "/api/companies/company-a/knowledge-pages/..%2Fpage-b", "GET", 404],
      ["reader", "/api/companies/company-a/knowledge-pages/page-a", "POST", 405],
      ["reader", "/api/companies/company-a/knowledge-pages/ab", "GET", 200],
    ] as const;
    for (const [id, path, method, status] of cases) {
      const response = await handleRequest(new Request(`http://localhost${path}`, { method }), cfg(id));
      expect(response.status).toBe(status);
      if (status !== 200) { const body = await response.text(); expect(body).not.toContain("B-SECRET"); expect(body).not.toContain("WORKSPACE-SECRET"); }
    }
    revokeCompanyMembership(db, root, { userId: "reader", companySlug: "company-a", ...audit });
    expect((await handleRequest(new Request("http://localhost/api/companies/company-a/knowledge-pages/page-a"), cfg("reader"))).status).toBe(401);
  } finally { db.close(); rmSync(root, { recursive: true, force: true }); }
});
