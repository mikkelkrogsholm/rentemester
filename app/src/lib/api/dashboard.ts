import type { NativeCashflow } from "../types/native-reports";
import type { ArchiveResponse, CashflowResponse, DashboardResponse, FiscalYearsResponse, MultiYearResponse, ObligationsResponse, OverviewResponse, ChangesSinceResponse } from "../types";
import { checkedReportSource } from "./statements";
import type { ReportSource } from "../types/common";
import { request, type ReadRequestOptions } from "./_shared";
export const dashboardApi = {
  dashboard: (slug: string, asOf?: string, options?: ReadRequestOptions) => request<DashboardResponse>(`/api/companies/${encodeURIComponent(slug)}/dashboard${asOf ? `?asOf=${encodeURIComponent(asOf)}` : ""}`, options).then(r => r.dashboard),
  fiscalYears: (slug: string, options?: ReadRequestOptions) => request<FiscalYearsResponse>(`/api/companies/${encodeURIComponent(slug)}/fiscal-years`, options).then(r => r.fiscalYears.years),
  overview: (slug: string, year?: string, asOf?: string, options?: ReadRequestOptions & {
    source?: ReportSource;
  }) => {
    const params = new URLSearchParams();
    if (year) params.set("year", year);
    if (asOf) params.set("asOf", asOf);
    if (options?.source) params.set("source", options.source);
    const query = params.toString();
    return request<OverviewResponse>(`/api/companies/${encodeURIComponent(slug)}/overview${query ? `?${query}` : ""}`, options ? {
      signal: options.signal
    } : undefined).then(r => checkedReportSource(r.overview, options?.source));
  },
  changesSince: (slug: string, after = 0, options?: ReadRequestOptions) => request<ChangesSinceResponse>(`/api/companies/${encodeURIComponent(slug)}/changes-since?after=${encodeURIComponent(String(after))}`, options).then(r => r.changes),
  archive: (slug: string, year: string, options?: ReadRequestOptions) => request<ArchiveResponse>(`/api/companies/${encodeURIComponent(slug)}/archive/${encodeURIComponent(year)}`, options).then(r => r.archive),
  multiYear: (slug: string, options?: ReadRequestOptions) => request<MultiYearResponse>(`/api/companies/${encodeURIComponent(slug)}/multi-year`, options).then(r => r.multiYear),
  obligations: (slug: string, year?: string, options?: ReadRequestOptions) => request<ObligationsResponse>(`/api/companies/${encodeURIComponent(slug)}/obligations${year ? `?year=${encodeURIComponent(year)}` : ""}`, options).then(r => r.obligations),
  nativeCashflow: (slug: string, year?: string, options?: ReadRequestOptions) => request<{
    ok: true;
    cashflow: NativeCashflow;
  }>(`/api/companies/${encodeURIComponent(slug)}/cashflow${year ? `?year=${encodeURIComponent(year)}` : ""}`, options).then(r => r.cashflow),
  cashflow: (slug: string, year?: string, options?: ReadRequestOptions) => request<CashflowResponse>(`/api/companies/${encodeURIComponent(slug)}/cashflow${year ? `?year=${encodeURIComponent(year)}` : ""}`, options).then(r => r.cashflow)
};