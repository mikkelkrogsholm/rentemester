// Dashboard CLI command — assembles DashboardInput from core APIs and writes
// the rendered HTML to --out. The render-engine in core/dashboard.ts is pure;
// all real-world data (clock, git, filesystem, db) is gathered here.

import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync } from "node:fs";
import { dirname } from "node:path";
import { companyPaths } from "../core/paths";
import { diffDaysSafe as daysBetween } from "../core/dates";
import { openDb, migrate } from "../core/db";
import { getCompanySettings } from "../core/company";
import { buildInvoiceList, buildOverdueInvoiceList } from "../core/invoice-list";
import { listBankTransactions } from "../core/reconciliation";
import { listExceptions } from "../core/exceptions";
import { buildVatReport, type VatPeriodReport } from "../core/vat";
import {
  effectivePeriodState,
  vatPeriodWindowFor,
  vatPeriodsForYear,
  type VatPeriodType,
} from "../core/periods";
import { getBackupComplianceStatus } from "../core/system-backups";
import { listRecentAuditLog } from "../core/audit-log";
import { verifyAuditChain } from "../core/ledger";
import type { Database } from "bun:sqlite";
import { currentRuleBundleVersion } from "../core/rules-metadata";
import {
  renderDashboard,
  type DashboardInput,
  type DashboardExceptionsResult,
} from "../core/dashboard";
import { writeTempFileFor, promoteTempFile, removeIfExists } from "../core/atomic-file";
import type { CommandDispatch } from "../cli-dispatch";

const ISO_DATE_RE = /^\d{4}-\d{2}-\d{2}$/;

function todayIsoDate(): string {
  // Pure CLI-side use of the wall clock; render-engine stays deterministic.
  const now = new Date();
  const y = now.getUTCFullYear();
  const m = String(now.getUTCMonth() + 1).padStart(2, "0");
  const d = String(now.getUTCDate()).padStart(2, "0");
  return `${y}-${m}-${d}`;
}

/**
 * The VAT period window containing `asOfDate`, for a company on `vatType`.
 * #299: a fallback only — `selectVatPeriodForDashboard` prefers an earlier
 * unreported period that carries activity. A monthly/half-yearly company gets
 * its real cadence's window; a quarterly company keeps the calendar quarter.
 */
function vatPeriodForDate(
  asOfDate: string,
  vatType: VatPeriodType,
): { start: string; end: string } {
  if (!/^(\d{4})-(\d{2})-(\d{2})/.test(asOfDate)) {
    // Render-engine will then likely produce a degenerate period, but error
    // handling is reported elsewhere.
    return { start: asOfDate, end: asOfDate };
  }
  const window = vatPeriodWindowFor(asOfDate, vatType);
  return { start: window.start, end: window.end };
}

/** Whether a VAT report carries any booked VAT activity at all. */
function vatReportHasActivity(report: VatPeriodReport): boolean {
  return (
    report.outputVat !== 0 ||
    report.inputVat !== 0 ||
    report.netVatPayable !== 0 ||
    report.totalJournalEntryCount > 0
  );
}

/**
 * Whether the VAT period `start`..`end` has already been reported to SKAT —
 * i.e. an `accounting_periods` row whose bounds exactly match the window is in
 * the effective `reported` state. A reported period's momsangivelse is filed
 * and paid, so the dashboard must look past it to the next outstanding one.
 */
function vatPeriodIsReported(db: Database, start: string, end: string): boolean {
  const row = db
    .query(
      `SELECT id, status
         FROM accounting_periods
        WHERE kind = 'vat_quarter'
          AND period_start = ? AND period_end = ?
        ORDER BY id DESC
        LIMIT 1`,
    )
    .get(start, end) as { id: number; status: "open" | "closed" | "reported" } | undefined;
  if (!row) return false;
  return effectivePeriodState(db, row.id, row.status) === "reported";
}

/**
 * The VAT period the dashboard's "Næste momsfrist" box must surface.
 *
 * #299: the period follows the company's real VAT cadence (`vatType`) — a
 * monthly company is scanned month-by-month, a half-yearly company half-by-half
 * — so the box describes the SKAT period the company is actually registered
 * for, not a hardcoded quarter.
 *
 * The owner needs to see the *earliest* period whose momsangivelse is still
 * outstanding — the one that costs money if forgotten — NOT whichever period
 * today happens to fall in. Picking the current period (the old behaviour) hid
 * a booked, still-unpaid earlier period behind an empty "0,00 DKK" box. (#281)
 *
 * Selection: scan the prior year's periods then the as-of year's, in
 * chronological order, and return the first period that carries booked VAT
 * activity and is not yet reported. When nothing qualifies (a fresh company,
 * or every active period already filed) fall back to the period the as-of date
 * falls in, so the box still shows a sensible upcoming deadline.
 */
function selectVatPeriodForDashboard(
  db: Database,
  asOfDate: string,
  vatType: VatPeriodType,
): { start: string; end: string } {
  const m = /^(\d{4})-(\d{2})-(\d{2})/.exec(asOfDate);
  const fallback = vatPeriodForDate(asOfDate, vatType);
  if (!m) return fallback;
  const asOfYear = parseInt(m[1]!, 10);

  // Scan chronologically: last year's periods, then this year's. The earliest
  // unreported period with activity is the one currently owed.
  for (const year of [asOfYear - 1, asOfYear]) {
    for (const window of vatPeriodsForYear(year, vatType)) {
      // Don't surface a period that has not started yet.
      if (window.start > asOfDate) continue;
      const report = buildVatReport(db, window.start, window.end);
      if (!report.ok) continue;
      if (!vatReportHasActivity(report)) continue;
      if (vatPeriodIsReported(db, window.start, window.end)) continue;
      return { start: window.start, end: window.end };
    }
  }
  // No outstanding period — show the period today falls in.
  return fallback;
}

