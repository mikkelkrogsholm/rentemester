// Per-company financial-statement views for the cockpit (#320).
//
// Split out of `server/data.ts` by #320. The year-aware Overblik dashboard and
// the four core statement views — Resultatopgørelse, Balance, Saldobalance and
// the Flerårsoversigt — each computed from the posted ledger via
// `core/financial-statements`, or from the #197 archive for an archived year.
//
// Archive classification uses the shared `classifyAccountSection` (#321), the
// same rule the live balance sheet applies, so the archive-aware views never
// disagree with the live ones. Behaviour is unchanged from the pre-split
// `server/data.ts`. Money is kroner throughout.

import type { Database } from "bun:sqlite";
import { existsSync } from "node:fs";
import { companyPaths } from "../../core/paths";
import { diffDaysSafe as daysBetween } from "../../core/dates";
import { openDb, migrate } from "../../core/db";
import { getCompanySettings } from "../../core/company";
import { listExceptions } from "../../core/exceptions";
import { buildInvoiceList } from "../../core/invoice-list";
import {
  buildBalanceSheet,
  buildProfitAndLoss,
  buildTrialBalance,
} from "../../core/financial-statements";
import { classifyAccountSection } from "../../core/account-classification";
import {
  companyRootForSlug,
  findWorkspaceCompany,
} from "../../core/workspace";
import { ApiError } from "../errors";
import {
  buildCompanyFiscalYears,
  currentFiscalYear,
  MONTH_NAMES_DK,
  resolveStatementContext,
  roundKroner,
  statementCompanyBlock,
  todayIsoDate,
  type FiscalYearEntry,
} from "./shared";
import { bankBalanceAsOf, actualBankBalanceAsOf } from "./bank";
import { selectVatPeriod } from "./vat";
import { groupExceptions, type ExceptionGroup } from "./exceptions";
import {
  archiveIncomeStatement,
  archiveTypedBalances,
  archiveYearRow,
  type IncomeStatementLine,
} from "./archive";

export type { IncomeStatementLine } from "./archive";

export type OverviewMonth = {
  /** 1–12. */
  month: number;
  label: string;
  income: number;
  expense: number;
};

/** The Overblik VAT block — null for an archived year (no VAT data exists). */
export type OverviewVat = {
  periodStart: string;
  periodEnd: string;
  periodLabel: string;
  /** Genuine output VAT on sales — gross, before any bad-debt relief. */
  outputVat: number;
  /** Bad-debt (debitortab) output-VAT adjustment, ≤ 0; 0 when none. */
  outputVatAdjustment: number;
  inputVat: number;
  payable: number;
  deadline: string;
  daysRemaining: number;
};

type ExceptionPreview = {
  id: number;
  type: string;
  severity: string;
  message: string;
  /** The concrete action the owner must take; null when none is recorded. */
  requiredAction: string | null;
};

type RecentEntry = {
  id: number;
  entryNo: string;
  date: string;
  text: string;
  amount: number;
};

export type CompanyOverview = ReturnType<typeof buildCompanyOverview>;

/**
 * Per-company "Overblik" — the year-aware company dashboard the cockpit SPA's
 * P0 view renders. Every figure is computed from posted ledger postings: the
 * P&L from `core/financial-statements`, the VAT position from the booked VAT
 * accounts, the bank balance from the cash asset accounts. Money is kroner.
 *
 * `year` selects the calendar fiscal year; when omitted the company's most
 * recent live year is used. An archived-only year returns `archived: true`
 * with empty figures — the live ledger has nothing for it (#197 archive data
 * is surfaced in a later iteration).
 *
 * Throws `ApiError.notFound` when the slug is not registered or has no ledger.
 */
