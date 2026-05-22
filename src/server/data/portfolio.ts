// Portfolio + per-company dashboard read assembly for the cockpit (#320).
//
// Split out of `server/data.ts` by #320. The portfolio overview rolls one
// real-figure summary per workspace company into the cockpit's landing page;
// the per-company dashboard data backs the static-dashboard-equivalent JSON
// view. Every figure is computed by an existing core function or a shared data
// helper — no business logic is duplicated and nothing here mutates a ledger.
// Behaviour is unchanged from the pre-split `server/data.ts`.

import { existsSync } from "node:fs";
import type { Database } from "bun:sqlite";
import { companyPaths } from "../../core/paths";
import { diffDaysSafe as daysBetween } from "../../core/dates";
import { openDb, migrate } from "../../core/db";
import { getCompanySettings } from "../../core/company";
import { buildInvoiceList, buildOverdueInvoiceList } from "../../core/invoice-list";
import { listBankTransactions } from "../../core/reconciliation";
import { listExceptions } from "../../core/exceptions";
import { buildVatReport } from "../../core/vat";
import { buildProfitAndLoss } from "../../core/financial-statements";
import { getBackupComplianceStatus } from "../../core/system-backups";
import { listRecentAuditLog } from "../../core/audit-log";
import { verifyAuditChain } from "../../core/ledger";
import {
  companyRootForSlug,
  findWorkspaceCompany,
  type WorkspaceCompanyEntry,
} from "../../core/workspace";
import { discoverWorkspaceCompanies } from "../discovery";
import { ApiError } from "../errors";
import { currentFiscalYear, roundKroner, todayIsoDate } from "./shared";
import { actualBankBalanceAsOf } from "./bank";
import { selectVatPeriod } from "./vat";
import { groupExceptions, type ExceptionGroup } from "./exceptions";

// --------------------------------------------------------------------------
// Per-company summary (one row in the portfolio overview)
// --------------------------------------------------------------------------

/** The VAT block on a portfolio card — null when no VAT period is known. */
export type CompanyVatSummary = {
  /** Net VAT payable for the company's current quarter, kroner. */
  payable: number;
  /** The statutory filing/payment deadline (YYYY-MM-DD). */
  deadline: string;
  /** Whole days from today to the deadline; negative when overdue. */
  daysRemaining: number;
};

export type CompanySummary = {
  slug: string;
  name: string;
  cvr: string | null;
  archived: boolean;
  /** True when the slug is registered but has no ledger on disk. */
  ledgerMissing: boolean;
  /** The fiscal year these figures cover, e.g. "2026"; null when unknown. */
  fiscalYear: string | null;
  /** Year-to-date result (resultat) for the current fiscal year, kroner. */
  resultat: number;
  /** Year-to-date revenue (omsætning) for the current fiscal year, kroner. */
  omsaetning: number;
  /**
   * Actual bank balance from the imported statement, kroner — what the bank
   * app shows. Null when no statement balance is known for the company.
   */
  actualBankBalance: number | null;
  /** Current half-year VAT position + deadline; null when unknown. */
  vat: CompanyVatSummary | null;
  /** Open tasks — open exceptions, grouped into Danish summary lines. */
  openTaskCount: number;
  taskGroups: ExceptionGroup[];
  auditChainOk: boolean;
  // --- legacy fields retained for the MCP/older consumers -----------------
  openInvoiceCount: number;
  openInvoiceTotal: number;
  overdueInvoiceCount: number;
  unlinkedBankCount: number;
  /** Alias of `openTaskCount`. */
  openExceptionCount: number;
  /** Alias of `vat.payable` (0 when no VAT). */
  netVatPayable: number;
};