function shortCommitSha(): string {
  // Best-effort. Release tarballs or sandboxes without git will fall back to
  // 'unknown' and the render-engine still produces deterministic HTML.
  try {
    const result = spawnSync("git", ["rev-parse", "--short=7", "HEAD"], {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    });
    if (result.status === 0) {
      const sha = (result.stdout ?? "").trim();
      if (/^[0-9a-f]{7,}$/i.test(sha)) return sha.slice(0, 7);
    }
  } catch {
    // ignore
  }
  return "unknown";
}

function openInBrowser(path: string): void {
  // Detached: don't block CLI exit. spawnSync without detach but with quick
  // commands like `open` returns immediately on macOS.
  const platform = process.platform;
  let cmd: string;
  let args: string[];
  if (platform === "darwin") {
    cmd = "open";
    args = [path];
  } else if (platform === "win32") {
    cmd = "cmd";
    args = ["/c", "start", "", path];
  } else {
    cmd = "xdg-open";
    args = [path];
  }
  try {
    spawnSync(cmd, args, { stdio: "ignore" });
  } catch {
    // ignore; failing to open the browser must not fail the dashboard write.
  }
}

function buildExceptionsForDashboard(
  result: ReturnType<typeof listExceptions>,
): DashboardExceptionsResult {
  return {
    ok: result.ok,
    count: result.count,
    errors: result.errors,
    rows: result.rows.map((row: any) => ({
      id: row.id,
      type: row.type,
      severity: row.severity,
      status: row.status,
      message: row.message,
    })),
  };
}

export function register(dispatch: CommandDispatch): void {
  dispatch.on("dashboard", null, (ctx) => {
    const outPath = ctx.arg("--out");
    if (!outPath) {
      console.error("Missing required --out <file.html>");
      process.exit(2);
    }
    const asOfDate = ctx.arg("--as-of") ?? todayIsoDate();
    if (!ISO_DATE_RE.test(asOfDate)) {
      console.error("--as-of must be YYYY-MM-DD");
      process.exit(2);
    }

    const companyRoot = ctx.companyRoot();
    const db = openDb(companyPaths(companyRoot).db);
    migrate(db);

    const company = getCompanySettings(db);
    const invoices = buildInvoiceList(db, { status: "open", asOfDate });
    const overdueInvoices = buildOverdueInvoiceList(db, { asOfDate });
    const unlinkedBank = listBankTransactions(db, { status: "unmatched" });
    const exceptions = buildExceptionsForDashboard(listExceptions(db, { status: "open" }));
    // The "Næste momsfrist" box must point at the earliest VAT period that is
    // still unreported and carries activity — not the period today falls in
    // (#281). #299: the period follows the company's real VAT cadence
    // (`vatPeriodType`). The render-engine derives its label/countdown from
    // `vatPeriod.periodStart` + `company.vatPeriodType`, so the box always
    // matches the figure shown.
    const period = selectVatPeriodForDashboard(db, asOfDate, company.vatPeriodType);
    const vatPeriod = buildVatReport(db, period.start, period.end);
    const vatDaysRemaining = daysBetween(asOfDate, period.end);
    const recentActivity = listRecentAuditLog(db, 10);
    const backup = getBackupComplianceStatus(db, companyRoot, asOfDate);
    const auditResult = verifyAuditChain(db);

    const generatedAt = new Date().toISOString();
    const ruleBundleVersion = (() => {
      try {
        return currentRuleBundleVersion();
      } catch {
        return "unknown";
      }
    })();
    const commitSha = shortCommitSha();

    const input: DashboardInput = {
      asOfDate,
      generatedAt,
      commitSha,
      ruleBundleVersion,
      company,
      invoices,
      overdueInvoices,
      unlinkedBank,
      exceptions,
      vatPeriod,
      vatDaysRemaining,
      recentActivity,
      backup,
      audit: {
        ok: auditResult.ok,
        entryCount: auditResult.entries,
        firstError: auditResult.errors[0],
      },
    };

    const startNs =
      typeof Bun !== "undefined" && typeof Bun.nanoseconds === "function"
        ? Bun.nanoseconds()
        : Date.now() * 1_000_000;
    const html = renderDashboard(input);
    const elapsedMs =
      ((typeof Bun !== "undefined" && typeof Bun.nanoseconds === "function"
        ? Bun.nanoseconds()
        : Date.now() * 1_000_000) -
        startNs) /
      1_000_000;

    const outDir = dirname(outPath);
    if (outDir && !existsSync(outDir)) {
      mkdirSync(outDir, { recursive: true });
    }
    const tempPath = writeTempFileFor(outPath, html);
    try {
      promoteTempFile(tempPath, outPath);
    } catch (err) {
      removeIfExists(tempPath);
      throw err;
    }

    db.close();

    const opened = ctx.hasFlag("--open");
    if (opened) openInBrowser(outPath);

    const result = {
      ok: true,
      outPath,
      asOfDate,
      bytes: Buffer.byteLength(html, "utf8"),
      renderMs: Number(elapsedMs.toFixed(3)),
      commitSha,
      ruleBundleVersion,
      opened,
    };
    ctx.emitResult(result as Record<string, unknown>);
  });
}
