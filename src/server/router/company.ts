// Misc company-scoped read handlers — settings, sync-cvr, obligations,
// cashflow, mileage, budget(s), archive-year, accruals, bilagsmail,
// annual-report, accounts, retention, integrity, exceptions, periods,
// gdpr export, payables, agent-suggestions.

import type { ServerConfig } from "../config";
import { ApiError } from "../errors";
import { buildCompanyAccounts } from "../data/accounts-view";
import { buildCompanyAccruals } from "../data/accruals-view";
import { buildCompanyAnnualReport } from "../data/annual-report-view";
import { buildCompanyBilagsmail } from "../data/bilagsmail-view";
import { buildCompanyExceptions } from "../data/exceptions-list";
import { buildCompanyGdprExport } from "../data/gdpr-view";
import { buildCompanyIntegrity } from "../data/integrity-view";
import { buildCompanyPeriods } from "../data/periods-view";
import { buildCompanyRetention } from "../data/retention-view";
import {
  buildCompanyAgentSuggestions,
  buildCompanyArchiveYear,
  buildCompanyBudget,
  buildCompanyBudgetVsActual,
  buildCompanyCashflow,
  buildCompanyMileage,
  buildCompanyObligations,
  buildCompanyPayables,
  buildCompanySettings,
  resolvePathYear,
  resolveYearParam,
  syncCompanyCvr,
} from "../data";
import { okResponse } from "./_shared";

export function handleCompanySettings(config: ServerConfig, slug: string): Response {
  const data = buildCompanySettings(config.workspaceRoot, slug);
  return okResponse({ company: data });
}

/**
 * Refreshes a company's CVR-register stamdata. The CVR lookup runs server-side
 * so the CVR credentials never reach the browser. A failed lookup (missing
 * credentials, unknown CVR) is reported inside `sync.ok`, not as an HTTP error.
 */
export async function handleCompanySyncCvr(
  config: ServerConfig,
  slug: string,
): Promise<Response> {
  const data = await syncCompanyCvr(config.workspaceRoot, slug);
  return okResponse({ sync: data });
}

export function handleCompanyObligations(
  config: ServerConfig,
  slug: string,
  url: URL,
): Response {
  const year = resolveYearParam(url.searchParams.get("year"));
  const data = buildCompanyObligations(config.workspaceRoot, slug, year);
  return okResponse({ obligations: data });
}

export function handleCompanyCashflow(
  config: ServerConfig,
  slug: string,
  url: URL,
): Response {
  const year = resolveYearParam(url.searchParams.get("year"));
  const data = buildCompanyCashflow(config.workspaceRoot, slug, year);
  return okResponse({ cashflow: data });
}

/**
 * GET /api/companies/:slug/mileage[?year=] — the cockpit Kørsel view (#335).
 * Re-uses the SAME mileage core (`buildMileagePeriodReport`) the CLI's
 * `mileage report` command runs; the server just opens the ledger and shapes
 * the JSON.
 */
export function handleCompanyMileage(
  config: ServerConfig,
  slug: string,
  url: URL,
): Response {
  const year = resolveYearParam(url.searchParams.get("year"));
  const data = buildCompanyMileage(config.workspaceRoot, slug, year);
  return okResponse({ mileage: data });
}

/**
 * GET /api/companies/:slug/budget?year= — the effective (latest-revision)
 * budget lines for one fiscal year (#339). Drives the Budget input grid in
 * the cockpit; latest-revision-wins is owned by `core/budget.ts#listBudget`.
 */
export function handleCompanyBudget(
  config: ServerConfig,
  slug: string,
  url: URL,
): Response {
  const year = resolveYearParam(url.searchParams.get("year"));
  const data = buildCompanyBudget(config.workspaceRoot, slug, year);
  return okResponse({ budget: data });
}

/**
 * GET /api/companies/:slug/budget-vs-actual?year= — the budget-vs-faktisk
 * comparison for one fiscal year (#339). Third caller of the same
 * `buildBudgetVsActual` core the CLI report uses, so every surface gets the
 * same numbers and the same sign convention.
 */
