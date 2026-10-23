import type { RetentionResponse } from "../types";
import { request, type ReadRequestOptions } from "./_shared";

export const retentionApi = {
  complianceReport: (slug: string) => request<{ ok: true; complianceReport: { contentType: "text/html; charset=utf-8"; html: string; sha256: string; data: unknown } }>(
    `/api/companies/${encodeURIComponent(slug)}/compliance-report`,
  ).then(r => r.complianceReport),
  /**
   * #343 — 5-års retention-status pr. data-domæne for én virksomhed.
   */
  retention: (slug: string, options?: ReadRequestOptions) =>
    request<RetentionResponse>(
      `/api/companies/${encodeURIComponent(slug)}/retention`, options,
    ).then((r) => r.retention),
};
