import type {
  MileageEntryInput,
  MileageEntrySummary,
  MileageResponse,
} from "../types";
import { request } from "./_shared";

export const mileageApi = {
  /** Mileage register (Kørsel, #335) for the selected fiscal year. */
  mileage: (slug: string, year?: string) =>
    request<MileageResponse>(
      `/api/companies/${encodeURIComponent(slug)}/mileage${
        year ? `?year=${encodeURIComponent(year)}` : ""
      }`,
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
