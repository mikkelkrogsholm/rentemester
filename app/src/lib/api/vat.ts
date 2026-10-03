import type { VatResponse } from "../types";
import { request, type ReadRequestOptions } from "./_shared";

export const vatApi = {
  vat: (slug: string, year?: string, options?: ReadRequestOptions) =>
    request<VatResponse>(
      `/api/companies/${encodeURIComponent(slug)}/vat${
        year ? `?year=${encodeURIComponent(year)}` : ""
      }`, options,
    ).then((r) => r.vat),

  /**
   * Moms-rapport som PDF-download (#464). Indeholder SKAT-rubrikker
   * (salgsmoms, købsmoms, momstilsvar, A/B/C), periode-bounds og
   * betalingsfristen — én printbar arbejdsudskrift.
   */
  vatPdfUrl: (slug: string, year?: string) => {
    const params = new URLSearchParams({ format: "pdf" });
    if (year) params.set("year", year);
    return `/api/companies/${encodeURIComponent(slug)}/vat/export?${params.toString()}`;
  },
};
