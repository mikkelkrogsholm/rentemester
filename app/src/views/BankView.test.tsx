import { describe, expect, test } from "vitest";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { BankView } from "./BankView";
import { renderAt } from "../test/render";
import { bank, invoices, mockFetch } from "../test/fixtures";

function route(over = {}) {
  return {
    "GET /api/companies/acme-aps/bank": { bank: bank(over) },
  };
}

function renderView(path = "/companies/acme-aps/bank") {
  return renderAt(<BankView />, {
    route: path,
    path: "/companies/:slug/bank",
  });
}

describe("BankView — Bank", () => {
  test("shows the booked balance and the bank account", async () => {
    mockFetch(route());
    renderView();
    expect(
      await screen.findByRole("heading", { name: "Acme ApS" }),
    ).toBeInTheDocument();
    expect(screen.getByText("Bogført saldo")).toBeInTheDocument();
    expect(screen.getByText(/Danske Bank/)).toBeInTheDocument();
  });

  test("lists transactions with their reconciliation status", async () => {
    mockFetch(route());
    renderView();
    expect(
      await screen.findByText("Indbetaling faktura 1001"),
    ).toBeInTheDocument();
    expect(screen.getByText(/Afstemt/)).toBeInTheDocument();
    expect(screen.getByText("Uafstemt")).toBeInTheDocument();
  });

  test("shows the booked-vs-actual difference prominently at the top", async () => {
    mockFetch(route());
    renderView();
    expect(
      await screen.findByRole("heading", { name: "Acme ApS" }),
    ).toBeInTheDocument();
    // The gap banner names the difference and the actual statement balance.
    expect(screen.getByText("Difference")).toBeInTheDocument();
    expect(screen.getByText(/17\.733,28/)).toBeInTheDocument();
    expect(screen.getByText("Faktisk saldo")).toBeInTheDocument();
  });

  test("a reconciled account shows the afstemt banner", async () => {
    mockFetch(route({ actualBalance: 41388.03, difference: 0 }));
    renderView();
    expect(
      await screen.findByText(/Kontoudtog og bogført saldo stemmer/),
    ).toBeInTheDocument();
  });

  test("without an imported statement the banner says so", async () => {
    mockFetch(route({ actualBalance: null, difference: null }));
    renderView();
    expect(
      await screen.findByText(/Intet kontoudtog importeret for/),
    ).toBeInTheDocument();
  });

  test("an archived year shows the imported transactions with an arkiv notice", async () => {
    mockFetch(route({ archived: true, selectedYear: "2025" }));
    renderView();
    expect(
      await screen.findByText(/2025 er et arkiveret regnskabsår/),
    ).toBeInTheDocument();
    // The imported statement rows are still shown for the archived year.
    expect(
      screen.getByText("Indbetaling faktura 1001"),
    ).toBeInTheDocument();
  });

  // #213 slice 2 — the bank-CSV-import write action.

  test("a live year offers an Importér kontoudtog action", async () => {
    mockFetch(route());
    renderView();
    await screen.findByRole("heading", { name: "Acme ApS" });
    expect(
      screen.getByRole("button", { name: "Importér kontoudtog" }),
    ).toBeInTheDocument();
  });

  test("an archived year offers no import action", async () => {
    mockFetch(route({ archived: true, selectedYear: "2025" }));
    renderView();
    await screen.findByText(/2025 er et arkiveret regnskabsår/);
    expect(
      screen.queryByRole("button", { name: "Importér kontoudtog" }),
    ).not.toBeInTheDocument();
  });

  test("clicking Importér kontoudtog opens the import modal", async () => {
    mockFetch(route());
    renderView();
    await userEvent.click(
      await screen.findByRole("button", { name: "Importér kontoudtog" }),
    );
    expect(
      screen.getByRole("dialog", { name: "Importér kontoudtog" }),
    ).toBeInTheDocument();
  });

  // #365 — the cockpit-driven settlement of an unmatched bank row.

  test("each Uafstemt row offers a Bogfør action so the owner can post from the cockpit", async () => {
    mockFetch(route());
    renderView();
    await screen.findByText("Indbetaling faktura 1001");
    // The unmatched row (the gebyr) has a Bogfør button; the matched one
    // does not — only un-reconciled lines need the action.
    expect(
      screen.getAllByRole("button", { name: "Bogfør" }).length,
    ).toBe(1);
  });

  test("clicking Bogfør opens the bank-reconcile modal seeded with the row", async () => {
    mockFetch({
      ...route(),
      "GET /api/companies/acme-aps/invoices": { invoices: invoices() },
    });
    renderView();
    await userEvent.click(
      await screen.findByRole("button", { name: "Bogfør" }),
    );
    expect(
      screen.getByRole("dialog", { name: "Bogfør banktransaktion" }),
    ).toBeInTheDocument();
    // The bank-row's description is rendered in the modal as context.
    expect(screen.getAllByText(/Gebyr/).length).toBeGreaterThan(0);
  });

  test("an archived year shows no Bogfør action — there is no live ledger to post into", async () => {
    mockFetch(route({ archived: true, selectedYear: "2025" }));
    renderView();
    await screen.findByText(/2025 er et arkiveret regnskabsår/);
    expect(
      screen.queryByRole("button", { name: "Bogfør" }),
    ).not.toBeInTheDocument();
  });

  // #451 — filter-bar: fritekst, datointerval, status, sortering.
  const MULTI_TX = {
    transactions: [
      {
        id: 1,
        date: "2026-03-15",
        text: "Indbetaling Energinet",
        amount: 4200,
        runningBalance: 4200,
        reconciliationStatus: "matched" as const,
        journalEntryNo: "B-2026-0001",
      },
      {
        id: 2,
        date: "2026-01-10",
        text: "Gebyr Danske Bank",
        amount: -45,
        runningBalance: 4155,
        reconciliationStatus: "unmatched" as const,
        journalEntryNo: null,
      },
      {
        id: 3,
        date: "2026-05-04",
        text: "Husleje udbetaling",
        amount: -8500,
        runningBalance: -4345,
        reconciliationStatus: "unmatched" as const,
        journalEntryNo: null,
      },
    ],
    matchedCount: 1,
    unmatchedCount: 2,
  };

  test("filters bank rows by free-text on transaction text (#451)", async () => {
    mockFetch(route(MULTI_TX));
    renderView();
    await screen.findByText("Indbetaling Energinet");
    const search = screen.getByPlaceholderText(/Søg på tekst/i);
    await userEvent.type(search, "energinet");
    expect(screen.getByText("Indbetaling Energinet")).toBeInTheDocument();
    expect(screen.queryByText("Gebyr Danske Bank")).not.toBeInTheDocument();
    expect(screen.queryByText("Husleje udbetaling")).not.toBeInTheDocument();
    expect(
      screen.getByText(/1 af 3 transaktioner matcher/i),
    ).toBeInTheDocument();
  });

  test("filters bank rows by status — kun uafstemte (#451)", async () => {
    mockFetch(route(MULTI_TX));
    renderView();
    await screen.findByText("Indbetaling Energinet");
    const status = screen.getByLabelText(/Status/i);
    await userEvent.selectOptions(status, "unmatched");
    expect(screen.queryByText("Indbetaling Energinet")).not.toBeInTheDocument();
    expect(screen.getByText("Gebyr Danske Bank")).toBeInTheDocument();
    expect(screen.getByText("Husleje udbetaling")).toBeInTheDocument();
  });

  test("filters by date range (#451)", async () => {
    mockFetch(route(MULTI_TX));
    renderView();
    await screen.findByText("Indbetaling Energinet");
    const fromInput = screen.getByLabelText(/^Fra$/i);
    await userEvent.type(fromInput, "2026-02-01");
    expect(screen.queryByText("Gebyr Danske Bank")).not.toBeInTheDocument();
    expect(screen.getByText("Indbetaling Energinet")).toBeInTheDocument();
    expect(screen.getByText("Husleje udbetaling")).toBeInTheDocument();
  });

  test("filter state is reflected in URL params (#451)", async () => {
    mockFetch(route(MULTI_TX));
    renderView(
      "/companies/acme-aps/bank?q=energinet&status=unmatched",
    );
    // q=energinet matches "Indbetaling Energinet"; but status=unmatched
    // excludes it. The only row that survives both filters is none — empty
    // state should be shown.
    await screen.findByRole("heading", { name: "Acme ApS" });
    expect(
      screen.getByText(/Ingen transaktioner matcher filtrene/i),
    ).toBeInTheDocument();
    expect(screen.queryByText("Indbetaling Energinet")).not.toBeInTheDocument();
    expect(screen.queryByText("Husleje udbetaling")).not.toBeInTheDocument();
  });

  test("URL params drive the search input on first render (#451)", async () => {
    mockFetch(route(MULTI_TX));
    renderView("/companies/acme-aps/bank?q=husleje");
    await screen.findByText("Husleje udbetaling");
    expect(screen.queryByText("Indbetaling Energinet")).not.toBeInTheDocument();
    expect(screen.queryByText("Gebyr Danske Bank")).not.toBeInTheDocument();
  });

  test("Ryd filtre-knappen er kun synlig når et filter er aktivt (#451)", async () => {
    mockFetch(route(MULTI_TX));
    renderView();
    await screen.findByText("Indbetaling Energinet");
    expect(
      screen.queryByRole("button", { name: /Ryd filtre/i }),
    ).not.toBeInTheDocument();
    const search = screen.getByPlaceholderText(/Søg på tekst/i);
    await userEvent.type(search, "energinet");
    expect(
      screen.getByRole("button", { name: /Ryd filtre/i }),
    ).toBeInTheDocument();
  });
});
