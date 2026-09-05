import { describe, expect, test } from "bun:test";
import { rmSync } from "node:fs";
import { createCompany } from "../../src/core/company";
import { openDb } from "../../src/core/db";
import { companyPaths } from "../../src/core/paths";
import { createPurchaseCase, purchaseCaseSourceFingerprint } from "../../src/core/purchase-cases";
import { activateWorkspaceUser, grantCompanyMembership } from "../../src/core/workspace-access";
import { companyRootForSlug, initWorkspace } from "../../src/core/workspace";
import { openWorkspaceControlDb } from "../../src/core/workspace-control";
import { handleRequest } from "../../src/server/router";
import type { ServerConfig } from "../../src/server/config";
import { tmpRoot } from "./server-api/_shared";

describe("purchase-case HTTP contract", () => {
  test("reassesses a stale source through the authenticated route and reads it back", async () => {
    const workspace = tmpRoot("purchase-case-http");
    try {
      initWorkspace(workspace);
      const company = createCompany(workspace, { name: "Synthetic HTTP Purchase Company" });
      const control = openWorkspaceControlDb(workspace);
      try {
        for (const id of ["author", "reviewer"]) {
          control.query(`INSERT INTO "user" (id,name,email,emailVerified,createdAt,updatedAt,twoFactorEnabled)
            VALUES (?,?,?,?,?,?,?)`).run(id, id, `${id}@example.test`, 1, "2026-01-01T00:00:00.000Z", "2026-01-01T00:00:00.000Z", 1);
          activateWorkspaceUser(control, { userId: id, workspaceRole: "member", actor: "user:owner" });
        }
        grantCompanyMembership(control, workspace, { userId: "author", companySlug: company.slug, role: "bookkeeper", actor: "user:owner" });
        grantCompanyMembership(control, workspace, { userId: "reviewer", companySlug: company.slug, role: "reviewer", actor: "user:owner" });
      } finally { control.close(); }
      const db = openDb(companyPaths(companyRootForSlug(workspace, company.slug)).db);
      db.run("INSERT INTO bank_transactions(id,transaction_date,amount,currency,text,transaction_hash) VALUES(1,'2026-01-02',-125,'DKK','Synthetic original',?)", "a".repeat(64));
      const created = createPurchaseCase(db, { caseId: "http-stale-case", source: { kind: "bank_transaction", id: 1 }, actor: { createdBy: "agent:author", createdByProgram: "test" }, principalId: "author" });
      if (!created.ok) throw new Error(created.errors.join(","));
      db.run("UPDATE bank_transactions SET text='Synthetic changed' WHERE id=1");
      const currentSourceFingerprint = purchaseCaseSourceFingerprint(db, { kind: "bank_transaction", id: 1 })!;
      db.close();
      const config: ServerConfig = {
        host: "127.0.0.1", port: 0, workspaceRoot: workspace, authRequired: false, authToken: null,
        authenticateRequest: () => ({ id: "user:reviewer", userId: "reviewer", via: "session" }),
      };
      const response = await handleRequest(new Request(`http://localhost/api/companies/${company.slug}/purchase-cases/http-stale-case/reassess`, {
        method: "POST",
        headers: { host: "127.0.0.1", "content-type": "application/json", "idempotency-key": "http-stale-reassess" },
        body: JSON.stringify({ expectedVersion: 1, expectedSourceFingerprint: created.purchaseCase.sourceFingerprint, currentSourceFingerprint, documentationOutcome: "ordinary_evidence_sufficient", reason: "Synthetic HTTP source review", confirm: true }),
      }), config);
      expect(response.status).toBe(200);
      expect(await response.json()).toMatchObject({ ok: true, purchaseCase: { version: 2, sourceFingerprint: currentSourceFingerprint, sourceStatus: { status: "current" } } });
      const readBack = await handleRequest(new Request(`http://localhost/api/companies/${company.slug}/purchase-cases/http-stale-case`), config);
      expect(await readBack.json()).toMatchObject({ purchaseCase: { version: 2, documentationOutcome: "ordinary_evidence_sufficient", sourceStatus: { status: "current" } } });
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });
});
