import { describe, expect, test } from "bun:test";
import { rmSync } from "node:fs";
import { createCompany } from "../../src/core/company";
import { activateWorkspaceUser, grantCompanyMembership } from "../../src/core/workspace-access";
import { initWorkspace } from "../../src/core/workspace";
import { openWorkspaceControlDb } from "../../src/core/workspace-control";
import { handleRequest } from "../../src/server/router";
import type { ServerConfig } from "../../src/server/config";
import { tmpRoot } from "./server-api/_shared";

describe("accounting approval policy HTTP contract", () => {
  test("rejects elevated activation with a bounded code and reads legacy evidence as unenforced", async () => {
    const root = tmpRoot("approval-policy-http");
    try {
      initWorkspace(root);
      const company = createCompany(root, { name: "Synthetic Policy Company" });
      const control = openWorkspaceControlDb(root);
      try {
        control.query(`INSERT INTO "user" (id,name,email,emailVerified,createdAt,updatedAt,twoFactorEnabled)
          VALUES (?,?,?,?,?,?,?)`).run("owner", "Owner", "owner@example.test", 1, "2026-01-01T00:00:00.000Z", "2026-01-01T00:00:00.000Z", 1);
        activateWorkspaceUser(control, { userId: "owner", workspaceRole: "workspace_owner", actor: "agent:test" });
        grantCompanyMembership(control, root, { userId: "owner", companySlug: company.slug, role: "owner", actor: "agent:test" });
        control.query(`INSERT INTO rm_accounting_approval_policy_events
          (scope_kind,company_slug,risk_class,review_mode,version,prior_event_hash,event_hash,actor,principal_id,created_at)
          VALUES ('company',?,'elevated','sole_authorized_bookkeeper',1,NULL,?,'agent:legacy','owner','2026-01-02T00:00:00.000Z')`).run(company.slug, "e".repeat(64));
      } finally { control.close(); }
      const config: ServerConfig = {
        host: "127.0.0.1", port: 0, workspaceRoot: root, authRequired: false, authToken: null,
        authenticateRequest: () => ({ id: "user:owner", userId: "owner", serviceAccountId: "owner", via: "service-principal" }),
      };
      const get = await handleRequest(new Request(`http://localhost/api/companies/${company.slug}/accounting-approval-policy?riskClass=elevated`), config);
      expect(get.status).toBe(200);
      expect(await get.json()).toMatchObject({ policy: { riskClass: "elevated", enforcement: "not_enforced", unsupportedReason: "ELEVATED_APPROVAL_POLICY_UNSUPPORTED" } });
      const set = await handleRequest(new Request(`http://localhost/api/companies/${company.slug}/accounting-approval-policy`, {
        method: "POST", headers: { host: "127.0.0.1", "content-type": "application/json" },
        body: JSON.stringify({ riskClass: "elevated", reviewMode: "independent_reviewer", confirm: true }),
      }), config);
      expect(set.status).toBe(400);
      expect(await set.json()).toEqual({ ok: false, errors: ["elevated approval policy is not supported without a canonical risk classifier"], code: "bad_request", subcode: "ELEVATED_APPROVAL_POLICY_UNSUPPORTED" });
    } finally { rmSync(root, { recursive: true, force: true }); }
  });
});
