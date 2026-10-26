// Portfolio + workspace company-list read handlers.

import type { ServerConfig } from "../config";
import { buildPortfolioOverview, resolveAsOfDate } from "../data";
import { okResponse } from "./_shared";
import { openWorkspaceControlDb } from "../../core/workspace-control";
import { listActiveCompanyMembershipSlugs } from "../../core/workspace-access";
import {
  requireCanonicalLiveCompanies,
  WorkspaceCanonicalityError,
} from "../../core/workspace";
import { ApiError } from "../errors";

function canonicalCompanies(config: ServerConfig) {
  try {
    return requireCanonicalLiveCompanies(config.workspaceRoot).map((item) => item.entry);
  } catch (error) {
    if (error instanceof WorkspaceCanonicalityError) {
      throw ApiError.conflict(error.message, { subcode: "WORKSPACE_CANONICALITY_FAILED" });
    }
    throw error;
  }
}

/**
 * Hosted Better Auth reads are restricted before any discovery or ledger read.
 * Local and shared-secret modes intentionally retain their legacy full-workspace
 * behaviour for single-owner installations.
 */
function hostedVisibleCompanySlugs(config: ServerConfig): string[] | null {
  if (!config.betterAuthProvider || (config.requestPrincipal?.via !== "better-auth" && config.requestPrincipal?.via !== "service-principal")) return null;
  const userId = config.requestPrincipal.userId ?? "";
  const db = openWorkspaceControlDb(config.workspaceRoot);
  try {
    return listActiveCompanyMembershipSlugs(db, config.workspaceRoot, userId);
  } finally {
    db.close();
  }
}

export function handlePortfolio(config: ServerConfig, url: URL): Response {
  const asOf = resolveAsOfDate(url.searchParams.get("asOf"));
  const visibleSlugs = hostedVisibleCompanySlugs(config);
  try {
    const overview = buildPortfolioOverview(
      config.workspaceRoot,
      asOf,
      visibleSlugs === null ? {} : { companySlugs: visibleSlugs },
    );
    return okResponse({ portfolio: overview });
  } catch (error) {
    if (error instanceof WorkspaceCanonicalityError) {
      throw ApiError.conflict(error.message, { subcode: "WORKSPACE_CANONICALITY_FAILED" });
    }
    throw error;
  }
}

export function handleCompanyList(config: ServerConfig): Response {
  const visibleSlugs = hostedVisibleCompanySlugs(config);
  const canonical = canonicalCompanies(config);
  const companies = visibleSlugs === null ? canonical : canonical.filter((company) => new Set(visibleSlugs).has(company.slug));
  return okResponse({
    workspace: config.workspaceRoot,
    count: companies.length,
    companies: companies.map((c) => ({
      slug: c.slug,
      name: c.name,
      createdAt: c.createdAt,
      archived: c.archived,
      purpose: c.purpose,
    })),
  });
}
