// Corporate tax return preparation (oplysningsskema) — a deterministic FIRST
// SLICE that closes the year-end-to-tax gap.
//
// `buildTaxReturn` takes a fiscal year and derives the corporate taxable income
// (skattepligtig indkomst) from the bookkept årets resultat plus the
// skattemæssige reguleringer the ledger can see DETERMINISTICALLY. From the
// taxable income it computes the 22% selskabsskat for the common micro-ApS
// case. The output is an "oplysningsskema preparation" — the figures a user or
// agent then transfers into TastSelv Erhverv. It is NOT an API integration.
//
// It is a pure read: it consumes the #177 annual report and the #176 financial
// statements / VAT report, never mutates the database, never calls the wall
// clock, and produces byte-identical output for identical input.
//
// CONSERVATIVE BY DESIGN. This slice computes only the adjustments that follow
// deterministically from data the system already holds:
//   - the non-deductible representation VAT add-back (75% of representation VAT
//     is expensed but is not tax-deductible).
// Everything that cannot be derived deterministically — tax depreciation
// (saldoafskrivning is the company's choice, not derivable from book linear
// depreciation), loss carry-forward (fremført underskud from prior years),
// and any company form other than a micro-ApS — is surfaced as a NEEDS-REVIEW
// item rather than guessed. Rentemester PREPARES the figures; the owner or
// advisor reviews them and is responsible for the actual SKAT filing.

import type { Database } from "bun:sqlite";
import { buildAnnualReport, type AnnualReport } from "./annual-report";
import { buildVatReport } from "./vat";
import { buildAssetRegisterReport } from "./assets";
import { isValidIsoDate as looksLikeIsoDate } from "./dates";
import { addDkk, percentOfDkk, roundDkk } from "./money";

// A derived-report identifier (workflow guardrail), not a normative ledger
// rule keyed against a single statutory provision. It appears in
// `appliedRules` as provenance and is declared in rules/dk/tax-return.yaml.
const TAX_RETURN_RULE_ID = "DK-TAX-RETURN-CORP-001";

/**
 * Danish corporate-tax rate (selskabsskat) for the common case. A documented
 * assist constant — like the straksafskrivning threshold in assets.ts — kept
 * in code so the slice stays deterministic; the owner/advisor owns the actual
 * tax-law determination if a non-standard rate applies (those are surfaced as
 * needs-review via the company-form check).
 */
export const CORPORATE_TAX_RATE = 0.22;

/** Company-form values (case-insensitive) that this slice treats as a micro-ApS. */
const APS_FORMS = new Set(["aps", "anpartsselskab"]);

/** A skattemæssig regulering Rentemester can apply deterministically. */
export type TaxAdjustment = {
  /** Stable adjustment identifier. */
  kind: "non_deductible_representation";
  /** Danish label. */
  label: string;
  /** Add-back amount, DKK. Positive increases taxable income above the bookkept result. */
  amount: number;
  /** Short Danish explanation of why the adjustment applies. */
  explanation: string;
};

/** A figure that the slice deliberately does NOT compute — flagged for review. */
export type TaxNeedsReview = {
  kind:
    | "depreciation_difference"
    | "tax_loss_carry_forward"
    | "company_form_out_of_scope";
  /** Danish label. */
  label: string;
  /** What the owner/advisor must determine. */
  requiredAction: string;
  /** Book depreciation posted in the year, DKK — present for depreciation_difference. */
  bookDepreciation?: number;
};

