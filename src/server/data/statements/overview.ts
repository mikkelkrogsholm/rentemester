// Per-company Overblik (cockpit dashboard) — split out of statements.ts.
//
// Year-aware company dashboard the SPA renders as the P0 view. Every figure is
// computed from posted ledger postings (P&L, VAT, bank) or, for an archived
// year, from the #197 archive. Money is kroner throughout.

import { existsSync } from "node:fs";
import { companyPaths } from "../../../core/paths";
import { diffDaysSafe as daysBetween } from "../../../core/dates";
import { openDb, migrate } from "../../../core/db";
import { getCompanySettings } from "../../../core/company";
import { listExceptions } from "../../../core/exceptions";
import { buildInvoiceList } from "../../../core/invoice-list";
import { buildProfitAndLoss, buildBalanceSheet } from "../../../core/financial-statements";
import {
  companyRootForSlug,
  findWorkspaceCompany,
} from "../../../core/workspace";
import { ApiError } from "../../errors";
import {
  buildCompanyFiscalYears,
  MONTH_NAMES_DK,
  roundKroner,
  todayIsoDate,
} from "../shared";
import { bankBalanceAsOf, actualBankBalanceAsOf } from "../bank";
import { selectVatPeriod } from "../vat";
import { groupExceptions, type ExceptionGroup } from "../exceptions";
import {
  archiveIncomeStatement,
  archiveYearRow,
} from "../archive";

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
