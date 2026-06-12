/**
 * Budget per account per period, and the budget-vs-actual report.
 *
 * A *budget line* is the owner's expectation for one account in one calendar
 * month: a planned expense ("5000 kr. software in 2026-06") or an income
 * target ("20000 kr. revenue in 2026-06"). Budget lines are append-only — a
 * `setBudget` for a pair that already has a line inserts a NEW revision rather
 * than mutating; the highest-id row for an (account, period) pair is the
 * effective budget. The full revision history is therefore auditable, exactly
 * like the append-only ledger.
 *
 * The *budget-vs-actual* report is a pure deterministic read: for every
 * (account, month) that has either a budget line or real ledger movement, it
 * pairs the effective budget against the actual movement summed from the
 * append-only journal. Money is integer øre internally (src/core/money.ts).
 *
 * Sign / variance convention:
 *  - expense accounts: actual = debit − credit; variance = budget − actual
 *    (positive ⇒ under budget, i.e. spent less than planned — good).
 *  - income / every other account: actual = credit − debit; variance =
 *    actual − budget (positive ⇒ above target — good).
 *
 * This slice is deliberately retrospective + plan-only: it stores a number and
 * compares it to reality. There is no forecasting model here — that lives in
 * src/core/liquidity-forecast.ts, which consumes these same budget lines.
 */

import type { Database } from "bun:sqlite";
import { fromOre, toOre } from "./money";
import { insertAuditLog } from "./actor";

const SET_BUDGET_RULE_ID = "budget:set";
const BUDGET_VS_ACTUAL_REPORT_ID = "budget:vs-actual";

/** A calendar-month period string, `YYYY-MM`. */
export type BudgetPeriod = string;

export type SetBudgetInput = {
  /** Account number the budget applies to — must exist in the chart of accounts. */
  accountNo: string;
  /** Calendar month in `YYYY-MM` form. */
  period: BudgetPeriod;
  /** Planned amount in kroner, in the account's natural sign (never negative). */
  amount: number;
  notes?: string;
  createdBy?: string;
  createdByProgram?: string;
};

export type SetBudgetResult = {
  ok: boolean;
  budgetLineId?: number;
  accountNo?: string;
  period?: string;
  amount?: number;
  appliedRules: string[];
  errors: string[];
};

export type BudgetLine = {
  id: number;
  accountNo: string;
  accountName: string | null;
  period: string;
  amount: number;
  notes: string | null;
  createdAt: string;
};

export type ListBudgetResult = {
  ok: boolean;
  count: number;
  rows: BudgetLine[];
  errors: string[];
};

export type BudgetVsActualLine = {
  accountNo: string;
  accountName: string | null;
  accountType: string | null;
  period: string;
  budget: number;
  actual: number;
  /** Signed variance — see the module header for the per-type convention. */
  variance: number;
};

export type BudgetVsActualReport = {
  ok: boolean;
  appliedRules: string[];
  periodStart: string;
  periodEnd: string;
  lines: BudgetVsActualLine[];
  totalBudget: number;
  totalActual: number;
  /** totalActual − totalBudget. Interpretation depends on the account mix. */
  totalVariance: number;
  errors: string[];
};

/** A `YYYY-MM` calendar-month string with a real month 01-12. */
export function isValidBudgetPeriod(value: unknown): value is BudgetPeriod {
  if (typeof value !== "string") return false;
  const match = /^(\d{4})-(\d{2})$/.exec(value.trim());
  if (!match) return false;
  const month = Number(match[2]);
  return month >= 1 && month <= 12;
}

/** First day of a `YYYY-MM` period as a `YYYY-MM-DD` date. */
export function periodStartDate(period: BudgetPeriod): string {
  return `${period}-01`;
}

/** Last day of a `YYYY-MM` period as a `YYYY-MM-DD` date (UTC calendar math). */
export function periodEndDate(period: BudgetPeriod): string {
  const year = Number(period.slice(0, 4));
  const month = Number(period.slice(5, 7));
  const lastDay = new Date(Date.UTC(year, month, 0)).getUTCDate();
  return `${period}-${String(lastDay).padStart(2, "0")}`;
}

