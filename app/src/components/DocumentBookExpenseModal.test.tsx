import { describe, expect, test, vi } from "bun:test";
import { render, screen, within } from "@testing-library/react";
import { DocumentBookExpenseModal } from "./DocumentBookExpenseModal";
import userEvent from "@testing-library/user-event";
import { stubGlobal } from "../test/globals";
import { mockFetch } from "../test/fixtures";

describe("DocumentBookExpenseModal", () => {
  test("#529/#530 shows durable supplier identity and the exact purchase VAT split", async () => {
    mockFetch({
      "GET /api/companies/acme-aps/documents/1/booking-options": {
        options: {
          document: {
            id: 1,
            documentNo: "DOC-2026-000001",
            documentType: "purchase_sale",
            invoiceNo: "US-529",
            invoiceDate: "2026-07-18",
            supplierName: "US SaaS Inc.",
            supplierVatOrCvr: null,
            supplierCountryCode: "US",
            supplierIdentifierKind: "non_eu",
            supplierIdentityStatus: "resolved",
            purchaseVatLines: [
              { classification: "dk_purchase_25", netAmount: 975, vatAmount: 243.75 },
              { classification: "exempt", netAmount: 670, vatAmount: 0 },
            ],
            amountIncVat: 1888.75,
            vatAmount: 243.75,
            currency: "DKK",
          },
          expenseAccounts: [{ accountNo: "3000", name: "Software", defaultVatCode: "DK_PURCHASE_25" }],
          unmatchedOutgoingBank: [{ id: 1, date: "2026-07-18", text: "US SaaS", amount: -1888.75, currency: "DKK", reference: null }],
        },
      },
    });

    render(
      <DocumentBookExpenseModal
        slug="acme-aps"
        documentId={1}
        onBooked={() => {}}
        onClose={() => {}}
      />,
    );

    expect(await screen.findByText(/Leverandøridentitet: US · non_eu · resolved/)).toBeInTheDocument();
    expect(screen.getByLabelText("Momsfordeling")).toHaveTextContent("dk_purchase_25");
    expect(screen.getByLabelText("Momsfordeling")).toHaveTextContent("exempt");
    expect(screen.getByLabelText("Moms-behandling (valgfri — udledes ellers af kontoen)")).toHaveTextContent("Omvendt betalingspligt (udenlandsk ydelse)");
  });
});

const BOOKING_OPTIONS = {
  document: {
    id: 1, documentNo: "DOC-2026-000001", documentType: "purchase_sale", sourceBankTransactionId: null,
    invoiceNo: "F-001", invoiceDate: "2026-07-18", supplierName: "Leverandør ApS", supplierVatOrCvr: "DK11223344",
    supplierCountryCode: "DK", supplierIdentifierKind: "dk_cvr", supplierIdentityStatus: "resolved", purchaseVatLines: [], amountIncVat: 1250, vatAmount: 250, currency: "DKK",
  },
  expenseAccounts: [{ accountNo: "3000", name: "Kontorartikler", defaultVatCode: "DK_PURCHASE_25" }],
  unmatchedOutgoingBank: [{ id: 1, date: "2026-07-18", text: "Leverandør", amount: -1250, currency: "DKK", amountDkk: -1250, fxRateToDkk: 1, reference: "REF-1" }],
};

describe("DocumentBookExpenseModal — mutation safety", () => {
  test("uncertain writes cannot be submitted twice and preserve the selected account", async () => {
    mockFetch({ "GET /api/companies/acme-aps/documents/1/booking-options": { options: BOOKING_OPTIONS } });
    const originalFetch = globalThis.fetch;
    const fetchSpy = vi.fn((input: RequestInfo | URL, init?: RequestInit) => {
      if (String(input).endsWith("/documents/book-expense")) return Promise.reject(new TypeError("connection lost"));
      return originalFetch(input, init);
    });
    stubGlobal("fetch", fetchSpy);
    const onBooked = vi.fn();
    render(<DocumentBookExpenseModal slug="acme-aps" documentId={1} onBooked={onBooked} onClose={() => {}} />);
    await screen.findByRole("option", { name: "3000 · Kontorartikler" });
    await userEvent.selectOptions(screen.getByLabelText("Udgiftskonto"), "3000");
    await userEvent.click(screen.getByRole("button", { name: "Bogfør" }));
    expect(await screen.findByText(/Kontrollér bilaget og posteringerne/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Bogfør" })).toBeDisabled();
    expect(screen.getByLabelText("Udgiftskonto")).toHaveValue("3000");
    expect(fetchSpy.mock.calls.filter((entry) => String(entry[0]).endsWith("/documents/book-expense"))).toHaveLength(1);
    expect(onBooked).not.toHaveBeenCalled();
  });
  test("a known blocking VAT preflight disables posting until its evidence is resolved", async () => {
    mockFetch({
      "GET /api/companies/acme-aps/documents/1/booking-options": { options: BOOKING_OPTIONS },
      "GET /api/companies/acme-aps/documents/1/vat-preflight": { preflight: { ok: false, derivedRegion: "EU", requiredValidation: "vies", cache: { reused: false, freshUntil: null }, applyWouldCallProvider: true, errors: ["Momsnummeret skal valideres."], exception: null } },
    });
    render(<DocumentBookExpenseModal slug="acme-aps" documentId={1} onBooked={() => {}} onClose={() => {}} />);
    await screen.findByRole("option", { name: "3000 · Kontorartikler" });
    await userEvent.selectOptions(screen.getByLabelText("Udgiftskonto"), "3000");
    await screen.findByText(/Momsnummeret skal valideres/);
    expect(screen.getByRole("button", { name: "Bogfør" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Hent momsvalidering" })).toBeEnabled();
  });
});

describe("DocumentBookExpenseModal — unsaved dismiss protection", () => {
  test("cancel keeps the chosen account unless the owner explicitly discards it", async () => {
    mockFetch({ "GET /api/companies/acme-aps/documents/1/booking-options": { options: BOOKING_OPTIONS } });
    const onClose = vi.fn();
    render(<DocumentBookExpenseModal slug="acme-aps" documentId={1} onBooked={() => {}} onClose={onClose} />);
    await screen.findByRole("option", { name: "3000 · Kontorartikler" });
    await userEvent.selectOptions(screen.getByLabelText("Udgiftskonto"), "3000");
    const cancel = screen.getByRole("button", { name: "Annullér" });
    await userEvent.click(cancel);
    expect(onClose).not.toHaveBeenCalled();
    await userEvent.click(within(screen.getByRole("dialog", { name: "Kassér ændringer?" })).getByRole("button", { name: "Annullér" }));
    expect(screen.getByLabelText("Udgiftskonto")).toHaveValue("3000");
    expect(onClose).not.toHaveBeenCalled();
    await userEvent.click(cancel);
    await userEvent.click(within(screen.getByRole("dialog", { name: "Kassér ændringer?" })).getByRole("button", { name: "Kassér ændringer" }));
    expect(onClose).toHaveBeenCalledTimes(1);
    const calls = (globalThis.fetch as unknown as ReturnType<typeof vi.fn>).mock.calls;
    expect(calls.some((call) => String(call[0]).endsWith("/documents/book-expense"))).toBe(false);
  });
});
