import { afterEach, describe, expect, test, vi } from "bun:test";
import { cleanup, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { BookkeepingBatchView } from "./BookkeepingBatchView";
import { renderAt } from "../test/render";
import { mockFetch, STATEMENT_FISCAL_YEARS } from "../test/fixtures";

const plan = { planHash: "a".repeat(64), candidateSetHash: "b".repeat(64), items: [{ actionKey: "bank:1", partition: "ready" }] };
const row = { bankTransactionId: 1, date: "2026-01-02", text: "Kontorvarer", amount: -125, currency: "DKK", bankAccount: { id: 3, name: "Driftskonto" }, document: { id: 8, quality: "matched", party: { id: "party-8", name: "Leverandør A/S" }, resolutionState: "resolved" }, proposed: { account: "6100", vatTreatment: "input_vat", dimensions: [] }, status: "ready", nextAction: "Review the exact batch plan", drilldown: { bankTransactionId: 1, documentId: 8, partyId: "party-8", periodClose: { from: "2026-01-01", to: "2026-12-31" } }, sourceHash: "c".repeat(64) };
const workbench = (over: Record<string, unknown> = {}) => ({ state: "available", counts: { ready: 1, suggestedMatch: 0, missingDocument: 0, partyUnresolved: 0, accountingDecisionRequired: 0, vatEvidenceRequired: 0, dimensionEvidenceRequired: 0, stalePlan: 0, applyFailed: 0 }, population: { total: 1, ready: 1, blockers: 0 }, selection: { total: 1, ready: 1, blockers: 0 }, page: { total: 1, nextCursor: null }, completeness: { nextAction: "Review." }, rows: [row], periodClose: { status: "available", blockers: 0 }, plan: { planHash: plan.planHash, candidateSetHash: plan.candidateSetHash, readyCount: 1 }, ...over });
const routes = (over: Record<string, unknown> = {}) => ({ "GET /api/companies/acme-aps/fiscal-years": { fiscalYears: { slug: "acme-aps", years: STATEMENT_FISCAL_YEARS } }, "GET /api/companies/acme-aps/bookkeeping-workbench": { workbench: workbench() }, "GET /api/companies/acme-aps/bookkeeping-batch": { dryRun: true, plan }, ...over });
const renderView = () => renderAt(<BookkeepingBatchView />, { route: "/companies/acme-aps/batchbogfoering", path: "/companies/:slug/batchbogfoering" });

afterEach(cleanup);

describe("BookkeepingBatchView", () => {
  test("loads the selected fiscal period automatically and presents canonical row facts", async () => {
    mockFetch(routes()); renderView();
    await screen.findByRole("link", { name: "Åbn Bank" });
    expect(screen.getByText("Leverandør A/S")).toBeInTheDocument();
    expect(screen.getAllByText("Klar til dry run").length).toBeGreaterThan(0);
    expect(screen.queryByLabelText(/part-id|bankkonto-id/i)).not.toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Åbn Bank" })).toHaveAttribute("href", "/companies/acme-aps/bank?transactionId=1");
  });

  test("keeps preview, persist, approval and apply as separate progressive requests", async () => {
    mockFetch(routes({ "POST /api/companies/acme-aps/bookkeeping-batch/persist": { runId: 7, plan, state: { revisions: [], attempts: [], receipts: [] } }, "POST /api/companies/acme-aps/bookkeeping-batch/approve": { state: { revisions: [{ planHash: plan.planHash, approvedAt: "2026-01-01T00:00:00.000Z" }], attempts: [], receipts: [] } }, "POST /api/companies/acme-aps/bookkeeping-batch/apply": { runId: 7, results: [], checks: [], state: { revisions: [{ planHash: plan.planHash, approvedAt: "2026-01-01T00:00:00.000Z" }], attempts: [], receipts: [] } } })); renderView();
    await screen.findByRole("link", { name: "Åbn Bank" });
    await userEvent.click(screen.getByRole("button", { name: "Forhåndsvis samlet dry run" }));
    await screen.findByText("Samlet dry run forhåndsvist");
    await userEvent.click(screen.getByRole("button", { name: "Gem eksakt plan" }));
    expect(screen.getByRole("button", { name: "Bogfør" })).toBeDisabled();
    await userEvent.click(screen.getByRole("button", { name: "Godkend" }));
    await userEvent.click(screen.getByRole("button", { name: "Bogfør" }));
    await screen.findByText("Kørselsresultat");
    const calls = (globalThis.fetch as any).mock.calls.map((call: any[]) => String(call[0]).replace(/^https?:\/\/[^/]+/, "").split("?")[0]);
    expect(calls.filter((path: string) => path.endsWith("/persist"))).toHaveLength(1);
    expect(calls.filter((path: string) => path.endsWith("/approve"))).toHaveLength(1);
    expect(calls.filter((path: string) => path.endsWith("/apply"))).toHaveLength(1);
  });

  test("explains a whole-period blocker and does not mutate a filtered queue", async () => {
    mockFetch(routes({ "GET /api/companies/acme-aps/bookkeeping-workbench": { workbench: workbench({ counts: { ...workbench().counts, missingDocument: 1 }, population: { total: 2, ready: 1, blockers: 1 } }) } })); renderView();
    await screen.findAllByText(/Hele perioden har 1 afklaringer/);
    expect(screen.getByRole("button", { name: "Gem eksakt plan" })).toBeDisabled();
    await userEvent.selectOptions(screen.getByLabelText("Statusfilter"), "ready");
    expect((globalThis.fetch as any).mock.calls.filter((call: any[]) => String(call[0]).includes("/persist"))).toHaveLength(0);
  });

  test("renders zero, loading, read-error and permission states", async () => {
    mockFetch({ "GET /api/companies/acme-aps/fiscal-years": { fiscalYears: { slug: "acme-aps", years: [] } } }); renderView();
    expect(await screen.findByText("Ingen poster klar til bogføring")).toBeInTheDocument();
  });

  test("keeps initial loading and fiscal-year errors inside #650 evidence root", async () => {
    globalThis.fetch = vi.fn(() => new Promise<Response>(() => {})) as unknown as typeof fetch;
    renderView();
    const loading = document.querySelector('[data-evidence-status="loading"]');
    expect(loading).not.toBeNull();
    if (!loading) throw new Error("expected #650 loading evidence status");
    expect(loading.closest("[data-evidence-issue]")).toHaveAttribute("data-evidence-issue", "650");
    expect(loading).toHaveAttribute("data-evidence-status", "loading");
    cleanup();
    mockFetch({ "GET /api/companies/acme-aps/fiscal-years": { __error: { code: "unavailable", message: "Midlertidig fejl" } } }); renderView();
    const status = await screen.findByText("Bogføringskø kunne ikke hentes", { selector: '[data-evidence-status="error"]' });
    expect(status.closest("[data-evidence-issue]")).toHaveAttribute("data-evidence-issue", "650");
    expect(status).toHaveAttribute("data-evidence-status", "error");
    cleanup();
    mockFetch({ "GET /api/companies/acme-aps/fiscal-years": { __error: { code: "forbidden", message: "403 adgang nægtet" } } }); renderView();
    const blocked = await screen.findByText("Bogføring kræver afklaring", { selector: '[data-evidence-status="warning-or-blocked"]' });
    expect(blocked.closest("[data-evidence-issue]")).toHaveAttribute("data-evidence-issue", "650");
  });

  test("does not advance on mutation failure and surfaces the failure", async () => {
    mockFetch(routes({ "POST /api/companies/acme-aps/bookkeeping-batch/persist": { __error: { code: "forbidden", message: "Ingen adgang til at gemme planen" } } })); renderView();
    await screen.findByRole("link", { name: "Åbn Bank" }); await userEvent.click(screen.getByRole("button", { name: "Gem eksakt plan" }));
    await screen.findByRole("alert");
    expect(screen.getByRole("button", { name: "Godkend" })).toBeDisabled();
    expect(screen.queryByText("Varig historik")).not.toBeInTheDocument();
  });

  test("restores the durable run from a runId drilldown", async () => {
    const restored = { ...plan, scope: { accountingFrom: "2026-01-01", accountingTo: "2026-12-31", bankFrom: "2026-01-01", bankTo: "2026-12-31" } };
    mockFetch({ ...routes(), "GET /api/companies/acme-aps/bookkeeping-batch/runs/7": { state: { run: { runId: 7, plan: JSON.stringify(restored) }, revisions: [{ planHash: plan.planHash, approvedAt: "2026-01-01T00:00:00.000Z" }], attempts: [], receipts: [] } } });
    renderAt(<BookkeepingBatchView />, { route: "/companies/acme-aps/batchbogfoering?runId=7", path: "/companies/:slug/batchbogfoering" });
    await screen.findByText("Varig historik"); expect(screen.getByText(/Revisioner: 1/)).toBeInTheDocument();
  });
});

test("a restored approval only enables apply for the exact plan hash", async () => {
  mockFetch({
    ...routes(),
    "GET /api/companies/acme-aps/bookkeeping-batch/runs/7": { state: {
      run: { plan: JSON.stringify({ ...plan, bankFrom: "2026-01-01", bankTo: "2026-12-31" }) },
      revisions: [{ planHash: "b".repeat(64), approvedAt: "2026-01-31T10:00:00Z" }], attempts: [], receipts: [],
    } },
  });
  renderAt(<BookkeepingBatchView />, { route: "/companies/acme-aps/batchbogfoering?runId=7", path: "/companies/:slug/batchbogfoering" });
  await screen.findByText("Varig historik");
  expect(screen.getByRole("button", { name: "Bogfør" })).toBeDisabled();
  expect(screen.getByText("Kanonisk periode: 2026-01-01 – 2026-12-31")).toBeInTheDocument();
});

test("changing the canonical year invalidates a restored approved plan", async () => {
  mockFetch({
    ...routes(),
    "GET /api/companies/acme-aps/bookkeeping-batch/runs/7": { state: {
      run: { plan: JSON.stringify({ ...plan, bankFrom: "2026-01-01", bankTo: "2026-12-31" }) },
      revisions: [{ planHash: plan.planHash, approvedAt: "2026-01-31T10:00:00Z" }], attempts: [], receipts: [],
    } },
  });
  renderAt(<BookkeepingBatchView />, { route: "/companies/acme-aps/batchbogfoering?runId=7", path: "/companies/:slug/batchbogfoering" });
  await screen.findByText("Varig historik");
  expect(screen.getByRole("button", { name: "Bogfør" })).toBeEnabled();
  await userEvent.selectOptions(screen.getByLabelText("Vælg regnskabsår"), "2025");
  expect(screen.getByRole("button", { name: "Bogfør" })).toBeDisabled();
  expect(screen.queryByText("Varig historik")).not.toBeInTheDocument();
});


for (const code of ["network", "internal"]) test(`batch apply remains blocked after ${code}, including a successful status read`, async () => {
  const restored = { ...plan, bankFrom: "2026-01-01", bankTo: "2026-12-31" };
  const durable = { run: { plan: JSON.stringify(restored) }, revisions: [{ planHash: plan.planHash, approvedAt: "2026-01-31T10:00:00Z" }], attempts: [], receipts: [] };
  mockFetch({ ...routes(), "GET /api/companies/acme-aps/bookkeeping-batch/runs/7": { state: durable }, "POST /api/companies/acme-aps/bookkeeping-batch/apply": { __error: { code, message: "Synthetic interrupted response" } } });
  renderAt(<BookkeepingBatchView />, { route: "/companies/acme-aps/batchbogfoering?runId=7", path: "/companies/:slug/batchbogfoering" });
  await screen.findByText("Varig historik");
  const write = screen.getByRole("button", { name: "Bogfør" });
  await userEvent.click(write);
  expect(await screen.findByText(/Serverens resultat kunne ikke bekræftes/)).toBeInTheDocument();
  expect(write).toBeDisabled();
  await userEvent.click(screen.getByRole("button", { name: "Kontrollér status" }));
  await userEvent.click(write);
  expect(write).toBeDisabled();
  expect(screen.getByText("Kanonisk periode: 2026-01-01 – 2026-12-31")).toBeInTheDocument();
  const writes = (fetch as unknown as ReturnType<typeof vi.fn>).mock.calls.filter(([url, init]) => String(url).endsWith("/apply") && init?.method === "POST");
  expect(writes).toHaveLength(1);
  expect(JSON.parse(String((writes[0]![1] as RequestInit).body))).toMatchObject({ runId: 7, planHash: plan.planHash });
});


test("a read-only batch load failure never becomes an unknown mutation", async () => {
  mockFetch({ ...routes(), "GET /api/companies/acme-aps/bookkeeping-workbench": { __error: { code: "network", message: "Synthetic read failure" } }, "GET /api/companies/acme-aps/bookkeeping-batch": { dryRun: true, plan } });
  renderAt(<BookkeepingBatchView />, { route: "/companies/acme-aps/batchbogfoering?from=2026-01-01&to=2026-01-31", path: "/companies/:slug/batchbogfoering" });

  expect(await screen.findByText("Synthetic read failure")).toBeInTheDocument();
  expect(screen.queryByText(/Serverens resultat kunne ikke bekræftes/)).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Forhåndsvis samlet dry run" })).toBeEnabled();
});
