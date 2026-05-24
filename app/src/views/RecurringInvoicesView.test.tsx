// Tests for app/src/views/RecurringInvoicesView.tsx (#435 + #386 + earlier).
//
// Covers the Deaktivér action (#435): the button is visible on active
// templates, the confirm-dialog gate must be passed, the API call carries
// the optional reason from the prompt, and a server failure is surfaced as
// a banner. Inactive templates do NOT show the Generér / Deaktivér actions.
//
// Also covers the Opret skabelon action (#386): both the page-head button
// and the empty-state CTA open the create modal; submitting the modal
// POSTs to `/recurring-invoices` with the minimal payload + confirm:true,
// and refreshes the list on success. The CLI snippet that used to live in
// the empty-state is gone.

import { describe, expect, test, vi, beforeEach, afterEach } from "vitest";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { RecurringInvoicesView } from "./RecurringInvoicesView";
import { renderAt } from "../test/render";
import { fiscalYears, mockFetch } from "../test/fixtures";
import type { CompanyRecurringInvoices } from "../lib/types";

function recurringInvoicesPayload(
  over: Partial<CompanyRecurringInvoices> = {},
): CompanyRecurringInvoices {
  return {
    slug: "acme-aps",
    templates: [
      {
        id: 7,
        name: "ABC ApS · månedligt abonnement",
        interval: "monthly",
        firstIssueDate: "2026-01-01",
        nextIssueDate: "2026-06-01",
        paymentTermsDays: 14,
        deliveryPeriodMode: "issue_month",
        notes: "Faktura sendes på e-mail",
        active: true,
        createdAt: "2026-01-01T08:00:00Z",
        generations: [],
      },
      {
        id: 8,
        name: "Gammel kunde · kvartalsvis",
        interval: "quarterly",
        firstIssueDate: "2024-01-01",
        nextIssueDate: "2025-10-01",
        paymentTermsDays: 30,
        deliveryPeriodMode: "issue_month",
        notes: null,
        active: false,
        createdAt: "2024-01-01T08:00:00Z",
        generations: [],
      },
    ],
    ...over,
  };
}

function routes(over: Partial<CompanyRecurringInvoices> = {}) {
  return {
    "GET /api/companies/acme-aps/recurring-invoices": {
      recurringInvoices: recurringInvoicesPayload(over),
    },
    "GET /api/companies/acme-aps/fiscal-years": { fiscalYears: fiscalYears() },
  };
}

function renderView() {
  return renderAt(<RecurringInvoicesView />, {
    route: "/companies/acme-aps/faktura-skabeloner",
    path: "/companies/:slug/faktura-skabeloner",
  });
}

