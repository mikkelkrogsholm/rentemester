import { describe, expect, test } from "vitest";
import { render, screen, within } from "@testing-library/react";
import { ArchiveView } from "./ArchiveView";
import { ArchivedBanner } from "../components/ArchivedBanner";
import { renderAt } from "../test/render";
import { fiscalYears, mockFetch } from "../test/fixtures";

function route(over: { fy?: any } = {}) {
  return {
    "GET /api/companies/acme-aps/fiscal-years": {
      fiscalYears: over.fy ?? fiscalYears(),
    },
  };
}

function renderView(routePath = "/companies/acme-aps/arkiv") {
  return renderAt(<ArchiveView />, {
    route: routePath,
    path: "/companies/:slug/arkiv",
  });
}

describe("ArchiveView — Om arkivet", () => {
  test("explains that the archive is the read-only Dinero import", async () => {
    mockFetch(route());
    renderView();
    expect(
      await screen.findByText(
        /Det arkiverede regnskab er skrivebeskyttet/,
      ),
    ).toBeInTheDocument();
    // The owner sees plain Danish — never raw GitHub-issue numbers (#371).
    expect(screen.getByText(/Dinero-regnskab/)).toBeInTheDocument();
    expect(
      screen.queryByText(/\(#\d+\)/),
    ).not.toBeInTheDocument();
  });

  test("the Kilde column reads as plain Danish, not a developer ticket (#371)", async () => {
    mockFetch(route());
    renderView();
    const row = (
      await screen.findByRole("cell", { name: /2025/ })
    ).closest("tr")!;
    const kilde = within(row as HTMLElement).getByText(/Dinero/);
    expect(kilde.textContent).not.toMatch(/\(#\d+\)/);
  });

  test("lists each archived year with links into the core views", async () => {
    mockFetch(route());
    renderView();
    const row2025 = (
      await screen.findByRole("cell", { name: /2025/ })
    ).closest("tr")!;
    expect(
      within(row2025 as HTMLElement).getByRole("link", { name: "Saldobalance" }),
    ).toHaveAttribute("href", "/companies/acme-aps/saldobalance?year=2025");
    expect(
      within(row2025 as HTMLElement).getByRole("link", {
        name: "Resultatopgørelse",
      }),
    ).toHaveAttribute(
      "href",
      "/companies/acme-aps/resultatopgorelse?year=2025",
    );
  });

  test("the banner copy is plain Danish without GitHub-issue numbers (#371)", () => {
    render(<ArchivedBanner year="2024" source="dinero" />);
    expect(
      screen.getByText(/Arkiveret regnskabsår 2024/),
    ).toBeInTheDocument();
    // The banner used to leak the literal "(#197)" into the body copy; the
    // owner must see only plain Danish prose, never a developer ticket.
    expect(screen.queryByText(/\(#\d+\)/)).not.toBeInTheDocument();
  });

  test("a company with no archive shows the empty state", async () => {
    mockFetch(
      route({
        fy: fiscalYears([
          { label: "2026", start: "2026-01-01", end: "2026-12-31", source: "live" },
        ]),
      }),
    );
    renderView();
    expect(
      await screen.findByText(/Ingen arkiverede regnskabsår/),
    ).toBeInTheDocument();
  });
});
