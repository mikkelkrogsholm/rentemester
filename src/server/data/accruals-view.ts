// Periodisering / accrual view (#337).
//
// Wrapper omkring buildAccrualRegisterReport. Read-only — register +
// recognize-period write-flows er separate handlers (følger ad
// behov-baseret iteration).

import { existsSync } from "node:fs";
import { ApiError } from "../errors";
import {
  buildAccrualRegisterReport,
  type AccrualRegisterReport,
} from "../../core/accruals";
import { findWorkspaceCompany, companyRootForSlug } from "../../core/workspace";
import { companyPaths } from "../../core/paths";
import { openDb, migrate } from "../../core/db";
import { getCompanySettings } from "../../core/company";

export type CompanyAccrualsView = {
  slug: string;
  company: {
    name: string;
    cvr: string | null;
    country: string;
    currency: string;
  };
  report: AccrualRegisterReport;
};

export function buildCompanyAccruals(
  workspaceRoot: string,
  slug: string,
): CompanyAccrualsView {
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
    return {
      slug,
      company: {
        name: company.name,
        cvr: company.cvr,
        country: company.country,
        currency: company.currency,
      },
      report: buildAccrualRegisterReport(db),
    };
  } finally {
    db.close();
  }
}
