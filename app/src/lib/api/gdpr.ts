import type { GdprErasureResult, GdprResponse } from "../types";
import { request } from "./_shared";

export const gdprApi = {
  /**
   * #334 — GDPR-indsigt (read). cvr ELLER name skal sættes.
   */
  gdprExport: (
    slug: string,
    key: { cvr?: string; name?: string; asOf?: string },
  ) => {
    const params = new URLSearchParams();
    if (key.cvr) params.set("cvr", key.cvr);
    if (key.name) params.set("name", key.name);
    if (key.asOf) params.set("asOf", key.asOf);
    return request<GdprResponse>(
      `/api/companies/${encodeURIComponent(slug)}/gdpr/export?${params.toString()}`,
    ).then((r) => r.gdpr);
  },

  /**
   * #334 — GDPR-anonymisering (write). Skriver append-only tombstones.
   */
  gdprErase: (
    slug: string,
    body: { cvr?: string; name?: string; asOf?: string },
  ) =>
    request<{ ok: true; gdprErasure: GdprErasureResult }>(
      `/api/companies/${encodeURIComponent(slug)}/gdpr/erase`,
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(body),
      },
    ).then((r) => r.gdprErasure),
};
