import type { Database } from "bun:sqlite";
import { buildVatReport, type VatPeriodReport } from "./vat";
import { emptyVatRubric, type VatRubric } from "./vat-rubric";
import { currentVatFilingEvidence, vatFilingFormForPeriod } from "./vat-filing-evidence";
import { buildViesRecapitulativeStatement } from "./vat-vies-list";
import { roundDkk } from "./money";
import { isValidIsoDate as looksLikeIsoDate } from "./dates";
import { getCompanySettings } from "./company";
import {
  effectivePeriodState,
  vatPeriodWindowFor,
  type VatPeriodType,
} from "./periods";

/**
 * Filing-ready momsangivelse (Danish VAT return).
 *
 * Maps the raw VAT data from {@link buildVatReport} into the standard SKAT
 * rubrikker a user submits via TastSelv. Conservative by design: Rentemester
 * produces the numbers, the user files them. No direct SKAT submission, no
 * OSS/MOSS one-stop-shop.
 *
 * A momsangivelse can only be produced for an exact registered-cadence VAT
 * period that has been closed (or marked reported) as a `vat_period` — an
 * open or incomplete period fails clearly. All amounts are integer-øre-
 * deterministic via the money helpers; 25% is the Danish standard rate.
 */
export type VatFilingRubrikker = VatRubric;

export type VatFilingReport = {
  ok: boolean;
  appliedRules: string[];
  periodStart: string;
  periodEnd: string;
  /** Company's registered VAT cadence used for bounds and deadline. */
  vatPeriodType: VatPeriodType | null;
  /** Status of the matching accounting period: "open" when no final VAT period covers it exactly. */
  periodStatus: "open" | "closed" | "reported";
  /** Reference recorded on the closed accounting period, if any. */
  periodReference: string | null;
  /**
   * Cadence-aware SKAT filing/payment deadline (YYYY-MM-DD). `null` only for
   * a non-registered company or an invalid period end.
   */
  filingDeadline: string | null;
  rubrikker: VatFilingRubrikker;
  /** The underlying raw VAT report, for traceability. */
  vatReport: VatPeriodReport;
  warnings: string[];
  errors: string[];
};

const FILING_RULE_ID = "DK-VAT-FILING-001";

function emptyRubrikker(): VatFilingRubrikker { return emptyVatRubric(); }

function failure(
  periodStart: string,
  periodEnd: string,
  periodStatus: VatFilingReport["periodStatus"],
  errors: string[],
  vatReport: VatPeriodReport,
  vatPeriodType: VatPeriodType | null,
  periodReference: string | null = null,
): VatFilingReport {
  return {
    ok: false,
    appliedRules: [FILING_RULE_ID],
    periodStart,
    periodEnd,
    vatPeriodType,
    periodStatus,
    periodReference,
    filingDeadline: vatReport.filingDeadline,
    rubrikker: emptyRubrikker(),
    vatReport,
    warnings: [],
    errors,
  };
}

/**
 * Build a filing-ready momsangivelse for a VAT period.
 *
 * The period must exactly match a canonical window for the company's
 * registered cadence and a closed/reported `vat_period` accounting period.
 */
