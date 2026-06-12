/**
 * Deterministic liquidity (cash-flow) forecast.
 *
 * The system is otherwise purely retrospective — it records what happened.
 * This module projects the bank balance *forward*, month by month, from data
 * that is already known and deterministic:
 *
 *   opening balance      the booked bank balance the day before the forecast
 *                        starts (computed from the append-only ledger)
 * + invoice inflow       the open balance of every issued invoice whose
 *                        effective due date falls in the month (invoice-list.ts)
 * + recurring inflow     the gross amount of every recurring-invoice template
 *                        generation scheduled to come due in the month
 *                        (recurring-invoices.ts)
 * - budgeted cost        the budgeted amount of every expense account for the
 *                        month (budget.ts)
 * = closing balance      carried forward as the next month's opening balance
 *
 * There is NO statistical model and NO ML here: every figure is arithmetic
 * over known rows. Identical inputs always yield byte-identical output. The
 * forecast is a planning aid, not a ledger posting — it writes nothing.
 *
 * Scope deliberately left out (documented, not hidden): unbudgeted/ad-hoc
 * spend, VAT settlement timing, payroll, overdue-invoice payment-probability
 * weighting, and any account that is neither an open invoice, a recurring
 * template, nor a budgeted expense. The projection is only as complete as the
 * budget the owner maintains.
 */

import type { Database } from "bun:sqlite";
import { isValidIsoDate, addDays } from "./dates";
import { fromOre, toOre } from "./money";
import { listBankAccounts } from "./bank";
import { buildInvoiceList } from "./invoice-list";
import { addMonths } from "./recurring-invoices";
import {
  isValidBudgetPeriod,
  periodStartDate,
  periodEndDate,
  nextPeriod,
  type BudgetPeriod,
} from "./budget";

const LIQUIDITY_FORECAST_REPORT_ID = "liquidity-forecast";

const INTERVAL_MONTHS: Record<"monthly" | "quarterly" | "yearly", number> = {
  monthly: 1,
  quarterly: 3,
  yearly: 12,
};

/** Hard cap on the forecast horizon — a year and a half of monthly periods. */
const MAX_FORECAST_MONTHS = 18;

export type LiquidityForecastInput = {
  /** First day of the first month to project, `YYYY-MM-DD`. */
  startDate: string;
  /** Number of consecutive monthly periods to project (1..18). */
  months: number;
};

export type LiquidityForecastPeriod = {
  /** Calendar month, `YYYY-MM`. */
  period: string;
  /** Bank balance carried into the month. */
  openingBalance: number;
  /** Open-invoice receipts expected to come due this month. */
  invoiceInflow: number;
  /** Recurring-invoice template generations projected to come due this month. */
  recurringInflow: number;
  /** Budgeted expense outflow for the month. */
  budgetedCostOutflow: number;
  /** Net change = invoiceInflow + recurringInflow − budgetedCostOutflow. */
  netChange: number;
  /** Projected bank balance at month end. */
  closingBalance: number;
};

export type LiquidityForecastResult = {
  ok: boolean;
  appliedRules: string[];
  startDate?: string;
  months?: number;
  /** The booked bank balance the day before startDate. */
  openingBalance: number;
  /** Projected balance at the end of the final period. */
  closingBalance: number;
  periods: LiquidityForecastPeriod[];
  errors: string[];
};

/**
 * The set of ledger account numbers that constitute "the bank" — the linked
 * `bank_accounts.ledger_account_no`s when any bank account is registered,
 * otherwise every `asset` account that reads as bank/cash/giro. This mirrors
 * the cockpit's `bankBalanceAsOf` so the forecast's opening balance agrees
 * with every other surface, without coupling to the server layer.
 */
function bankAccountNumbers(db: Database): string[] {
  const linked = listBankAccounts(db)
    .accounts.map((a) => a.ledgerAccountNo)
    .filter((no): no is string => typeof no === "string" && no.length > 0);
  if (linked.length > 0) return [...new Set(linked)];

  return (
    db
      .query(
        `SELECT account_no FROM accounts
          WHERE type = 'asset'
            AND (lower(name) LIKE '%bank%' OR lower(name) LIKE '%kasse%'
                 OR lower(name) LIKE '%giro%')`,
      )
      .all() as Array<{ account_no: string }>
  ).map((r) => r.account_no);
}

