import { expect, test } from "bun:test";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { mockFetch } from "../test/fixtures";
import { renderAt } from "../test/render";
import { BankJournalLinkModal } from "./BankJournalLinkModal";

const transaction = {
  id: 3,
  date: "2026-04-01",
  text: "Synthetic bank row",
  amount: 15.27,
  currency: "DKK",
};
const plan = {
  bankTransactionId: 3,
  bankDate: "2026-04-01",
  bankText: "Synthetic bank row",
  bankCurrency: "DKK",
  bankAmount: 15.27,
  bankAmountDkk: 15.27,
  bankAccountNo: "B",
  sourceHash: "a".repeat(64),
  transactionHash: "b".repeat(64),
  journalEntryId: 7,
  journalEntryNo: "SYN-LINK",
  journalHash: "c".repeat(64),
  journalDate: "2026-03-31",
  journalBankMovement: 15.27,
  document: null,
  planHash: "d".repeat(64),
  effect: "link_only_no_posting",
};
test("manual link requires explicit journal, reason and review, then sends exact proof", async () => {
  mockFetch({
    "GET /api/companies/acme-aps/bank/reconciliation-candidates": {
      reconciliationCandidates: { plans: [plan], note: "Synthetic read-only proof" },
    },
    "POST /api/companies/acme-aps/bank/reconciliation-link": {
      reconciliation: { id: 1, journalEntryNo: "SYN-LINK", effect: "link_only_no_posting" },
    },
  });
  let applied = 0;
  renderAt(
    <BankJournalLinkModal
      slug="acme-aps"
      transaction={transaction}
      onApplied={() => {
        applied++;
      }}
      onClose={() => {}}
    />,
  );
  const select = await screen.findByLabelText("Eksisterende postering");
  expect(screen.queryByRole("button", { name: "Opret afstemningslink" })).toBeNull();
  await userEvent.selectOptions(select, "7");
  const apply = screen.getByRole("button", { name: "Opret afstemningslink" });
  expect(apply).toBeDisabled();
  await userEvent.type(
    screen.getByLabelText("Begrundelse for afstemning"),
    "Synthetic reviewed event",
  );
  expect(apply).toBeDisabled();
  await userEvent.click(screen.getByRole("checkbox"));
  await userEvent.click(apply);
  expect(applied).toBe(1);
  const calls = (fetch as unknown as { mock: { calls: Array<[string, RequestInit]> } }).mock.calls;
  const body = calls.find(([url]) => url.endsWith("/bank/reconciliation-link"))?.[1].body;
  expect(JSON.parse(String(body))).toEqual({
    bankTransactionId: 3,
    journalEntryId: 7,
    planHash: plan.planHash,
    note: "Synthetic reviewed event",
    confirm: true,
  });
});
test("no eligible journal never offers to create accounting as a fallback", async () => {
  mockFetch({
    "GET /api/companies/acme-aps/bank/reconciliation-candidates": {
      reconciliationCandidates: { plans: [], note: "Review source mapping" },
    },
  });
  renderAt(
    <BankJournalLinkModal
      slug="acme-aps"
      transaction={transaction}
      onApplied={() => {}}
      onClose={() => {}}
    />,
  );
  expect(await screen.findByText(/Ingen egnede, uafstemte posteringer/)).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Opret afstemningslink" })).toBeNull();
});
