import { describe, expect, test, vi } from "bun:test";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { BookkeepingBatchView } from "./BookkeepingBatchView";
import { renderAt } from "../test/render";
import { mockFetch } from "../test/fixtures";

const plan = { planHash: "a".repeat(64), items: [{ actionKey: "bank:1", partition: "missingDocument" }] };

describe("BookkeepingBatchView", () => {
  test("keeps plan, persist, approval and apply as four separate requests", async () => {
    mockFetch({
      "GET /api/companies/acme-aps/bookkeeping-workbench": { workbench: { state: "available", counts: { ready: 1 }, population: { total: 1, ready: 1, blockers: 0 }, selection: { total: 1, ready: 1, blockers: 0 }, page: { total: 0, nextCursor: null }, completeness: { nextAction: "Review." }, rows: [], periodClose: { status: "available", blockers: 0 }, plan: { planHash: plan.planHash } } },
      "GET /api/companies/acme-aps/bookkeeping-batch": { dryRun: true, plan },
      "POST /api/companies/acme-aps/bookkeeping-batch/persist": { ok: true, runId: 7, plan, state: { revisions: [], attempts: [], receipts: [] } },
      "POST /api/companies/acme-aps/bookkeeping-batch/approve": { ok: true, state: { revisions: [{ planHash: plan.planHash, approvedAt: "2026-01-31T10:00:00Z" }], attempts: [], receipts: [] } },
      "POST /api/companies/acme-aps/bookkeeping-batch/apply": { ok: true, runId: 7, results: [], checks: [] },
    });
    renderAt(<BookkeepingBatchView />, { route: "/companies/acme-aps/batchbogfoering", path: "/companies/:slug/batchbogfoering" });
    await userEvent.type(screen.getByLabelText("Fra dato"), "2026-01-01");
    await userEvent.type(screen.getByLabelText("Til dato"), "2026-01-31");
    await userEvent.click(screen.getByRole("button", { name: "Vis arbejdskø" }));
    await screen.findByText("Plan-hash:");
    expect(screen.getByRole("button", { name: "Gem eksakt plan" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Anvend" })).toBeDisabled();
    await userEvent.click(screen.getByRole("button", { name: "Gem eksakt plan" }));
    expect(screen.getByRole("button", { name: "Anvend" })).toBeDisabled();
    await userEvent.click(screen.getByRole("button", { name: "Godkend" }));
    await userEvent.click(screen.getByRole("button", { name: "Anvend" }));
    await waitFor(() => expect(screen.getByText("Kørselsresultat")).toBeInTheDocument());
    const calls = (globalThis.fetch as any).mock.calls.map((x: any[]) => String(x[0]).replace(/^https?:\/\/[^/]+/, "").split("?")[0]);
    expect(calls.filter((path: string) => path.endsWith("/approve"))).toHaveLength(1);
    expect(calls.filter((path: string) => path.endsWith("/apply"))).toHaveLength(1);
  });

  test("cannot persist a filtered view while the canonical population has blockers", async () => {
    mockFetch({
      "GET /api/companies/acme-aps/bookkeeping-workbench": { workbench: { state: "available", counts: { ready: 1, missingDocument: 1 }, population: { total: 2, ready: 1, blockers: 1 }, selection: { total: 1, ready: 1, blockers: 0 }, page: { total: 0, nextCursor: null }, completeness: { nextAction: "Review." }, rows: [], periodClose: { status: "available", blockers: 1 }, plan: { planHash: plan.planHash } } },
      "GET /api/companies/acme-aps/bookkeeping-batch": { dryRun: true, plan },
    });
    renderAt(<BookkeepingBatchView />, { route: "/companies/acme-aps/batchbogfoering", path: "/companies/:slug/batchbogfoering" });
    await userEvent.type(screen.getByLabelText("Fra dato"), "2026-01-01");
    await userEvent.type(screen.getByLabelText("Til dato"), "2026-01-31");
    await userEvent.click(screen.getByRole("button", { name: "Vis arbejdskø" }));
    await screen.findByText("Plan-hash:");
    expect(screen.getByRole("button", { name: "Gem eksakt plan" })).toBeDisabled();
    expect(screen.getByText(/filtrering kan ikke omgå dem/)).toBeInTheDocument();
  });

  test("restores the durable run from a runId drilldown after restart", async () => {
    const restored = { ...plan, scope: { accountingFrom: "2026-01-01", accountingTo: "2026-01-31", bankFrom: "2026-01-01", bankTo: "2026-01-31" } };
    mockFetch({
      "GET /api/companies/acme-aps/bookkeeping-batch/runs/7": { state: { run: { runId: 7, plan: JSON.stringify(restored) }, revisions: [{}], attempts: [], receipts: [] } },
    });
    renderAt(<BookkeepingBatchView />, { route: "/companies/acme-aps/batchbogfoering?runId=7", path: "/companies/:slug/batchbogfoering" });
    await screen.findByText("Varig historik");
    expect(screen.getByText(/Revisioner: 1/)).toBeInTheDocument();
    expect(screen.getByText("Plan-hash:")).toBeInTheDocument();
  });
});

test("a restored approval only enables apply for the exact plan hash", async () => {
  mockFetch({
    "GET /api/companies/acme-aps/bookkeeping-batch/runs/7": { state: {
      run: { plan: JSON.stringify({ ...plan, bankFrom: "2026-01-01", bankTo: "2026-01-31" }) },
      revisions: [{ planHash: "b".repeat(64), approvedAt: "2026-01-31T10:00:00Z" }], attempts: [], receipts: [],
    } },
  });
  renderAt(<BookkeepingBatchView />, { route: "/companies/acme-aps/batchbogfoering?runId=7", path: "/companies/:slug/batchbogfoering" });
  await screen.findByText("Varig historik");
  expect(screen.getByRole("button", { name: "Anvend" })).toBeDisabled();
  expect(screen.getByLabelText("Fra dato")).toHaveValue("2026-01-01");
});

test("changing the date scope invalidates a restored approved plan", async () => {
  mockFetch({
    "GET /api/companies/acme-aps/bookkeeping-batch/runs/7": { state: {
      run: { plan: JSON.stringify({ ...plan, bankFrom: "2026-01-01", bankTo: "2026-01-31" }) },
      revisions: [{ planHash: plan.planHash, approvedAt: "2026-01-31T10:00:00Z" }], attempts: [], receipts: [],
    } },
  });
  renderAt(<BookkeepingBatchView />, { route: "/companies/acme-aps/batchbogfoering?runId=7", path: "/companies/:slug/batchbogfoering" });
  await screen.findByText("Varig historik");
  expect(screen.getByRole("button", { name: "Anvend" })).toBeEnabled();
  await userEvent.clear(screen.getByLabelText("Fra dato"));
  expect(screen.getByRole("button", { name: "Anvend" })).toBeDisabled();
  expect(screen.queryByText("Varig historik")).not.toBeInTheDocument();
});


for (const code of ["network", "internal"]) test(`batch apply remains blocked after ${code}, including a successful status read`, async () => {
  const restored = { ...plan, bankFrom: "2026-01-01", bankTo: "2026-01-31" };
  const durable = { run: { plan: JSON.stringify(restored) }, revisions: [{ planHash: plan.planHash, approvedAt: "2026-01-31T10:00:00Z" }], attempts: [], receipts: [] };
  mockFetch({ "GET /api/companies/acme-aps/bookkeeping-batch/runs/7": { state: durable }, "POST /api/companies/acme-aps/bookkeeping-batch/apply": { __error: { code, message: "Synthetic interrupted response" } } });
  renderAt(<BookkeepingBatchView />, { route: "/companies/acme-aps/batchbogfoering?runId=7", path: "/companies/:slug/batchbogfoering" });
  await screen.findByText("Varig historik");
  const write = screen.getByRole("button", { name: "Anvend" });
  await userEvent.click(write);
  expect(await screen.findByText(/Serverens resultat kunne ikke bekræftes/)).toBeInTheDocument();
  expect(write).toBeDisabled();
  await userEvent.click(screen.getByRole("button", { name: "Kontrollér status" }));
  await userEvent.click(write);
  expect(write).toBeDisabled();
  expect(screen.getByLabelText("Fra dato")).toHaveValue("2026-01-01");
  const writes = (fetch as unknown as ReturnType<typeof vi.fn>).mock.calls.filter(([url, init]) => String(url).endsWith("/apply") && init?.method === "POST");
  expect(writes).toHaveLength(1);
  expect(JSON.parse(String((writes[0]![1] as RequestInit).body))).toMatchObject({ runId: 7, planHash: plan.planHash });
});


test("a read-only batch load failure never becomes an unknown mutation", async () => {
  mockFetch({ "GET /api/companies/acme-aps/bookkeeping-workbench": { __error: { code: "network", message: "Synthetic read failure" } }, "GET /api/companies/acme-aps/bookkeeping-batch": { dryRun: true, plan } });
  renderAt(<BookkeepingBatchView />, { route: "/companies/acme-aps/batchbogfoering?from=2026-01-01&to=2026-01-31", path: "/companies/:slug/batchbogfoering" });
  await userEvent.click(screen.getByRole("button", { name: "Vis arbejdskø" }));
  expect(await screen.findByText("Synthetic read failure")).toBeInTheDocument();
  expect(screen.queryByText(/Serverens resultat kunne ikke bekræftes/)).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Vis arbejdskø" })).toBeEnabled();
});