/** Booked bank balance (debit − credit, kroner) at `asOfDate` from the ledger. */
function bookedBankBalance(db: Database, asOfDate: string): number {
  const accountNos = bankAccountNumbers(db);
  if (accountNos.length === 0) return 0;
  const placeholders = accountNos.map(() => "?").join(", ");
  const row = db
    .query(
      `SELECT COALESCE(SUM(jl.debit_amount - jl.credit_amount), 0) AS bal
         FROM journal_entries je
         JOIN journal_lines jl ON jl.journal_entry_id = je.id
         JOIN accounts a       ON a.id = jl.account_id
        WHERE je.status = 'posted'
          AND je.transaction_date <= ?
          AND a.account_no IN (${placeholders})`,
    )
    .get(asOfDate, ...accountNos) as { bal: number };
  return fromOre(toOre(Number(row.bal ?? 0)));
}

type RecurringTemplateRow = {
  id: number;
  interval: "monthly" | "quarterly" | "yearly";
  first_issue_date: string;
  payment_terms_days: number;
  payload_json: string;
};

/**
 * Projected recurring-invoice inflow per `YYYY-MM`, in integer øre.
 *
 * For every active template, every generation whose *due date* (issueDate +
 * paymentTermsDays, derived purely from `firstIssueDate` + interval) falls in
 * the forecast window contributes its gross amount. This is the same date
 * arithmetic `recurring-invoices.ts` uses for real generation — no wall clock.
 *
 * A generation already materialised before the forecast window is excluded:
 * its invoice is then a real `documents` row already counted by the invoice
 * inflow, so counting the template too would double-count it.
 */
function recurringInflowByPeriod(
  db: Database,
  windowStart: string,
  windowEnd: string,
): Map<string, bigint> {
  const byPeriod = new Map<string, bigint>();
  const templates = db
    .query(
      `SELECT id, interval, first_issue_date, payment_terms_days, payload_json
         FROM recurring_invoice_templates
        WHERE active = 1
        ORDER BY id ASC`,
    )
    .all() as RecurringTemplateRow[];

  for (const template of templates) {
    const intervalMonths = INTERVAL_MONTHS[template.interval];
    if (!intervalMonths) continue;
    const payload = JSON.parse(template.payload_json) as {
      totals?: { grossAmount?: number };
    };
    const gross = Number(payload.totals?.grossAmount ?? 0);
    if (!Number.isFinite(gross) || gross <= 0) continue;
    const grossOre = toOre(gross);

    // Walk period indices forward; the due date is monotonically increasing,
    // so once it passes windowEnd we can stop.
    for (let index = 0; index < 1000; index += 1) {
      const issueDate = addMonths(template.first_issue_date, intervalMonths * index);
      const dueDate = addDays(issueDate, template.payment_terms_days);
      if (dueDate > windowEnd) break;
      if (dueDate < windowStart) continue;
      // Skip a period already generated — its real invoice is counted by the
      // invoice inflow, so the template projection must not double-count it.
      const alreadyGenerated = db
        .query(
          `SELECT 1 FROM recurring_invoice_generations
            WHERE template_id = ? AND period_index = ? LIMIT 1`,
        )
        .get(template.id, index);
      if (alreadyGenerated) continue;
      const period = dueDate.slice(0, 7);
      byPeriod.set(period, (byPeriod.get(period) ?? 0n) + grossOre);
    }
  }
  return byPeriod;
}

/**
 * Open-invoice inflow per `YYYY-MM`, in integer øre — the open balance of every
 * issued invoice whose effective due date falls in the forecast window.
 *
 * Uses `buildInvoiceList` so the open-balance computation (payments, credit
 * notes, refunds, bad-debt write-offs) is exactly the live invoice ledger.
 * A paid/credited/written-off invoice has a zero open balance and so adds
 * nothing.
 */
function invoiceInflowByPeriod(
  db: Database,
  windowStart: string,
  windowEnd: string,
): Map<string, bigint> {
  const byPeriod = new Map<string, bigint>();
  // `asOfDate` only affects overdue flags, not the open balance — pass the
  // window start so the list is computed deterministically.
  const list = buildInvoiceList(db, { status: "all", asOfDate: windowStart });
  for (const row of list.rows) {
    if (row.openBalance <= 0) continue;
    const due = row.effectiveDueDate;
    if (!due || due < windowStart || due > windowEnd) continue;
    const period = due.slice(0, 7);
    byPeriod.set(period, (byPeriod.get(period) ?? 0n) + toOre(row.openBalance));
  }
  return byPeriod;
}

