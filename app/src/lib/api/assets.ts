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
import type { AssetDisposalInput, AssetDisposalApplyInput, AssetDisposalPreview, AssetDisposalReceipt } from "../types/assets";
import { ApiError, request, type ReadRequestOptions } from "./_shared";

// --- Anlæg (fixed assets) — #336 ----------------------------------------
//
// The cockpit becomes a THIRD caller of `src/core/assets.ts` alongside the
// CLI's `asset` sub-commands and the MCP `asset_*` tools — no depreciation
// arithmetic crosses the wire. Write actions carry `confirm: true`; the
// backend's `withCompanyMutation` enforces backup-lock + actor attribution.

export const assetsApi = {
  assetDisposalPreview: async (slug: string, assetId: number, input: AssetDisposalInput) => {
    validateDisposalInput(input, false);
    const r = await request<unknown>(disposalPath(slug, assetId, "disposal/preview"), { method: "POST", readOnly: true, body: JSON.stringify(input) });
    return disposalOutput(r, true, assetId, input) as AssetDisposalPreview;
  },
  disposeAsset: async (slug: string, assetId: number, input: AssetDisposalApplyInput) => {
    validateDisposalInput(input, true);
    const r = await request<unknown>(disposalPath(slug, assetId, "dispose"), { method: "POST", body: JSON.stringify(input) });
    return disposalOutput(r, false, assetId, input) as AssetDisposalReceipt;
  },
  assetDisposalReceipt: async (slug: string, assetId: number, key: string) => {
    if (!bounded(key, 160, 8)) throw new Error("Ugyldig kvitteringsnøgle.");
    const r = await request<unknown>(`${disposalPath(slug, assetId, "disposal-receipt")}?key=${encodeURIComponent(key)}`);
    return disposalOutput(r, false, assetId) as AssetDisposalReceipt;
  },
  /** GET /api/companies/:slug/assets — the anlægskartotek. */
  assets: (slug: string, options?: ReadRequestOptions) =>
    request<AssetsResponse>(
      `/api/companies/${encodeURIComponent(slug)}/assets`, options,
    ).then((r) => r.assets),

  /** GET .../assets/:id/next-depreciation — what posts on a "Beregn afskrivning". */
  assetNextDepreciation: (slug: string, assetId: number, options?: ReadRequestOptions) =>
    request<AssetNextDepreciationResponse>(
      `/api/companies/${encodeURIComponent(slug)}/assets/${assetId}/next-depreciation`, options,
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

function disposalPath(slug: string, id: number, suffix: string) {
  if (!safeId(id)) throw new Error("Ugyldigt anlægs-ID.");
  return `/api/companies/${encodeURIComponent(slug)}/assets/${id}/${suffix}`;
}
const safeId = (v: unknown): v is number => typeof v === "number" && Number.isSafeInteger(v) && v > 0;
const bounded = (v: unknown, max: number, min = 1): v is string => typeof v === "string" && v.length >= min && v.length <= max && Boolean(v.trim());
const hash = (v: unknown) => typeof v === "string" && /^[0-9a-f]{64}$/.test(v);
const date = (v: unknown) => typeof v === "string" && /^\d{4}-\d{2}-\d{2}$/.test(v) && v.slice(0,4) !== "0000" && Number.isFinite(Date.parse(`${v}T00:00:00Z`)) && new Date(`${v}T00:00:00Z`).toISOString().slice(0,10) === v;
const money = (v: unknown) => typeof v === "number" && Number.isFinite(v) && v >= 0 && v <= 90071992547409.9;
const obj = (v: unknown): v is Record<string, unknown> => !!v && typeof v === "object" && !Array.isArray(v);
function exact(v: Record<string, unknown>, keys: string[]) { return Object.keys(v).length === keys.length && keys.every(k => Object.hasOwn(v,k)); }
function validateDisposalInput(input: AssetDisposalInput, apply: boolean) {
  const v = input as unknown as Record<string, unknown>;
  const keys = ["transactionDate", "documentId", "reason", ...(v.lossAccountNo !== undefined ? ["lossAccountNo"] : []), ...(apply ? ["expectedPlanHash", "idempotencyKey", "confirm"] : [])];
  if (!exact(v, keys) || !date(v.transactionDate) || !safeId(v.documentId) || !bounded(v.reason,4096) || (v.lossAccountNo !== undefined && !bounded(v.lossAccountNo,128)) || (apply && (!hash(v.expectedPlanHash) || !bounded(v.idempotencyKey,160,8) || v.confirm !== true)) || new TextEncoder().encode(JSON.stringify(v)).length > 16384) throw new Error("Ugyldige felter til afhændelse.");
}
function disposalOutput(value: unknown, preview: boolean, id: number, input?: AssetDisposalInput): AssetDisposalPreview | AssetDisposalReceipt {
  const fail = () => { throw new ApiError("internal", "Serverens resultat kunne ikke valideres. Kontrollér kvitteringen før en ny handling.", 0); };
  const key = preview ? "disposalPreview" : "disposal";
  if (!obj(value) || !exact(value,["ok",key]) || value.ok !== true || !obj(value[key])) return fail();
  const p = value[key];
  const common = ["assetId","transactionDate","documentId","reason","cost","accumulatedDepreciation"];
  if (!exact(p,[...common,...(preview ? ["lossAccountNo","carryingValue","planHash","lines"] : ["disposalId","carryingValueWrittenOff","remainingCarryingValue","journalEntryId","journalEntryNo","journalEntryHash","replayed"])]) || p.assetId !== id || !date(p.transactionDate) || !safeId(p.documentId) || !bounded(p.reason,4096) || !money(p.cost) || !money(p.accumulatedDepreciation)) return fail();
  if (input && (p.transactionDate !== input.transactionDate || p.documentId !== input.documentId || p.reason !== input.reason)) return fail();
  if (preview) {
    if (!money(p.carryingValue) || !hash(p.planHash) || !(p.lossAccountNo === null || bounded(p.lossAccountNo,128)) || (input && p.lossAccountNo !== (input.lossAccountNo ?? null)) || !Array.isArray(p.lines) || p.lines.length < 2 || p.lines.length > 3 || !p.lines.every(l => obj(l) && exact(l,["accountNo","debit","credit"]) && bounded(l.accountNo,128) && money(l.debit) && money(l.credit))) return fail();
  } else if (!safeId(p.disposalId) || !safeId(p.journalEntryId) || !bounded(p.journalEntryNo,128) || !hash(p.journalEntryHash) || !money(p.carryingValueWrittenOff) || p.remainingCarryingValue !== 0 || typeof p.replayed !== "boolean") return fail();
  return p as unknown as AssetDisposalPreview | AssetDisposalReceipt;
}
