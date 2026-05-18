// Dashboard CLI command — assembles DashboardInput from core APIs and writes
// the rendered HTML to --out. The render-engine in core/dashboard.ts is pure;
// all real-world data (clock, git, filesystem, db) is gathered here.

import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync } from "node:fs";
import { dirname } from "node:path";
import { companyPaths } from "../core/paths";
import { openDb, migrate } from "../core/db";
import { getCompanySettings } from "../core/company";
import { buildInvoiceList, buildOverdueInvoiceList } from "../core/invoice-list";
import { listBankTransactions } from "../core/reconciliation";
import { listExceptions } from "../core/exceptions";
import { buildVatReport } from "../core/vat";
import { getBackupComplianceStatus } from "../core/system-backups";
import { listRecentAuditLog } from "../core/audit-log";
import { verifyAuditChain } from "../core/ledger";
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

function quarterPeriodForDate(asOfDate: string): { start: string; end: string } {
  const m = /^(\d{4})-(\d{2})-(\d{2})/.exec(asOfDate);
  if (!m) {
    // Fallback: same date for start and end. Render-engine will then likely
    // produce a degenerate quarter, but error handling is reported elsewhere.
    return { start: asOfDate, end: asOfDate };
  }
  const year = parseInt(m[1]!, 10);
  const month = parseInt(m[2]!, 10);
  const quarter = Math.floor((month - 1) / 3) + 1;
  const startMonth = (quarter - 1) * 3 + 1;
  const endMonth = startMonth + 2;
  const lastDay = new Date(Date.UTC(year, endMonth, 0)).getUTCDate();
  const pad = (n: number) => String(n).padStart(2, "0");
  return {
    start: `${year}-${pad(startMonth)}-01`,
    end: `${year}-${pad(endMonth)}-${pad(lastDay)}`,
  };
}

function daysBetween(a: string, b: string): number {
  const pa = /^(\d{4})-(\d{2})-(\d{2})/.exec(a);
  const pb = /^(\d{4})-(\d{2})-(\d{2})/.exec(b);
  if (!pa || !pb) return 0;
  const da = Date.UTC(parseInt(pa[1]!, 10), parseInt(pa[2]!, 10) - 1, parseInt(pa[3]!, 10));
  const db = Date.UTC(parseInt(pb[1]!, 10), parseInt(pb[2]!, 10) - 1, parseInt(pb[3]!, 10));
  return Math.round((db - da) / 86400000);
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
    const period = quarterPeriodForDate(asOfDate);
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
