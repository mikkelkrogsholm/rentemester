import type {
  ArchiveResponse,
  CashflowResponse,
  DashboardResponse,
  FiscalYearsResponse,
  MultiYearResponse,
  ObligationsResponse,
  OverviewResponse,
} from "../types";
import { request } from "./_shared";

export const dashboardApi = {
  dashboard: (slug: string, asOf?: string) =>
    request<DashboardResponse>(
      `/api/companies/${encodeURIComponent(slug)}/dashboard${
        asOf ? `?asOf=${encodeURIComponent(asOf)}` : ""
      }`,
    ).then((r) => r.dashboard),

  fiscalYears: (slug: string) =>
    request<FiscalYearsResponse>(
      `/api/companies/${encodeURIComponent(slug)}/fiscal-years`,
    ).then((r) => r.fiscalYears.years),

  overview: (slug: string, year?: string) =>
    request<OverviewResponse>(
      `/api/companies/${encodeURIComponent(slug)}/overview${
        year ? `?year=${encodeURIComponent(year)}` : ""
      }`,
    ).then((r) => r.overview),

  archive: (slug: string, year: string) =>
    request<ArchiveResponse>(
      `/api/companies/${encodeURIComponent(slug)}/archive/${encodeURIComponent(
        year,
      )}`,
    ).then((r) => r.archive),

  multiYear: (slug: string) =>
    request<MultiYearResponse>(
      `/api/companies/${encodeURIComponent(slug)}/multi-year`,
    ).then((r) => r.multiYear),

  obligations: (slug: string, year?: string) =>
    request<ObligationsResponse>(
      `/api/companies/${encodeURIComponent(slug)}/obligations${
        year ? `?year=${encodeURIComponent(year)}` : ""
      }`,
    ).then((r) => r.obligations),

  cashflow: (slug: string, year?: string) =>
    request<CashflowResponse>(
      `/api/companies/${encodeURIComponent(slug)}/cashflow${
        year ? `?year=${encodeURIComponent(year)}` : ""
      }`,
    ).then((r) => r.cashflow),
};
