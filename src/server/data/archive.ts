// Archived fiscal year (#197) read helpers for the cockpit backend (#320).
//
// Split out of `server/data.ts` by #320. A Dinero export ships a full
// `SaldoBalance.csv` (every account's closing balance) and `Posteringer.csv`
// (every posting line) per archived year. That is enough to render the same
// Resultatopgørelse / Balance / Saldobalance / Posteringer / Overblik the live
// ledger does — the only difference is the figures come from the archive,
// never from a posted journal entry.
//
// Sign convention: the archived `amount` is debit-signed, exactly like a live
// trial-balance `balance` (debit − credit). So income reads positive as
// `−amount`, expenses as `amount`, assets as `amount`, liabilities/equity as
// `−amount` — the same conversions `core/financial-statements` applies.
//
// Behaviour is unchanged from the pre-split `server/data.ts`.

import { existsSync } from "node:fs";
import type { Database } from "bun:sqlite";
import { companyPaths } from "../../core/paths";
import { openDb, migrate } from "../../core/db";
import { getCompanySettings } from "../../core/company";
import {
  companyRootForSlug,
  findWorkspaceCompany,
} from "../../core/workspace";
import { ApiError } from "../errors";
import { roundKroner, statementCompanyBlock } from "./shared";

export type IncomeStatementLine = {
  accountNo: string;
  name: string;
  amount: number;
  /** The same account's amount in the prior calendar year, kroner. */
  priorAmount: number;
};

/** The `import_archive_years` header row for a fiscal year, or null. */
export function archiveYearRow(
  db: Database,
  year: number,
): { id: number; sourceSystem: string } | null {
  const row = db
    .query(
      `SELECT id, source_system AS sourceSystem
         FROM import_archive_years
        WHERE fiscal_year = ?
        ORDER BY id DESC`,
    )
    .get(year) as { id: number; sourceSystem: string } | undefined;
  return row ?? null;
}

/** One archived `SaldoBalance` line joined to the live chart's account type. */
export type ArchiveTypedBalance = {
  accountNo: string;
  name: string;
  /** Debit-signed closing balance, kroner (debit − credit). */
  amount: number;
  /** The live `accounts.type`, or null when the account is unknown. */
  type: string | null;
  /** The live `accounts.normal_balance`, or null when unknown. */
  normalBalance: "debit" | "credit" | null;
};

/**
 * Every archived `SaldoBalance` line for `archiveYearId`, each joined to the
 * live chart of accounts so it carries the account `type` and `normalBalance`
 * needed to classify it into a statement section. Ordered by account number.
 */
export function archiveTypedBalances(
  db: Database,
  archiveYearId: number,
): ArchiveTypedBalance[] {
  const rows = db
    .query(
      `SELECT b.account_no     AS accountNo,
              b.account_name   AS name,
              b.amount         AS amount,
              a.type           AS type,
              a.normal_balance AS normalBalance
         FROM import_archive_balances b
         LEFT JOIN accounts a ON a.account_no = b.account_no
        WHERE b.archive_year_id = ?
        ORDER BY b.account_no ASC`,
    )
    .all(archiveYearId) as Array<{
    accountNo: string;
    name: string | null;
    amount: number;
    type: string | null;
    normalBalance: "debit" | "credit" | null;
  }>;
  return rows.map((r) => ({
    accountNo: r.accountNo,
    name: r.name ?? "",
    amount: roundKroner(r.amount),
    type: r.type,
    normalBalance: r.normalBalance,
  }));
}

/**
 * Resultatopgørelse figures for an archived year, classified from the archived
 * `SaldoBalance` by the live chart's account `type`. Returns empty totals when
 * the year is not archived. Money is kroner.
 */
