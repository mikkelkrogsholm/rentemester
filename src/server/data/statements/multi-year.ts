// Per-company Flerårsoversigt — split out of statements.ts.
//
// Key figures for every fiscal year (live + archive) so the SPA can chart a
// trend left-to-right. Live years are computed via
// `core/financial-statements`; archived years (#197) are classified through
// `classifyAccountSection` (#321) — the same rule the Balance view applies,
// so the two views never disagree. Money is kroner.

import { existsSync } from "node:fs";
import { companyPaths } from "../../../core/paths";
import { openDb, migrate } from "../../../core/db";
import { getCompanySettings } from "../../../core/company";
import {
  buildBalanceSheet,
  buildProfitAndLoss,
} from "../../../core/financial-statements";
import { classifyAccountSection } from "../../../core/account-classification";
import {
  companyRootForSlug,
  findWorkspaceCompany,
} from "../../../core/workspace";
import { ApiError } from "../../errors";
import {
  buildCompanyFiscalYears,
  roundKroner,
  statementCompanyBlock,
} from "../shared";

// --------------------------------------------------------------------------
// Per-company multi-year key figures (Flerårsoversigt) — cockpit-redesign it. 4
// --------------------------------------------------------------------------

/** Key figures for one fiscal year in the multi-year comparison. */
export type MultiYearRow = {
  /** The fiscal-year label, e.g. "2025". */
  year: string;
  /** Where the figures come from: the live ledger or the #197 archive. */
  source: "live" | "archive";
  /** Income / omsætning for the year, kroner. */
  omsaetning: number;
  /** Expenses / udgifter for the year, kroner. */
  udgifter: number;
  /** Result (omsætning − udgifter), kroner. */
  resultat: number;
  /** Total assets (balancesum) at the year end, kroner. */
  balancesum: number;
  /** Equity (egenkapital incl. period result) at the year end, kroner. */
  egenkapital: number;
  /**
   * Bruttomargin — resultat ÷ omsætning, a 0–1 fraction. Null when there is no
   * omsætning to divide by; no figure is invented.
   */
  bruttomargin: number | null;
  /**
   * Egenkapitalandel — egenkapital ÷ balancesum, a 0–1 fraction. Null when the
   * balance sum is zero.
   */
  egenkapitalandel: number | null;
};

export type CompanyMultiYear = ReturnType<typeof buildCompanyMultiYear>;

/**
 * Flerårsoversigt — key figures for every fiscal year available for a company,
 * oldest→newest so a trend can be charted: the P&L (omsætning / udgifter /
 * resultat), the balance-sheet development (balancesum / egenkapital) and the
 * two ratios an owner reads off a glance (bruttomargin, egenkapitalandel).
 *
 * The live year(s) are computed from the posted ledger via
 * `core/financial-statements` — exactly as `/income-statement` and `/balance`
 * do. The archived years (#197) are derived from `import_archive_balances`:
 * each archived account's closing balance is classified by joining its account
 * number to the live `accounts` table's `type` (and `normal_balance`), via the
 * shared #321 classification — the same rule the Balance view applies, so the
 * two views never disagree. Income accounts are credit-normal, so the archive's
 * signed balance is negated to read as a positive omsætning; expense accounts
 * read positive as-is. Assets are debit-normal (read as-is); equity is
 * credit-normal (negated) and carries the un-closed period result so it matches
 * the archive-aware Balance view.
 *
 * Throws `ApiError.notFound` when the slug is not registered or has no ledger.
 */
