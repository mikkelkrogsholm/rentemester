// Compliance-report CLI command — assembles ComplianceReportInput from core
// APIs and writes the rendered HTML to --out. The render-engine in
// core/compliance-report.ts is pure; all real-world data (clock, git,
// filesystem, db) is gathered here.

import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";
import type { Database } from "bun:sqlite";
import { companyPaths } from "../core/paths";
import { openDb, migrate } from "../core/db";
import { getCompanySettings } from "../core/company";
import { verifyAuditChain } from "../core/ledger";
import { buildRetentionStatusReport } from "../core/retention";
import { getBackupGovernanceStatus } from "../core/backup-governance";
import { buildGdprAuditExport } from "../core/gdpr";
import { computeRegulatoryCoverage } from "../core/regulatory-coverage";
import { readRuleMetadata, currentRuleBundleVersion } from "../core/rules-metadata";
import { fiscalYearForDate } from "../core/fiscal-year";
import { renderComplianceReport, complianceReportFingerprint } from "../core/compliance-report";
import type { CommandDispatch } from "../cli-dispatch";

const ISO_DATE_RE = /^\d{4}-\d{2}-\d{2}$/;

function todayIsoDate(): string {
  return new Date().toISOString().slice(0, 10);
}

function shortCommitSha(): string {
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

function lastClosedPeriodLabel(db: Database): { count: number; label: string | null } {
  const counted = db
    .query(
      "SELECT COUNT(*) AS n FROM accounting_periods WHERE status = 'closed'",
    )
    .get() as { n: number };
  const latest = db
    .query(
      `SELECT period_start, period_end, kind FROM accounting_periods
        WHERE status = 'closed'
        ORDER BY period_end DESC, id DESC LIMIT 1`,
    )
    .get() as { period_start: string; period_end: string; kind: string } | undefined;
  if (!latest) return { count: counted.n, label: null };
  return {
    count: counted.n,
    label: `${latest.kind} ${latest.period_start} → ${latest.period_end}`,
  };
}

export function register(dispatch: CommandDispatch): void {
  dispatch.on("compliance", "report", (ctx) => {
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
    const audit = verifyAuditChain(db);
    const retention = buildRetentionStatusReport(db, asOfDate);
    const backup = getBackupGovernanceStatus(db, companyRoot, asOfDate);
    const gdpr = buildGdprAuditExport(db, { asOf: asOfDate });
    const coverage = computeRegulatoryCoverage();
    const rules = readRuleMetadata();
    const periods = lastClosedPeriodLabel(db);
    const fy = fiscalYearForDate(
      asOfDate,
      company.fiscalYearStartMonth,
      company.fiscalYearLabelStrategy,
    );

    const ruleBundleVersion = (() => {
      try {
        return currentRuleBundleVersion();
      } catch {
        return "unknown";
      }
    })();
    const commitSha = shortCommitSha();
    const generatedAt = ctx.arg("--as-of-instant") ?? new Date().toISOString();

    const html = renderComplianceReport({
      generatedAt,
      companyName: company.name,
      companyCvr: company.cvr ?? null,
      fiscalYearLabel: fy.displayLabel,
      commitSha,
      ruleBundleVersion,
      audit: {
        ok: audit.ok,
        entryCount: audit.entries,
        errors: audit.errors,
      },
      backup,
      retention,
      periods: {
        closedCount: periods.count,
        lastClosedLabel: periods.label,
      },
      gdpr: {
        eventCount: gdpr.events.length,
        fingerprint: gdpr.fingerprint,
      },
      coverage,
      rules,
    });

    const outDir = dirname(outPath);
    if (outDir && outDir !== "." && !existsSync(outDir)) {
      mkdirSync(outDir, { recursive: true });
    }
    writeFileSync(outPath, html, "utf8");

    ctx.emitResult({
      ok: true,
      out: outPath,
      asOfDate,
      generatedAt,
      bytes: html.length,
      fingerprint: `sha256:${complianceReportFingerprint(html)}`,
      ruleBundleVersion,
      commitSha,
    });
    db.close();
  });
}
