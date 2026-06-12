import { describe, expect, test } from "vitest";
import { screen, within } from "@testing-library/react";
import { TrialBalanceView } from "./TrialBalanceView";
import { renderAt } from "../test/render";
import { trialBalance, mockFetch } from "../test/fixtures";

function route(over = {}) {
  return {
    "GET /api/companies/acme-aps/trial-balance": {
      trialBalance: trialBalance(over),
    },
  };
}

function renderView() {
  return renderAt(<TrialBalanceView />, {
    route: "/companies/acme-aps/saldobalance",
    path: "/companies/:slug/saldobalance",
  });
}

describe("TrialBalanceView — Saldobalance", () => {
  test("renders the account table with debit/credit/balance columns", async () => {
    mockFetch(route());
    renderView();
    expect(
      await screen.findByRole("heading", { name: "Acme ApS" }),
    ).toBeInTheDocument();
    expect(screen.getByText("Debet")).toBeInTheDocument();
    expect(screen.getByText("Kredit")).toBeInTheDocument();
    expect(screen.getByText("Saldo")).toBeInTheDocument();
  });

  test("lists every moved account", async () => {
    mockFetch(route());
    renderView();
    expect(await screen.findByText("Omsætning")).toBeInTheDocument();
    // "Bank" also appears as a sub-nav tab, so scope to the account cell.
    expect(screen.getByRole("cell", { name: "Bank" })).toBeInTheDocument();
  });

  test("shows the totals row and the balanced confirmation", async () => {
    mockFetch(route());
    renderView();
    const total = (await screen.findByText("I alt")).closest("tr")!;
    expect(
      within(total as HTMLElement).getAllByText(/59\.217,05/).length,
    ).toBeGreaterThan(0);
    expect(screen.getByText(/Saldobalancen stemmer/)).toBeInTheDocument();
  });

  test("#372 — 'Hent CSV' links to the trial-balance CSV export endpoint", async () => {
    mockFetch(route());
    renderView();
    const link = await screen.findByRole("link", { name: "Hent CSV" });
    const href = link.getAttribute("href") ?? "";
    expect(href).toContain("/api/companies/acme-aps/trial-balance/export");
    expect(href).toContain("format=csv");
    expect(link.hasAttribute("download")).toBe(true);
  });

  test("an archived year renders the archived rows under a read-only banner", async () => {
    mockFetch(
      route({ archived: true, archivedSource: "dinero", selectedYear: "2025" }),
    );
    renderView();
    expect(
      await screen.findByText(/Arkiveret regnskabsår 2025 — skrivebeskyttet/),
    ).toBeInTheDocument();
    // The archived SaldoBalance is still rendered as a real table.
    expect(screen.getByText("Omsætning")).toBeInTheDocument();
    expect(screen.getByText("Debet")).toBeInTheDocument();
  });
});
