// Tests for app/src/views/BudgetView.tsx (#339).
//
// The view talks to two read endpoints and one write endpoint:
//   - GET /api/companies/:slug/budget?year=         (input grid)
//   - GET /api/companies/:slug/budget-vs-actual?... (comparison)
//   - POST /api/companies/:slug/budget              (save a cell)
//
// `mockFetch` speaks the cockpit JSON envelope and matches by `METHOD path`
// (query stripped) — see test/fixtures.ts.

import { describe, expect, test } from "vitest";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { BudgetView } from "./BudgetView";
import { renderAt } from "../test/render";
import { budget, budgetVsActual, mockFetch } from "../test/fixtures";

function route(over: {
  budget?: Parameters<typeof budget>[0];
  budgetVsActual?: Parameters<typeof budgetVsActual>[0];
  postResult?: unknown;
} = {}) {
  return {
    "GET /api/companies/acme-aps/budget": { budget: budget(over.budget) },
    "GET /api/companies/acme-aps/budget-vs-actual": {
      budgetVsActual: budgetVsActual(over.budgetVsActual),
    },
    "POST /api/companies/acme-aps/budget": over.postResult ?? {
      budget: {
        id: 99,
        accountNo: "2200",
        period: "2026-06",
        amount: 5500,
      },
    },
  };
}

function renderView() {
  return renderAt(<BudgetView />, {
    route: "/companies/acme-aps/budget",
    path: "/companies/:slug/budget",
  });
}

describe("BudgetView — Budget", () => {
  test("renders the page header and total budget summary", async () => {
    mockFetch(route());
    renderView();
    expect(
      await screen.findByRole("heading", { name: "Acme ApS" }),
    ).toBeInTheDocument();
    // The two budget lines from the fixture sum to 12.000 kr.
    expect(screen.getByText(/Samlet budget/i)).toBeInTheDocument();
    expect(screen.getAllByText(/12\.000/).length).toBeGreaterThan(0);
  });

  test("shows the input grid with one row per budgeted account", async () => {
    mockFetch(route());
    renderView();
    // The grid renders the account number + name in the row header.
    expect(
      await screen.findByText((_, el) =>
        el?.textContent === "2200 · Markedsføring",
      ),
    ).toBeInTheDocument();
    // Twelve month columns: jan…dec 2026.
    expect(screen.getByRole("columnheader", { name: /jan 2026/i })).toBeInTheDocument();
    expect(screen.getByRole("columnheader", { name: /dec 2026/i })).toBeInTheDocument();
  });

  test("each grid cell is an editable, labelled input", async () => {
    mockFetch(route());
    renderView();
    const juneInput = await screen.findByLabelText(
      /Budget for konto 2200 2026-06/i,
    );
    expect(juneInput).toBeInTheDocument();
    expect(juneInput).toHaveValue("5000");
  });

  test("offers a Sammenlign med faktisk toggle that swaps to the comparison table", async () => {
    mockFetch(route());
    renderView();
    await screen.findByRole("heading", { name: "Acme ApS" });
    await userEvent.click(
      screen.getByRole("tab", { name: /Sammenlign med faktisk/i }),
    );
    // Now the comparison table is rendered with budget, faktisk and afvigelse.
    expect(
      await screen.findByRole("columnheader", { name: "Faktisk" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("columnheader", { name: "Afvigelse" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("columnheader", { name: /Afvigelse %/i }),
    ).toBeInTheDocument();
  });

  test("the comparison table reports variance in kr AND as a percentage", async () => {
    mockFetch(route());
    renderView();
    await screen.findByRole("heading", { name: "Acme ApS" });
    await userEvent.click(
      screen.getByRole("tab", { name: /Sammenlign med faktisk/i }),
    );
    // The fixture's June line: budget 5000, actual 4000, variance 1000, 20%.
    // Intl in da-DK renders the percentage as "20 %" (with a non-breaking
    // space), so match either a regular or a non-breaking space before %.
    expect(await screen.findByText(/1\.000,00/)).toBeInTheDocument();
    expect(screen.getByText(/20[\s ]?%/)).toBeInTheDocument();
  });

  test("Tilføj budgetlinje appends a new revision via POST", async () => {
    mockFetch(route());
    renderView();
    await screen.findByRole("heading", { name: "Acme ApS" });

    await userEvent.type(screen.getByLabelText(/Kontonr/i), "2300");
    await userEvent.type(screen.getByLabelText("Beløb"), "1500");
    await userEvent.click(
      screen.getByRole("button", { name: /Tilføj budgetlinje/i }),
    );

    // After submit the form clears; the API call uses POST .../budget which
    // mockFetch resolves to the fixture above without throwing.
    const fields = screen.getAllByLabelText(/Kontonr/i) as HTMLInputElement[];
    expect(fields[0]!.value).toBe("");
  });

  test("an archived year shows a calm not-available state", async () => {
    mockFetch(route({ budget: { archived: true, selectedYear: "2025" } }));
    renderView();
    expect(
      await screen.findByText(/Budget er ikke tilgængeligt for 2025/),
    ).toBeInTheDocument();
  });

  test("the company sub-nav exposes the Budget tab", async () => {
    mockFetch(route());
    renderView();
    const tab = await screen.findByRole("link", { name: "Budget" });
    expect(tab).toHaveAttribute(
      "href",
      expect.stringContaining("/companies/acme-aps/budget"),
    );
  });
});
