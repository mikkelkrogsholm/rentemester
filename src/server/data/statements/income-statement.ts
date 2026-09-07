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
import { dataCoverage } from "../../../data-coverage";

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
      const hasPrior = ctx.years.some((entry) => entry.label === String(archYear - 1));
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
          priorAmount: hasPrior ? (priorIncome.get(l.accountNo) ?? 0) : null,
        })),
        expense: current.expense.map((l) => ({
          ...l,
          priorAmount: hasPrior ? (priorExpense.get(l.accountNo) ?? 0) : null,
        })),
        totalIncome: current.totalIncome,
        totalExpense: current.totalExpense,
        priorTotalIncome: hasPrior ? prior.totalIncome : null,
        priorTotalExpense: hasPrior ? prior.totalExpense : null,
        result: current.result,
        priorResult: hasPrior ? prior.result : null,
        coverage: dataCoverage("final", `${ctx.selectedLabel}-12-31`, hasPrior ? "available" : "not_comparable", "archived", ["Læsbar arkivsaldo; perioden er endelig/låst."]),
      };
    }

    const yearNum = parseInt(ctx.selectedLabel, 10);
    const current = buildProfitAndLoss(ctx.db, `${yearNum}-01-01`, `${yearNum}-12-31`);
    const prior = buildProfitAndLoss(
      ctx.db,
      `${yearNum - 1}-01-01`,
      `${yearNum - 1}-12-31`,
    );
    const hasPrior = ctx.years.some((entry) => entry.label === String(yearNum - 1));
    const latest = ctx.db.query(`SELECT MAX(transaction_date) AS date FROM journal_entries WHERE status = 'posted' AND transaction_date >= ? AND transaction_date <= ?`).get(`${yearNum}-01-01`, `${yearNum}-12-31`) as { date: string | null };
    const priorIncome = new Map(prior.income.map((l) => [l.accountNo, l.amount]));
    const priorExpense = new Map(prior.expense.map((l) => [l.accountNo, l.amount]));

    const income: IncomeStatementLine[] = current.income.map((l) => ({
      accountNo: l.accountNo,
      name: l.name,
      amount: l.amount,
      priorAmount: hasPrior ? (priorIncome.get(l.accountNo) ?? 0) : null,
    }));
    const expense: IncomeStatementLine[] = current.expense.map((l) => ({
      accountNo: l.accountNo,
      name: l.name,
      amount: l.amount,
      priorAmount: hasPrior ? (priorExpense.get(l.accountNo) ?? 0) : null,
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
      priorTotalIncome: hasPrior ? prior.totalIncome : null,
      priorTotalExpense: hasPrior ? prior.totalExpense : null,
      result: current.result,
      priorResult: hasPrior ? prior.result : null,
      coverage: dataCoverage("current", latest.date, hasPrior ? "available" : "not_comparable", "native", ["Seneste bogførte journalpost i valgt regnskabsår."]),
    };
  } finally {
    ctx.db.close();
  }
}
