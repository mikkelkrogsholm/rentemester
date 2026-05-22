// Financial statements (#176): trial balance (saldobalance), profit & loss
// (resultatopgørelse) and balance sheet (balance) computed deterministically
// from the append-only ledger.
//
// All three functions are pure reads: they query `journal_entries`,
// `journal_lines` and `accounts` directly and never mutate the database, never
// call the wall clock, and produce byte-identical output for identical input.
// Money is integer øre internally (via src/core/money.ts) and surfaces as DKK
// numbers with 2 decimals.
//
// Reversals: a reversed entry and its reversal both remain in the ledger; each
// posting is counted in the period its transaction_date falls in. A reversal
// posted in the same period as the original therefore nets it to zero, exactly
// like the VAT report (src/core/vat.ts) treats period membership.

import type { Database } from "bun:sqlite";
import { classifyAccountSection } from "./account-classification";
import { isValidIsoDate as looksLikeIsoDate } from "./dates";
import { fromOre, toOre } from "./money";

// These are derived-report identifiers, not normative ledger rules. They are
// deliberately NOT in the DK-<area>-NNN rule-ID shape: a financial statement is
// a pure computation over already-validated postings, it enforces no rule of
// its own. The strings appear in `appliedRules` purely as provenance labels.
const TRIAL_BALANCE_REPORT_ID = "financial-statements:trial-balance";
const PROFIT_AND_LOSS_REPORT_ID = "financial-statements:profit-loss";
const BALANCE_SHEET_REPORT_ID = "financial-statements:balance-sheet";

/** Account types as constrained by the `accounts.type` CHECK in schema.sql. */
export type AccountType = "asset" | "liability" | "equity" | "income" | "expense" | "vat";

export type TrialBalanceAccount = {
  accountNo: string;
  name: string;
  type: AccountType;
  normalBalance: "debit" | "credit";
  /** Summed debit movement in the period, DKK. */
  debit: number;
  /** Summed credit movement in the period, DKK. */
  credit: number;
  /** Net movement, signed debit − credit, DKK. */
  balance: number;
};

export type TrialBalanceReport = {
  ok: boolean;
  appliedRules: string[];
  periodStart: string;
  periodEnd: string;
  accounts: TrialBalanceAccount[];
  totalDebit: number;
  totalCredit: number;
  /** True when totalDebit equals totalCredit (a well-formed double-entry set). */
  balanced: boolean;
  linesConsidered: number;
  journalEntryCount: number;
  errors: string[];
};

export type StatementLine = {
  accountNo: string;
  name: string;
  type: AccountType;
  /** Amount in the statement's natural sign convention, DKK. */
  amount: number;
};

export type ProfitAndLossReport = {
  ok: boolean;
  appliedRules: string[];
  periodStart: string;
  periodEnd: string;
  income: StatementLine[];
  expense: StatementLine[];
  totalIncome: number;
  totalExpense: number;
  /** totalIncome − totalExpense, DKK. Positive is a profit. */
  result: number;
  errors: string[];
};

export type BalanceSheetSection = {
  lines: StatementLine[];
  total: number;
};

export type BalanceSheetReport = {
  ok: boolean;
  appliedRules: string[];
  asOfDate: string;
  assets: BalanceSheetSection;
  liabilities: BalanceSheetSection;
  /** Equity excluding the un-closed period result. */
  equity: BalanceSheetSection;
  /** Income − expense up to and including asOfDate, carried into equity. */
  periodResult: number;
  totalAssets: number;
  /** liabilities + equity + periodResult. */
  totalLiabilitiesAndEquity: number;
  /** True when totalAssets equals totalLiabilitiesAndEquity. */
  balanced: boolean;
  errors: string[];
};

type LedgerLineRow = {
  account_no: string;
  account_name: string;
  account_type: AccountType;
  normal_balance: "debit" | "credit";
  debit_amount: number;
  credit_amount: number;
  entry_id: number;
};

/** øre-exact equality of two DKK amounts. */
function equalsOre(left: number, right: number): boolean {
  return toOre(left) === toOre(right);
}

function validatePeriod(periodStart: string, periodEnd: string): string[] {
  const errors: string[] = [];
  if (!looksLikeIsoDate(periodStart)) errors.push("periodStart must be YYYY-MM-DD");
  if (!looksLikeIsoDate(periodEnd)) errors.push("periodEnd must be YYYY-MM-DD");
  if (errors.length === 0 && periodStart > periodEnd) {
    errors.push("periodStart must be before or equal to periodEnd");
  }
  return errors;
}

/**
 * Fetch every journal line whose entry transaction_date falls within
 * [periodStart, periodEnd] inclusive, joined to its account. Ordered
 * deterministically by account number then entry/line id.
 */
