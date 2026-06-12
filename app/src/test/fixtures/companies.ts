import type {
  CompanyArchiveYear,
  CompanyMultiYear,
  CompanySettings,
  CompanySummary,
  FiscalYearEntry,
} from "../../lib/types";
import { STATEMENT_COMPANY, STATEMENT_FISCAL_YEARS } from "./_shared";

export function summary(over: Partial<CompanySummary> = {}): CompanySummary {
  return {
    slug: "acme-aps",
    name: "Acme ApS",
    cvr: "DK12345678",
    archived: false,
    ledgerMissing: false,
    fiscalYear: "2026",
    resultat: 13234.82,
    omsaetning: 17829.02,
    actualBankBalance: 23654.75,
    vat: { payable: 3371.2, deadline: "2026-09-01", daysRemaining: 103 },
    openTaskCount: 0,
    taskGroups: [],
    auditChainOk: true,
    openInvoiceCount: 0,
    openInvoiceTotal: 0,
    overdueInvoiceCount: 0,
    unlinkedBankCount: 0,
    openExceptionCount: 0,
    netVatPayable: 3371.2,
    ...over,
  };
}

/** The fiscal-years payload — drives the Arkiv view's year selector. */
export function fiscalYears(
  over: FiscalYearEntry[] = STATEMENT_FISCAL_YEARS,
): { slug: string; years: FiscalYearEntry[] } {
  return { slug: "acme-aps", years: over };
}

export function archive(
  over: Partial<CompanyArchiveYear> = {},
): CompanyArchiveYear {
  return {
    slug: "acme-aps",
    company: STATEMENT_COMPANY,
    year: "2025",
    sourceSystem: "dinero",
    importedAt: "2026-01-10T09:00:00.000Z",
    saldoBalance: [
      { accountNo: "1000", name: "Omsætning", amount: -42000 },
      { accountNo: "3000", name: "Vareforbrug", amount: 12500 },
      { accountNo: "55000", name: "Bank", amount: 29500 },
    ],
    postings: { count: 84, grossTotal: 168000 },
    ...over,
  };
}

export function multiYear(
  over: Partial<CompanyMultiYear> = {},
): CompanyMultiYear {
  return {
    slug: "acme-aps",
    company: STATEMENT_COMPANY,
    years: [
      {
        year: "2023",
        source: "archive",
        omsaetning: 31000,
        udgifter: 9000,
        resultat: 22000,
        balancesum: 64000,
        egenkapital: 41000,
        bruttomargin: 22000 / 31000,
        egenkapitalandel: 41000 / 64000,
      },
      {
        year: "2024",
        source: "archive",
        omsaetning: 38000,
        udgifter: 11000,
        resultat: 27000,
        balancesum: 72000,
        egenkapital: 48000,
        bruttomargin: 27000 / 38000,
        egenkapitalandel: 48000 / 72000,
      },
      {
        year: "2025",
        source: "archive",
        omsaetning: 42000,
        udgifter: 12500,
        resultat: 29500,
        balancesum: 81000,
        egenkapital: 56000,
        bruttomargin: 29500 / 42000,
        egenkapitalandel: 56000 / 81000,
      },
      {
        year: "2026",
        source: "live",
        omsaetning: 17829.02,
        udgifter: 4594.2,
        resultat: 13234.82,
        balancesum: 90000,
        egenkapital: 62000,
        bruttomargin: 13234.82 / 17829.02,
        egenkapitalandel: 62000 / 90000,
      },
    ],
    ...over,
  };
}

/** The full company settings row, backing GET /api/companies/:slug/company. */
export function companySettings(
  over: Partial<CompanySettings> = {},
): CompanySettings {
  return {
    id: 1,
    name: "Acme ApS",
    country: "DK",
    currency: "DKK",
    cvr: "DK12345678",
    fiscalYearStartMonth: 1,
    fiscalYearLabelStrategy: "end-year",
    address: null,
    postalCode: null,
    city: null,
    companyForm: null,
    industryCode: null,
    industryText: null,
    cvrStatus: null,
    auditWaived: null,
    cvrSyncedAt: null,
    // #300: the VAT settlement cadence — defaults to the historical `quarter`.
    vatPeriodType: "quarter",
    payment: null,
    ...over,
  };
}
