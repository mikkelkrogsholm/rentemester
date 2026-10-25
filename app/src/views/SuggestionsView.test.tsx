import { useState } from "react";
import * as authContext from "../lib/auth-context";
// Tests for the SuggestionsView — the Agent-forslag side of #346.
//
// The view consumes `/api/companies/:slug/agent-suggestions` (read) and posts
// to the approve/reject endpoints. These tests focus on what the OWNER sees
// and on whether the approve/reject one-click flow actually calls the right
// endpoint and reloads the list. The underlying exception-resolution logic is
// covered by the server-side tests; the frontend never re-implements it.

import { describe, expect, test, vi } from "bun:test";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MutationMemoryProvider } from "../lib/mutation-memory";
import { SuggestionsView } from "./SuggestionsView";
import { renderAt } from "../test/render";
import { agentSuggestions, mockFetch } from "../test/fixtures";
import { withProtocol } from "../test/protocol";
import { stubGlobal } from "../test/globals";

const capabilities = [
  ["GET", "/api/companies/{slug}/agent-suggestions"],
  ["POST", "/api/companies/{slug}/agent-suggestions/{id}/approve"],
  ["POST", "/api/companies/{slug}/agent-suggestions/{id}/reject"],
] as const;

function route(over = {}) {
  return {
    "GET /api/companies/acme-aps/agent-suggestions": {
      agentSuggestions: agentSuggestions(over),
    },
  };
}

function installRoute(over = {}) {
  mockFetch(route(over));
  const handler = globalThis.fetch;
  stubGlobal("fetch", withProtocol(handler, capabilities));
}

function renderView() {
  return renderAt(<MutationMemoryProvider><SuggestionsView /></MutationMemoryProvider>, {
    route: "/companies/acme-aps/agent-forslag",
    path: "/companies/:slug/agent-forslag",
  });
}

