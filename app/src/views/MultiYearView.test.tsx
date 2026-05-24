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

  // --- #452: år-over-år Δ-kolonner -----------------------------------------

  test("resultat-tabellen får Δ (kr) og Δ (%) kolonner i headeren", async () => {
    mockFetch(route());
    renderView();
    await screen.findByRole("heading", { name: "Acme ApS" });
    const table = screen
      .getByRole("heading", { name: /Resultat — omsætning/ })
      .closest(".section")!
      .querySelector("table")!;
    const headers = within(table).getAllByRole("columnheader");
    const labels = headers.map((h) => h.textContent ?? "");
    // Resultat-tabellen viser nu 3 numeriske mål, hver med Δ kr + Δ %.
    expect(labels.filter((l) => /Δ \(kr\)/.test(l)).length).toBeGreaterThanOrEqual(3);
    expect(labels.filter((l) => /Δ \(%\)/.test(l)).length).toBeGreaterThanOrEqual(3);
  });

  test("resultat-tabellen viser normal vækst fra 2023 til 2024 (omsætning +7.000 kr / +22,6 %)", async () => {
    mockFetch(route());
    renderView();
    await screen.findByRole("heading", { name: "Acme ApS" });
    const rows = sectionRows(/Resultat — omsætning/);
    const row2024 = rows.find((r) => r.textContent?.includes("2024"))!;
    // 38000 − 31000 = +7.000 kr (vises som "+7.000,00 kr.")
    expect(within(row2024).getByText(/\+7\.000,00 kr\./)).toBeInTheDocument();
    // 7000 / 31000 ≈ 22,6 % — formatPercent runder til én decimal.
    expect(within(row2024).getByText(/\+22,6 %/)).toBeInTheDocument();
  });

  test("det ældste år viser '—' i Δ-kolonnerne (ingen forrige år at sammenligne med)", async () => {
    mockFetch(route());
    renderView();
    await screen.findByRole("heading", { name: "Acme ApS" });
    const rows = sectionRows(/Resultat — omsætning/);
    const row2023 = rows.find((r) => r.textContent?.includes("2023"))!;
    // 4 numeriske celler + 3 mål × 2 Δ-kolonner = 4 dash-celler (omsætning/udgifter/resultat Δ + 1 fra Δ-pct? nej).
    // Bare verificér at den indeholder bindestreger i Δ-kolonnerne.
    const dashCells = within(row2023)
      .getAllByRole("cell")
      .filter((c) => c.textContent?.trim() === "—");
    expect(dashCells.length).toBeGreaterThanOrEqual(6);
  });

  test("partielt 'år til dato' viser '(ej sammenligneligt — år til dato)' i Δ-kolonnerne", async () => {
    mockFetch(route());
    renderView();
    await screen.findByRole("heading", { name: "Acme ApS" });
    const rows = sectionRows(/Resultat — omsætning/);
    const row2026 = rows.find((r) => r.textContent?.includes("2026"))!;
    expect(
      within(row2026).getAllByText(/ej sammenligneligt — år til dato/i).length,
    ).toBeGreaterThan(0);
  });

  test("når der kun findes ét regnskabsår, vises ingen Δ-kolonner", async () => {
    mockFetch(
      route({
        years: [
          {
            year: "2026",
            source: "live",
            omsaetning: 17829.02,
            udgifter: 4594.2,
            resultat: 13234.82,
            balancesum: 90000,
            egenkapital: 62000,
            bruttomargin: 13234.82 / 17829.02,
            egenkapitalandel: 62000 / 90000,
          },
        ],
      }),
    );
    renderView();
    await screen.findByRole("heading", { name: "Acme ApS" });
    expect(screen.queryByText(/Δ \(kr\)/)).not.toBeInTheDocument();
    expect(screen.queryByText(/Δ \(%\)/)).not.toBeInTheDocument();
    expect(screen.queryByText(/Δ \(pp\)/)).not.toBeInTheDocument();
  });

  test("Δ % håndterer 0-nævner — viser '—' i stedet for ∞/NaN", async () => {
    mockFetch(
      route({
        years: [
          {
            year: "2024",
            source: "archive",
            omsaetning: 0,
            udgifter: 0,
            resultat: 0,
            balancesum: 0,
            egenkapital: 0,
            bruttomargin: null,
            egenkapitalandel: null,
          },
          {
            year: "2025",
            source: "archive",
            omsaetning: 10000,
            udgifter: 4000,
            resultat: 6000,
            balancesum: 20000,
            egenkapital: 15000,
            bruttomargin: 0.6,
            egenkapitalandel: 0.75,
          },
        ],
      }),
    );
    renderView();
    await screen.findByRole("heading", { name: "Acme ApS" });
    const rows = sectionRows(/Resultat — omsætning/);
    const row2025 = rows.find((r) => r.textContent?.includes("2025"))!;
    // Δ kr findes for omsætning (+10.000,00 kr.); men Δ % er '—' fordi forrige år er 0.
    expect(within(row2025).getByText(/\+10\.000,00 kr\./)).toBeInTheDocument();
    // Skal ikke vise NaN/Infinity.
    expect(within(row2025).queryByText(/NaN|Infinity|∞/)).not.toBeInTheDocument();
  });

  test("nøgletal-tabellen viser Δ som procentpoint (pp), ikke procent", async () => {
    mockFetch(route());
    renderView();
    await screen.findByRole("heading", { name: "Acme ApS" });
    const ratioSection = screen
      .getByRole("heading", { name: /Nøgletal pr. regnskabsår/ })
      .closest(".section")!;
    const headers = within(ratioSection).getAllByRole("columnheader");
    const labels = headers.map((h) => h.textContent ?? "");
    expect(labels.filter((l) => /Δ \(pp\)/.test(l)).length).toBeGreaterThanOrEqual(2);
    // 2025-rækkens overskudsgrad: 29500/42000 ≈ 70,2 %, 2024: 27000/38000 ≈ 71,1 %
    // Δ ≈ −0,8 pp.
    const rows = within(ratioSection).getAllByRole("row");
    const row2025 = rows.find((r) => r.textContent?.includes("2025"))!;
    expect(within(row2025).getByText(/-0,8 pp/)).toBeInTheDocument();
  });

  test("balance-tabellen får Δ-kolonner for balancesum og egenkapital", async () => {
    mockFetch(route());
    renderView();
    await screen.findByRole("heading", { name: "Acme ApS" });
    const section = screen
      .getByRole("heading", { name: /Balance — balancesum/ })
      .closest(".section")!;
    const headers = within(section).getAllByRole("columnheader");
    const labels = headers.map((h) => h.textContent ?? "");
    expect(labels.filter((l) => /Δ \(kr\)/.test(l)).length).toBeGreaterThanOrEqual(2);
    expect(labels.filter((l) => /Δ \(%\)/.test(l)).length).toBeGreaterThanOrEqual(2);
  });
});