describe("RecurringInvoicesView — Faktura-skabeloner", () => {
  let confirmSpy: ReturnType<typeof vi.spyOn>;
  let promptSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(true);
    promptSpy = vi.spyOn(window, "prompt").mockReturnValue("Kontrakt opsagt");
  });

  afterEach(() => {
    confirmSpy.mockRestore();
    promptSpy.mockRestore();
  });

  test("renders an active and a retired template under their respective sections", async () => {
    mockFetch(routes());
    renderView();
    expect(await screen.findByText(/Aktive \(1\)/)).toBeInTheDocument();
    expect(screen.getByText(/Tilbagetrukne \(1\)/)).toBeInTheDocument();
    expect(
      screen.getByText("ABC ApS · månedligt abonnement"),
    ).toBeInTheDocument();
    expect(
      screen.getByText("Gammel kunde · kvartalsvis"),
    ).toBeInTheDocument();
  });

  test("active templates show Deaktivér and Generér; retired templates show neither", async () => {
    mockFetch(routes());
    renderView();
    await screen.findByText("ABC ApS · månedligt abonnement");

    const generateButtons = screen.getAllByRole("button", { name: /Generér/ });
    const retireButtons = screen.getAllByRole("button", {
      name: /Deaktivér skabelonen/,
    });
    // Exactly one of each — the active template only.
    expect(generateButtons).toHaveLength(1);
    expect(retireButtons).toHaveLength(1);

    // The retired template's card shows the "deactivated" explanation.
    expect(
      screen.getByText(/Skabelonen er deaktiveret/),
    ).toBeInTheDocument();
  });

  test("Deaktivér confirms, posts to the retire endpoint with the reason, and reloads", async () => {
    mockFetch({
      ...routes(),
      "POST /api/companies/acme-aps/recurring-invoices/7/retire": {
        template: { id: 7, retired: true },
      },
    });
    renderView();
    await screen.findByText("ABC ApS · månedligt abonnement");

    await userEvent.click(
      screen.getByRole("button", { name: /Deaktivér skabelonen/ }),
    );

    expect(confirmSpy).toHaveBeenCalled();
    expect(promptSpy).toHaveBeenCalled();

    await waitFor(() => {
      const calls = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls;
      const retireCall = calls.find(
        ([u]) =>
          typeof u === "string" &&
          u.includes("/recurring-invoices/7/retire"),
      );
      expect(retireCall).toBeTruthy();
      const init = retireCall![1] as RequestInit;
      expect(init.method).toBe("POST");
      const body = JSON.parse(String(init.body));
      expect(body.confirm).toBe(true);
      expect(body.reason).toBe("Kontrakt opsagt");
    });

    expect(
      await screen.findByText(
        /Skabelonen "ABC ApS · månedligt abonnement" er deaktiveret\./,
      ),
    ).toBeInTheDocument();
  });

  test("Cancelling the confirm dialog skips the API call", async () => {
    confirmSpy.mockReturnValue(false);
    mockFetch(routes());
    renderView();
    await screen.findByText("ABC ApS · månedligt abonnement");

    await userEvent.click(
      screen.getByRole("button", { name: /Deaktivér skabelonen/ }),
    );

    const calls = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls;
    const retireCall = calls.find(
      ([u]) =>
        typeof u === "string" && u.includes("/retire"),
    );
    expect(retireCall).toBeUndefined();
  });

  test("A server error from the retire endpoint is surfaced as a banner", async () => {
    mockFetch({
      ...routes(),
      "POST /api/companies/acme-aps/recurring-invoices/7/retire": {
        __error: {
          code: "bad_request",
          message: "Skabelonen kunne ikke deaktiveres",
        },
      },
    });
    renderView();
    await screen.findByText("ABC ApS · månedligt abonnement");

    await userEvent.click(
      screen.getByRole("button", { name: /Deaktivér skabelonen/ }),
    );

    expect(
      await screen.findByText(/Skabelonen kunne ikke deaktiveres/),
    ).toBeInTheDocument();
  });

  test("An empty reason in the prompt is omitted from the request body", async () => {
    promptSpy.mockReturnValue("");
    mockFetch({
      ...routes(),
      "POST /api/companies/acme-aps/recurring-invoices/7/retire": {
        template: { id: 7, retired: true },
      },
    });
    renderView();
    await screen.findByText("ABC ApS · månedligt abonnement");

    await userEvent.click(
      screen.getByRole("button", { name: /Deaktivér skabelonen/ }),
    );

    await waitFor(() => {
      const calls = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls;
      const retireCall = calls.find(
        ([u]) =>
          typeof u === "string" && u.includes("/retire"),
      );
      expect(retireCall).toBeTruthy();
      const body = JSON.parse(String((retireCall![1] as RequestInit).body));
      expect(body.confirm).toBe(true);
      expect(body.reason).toBeUndefined();
    });
  });

  // -------------------------------------------------------------------------
  // #386 — Opret skabelon-knap + modal-flow
  // -------------------------------------------------------------------------

  test("page-head has a 'Opret skabelon' button when there are templates", async () => {
    mockFetch(routes());
    renderView();
    await screen.findByText("ABC ApS · månedligt abonnement");
    expect(
      screen.getByRole("button", { name: /Opret skabelon/ }),
    ).toBeInTheDocument();
  });

  test("empty state shows a 'Opret skabelon' CTA and no CLI snippet", async () => {
    mockFetch({
      ...routes({ templates: [] }),
    });
    renderView();
    await screen.findByText(/Ingen skabeloner endnu/);

    // The old CLI snippet must NOT appear — that was the bug in #386.
    expect(screen.queryByText(/rentemester recurring-invoice create/)).toBeNull();

    // Both the page-head button and the empty-state CTA are present.
    const buttons = screen.getAllByRole("button", { name: /Opret skabelon/ });
    expect(buttons.length).toBeGreaterThanOrEqual(2);
  });

  test("clicking 'Opret skabelon' opens the modal with the expected fields", async () => {
    mockFetch({
      ...routes({ templates: [] }),
      "GET /api/companies/acme-aps/contacts": {
        contacts: {
          slug: "acme-aps",
          company: { name: "Acme ApS", cvr: "DK12345678", country: "DK", currency: "DKK", fiscalYearStartMonth: 1, fiscalYearLabelStrategy: "calendar" },
          fiscalYears: [],
          customers: [],
          vendors: [],
        },
      },
    });
    renderView();
    await screen.findByText(/Ingen skabeloner endnu/);

    await userEvent.click(
      screen.getAllByRole("button", { name: /Opret skabelon/ })[0]!,
    );

    expect(
      await screen.findByRole("dialog", { name: /Opret faktura-skabelon/ }),
    ).toBeInTheDocument();
    expect(screen.getByLabelText(/Skabelonens navn/)).toBeInTheDocument();
    expect(screen.getByLabelText(/^Interval$/)).toBeInTheDocument();
    expect(
      screen.getByLabelText(/Første udstedelsesdato/),
    ).toBeInTheDocument();
    expect(
      screen.getByLabelText(/Betalingsfrist i dage/),
    ).toBeInTheDocument();
    expect(screen.getByLabelText(/Linje 1 beskrivelse/)).toBeInTheDocument();
  });

  test("submitting the modal POSTs the template payload and reloads", async () => {
    const fetchMock = vi.fn();
    mockFetch({
      ...routes({ templates: [] }),
      "GET /api/companies/acme-aps/contacts": {
        contacts: {
          slug: "acme-aps",
          company: { name: "Acme ApS", cvr: "DK12345678", country: "DK", currency: "DKK", fiscalYearStartMonth: 1, fiscalYearLabelStrategy: "calendar" },
          fiscalYears: [],
          customers: [],
          vendors: [],
        },
      },
      "POST /api/companies/acme-aps/recurring-invoices": {
        template: {
          templateId: 42,
          name: "Ny månedlig",
          interval: "monthly",
          firstIssueDate: "2026-07-01",
        },
      },
    });
    void fetchMock;
    renderView();
    await screen.findByText(/Ingen skabeloner endnu/);

    await userEvent.click(
      screen.getAllByRole("button", { name: /Opret skabelon/ })[0]!,
    );
    await screen.findByRole("dialog", { name: /Opret faktura-skabelon/ });

    await userEvent.type(
      screen.getByLabelText(/Skabelonens navn/),
      "Ny månedlig",
    );
    await userEvent.type(
      screen.getByLabelText(/Første udstedelsesdato/),
      "2026-07-01",
    );
    await userEvent.type(screen.getByLabelText(/Kundens navn/), "Kunde ApS");
    await userEvent.type(
      screen.getByLabelText(/Linje 1 beskrivelse/),
      "Månedlig ydelse",
    );
    await userEvent.type(screen.getByLabelText(/Linje 1 antal/), "1");
    await userEvent.type(
      screen.getByLabelText(/Linje 1 enhedspris/),
      "1500",
    );

    const dialog = await screen.findByRole("dialog", {
      name: /Opret faktura-skabelon/,
    });
    const { getByRole } = within(dialog);
    await userEvent.click(getByRole("button", { name: /^Opret skabelon$/ }));

    await waitFor(() => {
      const calls = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls;
      const createCall = calls.find(
        ([u, init]) =>
          typeof u === "string" &&
          u === "/api/companies/acme-aps/recurring-invoices" &&
          (init as RequestInit | undefined)?.method === "POST",
      );
      expect(createCall).toBeTruthy();
      const body = JSON.parse(String((createCall![1] as RequestInit).body));
      expect(body.confirm).toBe(true);
      expect(body.name).toBe("Ny månedlig");
      expect(body.interval).toBe("monthly");
      expect(body.firstIssueDate).toBe("2026-07-01");
      expect(Array.isArray(body.lines)).toBe(true);
      expect(body.lines).toHaveLength(1);
      expect(body.lines[0].description).toBe("Månedlig ydelse");
      expect(body.lines[0].quantity).toBe(1);
      expect(body.lines[0].unitPriceExVat).toBe(1500);
    });
  });

  test("submitting the modal without name shows a validation error", async () => {
    mockFetch({
      ...routes({ templates: [] }),
      "GET /api/companies/acme-aps/contacts": {
        contacts: {
          slug: "acme-aps",
          company: { name: "Acme ApS", cvr: "DK12345678", country: "DK", currency: "DKK", fiscalYearStartMonth: 1, fiscalYearLabelStrategy: "calendar" },
          fiscalYears: [],
          customers: [],
          vendors: [],
        },
      },
    });
    renderView();
    await screen.findByText(/Ingen skabeloner endnu/);

    await userEvent.click(
      screen.getAllByRole("button", { name: /Opret skabelon/ })[0]!,
    );
    await screen.findByRole("dialog", { name: /Opret faktura-skabelon/ });

    const dialog = await screen.findByRole("dialog", {
      name: /Opret faktura-skabelon/,
    });
    const { getByRole } = within(dialog);
    await userEvent.click(getByRole("button", { name: /^Opret skabelon$/ }));

    expect(
      await screen.findByText(/Angiv et navn på skabelonen/),
    ).toBeInTheDocument();

    // No POST was made — only the GETs (recurring-invoices + fiscal-years +
    // contacts).
    const calls = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls;
    const createCall = calls.find(
      ([u, init]) =>
        typeof u === "string" &&
        u === "/api/companies/acme-aps/recurring-invoices" &&
        (init as RequestInit | undefined)?.method === "POST",
    );
    expect(createCall).toBeUndefined();
  });
});