export type TaxReturn = {
  ok: boolean;
  appliedRules: string[];
  fiscalYearStart: string;
  fiscalYearEnd: string;
  /** Company form as recorded in master data (e.g. "ApS"), or "" if unknown. */
  companyForm: string;
  /** Årets resultat from the bookkept annual report, DKK. */
  bookkeptResult: number;
  /** The deterministic skattemæssige reguleringer applied. */
  adjustments: TaxAdjustment[];
  /** Sum of `adjustments[].amount`, DKK. */
  totalAdjustments: number;
  /** Skattepligtig indkomst = bookkeptResult + totalAdjustments, DKK. */
  taxableIncome: number;
  /** The corporate-tax rate applied (0.22) — echoed for traceability. */
  corporateTaxRate: number;
  /**
   * Selskabsskat = max(0, taxableIncome) × 22%, DKK. `null` when the company
   * form is outside the micro-ApS slice (a needs-review item is emitted instead).
   */
  corporateTax: number | null;
  /** Figures the slice deliberately did not compute — for owner/advisor review. */
  needsReview: TaxNeedsReview[];
  /** Conservative claim: who prepared the figures. */
  preparedBy: "Rentemester";
  /** Conservative-language disclaimer (Danish). */
  disclaimer: string;
  errors: string[];
};

const DISCLAIMER =
  "Rentemester forbereder tallene til oplysningsskemaet (selskabets skattepligtige " +
  "indkomst) ud fra det lukkede regnskabsaar og de skattemaessige reguleringer " +
  "systemet kan se deterministisk. Ejer eller revisor gennemgaar tallene, afklarer " +
  "needs-review-punkterne og indberetter selv via TastSelv Erhverv. Dette er ikke " +
  "en fuldstaendig skatteberegning.";

function failure(
  fiscalYearStart: string,
  fiscalYearEnd: string,
  companyForm: string,
  errors: string[],
): TaxReturn {
  return {
    ok: false,
    appliedRules: [TAX_RETURN_RULE_ID],
    fiscalYearStart,
    fiscalYearEnd,
    companyForm,
    bookkeptResult: 0,
    adjustments: [],
    totalAdjustments: 0,
    taxableIncome: 0,
    corporateTaxRate: CORPORATE_TAX_RATE,
    corporateTax: null,
    needsReview: [],
    preparedBy: "Rentemester",
    disclaimer: DISCLAIMER,
    errors,
  };
}

type CompanyFormRow = { company_form: string | null };

/**
 * Prepare the corporate taxable-income figures (oplysningsskema) for a fiscal
 * year.
 *
 * Prerequisites are exactly the annual report's: valid ISO dates, a registered
 * CVR, a fully locked fiscal year and balanced books. The annual report is
 * consumed read-only — any prerequisite failure surfaces verbatim.
 */
