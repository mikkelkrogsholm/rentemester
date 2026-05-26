// Exceptions queue list (#332).
//
// Wrapper omkring `listExceptions` fra kernen så cockpittet kan vise SMB-ejeren
// alle åbne undtagelser (unmatched bank-rows, blokerede write-flows osv.) med
// company-block + tæller pr. severity. Read-only — POST .../exceptions/:id/
// resolve er allerede implementeret separat.

import { existsSync } from "node:fs";
import { ApiError } from "../errors";
import { listExceptions } from "../../core/exceptions";
import { findWorkspaceCompany, companyRootForSlug } from "../../core/workspace";
import { companyPaths } from "../../core/paths";
import { openDb, migrate } from "../../core/db";
import { getCompanySettings } from "../../core/company";

export type ExceptionRow = {
  id: number;
  type: string;
  severity: "low" | "medium" | "high";
  status: "open" | "resolved";
  relatedBankTransactionId: number | null;
  relatedDocumentId: number | null;
  message: string;
  requiredAction: string | null;
  sourceEvidence: unknown;
  postingPreview: unknown;
  createdAt: string;
  resolvedAt: string | null;
  resolvedBy: string | null;
  resolutionNote: string | null;
  archived: boolean;
};

export type CompanyExceptionsView = {
  slug: string;
  company: {
    name: string;
    cvr: string | null;
    country: string;
    currency: string;
  };
  /** Selected filter — defaults to "open" so a fresh visit shows the queue. */
  status: "open" | "resolved" | "all";
  rows: ExceptionRow[];
  bySeverity: { high: number; medium: number; low: number };
  count: number;
};

export function buildCompanyExceptions(
  workspaceRoot: string,
  slug: string,
  status: "open" | "resolved" | "all" = "open",
): CompanyExceptionsView {
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
    const result = listExceptions(db, { status });
    if (!result.ok) {
      // Validation failure in core — kun "status" is unknown her.
      throw ApiError.badRequest(result.errors.join("; "));
    }
    const rows = result.rows as ExceptionRow[];
    const bySeverity = { high: 0, medium: 0, low: 0 };
    for (const r of rows) {
      if (r.severity === "high") bySeverity.high += 1;
      else if (r.severity === "medium") bySeverity.medium += 1;
      else if (r.severity === "low") bySeverity.low += 1;
    }
    return {
      slug,
      company: {
        name: company.name,
        cvr: company.cvr,
        country: company.country,
        currency: company.currency,
      },
      status,
      rows,
      bySeverity,
      count: rows.length,
    };
  } finally {
    db.close();
  }
}
