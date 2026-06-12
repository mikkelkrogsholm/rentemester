// Assets register + next-depreciation read handlers.

import type { ServerConfig } from "../config";
import {
  buildAssetNextDepreciationPeriod,
  buildCompanyAssets,
} from "../data";
import { okResponse } from "./_shared";

export function handleCompanyAssets(config: ServerConfig, slug: string): Response {
  const data = buildCompanyAssets(config.workspaceRoot, slug);
  return okResponse({ assets: data });
}

export function handleAssetNextDepreciation(
  config: ServerConfig,
  slug: string,
  assetIdRaw: string,
): Response {
  const assetId = Number(assetIdRaw);
  const data = buildAssetNextDepreciationPeriod(
    config.workspaceRoot,
    slug,
    assetId,
  );
  return okResponse({ nextDepreciation: data });
}
