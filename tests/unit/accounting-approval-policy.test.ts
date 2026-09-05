import { Database } from "bun:sqlite";
import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { migrate } from "../../src/core/db";
import { createPurchaseCase, reviewPurchaseCase } from "../../src/core/purchase-cases";
import { approveAndPostAccountingDraft, createAccountingDraft, submitAccountingDraft } from "../../src/core/accounting-drafts";
import { seedAccounts, type JournalEntryInput } from "../../src/core/ledger";
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

const draftPayload = (): JournalEntryInput => ({
  transactionDate: "2026-01-02", text: "Synthetic policy draft",
  lines: [{ accountNo: "2000", debitAmount: 100 }, { accountNo: "5000", creditAmount: 100 }],
});

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
      })).toThrow("ELEVATED_APPROVAL_POLICY_UNSUPPORTED");
      expect(() => db.exec("UPDATE rm_accounting_approval_policy_events SET review_mode='independent_reviewer'"))
        .toThrow("append-only");
    } finally { db.close(); rmSync(root, { recursive: true, force: true }); }
  });

  test("retains historical elevated policy evidence but reports it as not enforced", () => {
    const root = workspace();
    const db = setup(root);
    try {
      // Simulates an append-only row created before elevated activation was
      // rejected. The migration deliberately does not rewrite or delete it.
      db.query(`INSERT INTO rm_accounting_approval_policy_events
        (scope_kind,company_slug,risk_class,review_mode,version,prior_event_hash,event_hash,actor,principal_id,created_at)
        VALUES ('company','company-a','elevated','sole_authorized_bookkeeper',1,NULL,?,'agent:legacy','owner','2026-01-02T00:00:00.000Z')`)
        .run("e".repeat(64));
      const historical = getAccountingApprovalPolicy(db, "company-a", "elevated");
      expect(historical).toMatchObject({
        riskClass: "elevated",
        reviewMode: "sole_authorized_bookkeeper",
        enforcement: "not_enforced",
        unsupportedReason: "ELEVATED_APPROVAL_POLICY_UNSUPPORTED",
      });
      expect(() => setAccountingApprovalPolicy(db, root, {
        scope: { kind: "company", companySlug: "company-a" }, riskClass: "elevated", reviewMode: "independent_reviewer",
        expectedEventHash: historical!.eventHash, principalId: "owner", actor: "agent:policy", confirm: true,
      })).toThrow("ELEVATED_APPROVAL_POLICY_UNSUPPORTED");
      expect(getAccountingApprovalPolicy(db, "company-a", "elevated")?.eventHash).toBe("e".repeat(64));
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

  test("binds a purchase review to the exact current policy without using its actor as authority", () => {
    const root = workspace();
    const control = setup(root);
    const ledger = new Database(":memory:");
    try {
      migrate(ledger);
      ledger.run("INSERT INTO companies(name,cvr) VALUES(?,?)", "Synthetic company", "12345678");
      ledger.run("INSERT INTO bank_transactions(transaction_date,amount,currency,text,transaction_hash) VALUES(?,?,?,?,?)", "2026-01-02", -125, "DKK", "Synthetic", "a".repeat(64));
      const policy = setAccountingApprovalPolicy(control, root, {
        scope: { kind: "company", companySlug: "company-a" }, riskClass: "normal", reviewMode: "independent_reviewer",
        expectedEventHash: null, principalId: "owner", actor: "agent:policy", confirm: true,
      });
      const created = createPurchaseCase(ledger, { caseId: "policy-bound-purchase", source: { kind: "bank_transaction", id: 1 }, actor: { createdBy: "agent:author", createdByProgram: "test", auditActor: "agent:author via test" }, principalId: "bookkeeper" });
      if (!created.ok) throw new Error(created.errors.join(","));
      const reviewed = reviewPurchaseCase(ledger, {
        caseId: created.purchaseCase.caseId, expectedVersion: 1, expectedSourceFingerprint: created.purchaseCase.sourceFingerprint,
        documentationOutcome: "ordinary_evidence_sufficient", actor: { createdBy: "agent:review", createdByProgram: "test", auditActor: "agent:review via test" },
        approval: { controlDb: control, workspaceRoot: root, companySlug: "company-a", principalId: "reviewer", expectedPolicyEventHash: policy.policy.eventHash },
      });
      expect(reviewed).toMatchObject({ ok: true, purchaseCase: { version: 2 } });
      const before = ledger.query("SELECT count(*) AS count FROM purchase_case_events").get() as { count: number };
      const changed = setAccountingApprovalPolicy(control, root, {
        scope: { kind: "company", companySlug: "company-a" }, riskClass: "normal", reviewMode: "sole_authorized_bookkeeper",
        expectedEventHash: policy.policy.eventHash, principalId: "owner", actor: "agent:policy", confirm: true,
      });
      expect(changed.policy.version).toBe(2);
      ledger.run("INSERT INTO bank_transactions(id,transaction_date,amount,currency,text,transaction_hash) VALUES(?,?,?,?,?,?)", 2, "2026-01-02", -50, "DKK", "Another synthetic", "b".repeat(64));
      const second = createPurchaseCase(ledger, { caseId: "stale-policy-purchase", source: { kind: "bank_transaction", id: 2 }, actor: { createdBy: "agent:author", createdByProgram: "test", auditActor: "agent:author via test" }, principalId: "bookkeeper" });
      if (!second.ok) throw new Error(second.errors.join(","));
      expect(reviewPurchaseCase(ledger, {
        caseId: second.purchaseCase.caseId, expectedVersion: 1, expectedSourceFingerprint: second.purchaseCase.sourceFingerprint,
        documentationOutcome: "ordinary_evidence_sufficient", actor: { createdBy: "owner", createdByProgram: "test", auditActor: "owner via test" },
        approval: { controlDb: control, workspaceRoot: root, companySlug: "company-a", principalId: "reviewer", expectedPolicyEventHash: policy.policy.eventHash },
      })).toEqual({ ok: false, errors: ["STALE_APPROVAL_POLICY"] });
      expect(ledger.query("SELECT count(*) AS count FROM purchase_case_events").get()).toEqual({ count: before.count + 1 });
    } finally { ledger.close(); control.close(); rmSync(root, { recursive: true, force: true }); }
  });

  test("keeps draft author, submitter and reviewer independent while binding posting to current policy", () => {
    const root = workspace();
    const control = setup(root);
    const ledger = new Database(":memory:");
    try {
      migrate(ledger);
      seedAccounts(ledger);
      const policy = setAccountingApprovalPolicy(control, root, {
        scope: { kind: "company", companySlug: "company-a" }, riskClass: "normal", reviewMode: "independent_reviewer",
        expectedEventHash: null, principalId: "owner", actor: "agent:policy", confirm: true,
      }).policy;
      const created = createAccountingDraft(ledger, "policy-draft", draftPayload(), { createdBy: "agent:author", createdByProgram: "test" }, { principalId: "bookkeeper" });
      const submitted = submitAccountingDraft(ledger, created.id, created.eventHash, { createdBy: "agent:submitter", createdByProgram: "test" }, { principalId: "owner" });
      expect(() => approveAndPostAccountingDraft(ledger, created.id, submitted.eventHash, { createdBy: "agent:reviewer", createdByProgram: "test" }, {
        controlDb: control, workspaceRoot: root, companySlug: "company-a", principalId: "actor-only", expectedPolicyEventHash: policy.eventHash,
      })).toThrow("PRINCIPAL_NOT_AUTHORIZED");
      const posted = approveAndPostAccountingDraft(ledger, created.id, submitted.eventHash, { createdBy: "agent:reviewer", createdByProgram: "test" }, {
        controlDb: control, workspaceRoot: root, companySlug: "company-a", principalId: "reviewer", expectedPolicyEventHash: policy.eventHash,
      });
      expect(posted).toMatchObject({ status: "approved_posted", principalId: "reviewer", approvalPolicyHash: policy.eventHash });
      expect(ledger.query("SELECT approval_policy_hash FROM accounting_draft_events WHERE event_type='approved_posted'").get()).toEqual({ approval_policy_hash: policy.eventHash });
      const changed = setAccountingApprovalPolicy(control, root, {
        scope: { kind: "company", companySlug: "company-a" }, riskClass: "normal", reviewMode: "sole_authorized_bookkeeper",
        expectedEventHash: policy.eventHash, principalId: "owner", actor: "agent:policy", confirm: true,
      }).policy;
      expect(changed.eventHash).not.toBe(policy.eventHash);
    } finally { ledger.close(); control.close(); rmSync(root, { recursive: true, force: true }); }
  });
});
