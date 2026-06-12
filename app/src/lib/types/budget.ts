// Budget / Budget wire types (GET .../budget?year=, .../budget-vs-actual) — #339.
//
// All money fields below are kroner (DKK with decimals) — use `formatKroner`.

import type { FiscalYearEntry, StatementCompany } from "./common";

/** One effective (latest-revision) budget line for a company. */
export type CompanyBudgetLine = {
  id: number;
  accountNo: string;
  accountName: string | null;
  /** `YYYY-MM` calendar month. */
  period: string;
  amount: number;
  notes: string | null;
  createdAt: string;
};

export type CompanyBudget = {
  slug: string;
  selectedYear: string;
  archived: boolean;
  company: StatementCompany;
  fiscalYears: FiscalYearEntry[];
  /** First calendar month covered, `YYYY-MM`. */
  periodStart: string;
  /** Last calendar month covered, `YYYY-MM`. */
  periodEnd: string;
  /** Every calendar month inside the fiscal year, chronological. */
  periods: string[];
  /** Effective budget lines, ordered period→account. */
  lines: CompanyBudgetLine[];
  /** Sum of every line's amount, kroner. */
  totalBudget: number;
};

export type BudgetResponse = {
  ok: true;
  budget: CompanyBudget;
};

/** One row in the budget-vs-faktisk comparison. */
export type CompanyBudgetVsActualLine = {
  accountNo: string;
  accountName: string | null;
  accountType: string | null;
  period: string;
  budget: number;
  actual: number;
  /**
   * Signed variance — see core/budget.ts for the per-account-type sign
   * convention. Positive is "good" (under budget for expense, over target
   * for income), negative is "bad".
   */
  variance: number;
  /** Variance as a fraction of `budget`; null when `budget` is 0. */
  variancePercent: number | null;
};

export type CompanyBudgetVsActual = {
  slug: string;
  selectedYear: string;
  archived: boolean;
  company: StatementCompany;
  fiscalYears: FiscalYearEntry[];
  periodStart: string;
  periodEnd: string;
  lines: CompanyBudgetVsActualLine[];
  totalBudget: number;
  totalActual: number;
  totalVariance: number;
};

export type BudgetVsActualResponse = {
  ok: true;
  budgetVsActual: CompanyBudgetVsActual;
};

/** Input for `api.setBudget` — append a budget revision for one cell. */
export type SetBudgetInput = {
  accountNo: string;
  period: string;
  amount: number;
  notes?: string;
};
