// Shared read-side internals for the cockpit backend (#170, #320).
//
// The cockpit data layer was one ~3.7k-LOC `server/data.ts` god module; #320
// split it into cohesive modules under `server/data/`. This module holds the
// pieces every other data module reuses: the request-parameter resolvers, the
// kroner rounding helper, the company-context preamble the statement builders
// share, and the per-company fiscal-year listing.
//
// `server/data.ts` is now a thin re-export barrel over these modules, so every
// existing `import ... from "./data"` keeps resolving unchanged.

import { existsSync } from "node:fs";
import type { Database } from "bun:sqlite";
import { companyPaths } from "../../core/paths";
import { openDb, migrate } from "../../core/db";
import { getCompanySettings, type CompanySettings } from "../../core/company";
import { fiscalYearForDate } from "../../core/fiscal-year";
import {
  companyRootForSlug,
  findWorkspaceCompany,
  type WorkspaceCompanyEntry,
} from "../../core/workspace";
import { ApiError } from "../errors";

export const ISO_DATE_RE = /^\d{4}-\d{2}-\d{2}$/;

/** Today as YYYY-MM-DD (UTC). The clock lives here, not in core. */
export function todayIsoDate(): string {
  const now = new Date();
  const y = now.getUTCFullYear();
  const m = String(now.getUTCMonth() + 1).padStart(2, "0");
  const d = String(now.getUTCDate()).padStart(2, "0");
  return `${y}-${m}-${d}`;
}

/** Validates an optional `?as-of=` query value, defaulting to today. */
export function resolveAsOfDate(raw: string | null | undefined): string {
  if (raw === null || raw === undefined || raw.length === 0) return todayIsoDate();
  if (!ISO_DATE_RE.test(raw)) {
    throw ApiError.badRequest("asOf must be a YYYY-MM-DD date");
  }
  return raw;
}

const YEAR_RE = /^\d{4}$/;

/**
 * Validates an optional `?year=` query value. Returns null when absent so the
 * caller can default to the company's most recent live fiscal year.
 */
export function resolveYearParam(raw: string | null | undefined): number | null {
  if (raw === null || raw === undefined || raw.length === 0) return null;
  if (!YEAR_RE.test(raw)) {
    throw ApiError.badRequest("year must be a four-digit calendar year");
  }
  return parseInt(raw, 10);
}

/**
 * Validates a required four-digit `:year` taken from the URL path (e.g. the
 * archive endpoint `/archive/:year`). Unlike `resolveYearParam` the year is
 * mandatory here, so an absent or malformed value is a 400.
 */
export function resolvePathYear(raw: string): number {
  if (!YEAR_RE.test(raw)) {
    throw ApiError.badRequest("year must be a four-digit calendar year");
  }
  return parseInt(raw, 10);
}

/** Rounds a kroner amount to whole øre, killing float drift. */
export function roundKroner(value: number): number {
  return Math.round(Number(value ?? 0) * 100) / 100;
}

/**
 * The current (most recent live) fiscal year for a company — the same default
 * the per-company Overblik view uses. Falls back to today's calendar year when
 * the ledger has no posted entries yet. Returns the calendar year as a number.
 */
export function currentFiscalYear(
  db: Database,
  settings: CompanySettings,
): {
  label: string;
  year: number;
} {
  const dateRows = db
    .query(
      "SELECT MAX(transaction_date) AS d FROM journal_entries WHERE status = 'posted'",
    )
    .get() as { d: string | null };
  const latest = dateRows?.d;
  if (latest && ISO_DATE_RE.test(latest)) {
    const fy = fiscalYearForDate(
      latest,
      settings.fiscalYearStartMonth,
      settings.fiscalYearLabelStrategy,
    );
    const y = parseInt(latest.slice(0, 4), 10);
    return { label: fy.identifierLabel, year: y };
  }
  const y = new Date().getUTCFullYear();
  return { label: String(y), year: y };
}

// --------------------------------------------------------------------------
// Per-company fiscal years
// --------------------------------------------------------------------------

/** One fiscal year available for a company. */
export type FiscalYearEntry = {
  /** Stable, sortable label for the year, e.g. "2026" or "2025-26". */
  label: string;
  /** Fiscal-year start date (YYYY-MM-DD); null for an archived year. */
  start: string | null;
  /** Fiscal-year end date (YYYY-MM-DD); null for an archived year. */
  end: string | null;
  /** Where the year's data lives: the live hash-chained ledger or the archive. */
  source: "live" | "archive";
};

export type CompanyFiscalYears = {
  slug: string;
  /** Fiscal years, descending by label — newest first. */
  years: FiscalYearEntry[];
};

/**
 * The fiscal years available for a company: the live ledger's year(s) — every
 * distinct fiscal year touched by a posted `journal_entries` row — plus any
 * read-only archived years from the `import_archive_years` table (#197).
 *
 * Years are deduplicated by label (a live year wins over an archived one of
 * the same label) and returned newest-first. Throws `ApiError.notFound` when
 * the slug is not registered or the ledger is missing on disk.
 */
