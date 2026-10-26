import { addDkk, roundDkk, subtractDkk } from "./money";
import type { VatPeriodReport } from "./vat";

/**
 * The ordinary Danish TastSelv VAT form, not a reporting convenience shape.
 * Every amount is a signed whole number of DKK. `momsIAlt` is calculated
 * from these submitted values; raw ledger values remain in `VatPeriodReport`.
 */
export type VatRubric = {
  salgsmoms: number;
  kobsmoms: number;
  momsAfVarekobUdland: number;
  momsAfYdelseskobUdland: number;
  rubrikAVarer: number;
  rubrikAYdelser: number;
  rubrikBVarerEuSalesList: number;
  rubrikBVarerIkkeEuSalesList: number;
  rubrikBYdelser: number;
  rubrikC: number;
  olieOgFlaskegasafgift: number;
  elafgift: number;
  naturgasOgBygasafgift: number;
  kulafgift: number;
  co2Afgift: number;
  vandafgift: number;
  momsIAlt: number;
  /** `momsIAlt - raw netVatPayable`, retained for filing reconciliation. */
  wholeKronerDifferenceDkk: number;
};

const formKeys = [
  "salgsmoms", "kobsmoms", "momsAfVarekobUdland", "momsAfYdelseskobUdland",
  "rubrikAVarer", "rubrikAYdelser", "rubrikBVarerEuSalesList",
  "rubrikBVarerIkkeEuSalesList", "rubrikBYdelser", "rubrikC",
  "olieOgFlaskegasafgift", "elafgift", "naturgasOgBygasafgift", "kulafgift",
  "co2Afgift", "vandafgift",
] as const;

export type VatFormInput = Pick<VatRubric, (typeof formKeys)[number]>;

/** Section 57: disregard øre by truncating each field towards zero. */
export function wholeKroner(value: number): number {
  return Number.isFinite(value) ? Math.trunc(value) : 0;
}

export function finalizeVatForm(raw: VatFormInput, rawNetVatPayable = 0): VatRubric {
  const fields = Object.fromEntries(formKeys.map((key) => [key, wholeKroner(raw[key])])) as VatFormInput;
  const refunds = fields.olieOgFlaskegasafgift + fields.elafgift + fields.naturgasOgBygasafgift
    + fields.kulafgift + fields.co2Afgift + fields.vandafgift;
  const momsIAlt = fields.salgsmoms + fields.momsAfVarekobUdland + fields.momsAfYdelseskobUdland
    - fields.kobsmoms - refunds;
  return { ...fields, momsIAlt, wholeKronerDifferenceDkk: roundDkk(momsIAlt - rawNetVatPayable) };
}

export function emptyVatRubric(): VatRubric {
  return finalizeVatForm({
    salgsmoms: 0, kobsmoms: 0, momsAfVarekobUdland: 0, momsAfYdelseskobUdland: 0,
    rubrikAVarer: 0, rubrikAYdelser: 0, rubrikBVarerEuSalesList: 0,
    rubrikBVarerIkkeEuSalesList: 0, rubrikBYdelser: 0, rubrikC: 0,
    olieOgFlaskegasafgift: 0, elafgift: 0, naturgasOgBygasafgift: 0,
    kulafgift: 0, co2Afgift: 0, vandafgift: 0,
  });
}

/**
 * Canonical form projection. B fields intentionally remain zero here: the
 * historical aggregate foreignReverseChargeSalesBase has no legally useful
 * goods/services/EU-sales-list provenance. `buildVatFiling` blocks a filing
 * with such an amount until an explicit evidence-backed classification exists.
 */
export function projectVatRubric(report: VatPeriodReport): VatRubric {
  const momsAfVarekobUdland = report.euGoodsAcquisitionPurchaseBase > 0
    ? report.euGoodsAcquisitionOutputVat : 0;
  const momsAfYdelseskobUdland = subtractDkk(report.reverseChargePurchaseOutputVat, momsAfVarekobUdland);
  const salgsmoms = subtractDkk(report.outputVat, addDkk(momsAfVarekobUdland, momsAfYdelseskobUdland));
  return finalizeVatForm({
    salgsmoms,
    kobsmoms: report.inputVat,
    momsAfVarekobUdland,
    momsAfYdelseskobUdland,
    rubrikAVarer: report.euGoodsAcquisitionPurchaseBase,
    rubrikAYdelser: report.reverseChargePurchaseBase,
    rubrikBVarerEuSalesList: 0,
    rubrikBVarerIkkeEuSalesList: 0,
    rubrikBYdelser: 0,
    rubrikC: addDkk(report.exemptSalesBase, report.domesticReverseChargeSalesBase),
    olieOgFlaskegasafgift: 0,
    elafgift: 0,
    naturgasOgBygasafgift: 0,
    kulafgift: 0,
    co2Afgift: 0,
    vandafgift: 0,
  }, report.netVatPayable);
}
