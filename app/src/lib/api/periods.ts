import type {
  ClosePeriodInput,
  ClosePeriodResponse,
  PeriodsResponse,
  ReopenPeriodInput,
  ReopenPeriodResponse,
} from "../types";
import { request } from "./_shared";

// Two definitions of `closePeriod` and `reopenPeriod` existed in the original
// `api.ts`: the later definitions shadow the earlier ones. To preserve that
// last-wins semantics deterministically we split them into two consts and
// spread them in the original order in the barrel.

export const periodsApiLegacy = {
  /**
   * #342 — Periodelås-liste (read).
   */
  periods: (slug: string) =>
    request<PeriodsResponse>(
      `/api/companies/${encodeURIComponent(slug)}/periods`,
    ).then((r) => r.periods),

  /**
   * #342 — close a period. Body shape matches CLI's `period close`.
   */
  closePeriod: (
    slug: string,
    body: {
      periodStart: string;
      periodEnd: string;
      kind: "vat_quarter" | "fiscal_year" | "custom";
      status?: "closed" | "reported";
      reference?: string;
    },
  ) =>
    request<{ ok: true; period: { id: number; effectiveStatus?: string } }>(
      `/api/companies/${encodeURIComponent(slug)}/periods/close`,
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(body),
      },
    ).then((r) => r.period),

  /**
   * #342 — reopen a closed period; `reason` is mandatory and recorded verbatim
   * in the audit log.
   */
  reopenPeriod: (
    slug: string,
    body: {
      periodStart: string;
      periodEnd: string;
      kind: "vat_quarter" | "fiscal_year" | "custom";
      reason: string;
    },
  ) =>
    request<{ ok: true; period: { id: number; effectiveStatus?: string } }>(
      `/api/companies/${encodeURIComponent(slug)}/periods/reopen`,
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(body),
      },
    ).then((r) => r.period),
};

export const periodsApi = {
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
