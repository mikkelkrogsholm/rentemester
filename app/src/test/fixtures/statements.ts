import type {
  CompanyBalance,
  CompanyIncomeStatement,
  CompanyJournal,
  CompanyTrialBalance,
} from "../../lib/types";
import { STATEMENT_COMPANY, STATEMENT_FISCAL_YEARS } from "./_shared";

export function incomeStatement(
  over: Partial<CompanyIncomeStatement> = {},
): CompanyIncomeStatement {
  return {
    slug: "acme-aps",
    selectedYear: "2026",
    archived: false,
    archivedSource: null,
    company: STATEMENT_COMPANY,
    fiscalYears: STATEMENT_FISCAL_YEARS,
    income: [
      { accountNo: "1000", name: "Omsætning", amount: 17829.02, priorAmount: 0 },
    ],
    expense: [
      { accountNo: "3000", name: "Vareforbrug", amount: 4594.2, priorAmount: 0 },
    ],
    totalIncome: 17829.02,
    totalExpense: 4594.2,
    priorTotalIncome: 0,
    priorTotalExpense: 0,
    result: 13234.82,
    priorResult: 0,
    ...over,
  };
}

export function balance(over: Partial<CompanyBalance> = {}): CompanyBalance {
  return {
    slug: "acme-aps",
    selectedYear: "2026",
    archived: false,
    archivedSource: null,
    company: STATEMENT_COMPANY,
    fiscalYears: STATEMENT_FISCAL_YEARS,
    asOfDate: "2026-12-31",
    assets: {
      lines: [
        {
          accountNo: "55000",
          name: "Bank",
          amount: 41388.03,
          priorAmount: 32000,
        },
      ],
      total: 41388.03,
      priorTotal: 32000,
    },
    liabilities: {
      lines: [
        {
          accountNo: "64000",
          name: "Salgsmoms",
          amount: 3371,
          priorAmount: 2500,
        },
      ],
      total: 3371,
      priorTotal: 2500,
    },
    equity: {
      // The period result is folded in as an "Årets resultat" line, so the
      // section total is the equity accounts (24782.21) plus the result.
      lines: [
        {
          accountNo: "51000",
          name: "Selskabskapital",
          amount: 24782.21,
          priorAmount: 24782.21,
        },
        {
          accountNo: "—",
          name: "Årets resultat",
          amount: 13234.82,
          priorAmount: 4717.79,
        },
      ],
      total: 38017.03,
      priorTotal: 29500,
    },
    periodResult: 13234.82,
    totalAssets: 41388.03,
    totalLiabilitiesAndEquity: 41388.03,
    priorTotalLiabilitiesAndEquity: 32000,
    balanced: true,
    ...over,
  };
}

export function trialBalance(
  over: Partial<CompanyTrialBalance> = {},
): CompanyTrialBalance {
  return {
    slug: "acme-aps",
    selectedYear: "2026",
    archived: false,
    archivedSource: null,
    company: STATEMENT_COMPANY,
    fiscalYears: STATEMENT_FISCAL_YEARS,
    periodStart: "2026-01-01",
    periodEnd: "2026-12-31",
    rows: [
      {
        accountNo: "1000",
        name: "Omsætning",
        type: "income",
        debit: 0,
        credit: 17829.02,
        balance: -17829.02,
      },
      {
        accountNo: "55000",
        name: "Bank",
        type: "asset",
        debit: 41388.03,
        credit: 0,
        balance: 41388.03,
      },
    ],
    totalDebit: 59217.05,
    totalCredit: 59217.05,
    balanced: true,
    ...over,
  };
}

export function journal(over: Partial<CompanyJournal> = {}): CompanyJournal {
  return {
    slug: "acme-aps",
    selectedYear: "2026",
    archived: false,
    archivedSource: null,
    company: STATEMENT_COMPANY,
    fiscalYears: STATEMENT_FISCAL_YEARS,
    periodStart: "2026-01-01",
    periodEnd: "2026-12-31",
    entries: [
      {
        id: 1,
        entryNo: "B-2026-0001",
        date: "2026-03-15",
        text: "Salg af ydelse",
        total: 22286.28,
        lines: [
          {
            accountNo: "55000",
            accountName: "Bank",
            debit: 22286.28,
            credit: 0,
            text: null,
          },
          {
            accountNo: "1000",
            accountName: "Omsætning",
            debit: 0,
            credit: 17829.02,
            text: null,
          },
          {
            accountNo: "64000",
            accountName: "Salgsmoms",
            debit: 0,
            credit: 4457.26,
            text: null,
          },
        ],
        documentId: 7,
        documentNo: "BIL-2026-0001",
      },
    ],
    accountFilter: null,
    ...over,
  };
}
