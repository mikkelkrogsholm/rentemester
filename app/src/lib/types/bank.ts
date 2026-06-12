// Bank wire types: per-year Bank view, cash-flow view, and the read-only
// Bankkonti + CSV-mapping-profile registry (#345).

import type { FiscalYearEntry, StatementCompany } from "./common";

// --- bank / Bank (GET .../bank?year=) -------------------------------------

export type BankAccountInfo = {
  id: number;
  name: string;
  bankName: string | null;
  accountNo: string | null;
  ledgerAccountNo: string | null;
};

export type BankTransactionRow = {
  id: number;
  date: string;
  text: string;
  amount: number;
  /** Running balance from the import, kroner; null when the export omits it. */
  runningBalance: number | null;
  reconciliationStatus: "matched" | "unmatched";
  journalEntryNo: string | null;
};

export type CompanyBank = {
  slug: string;
  selectedYear: string;
  archived: boolean;
  company: StatementCompany;
  fiscalYears: FiscalYearEntry[];
  periodStart: string;
  periodEnd: string;
  accounts: BankAccountInfo[];
  /** Booked ledger balance of the bank/cash accounts at the year end, kroner. */
  bookedBalance: number;
  /** Actual statement balance (latest imported `balance_after`), kroner; null when unknown. */
  actualBalance: number | null;
  /** bookedBalance − actualBalance; the unreconciled gap, kroner; null when unknown. */
  difference: number | null;
  /**
   * Why `actualBalance` is or is not known (#305):
   *  - `known`             — a statement balance was imported and is shown;
   *  - `no-balance-column` — transactions WERE imported, but the CSV carried
   *    no balance column, so the bank saldo is unknown (NOT "not imported");
   *  - `none`              — no bank statement has been imported at all.
   */
  bankStatementStatus: "known" | "no-balance-column" | "none";
  transactions: BankTransactionRow[];
  matchedCount: number;
  unmatchedCount: number;
};

export type BankResponse = {
  ok: true;
  bank: CompanyBank;
};

// --- cash flow / Likviditet (GET .../cashflow?year=) ----------------------

export type CashflowMonth = {
  /** 1–12. */
  month: number;
  label: string;
  /** Money in: sum of positive bank-transaction amounts, kroner. */
  indbetalinger: number;
  /** Money out: sum of negative amounts as a positive figure, kroner. */
  udbetalinger: number;
  /** indbetalinger − udbetalinger, kroner. */
  netto: number;
};

export type CashflowBalancePoint = {
  date: string;
  /** The imported running balance at this point, kroner. */
  balance: number;
};

export type CompanyCashflow = {
  slug: string;
  selectedYear: string;
  archived: boolean;
  company: StatementCompany;
  fiscalYears: FiscalYearEntry[];
  periodStart: string;
  periodEnd: string;
  /** False when the company has no bank transactions in the year. */
  hasTransactions: boolean;
  months: CashflowMonth[];
  /** The real bank-balance trajectory, oldest-first. */
  balanceSeries: CashflowBalancePoint[];
  /** Actual balance the day before the year starts, kroner; null when unknown. */
  openingBalance: number | null;
  /** Actual balance at the year end, kroner; null when unknown. */
  closingBalance: number | null;
  /** Total money in across the year, kroner. */
  totalIn: number;
  /** Total money out across the year, kroner. */
  totalOut: number;
};

export type CashflowResponse = {
  ok: true;
  cashflow: CompanyCashflow;
};

// ---------------------------------------------------------------------------
// #345 — Bankkonti + CSV-mapping-profiler.
// ---------------------------------------------------------------------------

export type BankAccount = {
  id: number;
  slug: string;
  name: string;
  bankName: string | null;
  registrationNo: string | null;
  accountNo: string | null;
  iban: string | null;
  currency: string;
  ledgerAccountNo: string | null;
  active: boolean;
  createdAt: string;
};

export type BankImportProfile = {
  name: string;
  bankName?: string;
  separator?: string;
  encoding?: string;
  dateOrder?: "dmy" | "mdy" | "ymd" | "iso";
  columns?: Record<string, string>;
};

export type CompanyBankAccounts = {
  slug: string;
  company: {
    name: string;
    cvr: string | null;
    country: string;
    currency: string;
  };
  accounts: BankAccount[];
  profiles: BankImportProfile[];
};

export type BankAccountsResponse = {
  ok: true;
  bankAccounts: CompanyBankAccounts;
};
