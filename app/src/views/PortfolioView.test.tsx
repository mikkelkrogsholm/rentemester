import { describe, expect, test } from "bun:test";
import { screen } from "@testing-library/react";
import { PortfolioView } from "./PortfolioView";
import { renderAt } from "../test/render";
import { mockFetch, summary } from "../test/fixtures";

function portfolioRoute(companies: ReturnType<typeof summary>[]) {
  const rollup = companies.reduce(
    (acc, c) => ({
      resultat: acc.resultat + c.resultat,
      liquidity: acc.liquidity + (c.actualBankBalance ?? 0),
      vatPayable: acc.vatPayable + (c.vat?.payable ?? 0),
      openTaskCount: acc.openTaskCount + c.openTaskCount,
    }),
    { resultat: 0, liquidity: 0, vatPayable: 0, openTaskCount: 0 },
  );
  return {
    "GET /api/portfolio": {
      portfolio: {
        workspace: "/ws",
        asOf: "2026-05-20",
        companyCount: companies.length,
        rollup: { ...rollup, liquidityComplete: companies.every((c) => c.actualBankBalance !== null) },
        totals: {},
        companies,
      },
    },
  };
}

describe("PortfolioView", () => {
  test("an empty workspace renders the first-run onboarding", async () => {
    mockFetch(portfolioRoute([]));
    renderAt(<PortfolioView />);
    expect(await screen.findByText(/Velkommen til Rentemester/i)).toBeInTheDocument();
    expect(
      screen.getByRole("form", { name: /Opret virksomhed/i }),
    ).toBeInTheDocument();
  });

  test("renders one card per company, attention first", async () => {
    mockFetch(
      portfolioRoute([
        summary({ slug: "calm-aps", name: "Calm ApS" }),
        summary({ slug: "broken-aps", name: "Broken ApS", auditChainOk: false }),
      ]),
    );
    renderAt(<PortfolioView />);
    const headings = await screen.findAllByRole("heading", { level: 3 });
    // "needs attention" sorts the broken company first.
    expect(headings[0]).toHaveTextContent("Broken ApS");
    expect(headings[1]).toHaveTextContent("Calm ApS");
  });

  test("shows the attention summary count", async () => {
    mockFetch(
      portfolioRoute([
        summary({ slug: "a", name: "A", openTaskCount: 1 }),
        summary({ slug: "b", name: "B" }),
      ]),
    );
    renderAt(<PortfolioView />);
    expect(await screen.findByText(/1 kræver opmærksomhed/i)).toBeInTheDocument();
  });

  test("renders the cross-company roll-up strip", async () => {
    mockFetch(
      portfolioRoute([
        summary({ slug: "a", name: "A" }),
        summary({ slug: "b", name: "B" }),
      ]),
    );
    renderAt(<PortfolioView />);
    expect(await screen.findByText(/Samlet resultat/i)).toBeInTheDocument();
    expect(screen.getByText(/Samlet likviditet/i)).toBeInTheDocument();
  });

  test("a single visible company opens its overview directly", async () => {
    mockFetch(portfolioRoute([summary({ actualBankBalance: null, bankStatementStatus: "ambiguous", bankStatementDiagnostics: ["source collision"] })]));
    renderAt(<PortfolioView />);
    expect(await screen.findByText(/Åbner virksomhedsoverblik/)).toBeInTheDocument();
  });

  test("surfaces an API error with a retry affordance", async () => {
    mockFetch({
      "GET /api/portfolio": {
        __error: { code: "not_found", message: "Portfolio unavailable" },
      },
    });
    renderAt(<PortfolioView />);
    expect(await screen.findByRole("alert")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Prøv igen/i })).toBeInTheDocument();
  });
});

test("unknown bank or VAT data does not imply healthy operation", async () => {
  mockFetch(portfolioRoute([summary({ actualBankBalance: null, vat: null }), summary({ slug: "beta-aps", name: "Beta ApS", actualBankBalance: null, vat: null })]));
  renderAt(<PortfolioView />);
  expect(await screen.findAllByText("Overblik kræver flere oplysninger")).toHaveLength(2);
  expect(screen.queryByText("Sund drift")).not.toBeInTheDocument();
  expect(screen.queryByText("Ingen kendte advarsler")).not.toBeInTheDocument();
});

test("negative VAT is described as receivable in the portfolio and company card", async () => {
  mockFetch(portfolioRoute([summary({ vat: { payable: -250, deadline: "2026-09-01", daysRemaining: 10 } }), summary({ slug: "beta-aps", name: "Beta ApS", vat: { payable: -100, deadline: "2026-09-01", daysRemaining: 10 } })]));
  renderAt(<PortfolioView />);
  expect(await screen.findAllByText("Moms til gode")).toHaveLength(3);
  expect(screen.queryByText("Moms at betale")).not.toBeInTheDocument();
});
