import { describe, expect, test } from "bun:test";
import { screen } from "@testing-library/react";
import { InvoiceCreateView } from "./InvoiceCreateView";
import { InvoiceDetailView } from "./InvoicesView";
import { renderAt } from "../test/render";
import { invoices, mockFetch } from "../test/fixtures";

describe("InvoiceCreateView", () => {
  test("the active year presents the existing invoice form as a full page", async () => {
    mockFetch({ "GET /api/companies/acme-aps/invoices": { invoices: invoices() } });
    renderAt(<InvoiceCreateView />, { route: "/companies/acme-aps/fakturaer/ny?year=2026", path: "/companies/:slug/fakturaer/ny" });
    expect(await screen.findByRole("heading", { name: "Ny faktura", level: 1 })).toBeInTheDocument();
    expect(screen.getByLabelText("Fakturadato")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Udsted faktura" })).toBeInTheDocument();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
  test("an archived year cannot create an invoice", async () => {
    mockFetch({ "GET /api/companies/acme-aps/invoices": { invoices: invoices({ selectedYear: "2025", archived: true }) } });
    renderAt(<InvoiceCreateView />, { route: "/companies/acme-aps/fakturaer/ny?year=2025", path: "/companies/:slug/fakturaer/ny" });
    expect(await screen.findByRole("heading", { name: "Regnskabsåret er arkiveret" })).toBeInTheDocument();
    expect(screen.queryByLabelText("Fakturadato")).not.toBeInTheDocument();
  });
});

describe("InvoiceDetailView", () => {
  test("looks up documentId within the selected year and keeps return filters", async () => {
    mockFetch({ "GET /api/companies/acme-aps/invoices": { invoices: invoices() } });
    renderAt(<InvoiceDetailView />, { route: "/companies/acme-aps/fakturaer/2?year=2026&returnTo=%2Fcompanies%2Facme-aps%2Ffakturaer%3Fyear%3D2026%26status%3Doverdue", path: "/companies/:slug/fakturaer/:documentId" });
    expect(await screen.findByRole("heading", { name: "Faktura 2026-00002" })).toBeInTheDocument();
    expect(screen.queryByText("2026-00001")).not.toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Hent PDF" })).toHaveAttribute("href", "/api/companies/acme-aps/invoices/2/pdf");
    expect(screen.getByRole("link", { name: "Tilbage til fakturaer" })).toHaveAttribute("href", "/companies/acme-aps/fakturaer?year=2026&status=overdue");
  });
  test("an absent invoice is clearly scoped to the selected year", async () => {
    mockFetch({ "GET /api/companies/acme-aps/invoices": { invoices: invoices() } });
    renderAt(<InvoiceDetailView />, { route: "/companies/acme-aps/fakturaer/99?year=2026", path: "/companies/:slug/fakturaer/:documentId" });
    expect(await screen.findByRole("heading", { name: "Fakturaen findes ikke i 2026" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Afstem" })).not.toBeInTheDocument();
  });
});