export function buildCompanyOverview(
  workspaceRoot: string,
  slug: string,
  year: number | null,
) {
  const entry = findWorkspaceCompany(workspaceRoot, slug);
  if (!entry) {
    throw ApiError.notFound(`ingen virksomhed med slug '${slug}' findes i workspacet`);
  }
  const companyRoot = companyRootForSlug(workspaceRoot, slug);
  const dbPath = companyPaths(companyRoot).db;
  if (!existsSync(dbPath)) {
    throw ApiError.notFound(`virksomheden '${slug}' har ingen ledger`);
  }

  const fiscalYears = buildCompanyFiscalYears(workspaceRoot, slug);
  const years = fiscalYears.years;
  // Default to the most recent live year, falling back to the newest year.
  const liveYears = years.filter((y) => y.source === "live");
  const defaultYear =
    liveYears[0]?.label ?? years[0]?.label ?? String(new Date().getUTCFullYear());
  const selectedLabel = year !== null ? String(year) : defaultYear;
  const selected = years.find((y) => y.label === selectedLabel);
  const isArchivedOnly = selected ? selected.source === "archive" : false;

  const db = openDb(dbPath);
  try {
    migrate(db);
    const company = getCompanySettings(db);

    const companyBlock = {
      name: company.name,
      cvr: company.cvr,
      country: company.country,
      currency: company.currency,
      fiscalYearStartMonth: company.fiscalYearStartMonth,
      fiscalYearLabelStrategy: company.fiscalYearLabelStrategy,
    };

    // An archived-only year has no live ledger — but the #197 archive holds
    // the full SaldoBalance + Posteringer, enough for a P&L-oriented overview.
    // The figures are derived from the archive; the live-only sections (bank
    // reconciliation, exception queue, VAT deadline) are surfaced as N/A
    // rather than faked — there is no archived data for them.
    if (isArchivedOnly) {
      const archYear = parseInt(selectedLabel, 10);
      const header = archiveYearRow(db, archYear);
      const pl = archiveIncomeStatement(db, archYear);

      // Monthly income/expense buckets — every archived posting line joined to
      // its account type and bucketed by its `transaction_date` month. Income
      // accounts are credit-normal (a negative archive amount is income);
      // expense accounts are debit-normal (a positive amount is an expense).
      const months: OverviewMonth[] = [];
      const monthIncome = new Array(12).fill(0) as number[];
      const monthExpense = new Array(12).fill(0) as number[];
      const recentEntries: RecentEntry[] = [];
      let lastPostedDate: string | null = null;
      if (header) {
        const postingRows = db
          .query(
            `SELECT p.line_no          AS lineNo,
                    p.account_no       AS accountNo,
                    p.account_name     AS accountName,
                    p.transaction_date AS date,
                    p.voucher          AS voucher,
                    p.text             AS text,
                    p.amount           AS amount,
                    a.type             AS type
               FROM import_archive_postings p
               LEFT JOIN accounts a ON a.account_no = p.account_no
              WHERE p.archive_year_id = ?`,
          )
          .all(header.id) as Array<{
          lineNo: number;
          accountNo: string;
          accountName: string | null;
          date: string | null;
          voucher: string | null;
          text: string | null;
          amount: number;
          type: string | null;
        }>;
        for (const r of postingRows) {
          if (!r.date) continue;
          const m = parseInt(r.date.slice(5, 7), 10);
          if (!(m >= 1 && m <= 12)) continue;
          const amount = Number(r.amount ?? 0);
          if (r.type === "income") monthIncome[m - 1]! += -amount;
          else if (r.type === "expense") monthExpense[m - 1]! += amount;
        }
        // The most recent archived postings — newest date first, capped at 8.
        const dated = postingRows
          .filter((r) => r.date)
          .sort((a, b) =>
            a.date! !== b.date!
              ? b.date!.localeCompare(a.date!)
              : b.lineNo - a.lineNo,
          );
        lastPostedDate = dated[0]?.date ?? null;
        for (const r of dated.slice(0, 8)) {
          recentEntries.push({
            id: r.lineNo,
            entryNo: r.voucher ?? "",
            date: r.date!,
            text: r.text && r.text.length > 0 ? r.text : (r.accountName ?? ""),
            amount: roundKroner(Number(r.amount ?? 0)),
          });
        }
      }
      for (let m = 1; m <= 12; m += 1) {
        months.push({
          month: m,
          label: MONTH_NAMES_DK[m - 1]!,
          income: roundKroner(monthIncome[m - 1]!),
          expense: roundKroner(monthExpense[m - 1]!),
        });
      }

      const bruttomargin =
        pl.totalIncome !== 0 ? pl.result / pl.totalIncome : null;

      return {
        slug: entry.slug,
        selectedYear: selectedLabel,
        archived: true,
        archivedSource: header?.sourceSystem ?? null,
        company: companyBlock,
        fiscalYears: years,
        profitAndLoss: {
          omsaetning: pl.totalIncome,
          udgifter: pl.totalExpense,
          resultat: pl.result,
          months,
        },
        // Live-only sections — no archived data exists, so N/A rather than 0.
        bank: { balance: 0, actualBalance: null, difference: null },
        receivables: { openCount: 0, openTotal: 0 },
        vat: null,
        exceptions: {
          count: 0,
          rows: [] as ExceptionPreview[],
          groups: [] as ExceptionGroup[],
        },
        recentEntries,
        lastPostedDate,
        keyFigures: { bruttomargin, egenkapitalandel: null },
      };
    }

    const yearNum = parseInt(selectedLabel, 10);
    const yearStart = `${yearNum}-01-01`;
    const yearEnd = `${yearNum}-12-31`;

    // P&L for the full year, reusing the core financial statement.
    const pl = buildProfitAndLoss(db, yearStart, yearEnd);

    // Monthly breakdown — one income/expense pair per calendar month.
    const months: OverviewMonth[] = [];
    for (let m = 1; m <= 12; m += 1) {
      const mm = String(m).padStart(2, "0");
      const last = new Date(Date.UTC(yearNum, m, 0)).getUTCDate();
      const mPl = buildProfitAndLoss(
        db,
        `${yearNum}-${mm}-01`,
        `${yearNum}-${mm}-${String(last).padStart(2, "0")}`,
      );
      months.push({
        month: m,
        label: MONTH_NAMES_DK[m - 1]!,
        income: mPl.totalIncome,
        expense: mPl.totalExpense,
      });
    }

    // VAT position: each VAT period settles separately. Surface the period
    // (month / quarter / half-year, per the company's `vatPeriodType`) that is
    // due now, so the cockpit agrees with the static dashboard and CLI (#299).
    const vatSelection = selectVatPeriod(db, yearNum, company.vatPeriodType);
    const vat = vatSelection.position;

    // The exception queue — grouped by type into one Danish summary line each,
    // so the "Opgaver" card reads "362 banktransaktioner mangler afstemning"
    // rather than 362 individual English exception messages.
    const exceptions = listExceptions(db, { status: "open" });
    const exceptionRows: ExceptionPreview[] = exceptions.rows
      .slice(0, 6)
      .map((row: any) => ({
        id: row.id,
        type: row.type,
        severity: row.severity,
        message: row.message,
        // The concrete "what the owner must do" guidance — the most useful
        // part of an exception. The CLI's `exceptions list` shows it; the
        // cockpit must too (#254). Null when the exception has none.
        requiredAction: row.requiredAction ?? null,
      }));
    const exceptionGroups = groupExceptions(
      exceptions.rows.map((row: any) => ({
        type: row.type,
        severity: row.severity,
      })),
    );

    // The most recent posted journal entries within the selected year.
    const entryRows = db
      .query(
        `SELECT je.id          AS id,
                je.entry_no    AS entryNo,
                je.transaction_date AS date,
                je.text        AS text,
                (SELECT COALESCE(SUM(debit_amount), 0)
                   FROM journal_lines WHERE journal_entry_id = je.id) AS amount
           FROM journal_entries je
          WHERE je.status = 'posted'
            AND je.transaction_date >= ? AND je.transaction_date <= ?
          ORDER BY je.transaction_date DESC, je.id DESC
          LIMIT 8`,
      )
      .all(yearStart, yearEnd) as Array<{
      id: number;
      entryNo: string;
      date: string;
      text: string;
      amount: number;
    }>;
    const recentEntries: RecentEntry[] = entryRows.map((r) => ({
      id: r.id,
      entryNo: r.entryNo,
      date: r.date,
      text: r.text,
      amount: Math.round(Number(r.amount ?? 0) * 100) / 100,
    }));

    // Bank: the booked ledger balance plus the actual statement balance (the
    // latest imported `balance_after`) and the gap between them. The owner
    // needs the actual figure — the booked one alone is misleading when the
    // import is not yet reconciled.
    const bookedBalance = bankBalanceAsOf(db, yearEnd);
    const actualBalance = actualBankBalanceAsOf(db, yearEnd);
    const bankDifference =
      actualBalance === null ? null : roundKroner(bookedBalance - actualBalance);

    // Receivables (debitorer): money owed TO the company — the still-open
    // balance of issued sales invoices as of the year end. `buildInvoiceList`
    // derives each invoice's open balance via `core/invoice-payments`; for
    // Helheim (0 issued invoices) this is a clean 0.
    const openInvoices = buildInvoiceList(db, {
      status: "open",
      asOfDate: yearEnd,
    });
    const receivables = {
      openCount: openInvoices.count,
      openTotal: roundKroner(
        openInvoices.rows.reduce((acc, r) => acc + r.openBalance, 0),
      ),
    };

    // The transaction date of the most recent posted journal entry in the
    // year — surfaced as "Senest bogført pr. <dato>" so the owner sees at a
    // glance how current the figures are. Null when nothing is posted yet.
    const lastPostedDate = recentEntries.length > 0 ? recentEntries[0]!.date : null;

    // Nøgletal: the two ratios an owner reads off a glance — bruttomargin
    // (resultat ÷ omsætning) and egenkapitalandel (egenkapital ÷ balancesum).
    // Both are fractions (0–1); the SPA renders them as percentages. Each is
    // null when its denominator is zero — no figure is invented.
    const bs = buildBalanceSheet(db, yearEnd);
    const equityTotal = roundKroner(bs.equity.total + bs.periodResult);
    const bruttomargin =
      pl.totalIncome !== 0 ? pl.result / pl.totalIncome : null;
    const egenkapitalandel =
      bs.totalAssets !== 0 ? equityTotal / bs.totalAssets : null;

    const vatDeadline = vatSelection.deadline;
    const vatBlock: OverviewVat = {
      periodStart: vat.periodStart,
      periodEnd: vat.periodEnd,
      periodLabel: vatSelection.label,
      outputVat: vat.outputVat,
      outputVatAdjustment: vat.outputVatAdjustment,
      inputVat: vat.inputVat,
      payable: vat.payable,
      deadline: vatDeadline,
      daysRemaining: daysBetween(todayIsoDate(), vatDeadline),
    };

    return {
      slug: entry.slug,
      selectedYear: selectedLabel,
      archived: false,
      archivedSource: null as string | null,
      company: companyBlock,
      fiscalYears: years,
      profitAndLoss: {
        omsaetning: pl.totalIncome,
        udgifter: pl.totalExpense,
        resultat: pl.result,
        months,
      },
      bank: {
        balance: bookedBalance,
        actualBalance,
        difference: bankDifference,
      },
      receivables,
      vat: vatBlock as OverviewVat | null,
      exceptions: {
        count: exceptions.count,
        rows: exceptionRows,
        groups: exceptionGroups,
      },
      recentEntries,
      lastPostedDate,
      keyFigures: { bruttomargin, egenkapitalandel },
    };
  } finally {
    db.close();
  }
}

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