function summariseCompany(
  workspaceRoot: string,
  entry: WorkspaceCompanyEntry,
): CompanySummary {
  const companyRoot = companyRootForSlug(workspaceRoot, entry.slug);
  const dbPath = companyPaths(companyRoot).db;
  if (!existsSync(dbPath)) {
    return {
      slug: entry.slug,
      name: entry.name,
      cvr: null,
      archived: entry.archived,
      ledgerMissing: true,
      fiscalYear: null,
      resultat: 0,
      omsaetning: 0,
      actualBankBalance: null,
      vat: null,
      openTaskCount: 0,
      taskGroups: [],
      auditChainOk: false,
      openInvoiceCount: 0,
      openInvoiceTotal: 0,
      overdueInvoiceCount: 0,
      unlinkedBankCount: 0,
      openExceptionCount: 0,
      netVatPayable: 0,
    };
  }

  let db: Database;
  try {
    db = openDb(dbPath);
  } catch {
    // An unreadable ledger degrades gracefully — treated as "missing".
    return {
      slug: entry.slug,
      name: entry.name,
      cvr: null,
      archived: entry.archived,
      ledgerMissing: true,
      fiscalYear: null,
      resultat: 0,
      omsaetning: 0,
      actualBankBalance: null,
      vat: null,
      openTaskCount: 0,
      taskGroups: [],
      auditChainOk: false,
      openInvoiceCount: 0,
      openInvoiceTotal: 0,
      overdueInvoiceCount: 0,
      unlinkedBankCount: 0,
      openExceptionCount: 0,
      netVatPayable: 0,
    };
  }
  try {
    migrate(db);
    const company = getCompanySettings(db);
    const { label: fyLabel, year: yearNum } = currentFiscalYear(db, company);
    const yearStart = `${yearNum}-01-01`;
    const yearEnd = `${yearNum}-12-31`;

    // Resultat + omsætning — the same P&L the per-company Overblik renders.
    const pl = buildProfitAndLoss(db, yearStart, yearEnd);

    // Actual bank balance from the imported statement (what the bank shows).
    const actualBankBalance = actualBankBalanceAsOf(db, yearEnd);

    // VAT: the booked position for the company's actual VAT period — the
    // period (month / quarter / half-year, per `vatPeriodType`) that is due
    // now. Every cockpit surface reads the same cadence, so they agree (#299).
    const vatPeriod = selectVatPeriod(db, yearNum, company.vatPeriodType);
    const vat: CompanyVatSummary = {
      payable: vatPeriod.position.payable,
      deadline: vatPeriod.deadline,
      daysRemaining: daysBetween(todayIsoDate(), vatPeriod.deadline),
    };

    // Open tasks — open exceptions grouped into Danish summary lines.
    const exceptions = listExceptions(db, { status: "open" });
    const taskGroups = groupExceptions(
      exceptions.rows.map((row: any) => ({
        type: row.type,
        severity: row.severity,
      })),
    );

    // Legacy figures retained for older consumers.
    const invoices = buildInvoiceList(db, { status: "open", asOfDate: yearEnd });
    const overdue = buildOverdueInvoiceList(db, { asOfDate: yearEnd });
    const unlinked = listBankTransactions(db, { status: "unmatched" });
    const audit = verifyAuditChain(db);

    return {
      slug: entry.slug,
      name: company.name,
      cvr: company.cvr,
      archived: entry.archived,
      ledgerMissing: false,
      fiscalYear: fyLabel,
      resultat: pl.result,
      omsaetning: pl.totalIncome,
      actualBankBalance,
      vat,
      openTaskCount: exceptions.count,
      taskGroups,
      auditChainOk: audit.ok,
      openInvoiceCount: invoices.count,
      openInvoiceTotal: roundKroner(
        invoices.rows.reduce((acc, r) => acc + r.openBalance, 0),
      ),
      overdueInvoiceCount: overdue.count,
      unlinkedBankCount: unlinked.count,
      openExceptionCount: exceptions.count,
      netVatPayable: vat.payable,
    };
  } finally {
    db.close();
  }
}

