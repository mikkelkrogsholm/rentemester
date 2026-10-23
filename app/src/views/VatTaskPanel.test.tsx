import { describe, expect, test } from "bun:test";
import { screen, within, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { VatTaskPanel } from "./VatTaskPanel";
import { vat, vatNotRegistered, mockFetch } from "../test/fixtures";
import { renderAt } from "../test/render";
import type { CompanyVat, AccountingPeriodRow, PeriodCloseReadinessPacket } from "../lib/types";

const row: AccountingPeriodRow = { id: 1, periodStart: "2026-01-01", periodEnd: "2026-03-31", kind: "vat_period", rowStatus: "closed", effectiveStatus: "closed", closePacketHash: "a".repeat(64), reference: null, closedAt: "2026-04-01", closedBy: "user:reviewer" };
const packet: PeriodCloseReadinessPacket = { hash: "a".repeat(64), blockers: 0, warnings: 0, items: [] };
function routes(rows: AccountingPeriodRow[] = [], readiness = packet) {
  return {
    "GET /api/companies/acme-aps/periods": { periods: { slug: "acme-aps", periods: rows, reportingAvailable: true } },
    "GET /api/companies/acme-aps/periods/close-readiness": { packet: readiness },
  };
}
function panel(v: CompanyVat = vat(), verified = true) {
  return <VatTaskPanel slug="acme-aps" vat={v} verified={verified} onRefresh={() => {}} />;
}
function render(v: CompanyVat = vat()) { return renderAt(panel(v)); }
async function state(value: string) {
  await waitFor(() => expect(screen.getByRole("region", { name: "Momsopgave" })).toHaveAttribute("data-state", value));
  return within(screen.getByRole("region", { name: "Momsopgave" }));
}

describe("canonical VAT task", () => {
  test("unresolved bank/document controls expose actual counts and source routes, not submission", async () => {
    mockFetch(routes([], { ...packet, blockers: 2, items: [
      { code: "BANK_UNRECONCILED", status: "blocked", count: 1, waivable: true },
      { code: "DOCUMENT_OUTSTANDING", status: "blocked", count: 1, waivable: true },
    ] }));
    render();
    const region = await state("blocked");
    expect(region.getByText(/Bankposter mangler afstemning/)).toHaveTextContent("1 (blokeret)");
    expect(region.getByText(/Bilag mangler gennemgang/)).toBeInTheDocument();
    expect(region.getByRole("link", { name: "Gennemgå bank" })).toHaveAttribute("href", "/companies/acme-aps/bank?year=2026");
    expect(region.getByRole("link", { name: "Gennemgå bilag" })).toHaveAttribute("href", "/companies/acme-aps/bilag?year=2026");
    expect(region.getByText("Rentemester sender ikke til SKAT.")).toBeInTheDocument();
    expect((globalThis.fetch as unknown as { mock: { calls: [unknown, RequestInit?][] } }).mock.calls.every(([, options]) => !options?.method || options.method === "GET")).toBe(true);
  });
  test("clean open period remains a draft", async () => {
    mockFetch(routes()); render();
    expect((await state("draft")).getByText(/Beregnet kladde/)).toBeInTheDocument();
    expect(screen.queryByText(/Kvitteringsreference:/)).not.toBeInTheDocument();
  });
  test("closed canonical packet is reviewed, never recorded from totals/closure", async () => {
    mockFetch(routes([row])); render(vat({ periodStatus: "closed", momsangivelseReady: true }));
    const region = await state("reviewed");
    expect(region.getByText(/Indberet manuelt i TastSelv/)).toBeInTheDocument();
    expect(region.getByRole("link", { name: "Åbn Periodelås" })).toHaveAttribute("href", "/companies/acme-aps/periodelas?year=2026");
  });
  test("reported receipt uses exact VAT period in preference to legacy quarter", async () => {
    mockFetch(routes([{ ...row, effectiveStatus: "reported", reference: "SYNTHETIC-RECEIPT" }, { ...row, id: 2, kind: "vat_quarter", effectiveStatus: "reported", reference: "OLD-QUARTER" }]));
    render(vat({ periodStatus: "reported", momsangivelseReady: true }));
    const region = await state("recorded");
    expect(region.getByText("SYNTHETIC-RECEIPT")).toBeInTheDocument();
    expect(region.queryByText("OLD-QUARTER")).not.toBeInTheDocument();
    expect(region.queryByText(/Indberet manuelt/)).not.toBeInTheDocument();
  });
  for (const [name, rows] of [
    ["wrong period", [{ ...row, periodEnd: "2026-06-30", effectiveStatus: "reported", reference: "OTHER" }]],
    ["missing receipt", [{ ...row, effectiveStatus: "reported", reference: " " }]],
    ["missing close proof", [{ ...row, effectiveStatus: "reported", closePacketHash: null, reference: "UNPROVEN" }]],
    ["contradictory status", [row]],
  ] as const) test(`refuses ${name}`, async () => {
    mockFetch(routes([...rows])); render(vat({ periodStatus: "reported", momsangivelseReady: true }));
    expect((await state("unavailable")).getByText(/Kan ikke verificere/)).toBeInTheDocument();
    expect(screen.queryByText(/Ekstern indberetning registreret/)).not.toBeInTheDocument();
  });
  test("failed refresh removes a previously recorded success", async () => {
    mockFetch(routes([{ ...row, effectiveStatus: "reported", reference: "RECEIPT" }]));
    render(vat({ periodStatus: "reported", momsangivelseReady: true })); await state("recorded");
    mockFetch({ "GET /api/companies/acme-aps/periods": { __error: { code: "forbidden", message: "Ingen adgang" } } });
    await userEvent.click(screen.getByRole("button", { name: "Opdatér momsopgave" }));
    expect((await state("unavailable")).queryByText("RECEIPT")).not.toBeInTheDocument();
  });
  test("stale VAT snapshot cannot certify a receipt", async () => {
    mockFetch(routes([{ ...row, effectiveStatus: "reported", reference: "RECEIPT" }]));
    const v = vat({ periodStatus: "reported", momsangivelseReady: true });
    const view = render(v); await state("recorded");
    view.rerender(panel(v, false));
    expect((await state("unavailable")).queryByText("RECEIPT")).not.toBeInTheDocument();
  });
  test("archive and nonregistration make no canonical calls", async () => {
    mockFetch({});
    const view = render(vat({ archived: true })); await state("archived");
    view.rerender(panel(vatNotRegistered())); await state("unavailable");
    expect(globalThis.fetch).not.toHaveBeenCalled();
  });
  test("unknown and unavailable controls remain visible, not waived", async () => {
    mockFetch(routes([], { ...packet, blockers: 1, items: [{ code: "FUTURE_CONTROL", status: "unavailable", count: 3, waivable: false }] }));
    render(); const region = await state("blocked");
    expect(region.getByText(/FUTURE_CONTROL/)).toHaveTextContent("3 (kan ikke kontrolleres)");
    expect(region.getByRole("link", { name: "Gennemgå kilde" })).toHaveAttribute("href", "/companies/acme-aps/periodelas?year=2026");
  });
});