export function buildVatFiling(db: Database, periodStart: string, periodEnd: string): VatFilingReport {
  const vatReport = buildVatReport(db, periodStart, periodEnd);
  const vatPeriodType = getCompanySettings(db).vatPeriodType;

  if (!looksLikeIsoDate(periodStart) || !looksLikeIsoDate(periodEnd)) {
    return failure(periodStart, periodEnd, "open", ["periodStart and periodEnd must be YYYY-MM-DD"], vatReport, vatPeriodType);
  }

  if (vatPeriodType === null) {
    return failure(
      periodStart,
      periodEnd,
      "open",
      ["selskabet er ikke momsregistreret og har derfor ingen momsangivelsesperiode"],
      vatReport,
      null,
    );
  }

  const canonicalWindow = vatPeriodWindowFor(periodStart, vatPeriodType);
  if (
    canonicalWindow.start !== periodStart ||
    canonicalWindow.end !== periodEnd
  ) {
    return failure(
      periodStart,
      periodEnd,
      "open",
      [
        `VAT period ${periodStart}..${periodEnd} does not match the company's registered ${vatPeriodType} cadence; the canonical period containing ${periodStart} is ${canonicalWindow.start}..${canonicalWindow.end}`,
      ],
      vatReport,
      vatPeriodType,
    );
  }

  // A momsangivelse may only be produced for a finalised VAT period. The
  // period bounds must exactly match a closed or reported VAT period
  // accounting period; anything else means the period is still open or
  // incomplete and must not be filed.
  const period = db.query(
    `SELECT id, status, reference
       FROM accounting_periods
      WHERE period_start = ? AND period_end = ? AND kind IN ('vat_period', 'vat_quarter')
      ORDER BY CASE kind WHEN 'vat_period' THEN 0 ELSE 1 END, id DESC
      LIMIT 1`
  ).get(periodStart, periodEnd) as {
    id: number;
    status: "open" | "closed" | "reported";
    reference: string | null;
  } | null;

  const periodStatus = period
    ? effectivePeriodState(db, period.id, period.status)
    : "open";

  // Resolve lifecycle before surfacing report-integrity failures. A closed
  // period with malformed legacy VAT data is still closed; labelling it open
  // would send the user down the wrong recovery path. The report's own
  // canonical deadline is used, so arbitrary bounds never gain a fake date.
  if (!vatReport.ok) {
    const reportErrors = [...vatReport.errors];
    if (!period || periodStatus === "open") {
      reportErrors.push(
        `VAT period ${periodStart}..${periodEnd} is not closed: a momsangivelse requires a closed or reported vat_period accounting period covering exactly this period — run 'period close --kind vat_period' first`,
      );
    }
    return failure(
      periodStart,
      periodEnd,
      periodStatus,
      reportErrors,
      vatReport,
      vatPeriodType,
      period?.reference ?? null,
    );
  }

  if (!period || periodStatus === "open") {
    return failure(
      periodStart,
      periodEnd,
      "open",
      [
        `VAT period ${periodStart}..${periodEnd} is not closed: a momsangivelse requires a closed or reported vat_period accounting period covering exactly this period — run 'period close --kind vat_period' first`,
      ],
      vatReport,
      vatPeriodType,
    );
  }

  const rubrikker = vatFilingFormForPeriod(db, vatReport);
  const evidence = currentVatFilingEvidence(db, periodStart, periodEnd);
  const classifiedB = roundDkk((evidence.rubrikBVarerEuSalesList ?? 0)
    + (evidence.rubrikBVarerIkkeEuSalesList ?? 0) + (evidence.rubrikBYdelser ?? 0));
  // A historical aggregate cannot legally select one of TastSelv's three B
  // fields. Do not silently move it: a dedicated evidence classification is
  // required before this return can be filed.
  if (classifiedB !== vatReport.foreignReverseChargeSalesBase) {
    return failure(
      periodStart,
      periodEnd,
      periodStatus,
      ["foreign VAT-free sales require documented B-field classification before filing; Rentemester will not infer goods/services or EU-sales-list status from an aggregate"],
      vatReport,
      vatPeriodType,
      period.reference,
    );
  }

  // The EU sales list is an independent filing surface. Only the two B fields
  // that are explicitly marked for it may reconcile to its total; the
  // non-list goods field must never be used to make the check pass.
  const vies = buildViesRecapitulativeStatement(db, periodStart, periodEnd);
  const euSalesListB = roundDkk((evidence.rubrikBVarerEuSalesList ?? 0) + (evidence.rubrikBYdelser ?? 0));
  if (!vies.ok || euSalesListB !== vies.totalValue) {
    return failure(
      periodStart,
      periodEnd,
      periodStatus,
      ["EU-sales-list total must reconcile to Rubrik B goods/EU-list plus Rubrik B services; non-list goods cannot satisfy this control"],
      vatReport,
      vatPeriodType,
      period.reference,
    );
  }

  return {
    ok: true,
    appliedRules: [FILING_RULE_ID],
    periodStart,
    periodEnd,
    vatPeriodType,
    periodStatus,
    periodReference: period.reference,
    filingDeadline: canonicalWindow.filingDeadline,
    rubrikker,
    vatReport,
    warnings: [...vatReport.warnings],
    errors: [],
  };
}