export type PortfolioOverview = {
  workspace: string;
  asOf: string;
  companyCount: number;
  /**
   * Workspace-wide roll-up — "how is the whole portfolio doing". The figures
   * are summed across legal entities for a portfolio glance; each company is
   * still a separate entity and is judged on its own card.
   */
  rollup: {
    /** Combined year-to-date result across all companies, kroner. */
    resultat: number;
    /** Combined liquidity — actual bank balance across all companies, kroner. */
    liquidity: number;
    /** Combined VAT owed across all companies, kroner. */
    vatPayable: number;
    /** Total open tasks across all companies. */
    openTaskCount: number;
  };
  /** Legacy totals block, retained for older consumers. */
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

/**
 * Aggregates one real-figure summary per workspace company plus a
 * workspace-wide roll-up. Each company's figures cover its current fiscal
 * year, computed by reusing the per-company core logic (`buildProfitAndLoss`,
 * `actualBankBalanceAsOf`, `vatPositionForPeriod`, the exception grouping).
 * `asOfDate` is retained on the response for backwards compatibility; the
 * figures themselves are year-to-date for each company's fiscal year.
 */
export function buildPortfolioOverview(
  workspaceRoot: string,
  asOfDate: string,
): PortfolioOverview {
  // Discover-and-adopt any present-but-unlisted company directory first
  // (#256): the portfolio is the cockpit's landing page, so an owner who set
  // a company up via the CLI must land on that real company — not onboarding.
  const entries = discoverWorkspaceCompanies(workspaceRoot);
  const companies = entries.map((entry) =>
    summariseCompany(workspaceRoot, entry),
  );
  const rollup = companies.reduce(
    (acc, c) => ({
      resultat: acc.resultat + c.resultat,
      liquidity: acc.liquidity + (c.actualBankBalance ?? 0),
      vatPayable: acc.vatPayable + (c.vat?.payable ?? 0),
      openTaskCount: acc.openTaskCount + c.openTaskCount,
    }),
    { resultat: 0, liquidity: 0, vatPayable: 0, openTaskCount: 0 },
  );
  const totals = companies.reduce(
    (acc, c) => ({
      openInvoiceCount: acc.openInvoiceCount + c.openInvoiceCount,
      openInvoiceTotal: acc.openInvoiceTotal + c.openInvoiceTotal,
      overdueInvoiceCount: acc.overdueInvoiceCount + c.overdueInvoiceCount,
      unlinkedBankCount: acc.unlinkedBankCount + c.unlinkedBankCount,
      openExceptionCount: acc.openExceptionCount + c.openExceptionCount,
      netVatPayable: acc.netVatPayable + c.netVatPayable,
    }),
    {
      openInvoiceCount: 0,
      openInvoiceTotal: 0,
      overdueInvoiceCount: 0,
      unlinkedBankCount: 0,
      openExceptionCount: 0,
      netVatPayable: 0,
    },
  );
  return {
    workspace: workspaceRoot,
    asOf: asOfDate,
    companyCount: companies.length,
    rollup: {
      resultat: roundKroner(rollup.resultat),
      liquidity: roundKroner(rollup.liquidity),
      vatPayable: roundKroner(rollup.vatPayable),
      openTaskCount: rollup.openTaskCount,
    },
    totals: {
      openInvoiceCount: totals.openInvoiceCount,
      openInvoiceTotal: roundKroner(totals.openInvoiceTotal),
      overdueInvoiceCount: totals.overdueInvoiceCount,
      unlinkedBankCount: totals.unlinkedBankCount,
      openExceptionCount: totals.openExceptionCount,
      netVatPayable: roundKroner(totals.netVatPayable),
    },
    companies,
  };
}

// --------------------------------------------------------------------------
// Per-company dashboard data
// --------------------------------------------------------------------------

export type CompanyDashboardData = ReturnType<typeof buildCompanyDashboardData>;

/**
 * Per-company dashboard data — the same figures the static HTML dashboard
 * (`src/core/dashboard.ts`) renders, returned as JSON for the cockpit SPA.
 *
 * Throws `ApiError.notFound` when the slug is not registered or the ledger is
 * missing on disk.
 */
export function buildCompanyDashboardData(
  workspaceRoot: string,
  slug: string,
  asOfDate: string,
) {
  const entry = findWorkspaceCompany(workspaceRoot, slug);
  if (!entry) {
    throw ApiError.notFound(`no company with slug '${slug}' in the workspace`);
  }
  const companyRoot = companyRootForSlug(workspaceRoot, slug);
  const dbPath = companyPaths(companyRoot).db;
  if (!existsSync(dbPath)) {
    throw ApiError.notFound(`company '${slug}' has no ledger`);
  }

  const db = openDb(dbPath);
  try {
    migrate(db);
    const company = getCompanySettings(db);
    const invoices = buildInvoiceList(db, { status: "open", asOfDate });
    const overdueInvoices = buildOverdueInvoiceList(db, { asOfDate });
    const unlinkedBank = listBankTransactions(db, { status: "unmatched" });
    const exceptions = listExceptions(db, { status: "open" });
    // VAT: surface the earliest unreported VAT period — the one the owner must
    // file now — exactly as the Overblik card and `vat momsangivelse` do
    // (#281). #299: the period follows the company's real VAT cadence
    // (`vatPeriodType`), so a monthly/half-yearly filer sees its own period.
    const { year: vatYear } = currentFiscalYear(db, company);
    const vatSelection = selectVatPeriod(db, vatYear, company.vatPeriodType);
    const period = { start: vatSelection.start, end: vatSelection.end };
    const vatPeriod = buildVatReport(db, period.start, period.end);
    const vatDeadline = vatSelection.deadline;
    const vatDaysRemaining = daysBetween(asOfDate, vatDeadline);
    const recentActivity = listRecentAuditLog(db, 10);
    const backup = getBackupComplianceStatus(db, companyRoot, asOfDate);
    const audit = verifyAuditChain(db);

    return {
      slug: entry.slug,
      asOf: asOfDate,
      company: {
        name: company.name,
        cvr: company.cvr,
        country: company.country,
        currency: company.currency,
        fiscalYearStartMonth: company.fiscalYearStartMonth,
        fiscalYearLabelStrategy: company.fiscalYearLabelStrategy,
      },
      invoices: {
        count: invoices.count,
        openTotal: invoices.rows.reduce((acc, r) => acc + r.openBalance, 0),
        rows: invoices.rows,
      },
      overdueInvoices: {
        count: overdueInvoices.count,
        rows: overdueInvoices.rows,
      },
      unlinkedBank: { count: unlinkedBank.count },
      exceptions: {
        count: exceptions.count,
        rows: exceptions.rows.map((row: any) => ({
          id: row.id,
          type: row.type,
          severity: row.severity,
          status: row.status,
          message: row.message,
        })),
      },
      vat: {
        periodStart: period.start,
        periodEnd: period.end,
        netVatPayable: vatPeriod.netVatPayable,
        daysRemaining: vatDaysRemaining,
        errors: vatPeriod.errors ?? [],
      },
      backup: {
        backupsFound: backup.backupsFound,
        latestBackupAt: backup.latestBackupAt,
        daysSinceLatestBackup: backup.daysSinceLatestBackup,
        hasActivitySinceBackup: backup.hasActivitySinceBackup,
      },
      audit: {
        ok: audit.ok,
        entryCount: audit.entries,
        firstError: audit.errors[0] ?? null,
      },
      recentActivity,
    };
  } finally {
    db.close();
  }
}
