// Tests for the cockpit app-shell topbar (#421).
//
// The cockpit's top-bar must give the SMB-ejer a synlig vej til hjælp,
// dokumentation og support. Issue #421 dokumenterede at top-baren kun havde
// "Portefølje" og "Tilføj virksomhed" og at en bruger der står fast ikke
// havde nogen klikbar exit-vej. Disse tests låser at hjælp-linket findes og
// at hjælpe-siden indeholder de centrale ressourcer.

import { describe, expect, test } from "vitest";
import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { App } from "./App";

function renderApp(route = "/help") {
  return render(
    <MemoryRouter initialEntries={[route]}>
      <App />
    </MemoryRouter>,
  );
}

describe("App topbar", () => {
  test("topbar contains a help/support link reachable from any route", () => {
    renderApp("/");
    const helpLink = screen.getByRole("link", { name: /^Hjælp$/i });
    expect(helpLink).toBeInTheDocument();
    expect(helpLink.getAttribute("href")).toBe("/help");
  });

  test("the /help route renders a help page with docs, contact and feedback links", () => {
    renderApp("/help");
    // Headline visible
    expect(
      screen.getByRole("heading", { name: /Hjælp og support/i }),
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
