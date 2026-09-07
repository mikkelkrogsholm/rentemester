import { request } from "./_shared";

const path = (slug: string) => `/api/companies/${encodeURIComponent(slug)}/bookkeeping-batch`;

export type BatchScope = { companyId: number; accountingFrom: string; accountingTo: string; bankFrom: string; bankTo: string };
export type WorkbenchStatus = "ready" | "suggestedMatch" | "missingDocument" | "partyUnresolved" | "accountingDecisionRequired" | "vatEvidenceRequired" | "dimensionEvidenceRequired" | "stalePlan" | "applyFailed";
export type WorkbenchRow = { bankTransactionId: number; date: string; text: string; amount: number; currency: string; bankAccount: { id: number | null; name: string | null }; document: { id: number; quality: "matched"; party: { id: string; name: string } | null; resolutionState: string } | null; proposed: { account: string | null; vatTreatment: string | null; dimensions: Array<{ dimensionId: string; memberId: string; status: "active" | "inactive" | "missing" }>; partyDefaults?: { account: string | null; vat: string | null; advisoryOnly: true } | null }; status: WorkbenchStatus; nextAction: string; drilldown: { documentId?: number; partyId?: string; bankTransactionId: number; bankAccountId?: number; runId?: number; journalEntryId?: number; periodClose: { from: string; to: string } }; sourceHash: string };
export type BookkeepingPlan = { planHash: string; candidateSetHash?: string; items: Array<{ actionKey: string; partition: string }> ; scope?: Partial<BatchScope> };
export type Workbench = { state: "available" | "zero" | "incomplete" | "unavailable"; counts: Record<WorkbenchStatus, number>; population: { total: number; ready: number; blockers: number }; selection: { total: number; ready: number; blockers: number }; page: { cursor?: number; total: number; nextCursor: number | null }; completeness: { state?: string; reasonCode?: string; nextAction: string }; rows: WorkbenchRow[]; periodClose: { status: "available" | "unavailable"; blockers: number; hash?: string } | null; plan: { planHash: string; candidateSetHash: string; readyCount: number } | null };
export type WorkbenchResponse = { ok: true; workbench: Workbench };
export type BatchPlanResponse = { ok: true; dryRun: true; plan: BookkeepingPlan };
export type BatchRunState = { run?: { runId: number; plan: string }; revisions: unknown[]; attempts: unknown[]; receipts: unknown[] };
export type BatchRunResponse = { ok: true; dryRun?: true; runId: number; duplicate?: boolean; plan: BookkeepingPlan; state: BatchRunState };
export type BatchApprovalResponse = { ok: true; state: BatchRunState };
export type BatchApplyResponse = { ok: true; runId: number; results: unknown[]; checks: unknown[]; state?: BatchRunState };
export type WorkbenchInput = { from: string; to: string; status?: WorkbenchStatus; documentQuality?: "matched" | "missing"; account?: string; vatTreatment?: string; dimension?: string; cursor?: number; limit?: number; search?: string };

const query = (input: Record<string, string | number | undefined>) => new URLSearchParams(Object.entries(input).filter(([, value]) => value !== undefined).map(([key, value]) => [key, String(value)])).toString();

export const bookkeepingBatchApi = {
  bookkeepingWorkbench: (slug: string, input: WorkbenchInput) => request<WorkbenchResponse>(`/api/companies/${encodeURIComponent(slug)}/bookkeeping-workbench?${query(input)}`),
  bookkeepingBatchPlan: (slug: string, input: BatchScope) => request<BatchPlanResponse>(`${path(slug)}?${query(input)}`),
  bookkeepingBatchPersist: (slug: string, input: BatchScope & { runKey: string }) => request<BatchRunResponse>(`${path(slug)}/persist`, { method: "POST", body: JSON.stringify({ ...input, confirm: true }) }),
  bookkeepingBatchApprove: (slug: string, input: { runId: number; planHash: string }) => request<BatchApprovalResponse>(`${path(slug)}/approve`, { method: "POST", body: JSON.stringify({ ...input, confirm: true }) }),
  bookkeepingBatchApply: (slug: string, input: { runId: number; planHash: string }) => request<BatchApplyResponse>(`${path(slug)}/apply`, { method: "POST", body: JSON.stringify({ ...input, confirm: true }) }),
  bookkeepingBatchStatus: (slug: string, runId: number) => request<{ ok: true; state: BatchRunState }>(`${path(slug)}/runs/${runId}`),
};