export type BalanceLine = {
  accountNo: string;
  name: string;
  amount: number;
  /**
   * The same account's balance one fiscal year earlier, kroner. `null` when
   * there is no prior year in the ledger (live or archived) — the view then
   * renders «—» rather than a misleading 0.
   */
  priorAmount: number | null;
};

export type BalanceSection = {
  lines: BalanceLine[];
  total: number;
  /**
   * The section total one fiscal year earlier, kroner. `null` when there is no
   * prior year in the ledger — the view then renders «—».
   */
  priorTotal: number | null;
};

export type CompanyBalance = ReturnType<typeof buildCompanyBalance>;

/**
 * Balance — the balance sheet as of the selected fiscal year's end date:
 * assets, liabilities and equity sections with section totals. The fiscal
 * year's result is folded into the equity section as an "Årets resultat" line,
 * so `equity.total` is the equity figure an owner reads (equity accounts plus
 * the result) and the sheet balances as assets = liabilities + equity. That
 * holds for live years (computed by `core/financial-statements`) and archived
 * years (#197) alike, and keeps `equity.total` equal to the Flerårsoversigt's
 * `egenkapital` for the same year. Money is kroner.
 */
/**
 * Internal — the raw balance-sheet figures for one fiscal year, before they
 * are merged onto a `BalanceLine[]` with prior-year comparison amounts.
 *
 * Returned by the per-year computers below for the selected year and (when
 * one exists in the ledger or the #197 archive) the year before, so the
 * outer builder can decorate every current line and section total with its
 * prior amount.
 */
