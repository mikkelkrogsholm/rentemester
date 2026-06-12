import { existsSync } from "node:fs";
import { companyPaths } from "../../../core/paths";
import { openDb, migrate } from "../../../core/db";
import { getCompanySettings } from "../../../core/company";
import {
  buildAssetRegisterReport,
  computeDepreciationSchedule,
  type AssetRegisterRow,
} from "../../../core/assets";
import {
  companyRootForSlug,
  findWorkspaceCompany,
} from "../../../core/workspace";
import { ApiError } from "../../errors";
import {
  requireCompanyDbPath,
  roundKroner,
  statementCompanyBlock,
} from "../shared";
import type { StatementCompanyBlock } from "../shared";

// --------------------------------------------------------------------------
// Anlæg (fixed assets) — #336
// --------------------------------------------------------------------------

/**
 * One row in the cockpit's Anlæg view. Mirrors `AssetRegisterRow` from
 * `core/assets.ts` (the SAME register the CLI's `asset register-report` and
 * the MCP tool consume) and augments it with the derived "anlæg-status":
 *
 *   - `active`           — at least one period still left to depreciate;
 *   - `fully-depreciated`— every scheduled period has been posted (book value 0);
 *
 * Plus the `remainingPeriods` countdown so the UI can show "X af N afskrevet"
 * without re-computing the schedule on the client. `note` is null today
 * (`buildAssetRegisterReport` does not expose it) and reserved for a follow-up.
 */
export type AssetRow = AssetRegisterRow & {
  status: "active" | "fully-depreciated";
  remainingPeriods: number;
};

/**
 * One row in the straksafskrivning history list — small purchases that were
 * expensed in one go via `postImmediateWriteOff` instead of being capitalised.
 */
export type AssetWriteOffRow = {
  id: number;
  name: string;
  category: string;
  acquisitionDate: string;
  writeOffDate: string;
  cost: number;
  expenseAccountNo: string;
  thresholdDkk: number;
  thresholdRuleSource: string;
  note: string | null;
  purchaseDocumentId: number;
  journalEntryId: number;
};

export type CompanyAssets = {
  slug: string;
  company: StatementCompanyBlock;
  /** Capitalised assets, oldest acquisition first. */
  assets: AssetRow[];
  /** Straksafskrivninger, oldest write-off first. */
  writeOffs: AssetWriteOffRow[];
  totals: {
    cost: number;
    accumulatedDepreciation: number;
    netBookValue: number;
    activeCount: number;
    fullyDepreciatedCount: number;
    writeOffCount: number;
    writeOffTotal: number;
  };
};

/**
 * Anlægskartoteket — backing `GET /api/companies/:slug/assets` (#336). Reads
 * EXACTLY what `buildAssetRegisterReport` returns (the deterministic SAME core
 * the CLI's `asset register-report` and the MCP tool consume), enriches each
 * row with the derived anlæg-status the cockpit's list and badges need, and
 * appends the straksafskrivning history so the owner can see both halves of
 * the asset workflow on one page. No business logic is duplicated.
 */
