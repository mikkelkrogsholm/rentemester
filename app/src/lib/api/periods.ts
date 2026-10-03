import type {
  ClosePeriodInput,
  ClosePeriodResponse,
  PeriodsResponse,
  ReopenPeriodInput,
  ReopenPeriodResponse,
  PeriodCloseReadinessResponse,
  PeriodCloseReviewResponse,
} from "../types";
import { request, type ReadRequestOptions } from "./_shared";

export const periodsApi = {
  /**
   * #342 — Periodelås-liste (read).
   */
  periods: (slug: string, options?: ReadRequestOptions) =>
    request<PeriodsResponse>(
      `/api/companies/${encodeURIComponent(slug)}/periods`, options,
    ).then((r) => r.periods),
  closeReadiness: (slug: string, periodStart: string, periodEnd: string, options?: ReadRequestOptions) =>
    request<PeriodCloseReadinessResponse>(`/api/companies/${encodeURIComponent(slug)}/periods/close-readiness?from=${encodeURIComponent(periodStart)}&to=${encodeURIComponent(periodEnd)}`, options).then((r) => r.packet),
  reviewCloseReadiness: (slug: string, periodStart: string, periodEnd: string) =>
    request<PeriodCloseReviewResponse>(`/api/companies/${encodeURIComponent(slug)}/periods/close-review`, { method: "POST", body: JSON.stringify({ periodStart, periodEnd, confirm: true }) }).then((r) => r.review),

  /**
   * Closes an accounting period (#287) — the prerequisite for a momsangivelse.
   * Calls the same `closeAccountingPeriod` core the CLI's `period close` uses.
   * Write-irreversible-shaped, so the server's pipeline requires `confirm`.
   */
  closePeriod: (slug: string, input: ClosePeriodInput) =>
    request<ClosePeriodResponse>(
      `/api/companies/${encodeURIComponent(slug)}/periods/close`,
      {
        method: "POST",
        body: JSON.stringify({
          periodStart: input.periodStart,
          periodEnd: input.periodEnd,
          ...(input.kind ? { kind: input.kind } : {}),
          ...(input.reference ? { reference: input.reference } : {}),
          ...(input.packetHash ? { packetHash: input.packetHash } : {}),
          reviewId: input.reviewId,
          ...(input.force ? { force: true, reason: input.reason } : {}),
          confirm: true,
        }),
      },
    ).then((r) => r.period),

  /**
   * Reopens a closed accounting period (#301) — the controlled, audit-logged
   * recovery path for a period closed too early. `reason` is recorded verbatim
   * in the audit log. Calls the same `reopenAccountingPeriod` core the CLI's
   * `period reopen` uses; the server's pipeline requires `confirm`.
   */
  reopenPeriod: (slug: string, input: ReopenPeriodInput) =>
    request<ReopenPeriodResponse>(
      `/api/companies/${encodeURIComponent(slug)}/periods/reopen`,
      {
        method: "POST",
        body: JSON.stringify({
          periodStart: input.periodStart,
          periodEnd: input.periodEnd,
          ...(input.kind ? { kind: input.kind } : {}),
          reason: input.reason,
          confirm: true,
        }),
      },
    ).then((r) => r.period),
};
