// Company dashboard, fiscal-years, overview, multi-year read handlers.

import type { ServerConfig } from "../config";
import {
  buildCompanyDashboardData,
  buildCompanyFiscalYears,
  buildCompanyMultiYear,
  buildCompanyOverview,
  resolveAsOfDate,
  resolveYearParam,
} from "../data";
import { okResponse } from "./_shared";

export function handleCompanyDashboard(
  config: ServerConfig,
  slug: string,
  url: URL,
): Response {
  const asOf = resolveAsOfDate(url.searchParams.get("asOf"));
  const data = buildCompanyDashboardData(config.workspaceRoot, slug, asOf);
  return okResponse({ dashboard: data });
}

export function handleCompanyFiscalYears(config: ServerConfig, slug: string): Response {
  const data = buildCompanyFiscalYears(config.workspaceRoot, slug);
  return okResponse({ fiscalYears: data });
}

export function handleCompanyOverview(
  config: ServerConfig,
  slug: string,
  url: URL,
): Response {
  const year = resolveYearParam(url.searchParams.get("year"));
  const data = buildCompanyOverview(config.workspaceRoot, slug, year);
  return okResponse({ overview: data });
}

export function handleCompanyMultiYear(config: ServerConfig, slug: string): Response {
  const data = buildCompanyMultiYear(config.workspaceRoot, slug);
  return okResponse({ multiYear: data });
}
