import { describe, expect, test } from "bun:test";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { documents, mockFetch } from "../test/fixtures";
import { BankExpenseModal } from "./BankExpenseModal";

describe("Bank expense review", () => {
  test("requires an explicit purchase selection and carries the originating bank row into review", async () => {
    const base = documents().documents[0]!;
    mockFetch({
      "GET /api/companies/acme-aps/documents": { documents: documents({ documents: [
        { ...base, id: 1, journalEntryId: null, invoiceNo: "FIRST" },
        { ...base, id: 2, journalEntryId: null, invoiceNo: "SECOND" },
        { ...base, id: 3, invoiceNo: "ALREADY-POSTED" },
      ] }) },
      "GET /api/companies/acme-aps/documents/1/booking-options": { options: {
        document: { ...base, id: 1, journalEntryId: null, vatAmount: 250 },
        expenseAccounts: [{ accountNo: "6000", name: "Supplies", defaultVatCode: "DK_PURCHASE_25" }],
        unmatchedOutgoingBank: [
          { id: 7, date: "2026-10-18", text: "Selected bank row", amount: -1250, currency: "DKK" },
          { id: 8, date: "2026-10-18", text: "Other bank row", amount: -1250, currency: "DKK" },
        ],
      } },
    });
    render(<BankExpenseModal slug="acme-aps" transaction={{ id: 7, date: "2026-10-18", text: "Selected bank row", amount: -1250, currency: "DKK" }} onClose={() => {}} onReconciled={() => {}} />);
    const picker = await screen.findByLabelText("Bilag");
    expect(screen.getByRole("button", { name: "Gennemgå bilag" })).toBeDisabled();
    expect(screen.queryByRole("option", { name: /ALREADY-POSTED/ })).not.toBeInTheDocument();
    await userEvent.selectOptions(picker, "1");
    await userEvent.click(screen.getByRole("button", { name: "Gennemgå bilag" }));
    await screen.findByRole("dialog", { name: "Bogfør bilag" });
    await screen.findByRole("option", { name: /Selected bank row/ });
    expect(screen.getByRole("option", { name: /Selected bank row/ })).toHaveProperty("selected", true);
    expect(screen.getByRole("button", { name: "Bogfør" })).toBeDisabled();
  });
});
