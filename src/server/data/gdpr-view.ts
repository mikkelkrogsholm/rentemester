// GDPR view (#334).
//
// Wrapper omkring buildGdprSubjectExport fra kernen så cockpittet kan
// hjælpe ejeren med at besvare en indsigtsanmodning. Read-only — erase
// går via withCompanyMutation som en separat write-handler.

import { existsSync } from "node:fs";
import { ApiError } from "../errors";
import {
  buildGdprSubjectExport,
  type GdprSubjectExport,
} from "../../core/gdpr";
import { findWorkspaceCompany, companyRootForSlug } from "../../core/workspace";
import { companyPaths } from "../../core/paths";
import { openDb, migrate } from "../../core/db";
import { getCompanySettings } from "../../core/company";

export type CompanyGdprExportView = {
  slug: string;
  company: {
    name: string;
    cvr: string | null;
    country: string;
    currency: string;
  };
  export: GdprSubjectExport;
};

export function buildCompanyGdprExport(
  workspaceRoot: string,
  slug: string,
  key: { cvr?: string; name?: string; asOf?: string },
): CompanyGdprExportView {
  const entry = findWorkspaceCompany(workspaceRoot, slug);
  if (!entry) {
    throw ApiError.notFound(`no company with slug '${slug}' in the workspace`);
  }
  if (!key.cvr && !key.name) {
    throw ApiError.badRequest(
      "Brug cvr eller name som søgekriterie — mindst ét er påkrævet.",
    );
  }
  const companyRoot = companyRootForSlug(workspaceRoot, slug);
  const dbPath = companyPaths(companyRoot).db;
  if (!existsSync(dbPath)) {
    throw ApiError.notFound(`company '${slug}' has no ledger`);
  }
  const db = openDb(dbPath);
  try {
    migrate(db);
    const company = getCompanySettings(db);
    const exportResult = buildGdprSubjectExport(db, {
      cvr: key.cvr ?? null,
      name: key.name ?? null,
      asOf: key.asOf ?? null,
    });
    return {
      slug,
      company: {
        name: company.name,
        cvr: company.cvr,
        country: company.country,
        currency: company.currency,
      },
      export: exportResult,
    };
  } finally {
    db.close();
  }
}
