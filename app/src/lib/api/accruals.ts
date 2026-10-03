import type { CompanyAccrualsResponse } from "../types";
import { request, type ReadRequestOptions } from "./_shared";

export const accrualsApi = {
  /**
   * #337 — Periodiseringsregister (read).
   */
  accruals: (slug: string, options?: ReadRequestOptions) =>
    request<CompanyAccrualsResponse>(
      `/api/companies/${encodeURIComponent(slug)}/accruals`, options,
    ).then((r) => r.accruals),
};
