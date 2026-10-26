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
import { plannedCommitmentOccurrences } from "./supplier-commitments";
import { buildPayablesList } from "./payables";
import { buildVatReport } from "./vat";
import { effectivePeriodState, vatPeriodsForYear, type VatPeriodType } from "./periods";
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
  /** False means no canonical bank/cash account exists; zero is not cash evidence. */
  openingBalanceVerified: boolean;
  /** Canonical scope exclusions that a UI must disclose. */
  exclusions: string[];
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
      openingBalanceVerified: false,
      exclusions: ["Ingen verificeret startsaldo ved ugyldig forecast-forespørgsel."],
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
      openingBalanceVerified: false,
      exclusions: ["Ingen verificeret startsaldo ved ugyldig forecast-forespørgsel."],
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
  const openingBalanceVerified = bankAccountNumbers(db).length > 0;
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
    openingBalanceVerified,
    exclusions: ["Ubudgetterede/ad hoc-udgifter", "momsafregningstidspunkt", "løn", "sandsynlighedsvægtning af forfaldne fakturaer"],
    closingBalance: fromOre(runningOre),
    periods: out,
    errors: [],
  };
}

/**
 * A reviewed, company-scoped cash item supplied by a workspace integration.
 *
 * The company ledger deliberately does not open the workspace-control database.
 * This narrow seam lets that layer pass an already-authorised intercompany
 * disposition, approved scenario, or other legally due obligation without
 * weakening company isolation. `companyId` must match the ledger's one and a
 * non-DKK item is never converted implicitly.
 */
export type ReviewedLiquiditySupplement = {
  kind:
    | "legally_due_obligation"
    | "approved_budget_assumption"
    | "approved_scenario_assumption"
    | "approved_intercompany_disposition";
  /** Workspace-unique legal-company identity. Never a ledger-local numeric id. */
  companySlug: string;
  dueDate: string;
  /** Positive absolute amount in the stated currency. */
  amount: number;
  currency: string;
  direction: "inflow" | "outflow";
  /** Stable canonical record id, not a free-text description. */
  reference: string;
  /** Immutable review/audit reference proving that this assumption is approved. */
  approvalReference: string;
};

export type ThirteenWeekLiquidityInput = {
  startDate: string;
  weeks?: number;
  /** Read-only reviewed items from the same legal company only. */
  supplements?: readonly ReviewedLiquiditySupplement[];
  /** Required before workspace supplements can affect this company. */
  companySlug?: string;
};

/** Thirteen-week cash view. Unlike the legacy monthly planning report this
 * keeps native currencies explicit: only DKK changes the DKK cash line unless
 * an integration supplies a dated FX source (not inferred here). */
export type WeeklyLiquidityPeriod = {
  weekStart:string;
  openingCash:number;
  receivables:number;
  payables:number;
  commitments:number;
  /** Approved budget assumptions, never booked facts. */
  budgets:number;
  /** Account-level monthly budgets with no cash date. Informational only. */
  undatedBudgetAssumptions:number;
  /** Approved scenario assumptions, never booked facts. */
  scenarios:number;
  /** VAT, tax and other legally due canonical obligations. */
  obligations:number;
  /** Explicit, reviewed company-scoped intercompany dispositions. */
  intercompany:number;
  excluded:Array<{source:string;amount:number;currency:string;reason?:string}>;
  /** Forecast from observed/canonical cash sources, excluding reviewed assumptions. */
  closingCash:number;
  /** `closingCash` plus dated reviewed scenario/budget/intercompany assumptions. */
  scenarioClosingCash:number;
  sources:Array<{source:string;amount:number;reference:string;assumption?:boolean;settlementStatus?:"unknown"}>;
};
export type WeeklyLiquidityResult = { ok:boolean; startDate?:string; openingCash:number; openingBalanceVerified:boolean; lowestPoint:number; completeness:{included:string[];excluded:string[]}; periods:WeeklyLiquidityPeriod[]; errors:string[]; appliedRules:string[] };