function selectLinesInPeriod(db: Database, periodStart: string, periodEnd: string): LedgerLineRow[] {
  return db
    .query(
      `SELECT a.account_no AS account_no,
              a.name       AS account_name,
              a.type       AS account_type,
              a.normal_balance AS normal_balance,
              jl.debit_amount  AS debit_amount,
              jl.credit_amount AS credit_amount,
              je.id            AS entry_id
         FROM journal_entries je
         JOIN journal_lines jl ON jl.journal_entry_id = je.id
         JOIN accounts a       ON a.id = jl.account_id
        WHERE je.transaction_date >= ? AND je.transaction_date <= ?
        ORDER BY a.account_no ASC, je.id ASC, jl.id ASC`,
    )
    .all(periodStart, periodEnd) as LedgerLineRow[];
}

/**
 * Trial balance (saldobalance): per-account debit total, credit total and the
 * signed net balance for a period, listing only accounts that moved. The
 * report is balanced when total debit equals total credit.
 */
export function buildTrialBalance(
  db: Database,
  periodStart: string,
  periodEnd: string,
): TrialBalanceReport {
  const errors = validatePeriod(periodStart, periodEnd);
  if (errors.length > 0) {
    return {
      ok: false,
      appliedRules: [TRIAL_BALANCE_REPORT_ID],
      periodStart,
      periodEnd,
      accounts: [],
      totalDebit: 0,
      totalCredit: 0,
      balanced: false,
      linesConsidered: 0,
      journalEntryCount: 0,
      errors,
    };
  }

  const rows = selectLinesInPeriod(db, periodStart, periodEnd);

  // Accumulate debit/credit per account in integer øre to avoid float drift.
  type Acc = {
    accountNo: string;
    name: string;
    type: AccountType;
    normalBalance: "debit" | "credit";
    debitOre: bigint;
    creditOre: bigint;
  };
  const byAccount = new Map<string, Acc>();
  const entryIds = new Set<number>();
  let totalDebitOre = 0n;
  let totalCreditOre = 0n;

  for (const row of rows) {
    entryIds.add(row.entry_id);
    let acc = byAccount.get(row.account_no);
    if (!acc) {
      acc = {
        accountNo: row.account_no,
        name: row.account_name,
        type: row.account_type,
        normalBalance: row.normal_balance,
        debitOre: 0n,
        creditOre: 0n,
      };
      byAccount.set(row.account_no, acc);
    }
    const debitOre = toOre(Number(row.debit_amount ?? 0));
    const creditOre = toOre(Number(row.credit_amount ?? 0));
    acc.debitOre += debitOre;
    acc.creditOre += creditOre;
    totalDebitOre += debitOre;
    totalCreditOre += creditOre;
  }

  const accounts: TrialBalanceAccount[] = [...byAccount.values()]
    .sort((a, b) => (a.accountNo < b.accountNo ? -1 : a.accountNo > b.accountNo ? 1 : 0))
    .map((acc) => ({
      accountNo: acc.accountNo,
      name: acc.name,
      type: acc.type,
      normalBalance: acc.normalBalance,
      debit: fromOre(acc.debitOre),
      credit: fromOre(acc.creditOre),
      balance: fromOre(acc.debitOre - acc.creditOre),
    }));

  return {
    ok: true,
    appliedRules: [TRIAL_BALANCE_REPORT_ID],
    periodStart,
    periodEnd,
    accounts,
    totalDebit: fromOre(totalDebitOre),
    totalCredit: fromOre(totalCreditOre),
    balanced: totalDebitOre === totalCreditOre,
    linesConsidered: rows.length,
    journalEntryCount: entryIds.size,
    errors: [],
  };
}

/**
 * Profit & loss (resultatopgørelse): income minus expenses for a period.
 *
 * Income accounts are credit-normal — their statement amount is credit − debit.
 * Expense accounts are debit-normal — their statement amount is debit − credit.
 * The result is totalIncome − totalExpense.
 */
export function buildProfitAndLoss(
  db: Database,
  periodStart: string,
  periodEnd: string,
): ProfitAndLossReport {
  const errors = validatePeriod(periodStart, periodEnd);
  if (errors.length > 0) {
    return {
      ok: false,
      appliedRules: [PROFIT_AND_LOSS_REPORT_ID],
      periodStart,
      periodEnd,
      income: [],
      expense: [],
      totalIncome: 0,
      totalExpense: 0,
      result: 0,
      errors,
    };
  }

  const tb = buildTrialBalance(db, periodStart, periodEnd);

  const income: StatementLine[] = [];
  const expense: StatementLine[] = [];
  let totalIncomeOre = 0n;
  let totalExpenseOre = 0n;

  for (const acc of tb.accounts) {
    if (acc.type === "income") {
      // credit − debit (negate the debit-signed trial-balance balance).
      const amountOre = -toOre(acc.balance);
      totalIncomeOre += amountOre;
      income.push({ accountNo: acc.accountNo, name: acc.name, type: acc.type, amount: fromOre(amountOre) });
    } else if (acc.type === "expense") {
      // debit − credit (already the trial-balance balance sign).
      const amountOre = toOre(acc.balance);
      totalExpenseOre += amountOre;
      expense.push({ accountNo: acc.accountNo, name: acc.name, type: acc.type, amount: fromOre(amountOre) });
    }
  }

  return {
    ok: true,
    appliedRules: [PROFIT_AND_LOSS_REPORT_ID],
    periodStart,
    periodEnd,
    income,
    expense,
    totalIncome: fromOre(totalIncomeOre),
    totalExpense: fromOre(totalExpenseOre),
    result: fromOre(totalIncomeOre - totalExpenseOre),
    errors: [],
  };
}

