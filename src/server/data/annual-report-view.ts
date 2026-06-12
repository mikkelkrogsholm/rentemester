// Annual report builder view (#338).
//
// Wrapper omkring `buildAnnualReport` fra kernen. Read-only: bygger
// regnskabsklasse-B-arsrapporten for en given fiscal year og rapporterer
// hver forudsætning som klare danske fejl (CVR mangler, periode er ikke
// låst, bøgerne balancerer ikke …).

import { existsSync } from "node:fs";
import { ApiError } from "../errors";
import { buildAnnualReport, type AnnualReport } from "../../core/annual-report";
import { findWorkspaceCompany, companyRootForSlug } from "../../core/workspace";
import { companyPaths } from "../../core/paths";
import { openDb, migrate } from "../../core/db";
import { getCompanySettings } from "../../core/company";

export type CompanyAnnualReportView = {
  slug: string;
  company: {
    name: string;
    cvr: string | null;
    country: string;
    currency: string;
    fiscalYearStartMonth: number | string;
    fiscalYearLabelStrategy: string;
  };
  fiscalYearStart: string;
  fiscalYearEnd: string;
  report: AnnualReport;
};

export function buildCompanyAnnualReport(
  workspaceRoot: string,
  slug: string,
  fiscalYearStart: string,
  fiscalYearEnd: string,
): CompanyAnnualReportView {
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
    const report = buildAnnualReport(db, fiscalYearStart, fiscalYearEnd);
    return {
      slug,
      company: {
        name: company.name,
        cvr: company.cvr,
        country: company.country,
        currency: company.currency,
        fiscalYearStartMonth: company.fiscalYearStartMonth,
        fiscalYearLabelStrategy: company.fiscalYearLabelStrategy,
      },
      fiscalYearStart,
      fiscalYearEnd,
      report,
    };
  } finally {
    db.close();
  }
}
