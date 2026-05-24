// Tests for the AssetsView — the Anlægskartotek-side of #336.
//
// The view consumes `/api/companies/:slug/assets` (read) plus a write
// endpoint per action. Tests focus on what the OWNER sees and on whether the
// one-click depreciation flow posts. Depreciation arithmetic itself is
// covered by the server-side tests; the frontend never re-implements it.

import { describe, expect, test, vi } from "vitest";
import { screen, within, fireEvent } from "@testing-library/react";
import { AssetsView } from "./AssetsView";
import { renderAt } from "../test/render";
import { assets, documents, mockFetch } from "../test/fixtures";

function route(over = {}) {
  return {
    "GET /api/companies/acme-aps/assets": {
      assets: assets(over),
    },
    "GET /api/companies/acme-aps/documents": {
      documents: documents(),
    },
  };
}

function renderView() {
  return renderAt(<AssetsView />, {
    route: "/companies/acme-aps/anlaeg",
    path: "/companies/:slug/anlaeg",
  });
}

describe("AssetsView — Anlægskartotek (#336)", () => {
  test("lists capitalised assets with cost, accumulated depreciation and net book value", async () => {
    mockFetch(route());
    renderView();
    expect(
      await screen.findByRole("heading", { name: "Acme ApS" }),
    ).toBeInTheDocument();
    expect(screen.getByText("MacBook Pro")).toBeInTheDocument();
    expect(screen.getByText("Server rack")).toBeInTheDocument();
    // The "6/36 afskrevet" badge is the cockpit's read of postedPeriods.
    expect(screen.getByText(/6\/36 afskrevet/)).toBeInTheDocument();
    expect(screen.getByText(/Fuldt afskrevet/)).toBeInTheDocument();
  });

  test("shows the summary totals (kostpris, restværdi, straksafskrivninger)", async () => {
    mockFetch(route());
    renderView();
    await screen.findByText("Bogført kostpris");
    expect(screen.getByText("Bogført kostpris")).toBeInTheDocument();
    expect(screen.getByText("Restværdi (netto)")).toBeInTheDocument();
    // "Straksafskrivninger" labels both the summary card and the section
    // header — at least one of them must render.
    expect(screen.getAllByText("Straksafskrivninger").length).toBeGreaterThan(
      0,
    );
    // Kostpris-summen: 72.000 viser samme tal i kort og i totalrækken.
    expect(screen.getAllByText(/72\.000,00/).length).toBeGreaterThan(0);
  });

  test("lists straksafskrivninger with the threshold rule reference", async () => {
    mockFetch(route());
    renderView();
    expect(await screen.findByText("Tastatur")).toBeInTheDocument();
    expect(
      screen.getByText(/AL §6 stk\. 1 nr\. 2/),
    ).toBeInTheDocument();
  });

  test("a fresh ledger with no assets shows the empty state", async () => {
    mockFetch(
      route({
        assets: [],
        writeOffs: [],
        totals: {
          cost: 0,
          accumulatedDepreciation: 0,
          netBookValue: 0,
          activeCount: 0,
          fullyDepreciatedCount: 0,
          writeOffCount: 0,
          writeOffTotal: 0,
        },
      }),
    );
    renderView();
    expect(
      await screen.findByText(/Der er ingen kapitaliserede anlæg/),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/Ingen straksafskrivninger endnu/),
    ).toBeInTheDocument();
  });

  test("'Beregn afskrivning' calls the depreciate endpoint and reloads the list", async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input.toString();
      const path = url.replace(/^https?:\/\/[^/]+/, "").split("?")[0];
      const method = (init?.method ?? "GET").toUpperCase();
      if (
        method === "GET" &&
        path === "/api/companies/acme-aps/assets"
      ) {
        return new Response(
          JSON.stringify({ ok: true, assets: assets() }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (
        method === "POST" &&
        path === "/api/companies/acme-aps/assets/1/depreciate"
      ) {
        return new Response(
          JSON.stringify({
            ok: true,
            depreciation: {
              entryId: 1,
              assetId: 1,
              periodIndex: 7,
              periodAmount: 1333.33,
            },
          }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      return new Response(JSON.stringify({ ok: true }), {
        status: 200,
        headers: { "content-type": "application/json" },
      });
    });
    vi.stubGlobal("fetch", fetchMock);
    vi.spyOn(window, "confirm").mockReturnValue(true);

    renderView();
    const row = (await screen.findByText("MacBook Pro")).closest("tr")!;
    const button = within(row as HTMLElement).getByRole("button", {
      name: /Beregn afskrivning for MacBook Pro/,
    });
    fireEvent.click(button);

    await vi.waitFor(() => {
      const calls = fetchMock.mock.calls;
      const posted = calls.some(([url, init]) => {
        const u = typeof url === "string" ? url : url.toString();
        const m = (init?.method ?? "GET").toUpperCase();
        return (
          m === "POST" &&
          u.endsWith("/api/companies/acme-aps/assets/1/depreciate")
        );
      });
      expect(posted).toBe(true);
    });
  });

  test("'Beregn afskrivning' is disabled for a fully depreciated asset", async () => {
    mockFetch(route());
    renderView();
    const row = (await screen.findByText("Server rack")).closest("tr")!;
    const button = within(row as HTMLElement).getByRole("button", {
      name: /Beregn afskrivning/,
    });
    expect(button).toBeDisabled();
  });

  test("'Registrér anlæg' opens the modal with name + bilag picker", async () => {
    mockFetch(route());
    renderView();
    const open = await screen.findByRole("button", {
      name: "Registrér anlæg",
    });
    fireEvent.click(open);
    expect(
      await screen.findByRole("heading", { name: "Registrér nyt anlæg" }),
    ).toBeInTheDocument();
    // The dropdown reads from /documents so the bilag is selectable.
    expect(screen.getByText(/DOC-2026-000001/)).toBeInTheDocument();
  });

  test("'Straksafskriv' opens the straksafskrivnings-modal with hjemmel-felt", async () => {
    mockFetch(route());
    renderView();
    const open = await screen.findByRole("button", { name: "Straksafskriv" });
    fireEvent.click(open);
    expect(
      await screen.findByRole("heading", {
        name: "Straksafskriv småanskaffelse",
      }),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/Hjemmelshenvisning \(tærskelregel\)/),
    ).toBeInTheDocument();
  });
});