/** Budgeted expense outflow per `YYYY-MM`, in integer øre, for the window. */
function budgetedCostByPeriod(
  db: Database,
  firstPeriod: BudgetPeriod,
  lastPeriod: BudgetPeriod,
): Map<string, bigint> {
  const byPeriod = new Map<string, bigint>();
  const rows = db
    .query(
      `SELECT b.period AS period, b.amount AS amount
         FROM budget_lines b
         JOIN accounts a ON a.account_no = b.account_no
        WHERE b.id IN (
          SELECT MAX(id) FROM budget_lines GROUP BY account_no, period
        )
          AND a.type = 'expense'
          AND b.period >= ? AND b.period <= ?`,
    )
    .all(firstPeriod, lastPeriod) as Array<{ period: string; amount: number }>;
  for (const row of rows) {
    byPeriod.set(
      row.period,
      (byPeriod.get(row.period) ?? 0n) + toOre(Number(row.amount ?? 0)),
    );
  }
  return byPeriod;
}

/**
 * Project the bank balance forward `months` calendar months from `startDate`.
 *
 * Pure deterministic read: never mutates the database, never reads the wall
 * clock, and yields byte-identical output for identical input.
 */
export function buildLiquidityForecast(
  db: Database,
  input: LiquidityForecastInput,
): LiquidityForecastResult {
  const errors: string[] = [];
  if (!isValidIsoDate(input.startDate)) {
    errors.push("startDate must be a YYYY-MM-DD date");
  }
  const months = Number(input.months);
  if (!Number.isInteger(months) || months <= 0) {
    errors.push("months must be a positive integer");
  } else if (months > MAX_FORECAST_MONTHS) {
    errors.push(`months must not exceed ${MAX_FORECAST_MONTHS}`);
  }
  if (errors.length > 0) {
    return {
      ok: false,
      appliedRules: [LIQUIDITY_FORECAST_REPORT_ID],
      openingBalance: 0,
      closingBalance: 0,
      periods: [],
      errors,
    };
  }

  const startDate = input.startDate.trim();
  // The first forecast month is the calendar month containing startDate.
  const firstPeriod: BudgetPeriod = startDate.slice(0, 7);
  // Defensive: a YYYY-MM-DD start always yields a valid YYYY-MM month.
  if (!isValidBudgetPeriod(firstPeriod)) {
    return {
      ok: false,
      appliedRules: [LIQUIDITY_FORECAST_REPORT_ID],
      openingBalance: 0,
      closingBalance: 0,
      periods: [],
      errors: ["startDate does not resolve to a valid calendar month"],
    };
  }

  // Enumerate the consecutive YYYY-MM periods to project.
  const periods: BudgetPeriod[] = [];
  let cursor = firstPeriod;
  for (let i = 0; i < months; i += 1) {
    periods.push(cursor);
    cursor = nextPeriod(cursor);
  }
  const lastPeriod = periods[periods.length - 1]!;
  const windowStart = periodStartDate(firstPeriod);
  const windowEnd = periodEndDate(lastPeriod);

  // Opening balance: the booked bank balance the day before the window opens,
  // so a posting dated on the window start counts as a forecast-period event,
  // not as part of the baseline.
  const openingBalanceOre = toOre(bookedBankBalance(db, addDays(windowStart, -1)));

  const invoiceInflow = invoiceInflowByPeriod(db, windowStart, windowEnd);
  const recurringInflow = recurringInflowByPeriod(db, windowStart, windowEnd);
  const budgetedCost = budgetedCostByPeriod(db, firstPeriod, lastPeriod);

  const out: LiquidityForecastPeriod[] = [];
  let runningOre = openingBalanceOre;
  for (const period of periods) {
    const inflowOre = invoiceInflow.get(period) ?? 0n;
    const recurringOre = recurringInflow.get(period) ?? 0n;
    const costOre = budgetedCost.get(period) ?? 0n;
    const netOre = inflowOre + recurringOre - costOre;
    const openingOre = runningOre;
    const closingOre = openingOre + netOre;
    runningOre = closingOre;
    out.push({
      period,
      openingBalance: fromOre(openingOre),
      invoiceInflow: fromOre(inflowOre),
      recurringInflow: fromOre(recurringOre),
      budgetedCostOutflow: fromOre(costOre),
      netChange: fromOre(netOre),
      closingBalance: fromOre(closingOre),
    });
  }

  return {
    ok: true,
    appliedRules: [LIQUIDITY_FORECAST_REPORT_ID],
    startDate,
    months,
    openingBalance: fromOre(openingBalanceOre),
    closingBalance: fromOre(runningOre),
    periods: out,
    errors: [],
  };
}
