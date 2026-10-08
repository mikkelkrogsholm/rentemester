import { MemoryRouter } from "react-router-dom";
import { beforeEach, describe, expect, test, vi } from "bun:test";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";

const groupOverview = vi.fn();
const groupReconciliation = vi.fn();
const groupEliminations = vi.fn();
const groupReportProfiles = vi.fn();
const groupConsolidatedReport = vi.fn();
const intercompanyDispositionStatus = vi.fn();
vi.mock("../lib/api", () => ({ api: {
  groupOverview, groupReconciliation, groupEliminations,
  groupReportProfiles, groupConsolidatedReport, intercompanyDispositionStatus,
} }));

import { GroupOverviewView } from "./GroupOverviewView";

const overview = {
  ok: true as const,
  scope: "structure-status-only" as const,
  consolidationStatus: "not-available" as const,
  consolidatedFigures: null,
  rawCompanySums: null,
  blockers: ["consolidated reports are not available"],
  manifestStatus: "blocked" as const,
  asOf: "2026-08-23",
  groups: [{
    partial: true, readiness: "blocked" as const,
    visibleMemberships: [{ id: "member-1", companySlug: "visible-aps", validFrom: "2026-01-01", archived: true }],
    visibleOwnership: [{ id: "owner-1", parentCompanySlug: "visible-aps", childCompanySlug: "child-aps", basisPoints: 7500, validFrom: "2026-01-01", evidenceRefs: ["evidence-2026-01"] }],
    blockers: ["group structure is partial for this user"],
  }],
};

