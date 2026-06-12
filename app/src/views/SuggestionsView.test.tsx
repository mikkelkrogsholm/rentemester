// Tests for the SuggestionsView — the Agent-forslag side of #346.
//
// The view consumes `/api/companies/:slug/agent-suggestions` (read) and posts
// to the approve/reject endpoints. These tests focus on what the OWNER sees
// and on whether the approve/reject one-click flow actually calls the right
// endpoint and reloads the list. The underlying exception-resolution logic is
// covered by the server-side tests; the frontend never re-implements it.

import { describe, expect, test, vi } from "vitest";
import { screen, fireEvent, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { SuggestionsView } from "./SuggestionsView";
import { renderAt } from "../test/render";
import { agentSuggestions, mockFetch } from "../test/fixtures";

function route(over = {}) {
  return {
    "GET /api/companies/acme-aps/agent-suggestions": {
      agentSuggestions: agentSuggestions(over),
    },
  };
}

function renderView() {
  return renderAt(<SuggestionsView />, {
    route: "/companies/acme-aps/agent-forslag",
    path: "/companies/:slug/agent-forslag",
  });
}

describe("SuggestionsView — Agent-forslag (#346)", () => {
  test("lists pending agent suggestions with kind, rule and severity", async () => {
    mockFetch(route());
    renderView();
    expect(
      await screen.findByRole("heading", { name: "Acme ApS" }),
    ).toBeInTheDocument();
    expect(
      screen.getByText("Overforfalden kreditorpost"),
    ).toBeInTheDocument();
    expect(
      screen.getByText("Periodeafgrænsning klar til bogføring"),
    ).toBeInTheDocument();
    expect(screen.getByText(/DK-PAYABLE-001/)).toBeInTheDocument();
    // "Høj prioritet" appears both as a summary-card heading and as the row
    // severity flag — at least one must render.
    expect(screen.getAllByText(/Høj prioritet/).length).toBeGreaterThan(0);
    expect(screen.getByText(/Mellem prioritet/)).toBeInTheDocument();
  });

  test("shows summary cards (kø / høj / mellem+lav)", async () => {
    mockFetch(route());
    renderView();
    await screen.findByText("Forslag i kø");
    expect(screen.getByText("Forslag i kø")).toBeInTheDocument();
    // "Høj prioritet" also surfaces as a row flag, so use getAllByText.
    expect(screen.getAllByText("Høj prioritet").length).toBeGreaterThan(0);
    expect(screen.getByText("Mellem / Lav")).toBeInTheDocument();
  });

  test("renders the agent's rationale + foreslået handling for each row", async () => {
    mockFetch(route());
    renderView();
    expect(
      await screen.findByText(/V-1001 til Software ApS/),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/Betal kreditorposten/),
    ).toBeInTheDocument();
  });

  test("shows the empty-state when the queue is empty", async () => {
    mockFetch(
      route({
        rows: [],
        count: 0,
        bySeverity: { high: 0, medium: 0, low: 0 },
      }),
    );
    renderView();
    expect(
      await screen.findByText(/Agenten har ingen åbne forslag/),
    ).toBeInTheDocument();
  });

  test("'Godkend' calls the approve endpoint and reloads the list", async () => {
    let approved = false;
    let listCalls = 0;
    const fetchMock = vi.fn(
      async (input: RequestInfo | URL, init?: RequestInit) => {
        const url = typeof input === "string" ? input : input.toString();
        const path = url.replace(/^https?:\/\/[^/]+/, "").split("?")[0];
        const method = (init?.method ?? "GET").toUpperCase();
        if (
          method === "GET" &&
          path === "/api/companies/acme-aps/agent-suggestions"
        ) {
          listCalls += 1;
          // After approval the row disappears from the queue.
          const rows = approved ? [] : agentSuggestions().rows;
          return new Response(
            JSON.stringify({
              ok: true,
              agentSuggestions: {
                ...agentSuggestions(),
                rows,
                count: rows.length,
                bySeverity: { high: 0, medium: 0, low: 0 },
              },
            }),
            { status: 200, headers: { "content-type": "application/json" } },
          );
        }
        if (
          method === "POST" &&
          path === "/api/companies/acme-aps/agent-suggestions/101/approve"
        ) {
          approved = true;
          return new Response(
            JSON.stringify({
              ok: true,
              suggestion: {
                id: 101,
                decision: "approved",
                resolved: true,
              },
            }),
            { status: 200, headers: { "content-type": "application/json" } },
          );
        }
        return new Response(
          JSON.stringify({ ok: false, errors: ["no route"], code: "not_found" }),
          { status: 404, headers: { "content-type": "application/json" } },
        );
      },
    );
    vi.stubGlobal("fetch", fetchMock);

    renderView();
    const approveBtn = await screen.findByRole("button", {
      name: /Godkend Overforfalden kreditorpost/,
    });
    fireEvent.click(approveBtn);

    // The cockpit now opens a ConfirmDialog instead of using window.confirm.
    const dialog = await screen.findByRole("dialog", {
      name: /Godkend forslag: Overforfalden kreditorpost/,
    });
    fireEvent.click(
      within(dialog as HTMLElement).getByRole("button", { name: /^Godkend$/ }),
    );

    // The list should reload after the approve completes, so the cleared row
    // disappears and the empty-state shows up.
    expect(
      await screen.findByText(/Agenten har ingen åbne forslag/),
    ).toBeInTheDocument();
    // GET was called twice (initial + reload after approve).
    expect(listCalls).toBeGreaterThanOrEqual(2);
    // Approve POST was hit exactly once.
    const approvePosts = fetchMock.mock.calls.filter((c) => {
      const url = typeof c[0] === "string" ? c[0] : (c[0] as URL).toString();
      return url.includes("/agent-suggestions/101/approve");
    });
    expect(approvePosts.length).toBe(1);
  });

  test("'Afvis' calls the reject endpoint with the owner's reason", async () => {
    let rejected = false;
    let rejectBody: unknown = null;
    const fetchMock = vi.fn(
      async (input: RequestInfo | URL, init?: RequestInit) => {
        const url = typeof input === "string" ? input : input.toString();
        const path = url.replace(/^https?:\/\/[^/]+/, "").split("?")[0];
        const method = (init?.method ?? "GET").toUpperCase();
        if (
          method === "GET" &&
          path === "/api/companies/acme-aps/agent-suggestions"
        ) {
          const rows = rejected
            ? agentSuggestions().rows.filter((r) => r.exceptionId !== 102)
            : agentSuggestions().rows;
          return new Response(
            JSON.stringify({
              ok: true,
              agentSuggestions: {
                ...agentSuggestions(),
                rows,
                count: rows.length,
              },
            }),
            { status: 200, headers: { "content-type": "application/json" } },
          );
        }
        if (
          method === "POST" &&
          path === "/api/companies/acme-aps/agent-suggestions/102/reject"
        ) {
          rejected = true;
          rejectBody = init?.body ? JSON.parse(String(init.body)) : null;
          return new Response(
            JSON.stringify({
              ok: true,
              suggestion: {
                id: 102,
                decision: "rejected",
                resolved: true,
              },
            }),
            { status: 200, headers: { "content-type": "application/json" } },
          );
        }
        return new Response(
          JSON.stringify({ ok: false, errors: ["no route"], code: "not_found" }),
          { status: 404, headers: { "content-type": "application/json" } },
        );
      },
    );
    vi.stubGlobal("fetch", fetchMock);

    renderView();
    const rejectBtn = await screen.findByRole("button", {
      name: /Afvis Periodeafgrænsning klar til bogføring/,
    });
    fireEvent.click(rejectBtn);

    // The cockpit now opens a ConfirmDialog with a note textarea instead of
    // using window.prompt.
    const dialog = await screen.findByRole("dialog", {
      name: /Afvis forslag: Periodeafgrænsning klar til bogføring/,
    });
    const noteField = within(dialog as HTMLElement).getByRole("textbox");
    await userEvent.type(noteField, "ikke forfalden — bilaget er fejldatoet");
    fireEvent.click(
      within(dialog as HTMLElement).getByRole("button", { name: /Afvis forslag/ }),
    );

    // Wait for the reject POST + the list reload to flush.
    await vi.waitFor(() => expect(rejected).toBe(true));
    expect(rejectBody).toEqual({ note: "ikke forfalden — bilaget er fejldatoet" });
  });
});
