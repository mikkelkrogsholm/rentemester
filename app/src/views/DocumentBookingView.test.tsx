import { describe, expect, test, vi } from "bun:test";
import { screen } from "@testing-library/react";
import { DocumentBookingView } from "./DocumentBookingView";
import { DocumentDetailView } from "./DocumentsView";
import { renderAt } from "../test/render";
import { documents, mockFetch } from "../test/fixtures";

const years = { slug: "acme-aps", years: [
  { label: "2026", start: "2026-01-01", end: "2026-12-31", source: "live" },
  { label: "2025", start: null, end: null, source: "archive" },
] };
function routes(booked = false) {
  const row = documents().documents[0]!;
  return {
    "GET /api/companies/acme-aps/documents": { documents: documents({ documents: [{ ...row, journalEntryNo: booked ? row.journalEntryNo : null, journalEntryId: booked ? row.journalEntryId : null }] }) },
    "GET /api/companies/acme-aps/fiscal-years": { fiscalYears: years },
    "GET /api/companies/acme-aps/documents/party-links": { links: [] },
    "GET /api/companies/acme-aps/documents/1/booking-options": { options: { document: { ...row, vatAmount: 250, purchaseVatLines: [] }, expenseAccounts: [{ accountNo: "3000", name: "Kontorartikler", defaultVatCode: "DK_PURCHASE_25" }], unmatchedOutgoingBank: [{ id: 9, date: "2026-02-01", text: "Leverandør", amount: -1250, currency: "DKK", amountDkk: -1250, fxRateToDkk: 1, reference: "REF-1" }] } },
  };
}
function renderBooking(query = "", id = "1") {
  return renderAt(<DocumentBookingView />, { route: `/companies/acme-aps/bilag/${id}/bogfoer${query}`, path: "/companies/:slug/bilag/:documentId/bogfoer" });
}

describe("DocumentBookingView", () => {
  test("uses a full page and preserves safe list context", async () => {
    mockFetch(routes());
    renderBooking("?year=2026&returnTo=%2Fcompanies%2Facme-aps%2Fbilag%3Fyear%3D2026%26status%3Dunbooked");
    expect(await screen.findByRole("heading", { name: "Bogfør bilag", level: 1 })).toBeInTheDocument();
    expect(await screen.findByLabelText("Udgiftskonto")).toBeInTheDocument();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Tilbage til bilag" })).toHaveAttribute("href", "/companies/acme-aps/bilag?year=2026&status=unbooked");
    expect(screen.getByRole("link", { name: "Åbn bilagsfil" })).toHaveAttribute("href", "/api/companies/acme-aps/documents/1/file");
  });
  test("an archived year cannot load a booking form", async () => {
    mockFetch(routes());
    renderBooking("?year=2025");
    expect(await screen.findByRole("heading", { name: "Regnskabsåret er arkiveret" })).toBeInTheDocument();
    expect(screen.queryByLabelText("Udgiftskonto")).not.toBeInTheDocument();
    const calls = (globalThis.fetch as unknown as ReturnType<typeof vi.fn>).mock.calls;
    expect(calls.some((call) => String(call[0]).includes("booking-options"))).toBe(false);
  });
  test("an already booked document is read-only in the booking route", async () => {
    mockFetch(routes(true));
    renderBooking();
    expect(await screen.findByRole("heading", { name: "Bilaget er allerede bogført" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Bogfør" })).not.toBeInTheDocument();
  });
  test("invalid and missing document IDs do not invoke booking options", async () => {
    mockFetch(routes());
    renderBooking("", "invalid");
    expect(await screen.findByRole("heading", { name: "Bilaget findes ikke" })).toBeInTheDocument();
    const calls = (globalThis.fetch as unknown as ReturnType<typeof vi.fn>).mock.calls;
    expect(calls.some((call) => String(call[0]).includes("booking-options"))).toBe(false);
  });
});

describe("DocumentDetailView", () => {
  test("invalid identifiers never show unrelated documents", async () => {
    mockFetch(routes());
    renderAt(<DocumentDetailView />, { route: "/companies/acme-aps/bilag/invalid", path: "/companies/:slug/bilag/:documentId" });
    expect(await screen.findByRole("heading", { name: "Bilaget findes ikke" })).toBeInTheDocument();
    expect(screen.queryByText("DOC-2026-000001")).not.toBeInTheDocument();
    expect(screen.queryByRole("link", { name: "Bogfør bilag" })).not.toBeInTheDocument();
  });
});
