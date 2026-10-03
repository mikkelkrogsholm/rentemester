import type { CompanyAnnualReportResponse } from "../types";
import { request, type ReadRequestOptions } from "./_shared";

export const annualReportApi = {
  /**
   * #338 — Annual report (regnskabsklasse-B) builder.
   */
  annualReport: (slug: string, fiscalYearStart: string, fiscalYearEnd: string, options?: ReadRequestOptions) => {
    const params = new URLSearchParams({ fiscalYearStart, fiscalYearEnd });
    return request<CompanyAnnualReportResponse>(
      `/api/companies/${encodeURIComponent(slug)}/annual-report?${params.toString()}`, options,
    ).then((r) => r.annualReport);
  },
};
