import { describe, expect, test } from "vitest";
import { screen, within } from "@testing-library/react";
import { MultiYearView } from "./MultiYearView";
import { renderAt } from "../test/render";
import { multiYear, mockFetch } from "../test/fixtures";

function route(over = {}) {
  return {
    "GET /api/companies/acme-aps/multi-year": { multiYear: multiYear(over) },
  };
}

function renderView() {
  return renderAt(<MultiYearView />, {
    route: "/companies/acme-aps/fleraar",
    path: "/companies/:slug/fleraar",
  });
}

/** The <tbody> rows of the section whose heading matches `name`. */
function sectionRows(name: RegExp) {
  const table = screen
    .getByRole("heading", { name })
    .closest(".section")!
    .querySelector("table")!;
  return within(table).getAllByRole("row");
}

describe("MultiYearView — Flerårsoversigt", () => {
  test("renders a P&L row per fiscal year", async () => {
    mockFetch(route());
    renderView();
    await screen.findByRole("heading", { name: "Acme ApS" });
    const rows = sectionRows(/Resultat — omsætning/);
    // Header row + one per year (archive 2023–25, live 2026).
    expect(rows).toHaveLength(5);
    for (const year of ["2023", "2024", "2025", "2026"]) {
      expect(rows.some((r) => r.textContent?.includes(year))).toBe(true);
    }
  });

  test("marks archived years with an arkiv tag", async () => {
    mockFetch(route());
    renderView();
    await screen.findByRole("heading", { name: "Acme ApS" });
    const rows = sectionRows(/Resultat — omsætning/);
    const row2025 = rows.find((r) => r.textContent?.includes("2025"))!;
    expect(within(row2025).getByText("arkiv")).toBeInTheDocument();
  });

  test("shows the omsætning / udgifter / resultat columns", async () => {
    mockFetch(route());
    renderView();
    expect(await screen.findByText("Omsætning")).toBeInTheDocument();
    expect(screen.getByText("Udgifter")).toBeInTheDocument();
    expect(screen.getAllByText("Resultat").length).toBeGreaterThan(0);
  });

  test("shows the balance-sheet development per year", async () => {
    mockFetch(route());
    renderView();
    await screen.findByRole("heading", { name: "Acme ApS" });
    expect(
      screen.getByRole("heading", { name: /Balance — balancesum/ }),
    ).toBeInTheDocument();
    const rows = sectionRows(/Balance — balancesum/);
    expect(rows).toHaveLength(5);
    expect(screen.getByText("Balancesum")).toBeInTheDocument();
    expect(screen.getByText("Egenkapital")).toBeInTheDocument();
  });

  test("shows key ratios per year as percentages", async () => {
    mockFetch(route());
    renderView();
    await screen.findByRole("heading", { name: "Acme ApS" });
    const rows = sectionRows(/Nøgletal pr. regnskabsår/);
    expect(rows).toHaveLength(5);
    expect(screen.getByText("Overskudsgrad")).toBeInTheDocument();
    expect(screen.queryByText("Bruttomargin")).not.toBeInTheDocument();
    expect(
      screen.getByText(/Overskudsgrad er resultat ÷ omsætning/),
    ).toBeInTheDocument();
    expect(screen.getByText("Egenkapitalandel")).toBeInTheDocument();
    // 2023: bruttomargin 22000/31000 ≈ 71 %.
    const row2023 = rows.find((r) => r.textContent?.includes("2023"))!;
    expect(within(row2023).getByText(/71/)).toBeInTheDocument();
  });

  test("renders the P&L and balance trend charts", async () => {
    mockFetch(route());
    const { container } = renderView();
    await screen.findByRole("heading", { name: "Acme ApS" });
    expect(container.querySelectorAll(".pnl-chart canvas")).toHaveLength(2);
  });

  test("marks the live/current year as '(år til dato)'", async () => {
    mockFetch(route());
    renderView();
    await screen.findByRole("heading", { name: "Acme ApS" });
    const rows = sectionRows(/Resultat — omsætning/);
    const row2026 = rows.find((r) => r.textContent?.includes("2026"))!;
    expect(within(row2026).getByText("(år til dato)")).toBeInTheDocument();
    const row2025 = rows.find((r) => r.textContent?.includes("2025"))!;
    expect(
      within(row2025).queryByText("(år til dato)"),
    ).not.toBeInTheDocument();
  });

  test("an empty company shows the no-years state", async () => {
    mockFetch(route({ years: [] }));
    renderView();
    expect(await screen.findByText(/Ingen regnskabsår/)).toBeInTheDocument();
  });
});
