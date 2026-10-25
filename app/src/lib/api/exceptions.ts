import type { ExceptionsResponse } from "../types";
import { ApiError, request, type ReadRequestOptions } from "./_shared";

export const exceptionsApi = {
  /**
   * #332 — Exceptions queue list. Default status er 'open' så cockpittet
   * altid starter på det aktive arbejde.
   */
  exceptions: (slug: string, status?: "open" | "resolved" | "all", options?: ReadRequestOptions) => {
    const params = new URLSearchParams();
    if (status) params.set("status", status);
    const qs = params.toString();
    return request<ExceptionsResponse>(
      `/api/companies/${encodeURIComponent(slug)}/exceptions${qs ? `?${qs}` : ""}`, options,
    ).then((r) => r.exceptions);
  },

  /** Resolves an open exception. `note` is optional free text. */
  resolveException: (slug: string, id: number, note?: string) =>
    request<{ ok: true; exception: { id: number; resolved: boolean } }>(
      `/api/companies/${encodeURIComponent(slug)}/exceptions/${id}/resolve`,
      {
        method: "POST",
        body: JSON.stringify(note ? { note } : {}),
      },
    ).then((r) => r.exception),
};

// Review existing AGENT_ exceptions only. This consumer never posts bookkeeping.
async function decide(slug: string, id: number, decision: "approve" | "reject", note?: string) {
  if (!Number.isSafeInteger(id) || id <= 0) throw new ApiError("bad_request", "Ugyldigt forslag-ID.", 400);
  if (note !== undefined && typeof note !== "string") throw new ApiError("bad_request", "Noten skal være tekst.", 400);
  const body = await request<unknown>(
    `/api/companies/${encodeURIComponent(slug)}/agent-suggestions/${id}/${decision}`,
    { method: "POST", body: JSON.stringify(note?.trim() ? { note: note.trim() } : {}) },
  );
  const receipt = body as import("../types").AgentSuggestionDecisionResponse | null;
  const expected = decision === "approve" ? "approved" : "rejected";
  if (receipt?.ok !== true || receipt.suggestion?.id !== id || receipt.suggestion.decision !== expected || receipt.suggestion.resolved !== true || Object.keys(receipt).sort().join(",") !== "ok,suggestion" || Object.keys(receipt.suggestion).sort().join(",") !== "decision,id,resolved") {
    throw new ApiError("internal", "Serverens beslutning kunne ikke bekræftes. Kontrollér status før en ny handling.", 200);
  }
  return receipt.suggestion;
}

export const agentSuggestionsApi = {
  agentSuggestions: (slug: string, options?: ReadRequestOptions) =>
    request<import("../types").AgentSuggestionsResponse>(
      `/api/companies/${encodeURIComponent(slug)}/agent-suggestions`, options,
    ).then((r) => r.agentSuggestions),
  approveAgentSuggestion: (slug: string, id: number, note?: string) => decide(slug, id, "approve", note),
  rejectAgentSuggestion: (slug: string, id: number, note?: string) => decide(slug, id, "reject", note),
};
