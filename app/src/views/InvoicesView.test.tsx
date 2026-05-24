import { describe, expect, test, vi } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { InvoicesView } from "./InvoicesView";
import { renderAt } from "../test/render";
import { invoices, mockFetch } from "../test/fixtures";

function route(over = {}) {
  return {
    "GET /api/companies/acme-aps/invoices": { invoices: invoices(over) },
  };
}

function renderView() {
  return renderAt(<InvoicesView />, {
    route: "/companies/acme-aps/fakturaer",
    path: "/companies/:slug/fakturaer",
  });
}

describe("InvoicesView — Fakturaer", () => {
  test("lists the issued invoices with their status", async () => {
    mockFetch(route());
    renderView();
    expect(
      await screen.findByRole("heading", { name: "Acme ApS" }),
    ).toBeInTheDocument();
    expect(screen.getByText("2026-00001")).toBeInTheDocument();
    expect(screen.getByText("Betalt")).toBeInTheDocument();
    expect(screen.getByText(/Forfalden/)).toBeInTheDocument();
  });

  test("shows a graceful empty state when there are no invoices", async () => {
    mockFetch(route({ invoices: [], totalGross: 0, totalOpen: 0, overdueCount: 0 }));
    renderView();
    expect(
      await screen.findByText(/Ingen fakturaer endnu/),
    ).toBeInTheDocument();
  });

  test("the fiscal-year selector reloads for the chosen year", async () => {
    mockFetch(route());
    renderView();
    const select = await screen.findByLabelText("Vælg regnskabsår");
    await userEvent.selectOptions(select, "2025");
    const calls = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls;
    const lastUrl = String(calls[calls.length - 1]![0]);
    expect(lastUrl).toContain("year=2025");
  });

  test("an archived year shows an honest 'not available' state", async () => {
    mockFetch(route({ archived: true, selectedYear: "2025", invoices: [] }));
    renderView();
    expect(
      await screen.findByText(/Fakturaer er ikke tilgængelige for 2025/),
    ).toBeInTheDocument();
  });
});

// --------------------------------------------------------------------------
// #213, slice 4 — human write-actions on the Fakturaer view.
// --------------------------------------------------------------------------

describe("InvoicesView — write actions", () => {
  test("the Udsted faktura action opens the issue modal", async () => {
    mockFetch(route());
    renderView();
    await screen.findByRole("heading", { name: "Acme ApS" });
    await userEvent.click(
      screen.getByRole("button", { name: "Udsted faktura" }),
    );
    expect(
      screen.getByRole("dialog", { name: "Udsted faktura" }),
    ).toBeInTheDocument();
  });

  test("the Udsted faktura action is hidden for an archived year", async () => {
    mockFetch(route({ archived: true, selectedYear: "2025", invoices: [] }));
    renderView();
    await screen.findByText(/Fakturaer er ikke tilgængelige for 2025/);
    expect(
      screen.queryByRole("button", { name: "Udsted faktura" }),
    ).not.toBeInTheDocument();
  });

  // Issue #385 — every invoice in this view is already posted (the
  // `InvoiceStatus` union has no `draft` and the empty-state copy says
  // "Udstedte fakturaer vises her, så snart de er bogført"). Showing a
  // "Bogfør" button on already-posted rows tempts the owner into a
  // double-post — the cockpit must never render it from this view.
  test("Bogfør is never offered from this view (all rows are already posted)", async () => {
    const allStatuses: Array<
      "open" | "paid" | "credited" | "refunded" | "overpaid" | "written_off" | "overdue"
    > = ["open", "paid", "credited", "refunded", "overpaid", "written_off", "overdue"];
    mockFetch(
      route({
        invoices: allStatuses.map((status, idx) => ({
          documentId: idx + 1,
          invoiceNo: `2026-${String(idx + 1).padStart(5, "0")}`,
          invoiceDate: "2026-03-15",
          customerName: `Kunde ${idx + 1}`,
          grossAmount: 1000,
          openBalance: status === "paid" || status === "written_off" ? 0 : 1000,
          currency: "DKK",
          status,
          effectiveDueDate: "2026-04-14",
          overdueDays: status === "overdue" ? 21 : 0,
        })),
      }),
    );
    renderView();
    await screen.findByRole("heading", { name: "Acme ApS" });
    expect(
      screen.queryAllByRole("button", { name: "Bogfør" }),
    ).toHaveLength(0);
  });

  test("Afstem settles the invoice against a bank reference", async () => {
    mockFetch({
      "GET /api/companies/acme-aps/invoices": { invoices: invoices() },
      "POST /api/companies/acme-aps/invoices/settle": {
        settlement: {
          entryId: 20,
          paymentId: 5,
          principalAmount: 6250,
          claimAmount: 0,
          invoiceNumber: "2026-00002",
          openBalance: 0,
        },
      },
    });
    renderView();
    await screen.findByRole("heading", { name: "Acme ApS" });
    // Only the second (overdue, open-balance) invoice offers "Afstem".
    await userEvent.click(screen.getByRole("button", { name: "Afstem" }));
    await userEvent.type(
      screen.getByLabelText("Bankreference"),
      "INV-0990",
    );
    await userEvent.click(
      screen.getByRole("button", { name: "Afstem faktura" }),
    );

    await waitFor(() => {
      const calls = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls;
      const settleCall = calls.find((c) =>
        String(c[0]).includes("/invoices/settle"),
      );
      expect(settleCall).toBeDefined();
      const sent = JSON.parse(String((settleCall![1] as RequestInit).body));
      expect(sent.invoiceDocumentId).toBe(2);
      expect(sent.bankTransactionReference).toBe("INV-0990");
      expect(sent.confirm).toBe(true);
    });
  });

  // #378 — the row's primary action is the PDF download. Without it, the
  // owner has to open the CLI to send the invoice to the customer.
  test("each invoice row offers Hent PDF pointing at the cockpit PDF route", async () => {
    mockFetch(route());
    renderView();
    await screen.findByRole("heading", { name: "Acme ApS" });
    const links = screen.getAllByRole("link", { name: "Hent PDF" });
    // The default fixture renders two invoices.
    expect(links.length).toBeGreaterThanOrEqual(2);
    for (const link of links) {
      expect(link.getAttribute("href")).toMatch(
        /^\/api\/companies\/acme-aps\/invoices\/\d+\/pdf$/,
      );
      expect(link.getAttribute("target")).toBe("_blank");
    }
  });

  test("Afstem is offered only for invoices with an open balance", async () => {
    mockFetch(route());
    renderView();
    await screen.findByRole("heading", { name: "Acme ApS" });
    // Fixture: invoice 1 is paid (openBalance 0), invoice 2 is overdue
    // (openBalance 6250) — so exactly one "Afstem" button is rendered.
    expect(screen.getAllByRole("button", { name: "Afstem" })).toHaveLength(1);
  });
});