describe("SuggestionsView — Agent-forslag (#346)", () => {
  test("lists pending agent suggestions with kind, rule and severity", async () => {
    installRoute();
    renderView();
    expect(
      await screen.findByRole("heading", { name: "Forslag", level: 1 }),
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
    installRoute();
    renderView();
    await screen.findByText("Forslag i kø");
    expect(screen.getByText("Forslag i kø")).toBeInTheDocument();
    // "Høj prioritet" also surfaces as a row flag, so use getAllByText.
    expect(screen.getAllByText("Høj prioritet").length).toBeGreaterThan(0);
    expect(screen.getByText("Mellem / Lav")).toBeInTheDocument();
  });

  test("renders the agent's rationale + foreslået handling for each row", async () => {
    installRoute();
    renderView();
    expect(
      await screen.findByText(/V-1001 til Software ApS/),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/Betal kreditorposten/),
    ).toBeInTheDocument();
  });

  test("shows the empty-state when the queue is empty", async () => {
    installRoute({
        rows: [],
        count: 0,
        bySeverity: { high: 0, medium: 0, low: 0 },
      });
    renderView();
    expect(
      await screen.findByText(/ingen åbne forslag/),
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
    stubGlobal("fetch", withProtocol(fetchMock, capabilities));

    renderView();
    const approveBtn = await screen.findByRole("button", {
      name: /Godkend Overforfalden kreditorpost/,
    });
    await userEvent.click(approveBtn);

    // The cockpit now opens a ConfirmDialog instead of using window.confirm.
    const dialog = await screen.findByRole("dialog", {
      name: /Beslut forslag #101/,
    });
    await userEvent.click(
      within(dialog as HTMLElement).getByRole("button", { name: /^Godkend$/ }),
    );

    // The list should reload after the approve completes, so the cleared row
    // disappears and the empty-state shows up.
    expect(
      await screen.findByText(/ingen åbne forslag/),
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
    stubGlobal("fetch", withProtocol(fetchMock, capabilities));

    renderView();
    const rejectBtn = await screen.findByRole("button", {
      name: /Afvis Periodeafgrænsning klar til bogføring/,
    });
    await userEvent.click(rejectBtn);

    // The cockpit now opens a ConfirmDialog with a note textarea instead of
    // using window.prompt.
    const dialog = await screen.findByRole("dialog", {
      name: /Beslut forslag #102/,
    });
    const noteField = within(dialog as HTMLElement).getByRole("textbox");
    await userEvent.type(noteField, "ikke forfalden — bilaget er fejldatoet");
    await userEvent.click(
      within(dialog as HTMLElement).getByRole("button", { name: /Afvis forslag/ }),
    );

    // Wait for the reject POST + the list reload to flush.
    await waitFor(() => expect(rejected).toBe(true));
    expect(rejectBody).toEqual({ note: "ikke forfalden — bilaget er fejldatoet" });
  });
});


test("unknown source remains visible; read and cancel send no decision", async () => {
  const row = { ...agentSuggestions().rows[0]!, exceptionId: 999, type: "AGENT_FUTURE_SYNTHETIC", kindLabel: "Agent-forslag", ruleId: null, agentActor: null, agentProgram: null, link: null, sourceEvidence: "<invalid synthetic evidence>", postingPreview: { synthetic: true } };
  installRoute({ rows: [row], count: 1, bySeverity: { high: 1, medium: 0, low: 0 } });
  const fetchMock = vi.fn(globalThis.fetch);
  stubGlobal("fetch", fetchMock);
  renderView();
  await userEvent.click(await screen.findByRole("button", { name: "Godkend Agent-forslag" }));
  const dialog = await screen.findByRole("dialog", { name: "Beslut forslag #999" });
  expect(within(dialog).getByText("<invalid synthetic evidence>")).toBeInTheDocument();
  expect(within(dialog).getByText(/"synthetic": true/)).toBeInTheDocument();
  expect(within(dialog).getByText(row.rationale)).toBeInTheDocument();
  expect(within(dialog).getByText(/Ingen bogføring/)).toBeInTheDocument();
  await userEvent.click(within(dialog).getByRole("button", { name: "Annullér" }));
  expect(fetchMock.mock.calls.filter(([, init]) => init?.method === "POST")).toHaveLength(0);
});

test("unknown result locks both decisions across read and remount; manual release sends nothing", async () => {
  sessionStorage.clear();
  let posts = 0;
  const handler = vi.fn(async (_input: RequestInfo | URL, init?: RequestInit) => {
    if (init?.method === "POST") { posts++; return Response.json({ ok: true, suggestion: { id: 999, decision: "approved", resolved: true } }); }
    return Response.json({ ok: true, agentSuggestions: agentSuggestions() });
  });
  stubGlobal("fetch", withProtocol(handler, capabilities));
  const view = renderView();
  await userEvent.click(await screen.findByRole("button", { name: /Godkend Overforfalden/ }));
  let dialog = await screen.findByRole("dialog", { name: "Beslut forslag #101" });
  await userEvent.click(within(dialog).getByRole("button", { name: "Godkend" }));
  await screen.findByText(/En ny skrivning er blokeret/);
  expect(posts).toBe(1);
  await userEvent.click(within(dialog).getByRole("button", { name: "Kontrollér status" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  view.unmount();
  renderView();
  await userEvent.click(await screen.findByRole("button", { name: /Afvis Overforfalden/ }));
  dialog = await screen.findByRole("dialog", { name: "Beslut forslag #101" });
  expect(within(dialog).getByRole("button", { name: "Afvis forslag" })).toBeDisabled();
  await userEvent.click(within(dialog).getByRole("button", { name: "Resultatet er afklaret" }));
  await userEvent.click(await screen.findByRole("button", { name: "Jeg har kontrolleret resultatet" }));
  expect(posts).toBe(1);
  expect(within(dialog).getByRole("button", { name: "Afvis forslag" })).not.toBeDisabled();
  sessionStorage.clear();
});

function reviewAuth(role: "reader" | "reviewer"): authContext.AuthState {
  const user = { id: "synthetic-reviewer", email: "reviewer@example.invalid", emailVerified: true, twoFactorEnabled: true };
  return { loading: false, hosted: true, backend: "rust", session: user, currentSessionId: "synthetic-session", context: { user, workspaceRole: "member", companies: [{ slug: "acme-aps", name: "Synthetic", role, archived: false }] }, refresh: async () => {}, clear: () => {} };
}

test("reader can inspect proposals but cannot open a decision", async () => {
  const spy = vi.spyOn(authContext, "useOptionalAuth").mockReturnValue(reviewAuth("reader"));
  try {
    mockFetch(route()); renderView();
    await screen.findByText("Overforfalden kreditorpost");
    expect(screen.queryByRole("button", { name: /Godkend Overforfalden/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Afvis Overforfalden/ })).not.toBeInTheDocument();
    expect(screen.getAllByText(/Kun læseadgang/).length).toBeGreaterThan(0);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  } finally { spy.mockRestore(); }
});

test("authority loss invalidates the open review and hides old queue facts before any POST", async () => {
  let auth = reviewAuth("reviewer");
  const spy = vi.spyOn(authContext, "useOptionalAuth").mockImplementation(() => auth);
  try {
    mockFetch(route()); const original = globalThis.fetch;
    const calls = vi.fn(original); stubGlobal("fetch", calls);
    function Harness() {
      const [, setTick] = useState(0);
      return <><button onClick={() => { auth = { ...auth, context: { ...auth.context!, companies: [] } }; setTick(x => x + 1); }}>Synthetic revoke</button><SuggestionsView /></>;
    }
    renderAt(<Harness />, { route: "/companies/acme-aps/agent-forslag", path: "/companies/:slug/agent-forslag" });
    await userEvent.click(await screen.findByRole("button", { name: /Godkend Overforfalden/ }));
    await screen.findByRole("dialog", { name: "Beslut forslag #101" });
    await userEvent.click(screen.getByRole("button", { name: "Synthetic revoke" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(screen.queryByText("Overforfalden kreditorpost")).not.toBeInTheDocument();
    await screen.findByText(/Din adgang til forslagene kunne ikke bekræftes/);
    expect(calls.mock.calls.filter(([,init]) => init?.method === "POST")).toHaveLength(0);
  } finally { spy.mockRestore(); }
});
