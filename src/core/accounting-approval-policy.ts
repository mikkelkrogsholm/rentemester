/**
 * Workspace-owned approval policy. It decides who may approve an already
 * otherwise-valid accounting action; it never authorizes access by actor and
 * it never changes a company ledger.
 */
import { createHash } from "node:crypto";
import type { Database } from "bun:sqlite";
import { canonicalJson } from "./canonical-json";
import { insertWorkspaceAudit } from "./workspace-control";
import { authorizeWorkspaceRoute } from "./workspace-access";

export const ACCOUNTING_APPROVAL_RISK_CLASSES = ["normal", "elevated"] as const;
export type AccountingApprovalRiskClass = typeof ACCOUNTING_APPROVAL_RISK_CLASSES[number];
export const ACCOUNTING_APPROVAL_REVIEW_MODES = ["sole_authorized_bookkeeper", "independent_reviewer"] as const;
export type AccountingApprovalReviewMode = typeof ACCOUNTING_APPROVAL_REVIEW_MODES[number];
export const ACCOUNTING_APPROVAL_ACTIONS = ["purchase_case_review", "bookkeeping_batch_approve", "accounting_draft_review"] as const;
export type AccountingApprovalAction = typeof ACCOUNTING_APPROVAL_ACTIONS[number];

type PolicyRow = {
  scope_kind: "workspace" | "company";
  company_slug: string | null;
  risk_class: AccountingApprovalRiskClass;
  review_mode: AccountingApprovalReviewMode;
  version: number;
  prior_event_hash: string | null;
  event_hash: string;
  actor: string;
  principal_id: string;
  created_at: string;
};

export type AccountingApprovalPolicy = {
  scope: { kind: "workspace" } | { kind: "company"; companySlug: string };
  riskClass: AccountingApprovalRiskClass;
  reviewMode: AccountingApprovalReviewMode;
  version: number;
  eventHash: string;
  previousEventHash: string | null;
  actor: string;
  principalId: string;
  createdAt: string;
};

export type AccountingApprovalDecision = {
  allowed: boolean;
  code: "ALLOWED" | "PRINCIPAL_NOT_AUTHORIZED" | "INDEPENDENT_REVIEW_REQUIRED";
  action: AccountingApprovalAction;
  reviewMode: AccountingApprovalReviewMode;
  policy: AccountingApprovalPolicy | null;
};

const sha256 = (value: string) => createHash("sha256").update(value).digest("hex");
const text = (value: unknown, label: string, max = 160): string => {
  const normalized = typeof value === "string" ? value.trim().normalize("NFC") : "";
  if (!normalized || normalized.length > max) throw new Error(`${label} is required and bounded`);
  return normalized;
};
function risk(value: unknown): AccountingApprovalRiskClass {
  if (!ACCOUNTING_APPROVAL_RISK_CLASSES.includes(value as AccountingApprovalRiskClass)) throw new Error("unsupported approval risk class");
  return value as AccountingApprovalRiskClass;
}
function mode(value: unknown): AccountingApprovalReviewMode {
  if (!ACCOUNTING_APPROVAL_REVIEW_MODES.includes(value as AccountingApprovalReviewMode)) throw new Error("unsupported approval review mode");
  return value as AccountingApprovalReviewMode;
}
function rowPolicy(row: PolicyRow): AccountingApprovalPolicy {
  return {
    scope: row.scope_kind === "workspace" ? { kind: "workspace" } : { kind: "company", companySlug: row.company_slug! },
    riskClass: row.risk_class, reviewMode: row.review_mode, version: row.version,
    eventHash: row.event_hash, previousEventHash: row.prior_event_hash,
    actor: row.actor, principalId: row.principal_id, createdAt: row.created_at,
  };
}
function current(db: Database, scope: "workspace" | "company", companySlug: string | null, riskClass: AccountingApprovalRiskClass): PolicyRow | null {
  return db.query(`SELECT scope_kind,company_slug,risk_class,review_mode,version,prior_event_hash,event_hash,actor,principal_id,created_at
    FROM rm_current_accounting_approval_policies WHERE scope_kind=? AND company_slug IS ? AND risk_class=?`).get(scope, companySlug, riskClass) as PolicyRow | null;
}

/** The company-specific rule wins; absence remains fail-safe four-eyes review. */
export function getAccountingApprovalPolicy(db: Database, companySlug: string, riskClass: AccountingApprovalRiskClass = "normal"): AccountingApprovalPolicy | null {
  const company = text(companySlug, "companySlug", 120);
  const selectedRisk = risk(riskClass);
  const selected = current(db, "company", company, selectedRisk) ?? current(db, "workspace", null, selectedRisk);
  return selected ? rowPolicy(selected) : null;
}

