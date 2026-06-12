// VAT / Moms wire types (GET .../vat?year=).

import type { FiscalYearEntry, StatementCompany } from "./common";

/**
 * The standard SKAT TastSelv momsangivelse rubrics for a VAT period — the
 * numbers an owner types into the momsangivelse. All amounts are kroner.
 */
export type VatRubrikker = {
  /** Salgsmoms — output VAT on domestic sales (net of bad-debt relief). */
  salgsmoms: number;
  /** Moms af varekøb i udlandet — VAT on goods purchased abroad. */
  momsAfVarekobUdland: number;
  /** Moms af ydelseskøb i udlandet — reverse-charge VAT on foreign services. */
  momsAfYdelseskobUdland: number;
  /** Købsmoms — total deductible input VAT. */
  kobsmoms: number;
  /** Momstilsvar — salgsmoms + udenlandsk moms − købsmoms; positive = owed. */
  momstilsvar: number;
  /** Rubrik A — value of goods/services bought abroad without Danish VAT. */
  rubrikA: number;
  /** Rubrik B — value of goods/services sold abroad without Danish VAT. */
  rubrikB: number;
  /** Rubrik C — value of other VAT-exempt sales. */
  rubrikC: number;
};

export type CompanyVat = {
  slug: string;
  selectedYear: string;
  archived: boolean;
  company: StatementCompany;
  fiscalYears: FiscalYearEntry[];
  periodStart: string;
  periodEnd: string;
  /**
   * The VAT period label — follows the company's settlement cadence (#299):
   * "Q2 2026" (quarter), "Maj 2026" (month), "1. halvår 2026" (half-year).
   */
  periodLabel: string;
  /**
   * Genuine output VAT on sales (salgsmoms) for the period, kroner — gross,
   * before any bad-debt relief. A bad-debt write-off books a debit on the
   * output-VAT account; surfacing the relief separately keeps this headline
   * from going negative (#271).
   */
  outputVat: number;
  /** Bad-debt (debitortab) output-VAT adjustment, ≤ 0; 0 when none, kroner. */
  outputVatAdjustment: number;
  /** Input VAT (købsmoms) booked for the period, kroner. */
  inputVat: number;
  /** outputVat + outputVatAdjustment − inputVat; positive is payable, kroner. */
  payable: number;
  /** The statutory VAT filing/payment deadline, YYYY-MM-DD. */
  deadline: string;
  /** Signed countdown from today to the deadline; negative once passed. */
  daysRemaining: number;
  /**
   * The VAT period's effective lifecycle state (#303). `open` means the period
   * is NOT yet closed — its figures are provisional and a momsangivelse cannot
   * be filed for it. `closed`/`reported` means the figures are final.
   */
  periodStatus: "open" | "closed" | "reported";
  /**
   * True only when the momsangivelse is filing-ready — i.e. the period is
   * closed or reported. The terminal `vat momsangivelse` refuses an open
   * period, so the cockpit must not present the rubrics as a ready-to-file
   * momsangivelse unless this is true (#303).
   */
  momsangivelseReady: boolean;
  /** The full SKAT TastSelv momsangivelse rubrics for the period. */
  rubrikker: VatRubrikker;
};

export type VatResponse = {
  ok: true;
  vat: CompanyVat;
};
