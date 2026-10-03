import { beforeEach, describe, expect, test, vi } from "bun:test";
import { fireEvent, screen } from "@testing-library/react";
import { renderAt } from "../test/render";

const companies = vi.fn();
const cfoAnalytics = vi.fn();
const groupReportProfiles = vi.fn();
vi.mock("../lib/api", () => ({ api: { companies, cfoAnalytics, groupReportProfiles } }));

import { CfoCockpitView } from "./CfoCockpitView";

const companyAnalytics = {
  ok: true as const, schemaVersion: "rentemester-cfo-analytics-v1", scope: "company" as const,
  status: "ready" as const, asOf: "2026-08-30", from: "2026-01-01", to: "2026-08-30",
  companies: ["alpha"], partial: false, mode: "legal-company" as const, aggregate: "none" as const,
  limitations: [], page: { limit: 100, nextCursor: null }, freshness: [{ source: "ledger" as const, companySlug: "alpha", latestTransactionDate: "2026-08-29" }],
  evidenceCompleteness: [{ companySlug: "alpha", status: "ready" as const, postedWithoutDocument: 2, openExceptions: 1 }],
  reconciliation: { rowCount: 1, amountByCurrency: { DKK: 250 }, sourceHashes: ["a".repeat(64)], method: "sum" },
  rows: [{ companySlug: "alpha", sourceType: "ledger" as const, sourceHash: "a".repeat(64), sourceId: "journal:1:line:1", journalEntryId: 1, journalEntryNo: "J-1", documentId: 7, documentHash: "b".repeat(64), partyId: null, partyName: "Syntetisk leverandør", accountNo: "4200", accountName: "Køb", transactionDate: "2026-08-29", currency: "DKK", amount: 250, text: "Syntetisk køb" }],
};

describe("CfoCockpitView", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    companies.mockResolvedValue([{ slug: "alpha", name: "Alpha ApS", archived: false }]);
    cfoAnalytics.mockResolvedValue(companyAnalytics);
    groupReportProfiles.mockResolvedValue({ profiles: [] });
  });

  test("renders a source-linked company view with scope, as-of, freshness and authorized drill-downs", async () => {
    renderAt(<CfoCockpitView />);
    expect(await screen.findByRole("heading", { name: "CFO-overblik" })).toBeInTheDocument();
    expect(await screen.findByText("Virksomhed: Alpha ApS")).toBeInTheDocument();
    expect(screen.getByText(/Pr\. 2026-08-30/)).toBeInTheDocument();
    expect(screen.getByText(/Seneste ledger: 2026-08-29/)).toBeInTheDocument();
    expect(screen.getByText(/Stale-status kan ikke udledes/)).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Bank" })).toHaveAttribute("href", "/companies/alpha/bank");
    expect(screen.getByRole("link", { name: "Postering J-1" })).toHaveAttribute("href", "/companies/alpha/posteringer?account=4200");
    expect(cfoAnalytics).toHaveBeenCalledWith(expect.objectContaining({ scope: "company", companySlug: "alpha" }), expect.objectContaining({ signal: expect.anything() }));
  });

  test("labels portfolio as non-consolidated and never shows a hidden aggregate", async () => {
    cfoAnalytics.mockResolvedValue({ ...companyAnalytics, scope: "portfolio", status: "incomplete", partial: true, mode: "juxtaposed-non-consolidated", companies: ["alpha"], reconciliation: { rowCount: 1, sourceHashes: [], method: "sum", omitted: "access is partial" } });
    renderAt(<CfoCockpitView />);
    fireEvent.change(await screen.findByLabelText("Visning"), { target: { value: "portfolio" } });
    expect(await screen.findByText("Portefølje — ikke konsolideret")).toBeInTheDocument();
    expect(screen.getByText(/Ufuldstændigt udsnit/)).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Analyseret bevægelse" })).not.toBeInTheDocument();
  });

  test("shows unsupported group reports as blockers, not zero totals", async () => {
    cfoAnalytics.mockResolvedValue({ ok: true, schemaVersion: "rentemester-cfo-analytics-v1", scope: "group", status: "unsupported", asOf: "2026-08-30", limitations: ["blocked"], group: { status: "blocked", blockers: ["approved profile is incomplete"] } });
    groupReportProfiles.mockResolvedValue({ profiles: [{ id: "group-1", currency: "DKK" }] });
    renderAt(<CfoCockpitView />);
    fireEvent.change(await screen.findByLabelText("Visning"), { target: { value: "group" } });
    expect(await screen.findByText("Koncernrapporten understøttes ikke for dette udsnit.")).toBeInTheDocument();
    expect(screen.getByText("approved profile is incomplete")).toBeInTheDocument();
    expect(screen.queryByText("0,00 kr.")).not.toBeInTheDocument();
  });

  test("keeps an empty source result distinct from zero", async () => {
    cfoAnalytics.mockResolvedValue({ ...companyAnalytics, rows: [], reconciliation: { ...companyAnalytics.reconciliation, rowCount: 0, amountByCurrency: {} }, freshness: [] });
    renderAt(<CfoCockpitView />);
    expect(await screen.findByText("Ingen kildeposteringer i den valgte periode.")).toBeInTheDocument();
    expect(screen.queryByText("0,00 kr.")).not.toBeInTheDocument();
    expect(screen.getByText(/2 bogførte poster uden bilag · 1 åbne undtagelser/)).toBeInTheDocument();
  });
});
