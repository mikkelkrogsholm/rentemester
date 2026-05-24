import { describe, expect, test, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { InvoiceIssueModal } from "./InvoiceIssueModal";
import { companySettings, contacts, mockFetch } from "../test/fixtures";

function noop() {}

/**
 * The company-settings route the modal fetches on mount to decide whether to
 * warn about missing payment details (#284). By default the company HAS a
 * bank account, so no warning is shown.
 */
function companyRoute(paymentConfigured = true) {
  return {
    "GET /api/companies/acme-aps/company": {
      company: companySettings({
        payment: paymentConfigured
          ? {
              bankName: "Danske Bank",
              registrationNo: "1234",
              accountNo: "0001234567",
              iban: null,
            }
          : null,
      }),
    },
  };
}

/** A contacts route returning one customer "Kunde A/S" with a known CVR. */
function contactsRoute() {
  return {
    "GET /api/companies/acme-aps/contacts": { contacts: contacts() },
  };
}

/** Routes the invoice-issue POST to a success summary (+ the company route). */
function issueRoute(over: Record<string, unknown> = {}) {
  return {
    ...companyRoute(),
    "POST /api/companies/acme-aps/invoices/issue": {
      invoice: {
        documentId: 7,
        invoiceNumber: "2026-00007",
        netAmount: 2000,
        vatRate: 0.25,
        vatAmount: 500,
        grossAmount: 2500,
        lines: [
          {
            description: "Bogføring maj",
            quantity: 2,
            unitPriceExVat: 1000,
            lineTotalExVat: 2000,
          },
        ],
        ...over,
      },
    },
  };
}

/** Fills the minimal required fields — date + one complete line item. */
async function fillMinimal() {
  await userEvent.type(screen.getByLabelText("Fakturadato"), "2026-05-16");
  await userEvent.type(
    screen.getByLabelText("Linje 1 beskrivelse"),
    "Bogføring maj",
  );
  await userEvent.type(screen.getByLabelText("Linje 1 antal"), "2");
  await userEvent.type(screen.getByLabelText("Linje 1 enhedspris"), "1000");
}

describe("InvoiceIssueModal", () => {
  test("renders the dialog with date, VAT and line-item fields", () => {
    mockFetch(companyRoute());
    render(
      <InvoiceIssueModal slug="acme-aps" onIssued={noop} onClose={noop} />,
    );
    expect(
      screen.getByRole("dialog", { name: "Udsted faktura" }),
    ).toBeInTheDocument();
    expect(screen.getByLabelText("Fakturadato")).toBeInTheDocument();
    expect(screen.getByLabelText("Momssats (%)")).toBeInTheDocument();
    expect(
      screen.getByLabelText("Linje 1 beskrivelse"),
    ).toBeInTheDocument();
  });

  test("Tilføj linje adds another editable line-item row", async () => {
    mockFetch(companyRoute());
    render(
      <InvoiceIssueModal slug="acme-aps" onIssued={noop} onClose={noop} />,
    );
    expect(
      screen.queryByLabelText("Linje 2 beskrivelse"),
    ).not.toBeInTheDocument();
    await userEvent.click(
      screen.getByRole("button", { name: "Tilføj linje" }),
    );
    expect(
      screen.getByLabelText("Linje 2 beskrivelse"),
    ).toBeInTheDocument();
  });

  test("issuing POSTs only the human's essentials — no computed amounts", async () => {
    mockFetch(issueRoute());
    const onIssued = vi.fn();
    render(
      <InvoiceIssueModal
        slug="acme-aps"
        onIssued={onIssued}
        onClose={noop}
      />,
    );
    await fillMinimal();
    await userEvent.click(
      screen.getByRole("button", { name: "Udsted faktura" }),
    );

    const calls = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls;
    const issueCall = calls.find((c) =>
      String(c[0]).includes("/invoices/issue"),
    );
    expect(issueCall).toBeDefined();
    const init = issueCall![1] as RequestInit;
    expect(init.method).toBe("POST");
    const sent = JSON.parse(String(init.body));
    // The human supplied only description/quantity/unitPrice + a VAT rate —
    // Rentemester computes net/VAT/gross server-side, so they are NOT sent.
    expect(sent.lines).toEqual([
      { description: "Bogføring maj", quantity: 2, unitPriceExVat: 1000 },
    ]);
    expect(sent.vatRatePercent).toBe(25);
    expect(sent.issueDate).toBe("2026-05-16");
    expect(sent.totals).toBeUndefined();
    expect(onIssued).toHaveBeenCalled();
  });

  test("shows the Rentemester-computed totals as a receipt after success", async () => {
    mockFetch(issueRoute());
    render(
      <InvoiceIssueModal slug="acme-aps" onIssued={noop} onClose={noop} />,
    );
    await fillMinimal();
    await userEvent.click(
      screen.getByRole("button", { name: "Udsted faktura" }),
    );
    expect(await screen.findByText(/2026-00007/)).toBeInTheDocument();
    // The computed gross is shown back to the human.
    expect(screen.getByText(/I alt inkl\. moms/)).toBeInTheDocument();
  });

  test("a missing date is reported without POSTing", async () => {
    mockFetch(companyRoute());
    render(
      <InvoiceIssueModal slug="acme-aps" onIssued={noop} onClose={noop} />,
    );
    await userEvent.type(
      screen.getByLabelText("Linje 1 beskrivelse"),
      "Bogføring",
    );
    await userEvent.type(screen.getByLabelText("Linje 1 antal"), "1");
    await userEvent.type(screen.getByLabelText("Linje 1 enhedspris"), "500");
    await userEvent.click(
      screen.getByRole("button", { name: "Udsted faktura" }),
    );
    expect(
      await screen.findByText(/Angiv en fakturadato/),
    ).toBeInTheDocument();
  });

  test("a non-numeric quantity is reported without POSTing", async () => {
    mockFetch(companyRoute());
    render(
      <InvoiceIssueModal slug="acme-aps" onIssued={noop} onClose={noop} />,
    );
    await userEvent.type(screen.getByLabelText("Fakturadato"), "2026-05-16");
    await userEvent.type(
      screen.getByLabelText("Linje 1 beskrivelse"),
      "Bogføring",
    );
    await userEvent.type(screen.getByLabelText("Linje 1 antal"), "to");
    await userEvent.type(screen.getByLabelText("Linje 1 enhedspris"), "500");
    await userEvent.click(
      screen.getByRole("button", { name: "Udsted faktura" }),
    );
    expect(
      await screen.findByText(/antal skal være et tal/),
    ).toBeInTheDocument();
  });

  test("a 409 backup-lock conflict is shown as a kind lock banner", async () => {
    mockFetch({
      ...companyRoute(),
      "POST /api/companies/acme-aps/invoices/issue": {
        __error: {
          code: "conflict",
          message: "Bogføring er låst: en ugentlig backup er overskredet.",
        },
      },
    });
    const onClose = vi.fn();
    render(
      <InvoiceIssueModal
        slug="acme-aps"
        onIssued={noop}
        onClose={onClose}
      />,
    );
    await fillMinimal();
    await userEvent.click(
      screen.getByRole("button", { name: "Udsted faktura" }),
    );
    expect(
      await screen.findByText("Bogføringen er låst"),
    ).toBeInTheDocument();
    expect(onClose).not.toHaveBeenCalled();
  });

  test("a validation error from the server is shown as an error banner", async () => {
    mockFetch({
      ...companyRoute(),
      "POST /api/companies/acme-aps/invoices/issue": {
        __error: { code: "bad_request", message: "buyer.name is required" },
      },
    });
    render(
      <InvoiceIssueModal slug="acme-aps" onIssued={noop} onClose={noop} />,
    );
    await fillMinimal();
    await userEvent.click(
      screen.getByRole("button", { name: "Udsted faktura" }),
    );
    expect(
      await screen.findByText("buyer.name is required"),
    ).toBeInTheDocument();
  });

  test("Annullér closes the modal without issuing", async () => {
    mockFetch(companyRoute());
    const onClose = vi.fn();
    render(
      <InvoiceIssueModal
        slug="acme-aps"
        onIssued={noop}
        onClose={onClose}
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: "Annullér" }));
    expect(onClose).toHaveBeenCalled();
  });

  // #284 — issuing an invoice without a bank account warns the human that the
  // invoice will carry no payment instructions.
  test("warns when the company has no payment details configured", async () => {
    mockFetch(companyRoute(false));
    render(
      <InvoiceIssueModal slug="acme-aps" onIssued={noop} onClose={noop} />,
    );
    expect(
      await screen.findByText(/ingen bankkonto registreret/i),
    ).toBeInTheDocument();
  });

  // #380 — picking an existing customer from Kontakter prefills the buyer
  // fields so the owner does not retype name/CVR/address every invoice. The
  // fields stay editable: the invoice's buyer block is a per-invoice snapshot.
  test("selecting an existing customer prefills buyer name, CVR and address", async () => {
    mockFetch({
      ...companyRoute(),
      ...contactsRoute(),
    });
    render(
      <InvoiceIssueModal slug="acme-aps" onIssued={noop} onClose={noop} />,
    );
    const picker = await screen.findByLabelText("Vælg kunde");
    await userEvent.selectOptions(picker, "1");
    const buyer = screen.getByLabelText("Kunde") as HTMLInputElement;
    expect(buyer.value).toBe("Kunde A/S");
    const cvrInput = screen.getByLabelText(
      "Kunde CVR/moms",
    ) as HTMLInputElement;
    expect(cvrInput.value).toBe("DK87654321");
  });

  test("no warning when the company has payment details", async () => {
    mockFetch(companyRoute(true));
    render(
      <InvoiceIssueModal slug="acme-aps" onIssued={noop} onClose={noop} />,
    );
    // Let the company-settings fetch settle.
    await screen.findByLabelText("Fakturadato");
    expect(
      screen.queryByText(/ingen bankkonto registreret/i),
    ).not.toBeInTheDocument();
  });
});
