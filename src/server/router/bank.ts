// Bank-transaction list + registered bank-accounts read handlers.

import type { ServerConfig } from "../config";
import { buildCompanyBankAccounts } from "../data/bank-accounts-view";
import { buildCompanyBank, resolveYearParam } from "../data";
import { okResponse } from "./_shared";

export function handleCompanyBank(
  config: ServerConfig,
  slug: string,
  url: URL,
): Response {
  const year = resolveYearParam(url.searchParams.get("year"));
  const data = buildCompanyBank(config.workspaceRoot, slug, year);
  return okResponse({ bank: data });
}

/**
 * GET /api/companies/:slug/bank-accounts — Bankkonti + CSV-mapping-profiler
 * (#345). Read-only. POST på samme path opretter en konto.
 */
export function handleCompanyBankAccounts(
  config: ServerConfig,
  slug: string,
): Response {
  const data = buildCompanyBankAccounts(config.workspaceRoot, slug);
  return okResponse({ bankAccounts: data });
}