export function buildTaxReturn(
  db: Database,
  fiscalYearStart: string,
  fiscalYearEnd: string,
): TaxReturn {
  // 1. Date validation up front so a bad date never reaches the annual report
  // or VAT queries.
  if (!looksLikeIsoDate(fiscalYearStart) || !looksLikeIsoDate(fiscalYearEnd)) {
    return failure(fiscalYearStart, fiscalYearEnd, "", [
      "fiscalYearStart and fiscalYearEnd must be YYYY-MM-DD",
    ]);
  }
  if (fiscalYearStart > fiscalYearEnd) {
    return failure(fiscalYearStart, fiscalYearEnd, "", [
      "fiscalYearStart must be before or equal to fiscalYearEnd",
    ]);
  }

  const formRow = db
    .query(`SELECT company_form FROM companies ORDER BY id ASC LIMIT 1`)
    .get() as CompanyFormRow | null;
  const companyForm = formRow?.company_form ?? "";

  // 2. The taxable income rests on a final, locked annual report. Reuse the
  // #177 prerequisites (locked year, registered CVR, balanced books) rather
  // than re-deriving them — a failing annual report fails the tax return.
  const annualReport: AnnualReport = buildAnnualReport(db, fiscalYearStart, fiscalYearEnd);
  if (!annualReport.ok) {
    return failure(fiscalYearStart, fiscalYearEnd, companyForm, [...annualReport.errors]);
  }

  const bookkeptResult = roundDkk(annualReport.aretsResultat);

  // 3. Deterministic skattemæssig regulering: non-deductible representation.
  // postRepresentationPurchase books 75% of the representation VAT as a
  // non-deductible expense (account 3070). That portion sits in the bookkept
  // result but is not tax-deductible, so it is added back. The base is
  // `representationPurchaseBase` from the VAT report for the fiscal year:
  //   non-deductible VAT = base × 25% (full VAT) × 75% (non-deductible share).
  const adjustments: TaxAdjustment[] = [];
  const vatReport = buildVatReport(db, fiscalYearStart, fiscalYearEnd);
  if (vatReport.ok && vatReport.representationPurchaseBase > 0) {
    const fullVat = percentOfDkk(vatReport.representationPurchaseBase, 25);
    const nonDeductibleVat = roundDkk(percentOfDkk(fullVat, 75));
    if (nonDeductibleVat > 0) {
      adjustments.push({
        kind: "non_deductible_representation",
        label: "Ikke-fradragsberettiget repraesentationsmoms",
        amount: nonDeductibleVat,
        explanation:
          "75% af repraesentationsmomsen er bogfoert som en udgift, men er ikke " +
          "skattemaessigt fradragsberettiget og laegges derfor til den skattepligtige indkomst.",
      });
    }
  }

  const totalAdjustments = roundDkk(addDkk(...adjustments.map((a) => a.amount)));
  const taxableIncome = roundDkk(addDkk(bookkeptResult, totalAdjustments));

  // 4. Needs-review items — things the slice deliberately does not compute.
  const needsReview: TaxNeedsReview[] = [];

  // 4a. Depreciation difference. Book depreciation is deterministic linear
  // (#124); tax depreciation is saldoafskrivning (declining-balance, up to
  // 25%, the company's choice). The difference cannot be derived from data the
  // system holds — flag it when any book depreciation was posted.
  const assetRegister = buildAssetRegisterReport(db);
  const bookDepreciation = assetRegister.ok
    ? roundDkk(
        assetRegister.assets
          .filter((a) => a.postedPeriods > 0)
          .reduce((sum, a) => sum + a.accumulatedDepreciation, 0),
      )
    : 0;
  if (bookDepreciation > 0) {
    needsReview.push({
      kind: "depreciation_difference",
      label: "Forskel mellem regnskabsmaessige og skattemaessige afskrivninger",
      requiredAction:
        "Opgoer de skattemaessige afskrivninger (saldoafskrivning) og reguler for " +
        "forskellen til de regnskabsmaessige (lineaere) afskrivninger.",
      bookDepreciation,
    });
  }

  // 4b. Tax loss carry-forward. A loss this year (or fremført underskud from
  // earlier years) is not computed here — the slice only prepares the current
  // year's figures.
  if (taxableIncome < 0) {
    needsReview.push({
      kind: "tax_loss_carry_forward",
      label: "Fremfoert skattemaessigt underskud",
      requiredAction:
        "Aarets skattepligtige indkomst er negativ. Underskuddet fremfoeres til " +
        "modregning i senere aars positive indkomst — afklar fremfoerslen i oplysningsskemaet.",
    });
  }

  // 4c. Company form. This slice handles the micro-ApS case only. Any other
  // form (or an unknown form) is flagged and no corporate tax is computed.
  const isAps = APS_FORMS.has(companyForm.trim().toLowerCase());
  if (!isAps) {
    needsReview.push({
      kind: "company_form_out_of_scope",
      label: "Selskabsform uden for dette slice",
      requiredAction:
        "Dette slice beregner kun selskabsskat (22%) for et anpartsselskab (ApS). " +
        `Den registrerede selskabsform er "${companyForm || "ukendt"}" — ` +
        "afklar den korrekte skatteberegning med revisor.",
    });
  }

  // 5. Corporate tax — 22% of a non-negative taxable income, micro-ApS only.
  const corporateTax = isAps
    ? roundDkk(percentOfDkk(Math.max(0, taxableIncome), CORPORATE_TAX_RATE * 100))
    : null;

  return {
    ok: true,
    appliedRules: [TAX_RETURN_RULE_ID],
    fiscalYearStart,
    fiscalYearEnd,
    companyForm,
    bookkeptResult,
    adjustments,
    totalAdjustments,
    taxableIncome,
    corporateTaxRate: CORPORATE_TAX_RATE,
    corporateTax,
    needsReview,
    preparedBy: "Rentemester",
    disclaimer: DISCLAIMER,
    errors: [],
  };
}