export function handleCompanyBudgetVsActual(
  config: ServerConfig,
  slug: string,
  url: URL,
): Response {
  const year = resolveYearParam(url.searchParams.get("year"));
  const data = buildCompanyBudgetVsActual(config.workspaceRoot, slug, year);
  return okResponse({ budgetVsActual: data });
}

export function handleCompanyArchiveYear(
  config: ServerConfig,
  slug: string,
  yearRaw: string,
): Response {
  const year = resolvePathYear(yearRaw);
  const data = buildCompanyArchiveYear(config.workspaceRoot, slug, year);
  return okResponse({ archive: data });
}

/**
 * GET /api/companies/:slug/accruals — Periodiseringsregister (#337).
 *
 * Wrapper omkring buildAccrualRegisterReport. Read-only.
 */
export function handleCompanyAccruals(
  config: ServerConfig,
  slug: string,
): Response {
  const data = buildCompanyAccruals(config.workspaceRoot, slug);
  return okResponse({ accruals: data });
}

/**
 * GET /api/companies/:slug/annual-report?fiscalYearStart=&fiscalYearEnd= —
 * Årsrapport-builder (#338). Read-only.
 */
/**
 * GET /api/companies/:slug/bilagsmail — Bilagsmail-status (#348/#350/#351).
 *
 * Read-only: returnerer IMAP-config-status (uden password), mail-alias, og
 * inbox-listen over mail-drop-dokumenter. Write-handlers ligger på de
 * separate /imap-config + /alias paths.
 */
export function handleCompanyBilagsmail(
  config: ServerConfig,
  slug: string,
): Response {
  const data = buildCompanyBilagsmail(config.workspaceRoot, slug);
  return okResponse({ bilagsmail: data });
}

export function handleCompanyAnnualReport(
  config: ServerConfig,
  slug: string,
  url: URL,
): Response {
  const fiscalYearStart = url.searchParams.get("fiscalYearStart");
  const fiscalYearEnd = url.searchParams.get("fiscalYearEnd");
  if (!fiscalYearStart || !fiscalYearEnd) {
    throw ApiError.badRequest(
      "fiscalYearStart og fiscalYearEnd (YYYY-MM-DD) er begge påkrævet.",
    );
  }
  const data = buildCompanyAnnualReport(
    config.workspaceRoot,
    slug,
    fiscalYearStart,
    fiscalYearEnd,
  );
  return okResponse({ annualReport: data });
}

/**
 * GET /api/companies/:slug/accounts — Kontoplan-view (#344).
 *
 * Read-only liste over kontoplanen. Cockpittet kan vise hvilke konti der
 * er seedet og hvilke der har bogføringslinjer.
 */
export function handleCompanyAccounts(
  config: ServerConfig,
  slug: string,
): Response {
  const data = buildCompanyAccounts(config.workspaceRoot, slug);
  return okResponse({ accounts: data });
}

/**
 * GET /api/companies/:slug/retention — Retention status view (#343).
 *
 * Genbruger `buildRetentionStatusReport` fra kernen og wrapper den i en
 * cockpit-venlig payload med company-block + dansk rule-citation. Read-only.
 */
export function handleCompanyRetention(
  config: ServerConfig,
  slug: string,
): Response {
  const data = buildCompanyRetention(config.workspaceRoot, slug);
  return okResponse({ retention: data });
}

/**
 * GET /api/companies/:slug/integrity — Audit chain + backup status panel (#333).
 *
 * Idempotent — `verifyAuditChain` er read-only og kan kaldes så ofte cockpittet
 * ønsker uden side-effekter. Genbruger `getBackupComplianceStatus` +
 * `listBackupDestinations` så cockpittet ikke duplikerer kerne-logik.
 */
export function handleCompanyIntegrity(
  config: ServerConfig,
  slug: string,
): Response {
  const data = buildCompanyIntegrity(config.workspaceRoot, slug);
  return okResponse({ integrity: data });
}

/**
 * GET /api/companies/:slug/exceptions[?status=open|resolved|all] — Exceptions
 * queue (#332). Read-only liste; POST .../exceptions/:id/resolve er en
 * separat write-handler.
 */
