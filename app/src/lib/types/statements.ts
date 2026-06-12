// Financial statement wire types: income statement, balance sheet, trial
// balance, multi-year overview and the read-only #197 archive.
//
// All money fields below are kroner (DKK with decimals) — use `formatKroner`.

import type { FiscalYearEntry, StatementCompany } from "./common";

// --- income statement (GET .../income-statement?year=) --------------------

export type IncomeStatementLine = {
  accountNo: string;
  name: string;
  amount: number;
  /** The same account's amount in the prior calendar year, kroner. */
  priorAmount: number;
};

export type CompanyIncomeStatement = {
  slug: string;
  selectedYear: string;
  /** True when the figures are derived from the #197 archive. */
  archived: boolean;
  /** The archive's source system when archived, else null. */
  archivedSource: string | null;
  company: StatementCompany;
  fiscalYears: FiscalYearEntry[];
  income: IncomeStatementLine[];
  expense: IncomeStatementLine[];
  totalIncome: number;
  totalExpense: number;
  priorTotalIncome: number;
  priorTotalExpense: number;
  result: number;
  priorResult: number;
};

export type IncomeStatementResponse = {
  ok: true;
  incomeStatement: CompanyIncomeStatement;
};

// --- balance sheet (GET .../balance?year=) --------------------------------

export type BalanceLine = {
  accountNo: string;
  name: string;
  amount: number;
  /**
   * The same account's balance one fiscal year earlier, kroner. `null` when
   * there is no prior year in the ledger — the view renders «—». ÅRL § 24
   * kræver sammenligningstal på balancen for regnskabsklasse B.
   */
  priorAmount: number | null;
};

export type BalanceSection = {
  lines: BalanceLine[];
  total: number;
  /** The section total one fiscal year earlier, kroner. `null` when none. */
  priorTotal: number | null;
};

export type CompanyBalance = {
  slug: string;
  selectedYear: string;
  /** True when the figures are derived from the #197 archive. */
  archived: boolean;
  /** The archive's source system when archived, else null. */
  archivedSource: string | null;
  company: StatementCompany;
  fiscalYears: FiscalYearEntry[];
  asOfDate: string;
  assets: BalanceSection;
  liabilities: BalanceSection;
  /**
   * Equity including the fiscal year's result: the result is folded in as an
   * "Årets resultat" line so `equity.total` is the equity figure an owner
   * reads — and the same number as the Flerårsoversigt's `egenkapital`.
   */
  equity: BalanceSection;
  /** The fiscal year's result, also folded into the equity section. */
  periodResult: number;
  totalAssets: number;
  totalLiabilitiesAndEquity: number;
  /**
   * The prior year's `totalLiabilitiesAndEquity`, kroner. `null` when no
   * prior year is available — the view renders «—» in that cell. Mirrors the
   * resultatopgørelsens prior-year footer.
   */
  priorTotalLiabilitiesAndEquity: number | null;
  balanced: boolean;
};

export type BalanceResponse = {
  ok: true;
  balance: CompanyBalance;
};

// --- trial balance (GET .../trial-balance?year=) --------------------------

export type TrialBalanceRow = {
  accountNo: string;
  name: string;
  type: string;
  debit: number;
  credit: number;
  balance: number;
};

export type CompanyTrialBalance = {
  slug: string;
  selectedYear: string;
  /** True when the rows are the read-only #197 archived SaldoBalance. */
  archived: boolean;
  /** The archive's source system when archived, else null. */
  archivedSource: string | null;
  company: StatementCompany;
  fiscalYears: FiscalYearEntry[];
  periodStart: string;
  periodEnd: string;
  rows: TrialBalanceRow[];
  totalDebit: number;
  totalCredit: number;
  balanced: boolean;
};

export type TrialBalanceResponse = {
  ok: true;
  trialBalance: CompanyTrialBalance;
};

// --- archive / Arkiv (GET .../archive/:year) — cockpit-redesign it. 4 -------

export type ArchiveBalanceRow = {
  accountNo: string;
  name: string;
  /** Closing balance, kroner, exactly as the Dinero export stored it. */
  amount: number;
};

export type CompanyArchiveYear = {
  slug: string;
  company: StatementCompany;
  /** The archived fiscal-year label, e.g. "2025". */
  year: string;
  /** The accounting system the archive was exported from, e.g. "dinero". */
  sourceSystem: string;
  importedAt: string;
  /** The year's full SaldoBalance — every account's closing balance. */
  saldoBalance: ArchiveBalanceRow[];
  /** A summary of the archived Posteringer for the year. */
  postings: {
    count: number;
    /** Sum of the absolute posting amounts — the gross volume, kroner. */
    grossTotal: number;
  };
};

export type ArchiveResponse = {
  ok: true;
  archive: CompanyArchiveYear;
};

// --- multi-year / Flerårsoversigt (GET .../multi-year) — it. 4 --------------

export type MultiYearRow = {
  /** The fiscal-year label, e.g. "2025". */
  year: string;
  source: "live" | "archive";
  /** Income / omsætning for the year, kroner. */
  omsaetning: number;
  /** Expenses / udgifter for the year, kroner. */
  udgifter: number;
  /** Result (omsætning − udgifter), kroner. */
  resultat: number;
  /** Total assets (balancesum) at the year end, kroner. */
  balancesum: number;
  /** Equity (egenkapital incl. period result) at the year end, kroner. */
  egenkapital: number;
  /** Bruttomargin — resultat ÷ omsætning, a 0–1 fraction; null when no omsætning. */
  bruttomargin: number | null;
  /** Egenkapitalandel — egenkapital ÷ balancesum, a 0–1 fraction; null when balancesum is 0. */
  egenkapitalandel: number | null;
};

export type CompanyMultiYear = {
  slug: string;
  company: StatementCompany;
  /** Key figures per fiscal year, oldest→newest for charting a trend. */
  years: MultiYearRow[];
};

export type MultiYearResponse = {
  ok: true;
  multiYear: CompanyMultiYear;
};
