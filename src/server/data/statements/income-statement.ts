// Per-company Resultatopgørelse — split out of statements.ts.
//
// Income statement for the selected calendar fiscal year with a prior-year
// comparison column. Live years are computed via `core/financial-statements`;
// archived years use the #197 archive. Money is kroner.

import { buildProfitAndLoss } from "../../../core/financial-statements";
import {
  resolveStatementContext,
  statementCompanyBlock,
} from "../shared";
import {
  archiveIncomeStatement,
  archiveYearRow,
  type IncomeStatementLine,
} from "../archive";

export type CompanyIncomeStatement = ReturnType<typeof buildCompanyIncomeStatement>;

/**
 * Resultatopgørelse — the income statement for the selected calendar fiscal
 * year: income accounts and expense accounts, each with its own amount and the
 * prior year's amount for comparison, plus the totals and the result. Every
 * figure is computed by `core/financial-statements`. Money is kroner.
 */
export function buildCompanyIncomeStatement(
  workspaceRoot: string,
  slug: string,
  year: number | null,
) {
  const ctx = resolveStatementContext(workspaceRoot, slug, year);
  try {
    const companyBlock = statementCompanyBlock(ctx.company);
    if (ctx.isArchivedOnly) {
      // Archived year — derive the resultatopgørelse from the archived
      // SaldoBalance (#197). The prior column comes from the prior year's
      // archive when one exists, so a year-over-year comparison still works.
      const archYear = parseInt(ctx.selectedLabel, 10);
      const current = archiveIncomeStatement(ctx.db, archYear);
      const prior = archiveIncomeStatement(ctx.db, archYear - 1);
      const priorIncome = new Map(prior.income.map((l) => [l.accountNo, l.amount]));
      const priorExpense = new Map(
        prior.expense.map((l) => [l.accountNo, l.amount]),
      );
      return {
        slug: ctx.entry.slug,
        selectedYear: ctx.selectedLabel,
        archived: true,
        archivedSource: archiveYearRow(ctx.db, archYear)?.sourceSystem ?? null,
        company: companyBlock,
        fiscalYears: ctx.years,
        income: current.income.map((l) => ({
          ...l,
          priorAmount: priorIncome.get(l.accountNo) ?? 0,
        })),
        expense: current.expense.map((l) => ({
          ...l,
          priorAmount: priorExpense.get(l.accountNo) ?? 0,
        })),
        totalIncome: current.totalIncome,
        totalExpense: current.totalExpense,
        priorTotalIncome: prior.totalIncome,
        priorTotalExpense: prior.totalExpense,
        result: current.result,
        priorResult: prior.result,
      };
    }

    const yearNum = parseInt(ctx.selectedLabel, 10);
    const current = buildProfitAndLoss(ctx.db, `${yearNum}-01-01`, `${yearNum}-12-31`);
    const prior = buildProfitAndLoss(
      ctx.db,
      `${yearNum - 1}-01-01`,
      `${yearNum - 1}-12-31`,
    );
    const priorIncome = new Map(prior.income.map((l) => [l.accountNo, l.amount]));
    const priorExpense = new Map(prior.expense.map((l) => [l.accountNo, l.amount]));

    const income: IncomeStatementLine[] = current.income.map((l) => ({
      accountNo: l.accountNo,
      name: l.name,
      amount: l.amount,
      priorAmount: priorIncome.get(l.accountNo) ?? 0,
    }));
    const expense: IncomeStatementLine[] = current.expense.map((l) => ({
      accountNo: l.accountNo,
      name: l.name,
      amount: l.amount,
      priorAmount: priorExpense.get(l.accountNo) ?? 0,
    }));

    return {
      slug: ctx.entry.slug,
      selectedYear: ctx.selectedLabel,
      archived: false,
      archivedSource: null as string | null,
      company: companyBlock,
      fiscalYears: ctx.years,
      income,
      expense,
      totalIncome: current.totalIncome,
      totalExpense: current.totalExpense,
      priorTotalIncome: prior.totalIncome,
      priorTotalExpense: prior.totalExpense,
      result: current.result,
      priorResult: prior.result,
    };
  } finally {
    ctx.db.close();
  }
}
