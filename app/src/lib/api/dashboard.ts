import type {
  ArchiveResponse,
  CashflowResponse,
  DashboardResponse,
  FiscalYearsResponse,
  MultiYearResponse,
  ObligationsResponse,
  OverviewResponse,
} from "../types";
import { request, type ReadRequestOptions } from "./_shared";

export const dashboardApi = {
  dashboard: (slug: string, asOf?: string, options?: ReadRequestOptions) =>
    request<DashboardResponse>(
      `/api/companies/${encodeURIComponent(slug)}/dashboard${
        asOf ? `?asOf=${encodeURIComponent(asOf)}` : ""
      }`,
      options,
    ).then((r) => r.dashboard),

  fiscalYears: (slug: string, options?: ReadRequestOptions) =>
    request<FiscalYearsResponse>(
      `/api/companies/${encodeURIComponent(slug)}/fiscal-years`,
      options,
    ).then((r) => r.fiscalYears.years),

  overview: (slug: string, year?: string, asOf?: string, options?: ReadRequestOptions) => {
    const params = new URLSearchParams();
    if (year) params.set("year", year);
    if (asOf) params.set("asOf", asOf);
    const query = params.toString();
    return request<OverviewResponse>(
      `/api/companies/${encodeURIComponent(slug)}/overview${
        query ? `?${query}` : ""
      }`,
      options,
    ).then((r) => r.overview);
  },

  archive: (slug: string, year: string, options?: ReadRequestOptions) =>
    request<ArchiveResponse>(
      `/api/companies/${encodeURIComponent(slug)}/archive/${encodeURIComponent(
        year,
      )}`,
      options,
    ).then((r) => r.archive),

  multiYear: (slug: string, options?: ReadRequestOptions) =>
    request<MultiYearResponse>(
      `/api/companies/${encodeURIComponent(slug)}/multi-year`,
      options,
    ).then((r) => r.multiYear),

  obligations: (slug: string, year?: string, options?: ReadRequestOptions) =>
    request<ObligationsResponse>(
      `/api/companies/${encodeURIComponent(slug)}/obligations${
        year ? `?year=${encodeURIComponent(year)}` : ""
      }`,
      options,
    ).then((r) => r.obligations),

  cashflow: (slug: string, year?: string, options?: ReadRequestOptions) =>
    request<CashflowResponse>(
      `/api/companies/${encodeURIComponent(slug)}/cashflow${
        year ? `?year=${encodeURIComponent(year)}` : ""
      }`,
      options,
    ).then((r) => r.cashflow),
};
