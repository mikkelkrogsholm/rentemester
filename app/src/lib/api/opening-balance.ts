import { ApiError, request, type ReadRequestOptions } from "./_shared";
import type { OpeningApply, OpeningInput, OpeningPlan, OpeningReceipt, OpeningState } from "../types/opening-balance";
const base = (slug: string) => `/api/companies/${encodeURIComponent(slug)}/opening-balance`;
function state(value: OpeningState) {
  if (value?.ok !== true || !Array.isArray(value.errors) || value.errors.length || !("openingBalance" in value)) {
    throw new ApiError("invalid_opening_state", "Åbningsbalancens aktuelle tilstand kunne ikke bekræftes.", 200);
  }
  return value;
}
export const openingBalanceApi = {
  read: (slug: string, options?: ReadRequestOptions) => request<OpeningState>(base(slug), options).then(state),
  plan: (slug: string, input: OpeningInput, options?: ReadRequestOptions) =>
    request<{ ok: true; plan: OpeningPlan; errors: string[] }>(`${base(slug)}/plan`, { method: "POST", readOnly: true, signal: options?.signal, body: JSON.stringify(input) }).then(value => {
      if (value?.ok !== true || !Array.isArray(value.errors) || value.errors.length || !value.plan) throw new ApiError("invalid_opening_plan", "Serverens gennemgang kunne ikke bekræftes.", 200);
      return value.plan;
    }),
  apply: (slug: string, body: OpeningApply) => request<OpeningReceipt>(`${base(slug)}/apply`, { method: "POST", body: JSON.stringify(body) }).then(value => { state(value); return value; }),
};
