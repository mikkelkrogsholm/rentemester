// Dashboard + Overblik wire types.

import type { ExceptionRow } from "./exceptions";
import type { FiscalYearEntry } from "./common";

export type InvoiceRow = {
  invoiceNumber: string;
  customerName?: string;
  issueDate?: string;
  dueDate?: string;
  total?: number;
  openBalance: number;
  [key: string]: unknown;
};

export type AuditLogRow = {
  occurredAt?: string;
  action?: string;
  summary?: string;
  [key: string]: unknown;
};

export type CompanyDashboard = {
  slug: string;
  asOf: string;
  company: {
    name: string;
    cvr: string | null;
    country: string;
    currency: string;
    fiscalYearStartMonth: number | string;
    fiscalYearLabelStrategy: string;
  };
  invoices: { count: number; openTotal: number; rows: InvoiceRow[] };
  overdueInvoices: { count: number; rows: InvoiceRow[] };
  unlinkedBank: { count: number };
  exceptions: { count: number; rows: ExceptionRow[] };
  vat: {
    periodStart: string;
    periodEnd: string;
    netVatPayable: number;
    daysRemaining: number;
    errors: unknown[];
  };
  backup: {
    backupsFound: boolean;
    latestBackupAt: string | null;
    daysSinceLatestBackup: number | null;
    hasActivitySinceBackup: boolean;
  };
  audit: { ok: boolean; entryCount: number; firstError: unknown };
  recentActivity: AuditLogRow[];
};

export type DashboardResponse = {
  ok: true;
  dashboard: CompanyDashboard;
};

// --- overview (GET /api/companies/:slug/overview?year=) -------------------

export type OverviewMonth = {
  month: number;
  label: string;
  income: number;
  expense: number;
};

export type OverviewExceptionRow = {
  id: number;
  type: string;
  severity: string;
  message: string;
  /**
   * The concrete "what the owner must do" guidance for this exception — the
   * same `requiredAction` the CLI's `exceptions list` shows. Null when the
   * exception carries no recorded action.
   */
  requiredAction: string | null;
};

/**
 * One grouped exception line for the "Opgaver" card — every open exception of
 * one `type` collapsed into a single Danish, actionable summary line.
 */
export type OverviewExceptionGroup = {
  type: string;
  count: number;
  severity: "low" | "medium" | "high";
  /** Danish one-liner, e.g. "362 banktransaktioner mangler afstemning". */
  label: string;
  /** The cockpit sub-view this group links to (e.g. "bank"); null when none. */
  link: string | null;
};

export type OverviewRecentEntry = {
  id: number;
  entryNo: string;
  date: string;
  text: string;
  amount: number;
};

/**
 * The "Overblik" payload. All money fields are kroner (DKK with decimals) —
 * use `formatKroner`, not `formatCurrency` (which expects minor units).
 */
/** The Overblik VAT block — null for an archived year (no VAT data exists). */
export type OverviewVat = {
  periodStart: string;
  periodEnd: string;
  periodLabel: string;
  /**
   * Genuine output VAT on sales for the period, kroner — gross, before any
   * bad-debt relief. The bad-debt adjustment is surfaced separately so a
   * write-off never drags the salgsmoms headline negative (#271).
   */
  outputVat: number;
  /** Bad-debt (debitortab) output-VAT adjustment, ≤ 0; 0 when none, kroner. */
  outputVatAdjustment: number;
  /** 25% of the standard-rated purchase base for the period, kroner. */
  inputVat: number;
  /** outputVat + outputVatAdjustment − inputVat; positive is payable, kroner. */
  payable: number;
  /** The statutory VAT filing/payment deadline, YYYY-MM-DD. */
  deadline: string;
  /** Signed countdown from today to the deadline; negative once passed. */
  daysRemaining: number;
};

export type CompanyOverview = {
  slug: string;
  selectedYear: string;
  /** True when the figures are derived from the #197 archive, not the ledger. */
  archived: boolean;
  /** The archive's source system (e.g. "dinero") when archived, else null. */
  archivedSource: string | null;
  company: {
    name: string;
    cvr: string | null;
    country: string;
    currency: string;
    fiscalYearStartMonth: number | string;
    fiscalYearLabelStrategy: string;
  };
  fiscalYears: FiscalYearEntry[];
  profitAndLoss: {
    omsaetning: number;
    udgifter: number;
    resultat: number;
    months: OverviewMonth[];
  };
  bank: {
    /** Booked ledger balance of the bank/cash accounts at the year end, kroner. */
    balance: number;
    /** Actual statement balance (latest imported `balance_after`), kroner; null when unknown. */
    actualBalance: number | null;
    /** balance − actualBalance; the unreconciled gap, kroner; null when unknown. */
    difference: number | null;
  };
  /** Money owed TO the company — open issued-invoice balances at year end. */
  receivables: {
    /** Number of issued invoices still carrying an open balance. */
    openCount: number;
    /** Sum of the open balances, kroner. */
    openTotal: number;
  };
  /** The half-yearly VAT position; null for an archived year. */
  vat: OverviewVat | null;
  exceptions: {
    count: number;
    rows: OverviewExceptionRow[];
    groups: OverviewExceptionGroup[];
  };
  recentEntries: OverviewRecentEntry[];
  /** Transaction date of the most recent posted entry; null when none. */
  lastPostedDate: string | null;
  /** Key ratios as fractions (0–1); each null when its denominator is zero. */
  keyFigures: {
    /** Resultat ÷ omsætning. */
    bruttomargin: number | null;
    /** Egenkapital ÷ balancesum. */
    egenkapitalandel: number | null;
  };
};

export type OverviewResponse = {
  ok: true;
  overview: CompanyOverview;
};
