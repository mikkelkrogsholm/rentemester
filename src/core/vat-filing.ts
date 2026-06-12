import type { Database } from "bun:sqlite";
import { buildVatReport, vatFilingDeadline, type VatPeriodReport } from "./vat";
import { isValidIsoDate as looksLikeIsoDate } from "./dates";
import { addDkk, percentOfDkk, subtractDkk } from "./money";

/**
 * Filing-ready momsangivelse (Danish VAT return).
 *
 * Maps the raw VAT data from {@link buildVatReport} into the standard SKAT
 * rubrikker a user submits via TastSelv. Conservative by design: Rentemester
 * produces the numbers, the user files them. No direct SKAT submission, no
 * OSS/MOSS one-stop-shop.
 *
 * A momsangivelse can only be produced for a VAT period that has been closed
 * (or marked reported) as a vat_quarter accounting period — an open or
 * incomplete period fails clearly. All amounts are integer-øre-deterministic
 * via the money helpers; 25% is the only Danish standard rate.
 */
export type VatFilingRubrikker = {
  /** Salgsmoms — output VAT on domestic sales (net of bad-debt relief). */
  salgsmoms: number;
  /** Moms af varekøb i udlandet — VAT on goods purchased abroad. */
  momsAfVarekobUdland: number;
  /** Moms af ydelseskøb i udlandet — VAT on services purchased abroad (reverse charge). */
  momsAfYdelseskobUdland: number;
  /** Købsmoms — total deductible input VAT. */
  kobsmoms: number;
  /** Momstilsvar — salgsmoms + udenlandsk moms − købsmoms. Positive = owed to SKAT. */
  momstilsvar: number;
  /** Rubrik A — value of goods/services purchased abroad without Danish VAT. */
  rubrikA: number;
  /** Rubrik B — value of goods/services sold abroad without Danish VAT. */
  rubrikB: number;
  /** Rubrik C — value of other sales exempt from VAT. */
  rubrikC: number;
};

export type VatFilingReport = {
  ok: boolean;
  appliedRules: string[];
  periodStart: string;
  periodEnd: string;
  /** Status of the matching accounting period: "open" when no closed/reported vat_quarter covers it exactly. */
  periodStatus: "open" | "closed" | "reported";
  /** Reference recorded on the closed accounting period, if any. */
  periodReference: string | null;
  /**
   * SKAT filing/payment deadline (YYYY-MM-DD) — the 1st of the third month
   * after the period ends. `null` only when periodEnd is not a valid date.
   */
  filingDeadline: string | null;
  rubrikker: VatFilingRubrikker;
  /** The underlying raw VAT report, for traceability. */
  vatReport: VatPeriodReport;
  warnings: string[];
  errors: string[];
};

const FILING_RULE_ID = "DK-VAT-FILING-001";

function emptyRubrikker(): VatFilingRubrikker {
  return {
    salgsmoms: 0,
    momsAfVarekobUdland: 0,
    momsAfYdelseskobUdland: 0,
    kobsmoms: 0,
    momstilsvar: 0,
    rubrikA: 0,
    rubrikB: 0,
    rubrikC: 0,
  };
}

function failure(periodStart: string, periodEnd: string, periodStatus: VatFilingReport["periodStatus"], errors: string[], vatReport: VatPeriodReport): VatFilingReport {
  return {
    ok: false,
    appliedRules: [FILING_RULE_ID],
    periodStart,
    periodEnd,
    periodStatus,
    periodReference: null,
    filingDeadline: vatFilingDeadline(periodEnd),
    rubrikker: emptyRubrikker(),
    vatReport,
    warnings: [],
    errors,
  };
}

/**
 * Build a filing-ready momsangivelse for a VAT period.
 *
 * The period must exactly match a closed or reported `vat_quarter`
 * accounting period — otherwise the filing fails (an open period is not yet
 * final and must not be submitted).
 */
