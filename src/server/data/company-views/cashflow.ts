import {
  resolveStatementContext,
  roundKroner,
  statementCompanyBlock,
  MONTH_NAMES_DK,
} from "../shared";
import { actualBankBalanceAsOf } from "../bank";

// --------------------------------------------------------------------------
// Per-company cash flow (Likviditet — actual money in/out, year-aware)
// — cockpit-redesign Runde 2, iteration 8
// --------------------------------------------------------------------------

/** One calendar month of actual money movement on the bank statement. */
export type CashflowMonth = {
  /** 1–12. */
  month: number;
  label: string;
  /** Sum of positive `bank_transactions.amount` in the month, kroner. */
  indbetalinger: number;
  /** Sum of negative amounts as a positive figure (money out), kroner. */
  udbetalinger: number;
  /** indbetalinger − udbetalinger; the net movement, kroner. */
  netto: number;
};

/** One dated point on the real bank-balance trajectory. */
export type CashflowBalancePoint = {
  date: string;
  /** The imported `balance_after` at this point, kroner. */
  balance: number;
};

export type CompanyCashflow = ReturnType<typeof buildCompanyCashflow>;

/**
 * Likviditet / pengestrøm — actual money in and out of the bank for the
 * selected calendar fiscal year, computed straight from the imported
 * `bank_transactions` (NOT the accrual ledger). This is what the owner's bank
 * app shows: real cash, not booked revenue.
 *
 *  - `months` — per-month indbetalinger (positive amounts) and udbetalinger
 *    (negative amounts, shown as a positive figure), all twelve months.
 *  - `balanceSeries` — the `balance_after` trajectory: every transaction in the
 *    year that carries a running balance, oldest-first; the real bank-balance
 *    line.
 *  - summary — opening balance (the actual balance the day before the year
 *    starts), total in, total out and closing balance for the year.
 *
 * `hasTransactions` is false when the company has no bank rows at all in the
 * year — the UI renders a clean empty state. Money is kroner. Throws
 * `ApiError.notFound` when the slug is not registered or has no ledger.
 */
export function buildCompanyCashflow(
  workspaceRoot: string,
  slug: string,
  year: number | null,
) {
  const ctx = resolveStatementContext(workspaceRoot, slug, year);
  try {
    const companyBlock = statementCompanyBlock(ctx.company);
    const yearNum = parseInt(ctx.selectedLabel, 10);
    const yearStart = `${yearNum}-01-01`;
    const yearEnd = `${yearNum}-12-31`;
    const priorYearEnd = `${yearNum - 1}-12-31`;

    const emptyMonths = (): CashflowMonth[] =>
      MONTH_NAMES_DK.map((label, i) => ({
        month: i + 1,
        label,
        indbetalinger: 0,
        udbetalinger: 0,
        netto: 0,
      }));

    if (ctx.isArchivedOnly) {
      return {
        slug: ctx.entry.slug,
        selectedYear: ctx.selectedLabel,
        archived: true,
        company: companyBlock,
        fiscalYears: ctx.years,
        periodStart: yearStart,
        periodEnd: yearEnd,
        hasTransactions: false,
        months: emptyMonths(),
        balanceSeries: [] as CashflowBalancePoint[],
        openingBalance: null as number | null,
        closingBalance: null as number | null,
        totalIn: 0,
        totalOut: 0,
      };
    }

    // Every bank transaction in the year, oldest-first — drives both the
    // monthly in/out totals and the balance trajectory.
    const rows = ctx.db
      .query(
        `SELECT bt.transaction_date AS date,
                bt.amount           AS amount,
                bt.balance_after    AS balanceAfter
           FROM bank_transactions bt
          WHERE bt.transaction_date >= ? AND bt.transaction_date <= ?
          ORDER BY bt.transaction_date ASC, bt.id ASC`,
      )
      .all(yearStart, yearEnd) as Array<{
      date: string;
      amount: number;
      balanceAfter: number | null;
    }>;

    const months = emptyMonths();
    let totalIn = 0;
    let totalOut = 0;
    const balanceSeries: CashflowBalancePoint[] = [];
    for (const r of rows) {
      const month = parseInt(r.date.slice(5, 7), 10);
      const slot = months[month - 1];
      const amount = Number(r.amount ?? 0);
      if (slot) {
        if (amount >= 0) slot.indbetalinger += amount;
        else slot.udbetalinger += -amount;
      }
      if (amount >= 0) totalIn += amount;
      else totalOut += -amount;
      if (r.balanceAfter !== null && r.balanceAfter !== undefined) {
        balanceSeries.push({
          date: r.date,
          balance: roundKroner(Number(r.balanceAfter)),
        });
      }
    }
    for (const m of months) {
      m.indbetalinger = roundKroner(m.indbetalinger);
      m.udbetalinger = roundKroner(m.udbetalinger);
      m.netto = roundKroner(m.indbetalinger - m.udbetalinger);
    }

    // Opening balance — the actual statement balance the day before the year
    // begins; closing balance — the actual balance at the year end. Both come
    // from the same `balance_after`-based helper the bank view uses, so they
    // are null when no statement carries a running balance.
    const openingBalance = actualBankBalanceAsOf(ctx.db, priorYearEnd);
    const closingBalance = actualBankBalanceAsOf(ctx.db, yearEnd);

    return {
      slug: ctx.entry.slug,
      selectedYear: ctx.selectedLabel,
      archived: false,
      company: companyBlock,
      fiscalYears: ctx.years,
      periodStart: yearStart,
      periodEnd: yearEnd,
      hasTransactions: rows.length > 0,
      months,
      balanceSeries,
      openingBalance,
      closingBalance,
      totalIn: roundKroner(totalIn),
      totalOut: roundKroner(totalOut),
    };
  } finally {
    ctx.db.close();
  }
}