export function buildCompanyAssets(
  workspaceRoot: string,
  slug: string,
): CompanyAssets {
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
    const report = buildAssetRegisterReport(db);

    const assets: AssetRow[] = report.assets
      .slice()
      .sort((a, b) => a.acquisitionDate.localeCompare(b.acquisitionDate))
      .map((row) => {
        const remaining = Math.max(0, row.usefulLifeMonths - row.postedPeriods);
        return {
          ...row,
          remainingPeriods: remaining,
          status: remaining === 0 ? "fully-depreciated" : "active",
        };
      });

    const writeOffRows = db.query(
      `SELECT id, name, category, acquisition_date, writeoff_date, cost,
              expense_account_no, threshold_dkk, threshold_rule_source, note,
              purchase_document_id, journal_entry_id
         FROM asset_writeoffs
         ORDER BY writeoff_date ASC, id ASC`,
    ).all() as Array<{
      id: number;
      name: string;
      category: string;
      acquisition_date: string;
      writeoff_date: string;
      cost: number;
      expense_account_no: string;
      threshold_dkk: number;
      threshold_rule_source: string;
      note: string | null;
      purchase_document_id: number;
      journal_entry_id: number;
    }>;
    const writeOffs: AssetWriteOffRow[] = writeOffRows.map((row) => ({
      id: row.id,
      name: row.name,
      category: row.category,
      acquisitionDate: row.acquisition_date,
      writeOffDate: row.writeoff_date,
      cost: roundKroner(Number(row.cost)),
      expenseAccountNo: row.expense_account_no,
      thresholdDkk: roundKroner(Number(row.threshold_dkk)),
      thresholdRuleSource: row.threshold_rule_source,
      note: row.note,
      purchaseDocumentId: row.purchase_document_id,
      journalEntryId: row.journal_entry_id,
    }));

    const activeCount = assets.filter((a) => a.status === "active").length;
    const fullyDepreciatedCount = assets.length - activeCount;
    const writeOffTotal = writeOffs.reduce((sum, row) => sum + row.cost, 0);

    return {
      slug: entry.slug,
      company: statementCompanyBlock(company),
      assets,
      writeOffs,
      totals: {
        cost: report.totals.cost,
        accumulatedDepreciation: report.totals.accumulatedDepreciation,
        netBookValue: report.totals.netBookValue,
        activeCount,
        fullyDepreciatedCount,
        writeOffCount: writeOffs.length,
        writeOffTotal: roundKroner(writeOffTotal),
      },
    };
  } finally {
    db.close();
  }
}

/**
 * The next un-posted depreciation period for a single asset. Used by the
 * cockpit's "Beregn afskrivning"-action to show the owner exactly which period
 * will be posted (#336). The schedule is recomputed via the deterministic core
 * — never persisted — and the next period is whichever index has no row in
 * `asset_depreciation_entries` yet.
 */
export type NextDepreciationPeriodView = {
  assetId: number;
  totalPeriods: number;
  postedPeriods: number;
  remainingPeriods: number;
  nextPeriodIndex: number | null;
  nextPeriodAmount: number | null;
};

export function buildAssetNextDepreciationPeriod(
  workspaceRoot: string,
  slug: string,
  assetId: number,
): NextDepreciationPeriodView {
  if (!Number.isInteger(assetId) || assetId <= 0) {
    throw ApiError.badRequest("'assetId' must be a positive integer");
  }
  const dbPath = requireCompanyDbPath(workspaceRoot, slug);
  const db = openDb(dbPath);
  try {
    migrate(db);
    const asset = db.query(
      `SELECT id, cost, useful_life_months, acquisition_date FROM assets WHERE id = ?`,
    ).get(assetId) as {
      id: number;
      cost: number;
      useful_life_months: number;
      acquisition_date: string;
    } | null;
    if (!asset) {
      throw ApiError.notFound(`asset ${assetId} does not exist`);
    }
    const schedule = computeDepreciationSchedule({
      cost: Number(asset.cost),
      acquisitionDate: asset.acquisition_date,
      usefulLifeMonths: asset.useful_life_months,
      method: "linear",
    });
    const postedRow = db.query(
      "SELECT COUNT(*) AS posted FROM asset_depreciation_entries WHERE asset_id = ?",
    ).get(assetId) as { posted: number };
    const posted = Number(postedRow.posted ?? 0);
    const remaining = schedule.length - posted;
    const nextIndex = remaining > 0 ? posted + 1 : null;
    const nextAmount =
      nextIndex !== null ? schedule[nextIndex - 1]!.amount : null;
    return {
      assetId,
      totalPeriods: schedule.length,
      postedPeriods: posted,
      remainingPeriods: Math.max(0, remaining),
      nextPeriodIndex: nextIndex,
      nextPeriodAmount: nextAmount,
    };
  } finally {
    db.close();
  }
}