/** The next `YYYY-MM` period after `period` (December rolls into January). */
export function nextPeriod(period: BudgetPeriod): BudgetPeriod {
  const year = Number(period.slice(0, 4));
  const month = Number(period.slice(5, 7));
  const total = year * 12 + (month - 1) + 1;
  const ny = Math.floor(total / 12);
  const nm = total - ny * 12 + 1;
  return `${ny}-${String(nm).padStart(2, "0")}`;
}

/**
 * Every `YYYY-MM` period in `[start, end]` inclusive, chronologically.
 * Returns an empty list when the range is malformed or inverted.
 */
export function periodsInRange(start: BudgetPeriod, end: BudgetPeriod): BudgetPeriod[] {
  if (!isValidBudgetPeriod(start) || !isValidBudgetPeriod(end) || start > end) return [];
  const periods: BudgetPeriod[] = [];
  let cursor = start;
  // The range is bounded by the validated end period, so this terminates.
  while (cursor <= end) {
    periods.push(cursor);
    cursor = nextPeriod(cursor);
  }
  return periods;
}

function accountExists(db: Database, accountNo: string): boolean {
  return (
    db.query("SELECT 1 FROM accounts WHERE account_no = ? LIMIT 1").get(accountNo) !== null
  );
}

/**
 * Record (append) a budget line for an account in a period. Each call inserts
 * a new revision; the latest revision is the effective budget. Re-setting the
 * same pair is therefore always safe and fully audited.
 */
export function setBudget(db: Database, input: SetBudgetInput): SetBudgetResult {
  const appliedRules = [SET_BUDGET_RULE_ID];
  const errors: string[] = [];

  const accountNo = typeof input.accountNo === "string" ? input.accountNo.trim() : "";
  if (!accountNo) {
    errors.push("accountNo is required");
  } else if (!accountExists(db, accountNo)) {
    errors.push(`account ${accountNo} does not exist in the chart of accounts`);
  }

  if (!isValidBudgetPeriod(input.period)) {
    errors.push("period must be a YYYY-MM calendar month");
  }

  const amount = Number(input.amount);
  if (!Number.isFinite(amount) || amount < 0) {
    errors.push("amount must be a non-negative number");
  }

  if (errors.length > 0) return { ok: false, appliedRules, errors };

  const period = input.period.trim();
  // Store the amount øre-rounded so the report never re-introduces float drift.
  const normalizedAmount = fromOre(toOre(amount));
  const notes = typeof input.notes === "string" && input.notes.trim().length > 0 ? input.notes.trim() : null;

  const inserted = db.transaction(() => {
    const row = db
      .query(
        `INSERT INTO budget_lines (account_no, period, amount, notes)
         VALUES (?, ?, ?, ?) RETURNING id`,
      )
      .get(accountNo, period, normalizedAmount, notes) as { id: number };

    insertAuditLog(db, {
      eventType: "budget_set",
      entityType: "budget_line",
      entityId: row.id,
      message: `Set budget for account ${accountNo} period ${period} to ${normalizedAmount}`,
      createdBy: input.createdBy,
      createdByProgram: input.createdByProgram,
    });
    return row;
  }, { immediate: true })();

  return {
    ok: true,
    budgetLineId: inserted.id,
    accountNo,
    period,
    amount: normalizedAmount,
    appliedRules,
    errors: [],
  };
}

type EffectiveBudgetRow = {
  id: number;
  account_no: string;
  account_name: string | null;
  period: string;
  amount: number;
  notes: string | null;
  created_at: string;
};

/**
 * The effective (latest-revision) budget lines. Optionally filtered to a single
 * period. Ordered deterministically by period then account number.
 */
