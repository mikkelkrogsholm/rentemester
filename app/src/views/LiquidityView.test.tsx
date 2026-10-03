import { describe, expect, test, vi } from "bun:test";
import { screen } from "@testing-library/react";
import { LiquidityView } from "./LiquidityView";
import { renderAt } from "../test/render";
import { cashflow, mockFetch } from "../test/fixtures";

// The Chart.js graph needs a real <canvas>; stub it so the view tests stay
// fast and DOM-only (the same pattern DashboardView's test uses for PnlChart).
vi.mock("../components/CashflowChart", () => ({
  CashflowChart: () => <div data-testid="cashflow-chart" />,
}));

function route(over = {}) {
  return {
    "GET /api/companies/acme-aps/cashflow": {
      cashflow: cashflow(over),
    },
  };
}

function renderView() {
  return renderAt(<LiquidityView />, {
    route: "/companies/acme-aps/likviditet",
    path: "/companies/:slug/likviditet",
  });
}

describe("LiquidityView — Likviditet", () => {
  test("shows the primo/ind/ud/ultimo summary strip", async () => {
    mockFetch(route());
    renderView();
    expect(
      await screen.findByRole("heading", { name: "Likviditet", level: 1 }),
    ).toBeInTheDocument();
    expect(screen.getByText("Primo-saldo")).toBeInTheDocument();
    expect(screen.getByText("Ultimo-saldo")).toBeInTheDocument();
    // "Indbetalinger" / "Udbetalinger" label both a summary card and a table
    // column header — both appear, hence getAllByText.
    expect(screen.getAllByText("Indbetalinger").length).toBeGreaterThan(0);
    expect(screen.getAllByText("Udbetalinger").length).toBeGreaterThan(0);
  });

  test("renders the cash-flow chart and the monthly table", async () => {
    mockFetch(route());
    renderView();
    expect(
      await screen.findByTestId("cashflow-chart"),
    ).toBeInTheDocument();
    // The totals row carries the year's money in and out.
    expect(screen.getAllByText(/17\.829,02/).length).toBeGreaterThan(0);
    expect(screen.getAllByText(/4\.594,20/).length).toBeGreaterThan(0);
  });

  test("a company with no bank transactions shows the empty state", async () => {
    mockFetch(
      route({
        hasTransactions: false,
        months: cashflow().months.map((m) => ({
          ...m,
          indbetalinger: 0,
          udbetalinger: 0,
          netto: 0,
        })),
        balanceSeries: [],
        openingBalance: null,
        closingBalance: null,
        totalIn: 0,
        totalOut: 0,
      }),
    );
    renderView();
    expect(
      await screen.findByText(/Ingen pengestrøm/),
    ).toBeInTheDocument();
  });

  test("an archived year shows an honest 'not available' state", async () => {
    mockFetch(route({ archived: true, selectedYear: "2025" }));
    renderView();
    expect(
      await screen.findByText(/Likviditet er ikke tilgængelig for 2025/),
    ).toBeInTheDocument();
  });
});
