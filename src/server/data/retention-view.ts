// Retention status view (#343).
//
// Genbruger den eksisterende `buildRetentionStatusReport(db)` fra kernen og
// wrapper den i en cockpit-venlig payload med company-block + dansk
// rule-citation. Hver række er en data-domæne (bilag, posteringer,
// banktransaktioner) med ældste tilbageværende post, antal udløbne og næste
// udløbsdato — så ejeren kan se hvad der nærmer sig den 5-årige
// bogføringspligt og hvad der er beskyttet.

import { existsSync } from "node:fs";
import { ApiError } from "../errors";
import {
  buildRetentionStatusReport,
  type RetentionStatusReport,
} from "../../core/retention";
import { findWorkspaceCompany, companyRootForSlug } from "../../core/workspace";
import { companyPaths } from "../../core/paths";
import { openDb, migrate } from "../../core/db";
import { getCompanySettings } from "../../core/company";

export type RetentionViewCompany = {
  name: string;
  cvr: string | null;
  country: string;
  currency: string;
};

export type CompanyRetentionView = {
  slug: string;
  company: RetentionViewCompany;
  report: RetentionStatusReport;
  /**
   * Citation til bogføringsloven (Lov om bogføring § 12, stk. 1) — den
   * 5-årige opbevaringspligt. Indekset her er det `sourceId` cockpittet kan
   * deep-linke til /lovgrundlag-viewet (#347).
   */
  legalCitation: {
    sourceId: string;
    note: string;
  };
};

export function buildCompanyRetention(
  workspaceRoot: string,
  slug: string,
): CompanyRetentionView {
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
    const report = buildRetentionStatusReport(db);
    return {
      slug,
      company: {
        name: company.name,
        cvr: company.cvr,
        country: company.country,
        currency: company.currency,
      },
      report,
      legalCitation: {
        sourceId: "DK-BOGFORINGSLOVEN-2022-700",
        note:
          "Bogføringsloven § 12, stk. 1 — bilag og bogføringsmateriale skal " +
          "opbevares i 5 år efter udløbet af det regnskabsår, materialet vedrører.",
      },
    };
  } finally {
    db.close();
  }
}
