// Tests for app/src/views/RecurringInvoicesView.tsx (#435 + earlier slices).
//
// Covers the Deaktivér action: the button is visible on active templates,
// the confirm-dialog gate must be passed, the API call carries the optional
// reason from the prompt, and a server failure is surfaced as a banner.
// Inactive templates do NOT show the Generér / Deaktivér actions.

import { describe, expect, test, vi, beforeEach, afterEach } from "vitest";
import { screen, waitFor } from "@testing-library/react";
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
});
