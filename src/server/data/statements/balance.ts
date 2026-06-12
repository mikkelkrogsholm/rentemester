// Per-company Balance — split out of statements.ts.
//
// Balance sheet as of the selected fiscal year's end date. Live years come
// from `core/financial-statements`; archived years come from the #197 archive
// via `classifyAccountSection` (#321). The period result is folded into
// equity so assets = liabilities + equity. Money is kroner.

import type { Database } from "bun:sqlite";
import { buildBalanceSheet } from "../../../core/financial-statements";
import { classifyAccountSection } from "../../../core/account-classification";
import {
  resolveStatementContext,
  roundKroner,
  statementCompanyBlock,
  type FiscalYearEntry,
} from "../shared";
import {
  archiveTypedBalances,
  archiveYearRow,
} from "../archive";

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
