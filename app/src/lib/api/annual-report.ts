import type { CompanyAnnualReportResponse } from "../types";
import { request } from "./_shared";

export const annualReportApi = {
  /**
   * #338 — Annual report (regnskabsklasse-B) builder.
   */
  annualReport: (slug: string, fiscalYearStart: string, fiscalYearEnd: string) => {
    const params = new URLSearchParams({ fiscalYearStart, fiscalYearEnd });
    return request<CompanyAnnualReportResponse>(
      `/api/companies/${encodeURIComponent(slug)}/annual-report?${params.toString()}`,
    ).then((r) => r.annualReport);
  },
};
