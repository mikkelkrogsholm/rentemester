import type { IntegrityResponse } from "../types";
import { request, type ReadRequestOptions } from "./_shared";

export const integrityApi = {
  /**
   * Current read-only integrity evidence. Inspect availability and scope:
   * a backend may support journal hashes while backup evidence is unavailable.
   */
  integrity: (slug: string, options?: ReadRequestOptions) =>
    request<IntegrityResponse>(
      `/api/companies/${encodeURIComponent(slug)}/integrity`, options,
    ).then((r) => r.integrity),
};
