import { afterEach, describe, expect, test, vi } from "bun:test";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { PurchaseOverviewView } from "./PurchaseOverviewView";
import { mockFetch } from "../test/fixtures";
import { renderAt } from "../test/render";
import { restoreGlobals } from "../test/globals";

const overview = {
  basis: { canonical: { postedCaseCount: 0, unpostedCaseCount: 0, economicEffect: { expense: 0, result: 0, basis: "posted_ledger" } }, provisional: { unresolvedDocumentationCount: 0, economicEffect: { expense: 0, expectedVat: 0, status: "projection_not_filing_ready", effects: [] } } },
  groups: [],
};

afterEach(() => restoreGlobals());

describe("PurchaseOverviewView", () => {
  test("opens an unresolved source-bound case only after explicit confirmation", async () => {
    mockFetch({
      "GET /api/companies/acme-aps/purchase-overview": { overview },
      "GET /api/companies/acme-aps/purchase-cases": { purchaseCases: [] },
      "GET /api/companies/acme-aps/accounting-approval-policy": { policy: null },
      "POST /api/companies/acme-aps/purchase-cases": { purchaseCase: { id: "case-1" } },
    });
    renderAt(<PurchaseOverviewView />, { route: "/companies/acme-aps/koebsoverblik", path: "/companies/:slug/koebsoverblik" });
    await screen.findByText("Ingen åbne grupper i perioden.");
    await userEvent.selectOptions(screen.getByLabelText("Kildetype"), "bank_transaction");
    await userEvent.type(screen.getByLabelText("Kilde-id"), "7");
    await userEvent.click(screen.getByRole("button", { name: "Åbn case" }));
    expect((globalThis.fetch as unknown as ReturnType<typeof vi.fn>).mock.calls.some(([, init]) => init?.method === "POST")).toBe(false);
    const dialog = await screen.findByRole("dialog", { name: "Åbn foreløbig købscase" });
    await userEvent.click(dialog.querySelector<HTMLButtonElement>("button.btn:not(.secondary)")!);
    const call = (globalThis.fetch as unknown as ReturnType<typeof vi.fn>).mock.calls.find(([url, init]) => String(url).endsWith("/purchase-cases") && init?.method === "POST");
    expect(JSON.parse(String((call![1] as RequestInit).body))).toMatchObject({ source: { kind: "bank_transaction", id: 7 }, documentationOutcome: "unresolved", confirm: true });
  });

  test("shows the provisional projection as not filing-ready and keeps it optional", async () => {
    mockFetch({
      "GET /api/companies/acme-aps/purchase-overview": { overview: { ...overview, basis: { ...overview.basis, provisional: { ...overview.basis.provisional, economicEffect: { expense: 1000, expectedVat: 250, status: "projection_not_filing_ready", effects: [] } } } } },
      "GET /api/companies/acme-aps/purchase-cases": { purchaseCases: [{ caseId: "purchase-1", version: 1, source: { kind: "document", id: 7 }, sourceFingerprint: "a".repeat(64), documentationOutcome: "unresolved", accountingProgress: "unposted", vatEvidence: { status: "pending" }, note: "", eventHash: "b".repeat(64), createdAt: "2026-01-01T00:00:00.000Z", sourceStatus: { status: "current", currentSourceFingerprint: "a".repeat(64) } }] },
      "GET /api/companies/acme-aps/accounting-approval-policy": { policy: null },
    });
    renderAt(<PurchaseOverviewView />, { route: "/companies/acme-aps/koebsoverblik", path: "/companies/:slug/koebsoverblik" });
    expect(await screen.findByText("Ikke klar til momsindberetning.")).toBeTruthy();
    expect(screen.getByText("Bilag #7")).toBeTruthy();
    await userEvent.click(screen.getByLabelText("Vis foreløbig effekt"));
    expect(await screen.findAllByText("Slået fra")).toHaveLength(2);
  });
});