export function archiveIncomeStatement(
  db: Database,
  year: number,
): {
  income: IncomeStatementLine[];
  expense: IncomeStatementLine[];
  totalIncome: number;
  totalExpense: number;
  result: number;
} {
  const header = archiveYearRow(db, year);
  if (!header) {
    return { income: [], expense: [], totalIncome: 0, totalExpense: 0, result: 0 };
  }
  const income: IncomeStatementLine[] = [];
  const expense: IncomeStatementLine[] = [];
  let totalIncome = 0;
  let totalExpense = 0;
  for (const b of archiveTypedBalances(db, header.id)) {
    if (b.type === "income") {
      // Income is credit-normal — negate the debit-signed archive balance.
      const amount = roundKroner(-b.amount);
      income.push({ accountNo: b.accountNo, name: b.name, amount, priorAmount: 0 });
      totalIncome += amount;
    } else if (b.type === "expense") {
      const amount = roundKroner(b.amount);
      expense.push({ accountNo: b.accountNo, name: b.name, amount, priorAmount: 0 });
      totalExpense += amount;
    }
  }
  totalIncome = roundKroner(totalIncome);
  totalExpense = roundKroner(totalExpense);
  return {
    income,
    expense,
    totalIncome,
    totalExpense,
    result: roundKroner(totalIncome - totalExpense),
  };
}

// --------------------------------------------------------------------------
// Per-company archive (Arkiv — a single archived year) — cockpit-redesign it. 4
// --------------------------------------------------------------------------

/** One archived `SaldoBalance.csv` line — an account's closing balance. */
export type ArchiveBalanceRow = {
  accountNo: string;
  name: string;
  /** Closing balance, kroner, exactly as the Dinero export stored it. */
  amount: number;
};

export type CompanyArchiveYear = ReturnType<typeof buildCompanyArchiveYear>;

/**
 * Arkiv — one archived fiscal year's read-only reference data (#197). Returns
 * that year's full `SaldoBalance` (every account: number, name, closing
 * amount) from `import_archive_balances`, plus a summary of its archived
 * `Posteringer` (the line count and total). Nothing here touches the live
 * ledger — the archive is append-only Dinero export rows, never posted.
 *
 * Throws `ApiError.notFound` when the slug is not registered, the ledger is
 * missing, or the company has no archived data for `year`.
 */
export function buildCompanyArchiveYear(
  workspaceRoot: string,
  slug: string,
  year: number,
) {
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

    const yearRow = db
      .query(
        `SELECT id, source_system AS sourceSystem,
                posting_count AS postingCount,
                balance_count AS balanceCount,
                imported_at   AS importedAt
           FROM import_archive_years
          WHERE fiscal_year = ?
          ORDER BY id DESC`,
      )
      .get(year) as
      | {
          id: number;
          sourceSystem: string;
          postingCount: number;
          balanceCount: number;
          importedAt: string;
        }
      | undefined;
    if (!yearRow) {
      throw ApiError.notFound(
        `company '${slug}' has no archived data for ${year}`,
      );
    }

    const balanceRows = db
      .query(
        `SELECT account_no AS accountNo, account_name AS name, amount AS amount
           FROM import_archive_balances
          WHERE archive_year_id = ?
          ORDER BY account_no ASC`,
      )
      .all(yearRow.id) as Array<{
      accountNo: string;
      name: string | null;
      amount: number;
    }>;
    const saldoBalance: ArchiveBalanceRow[] = balanceRows.map((r) => ({
      accountNo: r.accountNo,
      name: r.name ?? "",
      amount: roundKroner(r.amount),
    }));

    // A summary of the archived postings — count + total amount. The signed
    // archive `amount` sums to ~0 over a balanced year, so the total here is
    // the gross posting volume (sum of absolute amounts) for an at-a-glance
    // sense of activity.
    const postingSummary = db
      .query(
        `SELECT COUNT(*) AS count,
                COALESCE(SUM(ABS(amount)), 0) AS grossTotal
           FROM import_archive_postings
          WHERE archive_year_id = ?`,
      )
      .get(yearRow.id) as { count: number; grossTotal: number };

    return {
      slug: entry.slug,
      company: statementCompanyBlock(company),
      year: String(year),
      sourceSystem: yearRow.sourceSystem,
      importedAt: yearRow.importedAt,
      saldoBalance,
      postings: {
        count: postingSummary.count,
        grossTotal: roundKroner(postingSummary.grossTotal),
      },
    };
  } finally {
    db.close();
  }
}