export function listBudget(
  db: Database,
  filters: { period?: string; accountNo?: string } = {},
): ListBudgetResult {
  if (filters.period !== undefined && !isValidBudgetPeriod(filters.period)) {
    return { ok: false, count: 0, rows: [], errors: ["period filter must be a YYYY-MM calendar month"] };
  }

  // The effective line for each (account, period) pair is the row with the
  // highest id — append-only revisions, latest wins.
  const rows = db
    .query(
      `SELECT b.id        AS id,
              b.account_no AS account_no,
              a.name       AS account_name,
              b.period     AS period,
              b.amount     AS amount,
              b.notes      AS notes,
              b.created_at AS created_at
         FROM budget_lines b
         LEFT JOIN accounts a ON a.account_no = b.account_no
        WHERE b.id IN (
          SELECT MAX(id) FROM budget_lines GROUP BY account_no, period
        )
          AND (? IS NULL OR b.period = ?)
          AND (? IS NULL OR b.account_no = ?)
        ORDER BY b.period ASC, b.account_no ASC`,
    )
    .all(
      filters.period ?? null,
      filters.period ?? null,
      filters.accountNo ?? null,
      filters.accountNo ?? null,
    ) as EffectiveBudgetRow[];

  return {
    ok: true,
    count: rows.length,
    rows: rows.map((row) => ({
      id: row.id,
      accountNo: row.account_no,
      accountName: row.account_name,
      period: row.period,
      amount: fromOre(toOre(Number(row.amount ?? 0))),
      notes: row.notes,
      createdAt: row.created_at,
    })),
    errors: [],
  };
}

/**
 * Budget-vs-actual report over a `[periodStart, periodEnd]` range of calendar
 * months.
 *
 * A budget is a profit-&-loss concept — you budget what you earn and spend,
 * not your cash position. So the report covers every income/expense account
 * with movement in the range, plus any account that carries a budget line
 * (a budget on a balance-sheet account is still surfaced so it is not lost).
 * Each covered (account, month) pair produces one line.
 *
 * Pure: queries `journal_entries` / `journal_lines` / `accounts` and the
 * append-only `budget_lines`, never the wall clock, and yields byte-identical
 * output for identical input.
 */
