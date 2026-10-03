import { describe, expect, test, vi } from "bun:test";
import { fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { InvoiceIssueModal } from "./InvoiceIssueModal";
import { companySettings, contacts, mockFetch } from "../test/fixtures";
import { stubGlobal } from "../test/globals";

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
  test("renders the dialog with date, VAT and line-item fields", async () => {
    mockFetch(companyRoute());
    render(
      <InvoiceIssueModal slug="acme-aps" onIssued={noop} onClose={noop} />,
    );
    expect(
      screen.getByRole("dialog", { name: "Udsted faktura" }),
    ).toBeInTheDocument();
    expect(await screen.findByLabelText("Fakturadato")).toBeInTheDocument();
    expect(screen.getByLabelText("Momssats (%)")).toBeInTheDocument();
    expect(
      screen.getByLabelText("Linje 1 beskrivelse"),
    ).toBeInTheDocument();
  });

  test("prefills seller CVR from company settings", async () => {
    mockFetch(companyRoute());
    render(
      <InvoiceIssueModal slug="acme-aps" onIssued={noop} onClose={noop} />,
    );

    expect(
      await screen.findByDisplayValue("DK12345678"),
    ).toBe(screen.getByLabelText("Sælger CVR/moms"));
  });

  test("does not overwrite a seller CVR typed before settings resolve", async () => {
    let resolveCompany!: (response: Response) => void;
    stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL) => {
        const url = typeof input === "string" ? input : input.toString();
        const path = url.replace(/^https?:\/\/[^/]+/, "").split("?")[0];
        if (path === "/api/companies/acme-aps/company") {
          return new Promise<Response>((resolve) => {
            resolveCompany = resolve;
          });
        }
        return Promise.resolve(
          new Response(
            JSON.stringify({
              ok: true,
              contacts: { customers: [], vendors: [] },
            }),
            { status: 200, headers: { "content-type": "application/json" } },
          ),
        );
      }),
    );
    render(
      <InvoiceIssueModal slug="acme-aps" onIssued={noop} onClose={noop} />,
    );

    const sellerVat = screen.getByLabelText("Sælger CVR/moms");
    await userEvent.type(sellerVat, "DK87654321");
    resolveCompany(
      new Response(
        JSON.stringify({
          ok: true,
          company: companySettings({ cvr: "DK12345678", payment: null }),
        }),
        { status: 200, headers: { "content-type": "application/json" } },
      ),
    );

    await screen.findByText(/udstedes uden betalingsoplysninger/);
    expect(sellerVat).toHaveValue("DK87654321");
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

    const calls = (globalThis.fetch as unknown as ReturnType<typeof vi.fn>).mock.calls;
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

  // #440 — Forhåndsvis button: must POST to /invoices/preview with the same
  // payload as Udsted, but never issue. The cockpit opens the returned PDF
  // blob in a new tab via URL.createObjectURL.
  describe("#440 — Forhåndsvis", () => {
    /**
     * Stubs `URL.createObjectURL` + `window.open` so the test can assert the
     * preview blob is rendered without a real browser. Returns the stubs so
     * individual tests can introspect what was opened.
     */
    function stubWindowOpen() {
      const createObjectURL = vi.fn(() => "blob:fake-preview-url");
      const revokeObjectURL = vi.fn();
      const open = vi.fn();
      stubGlobal("URL", {
        ...URL,
        createObjectURL,
        revokeObjectURL,
      });
      stubGlobal("open", open);
      return { createObjectURL, revokeObjectURL, open };
    }

    /**
     * `mockFetch` always returns the cockpit JSON envelope, but the preview
     * route returns binary PDF bytes. This helper installs a fetch that
     * returns the JSON envelope for issue/contacts/company routes and a fake
     * PDF blob for the preview route.
     */
    function mockFetchWithPreview() {
      const pdfBytes = new Uint8Array([0x25, 0x50, 0x44, 0x46, 0x2d, 0x31]); // %PDF-1
      stubGlobal(
        "fetch",
        vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
          const url = typeof input === "string" ? input : input.toString();
          const path = url.replace(/^https?:\/\/[^/]+/, "").split("?")[0];
          const method = (init?.method ?? "GET").toUpperCase();
          if (
            method === "POST" &&
            path === "/api/companies/acme-aps/invoices/preview"
          ) {
            return new Response(pdfBytes, {
              status: 200,
              headers: {
                "content-type": "application/pdf",
                "content-disposition":
                  "inline; filename*=UTF-8''2026-UDKAST.pdf",
              },
            });
          }
          if (method === "GET" && path === "/api/companies/acme-aps/company") {
            return new Response(
              JSON.stringify({
                ok: true,
                company: companySettings({
                  payment: {
                    bankName: "Danske Bank",
                    registrationNo: "1234",
                    accountNo: "0001234567",
                    iban: null,
                  },
                }),
              }),
              {
                status: 200,
                headers: { "content-type": "application/json" },
              },
            );
          }
          // contacts + any other route — empty success envelope so the modal
          // mounts without errors.
          return new Response(
            JSON.stringify({ ok: true, contacts: { customers: [], vendors: [] } }),
            {
              status: 200,
              headers: { "content-type": "application/json" },
            },
          );
        }),
      );
    }

    test("renders the Forhåndsvis button next to Udsted faktura", async () => {
      mockFetch(companyRoute());
      render(
        <InvoiceIssueModal slug="acme-aps" onIssued={noop} onClose={noop} />,
      );
      expect(
        await screen.findByRole("button", { name: "Forhåndsvis" }),
      ).toBeInTheDocument();
      expect(
        screen.getByRole("button", { name: "Udsted faktura" }),
      ).toBeInTheDocument();
    });

    test("clicking Forhåndsvis POSTs to /invoices/preview and opens the PDF blob", async () => {
      mockFetchWithPreview();
      const stubs = stubWindowOpen();
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
        screen.getByRole("button", { name: "Forhåndsvis" }),
      );

      // The preview endpoint received the SAME line/date/vat shape Udsted
      // would have sent — same source of truth for both calls.
      const calls = (globalThis.fetch as unknown as ReturnType<typeof vi.fn>).mock.calls;
      const previewCall = calls.find((c) =>
        String(c[0]).includes("/invoices/preview"),
      );
      expect(previewCall).toBeDefined();
      const init = previewCall![1] as RequestInit;
      expect(init.method).toBe("POST");
      const sent = JSON.parse(String(init.body));
      expect(sent.lines).toEqual([
        { description: "Bogføring maj", quantity: 2, unitPriceExVat: 1000 },
      ]);
      expect(sent.vatRatePercent).toBe(25);
      expect(sent.issueDate).toBe("2026-05-16");

      // The blob was handed to URL.createObjectURL and window.open — the
      // owner sees the PDF in a new tab.
      expect(stubs.createObjectURL).toHaveBeenCalled();
      expect(stubs.open).toHaveBeenCalledWith(
        "blob:fake-preview-url",
        "_blank",
        "noopener",
      );

      // Crucially: Forhåndsvis must NEVER touch the issue endpoint, and
      // must NEVER call onIssued — the ledger has not been mutated.
      const issueCall = calls.find((c) =>
        String(c[0]).includes("/invoices/issue"),
      );
      expect(issueCall).toBeUndefined();
      expect(onIssued).not.toHaveBeenCalled();
    });

    test("Forhåndsvis runs the same validation as Udsted — no fetch on bad input", async () => {
      mockFetchWithPreview();
      stubWindowOpen();
      render(
        <InvoiceIssueModal slug="acme-aps" onIssued={noop} onClose={noop} />,
      );
      // Click Forhåndsvis without filling the date — same red banner Udsted
      // would show, and the preview endpoint is NOT called.
      await userEvent.click(
        screen.getByRole("button", { name: "Forhåndsvis" }),
      );
      expect(
        await screen.findByText(/Angiv en fakturadato/),
      ).toBeInTheDocument();
      const calls = (globalThis.fetch as unknown as ReturnType<typeof vi.fn>).mock.calls;
      const previewCall = calls.find((c) =>
        String(c[0]).includes("/invoices/preview"),
      );
      expect(previewCall).toBeUndefined();
    });

    /**
     * Installs a fetch that fails the preview route with the UNIFIED cockpit
     * envelope `{ ok:false, errors:[message], code }` (#368) — the real shape
     * the server emits, not the dead `{ error: { code, message } }` shape the
     * old `previewInvoice` parsed (which swallowed every preview error).
     */
    function mockFetchPreviewError(code: string, message: string) {
      stubGlobal(
        "fetch",
        vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
          const url = typeof input === "string" ? input : input.toString();
          const path = url.replace(/^https?:\/\/[^/]+/, "").split("?")[0];
          const method = (init?.method ?? "GET").toUpperCase();
          if (
            method === "POST" &&
            path === "/api/companies/acme-aps/invoices/preview"
          ) {
            return new Response(
              JSON.stringify({ ok: false, errors: [message], code }),
              {
                status: code === "conflict" ? 409 : 400,
                headers: { "content-type": "application/json" },
              },
            );
          }
          return new Response(
            JSON.stringify({
              ok: true,
              company: companySettings({
                payment: {
                  bankName: "Danske Bank",
                  registrationNo: "1234",
                  accountNo: "0001234567",
                  iban: null,
                },
              }),
              contacts: { customers: [], vendors: [] },
            }),
            {
              status: 200,
              headers: { "content-type": "application/json" },
            },
          );
        }),
      );
    }

    test("a server validation error from preview is shown as an error banner", async () => {
      // #368 — the server returns the unified `{ ok:false, errors, code }`
      // envelope; `previewInvoice` must surface `errors[0]` as the message.
      mockFetchPreviewError("bad_request", "buyer.name is required");
      stubWindowOpen();
      render(
        <InvoiceIssueModal slug="acme-aps" onIssued={noop} onClose={noop} />,
      );
      await fillMinimal();
      await userEvent.click(
        screen.getByRole("button", { name: "Forhåndsvis" }),
      );
      expect(
        await screen.findByText("buyer.name is required"),
      ).toBeInTheDocument();
    });

    test("a 409 conflict from preview routes to the kind LockBanner, not the red error banner", async () => {
      // A `conflict` code (e.g. the backup lock) must be shown via LockBanner
      // ("Bogføringen er låst") — the curated message goes in the banner body.
      mockFetchPreviewError(
        "conflict",
        "Bogføring er låst: en ugentlig backup er overskredet.",
      );
      stubWindowOpen();
      render(
        <InvoiceIssueModal slug="acme-aps" onIssued={noop} onClose={noop} />,
      );
      await fillMinimal();
      await userEvent.click(
        screen.getByRole("button", { name: "Forhåndsvis" }),
      );
      expect(
        await screen.findByText("Bogføringen er låst"),
      ).toBeInTheDocument();
      // The curated lock message renders inside the banner body…
      expect(
        screen.getByText(/en ugentlig backup er overskredet/),
      ).toBeInTheDocument();
      // …and the message is NOT shown as a red error alert.
      const alerts = screen.queryAllByRole("alert");
      for (const a of alerts) {
        expect(a.textContent ?? "").not.toMatch(/backup er overskredet/);
      }
    });

    test("a pending preview only swaps the Forhåndsvis label — Udsted stays 'Udsted faktura'", async () => {
      // A single `busy` flag wrongly relabels BOTH buttons; only the button
      // whose action is in flight should show its progress text. Here the
      // preview never resolves, so Forhåndsvis reads "Henter…" while
      // "Udsted faktura" keeps its label (both disabled).
      stubGlobal(
        "fetch",
        vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
          const url = typeof input === "string" ? input : input.toString();
          const path = url.replace(/^https?:\/\/[^/]+/, "").split("?")[0];
          const method = (init?.method ?? "GET").toUpperCase();
          if (
            method === "POST" &&
            path === "/api/companies/acme-aps/invoices/preview"
          ) {
            // Never resolves — the preview stays in flight.
            return new Promise<Response>(() => {});
          }
          return new Response(
            JSON.stringify({
              ok: true,
              company: companySettings({
                payment: {
                  bankName: "Danske Bank",
                  registrationNo: "1234",
                  accountNo: "0001234567",
                  iban: null,
                },
              }),
              contacts: { customers: [], vendors: [] },
            }),
            { status: 200, headers: { "content-type": "application/json" } },
          );
        }),
      );
      stubWindowOpen();
      render(
        <InvoiceIssueModal slug="acme-aps" onIssued={noop} onClose={noop} />,
      );
      await fillMinimal();
      await userEvent.click(
        screen.getByRole("button", { name: "Forhåndsvis" }),
      );

      // Forhåndsvis flips to its progress label…
      const previewBtn = await screen.findByRole("button", { name: "Henter…" });
      expect(previewBtn).toBeDisabled();
      // …but Udsted keeps its label and must NOT read "Udsteder…".
      const issueBtn = screen.getByRole("button", { name: "Udsted faktura" });
      expect(issueBtn).toBeDisabled();
      expect(
        screen.queryByRole("button", { name: "Udsteder…" }),
      ).not.toBeInTheDocument();
    });
  });
});

