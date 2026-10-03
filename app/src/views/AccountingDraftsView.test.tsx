import { afterEach, describe, expect, test, vi } from "bun:test";
import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { AccountingDraftsView } from "./AccountingDraftsView";
import { mockFetch } from "../test/fixtures";
import { renderAt } from "../test/render";
import { restoreGlobals } from "../test/globals";

const payload = {
  transactionDate: "2026-08-23",
  text: "Synthetic draft",
  lines: [
    { accountNo: "1100", debitAmount: 100 },
    { accountNo: "2000", creditAmount: 100 },
  ],
};

const submitted = {
  id: "synthetic-draft",
  version: 1,
  status: "submitted" as const,
  payloadHash: "a".repeat(64),
  eventHash: "b".repeat(64),
  payload,
  actorId: "user:author",
};

function renderView() {
  return renderAt(<AccountingDraftsView />, {
    route: "/companies/acme-aps/kladder",
    path: "/companies/:slug/kladder",
  });
}

afterEach(() => restoreGlobals());

describe("AccountingDraftsView", () => {
  test("creates a generic balanced draft without client-controlled actor fields", async () => {
    mockFetch({
      "GET /api/companies/acme-aps/accounting-drafts": { accountingDrafts: [] },
      "GET /api/companies/acme-aps/accounting-approval-policy": { policy: null },
      "POST /api/companies/acme-aps/accounting-drafts": { accountingDraft: { ...submitted, status: "created" } },
    });
    renderView();
    await screen.findByText("Ingen kladder endnu.");
    await userEvent.type(screen.getByLabelText("Kladde-id"), "synthetic-draft");
    await userEvent.type(screen.getByLabelText("Tekst"), "Synthetic draft");
    await userEvent.type(screen.getByLabelText("Konto linje 1"), "1100");
    await userEvent.type(screen.getByLabelText("Debet linje 1"), "100");
    await userEvent.type(screen.getByLabelText("Konto linje 2"), "2000");
    await userEvent.type(screen.getByLabelText("Kredit linje 2"), "100");
    await userEvent.click(screen.getByRole("button", { name: "Opret kladde" }));

    const call = (fetch as unknown as ReturnType<typeof vi.fn>).mock.calls.find(([url, init]) =>
      String(url).endsWith("/accounting-drafts") && init?.method === "POST",
    );
    expect(call).toBeDefined();
    const body = JSON.parse(String((call![1] as RequestInit).body));
    expect(body).toMatchObject({ draftId: "synthetic-draft", payload: { text: "Synthetic draft", lines: payload.lines } });
    expect(body.actor).toBeUndefined();
    expect(body.payload.createdBy).toBeUndefined();
  });

  test("sends exact submitted evidence and explicit confirmation before posting", async () => {
    mockFetch({
      "GET /api/companies/acme-aps/accounting-drafts": { accountingDrafts: [submitted] },
      "GET /api/companies/acme-aps/accounting-approval-policy": { policy: { eventHash: "c".repeat(64) } },
      "POST /api/companies/acme-aps/accounting-drafts/synthetic-draft/approve-and-post": {
        accountingDraft: { ...submitted, status: "approved_posted", journalEntryId: 1 },
      },
    });
    renderView();
    await screen.findByText("Afventer godkendelse");
    await userEvent.click(screen.getByRole("button", { name: "Godkend og bogfør" }));
    const dialog = await screen.findByRole("dialog", { name: "Godkend og bogfør" });
    await userEvent.click(within(dialog).getByRole("button", { name: "Godkend og bogfør" }));

    const call = (fetch as unknown as ReturnType<typeof vi.fn>).mock.calls.find(([url, init]) =>
      String(url).endsWith("/approve-and-post") && init?.method === "POST",
    );
    expect(JSON.parse(String((call![1] as RequestInit).body))).toEqual({
      expectedEventHash: submitted.eventHash,
      confirm: true,
      expectedPolicyEventHash: "c".repeat(64),
    });
  });

  test("keeps a labelled responsive register and resets the active status filter", async () => {
    mockFetch({
      "GET /api/companies/acme-aps/accounting-drafts": { accountingDrafts: [submitted] },
      "GET /api/companies/acme-aps/accounting-approval-policy": { policy: null },
    });
    renderView();
    await screen.findByText("Afventer godkendelse");
    expect(screen.getByRole("table", { name: "Bogføringskladder" })).toBeInTheDocument();
    await userEvent.selectOptions(screen.getByLabelText("Filtrér kladder på status"), "rejected");
    expect(screen.getByText("Aktive filtre: Status: Afvist")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Nulstil filtre" }));
    expect(screen.getByText("Afventer godkendelse")).toBeInTheDocument();
  });
});

test("parses Danish thousands and decimal separators without rounding the draft payload", async () => {
  mockFetch({
    "GET /api/companies/acme-aps/accounting-drafts": { accountingDrafts: [] },
    "GET /api/companies/acme-aps/accounting-approval-policy": { policy: null },
    "POST /api/companies/acme-aps/accounting-drafts": { accountingDraft: { ...submitted, status: "created" } },
  });
  renderView();
  await screen.findByText("Ingen kladder endnu.");
  await userEvent.type(screen.getByLabelText("Kladde-id"), "danish-amount");
  await userEvent.type(screen.getByLabelText("Tekst"), "Synthetic Danish amounts");
  await userEvent.type(screen.getByLabelText("Konto linje 1"), "1100");
  await userEvent.type(screen.getByLabelText("Debet linje 1"), "1.234,56");
  await userEvent.type(screen.getByLabelText("Konto linje 2"), "2000");
  await userEvent.type(screen.getByLabelText("Kredit linje 2"), "1.234,56");
  await userEvent.click(screen.getByRole("button", { name: "Opret kladde" }));
  const call = (fetch as unknown as ReturnType<typeof vi.fn>).mock.calls.find(([url, init]) => String(url).endsWith("/accounting-drafts") && init?.method === "POST");
  expect(call).toBeDefined();
  expect(JSON.parse(String((call![1] as RequestInit).body)).payload.lines).toEqual([
    { accountNo: "1100", debitAmount: 1234.56 }, { accountNo: "2000", creditAmount: 1234.56 },
  ]);
});


for (const code of ["network", "internal", "bad_request"]) test(`draft creation handles ${code} without losing its inputs`, async () => {
  mockFetch({ "GET /api/companies/acme-aps/accounting-approval-policy": { policy: null }, "GET /api/companies/acme-aps/accounting-drafts": { accountingDrafts: [] }, "POST /api/companies/acme-aps/accounting-drafts": { __error: { code, message: "Synthetic interrupted response" } } });
  renderView();
  await screen.findByText("Ingen kladder endnu.");
  await userEvent.type(screen.getByLabelText("Kladde-id"), "synthetic-draft");
  await userEvent.type(screen.getByLabelText("Tekst"), "Synthetic draft");
  await userEvent.type(screen.getByLabelText("Konto linje 1"), "1100");
  await userEvent.type(screen.getByLabelText("Debet linje 1"), "100");
  await userEvent.type(screen.getByLabelText("Konto linje 2"), "2000");
  await userEvent.type(screen.getByLabelText("Kredit linje 2"), "100");
  const write = screen.getByRole("button", { name: "Opret kladde" });
  await userEvent.click(write);
  expect(screen.getByLabelText("Kladde-id")).toHaveValue("synthetic-draft");
  expect(screen.getByLabelText("Debet linje 1")).toHaveValue("100");
  if (code === "bad_request") { expect(write).toBeEnabled(); expect(screen.queryByText(/Serverens resultat kunne ikke bekræftes/)).not.toBeInTheDocument(); }
  else {
    expect(await screen.findByText(/Serverens resultat kunne ikke bekræftes/)).toBeInTheDocument();
    expect(write).toBeDisabled();
    await userEvent.click(screen.getByRole("button", { name: "Kontrollér status" }));
    await userEvent.click(write);
    expect((fetch as unknown as ReturnType<typeof vi.fn>).mock.calls.filter(([url, init]) => String(url).endsWith("/accounting-drafts") && init?.method === "POST")).toHaveLength(1);
    expect(screen.getByLabelText("Tekst")).toHaveValue("Synthetic draft");
  }
});