export function setAccountingApprovalPolicy(
  db: Database,
  workspaceRoot: string,
  input: {
    scope: { kind: "workspace" } | { kind: "company"; companySlug: string };
    riskClass: AccountingApprovalRiskClass;
    reviewMode: AccountingApprovalReviewMode;
    expectedEventHash: string | null;
    principalId: string;
    actor: string;
    confirm: boolean;
  },
): { policy: AccountingApprovalPolicy; replayed: boolean } {
  if (input.confirm !== true) throw new Error("approval policy change requires confirm");
  const principalId = text(input.principalId, "principalId");
  const actor = text(input.actor, "actor");
  const selectedRisk = risk(input.riskClass);
  const reviewMode = mode(input.reviewMode);
  const scope = input.scope.kind;
  const companySlug = scope === "company" ? text(input.scope.companySlug, "companySlug", 120) : null;
  const permission = scope === "company" ? "company.admin" : "workspace.manage";
  if (!authorizeWorkspaceRoute(db, workspaceRoot, { userId: principalId, permission, ...(companySlug ? { companySlug } : {}) }).allowed) {
    throw new Error("approval policy principal is not authorized");
  }
  const expected = input.expectedEventHash === null ? null : text(input.expectedEventHash, "expectedEventHash", 64);
  return db.transaction(() => {
    const prior = current(db, scope, companySlug, selectedRisk);
    if ((prior?.event_hash ?? null) !== expected) {
      if (prior && prior.prior_event_hash === expected && prior.review_mode === reviewMode && prior.actor === actor && prior.principal_id === principalId) {
        return { policy: rowPolicy(prior), replayed: true };
      }
      throw new Error("STALE_APPROVAL_POLICY");
    }
    const version = (prior?.version ?? 0) + 1;
    const createdAt = new Date().toISOString();
    const eventHash = sha256(canonicalJson({ scope, companySlug, riskClass: selectedRisk, reviewMode, version, priorEventHash: expected, actor, principalId, createdAt }));
    db.query(`INSERT INTO rm_accounting_approval_policy_events
      (scope_kind,company_slug,risk_class,review_mode,version,prior_event_hash,event_hash,actor,principal_id,created_at)
      VALUES (?,?,?,?,?,?,?,?,?,?)`).run(scope, companySlug, selectedRisk, reviewMode, version, expected, eventHash, actor, principalId, createdAt);
    insertWorkspaceAudit(db, { eventType: "accounting_approval_policy_set", entityType: "accounting_approval_policy", entityId: eventHash, createdBy: actor, createdByProgram: "accounting-approval-policy" });
    return { policy: rowPolicy({ scope_kind: scope, company_slug: companySlug, risk_class: selectedRisk, review_mode: reviewMode, version, prior_event_hash: expected, event_hash: eventHash, actor, principal_id: principalId, created_at: createdAt }), replayed: false };
  }).immediate();
}

/**
 * Uses memberships for every decision. An actor is deliberately absent: audit
 * attribution cannot become an authorization credential.
 */
export function evaluateAccountingApproval(
  db: Database,
  workspaceRoot: string,
  input: { companySlug: string; action: AccountingApprovalAction; principalId: string; proposedByPrincipalId?: string | null; riskClass?: AccountingApprovalRiskClass },
): AccountingApprovalDecision {
  if (!ACCOUNTING_APPROVAL_ACTIONS.includes(input.action)) throw new Error("unsupported accounting approval action");
  const companySlug = text(input.companySlug, "companySlug", 120);
  const principalId = text(input.principalId, "principalId");
  const riskClass = risk(input.riskClass ?? "normal");
  const policy = getAccountingApprovalPolicy(db, companySlug, riskClass);
  const reviewMode = policy?.reviewMode ?? "independent_reviewer";
  const permission = reviewMode === "sole_authorized_bookkeeper" ? "company.ledger.post" : "company.review";
  if (!authorizeWorkspaceRoute(db, workspaceRoot, { userId: principalId, companySlug, permission }).allowed) {
    return { allowed: false, code: "PRINCIPAL_NOT_AUTHORIZED", action: input.action, reviewMode, policy };
  }
  const proposedBy = typeof input.proposedByPrincipalId === "string" ? input.proposedByPrincipalId.trim() : "";
  if (reviewMode === "independent_reviewer" && (!proposedBy || proposedBy === principalId)) {
    return { allowed: false, code: "INDEPENDENT_REVIEW_REQUIRED", action: input.action, reviewMode, policy };
  }
  return { allowed: true, code: "ALLOWED", action: input.action, reviewMode, policy };
}

export function assertAccountingApproval(...args: Parameters<typeof evaluateAccountingApproval>): AccountingApprovalDecision {
  const decision = evaluateAccountingApproval(...args);
  if (!decision.allowed) throw new Error(decision.code);
  return decision;
}
