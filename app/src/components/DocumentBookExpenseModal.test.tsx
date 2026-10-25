import { describe, expect, test, vi } from "bun:test";
import { render, screen, within, waitFor } from "@testing-library/react";
import { DocumentBookExpenseModal } from "./DocumentBookExpenseModal";
import userEvent from "@testing-library/user-event";
import { stubGlobal } from "../test/globals";
import { mockFetch } from "../test/fixtures";
import { AuthProvider } from "../lib/auth-context";
import { Context as WorkspaceTransferContext } from "../lib/workspace-transfer-context";
import { MutationMemoryProvider } from "../lib/mutation-memory";

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
    mockFetch({ "GET /api/companies/acme-aps/documents/1/booking-options": { options: BOOKING_OPTIONS }, "POST /api/companies/acme-aps/documents/book-expense": {} });
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

const NATIVE_PATH = "/api/companies/acme-aps/expense/original/vat-preflight/1";
const NATIVE_NEEDED = {
  ok: false, documentId: 1, classification: "EU", cached: false, evidenceExpiresAt: null,
  wouldCallProvider: true, errors: ["fresh EU VAT validation evidence is required"], derivedRegion: "EU",
  requiredValidation: "EU VAT provider validation", cache: { reused: false, freshUntil: null },
  applyWouldCallProvider: true, evidence: [], exception: null,
};
function nativeFixture(post: unknown = NATIVE_NEEDED) {
  mockFetch({
    "GET /api/companies/acme-aps/documents/1/booking-options": { options: BOOKING_OPTIONS },
    [`GET ${NATIVE_PATH}`]: NATIVE_NEEDED, [`POST ${NATIVE_PATH}`]: post,
  });
}
function NativeForm({ documentId = 1 }: { documentId?: number }) {
  return <AuthProvider hosted={false} backend="rust"><MutationMemoryProvider><DocumentBookExpenseModal slug="acme-aps" documentId={documentId} onBooked={() => {}} onClose={() => {}} /></MutationMemoryProvider></AuthProvider>;
}
const providerPosts = () => (globalThis.fetch as unknown as ReturnType<typeof vi.fn>).mock.calls.filter(c => String(c[0]) === NATIVE_PATH && c[1]?.method === "POST");
async function openProviderReview() {
  await screen.findByText(/fresh EU VAT validation evidence is required/);
  await userEvent.click(screen.getByRole("button", { name: "Hent momsvalidering" }));
  return within(screen.getByRole("dialog", { name: "Hent momsvalidering" }));
}
describe("native purchase VAT owner review", () => {
  test("source false is visible, opening and cancel send zero POST, explicit confirm shows actionable source failure", async () => {
    sessionStorage.clear();
    nativeFixture({ ...NATIVE_NEEDED, exception: { id: 2, status: "open", severity: "high", message: "Provider unavailable", requiredAction: "Resolve authoritative supplier evidence", createdAt: "2026-10-24" } });
    render(<NativeForm />);
    let dialog = await openProviderReview();
    expect(dialog.getByText(/Virksomhed: acme-aps · Bilag 1/)).toBeInTheDocument();
    expect(dialog.getByText(/Leverandør: Leverandør ApS/)).toBeInTheDocument();
    expect(providerPosts()).toHaveLength(0);
    await userEvent.click(dialog.getByRole("button", { name: "Annullér" }));
    expect(providerPosts()).toHaveLength(0);
    dialog = await openProviderReview();
    await userEvent.click(dialog.getByRole("button", { name: "Hent momsvalidering" }));
    expect(await screen.findByText(/Resolve authoritative supplier evidence/)).toBeInTheDocument();
    expect(providerPosts()).toHaveLength(1);
    expect(providerPosts()[0]![1]?.body).toBe('{"confirm":true}');
    expect(screen.getByRole("button", { name: "Bogfør" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Hent momsvalidering" })).toBeEnabled();
  });
  test("unknown provider write survives read/remount and manual release never resends", async () => {
    sessionStorage.clear();
    nativeFixture();
    const original = globalThis.fetch;
    stubGlobal("fetch", vi.fn((input: RequestInfo | URL, init?: RequestInit) => String(input) === NATIVE_PATH && init?.method === "POST" ? Promise.reject(new TypeError("lost")) : original(input, init)));
    const view = render(<NativeForm />);
    const dialog = await openProviderReview();
    await userEvent.click(dialog.getByRole("button", { name: "Hent momsvalidering" }));
    await dialog.findByText(/En ny skrivning er blokeret/);
    expect(providerPosts()).toHaveLength(1);
    view.unmount();
    render(<NativeForm />);
    await screen.findByText(/fresh EU VAT validation evidence is required/);
    expect(screen.getByRole("button", { name: "Hent momsvalidering" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Bogfør" })).toBeDisabled();
    await userEvent.click(screen.getByRole("button", { name: "Kontrollér status" }));
    await screen.findByText(/fresh EU VAT validation evidence is required/);
    expect(screen.getByRole("button", { name: "Hent momsvalidering" })).toBeDisabled();
    await userEvent.click(screen.getByRole("button", { name: "Resultatet er afklaret" }));
    await userEvent.click(screen.getByRole("button", { name: "Jeg har kontrolleret resultatet" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "Hent momsvalidering" })).toBeEnabled());
    expect(providerPosts()).toHaveLength(1);
    sessionStorage.clear();
  });
  test("context change invalidates an open review without dispatch", async () => {
    sessionStorage.clear();
    nativeFixture();
    const view = render(<NativeForm />);
    await openProviderReview();
    view.rerender(<NativeForm documentId={2} />);
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Hent momsvalidering" })).not.toBeInTheDocument());
    expect(providerPosts()).toHaveLength(0);
  });
  test("access changes invalidate reviewed confirmation before provider dispatch", async () => {
    sessionStorage.clear();
    nativeFixture();
    const wrapper = (writable: boolean) => <WorkspaceTransferContext.Provider value={{ enabled: true, transfer: null, error: "", loading: false, writable, scope: "test", refresh: async () => {}, adopt: () => {} }}><NativeForm /></WorkspaceTransferContext.Provider>;
    const view = render(wrapper(true));
    await openProviderReview();
    view.rerender(wrapper(false));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Hent momsvalidering" })).not.toBeInTheDocument());
    expect(providerPosts()).toHaveLength(0);
    await screen.findByText(/fresh EU VAT validation evidence is required/);
    expect(screen.queryByRole("button", { name: "Hent momsvalidering" })).not.toBeInTheDocument();
  });
  test("native read failure is visible with explicit inert retry and no legacy fallback", async () => {
    sessionStorage.clear();
    nativeFixture();
    const original = globalThis.fetch;
    const spy = vi.fn((input: RequestInfo | URL, init?: RequestInit) => String(input) === NATIVE_PATH ? Promise.resolve(new Response('{"ok":', { status: 200 })) : original(input, init));
    stubGlobal("fetch", spy);
    render(<NativeForm />);
    await screen.findByRole("option", { name: "3000 · Kontorartikler" });
    await screen.findByRole("button", { name: "Prøv momsvalidering igen" });
    const readsBeforeRetry = spy.mock.calls.filter(c => String(c[0]) === NATIVE_PATH).length;
    await userEvent.click(screen.getByRole("button", { name: "Prøv momsvalidering igen" }));
    await waitFor(() => expect(spy.mock.calls.filter(c => String(c[0]) === NATIVE_PATH)).toHaveLength(readsBeforeRetry + 1));
    expect(providerPosts()).toHaveLength(0);
    expect(spy.mock.calls.some(c => String(c[0]).includes("/documents/1/vat-preflight"))).toBe(false);
    expect(screen.getByRole("button", { name: "Bogfør" })).toBeDisabled();
  });
});
