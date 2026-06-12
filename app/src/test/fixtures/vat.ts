import type { CompanyVat } from "../../lib/types";
import { STATEMENT_COMPANY, STATEMENT_FISCAL_YEARS } from "./_shared";

export function vat(over: Partial<CompanyVat> = {}): CompanyVat {
  return {
    slug: "acme-aps",
    selectedYear: "2026",
    archived: false,
    company: STATEMENT_COMPANY,
    fiscalYears: STATEMENT_FISCAL_YEARS,
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
    rubrikker: {
      salgsmoms: 4457,
      momsAfVarekobUdland: 0,
      momsAfYdelseskobUdland: 250,
      kobsmoms: 1086,
      momstilsvar: 3621,
      rubrikA: 1000,
      rubrikB: 0,
      rubrikC: 0,
    },
    ...over,
  };
}
