import type { RetentionResponse } from "../types";
import { request, type ReadRequestOptions } from "./_shared";

export const retentionApi = {
  /**
   * #343 — 5-års retention-status pr. data-domæne for én virksomhed.
   */
  retention: (slug: string, options?: ReadRequestOptions) =>
    request<RetentionResponse>(
      `/api/companies/${encodeURIComponent(slug)}/retention`, options,
    ).then((r) => r.retention),
};
