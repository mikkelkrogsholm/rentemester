import type { CompanyCashflow } from "../../lib/types";
import {
  CASHFLOW_MONTHS,
  STATEMENT_COMPANY,
  STATEMENT_FISCAL_YEARS,
} from "./_shared";

export function cashflow(
  over: Partial<CompanyCashflow> = {},
): CompanyCashflow {
  return {
    slug: "acme-aps",
    selectedYear: "2026",
    archived: false,
    company: STATEMENT_COMPANY,
    fiscalYears: STATEMENT_FISCAL_YEARS,
    periodStart: "2026-01-01",
    periodEnd: "2026-12-31",
    hasTransactions: true,
    months: CASHFLOW_MONTHS.map((label, i) => ({
      month: i + 1,
      label,
      indbetalinger: i === 1 ? 12000 : i === 4 ? 5829.02 : 0,
      udbetalinger: i === 1 ? 2000 : i === 4 ? 2594.2 : 0,
      netto: i === 1 ? 10000 : i === 4 ? 3234.82 : 0,
    })),
    balanceSeries: [
      { date: "2026-02-10", balance: 12000 },
      { date: "2026-02-20", balance: 10000 },
      { date: "2026-05-05", balance: 13234.82 },
    ],
    openingBalance: 4000,
    closingBalance: 13234.82,
    totalIn: 17829.02,
    totalOut: 4594.2,
    ...over,
  };
}
