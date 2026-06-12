// Anlæg (fixed assets) — #336.

import type { StatementCompany } from "./common";

/** One row in the Anlæg list — a capitalised asset and its derived status. */
export type AssetRow = {
  assetId: number;
  name: string;
  category: string;
  acquisitionDate: string;
  cost: number;
  usefulLifeMonths: number;
  postedPeriods: number;
  accumulatedDepreciation: number;
  netBookValue: number;
  status: "active" | "fully-depreciated";
  remainingPeriods: number;
};

/** One row in the straksafskrivning history list. */
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

/** Backs `GET /api/companies/:slug/assets` — the Anlæg page payload. */
export type CompanyAssets = {
  slug: string;
  company: StatementCompany;
  assets: AssetRow[];
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

export type AssetsResponse = {
  ok: true;
  assets: CompanyAssets;
};

/** The next-depreciation-period preview for a single asset. */
export type AssetNextDepreciation = {
  assetId: number;
  totalPeriods: number;
  postedPeriods: number;
  remainingPeriods: number;
  nextPeriodIndex: number | null;
  nextPeriodAmount: number | null;
};

export type AssetNextDepreciationResponse = {
  ok: true;
  nextDepreciation: AssetNextDepreciation;
};

/** Input for `api.registerAsset`. */
export type AssetRegisterInput = {
  name: string;
  category: string;
  acquisitionDate: string;
  cost: number;
  usefulLifeMonths: number;
  purchaseDocumentId: number;
  assetAccountNo?: string;
  depreciationExpenseAccountNo?: string;
  accumulatedDepreciationAccountNo?: string;
  note?: string;
};

export type AssetRegisterSummary = {
  assetId: number | null;
  totalPeriods: number | null;
  periodAmount: number | null;
};

/** Input for `api.depreciateAsset`. `periodIndex` is derived server-side. */
export type AssetDepreciateInput = {
  transactionDate?: string;
  periodIndex?: number;
};

export type AssetDepreciateSummary = {
  entryId: number | null;
  assetId: number | null;
  periodIndex: number | null;
  periodAmount: number | null;
};

/** Input for `api.writeOffAsset` — books a straksafskrivning. */
export type AssetWriteOffInput = {
  name: string;
  category: string;
  acquisitionDate: string;
  transactionDate: string;
  cost: number;
  purchaseDocumentId: number;
  expenseAccountNo: string;
  thresholdRuleSource: string;
  paymentAccountNo?: string;
  note?: string;
};

export type AssetWriteOffSummary = {
  writeOffId: number | null;
  entryId: number | null;
  cost: number | null;
  thresholdDkk: number | null;
};
