import { Database } from "bun:sqlite";
import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  evaluateAccountingApproval,
  getAccountingApprovalPolicy,
  setAccountingApprovalPolicy,
} from "../../src/core/accounting-approval-policy";
import { activateWorkspaceUser, grantCompanyMembership } from "../../src/core/workspace-access";
import { openWorkspaceControlDb } from "../../src/core/workspace-control";
import { initWorkspace, registerWorkspaceCompany } from "../../src/core/workspace";

function workspace() { return mkdtempSync(join(tmpdir(), "rentemester-approval-policy-")); }
function addUser(db: Database, id: string) {
  db.query(`INSERT INTO "user" (id,name,email,emailVerified,createdAt,updatedAt,twoFactorEnabled)
    VALUES (?,?,?,?,?,?,?)`).run(id, id, `${id}@example.test`, 1, "2026-01-01T00:00:00.000Z", "2026-01-01T00:00:00.000Z", 1);
}
function setup(root: string) {
  initWorkspace(root);
  for (const slug of ["company-a", "company-b"]) registerWorkspaceCompany(root, { slug, name: slug, createdAt: "2026-01-01T00:00:00.000Z", archived: false });
  const db = openWorkspaceControlDb(root);
  for (const id of ["owner", "bookkeeper", "reviewer", "actor-only"]) addUser(db, id);
  activateWorkspaceUser(db, { userId: "owner", workspaceRole: "workspace_owner", createdBy: "agent:test", createdByProgram: "test" });
  for (const id of ["bookkeeper", "reviewer", "actor-only"]) activateWorkspaceUser(db, { userId: id, workspaceRole: "member", createdBy: "agent:test", createdByProgram: "test" });
  grantCompanyMembership(db, root, { userId: "owner", companySlug: "company-a", role: "owner", createdBy: "agent:test", createdByProgram: "test" });
  grantCompanyMembership(db, root, { userId: "bookkeeper", companySlug: "company-a", role: "bookkeeper", createdBy: "agent:test", createdByProgram: "test" });
  grantCompanyMembership(db, root, { userId: "reviewer", companySlug: "company-a", role: "reviewer", createdBy: "agent:test", createdByProgram: "test" });
  return db;
}

describe("accounting approval policy", () => {
  test("allows a sole authorized bookkeeper only through scoped membership and never through actor", () => {
    const root = workspace();
    const db = setup(root);
    try {
      const created = setAccountingApprovalPolicy(db, root, {
        scope: { kind: "company", companySlug: "company-a" }, riskClass: "normal", reviewMode: "sole_authorized_bookkeeper",
        expectedEventHash: null, principalId: "owner", actor: "agent:policy", confirm: true,
      });
      expect(created.replayed).toBe(false);
      expect(getAccountingApprovalPolicy(db, "company-a")).toMatchObject({ reviewMode: "sole_authorized_bookkeeper", eventHash: created.policy.eventHash });
      expect(evaluateAccountingApproval(db, root, { companySlug: "company-a", action: "purchase_case_review", principalId: "bookkeeper", proposedByPrincipalId: "bookkeeper" })).toMatchObject({ allowed: true, code: "ALLOWED" });
      expect(evaluateAccountingApproval(db, root, { companySlug: "company-b", action: "purchase_case_review", principalId: "bookkeeper", proposedByPrincipalId: "bookkeeper" })).toMatchObject({ allowed: false, code: "PRINCIPAL_NOT_AUTHORIZED" });
      expect(() => setAccountingApprovalPolicy(db, root, {
        scope: { kind: "company", companySlug: "company-a" }, riskClass: "elevated", reviewMode: "sole_authorized_bookkeeper",
        expectedEventHash: null, principalId: "actor-only", actor: "owner", confirm: true,
      })).toThrow("not authorized");
      expect(() => db.exec("UPDATE rm_accounting_approval_policy_events SET review_mode='independent_reviewer'"))
        .toThrow("append-only");
    } finally { db.close(); rmSync(root, { recursive: true, force: true }); }
  });

  test("requires independent reviewer by default and rejects stale policy writes before a new event", () => {
    const root = workspace();
    const db = setup(root);
    try {
      expect(evaluateAccountingApproval(db, root, { companySlug: "company-a", action: "accounting_draft_review", principalId: "bookkeeper", proposedByPrincipalId: "bookkeeper" }))
        .toMatchObject({ allowed: false, code: "PRINCIPAL_NOT_AUTHORIZED", reviewMode: "independent_reviewer" });
      const policy = setAccountingApprovalPolicy(db, root, {
        scope: { kind: "company", companySlug: "company-a" }, riskClass: "normal", reviewMode: "independent_reviewer",
        expectedEventHash: null, principalId: "owner", actor: "agent:policy", confirm: true,
      });
      expect(evaluateAccountingApproval(db, root, { companySlug: "company-a", action: "accounting_draft_review", principalId: "reviewer", proposedByPrincipalId: "bookkeeper" }))
        .toMatchObject({ allowed: true, code: "ALLOWED" });
      expect(evaluateAccountingApproval(db, root, { companySlug: "company-a", action: "accounting_draft_review", principalId: "reviewer", proposedByPrincipalId: "reviewer" }))
        .toMatchObject({ allowed: false, code: "INDEPENDENT_REVIEW_REQUIRED" });
      const changed = setAccountingApprovalPolicy(db, root, {
        scope: { kind: "company", companySlug: "company-a" }, riskClass: "normal", reviewMode: "sole_authorized_bookkeeper",
        expectedEventHash: policy.policy.eventHash, principalId: "owner", actor: "agent:policy", confirm: true,
      });
      expect(changed.policy.version).toBe(2);
      expect(() => setAccountingApprovalPolicy(db, root, {
        scope: { kind: "company", companySlug: "company-a" }, riskClass: "normal", reviewMode: "independent_reviewer",
        expectedEventHash: policy.policy.eventHash, principalId: "owner", actor: "agent:policy", confirm: true,
      })).toThrow("STALE_APPROVAL_POLICY");
      expect(setAccountingApprovalPolicy(db, root, {
        scope: { kind: "company", companySlug: "company-a" }, riskClass: "normal", reviewMode: "sole_authorized_bookkeeper",
        expectedEventHash: policy.policy.eventHash, principalId: "owner", actor: "agent:policy", confirm: true,
      })).toMatchObject({ replayed: true, policy: { eventHash: changed.policy.eventHash } });
    } finally { db.close(); rmSync(root, { recursive: true, force: true }); }
  });
});
