import type {
  BatchScope,
  BookkeepingWorkbenchInput,
  BookkeepingWorkbenchResponse,
  BookkeepingBatchPlanResponse,
  BookkeepingBatchPersistResponse,
  BookkeepingBatchApproveResponse,
  BookkeepingBatchApplyResponse,
  BookkeepingBatchStatusResponse,
} from "../types/bookkeeping-batch";
import { request, type ReadRequestOptions } from "./_shared";

export type { BatchScope } from "../types/bookkeeping-batch";
const path = (slug: string) => `/api/companies/${encodeURIComponent(slug)}/bookkeeping-batch`;
const query = (input: object) => new URLSearchParams(Object.entries(input)
  .filter(([, value]) => value !== undefined)
  .map(([key, value]) => [key, String(value)]));

export const bookkeepingBatchApi = {
  bookkeepingWorkbench: (slug: string, input: BookkeepingWorkbenchInput, options?: ReadRequestOptions) =>
    request<BookkeepingWorkbenchResponse>(`/api/companies/${encodeURIComponent(slug)}/bookkeeping-workbench?${query(input)}`, options),
  bookkeepingBatchPlan: (slug: string, input: BatchScope, options?: ReadRequestOptions) =>
    request<BookkeepingBatchPlanResponse>(`${path(slug)}?${query(input)}`, options),
  bookkeepingBatchPersist: (slug: string, input: BatchScope & { runKey: string }) =>
    request<BookkeepingBatchPersistResponse>(`${path(slug)}/persist`, { method: "POST", body: JSON.stringify({ ...input, confirm: true }) }),
  bookkeepingBatchApprove: (slug: string, input: { runId: number; planHash: string }) =>
    request<BookkeepingBatchApproveResponse>(`${path(slug)}/approve`, { method: "POST", body: JSON.stringify({ ...input, confirm: true }) }),
  bookkeepingBatchApply: (slug: string, input: { runId: number; planHash: string }) =>
    request<BookkeepingBatchApplyResponse>(`${path(slug)}/apply`, { method: "POST", body: JSON.stringify({ ...input, confirm: true }) }),
  bookkeepingBatchStatus: (slug: string, runId: number, options?: ReadRequestOptions) =>
    request<BookkeepingBatchStatusResponse>(`${path(slug)}/runs/${runId}`, options),
};
