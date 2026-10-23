import type { GdprErasureResult, GdprResponse } from "../types";
import { request } from "./_shared";

export const gdprApi = {
  gdprForget: (slug: string, body: { cvr?: string; name?: string; afterRetentionExpiry: true; confirm: true }) =>
    request<{ ok: true; gdprErasure: GdprErasureResult }>(`/api/companies/${encodeURIComponent(slug)}/gdpr/forget`, { method: "POST", body: JSON.stringify(body) }).then(r => r.gdprErasure),
  gdprAction: (slug: string, action: "discover" | "legal-hold" | "audit-log", body: { confirm: true; cvr?: string; name?: string; holdId?: string; reason?: string; operation?: "place" | "release"; since?: string; until?: string }) =>
    request<Record<string, unknown>>(`/api/companies/${encodeURIComponent(slug)}/gdpr/${action}`, { method: "POST", body: JSON.stringify(body) }),
  /**
   * #334 — GDPR-indsigt. Eksporten audit-logges og er derfor et confirm-gatet
   * POST; subject-PII placeres aldrig i URL'en.
   */
  gdprExport: (
    slug: string,
    key: { cvr?: string; name?: string; asOf?: string },
  ) =>
    request<GdprResponse>(
      `/api/companies/${encodeURIComponent(slug)}/gdpr/export`,
      {
        method: "POST",
        body: JSON.stringify({ ...key, confirm: true }),
      },
    ).then((r) => r.gdpr),

  /**
   * #334 — GDPR-anonymisering (write). Skriver append-only tombstones.
   */
  gdprErase: (
    slug: string,
    body: { cvr?: string; name?: string },
  ) =>
    request<{ ok: true; gdprErasure: GdprErasureResult }>(
      `/api/companies/${encodeURIComponent(slug)}/gdpr/erase`,
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ ...body, confirm: true }),
      },
    ).then((r) => r.gdprErasure),
};