type RawBalance = {
  /** Plain (no priorAmount) lines per section, with the period-result line
   *  already folded into equity. Both archived and live years use this shape. */
  assets: { accountNo: string; name: string; amount: number }[];
  liabilities: { accountNo: string; name: string; amount: number }[];
  equity: { accountNo: string; name: string; amount: number }[];
  totalAssets: number;
  totalLiabilities: number;
  totalEquity: number;
  totalLiabilitiesAndEquity: number;
  periodResult: number;
  balanced: boolean;
  asOfDate: string;
  /** Where the figures come from — exposed only so the outer builder can label
   *  the response with `archived` and `archivedSource`. */
  source: "live" | "archive";
  archivedSource: string | null;
};

/**
 * Compute the raw balance-sheet figures for one fiscal year from the #197
 * archived SaldoBalance. The period result is folded into the equity section
 * as an "Årets resultat" line, so the sheet balances as assets = liabilities
 * + equity — identical to the live balance sheet's behaviour.
 */
function archivedBalanceForYear(
  db: Database,
  yearLabel: string,
): RawBalance | null {
  const archYear = parseInt(yearLabel, 10);
  const header = archiveYearRow(db, archYear);
  if (!header) return null;
  const assets: { accountNo: string; name: string; amount: number }[] = [];
  const liabilities: { accountNo: string; name: string; amount: number }[] = [];
  const equity: { accountNo: string; name: string; amount: number }[] = [];
  let totalAssets = 0;
  let totalLiabilities = 0;
  let equitySection = 0;
  let periodResult = 0;
  for (const b of archiveTypedBalances(db, header.id)) {
    const section = classifyAccountSection(b.type, b.normalBalance);
    if (section === "asset") {
      assets.push({ accountNo: b.accountNo, name: b.name, amount: b.amount });
      totalAssets += b.amount;
    } else if (section === "liability") {
      const amount = roundKroner(-b.amount);
      liabilities.push({ accountNo: b.accountNo, name: b.name, amount });
      totalLiabilities += amount;
    } else if (section === "equity") {
      const amount = roundKroner(-b.amount);
      equity.push({ accountNo: b.accountNo, name: b.name, amount });
      equitySection += amount;
    } else if (section === "income") {
      periodResult += -b.amount;
    } else if (section === "expense") {
      periodResult -= b.amount;
    }
  }
  totalAssets = roundKroner(totalAssets);
  totalLiabilities = roundKroner(totalLiabilities);
  equitySection = roundKroner(equitySection);
  periodResult = roundKroner(periodResult);
  equity.push({ accountNo: "—", name: "Årets resultat", amount: periodResult });
  const totalEquity = roundKroner(equitySection + periodResult);
  const totalLiabilitiesAndEquity = roundKroner(totalLiabilities + totalEquity);
  return {
    assets,
    liabilities,
    equity,
    totalAssets,
    totalLiabilities,
    totalEquity,
    totalLiabilitiesAndEquity,
    periodResult,
    balanced: Math.abs(totalAssets - totalLiabilitiesAndEquity) < 0.005,
    asOfDate: `${yearLabel}-12-31`,
    source: "archive",
    archivedSource: header.sourceSystem ?? null,
  };
}

