import type { CfoAnalyticsResponse, CfoAnalyticsScope } from "../types";
import { request, type ReadRequestOptions } from "./_shared";

export type CfoAnalyticsQuery = {
  scope: CfoAnalyticsScope;
  from: string;
  to: string;
  companySlug?: string | string[];
  account?: string;
  party?: string;
  dimension?: string;
  currency?: string;
  aggregate?: "none" | "sum";
  cursor?: string;
  limit?: number;
  groupProfileId?: string;
};

/** The Cockpit reads the same versioned report as external HTTP and MCP callers. */
export const cfoAnalyticsApi = {
  cfoAnalytics: (input: CfoAnalyticsQuery, options?: ReadRequestOptions) => {
    const params = new URLSearchParams({ scope: input.scope, from: input.from, to: input.to });
    for (const slug of Array.isArray(input.companySlug) ? input.companySlug : input.companySlug ? [input.companySlug] : []) params.append("companySlug", slug);
    for (const key of ["account", "party", "dimension", "currency", "aggregate", "cursor"] as const) if (input[key] !== undefined) params.set(key, input[key]);
    if (input.limit !== undefined) params.set("limit", String(input.limit));
    if (input.groupProfileId) params.set("groupProfileId", input.groupProfileId);
    return request<CfoAnalyticsResponse>(`/api/cfo-analytics?${params.toString()}`, options);
  },
};
