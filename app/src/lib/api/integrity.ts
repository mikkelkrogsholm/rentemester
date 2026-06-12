import type { IntegrityResponse } from "../types";
import { request } from "./_shared";

export const integrityApi = {
  /**
   * #333 — Audit chain + backup status panel. Idempotent: serveren kører
   * `verifyAuditChain` (read-only) hver gang og returnerer den aktuelle
   * status sammen med backup-compliance og destinations.
   */
  integrity: (slug: string) =>
    request<IntegrityResponse>(
      `/api/companies/${encodeURIComponent(slug)}/integrity`,
    ).then((r) => r.integrity),
};
