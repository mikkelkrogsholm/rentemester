import type { IntegrityResponse } from "../types";
import { request, type ReadRequestOptions } from "./_shared";

export const integrityApi = {
  /**
   * #333 — Audit chain + backup status panel. Idempotent: serveren kører
   * `verifyAuditChain` (read-only) hver gang og returnerer den aktuelle
   * status sammen med backup-compliance og destinations.
   */
  integrity: (slug: string, options?: ReadRequestOptions) =>
    request<IntegrityResponse>(
      `/api/companies/${encodeURIComponent(slug)}/integrity`, options,
    ).then((r) => r.integrity),
};
