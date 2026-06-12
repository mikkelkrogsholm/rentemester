import { describe, expect, test } from "vitest";
import { screen } from "@testing-library/react";
import { PeriodsView } from "./PeriodsView";
import { renderAt } from "../test/render";
import { mockFetch } from "../test/fixtures";

function payload(rows: any[] = []) {
  return {
    ok: true as const,
    periods: {
      slug: "acme-aps",
      company: {
        name: "Acme ApS",
        cvr: "DK12345678",
        country: "DK",
        currency: "DKK",
      },
      periods: rows,
      byStatus: {
        open: rows.filter((r) => r.effectiveStatus === "open").length,
        closed: rows.filter((r) => r.effectiveStatus === "closed").length,
        reported: rows.filter((r) => r.effectiveStatus === "reported").length,
      },
    },
  };
}

function renderView(body = payload()) {
  mockFetch({ "GET /api/companies/acme-aps/periods": body });
  return renderAt(<PeriodsView />, {
    route: "/companies/acme-aps/periodelas",
    path: "/companies/:slug/periodelas",
  });
}

describe("PeriodsView (#342)", () => {
  test("viser tom-state når der ikke er nogen perioder", async () => {
    renderView();
    expect(
      await screen.findByText(/Ingen lukkede perioder endnu/),
    ).toBeInTheDocument();
  });

  test("har en 'Luk periode'-knap i page-head", async () => {
    renderView();
    expect(
      await screen.findByRole("button", { name: /Luk periode/ }),
    ).toBeInTheDocument();
  });

  test("lister en lukket periode med effective status", async () => {
    renderView(
      payload([
        {
          id: 1,
          periodStart: "2026-01-01",
          periodEnd: "2026-03-31",
          kind: "vat_quarter",
          rowStatus: "closed",
          effectiveStatus: "closed",
          closedAt: "2026-04-15T12:00:00Z",
          closedBy: "user:owner",
          reference: "Q1 2026",
        },
      ]),
    );
    expect(await screen.findByText("2026-01-01")).toBeInTheDocument();
    expect(screen.getByText("2026-03-31")).toBeInTheDocument();
    expect(screen.getByText("Momsperiode")).toBeInTheDocument();
    expect(screen.getByText("Lukket")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /Genåbn/ }),
    ).toBeInTheDocument();
  });

  test("en indberettet periode kan ikke genåbnes", async () => {
    renderView(
      payload([
        {
          id: 2,
          periodStart: "2025-01-01",
          periodEnd: "2025-12-31",
          kind: "fiscal_year",
          rowStatus: "reported",
          effectiveStatus: "reported",
          closedAt: "2026-04-15T12:00:00Z",
          closedBy: "user:owner",
          reference: null,
        },
      ]),
    );
    // "Indberettet — kan ikke genåbnes"-labelet sidder i tabel-rækken;
    // intro-paragraffen nævner også at indberettede perioder ikke kan
    // genåbnes, så vi matcher det specifikke 'Indberettet —'-prefix.
    expect(
      await screen.findByText(/Indberettet — kan ikke genåbnes/),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /Genåbn/ }),
    ).not.toBeInTheDocument();
  });
});