export function buildCompanyFiscalYears(
  workspaceRoot: string,
  slug: string,
): CompanyFiscalYears {
  const entry = findWorkspaceCompany(workspaceRoot, slug);
  if (!entry) {
    throw ApiError.notFound(`ingen virksomhed med slug '${slug}' findes i workspacet`);
  }
  const companyRoot = companyRootForSlug(workspaceRoot, slug);
  const dbPath = companyPaths(companyRoot).db;
  if (!existsSync(dbPath)) {
    throw ApiError.notFound(`virksomheden '${slug}' har ingen ledger`);
  }

  const db = openDb(dbPath);
  try {
    migrate(db);
    const company = getCompanySettings(db);
    const byLabel = new Map<string, FiscalYearEntry>();

    // Live ledger: one fiscal year per distinct transaction_date, collapsed.
    const dateRows = db
      .query(
        "SELECT DISTINCT transaction_date AS d FROM journal_entries WHERE status = 'posted'",
      )
      .all() as Array<{ d: string }>;
    for (const row of dateRows) {
      if (!ISO_DATE_RE.test(row.d)) continue;
      const fy = fiscalYearForDate(
        row.d,
        company.fiscalYearStartMonth,
        company.fiscalYearLabelStrategy,
      );
      byLabel.set(fy.identifierLabel, {
        label: fy.identifierLabel,
        start: fy.start,
        end: fy.end,
        source: "live",
      });
    }

    // Archived years (#197) — read-only reference data, outside the ledger.
    const archiveRows = db
      .query(
        "SELECT DISTINCT fiscal_year AS y FROM import_archive_years ORDER BY fiscal_year",
      )
      .all() as Array<{ y: number }>;
    for (const row of archiveRows) {
      const label = String(row.y);
      // A live year of the same label is authoritative — never shadow it.
      if (byLabel.has(label)) continue;
      byLabel.set(label, { label, start: null, end: null, source: "archive" });
    }

    const years = [...byLabel.values()].sort((a, b) =>
      b.label.localeCompare(a.label),
    );
    return { slug: entry.slug, years };
  } finally {
    db.close();
  }
}

// --------------------------------------------------------------------------
// Statement context — the shared preamble for the per-company statement views
// --------------------------------------------------------------------------

/**
 * Resolves the company, opens its ledger and picks the selected fiscal year —
 * the shared preamble for the statement builders. The selected year follows
 * `buildCompanyOverview`: an explicit `?year=` wins, else the most recent live
 * year, else the newest available year.
 *
 * Throws `ApiError.notFound` when the slug is not registered or has no ledger.
 */
export function resolveStatementContext(
  workspaceRoot: string,
  slug: string,
  year: number | null,
): {
  entry: WorkspaceCompanyEntry;
  db: Database;
  company: ReturnType<typeof getCompanySettings>;
  years: FiscalYearEntry[];
  selectedLabel: string;
  isArchivedOnly: boolean;
} {
  const entry = findWorkspaceCompany(workspaceRoot, slug);
  if (!entry) {
    throw ApiError.notFound(`ingen virksomhed med slug '${slug}' findes i workspacet`);
  }
  const companyRoot = companyRootForSlug(workspaceRoot, slug);
  const dbPath = companyPaths(companyRoot).db;
  if (!existsSync(dbPath)) {
    throw ApiError.notFound(`virksomheden '${slug}' har ingen ledger`);
  }

  const years = buildCompanyFiscalYears(workspaceRoot, slug).years;
  const liveYears = years.filter((y) => y.source === "live");
  const defaultYear =
    liveYears[0]?.label ?? years[0]?.label ?? String(new Date().getUTCFullYear());
  const selectedLabel = year !== null ? String(year) : defaultYear;
  const selected = years.find((y) => y.label === selectedLabel);
  const isArchivedOnly = selected ? selected.source === "archive" : false;

  const db = openDb(dbPath);
  migrate(db);
  return {
    entry,
    db,
    company: getCompanySettings(db),
    years,
    selectedLabel,
    isArchivedOnly,
  };
}

export type StatementCompanyBlock = {
  name: string;
  cvr: string | null;
  country: string;
  currency: string;
  fiscalYearStartMonth: number | string;
  fiscalYearLabelStrategy: string;
};

export function statementCompanyBlock(
  company: ReturnType<typeof getCompanySettings>,
): StatementCompanyBlock {
  return {
    name: company.name,
    cvr: company.cvr,
    country: company.country,
    currency: company.currency,
    fiscalYearStartMonth: company.fiscalYearStartMonth,
    fiscalYearLabelStrategy: company.fiscalYearLabelStrategy,
  };
}

/** Resolve a slug to its ledger db path, asserting the company + ledger exist. */
export function requireCompanyDbPath(workspaceRoot: string, slug: string): string {
  if (!findWorkspaceCompany(workspaceRoot, slug)) {
    throw ApiError.notFound(`ingen virksomhed med slug '${slug}' findes i workspacet`);
  }
  const dbPath = companyPaths(companyRootForSlug(workspaceRoot, slug)).db;
  if (!existsSync(dbPath)) {
    throw ApiError.notFound(`virksomheden '${slug}' har ingen ledger`);
  }
  return dbPath;
}

/** Danish month abbreviations, jan–dec, used by the monthly breakdown views. */
export const MONTH_NAMES_DK = [
  "jan", "feb", "mar", "apr", "maj", "jun",
  "jul", "aug", "sep", "okt", "nov", "dec",
];
