// Per-company Saldobalance — split out of statements.ts.
//
// Trial balance for the selected calendar fiscal year. Live years are
// computed via `core/financial-statements`; archived years use the #197
// archive's signed closing balance per account. Money is kroner.

import { buildTrialBalance } from "../../../core/financial-statements";
import {
  resolveStatementContext,
  roundKroner,
  statementCompanyBlock,
} from "../shared";
import {
  archiveTypedBalances,
  archiveYearRow,
} from "../archive";

export type TrialBalanceRow = {
  accountNo: string;
  name: string;
  type: string;
  debit: number;
  credit: number;
  balance: number;
};

export type CompanyTrialBalance = ReturnType<typeof buildCompanyTrialBalance>;

/**
 * Saldobalance — the trial balance for the selected calendar fiscal year:
 * every account that moved, with its summed debit total, credit total and the
 * signed net balance. The report is balanced when total debit equals total
 * credit. Computed by `core/financial-statements`. Money is kroner.
 */
export function buildCompanyTrialBalance(
  workspaceRoot: string,
  slug: string,
  year: number | null,
) {
  const ctx = resolveStatementContext(workspaceRoot, slug, year);
  try {
    const companyBlock = statementCompanyBlock(ctx.company);
    if (ctx.isArchivedOnly) {
      // Archived year — the archived SaldoBalance (#197) is itself the trial
      // balance. The export stores only a signed (debit − credit) closing
      // balance per account, so a positive balance reads as a debit column
      // figure and a negative one as a credit; the report is balanced when
      // every account's balance nets to zero.
      const archYear = parseInt(ctx.selectedLabel, 10);
      const header = archiveYearRow(ctx.db, archYear);
      const rows: TrialBalanceRow[] = [];
      let totalDebit = 0;
      let totalCredit = 0;
      if (header) {
        for (const b of archiveTypedBalances(ctx.db, header.id)) {
          const debit = b.amount > 0 ? b.amount : 0;
          const credit = b.amount < 0 ? roundKroner(-b.amount) : 0;
          totalDebit += debit;
          totalCredit += credit;
          rows.push({
            accountNo: b.accountNo,
            name: b.name,
            type: b.type ?? "",
            debit,
            credit,
            balance: b.amount,
          });
        }
      }
      totalDebit = roundKroner(totalDebit);
      totalCredit = roundKroner(totalCredit);
      return {
        slug: ctx.entry.slug,
        selectedYear: ctx.selectedLabel,
        archived: true,
        archivedSource: header?.sourceSystem ?? null,
        company: companyBlock,
        fiscalYears: ctx.years,
        periodStart: `${ctx.selectedLabel}-01-01`,
        periodEnd: `${ctx.selectedLabel}-12-31`,
        rows,
        totalDebit,
        totalCredit,
        balanced: Math.abs(totalDebit - totalCredit) < 0.005,
      };
    }

    const yearNum = parseInt(ctx.selectedLabel, 10);
    const tb = buildTrialBalance(ctx.db, `${yearNum}-01-01`, `${yearNum}-12-31`);
    const rows: TrialBalanceRow[] = tb.accounts.map((a) => ({
      accountNo: a.accountNo,
      name: a.name,
      type: a.type,
      debit: a.debit,
      credit: a.credit,
      balance: a.balance,
    }));

    return {
      slug: ctx.entry.slug,
      selectedYear: ctx.selectedLabel,
      archived: false,
      archivedSource: null as string | null,
      company: companyBlock,
      fiscalYears: ctx.years,
      periodStart: tb.periodStart,
      periodEnd: tb.periodEnd,
      rows,
      totalDebit: tb.totalDebit,
      totalCredit: tb.totalCredit,
      balanced: tb.balanced,
    };
  } finally {
    ctx.db.close();
  }
}
