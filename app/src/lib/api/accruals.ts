import type { CompanyAccrualsResponse } from "../types";
import { request } from "./_shared";

export const accrualsApi = {
  /**
   * #337 — Periodiseringsregister (read).
   */
  accruals: (slug: string) =>
    request<CompanyAccrualsResponse>(
      `/api/companies/${encodeURIComponent(slug)}/accruals`,
    ).then((r) => r.accruals),
};
