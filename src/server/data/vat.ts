// VAT period selection and position computation for the cockpit (#320).
//
// Split out of `server/data.ts` by #320. Every cockpit surface that shows a
// VAT figure — the portfolio card, the per-company Overblik, the dedicated
// Moms view, the Forpligtelser list — reads the same period selection and the
// same booked-VAT-account position from here, so they never disagree.
// Behaviour is unchanged from the pre-split `server/data.ts`.

import type { Database } from "bun:sqlite";
import { buildVatReport } from "../../core/vat";
import { percentOfDkk } from "../../core/money";
import { emptyVatRubric, type VatRubric } from "../../core/vat-rubric";
import { vatFilingFormForPeriod } from "../../core/vat-filing-evidence";
import {
  vatPeriodWindowFor,
  vatPeriodsForYear,
  vatPeriodLabel,
  effectivePeriodState,
  type VatPeriodType,
  type EffectivePeriodState,
} from "../../core/periods";
import { roundKroner, todayIsoDate } from "./shared";

export type VatPosition = {
  periodStart: string;
  periodEnd: string;
  /**
   * Gross output-VAT control position for the period, kroner, before
   * bad-debt relief. The filing-only own-sale amount is `rubrikker.salgsmoms`.
   *
   * This is the *gross* figure: it does NOT have the bad-debt (debitortab)
   * output-VAT relief netted into it. A bad-debt write-off books a debit on
   * the output-VAT account, so a chart-of-accounts-level sum would let a large
   * write-off turn salgsmoms negative — a nonsensical headline for an owner
   * (#271). The relief is surfaced separately as `outputVatAdjustment`.
   */
  outputVat: number;
  /**
   * The bad-debt (debitortab) output-VAT adjustment for the period, kroner —
   * a value ≤ 0, the negative of the VAT relief claimed on written-off
   * receivables. Zero when no write-off falls in the period. Kept on its own
   * clearly-labelled line so it never silently drags salgsmoms negative.
   */
  outputVatAdjustment: number;
  /** Input VAT (købsmoms) booked for the period, kroner. */
  inputVat: number;
  /** outputVat + outputVatAdjustment − inputVat; positive is payable, kroner. */
  payable: number;
  /** Whether the canonical VAT report is filing-safe for this period. */
  reportOk: boolean;
  reportErrors: string[];
  reportWarnings: string[];
  /** Whether the period contains any posted bookkeeping, including zero-VAT activity. */
  hasBookkeepingActivity: boolean;
};

/**
 * The standard SKAT TastSelv momsangivelse rubrics for a VAT period — the
 * shape `core/vat-filing.ts#VatFilingRubrikker` produces, surfaced so the
 * cockpit shows the same numbers an owner files. All amounts are kroner.
 */
export type VatRubrikker = VatRubric;

/** Whether the period carries bookkeeping that can require a VAT return. */
function vatPeriodHasActivity(pos: VatPosition): boolean {
  return (
    pos.hasBookkeepingActivity ||
    pos.payable !== 0 ||
    pos.outputVat !== 0 ||
    pos.outputVatAdjustment !== 0 ||
    pos.inputVat !== 0
  );
}

/**
 * The VAT position for a period. It is a projection of the canonical core VAT
 * report, not a second SQL/account-number implementation. Report, filing,
 * cockpit and exports therefore share account semantics and failure state.
 */
export function vatPositionForPeriod(
  db: Database,
  periodStart: string,
  periodEnd: string,
): VatPosition {
  const report = buildVatReport(db, periodStart, periodEnd);
  const bookedOutputVat = roundKroner(report.outputVat);
  const inputVat = roundKroner(report.inputVat);

  // A bad-debt write-off (debitortab) books a debit on the output-VAT account
  // to claim back the VAT on a receivable that will never be paid. The booked
  // output-VAT total above therefore already has that relief netted in — a
  // large write-off can drive it negative. Split the relief back out so the
  // headline salgsmoms shows the genuine VAT on sales and the adjustment sits
  // on its own clearly-labelled line (#271). `buildVatReport` keys the relief
  // off the `DK_BAD_DEBT_25` vat-code base, the same source the CLI uses.
  const outputVatAdjustment = roundKroner(
    -percentOfDkk(report.badDebtReliefBase25, 25),
  );
  // Gross output-VAT control position with bad-debt relief added back in.
  const outputVat = roundKroner(bookedOutputVat - outputVatAdjustment);

  return {
    periodStart,
    periodEnd,
    outputVat,
    outputVatAdjustment,
    inputVat,
    payable: roundKroner(report.netVatPayable),
    reportOk: report.ok,
    reportErrors: [...report.errors],
    reportWarnings: [...report.warnings],
    // A VAT return can legitimately be zero. Such a period still needs to be
    // filed when the company traded, so the attention selector must not rely
    // solely on non-zero VAT account balances (#555).
    hasBookkeepingActivity: report.totalJournalEntryCount > 0,
  };
}

