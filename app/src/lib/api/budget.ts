import type {
  BudgetResponse,
  BudgetVsActualResponse,
  SetBudgetInput,
} from "../types";
import { request } from "./_shared";

export const budgetApi = {
  /**
   * #339 — the effective (latest-revision) budget lines for one fiscal year.
   * Drives the Budget input grid in the cockpit. The server collapses every
   * (account, period) pair to the newest revision before returning.
   */
  budget: (slug: string, year?: string) =>
    request<BudgetResponse>(
      `/api/companies/${encodeURIComponent(slug)}/budget${
        year ? `?year=${encodeURIComponent(year)}` : ""
      }`,
    ).then((r) => r.budget),

  /**
   * #339 — the budget-vs-faktisk comparison for one fiscal year. Third caller
   * of the same `buildBudgetVsActual` core the CLI report uses, so every
   * surface gets the same numbers and the same sign convention.
   */
  budgetVsActual: (slug: string, year?: string) =>
    request<BudgetVsActualResponse>(
      `/api/companies/${encodeURIComponent(slug)}/budget-vs-actual${
        year ? `?year=${encodeURIComponent(year)}` : ""
      }`,
    ).then((r) => r.budgetVsActual),

  /**
   * #339 — append a budget revision for one (account, period) cell. The core
   * is append-only: a second call for the same pair inserts a new revision
   * and the latest wins, so re-saving the grid is always safe.
   */
  setBudget: (slug: string, input: SetBudgetInput) =>
    request<{
      ok: true;
      budget: {
        id: number | null;
        accountNo: string | null;
        period: string | null;
        amount: number | null;
      };
    }>(`/api/companies/${encodeURIComponent(slug)}/budget`, {
      method: "POST",
      body: JSON.stringify({
        accountNo: input.accountNo,
        period: input.period,
        amount: input.amount,
        ...(input.notes ? { notes: input.notes } : {}),
      }),
    }).then((r) => r.budget),
};
