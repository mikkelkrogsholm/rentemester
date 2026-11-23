import { diffDaysSafe as daysBetween } from "../../../core/dates";
import { annualReportDeadline, fiscalYearForDate } from "../../../core/fiscal-year";
import {
  vatPeriodsForYear,
  vatPeriodLabel,
} from "../../../core/periods";
import {
  resolveStatementContext,
  roundKroner,
  statementCompanyBlock,
  todayIsoDate,
} from "../shared";
import { vatPositionForPeriod } from "../vat";
import { loadVatAccountSemantics } from "../../../core/vat-account-semantics";

// --------------------------------------------------------------------------
// Per-company obligations (Forpligtelser — what the company owes, year-aware)
// — cockpit-redesign Runde 2, iteration 7
// --------------------------------------------------------------------------

/** One thing the company owes — a payable surfaced from the ledger. */
export type ObligationRow = {
  /** A short, stable key for the obligation kind. */
  kind:
    | "vat"
    | "corporation-tax"
    | "annual-report"
    | "creditors"
    | "auditor"
    | "other";
  /** A human Danish label, e.g. "Moms — Q2 2026". */
  label: string;
  /** The amount owed, kroner; positive is payable. */
  amount: number;
  /** The filing/payment deadline as YYYY-MM-DD, or null when none is known. */
  dueDate: string | null;
  /** Signed countdown from today to `dueDate`; null when `dueDate` is null. */
  daysRemaining: number | null;
  /** The ledger account the figure was read from, when one applies. */
  accountNo: string | null;
};

export type CompanyObligations = ReturnType<typeof buildCompanyObligations>;

/**
 * Standard Danish liability account numbers (the Dinero chart) whose meaning
 * is well-known enough to label precisely and — for VAT and corporation tax —
 * carry a derived deadline. Trade creditors and accrued auditor have no
 * statutory date the ledger can derive, so their `dueDate` is left null.
 */
const KNOWN_LIABILITY_ACCOUNTS: Record<
  string,
  { kind: ObligationRow["kind"]; label: string }
> = {
  "63000": { kind: "creditors", label: "Kreditorer (leverandørgæld)" },
  "63040": { kind: "auditor", label: "Afsat revisor" },
  "63060": { kind: "corporation-tax", label: "Skyldig selskabsskat" },
};

/**
 * The credit-signed balance (credit − debit, kroner) of every `liability`-type
 * account at `asOfDate`, excluding canonical VAT amount/settlement accounts.
 *
 * No VAT account may appear as a liability row here — VAT is surfaced as its
 * own single obligation from the booked VAT position (`vatPositionForPeriod`),
 * and that net figure already represents the *whole* VAT obligation. The gross
 * VAT accounts (output VAT `64000`, foreign-services reverse-charge `64040`,
 * input VAT `64060`, …) are merely *components* of that computation, so
 * counting them here as well would double-count VAT. The exclusion uses the
 * same VAT-account semantics as `vatPositionForPeriod`/`buildVatReport`; no
 * broad account-number range is independently guessed here.
 */
function liabilityBalancesAsOf(
  db: import("bun:sqlite").Database,
  asOfDate: string,
): Array<{ accountNo: string; name: string; balance: number }> {
  const rows = db
    .query(
      `SELECT a.account_no AS accountNo,
              a.name       AS name,
              a.type AS accountType,
              a.normal_balance AS normalBalance,
              COALESCE(SUM(jl.credit_amount - jl.debit_amount), 0) AS balance
         FROM accounts a
         JOIN journal_lines jl     ON jl.account_id = a.id
         JOIN journal_entries je   ON je.id = jl.journal_entry_id
        WHERE a.type = 'liability'
          AND je.status = 'posted'
          AND je.transaction_date <= ?
          AND NOT EXISTS (
            SELECT 1 FROM dinero_vat_role_reviews r JOIN account_role_mappings m ON m.id = r.mapping_id
            WHERE m.account_no = a.account_no AND coalesce(je.reversal_of_entry_id, je.id) > r.last_existing_entry_id
          )
        GROUP BY a.id
        ORDER BY a.account_no ASC`,
    )
    .all(asOfDate) as Array<{
    accountNo: string;
    name: string;
    accountType: string;
    normalBalance: "debit" | "credit";
    balance: number;
  }>;
  const semantics = loadVatAccountSemantics(db);
  return rows
    .filter(
      (r) =>
        !semantics.amountSideByAccountNo.has(r.accountNo) &&
        !semantics.settlementAccountNos.has(r.accountNo) &&
        !/momsafregning/i.test(r.name),
    )
    .map((r) => ({
      accountNo: r.accountNo,
      name: r.name,
      balance: roundKroner(r.balance),
    }));
}

/**
 * Forpligtelser — "what does the company owe, and when". A year-aware list of
 * the company's outstanding payables, each with the amount owed and a due date
 * where one is derivable. Every figure is read straight from the posted
 * ledger:
 *
 *  - VAT — the booked position for each registered VAT period
 *    (`vatPositionForPeriod`); its deadline comes from that cadence's
 *    canonical period window.
 *  - Corporation tax, trade creditors, accrued auditor and any other payable
 *    — the credit balance of the `liability`-type accounts at the year end.
 *    Known account numbers get a precise Danish label; corporation tax also
 *    gets a derived SKAT deadline. The rest carry no date — that is fine and
 *    shown as "—" in the UI.
 *
 * Rows are returned sorted by due date (soonest first); rows with no date sink
 * to the bottom. Money is kroner. Throws `ApiError.notFound` when the slug is
 * not registered or has no ledger.
 */
