// Portfolio + workspace company-list read handlers.

import type { ServerConfig } from "../config";
import { discoverWorkspaceCompanies } from "../discovery";
import { buildPortfolioOverview, resolveAsOfDate } from "../data";
import { okResponse } from "./_shared";

export function handlePortfolio(config: ServerConfig, url: URL): Response {
  const asOf = resolveAsOfDate(url.searchParams.get("asOf"));
  const overview = buildPortfolioOverview(config.workspaceRoot, asOf);
  return okResponse({ portfolio: overview });
}

export function handleCompanyList(config: ServerConfig): Response {
  // Discover-and-adopt any present-but-unlisted company directory before
  // listing (#256): an owner who set a company up via the CLI then opened the
  // cockpit must see that real company, not "0 virksomheder" + a blank create.
  const companies = discoverWorkspaceCompanies(config.workspaceRoot);
  return okResponse({
    workspace: config.workspaceRoot,
    count: companies.length,
    companies: companies.map((c) => ({
      slug: c.slug,
      name: c.name,
      createdAt: c.createdAt,
      archived: c.archived,
    })),
  });
}
