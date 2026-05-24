// Tests for the Faktura-skabeloner view (#386).
//
// The view used to dump a bare CLI snippet in the empty state — the issue
// closes this gap by adding an "Opret skabelon" button + modal to the
// cockpit. These tests pin the new write-action behaviour: the button
// is present for a live year, hidden for an archived one, the modal opens
// when clicked, and the empty-state copy no longer contains the CLI
// command.

import { describe, expect, test } from "vitest";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { RecurringInvoicesView } from "./RecurringInvoicesView";
import { renderAt } from "../test/render";
import { mockFetch } from "../test/fixtures";

function emptyRecurring() {
  return { slug: "acme-aps", templates: [] };
}

function liveFiscalYears() {
  return {
    slug: "acme-aps",
    years: [
      { label: "2026", start: "2026-01-01", end: "2026-12-31", source: "live" as const },
      { label: "2025", start: null, end: null, source: "archive" as const },
    ],
  };
}

function archivedOnlyFiscalYears() {
  return {
    slug: "acme-aps",
    years: [
      { label: "2025", start: null, end: null, source: "archive" as const },
    ],
  };
}

function renderView() {
  return renderAt(<RecurringInvoicesView />, {
    route: "/companies/acme-aps/faktura-skabeloner",
    path: "/companies/:slug/faktura-skabeloner",
  });
}

describe("RecurringInvoicesView — Faktura-skabeloner", () => {
  test("empty state no longer dumps the CLI snippet at the owner", async () => {
    mockFetch({
      "GET /api/companies/acme-aps/recurring-invoices": {
        recurringInvoices: emptyRecurring(),
      },
      "GET /api/companies/acme-aps/fiscal-years": {
        fiscalYears: liveFiscalYears(),
      },
    });
    renderView();
    expect(
      await screen.findByText(/Ingen skabeloner endnu/),
    ).toBeInTheDocument();
    // The old empty state hard-coded `rentemester recurring-invoice create`.
    // The fix removes the CLI snippet entirely.
    expect(
      screen.queryByText(/rentemester recurring-invoice create/),
    ).not.toBeInTheDocument();
  });

  test("the Opret skabelon action opens the template modal", async () => {
    mockFetch({
      "GET /api/companies/acme-aps/recurring-invoices": {
        recurringInvoices: emptyRecurring(),
      },
      "GET /api/companies/acme-aps/fiscal-years": {
        fiscalYears: liveFiscalYears(),
      },
    });
    renderView();
    await screen.findByText(/Ingen skabeloner endnu/);
    await userEvent.click(
      screen.getByRole("button", { name: "Opret skabelon" }),
    );
    expect(
      screen.getByRole("dialog", { name: "Opret faktura-skabelon" }),
    ).toBeInTheDocument();
  });

  test("Opret skabelon is hidden for an archived (read-only) year", async () => {
    mockFetch({
      "GET /api/companies/acme-aps/recurring-invoices": {
        recurringInvoices: emptyRecurring(),
      },
      "GET /api/companies/acme-aps/fiscal-years": {
        fiscalYears: archivedOnlyFiscalYears(),
      },
    });
    renderView();
    await screen.findByText(/Ingen skabeloner endnu/);
    expect(
      screen.queryByRole("button", { name: "Opret skabelon" }),
    ).not.toBeInTheDocument();
  });
});
