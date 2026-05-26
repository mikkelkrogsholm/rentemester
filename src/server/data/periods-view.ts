// Periodelås view (#342).
//
// Wrapper omkring accounting_periods-tabellen + `effectivePeriodState` så
// cockpittet kan vise alle perioder (åbne, lukkede, indberettede) med deres
// effective state. Lukket/indberettet kommer fra row-status; reopen via
// audit-log fanges af effectivePeriodState. Read-only — close/reopen er
// separate write-handlers.

import { existsSync } from "node:fs";
import { ApiError } from "../errors";
import {
  effectivePeriodState,
  type AccountingPeriodKind,
  type AccountingPeriodStatus,
  type EffectivePeriodState,
} from "../../core/periods";
import { findWorkspaceCompany, companyRootForSlug } from "../../core/workspace";
import { companyPaths } from "../../core/paths";
import { openDb, migrate } from "../../core/db";
import { getCompanySettings } from "../../core/company";

export type AccountingPeriodRow = {
  id: number;
  periodStart: string;
  periodEnd: string;
  kind: AccountingPeriodKind;
  rowStatus: AccountingPeriodStatus;
  effectiveStatus: EffectivePeriodState;
  closedAt: string | null;
  closedBy: string | null;
  reference: string | null;
};

export type CompanyPeriodsView = {
  slug: string;
  company: {
    name: string;
    cvr: string | null;
    country: string;
    currency: string;
  };
  periods: AccountingPeriodRow[];
  /** Sammentælling pr. effective state. */
  byStatus: { open: number; closed: number; reported: number };
};

export function buildCompanyPeriods(
  workspaceRoot: string,
  slug: string,
): CompanyPeriodsView {
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
    const rows = db
      .query(
        `SELECT id           AS id,
                period_start AS periodStart,
                period_end   AS periodEnd,
                kind         AS kind,
                status       AS rowStatus,
                closed_at    AS closedAt,
                closed_by    AS closedBy,
                reference    AS reference
           FROM accounting_periods
          ORDER BY period_end DESC, id DESC`,
      )
      .all() as Array<{
      id: number;
      periodStart: string;
      periodEnd: string;
      kind: AccountingPeriodKind;
      rowStatus: AccountingPeriodStatus;
      closedAt: string | null;
      closedBy: string | null;
      reference: string | null;
    }>;
    const periods: AccountingPeriodRow[] = rows.map((r) => ({
      id: r.id,
      periodStart: r.periodStart,
      periodEnd: r.periodEnd,
      kind: r.kind,
      rowStatus: r.rowStatus,
      effectiveStatus: effectivePeriodState(db, r.id, r.rowStatus),
      closedAt: r.closedAt,
      closedBy: r.closedBy,
      reference: r.reference,
    }));
    const byStatus = { open: 0, closed: 0, reported: 0 };
    for (const p of periods) {
      byStatus[p.effectiveStatus] += 1;
    }
    return {
      slug,
      company: {
        name: company.name,
        cvr: company.cvr,
        country: company.country,
        currency: company.currency,
      },
      periods,
      byStatus,
    };
  } finally {
    db.close();
  }
}
