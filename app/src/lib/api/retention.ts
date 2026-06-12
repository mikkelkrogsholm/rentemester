import type { RetentionResponse } from "../types";
import { request } from "./_shared";

export const retentionApi = {
  /**
   * #343 — 5-års retention-status pr. data-domæne for én virksomhed.
   */
  retention: (slug: string) =>
    request<RetentionResponse>(
      `/api/companies/${encodeURIComponent(slug)}/retention`,
    ).then((r) => r.retention),
};
