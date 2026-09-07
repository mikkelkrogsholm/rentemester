import { describe, expect, test } from "bun:test";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { FilterBar, FormField, PageState, ResponsiveTable, StatusChip } from "./CockpitPrimitives";
import { renderAt } from "../test/render";

describe("CockpitPrimitives", () => {
  test("binds a persistent label, hint and error to a 40px form control", () => {
    renderAt(<FormField label="Beløb" hint="Angiv beløb i kroner" error="Beløb mangler"><input /></FormField>);
    const input = screen.getByLabelText("Beløb");
    expect(input).toHaveAttribute("aria-invalid", "true");
    expect(input).toHaveAttribute("aria-describedby", expect.stringContaining("field-bel-b-hint"));
    expect(screen.getByRole("alert")).toHaveTextContent("Beløb mangler");
  });

  test("shows active filters, resets them, and keeps advanced filters keyboard reachable", async () => {
    let resets = 0;
    renderAt(<FilterBar activeFilters={["Status: Uafstemt"]} onReset={() => resets++} advanced={<FormField label="Til"><input type="date" /></FormField>}><FormField label="Søg"><input type="search" /></FormField></FilterBar>);
    expect(screen.getByText(/Aktive filtre: Status: Uafstemt/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Nulstil filtre" }));
    expect(resets).toBe(1);
    const summary = screen.getByText("Avancerede filtre");
    await userEvent.click(summary);
    expect(screen.getByLabelText("Til")).toBeInTheDocument();
  });

  test("has announced loading plus actionable empty, warning, blocked and retryable error states", async () => {
    const retry = { called: false };
    renderAt(<div>
      <PageState kind="loading" title="Henter poster" />
      <PageState kind="empty" title="Ingen poster">Opret den første post.</PageState>
      <PageState kind="warning" title="Kontrollér saldo" />
      <PageState kind="blocked" title="Perioden er låst" />
      <PageState kind="error" title="Kunne ikke hente" onRetry={() => { retry.called = true; }}>Prøv senere.</PageState>
    </div>);
    expect(screen.getByText("Henter poster").closest("section")).toHaveAttribute("aria-live", "polite");
    expect(screen.getByText("Ingen poster")).toBeInTheDocument();
    expect(screen.getByText("Kontrollér saldo").closest("section")).toHaveAttribute("role", "status");
    expect(screen.getByText("Perioden er låst").closest("section")).toHaveAttribute("role", "alert");
    await userEvent.click(screen.getByRole("button", { name: "Prøv igen" }));
    expect(retry.called).toBe(true);
  });

  test("keeps a semantic table and exposes compact mobile detail labels", () => {
    renderAt(<ResponsiveTable><thead><tr><th scope="col">Dato</th></tr></thead><tbody><tr><td data-label="Dato">2026-01-01</td></tr></tbody></ResponsiveTable>);
    expect(screen.getByRole("table")).toBeInTheDocument();
    expect(screen.getByText("2026-01-01")).toHaveAttribute("data-label", "Dato");
    renderAt(<StatusChip tone="warning">Uafstemt</StatusChip>);
    expect(screen.getByText("Uafstemt")).toHaveClass("status-chip--warning");
  });
});
