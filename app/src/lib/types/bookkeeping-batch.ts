// Browser-safe wire contracts mirror the core plan, workbench and router SQL
// projections. No runtime module imports may pull the ledger into the SPA.
export type BatchPrincipalKind = "user" | "service-account" | "local-trusted";
export type FinalCheckName = "audit_chain" | "trial_balance" | "reconciliation" | "vat";
export type BatchScope = { companyId: number; accountingFrom: string; accountingTo: string; bankFrom: string; bankTo: string };
export type BookkeepingBatchPlan = BatchScope & {
  items: Array<{
    actionKey: string; evidenceHash: string;
    partition: "ready" | "suggestedMatch" | "missingDocument" | "humanDecision";
    documentId?: number; bankTransactionId?: number;
    ruleApplication?: { ruleVersionId: number; payloadHash: string };
    detail: Record<string, unknown>;
  }>;
  sourceIdentities: Record<string, unknown>;
  candidateSetHash: string;
  planHash: string;
};
export type WorkbenchStatus = "ready" | "suggestedMatch" | "missingDocument" | "partyUnresolved" | "accountingDecisionRequired" | "vatEvidenceRequired" | "dimensionEvidenceRequired" | "stalePlan" | "applyFailed";
export type BookkeepingWorkbenchInput = {
  from: string; to: string; status?: WorkbenchStatus; bankAccountId?: number;
  partyId?: string; documentQuality?: "matched" | "missing"; account?: string;
  vatTreatment?: string; dimension?: string; limit?: number; cursor?: number; search?: string;
};
export type WorkbenchRow = {
  bankTransactionId: number; date: string; text: string; amount: number; currency: string;
  bankAccount: { id: number | null; name: string | null };
  document: {
    id: number; quality: "matched"; party: { id: string; name: string } | null;
    resolutionState: "resolved" | "internal_no_external_party" | "unresolved";
  } | null;
  proposed: {
    account: string | null; vatTreatment: string | null;
    dimensions: Array<{ dimensionId: string; memberId: string; status: "active" | "inactive" | "missing" }>;
    partyDefaults?: { account: string | null; vat: string | null; advisoryOnly: true } | null;
  };
  status: WorkbenchStatus; nextAction: string; sourceHash: string;
  drilldown: {
    documentId?: number; partyId?: string; bankTransactionId: number; bankAccountId?: number;
    runId?: number; journalEntryId?: number; periodClose: { from: string; to: string };
  };
};
type WorkbenchReadiness = { total: number; ready: number; blockers: number };
export type BookkeepingWorkbenchResponse = {
  ok: true;
  workbench: {
    scope: { from: string; to: string; cutOverDate: string | null };
    state: "unavailable" | "incomplete" | "zero" | "available";
    completeness: {
      state: "unavailable" | "incomplete" | "zero" | "available";
      reasonCode: string; nextAction: string;
    };
    rows: WorkbenchRow[];
    page: { cursor: number; limit: number; total: number; nextCursor: number | null };
    counts: Record<WorkbenchStatus, number>;
    population: WorkbenchReadiness;
    selection: WorkbenchReadiness;
    staleSources: Array<{
      bankTransactionId: number | null; actionKey: string;
      changedSource: "evidence_changed" | "added_to_candidate_set" | "removed_from_candidate_set";
    }>;
    sourceHash: string;
    periodClose: { status: "available" | "unavailable"; blockers: number; hash?: string } | null;
    plan: { planHash: string; candidateSetHash: string; readyCount: number } | null;
  };
};
export type BookkeepingBatchPlanResponse = { ok: true; dryRun: true; plan: BookkeepingBatchPlan };

/** SQL column aliases are the exact append-only HTTP state representation. */
export type BookkeepingBatchState = {
  run: { runId: number; runKey: string; planHash: string; plan: string; createdAt: string };
  revisions: Array<{
    revisionId: number;
    planHash: string;
    candidateSetHash: string;
    plannerKind: BatchPrincipalKind;
    plannerSubjectId: string;
    plannerActor: string;
    createdAt: string;
    approverKind: BatchPrincipalKind | null;
    approverSubjectId: string | null;
    approverActor: string | null;
    approvedAt: string | null;
  }>;
  attempts: Array<{
    attemptId: number;
    planHash: string;
    principalKind: BatchPrincipalKind;
    principalSubjectId: string;
    actor: string;
    startedAt: string;
    eventType: "started" | "source_stale" | "item_applied" | "item_failed" | "final_checks" | "completed" | null;
    actionKey: string | null;
    detail: string | null;
    createdAt: string | null;
  }>;
  receipts: Array<{ actionKey: string; receipt: string; createdAt: string }>;
  finalChecks: Array<{ attemptId: number; name: FinalCheckName; ok: 0 | 1; detail: string; createdAt: string }>;
};

export type BookkeepingBatchStatusResponse = { ok: true; state: BookkeepingBatchState | null };
export type BookkeepingBatchPersistResponse = BookkeepingBatchPlanResponse & {
  runId: number;
  duplicate: boolean;
  state: BookkeepingBatchState | null;
};
export type BookkeepingBatchApproveResponse = BookkeepingBatchStatusResponse;
export type BookkeepingBatchApplyResponse = {
  ok: boolean;
  results: Array<{ actionKey: string; outcome: string; error?: string }>;
  checks: Array<{ name: FinalCheckName; ok: boolean; detail?: Record<string, unknown> }>;
  applyAttemptId?: number;
  errors?: string[];
  error?: { code: string; cause: string };
  runId: number;
  planHash: string;
  state: BookkeepingBatchState | null;
};
