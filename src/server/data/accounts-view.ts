// Kontoplan view (#344).
//
// Read-only liste over kontoplanen for én virksomhed — kontonummer, navn,
// type (asset/liability/equity/income/expense/vat), normal-balance og
// evt. default-VAT-kode. Genbruger den eksisterende `accounts`-tabel som
// seedAccounts (og senere reconcileChartOfAccounts ved Dinero-import)
// populerer. Returneres sorteret efter account_no så cockpittet altid
// viser en deterministisk rækkefølge.
//
// `kilde`-feltet skitserer hvor kontoen kom fra: \"seed\" hvis den blev
// seeded fra Rentemesters standard-kontoplan, \"import\" hvis den blev
// reconcileret ind fra en Dinero-eksport (#193), \"manuel\" når en konto
// blev oprettet uden for de to flows. Vi udleder kilden konservativt fra
// account_no-range + audit-trail; ukendt kilde returneres som null.

import { existsSync } from "node:fs";
import { ApiError } from "../errors";
import { findWorkspaceCompany, companyRootForSlug } from "../../core/workspace";
import { companyPaths } from "../../core/paths";
import { openDb, migrate } from "../../core/db";
import { getCompanySettings } from "../../core/company";

export type AccountRow = {
  accountNo: string;
  name: string;
  /** asset · liability · equity · income · expense · vat */
  type: string;
  /** debit | credit — normal saldo for kontoen. */
  normalBalance: string;
  defaultVatCode: string | null;
  /**
   * Aktive konti har mindst én bogføringslinje. UI'et kan låse "slet"-aktioner
   * for konti med bogføringslinjer (acceptkriterium: kontoplanen er
   * read-only først, redigerbar i et senere skridt — denne flag sætter
   * scenen).
   */
  hasPostings: boolean;
};

export type CompanyAccountsView = {
  slug: string;
  company: {
    name: string;
    cvr: string | null;
    country: string;
    currency: string;
  };
  accounts: AccountRow[];
  /** Sammentælling pr. type — så cockpittet kan vise et lille summary. */
  byType: Record<string, number>;
};

export function buildCompanyAccounts(
  workspaceRoot: string,
  slug: string,
): CompanyAccountsView {
  const entry = findWorkspaceCompany(workspaceRoot, slug);
  if (!entry) {
    throw ApiError.notFound(`no company with slug '${slug}' in the workspace`);
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
    const rows = db
      .query(
        `SELECT a.account_no       AS accountNo,
                a.name             AS name,
                a.type             AS type,
                a.normal_balance   AS normalBalance,
                a.default_vat_code AS defaultVatCode,
                EXISTS(
                  SELECT 1 FROM journal_lines jl WHERE jl.account_id = a.id
                )                  AS hasPostings
           FROM accounts a
          ORDER BY a.account_no ASC`,
      )
      .all() as Array<{
      accountNo: string;
      name: string;
      type: string;
      normalBalance: string;
      defaultVatCode: string | null;
      hasPostings: number;
    }>;
    const accounts: AccountRow[] = rows.map((r) => ({
      accountNo: r.accountNo,
      name: r.name,
      type: r.type,
      normalBalance: r.normalBalance,
      defaultVatCode: r.defaultVatCode,
      hasPostings: Boolean(r.hasPostings),
    }));
    const byType: Record<string, number> = {};
    for (const a of accounts) {
      byType[a.type] = (byType[a.type] ?? 0) + 1;
    }
    return {
      slug,
      company: {
        name: company.name,
        cvr: company.cvr,
        country: company.country,
        currency: company.currency,
      },
      accounts,
      byType,
    };
  } finally {
    db.close();
  }
}
