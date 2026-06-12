// Company portfolio + per-company settings/profile wire types.

import type { VatPeriodType } from "./common";

export type CompanyEntry = {
  slug: string;
  name: string;
  createdAt: string;
  archived: boolean;
};

export type CompanyListResponse = {
  ok: true;
  workspace: string;
  count: number;
  companies: CompanyEntry[];
};

/** One grouped open-task line on a portfolio card. */
export type ExceptionGroup = {
  type: string;
  count: number;
  severity: "low" | "medium" | "high";
  label: string;
  link: string | null;
};

/** The VAT block on a portfolio card — null when no VAT period is known. */
export type CompanyVatSummary = {
  payable: number;
  deadline: string;
  daysRemaining: number;
};

export type CompanySummary = {
  slug: string;
  name: string;
  cvr: string | null;
  archived: boolean;
  ledgerMissing: boolean;
  /** The fiscal year these figures cover, e.g. "2026"; null when unknown. */
  fiscalYear: string | null;
  /** Year-to-date result (resultat), kroner. */
  resultat: number;
  /** Year-to-date revenue (omsætning), kroner. */
  omsaetning: number;
  /** Actual bank balance from the imported statement, kroner; null if unknown. */
  actualBankBalance: number | null;
  /** Current half-year VAT position + deadline; null when unknown. */
  vat: CompanyVatSummary | null;
  /** Open tasks across the company. */
  openTaskCount: number;
  /** The open tasks grouped into Danish summary lines. */
  taskGroups: ExceptionGroup[];
  auditChainOk: boolean;
  // Legacy fields retained for older consumers.
  openInvoiceCount: number;
  openInvoiceTotal: number;
  overdueInvoiceCount: number;
  unlinkedBankCount: number;
  openExceptionCount: number;
  netVatPayable: number;
};

export type PortfolioOverview = {
  workspace: string;
  asOf: string;
  companyCount: number;
  /** Workspace-wide roll-up — how the whole portfolio is doing. */
  rollup: {
    resultat: number;
    liquidity: number;
    vatPayable: number;
    openTaskCount: number;
  };
  totals: {
    openInvoiceCount: number;
    openInvoiceTotal: number;
    overdueInvoiceCount: number;
    unlinkedBankCount: number;
    openExceptionCount: number;
    netVatPayable: number;
  };
  companies: CompanySummary[];
};

export type PortfolioResponse = {
  ok: true;
  portfolio: PortfolioOverview;
};

export type CreateCompanyInput = {
  name: string;
  slug?: string;
  cvr?: string;
  fiscalYearStartMonth?: string;
  fiscalYearLabelStrategy?: string;
  /** The VAT settlement cadence (#300). Defaults to `quarter` server-side. */
  vatPeriodType?: VatPeriodType;
  /** Optional bank/payment details — sets up the primary bank account (#284). */
  payment?: CompanyPaymentInput;
};

export type UpdateCompanyInput = {
  name?: string;
  archived?: boolean;
};

/** Optional bank fields on the create-company form (#284). */
export type CompanyPaymentInput = {
  bankName?: string;
  registrationNo?: string;
  accountNo?: string;
  iban?: string;
};

/**
 * The editable company profile fields (#284) — what `PATCH .../company` accepts.
 * Only the fields present are changed; the rest keep their current value.
 */
export type CompanyProfileInput = {
  name?: string;
  cvr?: string;
  address?: string;
  postalCode?: string;
  city?: string;
  paymentTermsDays?: number;
  /** The VAT settlement cadence (#300) — month / quarter / half-year. */
  vatPeriodType?: VatPeriodType;
  payment?: CompanyPaymentInput;
};

// --- company settings + CVR sync (GET .../company, POST .../sync-cvr) -------

/**
 * The company's payment/bank details — the primary bank account every issued
 * invoice's payment block reads from. Null on `CompanySettings` when no bank
 * account is configured yet.
 */
export type CompanyPaymentDetails = {
  bankName: string | null;
  registrationNo: string | null;
  accountNo: string | null;
  iban: string | null;
};

/** The full companies row, including CVR-register stamdata + bank details. */
export type CompanySettings = {
  id: number;
  name: string;
  country: string;
  currency: string;
  cvr: string | null;
  fiscalYearStartMonth: number;
  fiscalYearLabelStrategy: string;
  address: string | null;
  postalCode: string | null;
  city: string | null;
  companyForm: string | null;
  industryCode: string | null;
  industryText: string | null;
  cvrStatus: string | null;
  auditWaived: boolean | null;
  /** ISO timestamp the CVR stamdata was last synced; null when never. */
  cvrSyncedAt: string | null;
  /** The VAT settlement cadence the company is registered for with SKAT (#300). */
  vatPeriodType: VatPeriodType;
  /** The company's own bank/payment details; null when none is configured. */
  payment: CompanyPaymentDetails | null;
};

export type CompanySettingsResponse = {
  ok: true;
  company: CompanySettings;
};

export type CvrManagementMember = { name: string; role: string };

/** A normalised CVR-register snapshot for one company. */
export type CvrCompanyInfo = {
  cvr: string;
  name: string;
  address: string | null;
  postalCode: string | null;
  city: string | null;
  municipalityCode: number | null;
  companyFormCode: number | null;
  companyFormShort: string | null;
  companyFormLong: string | null;
  status: string | null;
  industryCode: string | null;
  industryText: string | null;
  email: string | null;
  phone: string | null;
  website: string | null;
  startDate: string | null;
  fiscalYearStart: string | null;
  fiscalYearEnd: string | null;
  fiscalYearStartMonth: number | null;
  auditWaived: boolean | null;
  shareCapital: number | null;
  shareCapitalCurrency: string | null;
  employees: number | null;
  advertisingProtected: boolean;
  management: CvrManagementMember[];
};

/** The result of `POST /api/companies/:slug/sync-cvr`. */
export type SyncCvrResult = {
  ok: boolean;
  cvr?: string;
  company?: CvrCompanyInfo;
  cached?: boolean;
  /** Names of the company fields whose value changed. */
  updatedFields?: string[];
  /** Configured vs. CVR-registered fiscal-year start month. */
  fiscalYearStartMonth?: { current: number; cvr: number | null; matches: boolean };
  errors: string[];
};

export type SyncCvrResponse = {
  ok: true;
  sync: SyncCvrResult;
};