function companyVatPeriodType(db: Database): VatPeriodType | null {
  const columns = db.query("PRAGMA table_info(companies)").all() as Array<{ name: string }>;
  if (!columns.some((column) => column.name === "vat_period_type")) return null;
  const row = db.query("SELECT vat_period_type FROM companies ORDER BY id ASC LIMIT 1").get() as
    | { vat_period_type: unknown }
    | null;
  return row?.vat_period_type === "month" || row?.vat_period_type === "quarter" || row?.vat_period_type === "half-year"
    ? row.vat_period_type
    : null;
}

/** Canonical VAT positions whose statutory payment date is inside the window. */
function vatObligationsForWindow(
  db: Database,
  start: string,
  end: string,
): { canonical:Array<{ dueDate: string; amount: number; reference: string }>; estimates:Array<{ dueDate: string; amount: number; reference: string }> } {
  const cadence = companyVatPeriodType(db);
  if (cadence === null) return { canonical: [], estimates: [] };
  // A Q4/half-year VAT period can have its statutory deadline in the following
  // calendar year, so include the preceding period year as well.
  const years = [...new Set([Number(start.slice(0, 4)) - 1, Number(start.slice(0, 4)), Number(end.slice(0, 4))])];
  const canonical: Array<{ dueDate: string; amount: number; reference: string }> = [];
  const estimates: Array<{ dueDate: string; amount: number; reference: string }> = [];
  for (const year of years) {
    for (const period of vatPeriodsForYear(year, cadence)) {
      if (period.filingDeadline < start || period.filingDeadline > end) continue;
      const report = buildVatReport(db, period.start, period.end);
      if (!report.ok || report.netVatPayable <= 0) continue;
      const row = {
        dueDate: period.filingDeadline,
        amount: Number(report.netVatPayable),
        reference: `vat:${period.start}:${period.end}`,
      };
      const stateRow = db.query(`SELECT id, status FROM accounting_periods WHERE kind IN ('vat_period','vat_quarter') AND period_start = ? AND period_end = ? ORDER BY CASE kind WHEN 'vat_period' THEN 0 ELSE 1 END, id DESC LIMIT 1`).get(period.start, period.end) as {id:number;status:"open"|"closed"|"reported"}|null;
      const status = stateRow ? effectivePeriodState(db, stateRow.id, stateRow.status) : "open";
      // A filing-safe, closed/reported period is a canonical payable. We do
      // not infer whether settlement was paid: that state is separate.
      if (status === "closed" || status === "reported") canonical.push(row);
      else estimates.push(row);
    }
  }
  return { canonical, estimates };
}

/**
 * Effective append-only budget revisions are account-level assumptions, not
 * individual payable or commitment evidence.  They are deliberately kept in
 * their own source bucket and provenance is the winning revision id.  There is
 * no canonical allocation linking a budget line to a particular payable, so
 * callers must not present the two as mutually exclusive facts.
 */
function budgetAssumptionsForWindow(
  db: Database,
  start: string,
  end: string,
): Array<{ period:string; amount: number; reference: string }> {
  const startPeriod = start.slice(0, 7);
  const endPeriod = end.slice(0, 7);
  const rows = db.query(
    `SELECT b.id, b.period, b.account_no, b.amount
       FROM budget_lines b
       JOIN accounts a ON a.account_no = b.account_no
      WHERE b.id IN (SELECT MAX(id) FROM budget_lines GROUP BY account_no, period)
        AND a.type = 'expense'
        AND b.period >= ? AND b.period <= ?
      ORDER BY b.period ASC, b.account_no ASC, b.id ASC`,
  ).all(startPeriod, endPeriod) as Array<{ id: number; period: string; account_no: string; amount: number }>;
  return rows
    .filter((row) => Number.isFinite(row.amount) && row.amount > 0)
    .map((row) => ({
      // A monthly account budget is not a dated payable. Keep it visible but
      // never invent a payment date or subtract it from cash.
      period: row.period,
      amount: Number(row.amount),
      reference: `budget-revision:${row.id}:account:${row.account_no}`,
    }));
}

