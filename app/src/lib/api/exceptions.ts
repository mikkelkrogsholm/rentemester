import type { ExceptionsResponse } from "../types";
import { request } from "./_shared";

// Two definitions of `resolveException` existed in the original `api.ts`:
// the second (later in the literal) shadowed the first. To preserve that
// last-wins semantics deterministically we split them into two consts and
// spread them in the original order in the barrel.

export const exceptionsApiLegacy = {
  /**
   * #332 — Exceptions queue list. Default status er 'open' så cockpittet
   * altid starter på det aktive arbejde.
   */
  exceptions: (slug: string, status?: "open" | "resolved" | "all") => {
    const params = new URLSearchParams();
    if (status) params.set("status", status);
    const qs = params.toString();
    return request<ExceptionsResponse>(
      `/api/companies/${encodeURIComponent(slug)}/exceptions${qs ? `?${qs}` : ""}`,
    ).then((r) => r.exceptions);
  },

  /**
   * POST /api/companies/:slug/exceptions/:id/resolve — closes an open
   * exception. Returns `{ resolved: boolean }`.
   */
  resolveException: (
    slug: string,
    id: number,
    body: { note?: string } = {},
  ) =>
    request<{ ok: true; exception: { id: number; resolved: boolean } }>(
      `/api/companies/${encodeURIComponent(slug)}/exceptions/${id}/resolve`,
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(body),
      },
    ).then((r) => r.exception),
};

export const exceptionsApi = {
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

// --- Agent-forslag → menneskelig godkendelse (#346) ----------------------
//
// The agent loop + the exception sync functions raise `AGENT_*` exceptions
// whenever a deterministic agent run needs a human decision. These API
// methods drive the dedicated cockpit view that lists them, approves them,
// or rejects them. Approve/reject NEVER post on their own — they only
// resolve the underlying exception with a Danish decision note, so the
// audit trail records WHO decided and WHY. The actual ledger action
// (e.g. "Beregn afskrivning", "payable pay") lives on the deep-linked view.

export const agentSuggestionsApi = {
  /** GET /api/companies/:slug/agent-suggestions — the open agent-forslag queue. */
  agentSuggestions: (slug: string) =>
    request<import("../types").AgentSuggestionsResponse>(
      `/api/companies/${encodeURIComponent(slug)}/agent-suggestions`,
    ).then((r) => r.agentSuggestions),

  /** POST .../agent-suggestions/:id/approve — owner accepts the suggestion. */
  approveAgentSuggestion: (slug: string, exceptionId: number, note?: string) =>
    request<import("../types").AgentSuggestionDecisionResponse>(
      `/api/companies/${encodeURIComponent(slug)}/agent-suggestions/${exceptionId}/approve`,
      {
        method: "POST",
        body: JSON.stringify(note ? { note } : {}),
      },
    ).then((r) => r.suggestion),

  /** POST .../agent-suggestions/:id/reject — owner declines the suggestion. */
  rejectAgentSuggestion: (slug: string, exceptionId: number, note?: string) =>
    request<import("../types").AgentSuggestionDecisionResponse>(
      `/api/companies/${encodeURIComponent(slug)}/agent-suggestions/${exceptionId}/reject`,
      {
        method: "POST",
        body: JSON.stringify(note ? { note } : {}),
      },
    ).then((r) => r.suggestion),
};
