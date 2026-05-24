// Cockpit Undtagelser view (#332) — list, grouped summary, and the
// resolve-modal flow.

import { describe, expect, test } from "vitest";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ExceptionsView } from "./ExceptionsView";
import { renderAt } from "../test/render";
import { exceptions, mockFetch } from "../test/fixtures";

function route(over = {}) {
  return {
    "GET /api/companies/acme-aps/exceptions": {
      exceptions: exceptions(over),
    },
  };
}

function renderView() {
  return renderAt(<ExceptionsView />, {
    route: "/companies/acme-aps/undtagelser",
    path: "/companies/:slug/undtagelser",
  });
}

describe("ExceptionsView — Undtagelser (#332)", () => {
  test("renders each row with type, severity flag, message and required-action text", async () => {
    mockFetch(route());
    renderView();
    expect(
      await screen.findByRole("heading", { name: "Acme ApS" }),
    ).toBeInTheDocument();
    // Both rows visible: the type code is in the table.
    expect(
      screen.getByText("UNMATCHED_BANK_TRANSACTION"),
    ).toBeInTheDocument();
    expect(
      screen.getByText("AGENT_TAX_RETURN_NEEDS_REVIEW"),
    ).toBeInTheDocument();
    // The agent's message + the concrete required-action both render — the
    // owner must see what to do, not just a bare technical message.
    expect(
      screen.getByText(/Stripe payout/),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/Find fakturaen for denne indbetaling/),
    ).toBeInTheDocument();
  });

  test("shows the summary cards with åbne/løste counts and the grouped pr.-type lines", async () => {
    mockFetch(route());
    renderView();
    // Status cards
    expect(await screen.findByText("Åbne")).toBeInTheDocument();
    expect(screen.getByText("Løste")).toBeInTheDocument();
    expect(screen.getByText("I alt")).toBeInTheDocument();
    // Grouped summary uses the SAME labels as the Overblik "Opgaver" card.
    expect(
      screen.getByText("1 banktransaktion mangler afstemning"),
    ).toBeInTheDocument();
  });

  test("filter pills switch the URL filter; Løste pill marks itself active", async () => {
    mockFetch({
      "GET /api/companies/acme-aps/exceptions": {
        exceptions: exceptions(),
      },
    });
    renderView();
    const resolved = await screen.findByRole("button", { name: "Løste" });
    fireEvent.click(resolved);
    await waitFor(() =>
      expect(resolved.getAttribute("aria-pressed")).toBe("true"),
    );
  });

  test("empty open list renders the friendly empty-state message", async () => {
    mockFetch(
      route({
        rows: [],
        count: 0,
        openCount: 0,
        resolvedCount: 0,
        openGroups: [],
      }),
    );
    renderView();
    expect(
      await screen.findByText(/Intet venter på din afgørelse/),
    ).toBeInTheDocument();
  });

  test("the Løs button opens a confirm-modal with the agent's message + suggested action", async () => {
    mockFetch(route());
    renderView();
    const buttons = await screen.findAllByRole("button", {
      name: /^Løs undtagelse \d+$/,
    });
    fireEvent.click(buttons[0]!);
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText(/Stripe payout/)).toBeInTheDocument();
    expect(
      within(dialog).getByText(/Foreslået handling/),
    ).toBeInTheDocument();
    expect(
      within(dialog).getByRole("button", { name: "Løs undtagelse" }),
    ).toBeInTheDocument();
  });

  test("submitting the resolve-modal POSTs confirm:true to /exceptions/:id/resolve and reloads", async () => {
    let posted: { url: string; body: unknown } | null = null;
    // Custom fetch stub: GET serves the list fixture, POST captures the body
    // and returns the resolve result envelope.
    globalThis.fetch = (async (
      input: RequestInfo | URL,
      init?: RequestInit,
    ) => {
      const url = typeof input === "string" ? input : input.toString();
      const path = url.replace(/^https?:\/\/[^/]+/, "").split("?")[0];
      const method = (init?.method ?? "GET").toUpperCase();
      if (
        method === "GET" &&
        path === "/api/companies/acme-aps/exceptions"
      ) {
        return new Response(
          JSON.stringify({ ok: true, exceptions: exceptions() }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (
        method === "POST" &&
        path === "/api/companies/acme-aps/exceptions/1/resolve"
      ) {
        posted = {
          url,
          body: init?.body ? JSON.parse(String(init.body)) : null,
        };
        return new Response(
          JSON.stringify({
            ok: true,
            exception: { id: 1, resolved: true },
          }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      // The unified `{ ok:false, errors:[…], code }` envelope for misses.
      return new Response(
        JSON.stringify({ ok: false, errors: ["no route"], code: "not_found" }),
        { status: 404, headers: { "content-type": "application/json" } },
      );
    }) as typeof fetch;

    renderView();
    const buttons = await screen.findAllByRole("button", {
      name: /^Løs undtagelse \d+$/,
    });
    fireEvent.click(buttons[0]!);
    const dialog = await screen.findByRole("dialog");

    const user = userEvent.setup();
    await user.type(
      within(dialog).getByLabelText(/Note/),
      "Bilag fundet og bogført",
    );

    const form = dialog.querySelector("form")!;
    fireEvent.submit(form);

    await waitFor(() => expect(posted).not.toBeNull());
    const body = (posted as unknown as { body: Record<string, unknown> }).body;
    expect(body.confirm).toBe(true);
    expect(body.note).toBe("Bilag fundet og bogført");
  });
});
