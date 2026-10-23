import type { MileageRateInput, MileageRateVersion, MileagePostPreviewInput, MileagePostPreview, MileagePostInput, MileagePostResult } from "../types/mileage";
import type {
  MileageEntryInput,
  MileageEntrySummary,
  MileageResponse,
} from "../types";
import { request, type ReadRequestOptions } from "./_shared";

const postingPath = (slug: string, action: string) => `/api/companies/${encodeURIComponent(slug)}/mileage/${action}`;
type PostingEnvelope<T> = { ok: true; mileagePosting: T };

export const mileageApi = {
  mileageRateVersions: (slug: string, options?: ReadRequestOptions) =>
    request<PostingEnvelope<{ rateVersions: MileageRateVersion[] }>>(postingPath(slug, "rate-versions"), options).then(r => r.mileagePosting),
  configureMileageRates: (slug: string, input: MileageRateInput) =>
    request<PostingEnvelope<{ rateVersion: MileageRateVersion; replayed: boolean }>>(postingPath(slug, "rate-versions"), { method: "POST", body: JSON.stringify({ ...input, confirm: true }) }).then(r => r.mileagePosting),
  previewMileagePost: (slug: string, input: MileagePostPreviewInput, options?: ReadRequestOptions) =>
    request<PostingEnvelope<MileagePostPreview>>(postingPath(slug, "post-preview"), { ...options, method: "POST", readOnly: true, body: JSON.stringify(input) }).then(r => r.mileagePosting),
  postMileage: (slug: string, input: MileagePostInput) =>
    request<PostingEnvelope<MileagePostResult>>(postingPath(slug, "post"), { method: "POST", body: JSON.stringify(input) }).then(r => r.mileagePosting),
  mileagePostings: (slug: string, options?: ReadRequestOptions) =>
    request<PostingEnvelope<{ postings: MileagePostResult[] }>>(postingPath(slug, "postings"), options).then(r => r.mileagePosting),
  /** Mileage register (Kørsel, #335) for the selected fiscal year. */
  mileage: (slug: string, year?: string, options?: ReadRequestOptions) =>
    request<MileageResponse>(
      `/api/companies/${encodeURIComponent(slug)}/mileage${
        year ? `?year=${encodeURIComponent(year)}` : ""
      }`, options,
    ).then((r) => r.mileage),

  /**
   * Registers a single mileage entry (Kørsel, #335). Calls the same
   * `createMileageEntry` core function the CLI's `mileage add` and the MCP
   * tool use. The mileage register is append-only audit data, so the body
   * carries `confirm: true`.
   */
  createMileageEntry: (slug: string, input: MileageEntryInput) =>
    request<{ ok: true; mileage: MileageEntrySummary }>(
      `/api/companies/${encodeURIComponent(slug)}/mileage`,
      {
        method: "POST",
        body: JSON.stringify({
          ...input,
          ...(input.rateSource ? { rateSource: input.rateSource } : {}),
          ...(input.notes ? { notes: input.notes } : {}),
          confirm: true,
        }),
      },
    ).then((r) => r.mileage),
};