/**
 * Compute the raw balance-sheet figures for one fiscal year from the live
 * ledger via `core/financial-statements`. The period result is folded into
 * the equity section so the sheet balances as assets = liabilities + equity.
 */
function liveBalanceForYear(db: Database, yearLabel: string): RawBalance {
  const yearNum = parseInt(yearLabel, 10);
  const asOfDate = `${yearNum}-12-31`;
  const bs = buildBalanceSheet(db, asOfDate);
  const toLines = (lines: { accountNo: string; name: string; amount: number }[]) =>
    lines.map((l) => ({ accountNo: l.accountNo, name: l.name, amount: l.amount }));
  const equityLines = toLines(bs.equity.lines);
  equityLines.push({
    accountNo: "—",
    name: "Årets resultat",
    amount: bs.periodResult,
  });
  const totalEquity = roundKroner(bs.equity.total + bs.periodResult);
  return {
    assets: toLines(bs.assets.lines),
    liabilities: toLines(bs.liabilities.lines),
    equity: equityLines,
    totalAssets: bs.totalAssets,
    totalLiabilities: bs.liabilities.total,
    totalEquity,
    totalLiabilitiesAndEquity: bs.totalLiabilitiesAndEquity,
    periodResult: bs.periodResult,
    balanced: bs.balanced,
    asOfDate: bs.asOfDate,
    source: "live",
    archivedSource: null,
  };
}

