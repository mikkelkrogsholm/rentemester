import type {
  AssetDepreciateInput,
  AssetDepreciateSummary,
  AssetNextDepreciationResponse,
  AssetRegisterInput,
  AssetRegisterSummary,
  AssetWriteOffInput,
  AssetWriteOffSummary,
  AssetsResponse,
} from "../types";
import { request } from "./_shared";

// --- Anlæg (fixed assets) — #336 ----------------------------------------
//
// The cockpit becomes a THIRD caller of `src/core/assets.ts` alongside the
// CLI's `asset` sub-commands and the MCP `asset_*` tools — no depreciation
// arithmetic crosses the wire. Write actions carry `confirm: true`; the
// backend's `withCompanyMutation` enforces backup-lock + actor attribution.

export const assetsApi = {
  /** GET /api/companies/:slug/assets — the anlægskartotek. */
  assets: (slug: string) =>
    request<AssetsResponse>(
      `/api/companies/${encodeURIComponent(slug)}/assets`,
    ).then((r) => r.assets),

  /** GET .../assets/:id/next-depreciation — what posts on a "Beregn afskrivning". */
  assetNextDepreciation: (slug: string, assetId: number) =>
    request<AssetNextDepreciationResponse>(
      `/api/companies/${encodeURIComponent(slug)}/assets/${assetId}/next-depreciation`,
    ).then((r) => r.nextDepreciation),

  /** POST /api/companies/:slug/assets — registers a capitalised anlæg. */
  registerAsset: (slug: string, input: AssetRegisterInput) =>
    request<{ ok: true; asset: AssetRegisterSummary }>(
      `/api/companies/${encodeURIComponent(slug)}/assets`,
      {
        method: "POST",
        body: JSON.stringify({
          name: input.name,
          category: input.category,
          acquisitionDate: input.acquisitionDate,
          cost: input.cost,
          usefulLifeMonths: input.usefulLifeMonths,
          purchaseDocumentId: input.purchaseDocumentId,
          ...(input.assetAccountNo
            ? { assetAccountNo: input.assetAccountNo }
            : {}),
          ...(input.depreciationExpenseAccountNo
            ? {
                depreciationExpenseAccountNo:
                  input.depreciationExpenseAccountNo,
              }
            : {}),
          ...(input.accumulatedDepreciationAccountNo
            ? {
                accumulatedDepreciationAccountNo:
                  input.accumulatedDepreciationAccountNo,
              }
            : {}),
          ...(input.note ? { note: input.note } : {}),
          confirm: true,
        }),
      },
    ).then((r) => r.asset),

  /** POST .../assets/:id/depreciate — posts the next depreciation period. */
  depreciateAsset: (
    slug: string,
    assetId: number,
    input: AssetDepreciateInput = {},
  ) =>
    request<{ ok: true; depreciation: AssetDepreciateSummary }>(
      `/api/companies/${encodeURIComponent(slug)}/assets/${assetId}/depreciate`,
      {
        method: "POST",
        body: JSON.stringify({
          ...(input.transactionDate
            ? { transactionDate: input.transactionDate }
            : {}),
          ...(input.periodIndex !== undefined
            ? { periodIndex: input.periodIndex }
            : {}),
          confirm: true,
        }),
      },
    ).then((r) => r.depreciation),

  /** POST .../assets/write-off — books a straksafskrivning. */
  writeOffAsset: (slug: string, input: AssetWriteOffInput) =>
    request<{ ok: true; writeOff: AssetWriteOffSummary }>(
      `/api/companies/${encodeURIComponent(slug)}/assets/write-off`,
      {
        method: "POST",
        body: JSON.stringify({
          name: input.name,
          category: input.category,
          acquisitionDate: input.acquisitionDate,
          transactionDate: input.transactionDate,
          cost: input.cost,
          purchaseDocumentId: input.purchaseDocumentId,
          expenseAccountNo: input.expenseAccountNo,
          thresholdRuleSource: input.thresholdRuleSource,
          ...(input.paymentAccountNo
            ? { paymentAccountNo: input.paymentAccountNo }
            : {}),
          ...(input.note ? { note: input.note } : {}),
          confirm: true,
        }),
      },
    ).then((r) => r.writeOff),
};
