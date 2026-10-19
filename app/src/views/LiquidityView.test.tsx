import { describe, expect, test, vi } from "bun:test";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { AuthProvider } from "../lib/auth-context";
import type {
  CommitmentInput,
  CommitmentProposal,
  SupplierCommitmentsResponse,
} from "../lib/types/commitments";
import { cashflow, mockFetch } from "../test/fixtures";

vi.mock("../components/CashflowChart", () => ({
  CashflowChart: () => <div data-testid="legacy-cashflow-chart" />,
}));

import { renderAt } from "../test/render";
import { LiquidityView } from "./LiquidityView";

const input: CommitmentInput = {
  commitmentId: "synthetic-plan",
  vendorPartyId: "vendor:1",
  type: "Service",
  description: "Synthetic service",
  businessPurpose: "Synthetic work",
  amount: 10.01,
  currency: "DKK",
  frequency: "monthly",
  nextDate: "2026-01-31",
  evidenceRefs: ["document:1"],
};
const proposal: CommitmentProposal = {
  ...input,
  payloadHash: "a".repeat(64),
  previousPayloadHash: null,
  vendorSnapshot: "Synthetic Supplier",
  schedule: [
    { date: "2026-01-31", amount: 10.01, currency: "DKK" },
    { date: "2026-02-28", amount: 10.01, currency: "DKK" },
  ],
  warnings: ["Planning alone"],
};
function data(): SupplierCommitmentsResponse["supplierCommitments"] {
  return {
    slug: "acme-aps",
    company: { name: "Synthetic company", currency: "DKK" },
    selectedYear: "2026",
    fiscalYears: [{ label: "2026", source: "live", start: "2026-01-01", end: "2026-12-31" }],
    startDate: "2026-01-31",
    vendors: [{ id: 1, name: "Synthetic Supplier" }],
    documents: [{ id: 1, documentNo: "DOC-SYN", filename: "synthetic.txt" }],
    evidenceOptions: [],
    occurrences: [],
    commitments: [],
    history: [],
    alerts: [],
    matches: [],
    forecast: {
      ok: true,
      currency: "DKK",
      openingCash: null,
      lowestPoint: null,
      periods: [],
      completeness: {
        status: "incomplete",
        included: ["Approved expectations"],
        excluded: ["Independent cash evidence unavailable"],
      },
    },
  };
}
function renderView(d = data(), extra = {}) {
  mockFetch({
    "GET /api/companies/acme-aps/supplier-commitments": { supplierCommitments: d },
    ...extra,
  });
  return renderAt(<LiquidityView />, {
    route: "/companies/acme-aps/likviditet",
    path: "/companies/:slug/likviditet",
  });
}
async function fillPlan() {
  fireEvent.click(await screen.findByRole("button", { name: "Ny forpligtelse" }));
  const dialog = screen.getByRole("dialog");
  for (const [label, value] of Object.entries({
    Reference: "synthetic-plan",
    Leverandør: "1",
    Type: "Service",
    Beskrivelse: "Synthetic service",
    Forretningsformål: "Synthetic work",
    "Beløb pr. betaling": "10.01",
    "Næste betaling": "2026-01-31",
    "Originalt kildebilag": "1",
  })) {
    fireEvent.change(within(dialog).getByLabelText(label, { exact: true }), { target: { value } });
  }
  return dialog;
}
describe("LiquidityView native planning", () => {
  test("unknown opening cash stays unavailable and coverage stays incomplete", async () => {
    renderView();
    await screen.findByRole("heading", { name: "Likviditet" });
    expect(screen.getByText(/Bogført bank pr\..*Ikke tilgængelig/)).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("Ufuldstændig dækning");
    fireEvent.click(screen.getByText("Kilder, antagelser og udeladelser"));
    expect(screen.getByText("Independent cash evidence unavailable")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /faktiske bankposter/ })).toHaveAttribute(
      "href",
      "/companies/acme-aps/bank",
    );
  });
  test("server plan is reviewed before explicit hash-bound apply", async () => {
    renderView(data(), {
      "POST /api/companies/acme-aps/supplier-commitments/plan": { plan: proposal },
      "POST /api/companies/acme-aps/supplier-commitments": {
        supplierCommitment: { commitmentId: input.commitmentId },
      },
    });
    const dialog = await fillPlan();
    fireEvent.submit(within(dialog).getByRole("form", { name: "Planlæg leverandørforpligtelse" }));
    await screen.findByRole("dialog", { name: "Godkend forpligtelse" });
    expect(screen.getByText("2026-02-28")).toBeInTheDocument();
    const writes = () =>
      (fetch as unknown as { mock: { calls: Array<[string, RequestInit]> } }).mock.calls.filter(
        ([path, init]) =>
          path === "/api/companies/acme-aps/supplier-commitments" && init.method === "POST",
      );
    expect(writes().length).toBe(0);
    fireEvent.click(screen.getByRole("button", { name: "Godkend planen" }));
    await waitFor(() => expect(writes().length).toBe(1));
    const body = JSON.parse(writes()[0][1].body as string);
    expect(body.payloadHash).toBe(proposal.payloadHash);
    expect(body.commitment.amount).toBe(10.01);
    expect(body.commitment.vendorPartyId).toBe("vendor:1");
    expect(body.confirm).toBe(true);
    expect(typeof body.idempotencyKey).toBe("string");
  });
  test("plan failure never displays an approval", async () => {
    renderView(data(), {
      "POST /api/companies/acme-aps/supplier-commitments/plan": {
        __error: { code: "supplier_commitment_refused", message: "Source original unavailable" },
      },
    });
    const dialog = await fillPlan();
    fireEvent.submit(within(dialog).getByRole("form", { name: "Planlæg leverandørforpligtelse" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Source original unavailable");
    expect(screen.queryByRole("button", { name: "Godkend planen" })).not.toBeInTheDocument();
  });
  test("paused evidence remains visible with reasoned history", async () => {
    const d = data();
    d.commitments = [
      {
        commitmentId: "synthetic-plan",
        payloadHash: "b".repeat(64),
        createdAt: "2026-01-31",
        vendor: "Synthetic Supplier",
        purpose: "Synthetic work",
        amount: 10.01,
        currency: "DKK",
        frequency: "monthly",
        nextDate: "2026-01-31",
        renewalDate: null,
        status: "paused",
        evidenceRefs: ["document:1"],
        missingDocument: false,
      },
    ];
    d.history = [
      {
        commitmentId: "synthetic-plan",
        event: "paused",
        payloadHash: "b".repeat(64),
        createdAt: "2026-01-31",
        payload: { reason: "Synthetic reviewed pause" },
      },
    ];
    renderView(d);
    await screen.findByRole("table", { name: "Leverandørforpligtelser" });
    expect(screen.getByText(/Månedlig · Pauseret/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Pause" })).not.toBeInTheDocument();
    fireEvent.click(screen.getByText("Uforanderlig historik"));
    expect(screen.getByText(/Synthetic reviewed pause/)).toBeInTheDocument();
  });
});

test("legacy backend retains annual cashflow chart and year selection", async () => {
  mockFetch({
    "GET /api/companies/acme-aps/cashflow": { cashflow: cashflow() },
    "GET /api/companies/acme-aps/supplier-commitments": { supplierCommitments: data() },
  });
  renderAt(
    <AuthProvider hosted={false}>
      <LiquidityView />
    </AuthProvider>,
    { route: "/companies/acme-aps/likviditet", path: "/companies/:slug/likviditet" },
  );
  expect(await screen.findByTestId("legacy-cashflow-chart")).toBeInTheDocument();
  expect(screen.getByText("Primo-saldo")).toBeInTheDocument();
  expect(screen.getByRole("combobox", { name: "Vælg regnskabsår" })).toBeInTheDocument();
});
