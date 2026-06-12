import { describe, expect, test } from "vitest";
import { screen, within } from "@testing-library/react";
import { AccrualsView } from "./AccrualsView";
import { renderAt } from "../test/render";
import { mockFetch } from "../test/fixtures";

function payload(rows: any[] = []) {
  return {
    ok: true as const,
    accruals: {
      slug: "acme-aps",
      company: {
        name: "Acme ApS",
        cvr: "DK12345678",
        country: "DK",
        currency: "DKK",
      },
      report: {
        ok: true,
        accruals: rows,
        totals: {
          totalAmount: rows.reduce((s, r) => s + (r.totalAmount ?? 0), 0),
          recognizedAmount: rows.reduce((s, r) => s + (r.recognizedAmount ?? 0), 0),
          remainingAmount: rows.reduce((s, r) => s + (r.remainingAmount ?? 0), 0),
        },
        errors: [],
      },
    },
  };
}

function renderView(body = payload()) {
  mockFetch({ "GET /api/companies/acme-aps/accruals": body });
  return renderAt(<AccrualsView />, {
    route: "/companies/acme-aps/periodisering",
    path: "/companies/:slug/periodisering",
  });
}

describe("AccrualsView (#337)", () => {
  test("viser tom-state med info om CLI-flow", async () => {
    renderView();
    expect(
      await screen.findByText(/Ingen accruals registreret/),
    ).toBeInTheDocument();
  });

  test("lister registrerede accruals med recognized + remaining beløb", async () => {
    renderView(
      payload([
        {
          accrualId: 1,
          accrualType: "prepaid_expense",
          description: "Forsikring 12 mdr",
          totalAmount: 12000,
          recognitionPeriods: 12,
          recognizedPeriods: 4,
          recognizedAmount: 4000,
          remainingAmount: 8000,
          fullyRecognized: false,
          balanceAccountNo: "1300",
          resultAccountNo: "3150",
          firstRecognitionDate: "2026-01-31",
          periodStepMonths: 1,
        },
      ]),
    );
    const row = (await screen.findByText("Forsikring 12 mdr")).closest("tr")!;
    expect(within(row as HTMLElement).getByText(/4\/12/)).toBeInTheDocument();
    expect(within(row as HTMLElement).getByText("1300")).toBeInTheDocument();
    expect(within(row as HTMLElement).getByText("3150")).toBeInTheDocument();
  });

  test("portfolio-totals vises som pills", async () => {
    renderView(
      payload([
        {
          accrualId: 1,
          accrualType: "prepaid_expense",
          description: "Forsikring",
          totalAmount: 12000,
          recognitionPeriods: 12,
          recognizedPeriods: 4,
          recognizedAmount: 4000,
          remainingAmount: 8000,
          fullyRecognized: false,
          balanceAccountNo: "1300",
          resultAccountNo: "3150",
          firstRecognitionDate: "2026-01-31",
          periodStepMonths: 1,
        },
      ]),
    );
    expect(await screen.findByText(/I alt:/)).toBeInTheDocument();
    expect(screen.getByText(/Realiseret:/)).toBeInTheDocument();
    expect(screen.getByText(/Tilbage:/)).toBeInTheDocument();
  });
});