describe("GroupOverviewView", () => {
  beforeEach(() => { vi.clearAllMocks(); groupOverview.mockResolvedValue(overview); groupReconciliation.mockResolvedValue({ ok: true, scope: "intercompany-reconciliation", asOf: "2026-08-23", rows: [{ mappingId: "visible-map", left: { companySlug: "visible-aps", balance: 100, currency: "DKK" }, right: { companySlug: "child-aps", balance: 95, currency: "DKK" }, difference: 5, status: "mismatch", reason: "exact-native-currency-difference", blockers: [] }] }); groupEliminations.mockResolvedValue({ ok: true, scope: "consolidation-eliminations", asOf: "2026-08-23", rows: [] }); groupReportProfiles.mockImplementation(async (asOf: string) => ({ ok: true, scope: "consolidation-report-profiles", asOf, profiles: [] })); });

  test("sends an explicit YYYY-MM-DD asOf and renders structure without hidden identities or figures", async () => {
    render(<MemoryRouter><GroupOverviewView /></MemoryRouter>);
    expect(await screen.findByRole("heading", { name: "Koncernstruktur" })).toBeInTheDocument();
    expect(groupOverview).toHaveBeenCalledWith(expect.stringMatching(/^\d{4}-\d{2}-\d{2}$/), expect.objectContaining({ signal: expect.anything() }));
    expect(screen.getByText("Strukturvisning")).toBeInTheDocument();
    expect(screen.getByText("Arkiveret")).toBeInTheDocument();
    expect(screen.getByText(/evidence:/)).toHaveTextContent("evidence-2026-01");
    expect(screen.getByText("Delvist synlig koncernstruktur")).toBeInTheDocument();
    expect(screen.queryByText(/hidden-secret|1000|kr\.|7500/i)).not.toBeInTheDocument();
    expect(screen.getAllByLabelText("Blokeringer")[0]).toHaveTextContent("consolidated reports are not available");
    expect(screen.getByLabelText("Mellemregningsafstemning")).toHaveTextContent("Difference");
    expect(groupReconciliation).toHaveBeenCalledWith(expect.stringMatching(/^\d{4}-\d{2}-\d{2}$/), expect.objectContaining({ signal: expect.anything() }));
    expect(groupEliminations).toHaveBeenCalledWith(expect.stringMatching(/^\d{4}-\d{2}-\d{2}$/), expect.objectContaining({ signal: expect.anything() }));
  });

  test("changes the date through the explicit asOf request", async () => {
    render(<MemoryRouter><GroupOverviewView /></MemoryRouter>);
    await screen.findByRole("heading", { name: "Koncernstruktur" });
    fireEvent.change(screen.getByLabelText("Pr. dato"), { target: { value: "2026-01-31" } });
    await waitFor(() => expect(groupOverview).toHaveBeenLastCalledWith("2026-01-31", expect.objectContaining({ signal: expect.anything() })));
    await waitFor(() => expect(groupReconciliation).toHaveBeenLastCalledWith("2026-01-31", expect.objectContaining({ signal: expect.anything() })));
    await waitFor(() => expect(groupEliminations).toHaveBeenLastCalledWith("2026-01-31", expect.objectContaining({ signal: expect.anything() })));
  });

  test("shows a disposition lifecycle response and fails closed on a server denial", async () => {
    intercompanyDispositionStatus.mockResolvedValue({ status: "partly_posted", exceptions: [{ kind: "one_sided_posting" }] });
    render(<MemoryRouter><GroupOverviewView /></MemoryRouter>);
    await screen.findByRole("heading", { name: "Intercompany dispositioner" });
    fireEvent.change(screen.getByLabelText("Disposition-ID"), { target: { value: "synthetic-disposition" } });
    fireEvent.click(screen.getByRole("button", { name: "Vis status" }));
    expect(await screen.findByText("one_sided_posting")).toBeInTheDocument();
    expect(intercompanyDispositionStatus).toHaveBeenCalledWith("synthetic-disposition", expect.stringMatching(/^\d{4}-\d{2}-\d{2}$/), expect.objectContaining({ signal: expect.anything() }));
    intercompanyDispositionStatus.mockRejectedValueOnce(new Error("unauthorized"));
    fireEvent.click(screen.getByRole("button", { name: "Vis status" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Dispositionen kan ikke vises med din aktuelle adgang.");
  });

  test("fails closed on a partial/unsafe group contract", async () => {
    groupOverview.mockResolvedValue({ ...overview, consolidatedFigures: { forbidden: 1 } });
    render(<MemoryRouter><GroupOverviewView /></MemoryRouter>);
    expect(await screen.findByRole("alert")).toHaveTextContent("Koncernstrukturen kan ikke vises sikkert");
    expect(screen.queryByText("Syntetisk koncern")).not.toBeInTheDocument();
  });

  test("fails closed on API errors without leaving structure visible", async () => {
    groupOverview.mockRejectedValue(new Error("unauthorized"));
    render(<MemoryRouter><GroupOverviewView /></MemoryRouter>);
    expect(await screen.findByRole("alert")).toHaveTextContent("Koncernstrukturen kan ikke vises sikkert");
    expect(screen.queryByText("Syntetisk koncern")).not.toBeInTheDocument();
  });

  test("fails closed when reconciliation cannot be authorized", async () => {
    groupReconciliation.mockRejectedValue(new Error("unauthorized"));
    render(<MemoryRouter><GroupOverviewView /></MemoryRouter>);
    expect(await screen.findByRole("alert")).toHaveTextContent("Koncernstrukturen kan ikke vises sikkert");
    expect(screen.queryByLabelText("Mellemregningsafstemning")).not.toBeInTheDocument();
  });

  test("does not render structure if elimination evidence fails", async () => {
    groupEliminations.mockRejectedValue(new Error("tampered"));
    render(<MemoryRouter><GroupOverviewView /></MemoryRouter>);
    expect(await screen.findByRole("alert")).toHaveTextContent("Koncernstrukturen kan ikke vises sikkert");
    expect(screen.queryByLabelText("Elimineringer")).not.toBeInTheDocument();
  });

  test("renders one approved read-only consolidated report with traceable source evidence", async () => {
    groupReportProfiles.mockImplementation(async (asOf: string) => ({
      ok: true, scope: "consolidation-report-profiles", asOf,
      profiles: [{ id: "approved-profile", groupId: "group-1", currency: "DKK", validFrom: "2026-01-01" }],
    }));
    groupConsolidatedReport.mockResolvedValue({
      ok: true, scope: "consolidated-report", status: "ready",
      profileId: "approved-profile", groupId: "group-1", currency: "DKK",
      period: { from: "2026-01-01", to: "2026-08-23" }, blockers: [],
      rawCompanySums: [], appliedEliminations: [],
      consolidatedFigures: [{
        lineId: "assets", label: "Aktiver", section: "asset",
        rawCompanySum: 125, eliminationAdjustment: -25, consolidatedAmount: 100,
      }],
      sourceSnapshots: [{ companySlug: "visible-aps", ledgerHeadHash: "a".repeat(64), entryCount: 12 }],
    });
    render(<MemoryRouter><GroupOverviewView /></MemoryRouter>);
    expect(await screen.findByRole("heading", { name: "Konsolideret rapport" })).toBeInTheDocument();
    expect(await screen.findByText("Kontrolleret koncernrapport")).toBeInTheDocument();
    expect(screen.getByLabelText("Konsolideret rapport")).toHaveTextContent("Aktiver");
    expect(screen.getByLabelText("Konsolideret rapport")).toHaveTextContent("100,00 kr");
    expect(screen.getByText(/ledger-head/)).toHaveTextContent("aaaaaaaaaaaaaaaa");
    expect(groupConsolidatedReport).toHaveBeenCalledWith(
      "approved-profile", expect.stringMatching(/^\d{4}-01-01$/), expect.stringMatching(/^\d{4}-\d{2}-\d{2}$/), expect.objectContaining({ signal: expect.anything() }),
    );
  });

  test("fails closed if the consolidated report endpoint fails", async () => {
    groupReportProfiles.mockImplementation(async (asOf: string) => ({
      ok: true, scope: "consolidation-report-profiles", asOf,
      profiles: [{ id: "approved-profile", groupId: "group-1", currency: "DKK", validFrom: "2026-01-01" }],
    }));
    groupConsolidatedReport.mockRejectedValue(new Error("tampered"));
    render(<MemoryRouter><GroupOverviewView /></MemoryRouter>);
    expect(await screen.findByRole("alert")).toHaveTextContent("Koncernstrukturen kan ikke vises sikkert");
    expect(screen.queryByLabelText("Konsolideret rapport")).not.toBeInTheDocument();
  });
});
