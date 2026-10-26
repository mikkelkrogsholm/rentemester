import type { CompanyVatRegistered, CompanyVatNotRegistered } from "../../lib/types";
import { STATEMENT_COMPANY, STATEMENT_FISCAL_YEARS } from "./_shared";

/**
 * The fixture builds a VAT-registered company by default — the discriminated
 * `vatRegistered: true` variant. Tests that need to render the
 * "ikke momsregistreret" card use `vatNotRegistered()` below.
 */
export function vat(over: Partial<CompanyVatRegistered> = {}): CompanyVatRegistered {
  return {
    slug: "acme-aps",
    selectedYear: "2026",
    archived: false,
    company: STATEMENT_COMPANY,
    fiscalYears: STATEMENT_FISCAL_YEARS,
    vatRegistered: true,
    // A past, already-ended quarter (#301): closing it is the normal flow with
    // no "period not over yet" warning. Tests that exercise the future-end
    // warning override `periodEnd` with a date in the future.
    periodStart: "2026-01-01",
    periodEnd: "2026-03-31",
    periodLabel: "Q1 2026",
    outputVat: 4457,
    outputVatAdjustment: 0,
    inputVat: 1086,
    payable: 3371,
    deadline: "2026-06-01",
    daysRemaining: 30,
    // #303: the VAT period's effective lifecycle state. The fixture defaults to
    // an OPEN, not-yet-filing-ready period — the historical pre-#303 behaviour
    // — so the "Luk momsperiode" action is offered. Tests that need a closed
    // (reopenable) or reported period override these two fields.
    periodStatus: "open",
    momsangivelseReady: false,
    vatReportErrors: [],
    vatReportWarnings: [],
    rubrikker: {
      salgsmoms: 4207,
      momsAfVarekobUdland: 0,
      momsAfYdelseskobUdland: 250,
      kobsmoms: 1086,
      rubrikAVarer: 0,
      rubrikAYdelser: 1000,
      rubrikBVarerEuSalesList: 0,
      rubrikBVarerIkkeEuSalesList: 0,
      rubrikBYdelser: 0,
      rubrikC: 0,
      olieOgFlaskegasafgift: 0,
      elafgift: 0,
      naturgasOgBygasafgift: 0,
      kulafgift: 0,
      co2Afgift: 0,
      vandafgift: 0,
      momsIAlt: 3371,
      wholeKronerDifferenceDkk: 0,
    },
    ...over,
  };
}

/** The not-VAT-registered variant — the cockpit renders an explanation. */
export function vatNotRegistered(
  over: Partial<CompanyVatNotRegistered> = {},
): CompanyVatNotRegistered {
  return {
    slug: "holding-aps",
    selectedYear: "2026",
    archived: false,
    company: STATEMENT_COMPANY,
    fiscalYears: STATEMENT_FISCAL_YEARS,
    vatRegistered: false,
    periodLabel: "Ikke momsregistreret",
    ...over,
  };
}