export function buildCompanyMultiYear(workspaceRoot: string, slug: string) {
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

  const db = openDb(dbPath);
  try {
    migrate(db);
    const company = getCompanySettings(db);

    // Account number → (type, normalBalance), for classifying archived
    // balances. The archive stores raw account numbers; the live chart of
    // accounts is the only source of an account's statement-section
    // classification. `normalBalance` is needed so a `vat` account is placed
    // by its normal balance — the same rule `buildCompanyBalance` applies.
    const accountTypeRows = db
      .query(
        "SELECT account_no AS accountNo, type AS type, normal_balance AS normalBalance FROM accounts",
      )
      .all() as Array<{
      accountNo: string;
      type: string;
      normalBalance: "debit" | "credit";
    }>;
    const accountType = new Map(
      accountTypeRows.map((r) => [
        r.accountNo,
        { type: r.type, normalBalance: r.normalBalance },
      ]),
    );

    // Bruttomargin (resultat ÷ omsætning) and egenkapitalandel (egenkapital ÷
    // balancesum) — each a 0–1 fraction, or null when its denominator is zero.
    // The same two ratios the Overblik view surfaces; no figure is invented.
    const ratios = (
      resultat: number,
      omsaetning: number,
      egenkapital: number,
      balancesum: number,
    ) => ({
      bruttomargin: omsaetning !== 0 ? resultat / omsaetning : null,
      egenkapitalandel: balancesum !== 0 ? egenkapital / balancesum : null,
    });

    const rows: MultiYearRow[] = [];
    for (const fy of years) {
      if (fy.source === "live") {
        const yearNum = parseInt(fy.label, 10);
        const yearEnd = `${yearNum}-12-31`;
        const pl = buildProfitAndLoss(db, `${yearNum}-01-01`, yearEnd);
        // Balance-sheet development — total assets and equity (the equity
        // section plus the un-closed period result), exactly as the Balance
        // and Overblik views compute them.
        const bs = buildBalanceSheet(db, yearEnd);
        const balancesum = roundKroner(bs.totalAssets);
        const egenkapital = roundKroner(bs.equity.total + bs.periodResult);
        const omsaetning = roundKroner(pl.totalIncome);
        const udgifter = roundKroner(pl.totalExpense);
        const resultat = roundKroner(pl.result);
        rows.push({
          year: fy.label,
          source: "live",
          omsaetning,
          udgifter,
          resultat,
          balancesum,
          egenkapital,
          ...ratios(resultat, omsaetning, egenkapital, balancesum),
        });
        continue;
      }

      // Archived year — classify each SaldoBalance line by account type. The
      // archive `amount` is debit-signed (debit − credit): income/equity are
      // credit-normal and read negated, expenses/assets read as-is.
      const archiveId = db
        .query(
          "SELECT id FROM import_archive_years WHERE fiscal_year = ? ORDER BY id DESC",
        )
        .get(parseInt(fy.label, 10)) as { id: number } | undefined;
      let omsaetning = 0;
      let udgifter = 0;
      let balancesum = 0;
      let equitySection = 0;
      if (archiveId) {
        const balRows = db
          .query(
            `SELECT account_no AS accountNo, amount AS amount
               FROM import_archive_balances
              WHERE archive_year_id = ?`,
          )
          .all(archiveId.id) as Array<{ accountNo: string; amount: number }>;
        for (const b of balRows) {
          const acc = accountType.get(b.accountNo);
          const amount = Number(b.amount ?? 0);
          // The statement section the account belongs to — the shared #321
          // classification, so the Flerårsoversigt agrees with the Balance
          // view (a `vat` account is placed by its normal balance, not left
          // unclassified). Liabilities do not feed any Flerårsoversigt figure.
          const section = classifyAccountSection(acc?.type, acc?.normalBalance);
          if (section === "income") omsaetning += -amount;
          else if (section === "expense") udgifter += amount;
          else if (section === "asset") balancesum += amount;
          else if (section === "equity") equitySection += -amount;
        }
      }
      omsaetning = roundKroner(omsaetning);
      udgifter = roundKroner(udgifter);
      const resultat = roundKroner(omsaetning - udgifter);
      balancesum = roundKroner(balancesum);
      // Equity carries the un-closed period result so it matches the
      // archive-aware Balance view (assets = liabilities + equity + result).
      const egenkapital = roundKroner(equitySection + resultat);
      rows.push({
        year: fy.label,
        source: "archive",
        omsaetning,
        udgifter,
        resultat,
        balancesum,
        egenkapital,
        ...ratios(resultat, omsaetning, egenkapital, balancesum),
      });
    }

    // Oldest→newest so the SPA can chart a trend left-to-right.
    rows.sort((a, b) => a.year.localeCompare(b.year));

    return {
      slug: entry.slug,
      company: statementCompanyBlock(company),
      years: rows,
    };
  } finally {
    db.close();
  }
}