/**
 * The VAT period the cockpit surfaces — generalised over the company's VAT
 * cadence (#299).
 *
 * For the current calendar year, the earliest period with posted bookkeeping
 * that has not been reported is the filing obligation needing attention. This
 * includes legitimate zero-VAT returns (#555). When every active period is
 * reported, the current accrual period is shown. Monthly, quarterly and
 * half-yearly companies all follow the same rule.
 *
 * Returns the chosen period's window, its booked VAT position, a Danish label
 * and the statutory filing deadline so callers do not recompute any of it.
 */
export function selectVatPeriod(
  db: Database,
  year: number,
  vatType: VatPeriodType,
  asOfDate = todayIsoDate(),
): {
  start: string;
  end: string;
  label: string;
  deadline: string;
  position: VatPosition;
} {
  const windows = vatPeriodsForYear(year, vatType);
  const positions = windows.map((w) => vatPositionForPeriod(db, w.start, w.end));

  const asOfYear = parseInt(asOfDate.slice(0, 4), 10);

  // The latest period index at or before `cap` that carries activity, or null.
  const latestActiveUpTo = (cap: number): number | null => {
    for (let i = windows.length - 1; i >= 0; i -= 1) {
      if (i <= cap && vatPeriodHasActivity(positions[i]!)) return i;
    }
    return null;
  };

  // The index of the period that contains the caller's explicit reference
  // date (clamped into the year). Read APIs must never silently substitute
  // the wall clock for an `asOf` snapshot.
  const indexOfDate = (iso: string): number => {
    const target = vatPeriodWindowFor(iso, vatType).start;
    const idx = windows.findIndex((w) => w.start === target);
    return idx >= 0 ? idx : windows.length - 1;
  };

  let selected: number;
  if (year === asOfYear) {
    const currentIndex = indexOfDate(asOfDate);
    // A filed period is settled. Of the remaining periods, the earliest one
    // with activity is the filing obligation that must be surfaced first.
    // This prevents a Q3 posting from hiding Q2 while Q2's 1 September
    // deadline is still the relevant statutory deadline.
    selected = windows.findIndex(
      (window, index) =>
        index <= currentIndex &&
        vatPeriodHasActivity(positions[index]!) &&
        vatPeriodEffectiveStatus(db, window.start, window.end) !== "reported",
    );
    // Every active period has been reported, or the company has no activity
    // yet. In both cases the current accrual period is the useful fallback;
    // never move backwards to an already-reported obligation.
    if (selected < 0) selected = currentIndex;
  } else {
    selected = windows.findIndex(
      (window, index) =>
        vatPeriodHasActivity(positions[index]!) &&
        vatPeriodEffectiveStatus(db, window.start, window.end) !== "reported",
    );
    if (selected < 0) selected = latestActiveUpTo(windows.length - 1) ?? 0;
  }

  const window = windows[selected]!;
  return {
    start: window.start,
    end: window.end,
    label: vatPeriodLabel(window),
    deadline: window.filingDeadline,
    position: positions[selected]!,
  };
}

/**
 * The effective lifecycle state of the VAT period `start`..`end` — `open`,
 * `closed` or `reported` (#303). A momsangivelse may only be filed for a
 * closed or reported period; for an open period the cockpit must mark the
 * figures as provisional. Replays the append-only `period reopen`/`close`
 * audit lifecycle, so a period that was closed-then-reopened reads `open`.
 */
export function vatPeriodEffectiveStatus(
  db: Database,
  start: string,
  end: string,
): EffectivePeriodState {
  const row = db
    .query(
      `SELECT id, status
         FROM accounting_periods
        WHERE kind IN ('vat_period', 'vat_quarter')
          AND period_start = ? AND period_end = ?
        ORDER BY CASE kind WHEN 'vat_period' THEN 0 ELSE 1 END, id DESC
        LIMIT 1`,
    )
    .get(start, end) as
    | { id: number; status: "open" | "closed" | "reported" }
    | undefined;
  if (!row) return "open";
  return effectivePeriodState(db, row.id, row.status);
}

/**
 * The standard SKAT TastSelv momsangivelse rubrics for a VAT period.
 *
 * This is the SAME mapping `core/vat-filing.ts#buildVatFiling` applies, run
 * directly off `core/vat.ts#buildVatReport` so the cockpit can show the
 * rubrics for an *open* (not yet closed) period too — `buildVatFiling` itself
 * only produces a return for a closed cadence-neutral VAT period. The
 * numbers are identical to what the CLI's `vat momsangivelse` reports once the
 * period is closed; the cockpit surface and the terminal therefore agree.
 */
export function vatRubrikkerForPeriod(
  db: Database,
  periodStart: string,
  periodEnd: string,
): VatRubrikker {
  return vatFilingFormForPeriod(db, buildVatReport(db, periodStart, periodEnd));
}

/** A VatRubrikker with every rubric zeroed — used for an archived year. */
export function emptyVatRubrikker(): VatRubrikker {
  return emptyVatRubric();
}