export function handleCompanyExceptions(
  config: ServerConfig,
  slug: string,
  url: URL,
): Response {
  const raw = (url.searchParams.get("status") ?? "open").toLowerCase();
  if (raw !== "open" && raw !== "resolved" && raw !== "all") {
    throw ApiError.badRequest(
      `status='${raw}' understøttes ikke — brug open, resolved eller all.`,
    );
  }
  const data = buildCompanyExceptions(
    config.workspaceRoot,
    slug,
    raw as "open" | "resolved" | "all",
  );
  return okResponse({ exceptions: data });
}

/**
 * GET /api/companies/:slug/periods — Periodelås-liste (#342).
 *
 * Read-only. Wrapper omkring accounting_periods + effectivePeriodState.
 */
export function handleCompanyPeriods(
  config: ServerConfig,
  slug: string,
): Response {
  const data = buildCompanyPeriods(config.workspaceRoot, slug);
  return okResponse({ periods: data });
}

/**
 * GET /api/companies/:slug/gdpr/export?cvr=&name=&asOf= — GDPR-indsigt (#334).
 *
 * Wrapper omkring buildGdprSubjectExport. Read-only: en indsigtsrapport
 * over de personoplysninger en data-subject findes med pr. en given dato.
 */
export function handleCompanyGdprExport(
  config: ServerConfig,
  slug: string,
  url: URL,
): Response {
  const cvr = url.searchParams.get("cvr") ?? undefined;
  const name = url.searchParams.get("name") ?? undefined;
  const asOf = url.searchParams.get("asOf") ?? undefined;
  const data = buildCompanyGdprExport(config.workspaceRoot, slug, {
    cvr: cvr || undefined,
    name: name || undefined,
    asOf: asOf || undefined,
  });
  return okResponse({ gdpr: data });
}

/**
 * GET /api/companies/:slug/payables — the read-side payload backing the
 * Leverandørfaktura-arbejdsbordet (#340): the kreditorliste from
 * `core/payables.ts#buildPayablesList`, plus the modal picker rows (vendors,
 * expense accounts, unregistered purchase documents) the "Registrér
 * leverandørfaktura"-modal needs to register an existing bilag.
 *
 * `?status=` filters the list: `open` (default), `paid`, `overdue`, `all`.
 * `?asOf=YYYY-MM-DD` overrides the comparison date for the aging buckets.
 */
export function handleCompanyPayables(
  config: ServerConfig,
  slug: string,
  url: URL,
): Response {
  // Validate the filter params at the edge — like the sibling exceptions list
  // (handleCompanyExceptions) — instead of silently coercing an unknown status
  // to "open" or a malformed asOf to today. A discarded filter that quietly
  // returns the default list is a misleading state for the owner. The status is
  // lower-cased first, matching handleCompanyExceptions (so `?status=Open`
  // behaves the same on both list endpoints).
  const rawStatus = url.searchParams.get("status");
  const status = rawStatus === null ? null : rawStatus.toLowerCase();
  if (
    status !== null &&
    status !== "open" &&
    status !== "paid" &&
    status !== "overdue" &&
    status !== "all"
  ) {
    throw ApiError.badRequest(
      `status='${rawStatus}' understøttes ikke — brug open, paid, overdue eller all.`,
    );
  }
  const asOf = url.searchParams.get("asOf");
  if (asOf !== null && !/^\d{4}-\d{2}-\d{2}$/.test(asOf)) {
    throw ApiError.badRequest("asOf skal være en YYYY-MM-DD dato.");
  }
  const data = buildCompanyPayables(config.workspaceRoot, slug, status, asOf);
  return okResponse({ payables: data });
}

/**
 * GET /api/companies/:slug/agent-suggestions — the agent-forslag-kø (#346):
 * every open `AGENT_*` exception, enriched with the agent's rule id, rationale
 * and audit attribution. The cockpit's Agent-forslag view drives off this
 * payload; approve/reject lives in the matching write routes.
 */
export function handleCompanyAgentSuggestions(
  config: ServerConfig,
  slug: string,
): Response {
  const data = buildCompanyAgentSuggestions(config.workspaceRoot, slug);
  return okResponse({ agentSuggestions: data });
}
