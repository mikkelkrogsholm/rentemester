// Tests for the cockpit app-shell topbar (#421).
//
// The cockpit's top-bar must give the SMB-ejer a synlig vej til hjælp,
// dokumentation og support. Issue #421 dokumenterede at top-baren kun havde
// "Portefølje" og "Tilføj virksomhed" og at en bruger der står fast ikke
// havde nogen klikbar exit-vej. Disse tests låser at hjælp-linket findes og
// at hjælpe-siden indeholder de centrale ressourcer.

import { describe, expect, test } from "bun:test";
import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { App } from "./App";
import { mockFetch } from "./test/fixtures";

function renderApp(route = "/help") {
  return render(
    <MemoryRouter initialEntries={[route]}>
      <App />
    </MemoryRouter>,
  );
}

describe("App topbar", () => {
  test("renders the approved daily navigation through the shared registry", async () => {
    const routes = [
      ["Status", "/companies/acme-aps"], ["Kræver opmærksomhed", "/companies/acme-aps/opmaerksomhed"],
      ["Penge og bilag", "/companies/acme-aps/bank"], ["Fakturaer", "/companies/acme-aps/fakturaer"],
      ["Moms og frister", "/companies/acme-aps/moms"], ["Rapporter", "/companies/acme-aps/resultatopgorelse"],
      ["Viden", "/companies/acme-aps/parter"], ["Administration", "/companies/acme-aps/manage"],
    ] as const;

    for (const [area, route] of routes) {
      const rendered = renderApp(route);
      expect(await screen.findByRole("link", { name: area })).toBeInTheDocument();
      rendered.unmount();
    }
  });

  test("topbar contains a help/support link reachable from any route", async () => {
    mockFetch({
      "GET /api/companies": { workspace: "/ws", count: 0, companies: [] },
      "GET /api/portfolio": {
        portfolio: {
          workspace: "/ws", asOf: "2026-05-20", companyCount: 0,
          rollup: { resultat: 0, liquidity: 0, vatPayable: 0, openTaskCount: 0 },
          totals: {}, companies: [],
        },
      },
    });
    renderApp("/");
    await screen.findByRole("form", { name: /Opret virksomhed/i });
    expect(screen.getByText("v0.2.0")).toBeInTheDocument();
    const helpLink = screen.getByRole("link", { name: /^Hjælp$/i });
    expect(helpLink).toBeInTheDocument();
    expect(helpLink.getAttribute("href")).toBe("/help");
  });

  test("the /help route renders a help page with docs, contact and feedback links", async () => {
    renderApp("/help");
    // Headline visible
    expect(
      await screen.findByRole("heading", { name: /Hjælp og support/i }),
    ).toBeInTheDocument();
    // Link til docs / sådan virker det
    expect(
      screen.getByRole("link", { name: /Sådan virker det/i }),
    ).toBeInTheDocument();
    // Link til kontakt
    expect(screen.getByRole("link", { name: /Kontakt/i })).toBeInTheDocument();
    // Link til GitHub-issues for fejlrapport
    expect(
      screen.getByRole("link", { name: /Rapportér en fejl|GitHub/i }),
    ).toBeInTheDocument();
    // Kom-i-gang tjekliste (oprettelse → bank → bogføring → moms)
    expect(screen.getByText(/Kom i gang/i)).toBeInTheDocument();
  });
});
