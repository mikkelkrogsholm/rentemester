import { diffDaysSafe as daysBetween } from "../../../core/dates";
import {
  vatPeriodWindowFor,
  vatPeriodLabel,
  type EffectivePeriodState,
} from "../../../core/periods";
import {
  resolveStatementContext,
  statementCompanyBlock,
  todayIsoDate,
} from "../shared";
import {
  selectVatPeriod,
  vatPeriodEffectiveStatus,
  vatRubrikkerForPeriod,
  emptyVatRubrikker,
} from "../vat";

// --------------------------------------------------------------------------
// Per-company VAT return (Moms, year-aware) — cockpit-redesign it. 3
// --------------------------------------------------------------------------

export type CompanyVat = ReturnType<typeof buildCompanyVat>;

/**
 * Moms — the VAT return for the selected calendar fiscal year. The VAT period
 * follows the company's own settlement cadence (`vatPeriodType` — month /
 * quarter / half-year, #299); the period that is due now is surfaced, the same
 * selection `buildCompanyOverview` uses. The figures come from the booked VAT
 * accounts via `vatPositionForPeriod`. Money is kroner.
 *
 * #303: `periodStatus` reports the period's effective lifecycle state. A
 * momsangivelse may only be FILED for a `closed`/`reported` period — for an
 * `open` period the figures are provisional, and the cockpit must say so
 * rather than claim they match the terminal `vat momsangivelse` (which refuses
 * an open period). `momsangivelseReady` is the single flag the SPA keys off.
 */
export function buildCompanyVat(
  workspaceRoot: string,
  slug: string,
  year: number | null,
) {
  const ctx = resolveStatementContext(workspaceRoot, slug, year);
  try {
    const companyBlock = statementCompanyBlock(ctx.company);
    if (ctx.isArchivedOnly) {
      const archYear = parseInt(ctx.selectedLabel, 10);
      const archWindow = vatPeriodWindowFor(
        `${archYear}-01-01`,
        ctx.company.vatPeriodType,
      );
      return {
        slug: ctx.entry.slug,
        selectedYear: ctx.selectedLabel,
        archived: true,
        company: companyBlock,
        fiscalYears: ctx.years,
        periodStart: archWindow.start,
        periodEnd: archWindow.end,
        periodLabel: vatPeriodLabel(archWindow),
        outputVat: 0,
        outputVatAdjustment: 0,
        inputVat: 0,
        payable: 0,
        deadline: archWindow.filingDeadline,
        daysRemaining: daysBetween(todayIsoDate(), archWindow.filingDeadline),
        // An archived year carries no live period to close — treat as open so
        // no provisional figures are ever claimed filing-ready.
        periodStatus: "open" as EffectivePeriodState,
        momsangivelseReady: false,
        rubrikker: emptyVatRubrikker(),
      };
    }

    const yearNum = parseInt(ctx.selectedLabel, 10);
    // Surface the VAT period (month / quarter / half-year, per the company's
    // `vatPeriodType`) that is due now — the same selection the static
    // dashboard and the Overblik view use, so the period type never depends on
    // which screen the owner looks at (#299).
    const vatSelection = selectVatPeriod(
      ctx.db,
      yearNum,
      ctx.company.vatPeriodType,
    );
    const vat = vatSelection.position;

    // The statutory filing/payment deadline for the surfaced period, plus a
    // signed countdown from today — negative once the deadline has passed.
    const deadline = vatSelection.deadline;

    // #303: a momsangivelse is only filing-ready for a closed/reported period.
    // For an open period the cockpit shows the figures as PROVISIONAL.
    const periodStatus = vatPeriodEffectiveStatus(
      ctx.db,
      vat.periodStart,
      vat.periodEnd,
    );
    const momsangivelseReady =
      periodStatus === "closed" || periodStatus === "reported";

    // The full SKAT TastSelv rubrics — the same numbers the CLI's
    // `vat momsangivelse` reports — so an owner can file straight from here.
    const rubrikker = vatRubrikkerForPeriod(
      ctx.db,
      vat.periodStart,
      vat.periodEnd,
    );

    return {
      slug: ctx.entry.slug,
      selectedYear: ctx.selectedLabel,
      archived: false,
      company: companyBlock,
      fiscalYears: ctx.years,
      periodStart: vat.periodStart,
      periodEnd: vat.periodEnd,
      periodLabel: vatSelection.label,
      outputVat: vat.outputVat,
      outputVatAdjustment: vat.outputVatAdjustment,
      inputVat: vat.inputVat,
      payable: vat.payable,
      deadline,
      daysRemaining: daysBetween(todayIsoDate(), deadline),
      periodStatus,
      momsangivelseReady,
      rubrikker,
    };
  } finally {
    ctx.db.close();
  }
}
