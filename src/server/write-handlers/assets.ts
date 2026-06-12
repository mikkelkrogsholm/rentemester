// Anlæg (fixed assets) write handlers — #336.
//
// All three actions are write-irreversible and go through the SAME core
// (`registerAsset`, `postDepreciationPeriod`, `postImmediateWriteOff`) the
// CLI `asset` sub-commands and the MCP tools use — the cockpit becomes a
// third caller, never re-implementing the depreciation arithmetic. Every
// action goes through `withCompanyMutation`, so the backup-lock, the
// localhost gate, actor attribution and the confirm gate all apply.

import {
  registerAsset,
  postDepreciationPeriod,
  postImmediateWriteOff,
} from "../../core/assets";
import type { ServerConfig } from "../config";
import { ApiError } from "../errors";
import { withCockpitActor } from "../actor";
import { withCompanyMutation } from "../mutations";
import {
  okResponse,
  optionalBodyString,
  parseIdParam,
  requireBodyPositiveInt,
  requireBodyPositiveNumber,
  requireBodyString,
} from "./_shared";

/**
 * POST /api/companies/:slug/assets — registers a capitalised asset and writes
 * its deterministic linear depreciation plan. Third caller of `registerAsset`
 * (CLI: `asset register`, MCP: `asset_register`). The asset row is
 * append-only; the schedule is recomputed deterministically per the core
 * (never persisted by the cockpit). Write-irreversible, so the body carries
 * `confirm: true`.
 */
export async function handleAssetRegister(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      const name = requireBodyString(body, "name");
      const category = requireBodyString(body, "category");
      const acquisitionDate = requireBodyString(body, "acquisitionDate");
      const cost = requireBodyPositiveNumber(body, "cost");
      const usefulLifeMonths = requireBodyPositiveInt(body, "usefulLifeMonths");
      const purchaseDocumentId = requireBodyPositiveInt(
        body,
        "purchaseDocumentId",
      );
      const assetAccountNo = optionalBodyString(body, "assetAccountNo");
      const depreciationExpenseAccountNo = optionalBodyString(
        body,
        "depreciationExpenseAccountNo",
      );
      const accumulatedDepreciationAccountNo = optionalBodyString(
        body,
        "accumulatedDepreciationAccountNo",
      );
      const note = optionalBodyString(body, "note");
      return registerAsset(
        ctx.db,
        withCockpitActor(
          {
            name,
            category,
            acquisitionDate,
            cost,
            usefulLifeMonths,
            purchaseDocumentId,
            ...(assetAccountNo ? { assetAccountNo } : {}),
            ...(depreciationExpenseAccountNo
              ? { depreciationExpenseAccountNo }
              : {}),
            ...(accumulatedDepreciationAccountNo
              ? { accumulatedDepreciationAccountNo }
              : {}),
            ...(note ? { note } : {}),
          },
          ctx.actor,
        ),
      );
    },
    { requireConfirm: true },
  );

  return okResponse({
    asset: {
      assetId: result.assetId ?? null,
      totalPeriods: result.totalPeriods ?? null,
      periodAmount: result.periodAmount ?? null,
    },
  });
}

/**
 * POST /api/companies/:slug/assets/:id/depreciate — posts one period of an
 * asset's linear depreciation schedule. The body may carry `transactionDate`
 * and `periodIndex`; when `periodIndex` is omitted the next unposted period
 * is derived from `asset_depreciation_entries` so the cockpit's one-click
 * "Beregn afskrivning" action just works. Third caller of
 * `postDepreciationPeriod`.
 */
export async function handleAssetDepreciate(
  config: ServerConfig,
  request: Request,
  slug: string,
  idRaw: string,
): Promise<Response> {
  const assetId = parseIdParam(idRaw, "id");

  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      const transactionDate =
        optionalBodyString(body, "transactionDate") ??
        new Date().toISOString().slice(0, 10);

      let periodIndex: number;
      const explicit = body.periodIndex;
      if (explicit === undefined || explicit === null) {
        const posted = ctx.db.query(
          "SELECT COUNT(*) AS posted FROM asset_depreciation_entries WHERE asset_id = ?",
        ).get(assetId) as { posted: number };
        periodIndex = Number(posted.posted ?? 0) + 1;
      } else {
        if (
          typeof explicit !== "number" ||
          !Number.isInteger(explicit) ||
          explicit <= 0
        ) {
          throw ApiError.badRequest(
            "'periodIndex' must be a positive integer when present",
          );
        }
        periodIndex = explicit;
      }

      return postDepreciationPeriod(
        ctx.db,
        withCockpitActor(
          { assetId, periodIndex, transactionDate },
          ctx.actor,
        ),
      );
    },
    { requireConfirm: true },
  );

  return okResponse({
    depreciation: {
      entryId: result.entryId ?? null,
      assetId: result.assetId ?? null,
      periodIndex: result.periodIndex ?? null,
      periodAmount: result.periodAmount ?? null,
    },
  });
}

/**
 * POST /api/companies/:slug/assets/write-off — books a small purchase as a
 * straksafskrivning (immediate write-off). The endpoint is anchored at the
 * collection rather than `:id` because no `assets` row exists yet for a
 * straksafskrivning — the purchase document is expensed directly. Third
 * caller of `postImmediateWriteOff`. The core's threshold-rule-source guard
 * is propagated verbatim through the request body.
 */
export async function handleAssetWriteOff(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      const name = requireBodyString(body, "name");
      const category = requireBodyString(body, "category");
      const acquisitionDate = requireBodyString(body, "acquisitionDate");
      const transactionDate = requireBodyString(body, "transactionDate");
      const cost = requireBodyPositiveNumber(body, "cost");
      const purchaseDocumentId = requireBodyPositiveInt(
        body,
        "purchaseDocumentId",
      );
      const expenseAccountNo = requireBodyString(body, "expenseAccountNo");
      const thresholdRuleSource = requireBodyString(
        body,
        "thresholdRuleSource",
      );
      const paymentAccountNo = optionalBodyString(body, "paymentAccountNo");
      const note = optionalBodyString(body, "note");

      return postImmediateWriteOff(
        ctx.db,
        withCockpitActor(
          {
            name,
            category,
            acquisitionDate,
            transactionDate,
            cost,
            purchaseDocumentId,
            expenseAccountNo,
            thresholdRuleSource,
            // The cockpit modal IS the human's deliberate confirmation —
            // mirroring the CLI's `--confirm yes`. The server's
            // `withCompanyMutation` already required `confirm: true` on the
            // request body, so this propagates the same intent into core.
            confirmImmediateWriteOff: true,
            ...(paymentAccountNo ? { paymentAccountNo } : {}),
            ...(note ? { note } : {}),
          },
          ctx.actor,
        ),
      );
    },
    { requireConfirm: true },
  );

  return okResponse({
    writeOff: {
      writeOffId: result.writeOffId ?? null,
      entryId: result.entryId ?? null,
      cost: result.cost ?? null,
      thresholdDkk: result.thresholdDkk ?? null,
    },
  });
}
