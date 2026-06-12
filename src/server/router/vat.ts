// VAT read handler.

import type { ServerConfig } from "../config";
import { buildCompanyVat, resolveYearParam } from "../data";
import { okResponse } from "./_shared";

export function handleCompanyVat(
  config: ServerConfig,
  slug: string,
  url: URL,
): Response {
  const year = resolveYearParam(url.searchParams.get("year"));
  const data = buildCompanyVat(config.workspaceRoot, slug, year);
  return okResponse({ vat: data });
}