/**
 * Balance sheet (balance): assets, liabilities and equity at a date.
 *
 * The sheet aggregates every posting with transaction_date <= asOfDate. The
 * ledger is double-entry so total debit equals total credit; therefore the
 * debit-signed net of all balance-sheet accounts equals minus the debit-signed
 * net of all income/expense accounts, i.e. the period result. Since no closing
 * entry has moved the result into a retained-earnings account, it is surfaced
 * explicitly as `periodResult` and added to the equity side so the statement
 * balances: assets = liabilities + equity + periodResult.
 *
 * Sign conventions in the returned lines:
 *  - assets    : debit − credit (debit-normal positive)
 *  - liabilities/equity : credit − debit (credit-normal positive)
 *  - `vat` accounts are placed by their `normal_balance`: a credit-normal VAT
 *    account (output VAT, a payable) sits under liabilities; a debit-normal VAT
 *    account (input VAT, a receivable) sits under assets.
 */
export function buildBalanceSheet(db: Database, asOfDate: string): BalanceSheetReport {
  const errors: string[] = [];
  if (!looksLikeIsoDate(asOfDate)) errors.push("asOfDate must be YYYY-MM-DD");
  if (errors.length > 0) {
    return {
      ok: false,
      appliedRules: [BALANCE_SHEET_REPORT_ID],
      asOfDate,
      assets: { lines: [], total: 0 },
      liabilities: { lines: [], total: 0 },
      equity: { lines: [], total: 0 },
      periodResult: 0,
      totalAssets: 0,
      totalLiabilitiesAndEquity: 0,
      balanced: false,
      errors,
    };
  }

  // The earliest possible ISO date as an inclusive lower bound; the trial
  // balance helper handles the upper bound and the per-account aggregation.
  const tb = buildTrialBalance(db, "0000-01-01", asOfDate);

  const assets: StatementLine[] = [];
  const liabilities: StatementLine[] = [];
  const equity: StatementLine[] = [];
  let totalAssetsOre = 0n;
  let totalLiabilitiesOre = 0n;
  let totalEquityOre = 0n;
  let periodResultOre = 0n;

  for (const acc of tb.accounts) {
    const debitSignedOre = toOre(acc.balance); // debit − credit
    // The statement section an account belongs to — the shared classification
    // (#321), so the live sheet and the archive-aware views never disagree.
    const section = classifyAccountSection(acc.type, acc.normalBalance);
    if (section === "asset") {
      totalAssetsOre += debitSignedOre;
      assets.push({ accountNo: acc.accountNo, name: acc.name, type: acc.type, amount: fromOre(debitSignedOre) });
    } else if (section === "liability") {
      const creditSignedOre = -debitSignedOre;
      totalLiabilitiesOre += creditSignedOre;
      liabilities.push({ accountNo: acc.accountNo, name: acc.name, type: acc.type, amount: fromOre(creditSignedOre) });
    } else if (section === "equity") {
      const creditSignedOre = -debitSignedOre;
      totalEquityOre += creditSignedOre;
      equity.push({ accountNo: acc.accountNo, name: acc.name, type: acc.type, amount: fromOre(creditSignedOre) });
    } else if (section === "income") {
      // credit − debit contributes positively to the result.
      periodResultOre += -debitSignedOre;
    } else if (section === "expense") {
      periodResultOre -= debitSignedOre;
    }
  }

  const totalAssets = fromOre(totalAssetsOre);
  const totalLiabilitiesAndEquityOre = totalLiabilitiesOre + totalEquityOre + periodResultOre;
  const totalLiabilitiesAndEquity = fromOre(totalLiabilitiesAndEquityOre);

  return {
    ok: true,
    appliedRules: [BALANCE_SHEET_REPORT_ID],
    asOfDate,
    assets: { lines: assets, total: totalAssets },
    liabilities: { lines: liabilities, total: fromOre(totalLiabilitiesOre) },
    equity: { lines: equity, total: fromOre(totalEquityOre) },
    periodResult: fromOre(periodResultOre),
    totalAssets,
    totalLiabilitiesAndEquity,
    balanced: equalsOre(totalAssets, totalLiabilitiesAndEquity),
    errors: [],
  };
}