export function buildBudgetVsActual(
  db: Database,
  periodStart: BudgetPeriod,
  periodEnd: BudgetPeriod,
): BudgetVsActualReport {
  const errors: string[] = [];
  if (!isValidBudgetPeriod(periodStart)) errors.push("periodStart must be a YYYY-MM calendar month");
  if (!isValidBudgetPeriod(periodEnd)) errors.push("periodEnd must be a YYYY-MM calendar month");
  if (errors.length === 0 && periodStart > periodEnd) {
    errors.push("periodStart must be before or equal to periodEnd");
  }
  if (errors.length > 0) {
    return {
      ok: false,
      appliedRules: [BUDGET_VS_ACTUAL_REPORT_ID],
      periodStart,
      periodEnd,
      lines: [],
      totalBudget: 0,
      totalActual: 0,
      totalVariance: 0,
      errors,
    };
  }

  const rangeStartDate = periodStartDate(periodStart);
  const rangeEndDate = periodEndDate(periodEnd);

  // Per (account, month) actual movement, summed in integer øre. The month is
  // derived from the entry's transaction_date (first 7 chars, YYYY-MM).
  const actualRows = db
    .query(
      `SELECT a.account_no AS account_no,
              a.name       AS account_name,
              a.type       AS account_type,
              substr(je.transaction_date, 1, 7) AS period,
              jl.debit_amount  AS debit_amount,
              jl.credit_amount AS credit_amount
         FROM journal_entries je
         JOIN journal_lines jl ON jl.journal_entry_id = je.id
         JOIN accounts a       ON a.id = jl.account_id
        WHERE je.transaction_date >= ? AND je.transaction_date <= ?
        ORDER BY a.account_no ASC, je.id ASC, jl.id ASC`,
    )
    .all(rangeStartDate, rangeEndDate) as Array<{
    account_no: string;
    account_name: string;
    account_type: string;
    period: string;
    debit_amount: number | null;
    credit_amount: number | null;
  }>;

  type Cell = {
    accountNo: string;
    accountName: string | null;
    accountType: string | null;
    period: string;
    debitOre: bigint;
    creditOre: bigint;
    budgetOre: bigint;
    /** True once a budget line has been attached to this cell. */
    hasBudget: boolean;
  };
  const cells = new Map<string, Cell>();
  const keyOf = (accountNo: string, period: string) => `${accountNo} ${period}`;

  for (const row of actualRows) {
    const key = keyOf(row.account_no, row.period);
    let cell = cells.get(key);
    if (!cell) {
      cell = {
        accountNo: row.account_no,
        accountName: row.account_name,
        accountType: row.account_type,
        period: row.period,
        debitOre: 0n,
        creditOre: 0n,
        budgetOre: 0n,
        hasBudget: false,
      };
      cells.set(key, cell);
    }
    cell.debitOre += toOre(Number(row.debit_amount ?? 0));
    cell.creditOre += toOre(Number(row.credit_amount ?? 0));
  }

  // Effective budget lines whose period falls inside the range.
  const budgetRows = db
    .query(
      `SELECT b.account_no AS account_no,
              a.name       AS account_name,
              a.type       AS account_type,
              b.period     AS period,
              b.amount     AS amount
         FROM budget_lines b
         LEFT JOIN accounts a ON a.account_no = b.account_no
        WHERE b.id IN (
          SELECT MAX(id) FROM budget_lines GROUP BY account_no, period
        )
          AND b.period >= ? AND b.period <= ?`,
    )
    .all(periodStart, periodEnd) as Array<{
    account_no: string;
    account_name: string | null;
    account_type: string | null;
    period: string;
    amount: number;
  }>;

  for (const row of budgetRows) {
    const key = keyOf(row.account_no, row.period);
    let cell = cells.get(key);
    if (!cell) {
      cell = {
        accountNo: row.account_no,
        accountName: row.account_name,
        accountType: row.account_type,
        period: row.period,
        debitOre: 0n,
        creditOre: 0n,
        budgetOre: 0n,
        hasBudget: false,
      };
      cells.set(key, cell);
    } else {
      // An actual-derived cell carries the live account name/type already.
      if (cell.accountName == null) cell.accountName = row.account_name;
      if (cell.accountType == null) cell.accountType = row.account_type;
    }
    cell.budgetOre += toOre(Number(row.amount ?? 0));
    cell.hasBudget = true;
  }

  let totalBudgetOre = 0n;
  let totalActualOre = 0n;
  const lines: BudgetVsActualLine[] = [...cells.values()]
    // A budget is a P&L concept: keep every income/expense account, plus any
    // account that actually carries a budget line — drop balance-sheet cash
    // movement (a credit to the bank account, etc.) that no one budgeted.
    .filter(
      (cell) =>
        cell.hasBudget || cell.accountType === "income" || cell.accountType === "expense",
    )
    .sort((a, b) => {
      if (a.period !== b.period) return a.period < b.period ? -1 : 1;
      return a.accountNo < b.accountNo ? -1 : a.accountNo > b.accountNo ? 1 : 0;
    })
    .map((cell) => {
      // Expense accounts are debit-normal; everything else surfaces credit-net.
      const isExpense = cell.accountType === "expense";
      const actualOre = isExpense
        ? cell.debitOre - cell.creditOre
        : cell.creditOre - cell.debitOre;
      const varianceOre = isExpense
        ? cell.budgetOre - actualOre
        : actualOre - cell.budgetOre;
      totalBudgetOre += cell.budgetOre;
      totalActualOre += actualOre;
      return {
        accountNo: cell.accountNo,
        accountName: cell.accountName,
        accountType: cell.accountType,
        period: cell.period,
        budget: fromOre(cell.budgetOre),
        actual: fromOre(actualOre),
        variance: fromOre(varianceOre),
      };
    });

  return {
    ok: true,
    appliedRules: [BUDGET_VS_ACTUAL_REPORT_ID],
    periodStart,
    periodEnd,
    lines,
    totalBudget: fromOre(totalBudgetOre),
    totalActual: fromOre(totalActualOre),
    totalVariance: fromOre(totalActualOre - totalBudgetOre),
    errors: [],
  };
}