export function buildCompanyObligations(
  workspaceRoot: string,
  slug: string,
  year: number | null,
) {
  const ctx = resolveStatementContext(workspaceRoot, slug, year);
  try {
    const companyBlock = statementCompanyBlock(ctx.company);
    if (ctx.isArchivedOnly) {
      return {
        slug: ctx.entry.slug,
        selectedYear: ctx.selectedLabel,
        archived: true,
        company: companyBlock,
        fiscalYears: ctx.years,
        obligations: [] as ObligationRow[],
        totalOwed: 0,
      };
    }

    const today = todayIsoDate();
    const yearNum = parseInt(ctx.selectedLabel, 10);
    const yearEnd = `${yearNum}-12-31`;
    const obligations: ObligationRow[] = [];

    // VAT: each VAT period settles separately. Surface every period that
    // carries a payable so the owner sees each filing deadline; if no period
    // has a payable, no VAT obligation is shown. #299: the periods follow the
    // company's real VAT cadence (`vatPeriodType`) — a monthly filer sees up to
    // twelve VAT lines, a half-yearly filer two — never a hardcoded quarter.
    // A non-registered company has no VAT periods — `vatPeriodsForYear`
    // returns `[]` for a null cadence, so the loop body simply never runs
    // and the obligations list carries zero VAT lines.
    for (const window of vatPeriodsForYear(yearNum, ctx.company.vatPeriodType)) {
      const position = vatPositionForPeriod(ctx.db, window.start, window.end);
      if (position.payable > 0) {
        obligations.push({
          kind: "vat",
          label: `Moms — ${vatPeriodLabel(window)}`,
          amount: position.payable,
          dueDate: window.filingDeadline,
          daysRemaining: daysBetween(today, window.filingDeadline),
          accountNo: null,
        });
      }
    }

    // Annual report (årsrapport) — the statutory filing to Erhvervsstyrelsen.
    // It is not a ledger payable (it has no amount owed), but it is the other
    // recurring legal deadline an owner must not miss, so the Forpligtelser
    // screen surfaces it alongside VAT (#290). The deadline is the statutory
    // ÅRL § 138 one — 6 months after the fiscal year ends — computed by the
    // canonical `annualReportDeadline`, shared with `agent run`
    // (`src/agent/loop.ts#checkDeadlines`). The fiscal year is derived from
    // the company's own `fiscalYearStartMonth` / label strategy, so a
    // non-calendar year is handled correctly. `amount` is 0 — it is a
    // deadline, not a debt.
    const fy = fiscalYearForDate(
      yearEnd,
      ctx.company.fiscalYearStartMonth,
      ctx.company.fiscalYearLabelStrategy,
    );
    const annualReportDueDate = annualReportDeadline(fy.end);
    obligations.push({
      kind: "annual-report",
      label: `Årsrapport — regnskabsår ${fy.displayLabel} (ÅRL § 138)`,
      amount: 0,
      dueDate: annualReportDueDate,
      daysRemaining: daysBetween(today, annualReportDueDate),
      accountNo: null,
    });

    // Liability accounts with a credit balance — corporation tax, trade
    // creditors, accrued auditor and anything else. A debit (negative) balance
    // is not a payable, so it is skipped. Corporation tax for an income year
    // is due to SKAT on 1 November of the following year (the standard ApS
    // restskat deadline) — the only liability date the ledger can derive.
    for (const acc of liabilityBalancesAsOf(ctx.db, yearEnd)) {
      if (acc.balance <= 0) continue;
      const known = KNOWN_LIABILITY_ACCOUNTS[acc.accountNo];
      const kind: ObligationRow["kind"] = known?.kind ?? "other";
      const label = known?.label ?? acc.name;
      const dueDate =
        kind === "corporation-tax" ? `${yearNum + 1}-11-01` : null;
      obligations.push({
        kind,
        label,
        amount: acc.balance,
        dueDate,
        daysRemaining: dueDate === null ? null : daysBetween(today, dueDate),
        accountNo: acc.accountNo,
      });
    }

    // Sorted by due date, soonest first; dateless rows sink to the bottom.
    // Ties break by descending amount, then by label — fully deterministic.
    obligations.sort((a, b) => {
      if (a.dueDate !== b.dueDate) {
        if (a.dueDate === null) return 1;
        if (b.dueDate === null) return -1;
        return a.dueDate.localeCompare(b.dueDate);
      }
      if (a.amount !== b.amount) return b.amount - a.amount;
      return a.label.localeCompare(b.label, "da");
    });

    const totalOwed = roundKroner(
      obligations.reduce((acc, o) => acc + o.amount, 0),
    );

    return {
      slug: ctx.entry.slug,
      selectedYear: ctx.selectedLabel,
      archived: false,
      company: companyBlock,
      fiscalYears: ctx.years,
      obligations,
      totalOwed,
    };
  } finally {
    ctx.db.close();
  }
}
