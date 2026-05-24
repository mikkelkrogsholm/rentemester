// Cockpit "Undtagelser" workbench (#332) — the read-side payload backing the
// Exceptions queue view.
//
// The agent + human-in-the-loop discipline (jf. `feedback_determinism_human_loop.md`)
// rests on the exception queue: usikre regelfortolkninger afgøres af et menneske
// ved bogføring. The CLI (`exceptions list`) and the agent loop already surface
// open exceptions; this module shapes the same `core/exceptions.ts#listExceptions`
// output for the cockpit, alongside per-status counts and the grouped Danish
// summary lines the Overblik "Opgaver" card already uses (`groupExceptions`).
//
// Every figure is computed by an existing core function — this module only
// opens the right ledger, calls core, and shapes the JSON. It never mutates a
// ledger and never re-implements business logic.
import { existsSync } from "node:fs";
import { openDb, migrate } from "../../core/db";
import {
  listExceptions,
  type ExceptionStatus,
} from "../../core/exceptions";
import { findWorkspaceCompany } from "../../core/workspace";
import { getCompanySettings } from "../../core/company";
import { ApiError } from "../errors";
import {
  buildCompanyFiscalYears,
  requireCompanyDbPath,
  statementCompanyBlock,
  type FiscalYearEntry,
} from "./shared";
import { groupExceptions, type ExceptionGroup } from "./exceptions";

/**
 * One exception row, shaped for the cockpit "Undtagelser" table. Mirrors the
 * core `listExceptions` row plus the wire-friendly `archived` flag.
 *
 * `sourceEvidence` and `postingPreview` are passed through as opaque JSON: each
 * exception type carries its own evidence shape (the bank-line id, the
 * accrual-period key, the tax needs-review kind …) and the cockpit only ever
 * displays them — it never reaches in to mutate them.
 */
export type CompanyExceptionRow = {
  id: number;
  type: string;
  severity: "low" | "medium" | "high";
  status: "open" | "resolved";
  message: string;
  requiredAction: string | null;
  relatedBankTransactionId: number | null;
  relatedDocumentId: number | null;
  /** Opaque per-type evidence JSON from `recordException` — view-only. */
  sourceEvidence: unknown;
  /** Opaque per-type "what would post on resolve" preview — view-only. */
  postingPreview: unknown;
  createdAt: string;
  resolvedAt: string | null;
  resolvedBy: string | null;
  resolutionNote: string | null;
  /** True when the exception belongs to an archived/closed period (read-only). */
  archived: boolean;
};

/**
 * The status filter the cockpit pills carry over the wire. Mirrors core's
 * `ExceptionStatus` so a malformed value never reaches `listExceptions`.
 */
export type CompanyExceptionStatusFilter = ExceptionStatus;

/** Aggregated cockpit payload backing `GET /api/companies/:slug/exceptions`. */
export type CompanyExceptions = {
  slug: string;
  /** Status filter that was effectively applied to the list. */
  status: CompanyExceptionStatusFilter;
  company: ReturnType<typeof statementCompanyBlock>;
  fiscalYears: FiscalYearEntry[];
  rows: CompanyExceptionRow[];
  /** Number of rows in the filtered list. */
  count: number;
  /** Total open exceptions across the company (status-filter-agnostic). */
  openCount: number;
  /** Total resolved exceptions across the company (status-filter-agnostic). */
  resolvedCount: number;
  /** Open exceptions grouped into one Danish summary line per type. */
  openGroups: ExceptionGroup[];
};

const STATUS_VALUES: ReadonlySet<string> = new Set(["open", "resolved", "all"]);

function normalizeStatus(
  raw: string | null | undefined,
): CompanyExceptionStatusFilter {
  if (typeof raw === "string" && STATUS_VALUES.has(raw)) {
    return raw as CompanyExceptionStatusFilter;
  }
  // Default surfaces the action-needed list — open exceptions — exactly what
  // the cockpit owner wants to see when they land on the view.
  return "open";
}

/**
 * Builds the cockpit exception-queue payload. Reuses
 * `core/exceptions.ts#listExceptions` (with archived-period filtering already
 * applied) for the rows, and `groupExceptions` for the grouped summary lines
 * the Overblik "Opgaver" card already shows — so the cockpit's two surfaces
 * cannot diverge on counts or wording.
 *
 * `?status=` filters the list: `open` (default), `resolved`, `all`.
 * `?includeArchived=true` opts back in to archived-period rows for an audit
 * trail; default leaves them hidden, exactly like the CLI.
 */
export function buildCompanyExceptions(
  workspaceRoot: string,
  slug: string,
  rawStatus?: string | null,
  rawIncludeArchived?: string | null,
): CompanyExceptions {
  const entry = findWorkspaceCompany(workspaceRoot, slug);
  if (!entry) {
    throw ApiError.notFound(`no company with slug '${slug}' in the workspace`);
  }
  const dbPath = requireCompanyDbPath(workspaceRoot, slug);
  if (!existsSync(dbPath)) {
    throw ApiError.notFound(`company '${slug}' has no ledger`);
  }
  const status = normalizeStatus(rawStatus);
  const includeArchived = rawIncludeArchived === "true";
  const db = openDb(dbPath);
  try {
    migrate(db);
    const settings = getCompanySettings(db);
    const companyBlock = statementCompanyBlock(settings);
    const fiscalYears = buildCompanyFiscalYears(workspaceRoot, slug).years;

    const list = listExceptions(db, { status, includeArchived });
    if (!list.ok) {
      throw ApiError.badRequest(
        list.errors[0] ?? "kunne ikke bygge undtagelses-listen",
      );
    }

    // Total open / resolved counts across the company are filter-agnostic so
    // the status pills can show a count badge regardless of which pill is
    // active. Archived rows are excluded so the badges align with the default
    // not-archived view; the same `listExceptions` call gives us both numbers.
    const allOpen = listExceptions(db, { status: "open" });
    const allResolved = listExceptions(db, { status: "resolved" });

    const rows: CompanyExceptionRow[] = list.rows.map((r: any) => ({
      id: r.id,
      type: r.type,
      severity: r.severity,
      status: r.status,
      message: r.message,
      requiredAction: r.requiredAction,
      relatedBankTransactionId: r.relatedBankTransactionId,
      relatedDocumentId: r.relatedDocumentId,
      sourceEvidence: r.sourceEvidence,
      postingPreview: r.postingPreview,
      createdAt: r.createdAt,
      resolvedAt: r.resolvedAt,
      resolvedBy: r.resolvedBy,
      resolutionNote: r.resolutionNote,
      archived: r.archived === true,
    }));

    return {
      slug,
      status,
      company: companyBlock,
      fiscalYears,
      rows,
      count: rows.length,
      openCount: allOpen.ok ? allOpen.count : 0,
      resolvedCount: allResolved.ok ? allResolved.count : 0,
      openGroups: allOpen.ok ? groupExceptions(allOpen.rows) : [],
    };
  } finally {
    db.close();
  }
}
