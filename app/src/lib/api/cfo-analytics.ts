import type { CfoAnalyticsResponse, CfoAnalyticsScope } from "../types";
import { request, type ReadRequestOptions } from "./_shared";

export type CfoAnalyticsQuery = {
  scope: CfoAnalyticsScope;
  from: string;
  to: string;
  companySlug?: string;
  groupProfileId?: string;
};

/** The Cockpit reads the same versioned report as external HTTP and MCP callers. */
export const cfoAnalyticsApi = {
  cfoAnalytics: (input: CfoAnalyticsQuery, options?: ReadRequestOptions) => {
    const params = new URLSearchParams({ scope: input.scope, from: input.from, to: input.to });
    if (input.companySlug) params.append("companySlug", input.companySlug);
    if (input.groupProfileId) params.set("groupProfileId", input.groupProfileId);
    return request<CfoAnalyticsResponse>(`/api/cfo-analytics?${params.toString()}`, options);
  },
};
