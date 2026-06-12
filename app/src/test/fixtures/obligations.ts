import type { CompanyObligations } from "../../lib/types";
import { STATEMENT_COMPANY, STATEMENT_FISCAL_YEARS } from "./_shared";

export function obligations(
  over: Partial<CompanyObligations> = {},
): CompanyObligations {
  return {
    slug: "acme-aps",
    selectedYear: "2026",
    archived: false,
    company: STATEMENT_COMPANY,
    fiscalYears: STATEMENT_FISCAL_YEARS,
    obligations: [
      {
        kind: "vat",
        label: "Moms — Q2 2026",
        amount: 3371,
        dueDate: "2026-09-01",
        daysRemaining: 103,
        accountNo: null,
      },
      {
        kind: "annual-report",
        label: "Årsrapport — regnskabsår 2026",
        amount: 0,
        dueDate: "2027-05-01",
        daysRemaining: 344,
        accountNo: null,
      },
      {
        kind: "corporation-tax",
        label: "Skyldig selskabsskat",
        amount: 2906.66,
        dueDate: "2027-11-01",
        daysRemaining: 529,
        accountNo: "63060",
      },
      {
        kind: "creditors",
        label: "Kreditorer (leverandørgæld)",
        amount: 1250,
        dueDate: null,
        daysRemaining: null,
        accountNo: "63000",
      },
    ],
    totalOwed: 7527.66,
    ...over,
  };
}