describe("InvoiceIssueModal — complete amount and uncertain write handling", () => {
  test("keeps Danish thousands and decimal separators until the canonical parser submits", async () => {
    mockFetch(issueRoute());
    render(<InvoiceIssueModal slug="acme-aps" onIssued={noop} onClose={noop} />);
    await fillMinimal();
    const price = screen.getByLabelText("Linje 1 enhedspris");
    await userEvent.clear(price);
    await userEvent.type(price, "1.234,56");
    expect(price).toHaveValue("1.234,56");
    await userEvent.click(screen.getByRole("button", { name: "Udsted faktura" }));
    await screen.findByText(/udstedt/);
    const calls = (globalThis.fetch as unknown as ReturnType<typeof vi.fn>).mock.calls;
    const call = calls.find((entry) => String(entry[0]).endsWith("/invoices/issue"))!;
    expect(JSON.parse(String((call[1] as RequestInit).body)).lines[0].unitPriceExVat).toBe(1234.56);
  });
  test("an interrupted issue is not silently retryable and keeps the user's fields", async () => {
    mockFetch(companyRoute());
    const originalFetch = globalThis.fetch;
    const fetchSpy = vi.fn((input: RequestInfo | URL, init?: RequestInit) => {
      if (String(input).endsWith("/invoices/issue")) return Promise.reject(new TypeError("connection lost"));
      return originalFetch(input, init);
    });
    stubGlobal("fetch", fetchSpy);
    const onIssued = vi.fn();
    render(<InvoiceIssueModal slug="acme-aps" onIssued={onIssued} onClose={noop} />);
    await fillMinimal();
    await userEvent.click(screen.getByRole("button", { name: "Udsted faktura" }));
    expect(await screen.findByText(/Kontrollér fakturaoversigten/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Udsted faktura" })).toBeDisabled();
    expect(screen.getByLabelText("Linje 1 beskrivelse")).toHaveValue("Bogføring maj");
    expect(fetchSpy.mock.calls.filter((entry) => String(entry[0]).endsWith("/invoices/issue"))).toHaveLength(1);
    expect(onIssued).not.toHaveBeenCalled();
  });
});

describe("InvoiceIssueModal — unsaved dismiss protection", () => {
  test("cancel preserves edits until an explicit discard, and nested Escape keeps the form", async () => {
    mockFetch(companyRoute());
    const onClose = vi.fn();
    render(<InvoiceIssueModal slug="acme-aps" onIssued={noop} onClose={onClose} />);
    await fillMinimal();
    const cancel = screen.getByRole("button", { name: "Annullér" });
    await userEvent.click(cancel);
    const confirmation = screen.getByRole("dialog", { name: "Kassér ændringer?" });
    expect(onClose).not.toHaveBeenCalled();
    await userEvent.click(within(confirmation).getByRole("button", { name: "Annullér" }));
    expect(screen.queryByRole("dialog", { name: "Kassér ændringer?" })).not.toBeInTheDocument();
    expect(screen.getByLabelText("Linje 1 beskrivelse")).toHaveValue("Bogføring maj");
    expect(onClose).not.toHaveBeenCalled();

    await userEvent.click(cancel);
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("dialog", { name: "Kassér ændringer?" })).not.toBeInTheDocument();
    expect(screen.getByRole("dialog", { name: "Udsted faktura" })).toBeInTheDocument();
    expect(onClose).not.toHaveBeenCalled();

    await userEvent.click(cancel);
    await userEvent.click(within(screen.getByRole("dialog", { name: "Kassér ændringer?" })).getByRole("button", { name: "Kassér ændringer" }));
    expect(onClose).toHaveBeenCalledTimes(1);
    const calls = (globalThis.fetch as unknown as ReturnType<typeof vi.fn>).mock.calls;
    expect(calls.some((call) => String(call[0]).endsWith("/invoices/issue"))).toBe(false);
  });
  test("Escape asks before discarding an edited invoice", async () => {
    mockFetch(companyRoute());
    const onClose = vi.fn();
    render(<InvoiceIssueModal slug="acme-aps" onIssued={noop} onClose={onClose} />);
    await userEvent.type(screen.getByLabelText("Kunde"), "Ny kunde");
    await userEvent.keyboard("{Escape}");
    expect(screen.getByRole("dialog", { name: "Kassér ændringer?" })).toBeInTheDocument();
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.getByLabelText("Kunde")).toHaveValue("Ny kunde");
  });
  test("a confirmed issued receipt closes without asking to discard", async () => {
    mockFetch(issueRoute());
    const onClose = vi.fn();
    render(<InvoiceIssueModal slug="acme-aps" onIssued={noop} onClose={onClose} />);
    await fillMinimal();
    await userEvent.click(screen.getByRole("button", { name: "Udsted faktura" }));
    await screen.findByText(/udstedt/);
    await userEvent.click(screen.getByRole("button", { name: "Luk" }));
    expect(onClose).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("dialog", { name: "Kassér ændringer?" })).not.toBeInTheDocument();
  });
});

test("an edited invoice is protected when the native dialog backdrop is clicked", async () => {
  mockFetch(companyRoute());
  const onClose = vi.fn();
  render(<InvoiceIssueModal slug="acme-aps" onIssued={noop} onClose={onClose} />);
  await userEvent.type(screen.getByLabelText("Kunde"), "Ny kunde");
  fireEvent.click(screen.getByRole("dialog", { name: "Udsted faktura" }), { clientX: -10, clientY: -10 });
  expect(screen.getByRole("dialog", { name: "Kassér ændringer?" })).toBeInTheDocument();
  expect(onClose).not.toHaveBeenCalled();
  expect(screen.getByLabelText("Kunde")).toHaveValue("Ny kunde");
});
