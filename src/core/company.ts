import type { Database } from "bun:sqlite";

export type FiscalYearLabelStrategy = "end-year" | "start-year" | "span";

export type CompanySettings = {
  id: number;
  name: string;
  country: string;
  currency: string;
  cvr: string | null;
  fiscalYearStartMonth: number;
  fiscalYearLabelStrategy: FiscalYearLabelStrategy;
};

const DEFAULT_COMPANY_SETTINGS: CompanySettings = {
  id: 1,
  name: "Rentemester company",
  country: "DK",
  currency: "DKK",
  cvr: null,
  fiscalYearStartMonth: 1,
  fiscalYearLabelStrategy: "end-year",
};

export function normalizeCvr(value?: string | null): string | null {
  const trimmed = value?.trim();
  if (!trimmed) return null;
  const stripped = trimmed.toUpperCase().replace(/^DK/, "");
  if (!/^\d{8}$/.test(stripped)) {
    throw new Error(`CVR must be 8 digits (optionally prefixed with DK), got: ${value}`);
  }
  return `DK${stripped}`;
}

export function normalizeFiscalYearStartMonth(value?: number | string | null) {
  const month = typeof value === "string" ? Number(value) : value;
  if (!Number.isInteger(month) || (month ?? 0) < 1 || (month ?? 0) > 12) return null;
  return month;
}

export function normalizeFiscalYearLabelStrategy(value?: string | null): FiscalYearLabelStrategy | null {
  if (value === "end-year" || value === "start-year" || value === "span") return value;
  return null;
}

export function getCompanySettings(db: Database): CompanySettings {
  const row = db.query(
    `SELECT id, name, country, currency, cvr, fiscal_year_start_month, fiscal_year_label_strategy
       FROM companies
      ORDER BY id ASC
      LIMIT 1`
  ).get() as {
    id: number;
    name: string;
    country: string;
    currency: string;
    cvr: string | null;
    fiscal_year_start_month: number;
    fiscal_year_label_strategy: FiscalYearLabelStrategy;
  } | null;

  if (!row) return DEFAULT_COMPANY_SETTINGS;
  return {
    id: row.id,
    name: row.name,
    country: row.country,
    currency: row.currency,
    cvr: normalizeCvr(row.cvr),
    fiscalYearStartMonth: normalizeFiscalYearStartMonth(row.fiscal_year_start_month) ?? 1,
    fiscalYearLabelStrategy: normalizeFiscalYearLabelStrategy(row.fiscal_year_label_strategy) ?? "end-year",
  };
}
