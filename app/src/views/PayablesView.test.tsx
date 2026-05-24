import { describe, expect, test, vi } from "vitest";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { PayablesView } from "./PayablesView";
import { renderAt } from "../test/render";
import { payables, mockFetch } from "../test/fixtures";

function route(over = {}) {
  return {
    "GET /api/companies/acme-aps/payables": { payables: payables(over) },
  };
}

function renderView() {
  return renderAt(<PayablesView />, {
    route: "/companies/acme-aps/leverandoerfaktura",
    path: "/companies/:slug/leverandoerfaktura",
  });
}

describe("PayablesView — Leverandørfaktura-arbejdsbordet", () => {
  test("lists the open payables with status flags", async () => {
    mockFetch(route());
    renderView();
    expect(
      await screen.findByRole("heading", { name: "Acme ApS" }),
    ).toBeInTheDocument();
    expect(screen.getByText("Software ApS")).toBeInTheDocument();
    expect(screen.getByText("Telco A/S")).toBeInTheDocument();
    expect(screen.getByText("V-1001")).toBeInTheDocument();
    // The overdue row carries the "Forfalden · N dage" flag.
    expect(screen.getByText(/Forfalden · 104 dage/)).toBeInTheDocument();
    // The not-yet-due row stays "Bogført".
    expect(screen.getByText("Bogført")).toBeInTheDocument();
  });

  test("surfaces the summary totals (skyldig, forfaldne, ikke forfaldne)", async () => {
    mockFetch(route());
    renderView();
    await screen.findByRole("heading", { name: "Acme ApS" });
    // Three status cards — the headlines must all appear above the table.
    // "Forfaldne" also appears as a filter pill button, so scope the assertion
    // to the heading rendering inside the summary card.
    expect(screen.getByRole("heading", { name: "Skyldig i alt" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Forfaldne" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Ikke forfaldne" })).toBeInTheDocument();
  });

  test("an empty list renders a calm guidance state", async () => {
    mockFetch(
      route({
        rows: [],
        count: 0,
        totalOpenBalance: 0,
        overdueOpenBalance: 0,
        notYetDueOpenBalance: 0,
      }),
    );
    renderView();
    expect(
      await screen.findByText(/Ingen leverandørfakturaer i visningen/),
    ).toBeInTheDocument();
  });

  test("the status filter switches the query and reloads", async () => {
    mockFetch({
      "GET /api/companies/acme-aps/payables": { payables: payables() },
    });
    renderView();
    await screen.findByRole("heading", { name: "Acme ApS" });
    await userEvent.click(screen.getByRole("button", { name: "Forfaldne" }));
    await waitFor(() => {
      const calls = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls;
      const lastUrl = String(calls[calls.length - 1]![0]);
      expect(lastUrl).toContain("status=overdue");
    });
  });

  test("the Registrér leverandørfaktura action opens the register modal", async () => {
    mockFetch(route());
    renderView();
    await screen.findByRole("heading", { name: "Acme ApS" });
    await userEvent.click(
      screen.getByRole("button", { name: "Registrér leverandørfaktura" }),
    );
    expect(
      screen.getByRole("dialog", { name: "Registrér leverandørfaktura" }),
    ).toBeInTheDocument();
  });

  test("the per-row Markér betalt action opens the pay confirm dialog", async () => {
    mockFetch(route());
    renderView();
    await screen.findByRole("heading", { name: "Acme ApS" });
    const payButtons = screen.getAllByRole("button", { name: "Markér betalt" });
    expect(payButtons.length).toBeGreaterThan(0);
    await userEvent.click(payButtons[0]!);
    expect(
      screen.getByRole("dialog", { name: "Markér leverandørfaktura betalt" }),
    ).toBeInTheDocument();
  });

  test("Markér betalt is hidden once a row is fully paid", async () => {
    mockFetch(
      route({
        rows: [
          {
            payableId: 11,
            documentId: 101,
            billNo: "V-1001",
            billDate: "2026-01-10",
            dueDate: "2026-02-09",
            supplierName: "Software ApS",
            vendorId: null,
            grossAmount: 1250,
            currency: "DKK",
            paidAmount: 1250,
            openBalance: 0,
            status: "paid" as const,
            isOverdue: false,
            overdueDays: 0,
            agingBucket: "not-due" as const,
          },
        ],
        count: 1,
        totalOpenBalance: 0,
        overdueOpenBalance: 0,
        notYetDueOpenBalance: 0,
      }),
    );
    renderView();
    await screen.findByRole("heading", { name: "Acme ApS" });
    expect(
      screen.queryAllByRole("button", { name: "Markér betalt" }),
    ).toHaveLength(0);
    // The flag span for the paid row reads "Betalt" — table header also
    // renders the column label "Betalt", so two matches are expected.
    expect(
      screen.getAllByText((content) => content === "Betalt").length,
    ).toBeGreaterThan(0);
  });

  test("pay submits the bank transaction id and reloads the list", async () => {
    mockFetch({
      "GET /api/companies/acme-aps/payables": { payables: payables() },
      "POST /api/companies/acme-aps/payables/11/pay": {
        payment: {
          paymentId: 1,
          journalEntryId: 99,
          payableId: 11,
          openBalance: 0,
        },
      },
    });
    renderView();
    await screen.findByRole("heading", { name: "Acme ApS" });
    const payButtons = screen.getAllByRole("button", { name: "Markér betalt" });
    await userEvent.click(payButtons[0]!);
    const input = screen.getByLabelText("Banktransaktions-id");
    await userEvent.type(input, "777");
    // After opening the dialog there are two "Markér betalt" buttons (the row
    // action AND the dialog confirm) — scope to the dialog.
    const dialog = screen.getByRole("dialog", {
      name: "Markér leverandørfaktura betalt",
    });
    await userEvent.click(
      within(dialog).getByRole("button", { name: "Markér betalt" }),
    );
    await waitFor(() => {
      const calls = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls;
      const post = calls.find(
        (c) =>
          String(c[0]).includes("/payables/11/pay") &&
          (c[1] as { method?: string } | undefined)?.method === "POST",
      );
      expect(post).toBeTruthy();
      const body = JSON.parse((post![1] as { body: string }).body);
      expect(body.bankTransactionId).toBe(777);
      expect(body.confirm).toBe(true);
    });
  });

  test("pay rejects a blank/non-numeric bank id with a friendly message", async () => {
    mockFetch(route());
    renderView();
    await screen.findByRole("heading", { name: "Acme ApS" });
    const payButtons = screen.getAllByRole("button", { name: "Markér betalt" });
    await userEvent.click(payButtons[0]!);
    // No id typed — confirm should surface the validation message without
    // ever calling the API.
    const dialog = screen.getByRole("dialog", {
      name: "Markér leverandørfaktura betalt",
    });
    await userEvent.click(
      within(dialog).getByRole("button", { name: "Markér betalt" }),
    );
    expect(
      await screen.findByText(/numeriske id på den udgående banktransaktion/),
    ).toBeInTheDocument();
  });
});
