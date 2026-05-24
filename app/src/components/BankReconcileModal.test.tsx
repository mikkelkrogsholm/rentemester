// Cockpit-driven settlement of an unmatched bank row against an open invoice
// (#365). The owner picks an open sales invoice; the modal POSTs to the
// existing `/invoices/settle` write endpoint with the bank-transaction id, so
// no new bookkeeping path is introduced — only a UI on top of the same core
// the CLI and MCP use.

import { describe, expect, test, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { BankReconcileModal } from "./BankReconcileModal";
import { invoices, mockFetch } from "../test/fixtures";

function noop() {}

const TX = {
  id: 42,
  date: "2026-04-15",
  text: "Indbetaling Beta ApS",
  amount: 6250,
  currency: "DKK",
};

function settleRoute(over: Record<string, unknown> = {}) {
  return {
    "GET /api/companies/acme-aps/invoices": { invoices: invoices() },
    "POST /api/companies/acme-aps/invoices/settle": {
      settlement: {
        entryId: 101,
        paymentId: 7,
        principalAmount: 6250,
        claimAmount: 0,
        invoiceNumber: "2026-00002",
        openBalance: 0,
        ...over,
      },
    },
  };
}

describe("BankReconcileModal", () => {
  test("renders the dialog with the bank-transaction context", async () => {
    mockFetch(settleRoute());
    render(
      <BankReconcileModal
        slug="acme-aps"
        transaction={TX}
        onReconciled={noop}
        onClose={noop}
      />,
    );
    expect(
      await screen.findByRole("dialog", { name: /Bogfør banktransaktion/i }),
    ).toBeInTheDocument();
    // The bank-row context is shown so the owner knows what they are matching.
    expect(screen.getByText(/Indbetaling Beta ApS/)).toBeInTheDocument();
    expect(screen.getByText(/2026-04-15/)).toBeInTheDocument();
  });

  test("lists only open invoices in the picker", async () => {
    mockFetch(settleRoute());
    render(
      <BankReconcileModal
        slug="acme-aps"
        transaction={TX}
        onReconciled={noop}
        onClose={noop}
      />,
    );
    // Wait for the picker to render.
    const select = (await screen.findByLabelText(
      /Match mod faktura/i,
    )) as HTMLSelectElement;
    const options = Array.from(select.options).map((o) => o.textContent ?? "");
    // The paid invoice (2026-00001) must not appear — only the overdue/open one.
    expect(options.join(" ")).toContain("2026-00002");
    expect(options.join(" ")).not.toContain("2026-00001");
  });

  test("posts the settlement and reloads on success", async () => {
    mockFetch(settleRoute());
    const onReconciled = vi.fn();
    render(
      <BankReconcileModal
        slug="acme-aps"
        transaction={TX}
        onReconciled={onReconciled}
        onClose={noop}
      />,
    );
    const select = (await screen.findByLabelText(
      /Match mod faktura/i,
    )) as HTMLSelectElement;
    await userEvent.selectOptions(select, "2");
    await userEvent.click(screen.getByRole("button", { name: /Bogfør/ }));

    await waitFor(() => {
      const calls = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls;
      const settleCall = calls.find((c) =>
        String(c[0]).includes("/invoices/settle"),
      );
      expect(settleCall).toBeDefined();
      const init = settleCall![1] as RequestInit;
      expect(init.method).toBe("POST");
      const sent = JSON.parse(String(init.body));
      expect(sent.invoiceDocumentId).toBe(2);
      expect(sent.bankTransactionId).toBe(42);
      expect(sent.paymentDate).toBe("2026-04-15");
      expect(sent.confirm).toBe(true);
    });
    expect(onReconciled).toHaveBeenCalled();
    // Receipt mentions the resulting journal entry id.
    expect(
      await screen.findByText(/2026-00002/),
    ).toBeInTheDocument();
  });

  test("a backend rejection (regelvalidering) is shown as an error banner", async () => {
    mockFetch({
      "GET /api/companies/acme-aps/invoices": { invoices: invoices() },
      "POST /api/companies/acme-aps/invoices/settle": {
        __error: {
          code: "validation",
          message: "Betalingen overstiger fakturaens åbne saldo.",
        },
      },
    });
    const onReconciled = vi.fn();
    render(
      <BankReconcileModal
        slug="acme-aps"
        transaction={TX}
        onReconciled={onReconciled}
        onClose={noop}
      />,
    );
    const select = (await screen.findByLabelText(
      /Match mod faktura/i,
    )) as HTMLSelectElement;
    await userEvent.selectOptions(select, "2");
    await userEvent.click(screen.getByRole("button", { name: /Bogfør/ }));
    expect(
      await screen.findByText(/Betalingen overstiger/),
    ).toBeInTheDocument();
    // A rejected posting must not signal success to the parent view.
    expect(onReconciled).not.toHaveBeenCalled();
  });

  test("a 409 backup-lock conflict is shown as a kind lock banner", async () => {
    mockFetch({
      "GET /api/companies/acme-aps/invoices": { invoices: invoices() },
      "POST /api/companies/acme-aps/invoices/settle": {
        __error: {
          code: "conflict",
          message: "Bogføring er låst: en ugentlig backup er overskredet.",
        },
      },
    });
    render(
      <BankReconcileModal
        slug="acme-aps"
        transaction={TX}
        onReconciled={noop}
        onClose={noop}
      />,
    );
    const select = (await screen.findByLabelText(
      /Match mod faktura/i,
    )) as HTMLSelectElement;
    await userEvent.selectOptions(select, "2");
    await userEvent.click(screen.getByRole("button", { name: /Bogfør/ }));
    expect(
      await screen.findByText("Bogføringen er låst"),
    ).toBeInTheDocument();
  });

  test("when no open invoices exist the picker explains why", async () => {
    mockFetch({
      "GET /api/companies/acme-aps/invoices": {
        invoices: invoices({
          invoices: [],
          totalGross: 0,
          totalOpen: 0,
          overdueCount: 0,
        }),
      },
    });
    render(
      <BankReconcileModal
        slug="acme-aps"
        transaction={TX}
        onReconciled={noop}
        onClose={noop}
      />,
    );
    expect(
      await screen.findByText(/Ingen åbne fakturaer at matche mod/),
    ).toBeInTheDocument();
    // The Bogfør action must be disabled when there is nothing to match against.
    expect(screen.getByRole("button", { name: /Bogfør/ })).toBeDisabled();
  });
});
