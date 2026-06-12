// Bankkonto-view (#345).
//
// Wrapper omkring listBankAccounts + listBankProfileNames så cockpittet kan
// vise SMB-ejeren alle registrerede bankkonti + de indbyggede CSV-mapping-
// profiler. Profil-mappingen er hard-coded i src/core/bank-profiles.ts (én
// pr. dansk bank). Pr.-konto-mapping (versioneret pr. konto) er en
// follow-up — read-side er nu fuld.

import { existsSync } from "node:fs";
import { ApiError } from "../errors";
import { listBankAccounts, type BankAccount } from "../../core/bank";
import {
  getBankProfile,
  listBankProfileNames,
  type BankImportProfile,
} from "../../core/bank-profiles";
import { findWorkspaceCompany, companyRootForSlug } from "../../core/workspace";
import { companyPaths } from "../../core/paths";
import { openDb, migrate } from "../../core/db";
import { getCompanySettings } from "../../core/company";

export type CompanyBankAccountsView = {
  slug: string;
  company: {
    name: string;
    cvr: string | null;
    country: string;
    currency: string;
  };
  accounts: BankAccount[];
  /** De indbyggede CSV-mapping-profiler (Lunar, Danske Bank, Sydbank, …). */
  profiles: BankImportProfile[];
};

export function buildCompanyBankAccounts(
  workspaceRoot: string,
  slug: string,
): CompanyBankAccountsView {
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
    const list = listBankAccounts(db, true);
    const profiles = listBankProfileNames()
      .map((name) => getBankProfile(name))
      .filter((p): p is BankImportProfile => p !== null);
    return {
      slug,
      company: {
        name: company.name,
        cvr: company.cvr,
        country: company.country,
        currency: company.currency,
      },
      accounts: list.accounts,
      profiles,
    };
  } finally {
    db.close();
  }
}