/**
 * Compute the raw balance for `yearLabel`, regardless of whether the year is
 * live or archived. Returns `null` when the year is not present in `years`
 * (live or archive) — used to drive the «—» prior-year column.
 */
function rawBalanceForYear(
  db: Database,
  yearLabel: string,
  years: FiscalYearEntry[],
): RawBalance | null {
  const entry = years.find((y) => y.label === yearLabel);
  if (!entry) return null;
  return entry.source === "archive"
    ? archivedBalanceForYear(db, yearLabel)
    : liveBalanceForYear(db, yearLabel);
}

/**
 * Decorate a current-year section's lines with the matching prior-year amount
 * — looked up by `accountNo`, falling back to 0 when the same account did
 * have postings in the prior year but no current movement is on the same
 * row. The synthetic "Årets resultat" line (`accountNo === "—"`) pairs with
 * the prior year's period result, not by accountNo.
 */
function applyPriorToSection(
  currentLines: { accountNo: string; name: string; amount: number }[],
  priorLines: { accountNo: string; name: string; amount: number }[] | null,
  priorPeriodResult: number | null,
): BalanceLine[] {
  const priorByAccount = new Map<string, number>();
  if (priorLines) {
    for (const l of priorLines) {
      if (l.accountNo !== "—") priorByAccount.set(l.accountNo, l.amount);
    }
  }
  return currentLines.map((l) => ({
    accountNo: l.accountNo,
    name: l.name,
    amount: l.amount,
    priorAmount:
      priorLines === null
        ? null
        : l.accountNo === "—"
          ? (priorPeriodResult ?? 0)
          : (priorByAccount.get(l.accountNo) ?? 0),
  }));
}

export function buildCompanyBalance(
  workspaceRoot: string,
  slug: string,
  year: number | null,
) {
  const ctx = resolveStatementContext(workspaceRoot, slug, year);
  try {
    const companyBlock = statementCompanyBlock(ctx.company);
    const current: RawBalance = ctx.isArchivedOnly
      ? (archivedBalanceForYear(ctx.db, ctx.selectedLabel) ?? {
          assets: [],
          liabilities: [],
          equity: [
            { accountNo: "—", name: "Årets resultat", amount: 0 },
          ],
          totalAssets: 0,
          totalLiabilities: 0,
          totalEquity: 0,
          totalLiabilitiesAndEquity: 0,
          periodResult: 0,
          balanced: true,
          asOfDate: `${ctx.selectedLabel}-12-31`,
          source: "archive",
          archivedSource: null,
        })
      : liveBalanceForYear(ctx.db, ctx.selectedLabel);

    // Prior-year lookup — the year directly preceding `selectedLabel` if it
    // exists in the ledger (live or archive). When it does not, the prior
    // column is uniformly `null` and the view renders «—». ÅRL § 24 requires
    // sammenligningstal on the balance sheet, so this is no cosmetic detail.
    const priorLabel = String(parseInt(ctx.selectedLabel, 10) - 1);
    const prior = rawBalanceForYear(ctx.db, priorLabel, ctx.years);
    const priorPresent = prior !== null;

    const assetsLines = applyPriorToSection(
      current.assets,
      prior ? prior.assets : null,
      prior ? prior.periodResult : null,
    );
    const liabilitiesLines = applyPriorToSection(
      current.liabilities,
      prior ? prior.liabilities : null,
      prior ? prior.periodResult : null,
    );
    const equityLines = applyPriorToSection(
      current.equity,
      prior ? prior.equity : null,
      prior ? prior.periodResult : null,
    );

    return {
      slug: ctx.entry.slug,
      selectedYear: ctx.selectedLabel,
      archived: ctx.isArchivedOnly,
      archivedSource: ctx.isArchivedOnly ? current.archivedSource : null,
      company: companyBlock,
      fiscalYears: ctx.years,
      asOfDate: current.asOfDate,
      assets: {
        lines: assetsLines,
        total: current.totalAssets,
        priorTotal: priorPresent ? (prior?.totalAssets ?? 0) : null,
      },
      liabilities: {
        lines: liabilitiesLines,
        total: current.totalLiabilities,
        priorTotal: priorPresent ? (prior?.totalLiabilities ?? 0) : null,
      },
      equity: {
        lines: equityLines,
        total: current.totalEquity,
        priorTotal: priorPresent ? (prior?.totalEquity ?? 0) : null,
      },
      periodResult: current.periodResult,
      totalAssets: current.totalAssets,
      totalLiabilitiesAndEquity: current.totalLiabilitiesAndEquity,
      /**
       * The prior year's `totalLiabilities + totalEquity` — `null` when no
       * prior year is in the ledger. Matches `totalLiabilitiesAndEquity` for
       * the current year.
       */
      priorTotalLiabilitiesAndEquity: priorPresent
        ? (prior?.totalLiabilitiesAndEquity ?? 0)
        : null,
      balanced: current.balanced,
    };
  } finally {
    ctx.db.close();
  }
}

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