export function buildThirteenWeekLiquidityForecast(db:Database,input:ThirteenWeekLiquidityInput):WeeklyLiquidityResult {
  const weeks=input.weeks??13;
  if(!isValidIsoDate(input.startDate)||!Number.isInteger(weeks)||weeks<1||weeks>13)return {ok:false,openingCash:0,openingBalanceVerified:false,lowestPoint:0,periods:[],errors:["startDate and weeks (1-13) are required"],appliedRules:["liquidity-forecast-13-week-v1"],completeness:{included:[],excluded:[]}};
  const start=input.startDate, end=addDays(start,weeks*7-1), openingBalanceVerified=bankAccountNumbers(db).length>0, openingOre=toOre(bookedBankBalance(db,addDays(start,-1)));
  const rows:WeeklyLiquidityPeriod[]=[];let baseCashOre=openingOre, scenarioCashOre=openingOre, lowestOre=openingOre;
  const occurrences=plannedCommitmentOccurrences(db,start,weeks);
  const invoices=buildInvoiceList(db,{status:"all",asOfDate:start}).rows.filter((r):r is typeof r & {effectiveDueDate:string}=>r.openBalance>0&&typeof r.effectiveDueDate==="string"&&r.effectiveDueDate>=start&&r.effectiveDueDate<=addDays(start,weeks*7-1));
  const payables=buildPayablesList(db,{status:"open",asOfDate:start}).rows.filter(x=>x.dueDate>=start&&x.dueDate<=end);
  const supplements=(input.supplements??[]).filter((item) =>
    Boolean(input.companySlug) && item.companySlug === input.companySlug &&
    isValidIsoDate(item.dueDate) && item.dueDate >= start && item.dueDate <= end &&
    Number.isFinite(item.amount) && item.amount > 0 &&
    item.currency.toUpperCase() === "DKK" &&
    item.reference.trim().length > 0 && item.approvalReference.trim().length > 0,
  );
  const excludedSupplements=(input.supplements??[]).filter((item) => !supplements.includes(item));
  const vatObligations=vatObligationsForWindow(db,start,end);
  const budgetAssumptions=budgetAssumptionsForWindow(db,start,end);
  for(let i=0;i<weeks;i++){
    const weekStart=addDays(start,i*7),weekEnd=addDays(weekStart,6);const sources:WeeklyLiquidityPeriod["sources"]=[],excluded:WeeklyLiquidityPeriod["excluded"]=[];
    const invOre=invoices.filter(x=>x.effectiveDueDate>=weekStart&&x.effectiveDueDate<=weekEnd).reduce((n,x)=>n+toOre(x.openBalance),0n);if(invOre)sources.push({source:"issued_receivables",amount:fromOre(invOre),reference:"invoice-list"});
    const payableOre=payables.filter(x=>x.dueDate>=weekStart&&x.dueDate<=weekEnd).reduce((n,x)=>n+toOre(Number(x.openBalance)),0n);if(payableOre)sources.push({source:"registered_payables",amount:fromOre(-payableOre),reference:"payables"});
    let commitmentOre=0n;for(const o of occurrences.filter(x=>x.date>=weekStart&&x.date<=weekEnd)){if(o.currency==="DKK"){const amount=toOre(o.amount);commitmentOre+=amount;sources.push({source:"approved_commitment",amount:fromOre(-amount),reference:o.commitmentId});}else excluded.push({source:o.commitmentId,amount:o.amount,currency:o.currency,reason:"dated_fx_required"});}
    let budgetOre=0n,scenarioOre=0n,obligationOre=0n,intercompanyOre=0n,undatedBudgetOre=0n;
    for(const vat of vatObligations.canonical.filter((item)=>item.dueDate>=weekStart&&item.dueDate<=weekEnd)){
      const amount=toOre(vat.amount); obligationOre+=amount;
      sources.push({source:"canonical_vat_obligation",amount:fromOre(-amount),reference:vat.reference,settlementStatus:"unknown"});
    }
    for(const vat of vatObligations.estimates.filter((item)=>item.dueDate>=weekStart&&item.dueDate<=weekEnd)){
      const amount=toOre(vat.amount); scenarioOre-=amount;
      sources.push({source:"estimated_vat_assumption",amount:fromOre(-amount),reference:vat.reference,assumption:true,settlementStatus:"unknown"});
    }
    // Surface an undated monthly budget once, in the first forecast week that
    // intersects its month. This is presentation only; it never changes cash.
    const firstWeekOfMonthInHorizon = i === 0 || weekStart.slice(0,7) !== addDays(weekStart,-7).slice(0,7);
    for(const budget of budgetAssumptions.filter((item)=>firstWeekOfMonthInHorizon && item.period === weekStart.slice(0,7))){
      const amount=toOre(budget.amount); undatedBudgetOre-=amount;
      sources.push({source:"effective_budget_assumption",amount:fromOre(-amount),reference:budget.reference,assumption:true});
    }
    for(const item of supplements.filter((candidate)=>candidate.dueDate>=weekStart&&candidate.dueDate<=weekEnd)){
      const signed=item.direction==="inflow"?toOre(item.amount):-toOre(item.amount);
      switch(item.kind){
        case "approved_budget_assumption": budgetOre+=signed; break;
        case "approved_scenario_assumption": scenarioOre+=signed; break;
        case "legally_due_obligation": obligationOre-=signed; break;
        case "approved_intercompany_disposition": intercompanyOre+=signed; break;
      }
      sources.push({source:item.kind,amount:fromOre(signed),reference:item.reference,assumption:item.kind.includes("assumption") || item.kind === "approved_intercompany_disposition"});
    }
    if(i===0) for(const item of excludedSupplements){
      excluded.push({source:item.reference,amount:item.amount,currency:item.currency,reason:!input.companySlug||item.companySlug!==input.companySlug?"workspace_company_scope_mismatch":item.currency.toUpperCase()!=="DKK"?"dated_fx_required":"missing_review_or_invalid_canonical_reference"});
    }
    const openingCashOre=baseCashOre;
    const baseMovementOre=invOre-payableOre-commitmentOre-obligationOre;
    baseCashOre+=baseMovementOre;
    scenarioCashOre+=baseMovementOre+budgetOre+scenarioOre+intercompanyOre;
    lowestOre=baseCashOre<lowestOre?baseCashOre:lowestOre;
    rows.push({weekStart,openingCash:fromOre(openingCashOre),receivables:fromOre(invOre),payables:fromOre(payableOre),commitments:fromOre(commitmentOre),budgets:fromOre(budgetOre),undatedBudgetAssumptions:fromOre(undatedBudgetOre),scenarios:fromOre(scenarioOre),obligations:fromOre(obligationOre),intercompany:fromOre(intercompanyOre),excluded,closingCash:fromOre(baseCashOre),scenarioClosingCash:fromOre(scenarioCashOre),sources});
  }
  return {ok:true,startDate:start,openingCash:fromOre(openingOre),openingBalanceVerified,lowestPoint:fromOre(lowestOre),periods:rows,errors:[],appliedRules:["liquidity-forecast-13-week-v3"],completeness:{included:["observed opening cash","issued receivables","registered payables","approved DKK commitments","filing-safe closed/reported VAT obligations (settlement status unknown)","dated reviewed DKK supplements scoped by workspace company identity"],excluded:["monthly account budgets without a documented cash date (informational scenario upper-bound only)","open VAT periods (estimated scenario assumption, not legal obligation)","foreign-currency commitments or supplements without explicit dated FX","unregistered obligations","unreviewed, malformed, or wrong-company supplements"]}};
}
