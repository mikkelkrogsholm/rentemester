import type {
  CompanyBudget,
  CompanyBudgetVsActual,
} from "../../lib/types";
import {
  FY2026_PERIODS,
  STATEMENT_COMPANY,
  STATEMENT_FISCAL_YEARS,
} from "./_shared";

export function budget(over: Partial<CompanyBudget> = {}): CompanyBudget {
  return {
    slug: "acme-aps",
    selectedYear: "2026",
    archived: false,
    company: STATEMENT_COMPANY,
    fiscalYears: STATEMENT_FISCAL_YEARS,
    periodStart: "2026-01",
    periodEnd: "2026-12",
    periods: FY2026_PERIODS,
    lines: [
      {
        id: 1,
        accountNo: "2200",
        accountName: "Markedsføring",
        period: "2026-06",
        amount: 5000,
        notes: null,
        createdAt: "2026-05-01T08:00:00.000Z",
      },
      {
        id: 2,
        accountNo: "2200",
        accountName: "Markedsføring",
        period: "2026-07",
        amount: 7000,
        notes: null,
        createdAt: "2026-05-01T08:00:00.000Z",
      },
    ],
    totalBudget: 12000,
    ...over,
  };
}

export function budgetVsActual(
  over: Partial<CompanyBudgetVsActual> = {},
): CompanyBudgetVsActual {
  return {
    slug: "acme-aps",
    selectedYear: "2026",
    archived: false,
    company: STATEMENT_COMPANY,
    fiscalYears: STATEMENT_FISCAL_YEARS,
    periodStart: "2026-01",
    periodEnd: "2026-12",
    lines: [
      {
        accountNo: "2200",
        accountName: "Markedsføring",
        accountType: "expense",
        period: "2026-06",
        budget: 5000,
        actual: 4000,
        variance: 1000,
        variancePercent: 0.2,
      },
      {
        accountNo: "2200",
        accountName: "Markedsføring",
        accountType: "expense",
        period: "2026-07",
        budget: 7000,
        actual: 8500,
        variance: -1500,
        variancePercent: -1500 / 7000,
      },
    ],
    totalBudget: 12000,
    totalActual: 12500,
    totalVariance: 500,
    ...over,
  };
}