// --------------------------------------------------------------------------
// Per-company multi-year key figures (Flerårsoversigt) — cockpit-redesign it. 4
// --------------------------------------------------------------------------

/** Key figures for one fiscal year in the multi-year comparison. */
export type MultiYearRow = {
  /** The fiscal-year label, e.g. "2025". */
  year: string;
  /** Where the figures come from: the live ledger or the #197 archive. */
  source: "live" | "archive";
  /** Income / omsætning for the year, kroner. */
  omsaetning: number;
  /** Expenses / udgifter for the year, kroner. */
  udgifter: number;
  /** Result (omsætning − udgifter), kroner. */
  resultat: number;
  /** Total assets (balancesum) at the year end, kroner. */
  balancesum: number;
  /** Equity (egenkapital incl. period result) at the year end, kroner. */
  egenkapital: number;
  /**
   * Bruttomargin — resultat ÷ omsætning, a 0–1 fraction. Null when there is no
   * omsætning to divide by; no figure is invented.
   */
  bruttomargin: number | null;
  /**
   * Egenkapitalandel — egenkapital ÷ balancesum, a 0–1 fraction. Null when the
   * balance sum is zero.
   */
  egenkapitalandel: number | null;
};

export type CompanyMultiYear = ReturnType<typeof buildCompanyMultiYear>;

/**
 * Flerårsoversigt — key figures for every fiscal year available for a company,
 * oldest→newest so a trend can be charted: the P&L (omsætning / udgifter /
 * resultat), the balance-sheet development (balancesum / egenkapital) and the
 * two ratios an owner reads off a glance (bruttomargin, egenkapitalandel).
 *
 * The live year(s) are computed from the posted ledger via
 * `core/financial-statements` — exactly as `/income-statement` and `/balance`
 * do. The archived years (#197) are derived from `import_archive_balances`:
 * each archived account's closing balance is classified by joining its account
 * number to the live `accounts` table's `type` (and `normal_balance`), via the
 * shared #321 classification — the same rule the Balance view applies, so the
 * two views never disagree. Income accounts are credit-normal, so the archive's
 * signed balance is negated to read as a positive omsætning; expense accounts
 * read positive as-is. Assets are debit-normal (read as-is); equity is
 * credit-normal (negated) and carries the un-closed period result so it matches
 * the archive-aware Balance view.
 *
 * Throws `ApiError.notFound` when the slug is not registered or has no ledger.
 */
