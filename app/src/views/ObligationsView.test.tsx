import { describe, expect, test } from "vitest";
import { screen, within } from "@testing-library/react";
import { ObligationsView } from "./ObligationsView";
import { renderAt } from "../test/render";
import { obligations, mockFetch } from "../test/fixtures";

function route(over = {}) {
  return {
    "GET /api/companies/acme-aps/obligations": {
      obligations: obligations(over),
    },
  };
}

function renderView() {
  return renderAt(<ObligationsView />, {
    route: "/companies/acme-aps/forpligtelser",
    path: "/companies/:slug/forpligtelser",
  });
}

describe("ObligationsView — Forpligtelser", () => {
  test("lists each obligation with its label and amount", async () => {
    mockFetch(route());
    renderView();
    expect(
      await screen.findByRole("heading", { name: "Acme ApS" }),
    ).toBeInTheDocument();
    expect(screen.getByText("Moms — Q2 2026")).toBeInTheDocument();
    expect(screen.getByText("Skyldig selskabsskat")).toBeInTheDocument();
    expect(
      screen.getByText("Kreditorer (leverandørgæld)"),
    ).toBeInTheDocument();
  });

  test("shows a derived deadline and a dateless dash", async () => {
    mockFetch(route());
    renderView();
    expect(await screen.findByText("2026-09-01")).toBeInTheDocument();
    expect(screen.getByText("2027-11-01")).toBeInTheDocument();
    // The creditor row has no known deadline — shown as an em dash.
    const creditorRow = screen
      .getByText("Kreditorer (leverandørgæld)")
      .closest("tr")!;
    expect(
      within(creditorRow as HTMLElement).getByText("Ingen frist"),
    ).toBeInTheDocument();
  });

  test("shows the total owed summary", async () => {
    mockFetch(route());
    renderView();
    // "Skyldige beløb i alt" labels both the summary card and the totals row.
    expect(
      (await screen.findAllByText("Skyldige beløb i alt")).length,
    ).toBeGreaterThan(0);
    expect(screen.getAllByText(/7\.527,66/).length).toBeGreaterThan(0);
  });

  test("a company that owes nothing shows the empty state", async () => {
    mockFetch(route({ obligations: [], totalOwed: 0 }));
    renderView();
    expect(
      await screen.findByText(/Ingen forpligtelser/),
    ).toBeInTheDocument();
  });

  // #290: the årsrapport filing deadline to Erhvervsstyrelsen must appear
  // alongside VAT — it is the other recurring legal deadline.
  test("lists the annual-report filing deadline", async () => {
    mockFetch(route());
    renderView();
    const annualRow = (
      await screen.findByText(/Årsrapport — regnskabsår 2026/)
    ).closest("tr")!;
    // It carries the derived Erhvervsstyrelsen deadline …
    expect(
      within(annualRow as HTMLElement).getByText("2027-05-01"),
    ).toBeInTheDocument();
    // … and no kroner amount — it is a deadline, not a debt. The amount
    // sits in the 5. kolonne (index 4): Forpligtelse, Konto, Frist, Status,
    // Beløb, Handlinger (#391). Den skal vise en bindestreg, ikke "0,00 kr.".
    const cells = within(annualRow as HTMLElement).getAllByRole("cell");
    expect(cells[4]).toHaveTextContent("—");
    expect(cells[4]).not.toHaveTextContent(/0,00/);
  });

  // #391 — Forpligtelser-rækken skal være en handlingsbar bro videre til
  // den officielle indberetnings-portal og en hurtig 'Kopiér beløb'.
  test("vat-rækken har et eksternt skat.dk-link (#391)", async () => {
    mockFetch(route());
    renderView();
    const vatRow = (
      await screen.findByText(/Moms — Q2 2026/)
    ).closest("tr")!;
    const link = within(vatRow as HTMLElement).getByRole("link", {
      name: /skat\.dk/i,
    });
    expect(link).toHaveAttribute("href", expect.stringContaining("skat.dk"));
    expect(link).toHaveAttribute("target", "_blank");
    expect(link).toHaveAttribute("rel", "noopener noreferrer");
  });

  test("annual-report-rækken har et eksternt virk.dk-link (#391)", async () => {
    mockFetch(route());
    renderView();
    const annualRow = (
      await screen.findByText(/Årsrapport — regnskabsår 2026/)
    ).closest("tr")!;
    const link = within(annualRow as HTMLElement).getByRole("link", {
      name: /virk\.dk/i,
    });
    expect(link).toHaveAttribute("href", expect.stringContaining("virk.dk"));
  });

  test("corporation-tax-rækken har et eksternt selskabsskat-link (#391)", async () => {
    mockFetch(route());
    renderView();
    const taxRow = (
      await screen.findByText(/Skyldig selskabsskat/)
    ).closest("tr")!;
    const link = within(taxRow as HTMLElement).getByRole("link", {
      name: /skat\.dk/i,
    });
    expect(link).toHaveAttribute(
      "href",
      expect.stringContaining("selskabsskat"),
    );
  });

  test("vat-rækken krydslinker til Moms-viewet (#391)", async () => {
    mockFetch(route());
    renderView();
    const vatRow = (
      await screen.findByText(/Moms — Q2 2026/)
    ).closest("tr")!;
    const link = within(vatRow as HTMLElement).getByRole("link", {
      name: /SKAT-rubrikker/,
    });
    expect(link).toHaveAttribute("href", "/companies/acme-aps/moms");
  });

  test("hver gæld-række har en 'Kopiér beløb'-knap; annual-report har ikke (#391)", async () => {
    mockFetch(route());
    renderView();
    const vatRow = (
      await screen.findByText(/Moms — Q2 2026/)
    ).closest("tr")!;
    expect(
      within(vatRow as HTMLElement).getByRole("button", { name: /Kopiér beløb/ }),
    ).toBeInTheDocument();

    // Annual-report har intet beløb at kopiere.
    const annualRow = (
      await screen.findByText(/Årsrapport — regnskabsår 2026/)
    ).closest("tr")!;
    expect(
      within(annualRow as HTMLElement).queryByRole("button", {
        name: /Kopiér beløb/,
      }),
    ).not.toBeInTheDocument();
  });

  test("an archived year shows an honest 'not available' state", async () => {
    mockFetch(route({ archived: true, selectedYear: "2025" }));
    renderView();
    expect(
      await screen.findByText(
        /Forpligtelser er ikke tilgængelige for 2025/,
      ),
    ).toBeInTheDocument();
  });
});
