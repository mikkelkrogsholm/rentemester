import { afterEach, describe, expect, test } from "bun:test";
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
    mockFetch(routes({ "POST /api/companies/acme-aps/bookkeeping-batch/persist": { runId: 7, plan, state: { revisions: [], attempts: [], receipts: [] } }, "POST /api/companies/acme-aps/bookkeeping-batch/approve": { state: { revisions: [{}], attempts: [], receipts: [] } }, "POST /api/companies/acme-aps/bookkeeping-batch/apply": { runId: 7, results: [], checks: [], state: { revisions: [{}], attempts: [], receipts: [] } } })); renderView();
    await screen.findByRole("link", { name: "Åbn Bank" });
    await userEvent.click(screen.getByRole("button", { name: "Forhåndsvis samlet dry run" }));
    await screen.findByText("Samlet dry run forhåndsvist");
    await userEvent.click(screen.getByRole("button", { name: "Gem eksakt plan" }));
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

  test("does not advance on mutation failure and surfaces the failure", async () => {
    mockFetch(routes({ "POST /api/companies/acme-aps/bookkeeping-batch/persist": { __error: { code: "forbidden", message: "Ingen adgang til at gemme planen" } } })); renderView();
    await screen.findByRole("link", { name: "Åbn Bank" }); await userEvent.click(screen.getByRole("button", { name: "Gem eksakt plan" }));
    await screen.findByRole("alert");
    expect(screen.getByRole("button", { name: "Godkend" })).toBeDisabled();
    expect(screen.queryByText("Varig historik")).not.toBeInTheDocument();
  });

  test("restores the durable run from a runId drilldown", async () => {
    const restored = { ...plan, scope: { accountingFrom: "2026-01-01", accountingTo: "2026-12-31", bankFrom: "2026-01-01", bankTo: "2026-12-31" } };
    mockFetch({ ...routes(), "GET /api/companies/acme-aps/bookkeeping-batch/runs/7": { state: { run: { runId: 7, plan: JSON.stringify(restored) }, revisions: [{}], attempts: [], receipts: [] } } });
    renderAt(<BookkeepingBatchView />, { route: "/companies/acme-aps/batchbogfoering?runId=7", path: "/companies/:slug/batchbogfoering" });
    await screen.findByText("Varig historik"); expect(screen.getByText(/Revisioner: 1/)).toBeInTheDocument();
  });
});