export function buildCompanyMultiYear(workspaceRoot: string, slug: string) {
  const entry = findWorkspaceCompany(workspaceRoot, slug);
  if (!entry) {
    throw ApiError.notFound(`ingen virksomhed med slug '${slug}' findes i workspacet`);
  }
  const companyRoot = companyRootForSlug(workspaceRoot, slug);
  const dbPath = companyPaths(companyRoot).db;
  if (!existsSync(dbPath)) {
    throw ApiError.notFound(`virksomheden '${slug}' har ingen ledger`);
  }

  const years = buildCompanyFiscalYears(workspaceRoot, slug).years;

  const db = openDb(dbPath);
  try {
    migrate(db);
    const company = getCompanySettings(db);

    // Account number → (type, normalBalance), for classifying archived
    // balances. The archive stores raw account numbers; the live chart of
    // accounts is the only source of an account's statement-section
    // classification. `normalBalance` is needed so a `vat` account is placed
    // by its normal balance — the same rule `buildCompanyBalance` applies.
    const accountTypeRows = db
      .query(
        "SELECT account_no AS accountNo, type AS type, normal_balance AS normalBalance FROM accounts",
      )
      .all() as Array<{
      accountNo: string;
      type: string;
      normalBalance: "debit" | "credit";
    }>;
    const accountType = new Map(
      accountTypeRows.map((r) => [
        r.accountNo,
        { type: r.type, normalBalance: r.normalBalance },
      ]),
    );

    // Bruttomargin (resultat ÷ omsætning) and egenkapitalandel (egenkapital ÷
    // balancesum) — each a 0–1 fraction, or null when its denominator is zero.
    // The same two ratios the Overblik view surfaces; no figure is invented.
    const ratios = (
      resultat: number,
      omsaetning: number,
      egenkapital: number,
      balancesum: number,
    ) => ({
      bruttomargin: omsaetning !== 0 ? resultat / omsaetning : null,
      egenkapitalandel: balancesum !== 0 ? egenkapital / balancesum : null,
    });

    const rows: MultiYearRow[] = [];
    for (const fy of years) {
      if (fy.source === "live") {
        const yearNum = parseInt(fy.label, 10);
        const yearEnd = `${yearNum}-12-31`;
        const pl = buildProfitAndLoss(db, `${yearNum}-01-01`, yearEnd);
        // Balance-sheet development — total assets and equity (the equity
        // section plus the un-closed period result), exactly as the Balance
        // and Overblik views compute them.
        const bs = buildBalanceSheet(db, yearEnd);
        const balancesum = roundKroner(bs.totalAssets);
        const egenkapital = roundKroner(bs.equity.total + bs.periodResult);
        const omsaetning = roundKroner(pl.totalIncome);
        const udgifter = roundKroner(pl.totalExpense);
        const resultat = roundKroner(pl.result);
        rows.push({
          year: fy.label,
          source: "live",
          omsaetning,
          udgifter,
          resultat,
          balancesum,
          egenkapital,
          ...ratios(resultat, omsaetning, egenkapital, balancesum),
        });
        continue;
      }

      // Archived year — classify each SaldoBalance line by account type. The
      // archive `amount` is debit-signed (debit − credit): income/equity are
      // credit-normal and read negated, expenses/assets read as-is.
      const archiveId = db
        .query(
          "SELECT id FROM import_archive_years WHERE fiscal_year = ? ORDER BY id DESC",
        )
        .get(parseInt(fy.label, 10)) as { id: number } | undefined;
      let omsaetning = 0;
      let udgifter = 0;
      let balancesum = 0;
      let equitySection = 0;
      if (archiveId) {
        const balRows = db
          .query(
            `SELECT account_no AS accountNo, amount AS amount
               FROM import_archive_balances
              WHERE archive_year_id = ?`,
          )
          .all(archiveId.id) as Array<{ accountNo: string; amount: number }>;
        for (const b of balRows) {
          const acc = accountType.get(b.accountNo);
          const amount = Number(b.amount ?? 0);
          // The statement section the account belongs to — the shared #321
          // classification, so the Flerårsoversigt agrees with the Balance
          // view (a `vat` account is placed by its normal balance, not left
          // unclassified). Liabilities do not feed any Flerårsoversigt figure.
          const section = classifyAccountSection(acc?.type, acc?.normalBalance);
          if (section === "income") omsaetning += -amount;
          else if (section === "expense") udgifter += amount;
          else if (section === "asset") balancesum += amount;
          else if (section === "equity") equitySection += -amount;
        }
      }
      omsaetning = roundKroner(omsaetning);
      udgifter = roundKroner(udgifter);
      const resultat = roundKroner(omsaetning - udgifter);
      balancesum = roundKroner(balancesum);
      // Equity carries the un-closed period result so it matches the
      // archive-aware Balance view (assets = liabilities + equity + result).
      const egenkapital = roundKroner(equitySection + resultat);
      rows.push({
        year: fy.label,
        source: "archive",
        omsaetning,
        udgifter,
        resultat,
        balancesum,
        egenkapital,
        ...ratios(resultat, omsaetning, egenkapital, balancesum),
      });
    }

    // Oldest→newest so the SPA can chart a trend left-to-right.
    rows.sort((a, b) => a.year.localeCompare(b.year));

    return {
      slug: entry.slug,
      company: statementCompanyBlock(company),
      years: rows,
    };
  } finally {
    db.close();
  }
}
