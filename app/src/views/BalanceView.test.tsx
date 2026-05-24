import { describe, expect, test } from "vitest";
import { screen, within } from "@testing-library/react";
import { BalanceView } from "./BalanceView";
import { renderAt } from "../test/render";
import { balance, mockFetch } from "../test/fixtures";

function route(over = {}) {
  return {
    "GET /api/companies/acme-aps/balance": { balance: balance(over) },
  };
}

function renderView() {
  return renderAt(<BalanceView />, {
    route: "/companies/acme-aps/balance",
    path: "/companies/:slug/balance",
  });
}

describe("BalanceView — Balance", () => {
  test("renders the three balance sheet sections", async () => {
    mockFetch(route());
    renderView();
    expect(
      await screen.findByRole("heading", { name: "Acme ApS" }),
    ).toBeInTheDocument();
    expect(screen.getByText("Aktiver")).toBeInTheDocument();
    expect(screen.getByText("Passiver")).toBeInTheDocument();
    expect(screen.getByText("Egenkapital")).toBeInTheDocument();
  });

  test("shows the balanced confirmation when the sheet balances", async () => {
    mockFetch(route());
    renderView();
    expect(
      await screen.findByText(/Balancen stemmer/),
    ).toBeInTheDocument();
  });

  test("warns when the sheet does not balance", async () => {
    mockFetch(route({ balanced: false }));
    renderView();
    expect(
      await screen.findByText(/Balancen stemmer ikke/),
    ).toBeInTheDocument();
  });

  test("surfaces the period result on the equity side", async () => {
    mockFetch(route());
    renderView();
    expect(await screen.findByText("Årets resultat")).toBeInTheDocument();
  });

  test("the total liabilities + equity figure is rendered", async () => {
    mockFetch(route());
    renderView();
    const total = (
      await screen.findByText("Passiver og egenkapital i alt")
    ).closest("tr")!;
    expect(
      within(total as HTMLElement).getByText(/41\.388,03/),
    ).toBeInTheDocument();
  });

  test("an archived year renders the balance sheet under a read-only banner", async () => {
    mockFetch(
      route({ archived: true, archivedSource: "dinero", selectedYear: "2024" }),
    );
    renderView();
    expect(
      await screen.findByText(/Arkiveret regnskabsår 2024 — skrivebeskyttet/),
    ).toBeInTheDocument();
    // The archived balance sheet is still rendered as a real table.
    expect(screen.getByText("Aktiver")).toBeInTheDocument();
    expect(screen.getByText("Egenkapital")).toBeInTheDocument();
  });

  test("renders a prior-year sammenligningskolonne next to the current year (#400)", async () => {
    mockFetch(route());
    renderView();
    // The header carries both year-ultimo labels so the owner can read which
    // tal hører til hvilket regnskabsår — symmetry with resultatopgørelsen.
    expect(
      await screen.findByRole("columnheader", { name: "Pr. 2026-12-31" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("columnheader", { name: "Pr. 2025-12-31" }),
    ).toBeInTheDocument();
    // The prior-year asset on account 55000 (32.000) is shown on its row.
    const bankRow = screen
      .getByRole("link", { name: "55000" })
      .closest("tr")!;
    expect(
      within(bankRow as HTMLElement).getByText(/32\.000,00/),
    ).toBeInTheDocument();
    // The «Passiver og egenkapital i alt»-rækken har også en prior-celle.
    const totalRow = (
      screen.getByText("Passiver og egenkapital i alt")
    ).closest("tr")!;
    expect(
      within(totalRow as HTMLElement).getByText(/32\.000,00/),
    ).toBeInTheDocument();
  });

  test("when no prior year is in the ledger the prior column shows «—» rather than 0 (#400)", async () => {
    // A company in its first regnskabsår — every prior field is null on the
    // wire, the view must therefore render «—» in every prior cell rather
    // than misleadingly print 0,00.
    mockFetch(
      route({
        assets: {
          lines: [
            {
              accountNo: "55000",
              name: "Bank",
              amount: 41388.03,
              priorAmount: null,
            },
          ],
          total: 41388.03,
          priorTotal: null,
        },
        liabilities: {
          lines: [
            {
              accountNo: "64000",
              name: "Salgsmoms",
              amount: 3371,
              priorAmount: null,
            },
          ],
          total: 3371,
          priorTotal: null,
        },
        equity: {
          lines: [
            {
              accountNo: "51000",
              name: "Selskabskapital",
              amount: 24782.21,
              priorAmount: null,
            },
            {
              accountNo: "—",
              name: "Årets resultat",
              amount: 13234.82,
              priorAmount: null,
            },
          ],
          total: 38017.03,
          priorTotal: null,
        },
        priorTotalLiabilitiesAndEquity: null,
      }),
    );
    renderView();
    const bankRow = (
      await screen.findByRole("link", { name: "55000" })
    ).closest("tr")!;
    // The prior cell on a real account row is «—», not 0,00 kr.
    const dashCells = within(bankRow as HTMLElement).getAllByText("—");
    expect(dashCells.length).toBeGreaterThanOrEqual(1);
    // The «Passiver og egenkapital i alt»-rækken viser også «—».
    const totalRow = (
      screen.getByText("Passiver og egenkapital i alt")
    ).closest("tr")!;
    expect(within(totalRow as HTMLElement).getByText("—")).toBeInTheDocument();
  });
});
