import { describe, expect, test } from "bun:test";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { AnnualReportView } from "./AnnualReportView";
import { renderAt } from "../test/render";
import { mockFetch } from "../test/fixtures";

function payload(over: { report?: any; year?: string } = {}) {
  const year = over.year ?? "2025";
  return {
    ok: true as const,
    annualReport: {
      slug: "acme-aps",
      company: {
        name: "Acme ApS",
        cvr: "DK12345678",
        country: "DK",
        currency: "DKK",
        fiscalYearStartMonth: 1,
        fiscalYearLabelStrategy: "calendar",
      },
      fiscalYearStart: `${year}-01-01`,
      fiscalYearEnd: `${year}-12-31`,
      report: over.report ?? {
        ok: false,
        fiscalYearStart: `${year}-01-01`,
        fiscalYearEnd: `${year}-12-31`,
        company: {
          name: "Acme ApS",
          cvr: null,
          country: "DK",
          currency: "DKK",
        },
        errors: ["regnskabsåret er ikke låst"],
      },
    },
  };
}

function renderView(body = payload()) {
  mockFetch({ "GET /api/companies/acme-aps/annual-report": body });
  return renderAt(<AnnualReportView />, {
    route: "/companies/acme-aps/aarsrapport",
    path: "/companies/:slug/aarsrapport",
  });
}

describe("AnnualReportView (#338)", () => {
  test("formularen kalder API'et og viser forudsætnings-fejl", async () => {
    const user = userEvent.setup();
    renderView();
    await user.click(screen.getByRole("button", { name: /Byg årsrapport/ }));
    expect(
      await screen.findByText(/Forudsætninger mangler/),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/regnskabsåret er ikke låst/),
    ).toBeInTheDocument();
  });

  test("succes-respons viser stamdata + resultatopgørelse + balance", async () => {
    const user = userEvent.setup();
    renderView(
      payload({
        report: {
          ok: true,
          fiscalYearStart: "2025-01-01",
          fiscalYearEnd: "2025-12-31",
          company: {
            name: "Acme ApS",
            cvr: "DK12345678",
            country: "DK",
            currency: "DKK",
          },
          profitAndLoss: {
            income: { total: 1000000, lines: [] },
            expense: { total: 600000, lines: [] },
            result: 400000,
          },
          balanceSheet: {
            assets: { total: 500000, lines: [] },
            liabilities: { total: 100000, lines: [] },
            equity: { total: 400000, lines: [] },
          },
          notes: [],
          ledelsespategning: { text: "Ledelsens skabelon med dato og underskrifter.", placeholder: true },
          comparison: { fiscalYearStart: "2024-01-01", fiscalYearEnd: "2024-12-31", available: false },
          disclaimer: "Ejer eller revisor er ansvarlig for indberetning.",
          errors: [],
        },
      }),
    );
    await user.click(screen.getByRole("button", { name: /Byg årsrapport/ }));
    expect(await screen.findByText(/Resultatopgørelse/)).toBeInTheDocument();
    expect(screen.getByText(/Ledelsespåtegning/)).toBeInTheDocument();
    expect(screen.getByText(/Ledelsens skabelon/)).toBeInTheDocument();
    expect(screen.getByText(/Skabelon — udfyld dato/)).toBeInTheDocument();
    expect(screen.getByText(/Ingen sammenligningstal/)).toBeInTheDocument();
    expect(screen.getByText("Ejer eller revisor er ansvarlig for indberetning.", {exact:true})).toBeInTheDocument();
  });
});
