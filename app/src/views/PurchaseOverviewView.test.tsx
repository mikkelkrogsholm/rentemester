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
const purchaseCase = { caseId: "purchase-1", version: 1, source: { kind: "document" as const, id: 7 }, sourceFingerprint: "a".repeat(64), documentationOutcome: "unresolved" as const, accountingProgress: "unposted" as const, vatEvidence: { status: "pending" }, note: "", eventHash: "b".repeat(64), createdAt: "2026-01-01T00:00:00.000Z", sourceFact: { date: "2026-01-01", supplier: null, amount: 1250, currency: "DKK", documentId: 7 }, sourceStatus: { status: "current" as const, currentSourceFingerprint: "a".repeat(64) } };

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

  test("prefills a readable source selected from an existing context link", async () => {
    mockFetch({
      "GET /api/companies/acme-aps/purchase-overview": { overview },
      "GET /api/companies/acme-aps/purchase-cases": { purchaseCases: [] },
      "GET /api/companies/acme-aps/accounting-approval-policy": { policy: null },
      "POST /api/companies/acme-aps/purchase-cases": { purchaseCase: { id: "case-1" } },
    });
    renderAt(<PurchaseOverviewView />, { route: "/companies/acme-aps/koebsoverblik?sourceKind=document&sourceId=7", path: "/companies/:slug/koebsoverblik" });
    expect(await screen.findByText("Valgt kilde: Bilag #7")).toBeTruthy();
    expect(screen.queryByLabelText("Kilde-id")).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Åbn case" }));
    const dialog = await screen.findByRole("dialog", { name: "Åbn foreløbig købscase" });
    await userEvent.click(dialog.querySelector<HTMLButtonElement>("button.btn:not(.secondary)")!);
    const call = (globalThis.fetch as unknown as ReturnType<typeof vi.fn>).mock.calls.find(([url, init]) => String(url).endsWith("/purchase-cases") && init?.method === "POST");
    expect(JSON.parse(String((call![1] as RequestInit).body))).toMatchObject({ source: { kind: "document", id: 7 }, confirm: true });
  });

  test("shows the provisional projection as not filing-ready and keeps it optional", async () => {
    mockFetch({
      "GET /api/companies/acme-aps/purchase-overview": { overview: { ...overview, basis: { ...overview.basis, provisional: { ...overview.basis.provisional, economicEffect: { expense: 1000, expectedVat: 250, status: "projection_not_filing_ready", effects: [] } } } } },
      "GET /api/companies/acme-aps/purchase-cases": { purchaseCases: [purchaseCase] },
      "GET /api/companies/acme-aps/accounting-approval-policy": { policy: null },
    });
    renderAt(<PurchaseOverviewView />, { route: "/companies/acme-aps/koebsoverblik", path: "/companies/:slug/koebsoverblik" });
    expect(await screen.findByText("Ikke klar til momsindberetning.")).toBeTruthy();
    expect(screen.getByText("Bilag #7")).toBeTruthy();
    expect(screen.getByText(/Leverandør ukendt/)).toBeTruthy();
    await userEvent.click(screen.getByLabelText("Vis foreløbig effekt"));
    expect(await screen.findAllByText("Slået fra")).toHaveLength(2);
  });

  test("reviews one current case only after reading its exact current version", async () => {
    mockFetch({
      "GET /api/companies/acme-aps/purchase-overview": { overview },
      "GET /api/companies/acme-aps/purchase-cases": { purchaseCases: [purchaseCase] },
      "GET /api/companies/acme-aps/purchase-cases/purchase-1": { purchaseCase },
      "GET /api/companies/acme-aps/accounting-approval-policy": { policy: null },
      "POST /api/companies/acme-aps/purchase-cases/purchase-1/review": { purchaseCase },
    });
    renderAt(<PurchaseOverviewView />, { route: "/companies/acme-aps/koebsoverblik", path: "/companies/:slug/koebsoverblik" });
    await screen.findByText("Review almindeligt bilag");
    await userEvent.click(screen.getByRole("button", { name: "Review almindeligt bilag" }));
    const dialog = await screen.findByRole("dialog", { name: "Review købscase" });
    await userEvent.click(dialog.querySelector<HTMLButtonElement>("button.btn:not(.secondary)")!);
    const call = (globalThis.fetch as unknown as ReturnType<typeof vi.fn>).mock.calls.find(([url, init]) => String(url).endsWith("/purchase-1/review") && init?.method === "POST");
    expect(JSON.parse(String((call![1] as RequestInit).body))).toMatchObject({ expectedVersion: 1, expectedSourceFingerprint: "a".repeat(64), documentationOutcome: "ordinary_evidence_sufficient", confirm: true });
  });

  test("requires a reason and sends the fresh fingerprint for stale reassessment", async () => {
    const stale = { ...purchaseCase, sourceStatus: { status: "stale" as const, currentSourceFingerprint: "c".repeat(64) } };
    mockFetch({
      "GET /api/companies/acme-aps/purchase-overview": { overview },
      "GET /api/companies/acme-aps/purchase-cases": { purchaseCases: [stale] },
      "GET /api/companies/acme-aps/purchase-cases/purchase-1": { purchaseCase: stale },
      "GET /api/companies/acme-aps/accounting-approval-policy": { policy: null },
      "POST /api/companies/acme-aps/purchase-cases/purchase-1/reassess": { purchaseCase: stale },
    });
    renderAt(<PurchaseOverviewView />, { route: "/companies/acme-aps/koebsoverblik", path: "/companies/:slug/koebsoverblik" });
    await screen.findByText("Genvurdér ændret kilde");
    await userEvent.click(screen.getByRole("button", { name: "Genvurdér ændret kilde" }));
    const dialog = await screen.findByRole("dialog", { name: "Genvurdér ændret købskilde" });
    await userEvent.type(screen.getByLabelText("Begrundelse for genvurdering"), "Kilden er opdateret");
    await userEvent.click(dialog.querySelector<HTMLButtonElement>("button.btn:not(.secondary)")!);
    const call = (globalThis.fetch as unknown as ReturnType<typeof vi.fn>).mock.calls.find(([url, init]) => String(url).endsWith("/purchase-1/reassess") && init?.method === "POST");
    expect(JSON.parse(String((call![1] as RequestInit).body))).toMatchObject({ expectedVersion: 1, expectedSourceFingerprint: "a".repeat(64), currentSourceFingerprint: "c".repeat(64), reason: "Kilden er opdateret", confirm: true });
  });

  test("group review sends only the concretely selected members", async () => {
    const member = { caseId: "purchase-1", version: 1, source: purchaseCase.source, sourceFingerprint: "a".repeat(64), documentationOutcome: "unresolved", accountingProgress: "unposted", vatEvidence: { status: "pending" }, sourceFact: purchaseCase.sourceFact, sourceStatus: purchaseCase.sourceStatus, need: { key: "documentation:unresolved", question: "Review" } };
    const secondMember = { ...member, caseId: "purchase-2", version: 2, source: { kind: "document" as const, id: 8 }, sourceFingerprint: "c".repeat(64), sourceFact: { date: "2026-01-02", supplier: "Leverandør A", amount: 250, currency: "DKK", documentId: 8 } };
    mockFetch({
      "GET /api/companies/acme-aps/purchase-overview": { overview: { ...overview, groups: [{ need: { key: "documentation:unresolved", question: "Review" }, caseCount: 2, members: [member, secondMember], selectionHash: "selection-1" }] } },
      "GET /api/companies/acme-aps/purchase-cases": { purchaseCases: [purchaseCase] },
      "GET /api/companies/acme-aps/accounting-approval-policy": { policy: null },
      "POST /api/companies/acme-aps/purchase-cases/group-review": { group: { groupId: "group-1" } },
    });
    renderAt(<PurchaseOverviewView />, { route: "/companies/acme-aps/koebsoverblik", path: "/companies/:slug/koebsoverblik" });
    await screen.findByRole("button", { name: "Review 2 valgte" });
    await userEvent.click(screen.getAllByLabelText("Vælg")[0]!);
    await userEvent.click(screen.getByRole("button", { name: "Review 1 valgt" }));
    const dialog = await screen.findByRole("dialog", { name: "Review købsdokumentation" });
    await userEvent.click(dialog.querySelector<HTMLButtonElement>("button.btn:not(.secondary)")!);
    const call = (globalThis.fetch as unknown as ReturnType<typeof vi.fn>).mock.calls.find(([url, init]) => String(url).endsWith("/group-review") && init?.method === "POST");
    expect(JSON.parse(String((call![1] as RequestInit).body)).members).toEqual([{ caseId: "purchase-2", expectedVersion: 2, expectedSourceFingerprint: "c".repeat(64) }]);
  });
});