export function buildVatFiling(db: Database, periodStart: string, periodEnd: string): VatFilingReport {
  const vatReport = buildVatReport(db, periodStart, periodEnd);

  // Surface date-validation errors from the underlying report verbatim.
  if (!vatReport.ok) {
    return failure(periodStart, periodEnd, "open", [...vatReport.errors], vatReport);
  }

  if (!looksLikeIsoDate(periodStart) || !looksLikeIsoDate(periodEnd)) {
    return failure(periodStart, periodEnd, "open", ["periodStart and periodEnd must be YYYY-MM-DD"], vatReport);
  }

  // A momsangivelse may only be produced for a finalised VAT period. The
  // period bounds must exactly match a closed or reported vat_quarter
  // accounting period; anything else means the period is still open or
  // incomplete and must not be filed.
  const period = db.query(
    `SELECT status, reference
       FROM accounting_periods
      WHERE period_start = ? AND period_end = ? AND kind = 'vat_quarter'
        AND status IN ('closed', 'reported')
      ORDER BY id DESC
      LIMIT 1`
  ).get(periodStart, periodEnd) as { status: "closed" | "reported"; reference: string | null } | null;

  if (!period) {
    return failure(
      periodStart,
      periodEnd,
      "open",
      [
        `VAT period ${periodStart}..${periodEnd} is not closed: a momsangivelse requires a closed or reported vat_quarter accounting period covering exactly this period — run 'period close' first`,
      ],
      vatReport,
    );
  }

  // Salgsmoms: output VAT booked on domestic sales and reverse-charge output.
  // buildVatReport.outputVat already nets bad-debt relief out of output VAT.
  const salgsmoms = vatReport.outputVat;

  // Moms af ydelseskøb i udlandet: reverse charge on EU service purchases.
  // 25% of the reverse-charge purchase base.
  const momsAfYdelseskobUdland = percentOfDkk(vatReport.reverseChargePurchaseBase, 25);

  // Moms af varekøb i udlandet: there is no separate goods-import VAT code in
  // the ledger today, so foreign-goods VAT is always 0. Kept as an explicit
  // rubrik so the momsangivelse shape matches the SKAT form.
  const momsAfVarekobUdland = 0;

  // Købsmoms: total deductible input VAT (domestic + reverse-charge +
  // representation), already aggregated by buildVatReport.
  const kobsmoms = vatReport.inputVat;

  // Momstilsvar = salgsmoms + udenlandsk moms − købsmoms.
  // Positive = payable to SKAT; negative = refund (negativt momstilsvar).
  const momstilsvar = subtractDkk(addDkk(salgsmoms, momsAfVarekobUdland, momsAfYdelseskobUdland), kobsmoms);

  // Rubrik A: value of goods/services purchased abroad without Danish VAT.
  const rubrikA = vatReport.reverseChargePurchaseBase;
  // Rubrik B: value of goods/services sold abroad without Danish VAT.
  const rubrikB = vatReport.reverseChargeSalesBase;
  // Rubrik C: value of other VAT-exempt sales (momsloven §13), now derived
  // from real ledger data — revenue lines booked with the DK_SALE_EXEMPT VAT
  // code. OSS consumer sales (OSS_EU_CONSUMER) are deliberately NOT part of
  // rubrik C: they belong on the separate OSS return, so buildVatReport keeps
  // them in their own base and they never reach this momsangivelse.
  const rubrikC = vatReport.exemptSalesBase;

  return {
    ok: true,
    appliedRules: [FILING_RULE_ID],
    periodStart,
    periodEnd,
    periodStatus: period.status,
    periodReference: period.reference,
    filingDeadline: vatFilingDeadline(periodEnd),
    rubrikker: {
      salgsmoms,
      momsAfVarekobUdland,
      momsAfYdelseskobUdland,
      kobsmoms,
      momstilsvar,
      rubrikA,
      rubrikB,
      rubrikC,
    },
    vatReport,
    warnings: [...vatReport.warnings],
    errors: [],
  };
}
