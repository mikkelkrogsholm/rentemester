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

test("contact EAN and reviewed references use identical preview and issue input",async()=>{
  const auth=await import("../lib/auth-context");vi.spyOn(auth,"useOptionalAuth").mockReturnValue({backend:"rust",loading:false,hosted:false,session:null,currentSessionId:null,context:null,refresh:async()=>{},clear:()=>{}});
  const {api}=await import("../lib/api");const customer={...contacts().customers[0]!,eanNumber:"5790000000074",vatOrCvr:null};mockFetch({...issueRoute(),"GET /api/companies/acme-aps/contacts":{contacts:contacts({customers:[customer]})}});
  const preview=vi.spyOn(api,"previewInvoice").mockResolvedValue(new Blob(["%PDF-synthetic"],{type:"application/pdf"}));const issue=vi.spyOn(api,"issueInvoice").mockResolvedValue({documentId:74,invoiceNumber:"2026-00074",netAmount:2000,vatRate:0.25,vatAmount:500,grossAmount:2500,lines:[]});vi.spyOn(URL,"createObjectURL").mockReturnValue("blob:synthetic-74");vi.spyOn(window,"open").mockReturnValue(null);
  render(<InvoiceIssueModal slug="acme-aps" onIssued={noop} onClose={noop}/>);await fillMinimal();await screen.findByRole("option",{name:new RegExp(customer.name)});await userEvent.selectOptions(screen.getByLabelText("Vælg kunde"),String(customer.id));expect(screen.getByLabelText("EAN-nummer")).toHaveValue("5790000000074");expect(screen.getByLabelText("Offentlig modtager")).toBeChecked();await userEvent.type(screen.getByLabelText("Købers reference"),"Synthetic buyer 74");await userEvent.type(screen.getByLabelText("Ordrenummer"),"Synthetic order 74");await userEvent.click(screen.getByRole("button",{name:"Forhåndsvis"}));await screen.findByRole("button",{name:"Udsted faktura"});await userEvent.click(screen.getByRole("button",{name:"Udsted faktura"}));expect(issue).toHaveBeenCalledTimes(1);expect(preview).toHaveBeenCalledTimes(1);expect(issue.mock.calls[0]![1]).toEqual(preview.mock.calls[0]![1]);expect(issue.mock.calls[0]![1].buyer).toMatchObject({eanNumber:"5790000000074",publicRecipient:true,buyerReference:"Synthetic buyer 74",orderReference:"Synthetic order 74"});
});

async function nativeTaxForm(summary: Record<string, unknown> = {}) {
  const auth = await import("../lib/auth-context");
  vi.spyOn(auth, "useOptionalAuth").mockReturnValue({backend:"rust",loading:false,hosted:false,session:null,currentSessionId:null,context:null,refresh:async()=>{},clear:()=>{}});
  const {api} = await import("../lib/api");
  mockFetch({...companyRoute(),...contactsRoute()});
  const preview=vi.spyOn(api,"previewInvoice").mockResolvedValue(new Blob(["%PDF-synthetic"],{type:"application/pdf"}));
  const issue=vi.spyOn(api,"issueInvoice").mockResolvedValue({documentId:75,invoiceNumber:"2026-00075",netAmount:2000,vatAmount:250,grossAmount:2250,lines:[],...summary});
  vi.spyOn(URL,"createObjectURL").mockReturnValue("blob:synthetic-vat-75");
  vi.spyOn(window,"open").mockReturnValue(null);
  render(<InvoiceIssueModal slug="acme-aps" onIssued={noop} onClose={noop}/>);
  await fillMinimal();
  return {preview,issue};
}

test("native mixed VAT sends reviewed fractional line choices identically to preview and issue",async()=>{
  const {preview,issue}=await nativeTaxForm({vatRate:null,vatTreatment:"standard"});
  await userEvent.click(screen.getByLabelText("Specificér moms pr. linje"));
  await userEvent.click(screen.getByRole("button",{name:"Tilføj linje"}));
  await userEvent.type(screen.getByLabelText("Linje 2 beskrivelse"),"Synthetic exempt service");
  await userEvent.type(screen.getByLabelText("Linje 2 antal"),"1");
  await userEvent.type(screen.getByLabelText("Linje 2 enhedspris"),"1000");
  await userEvent.selectOptions(screen.getByLabelText("Linje 2 momstype"),"exempt");
  await userEvent.click(screen.getByRole("button",{name:"Forhåndsvis"}));
  await userEvent.click(screen.getByRole("button",{name:"Udsted faktura"}));
  expect(issue.mock.calls[0]![1]).toEqual(preview.mock.calls[0]![1]);
  const input=issue.mock.calls[0]![1];
  expect(input).not.toHaveProperty("vatRatePercent");
  expect(input.lines).toEqual([
    {description:"Bogføring maj",quantity:2,unitPriceExVat:1000,taxClassification:"taxable",vatRate:0.25},
    {description:"Synthetic exempt service",quantity:1,unitPriceExVat:1000,taxClassification:"exempt",vatRate:0},
  ]);
  expect(screen.getByText("Moms (pr. linje)")).toBeInTheDocument();
  expect(screen.queryByText(/NaN/)).not.toBeInTheDocument();
});

test("native domestic reverse requires explicit basis/note and receipt never invents omitted VAT",async()=>{
  const basis="DK_MOMSLOVEN_§46_STK_1_NR_6";
  const {preview,issue}=await nativeTaxForm({vatTreatment:"domestic_reverse_charge",vatRate:undefined,vatAmount:undefined,grossAmount:2000,reverseChargeBasis:basis,reverseChargeNote:"Synthetic eligibility reviewed"});
  await userEvent.selectOptions(screen.getByLabelText("Momsbehandling"),"domestic_reverse_charge");
  await userEvent.click(screen.getByRole("button",{name:"Forhåndsvis"}));
  expect(preview).not.toHaveBeenCalled();
  expect(screen.getByText(/Vælg dansk lovgrundlag og angiv en note/)).toBeInTheDocument();
  await userEvent.selectOptions(screen.getByLabelText("Dansk lovgrundlag"),basis);
  await userEvent.type(screen.getByLabelText("Note om omvendt betalingspligt"),"Synthetic eligibility reviewed");
  await userEvent.click(screen.getByRole("button",{name:"Forhåndsvis"}));
  await userEvent.click(screen.getByRole("button",{name:"Udsted faktura"}));
  const input=issue.mock.calls[0]![1];
  expect(input).toEqual(preview.mock.calls[0]![1]);
  expect(input).toMatchObject({vatTreatment:"domestic_reverse_charge",reverseChargeBasis:basis,reverseChargeNote:"Synthetic eligibility reviewed",lines:[{taxClassification:"reverse_charge",vatRate:0,reverseChargeBasis:basis}]});
  expect(input).not.toHaveProperty("vatRatePercent");
  expect(screen.getByText("Køber afregner moms")).toBeInTheDocument();
  expect(screen.getByText("I alt — uden opkrævet moms")).toBeInTheDocument();
  expect(screen.queryByText(/NaN/)).not.toBeInTheDocument();
});

test("native reverse line refuses missing basis before provider-free preview",async()=>{
  const {preview}=await nativeTaxForm();
  await userEvent.click(screen.getByLabelText("Specificér moms pr. linje"));
  await userEvent.selectOptions(screen.getByLabelText("Linje 1 momstype"),"reverse_charge");
  await userEvent.click(screen.getByRole("button",{name:"Forhåndsvis"}));
  expect(preview).not.toHaveBeenCalled();
  expect(screen.getByText(/Linje 1: vælg dansk lovgrundlag/)).toBeInTheDocument();
  await userEvent.selectOptions(screen.getByLabelText("Linje 1 lovgrundlag"),"DK_MOMSLOVEN_§46_STK_1_NR_7");
  await userEvent.click(screen.getByRole("button",{name:"Forhåndsvis"}));
  expect(preview.mock.calls[0]![1].lines[0]).toMatchObject({taxClassification:"reverse_charge",vatRate:0,reverseChargeBasis:"DK_MOMSLOVEN_§46_STK_1_NR_7"});
});

test("native EU service keeps EUR and fixed DKK rate identical in preview and issue, VIES only on request",async()=>{
  const {preview,issue}=await nativeTaxForm({currency:"EUR",vatTreatment:"foreign_reverse_charge",netAmount:2000,vatAmount:0,grossAmount:2000,fxRateToDkk:7.45,netAmountDkk:14900,vatAmountDkk:0,grossAmountDkk:14900});
  const {api}=await import("../lib/api");const vies=vi.spyOn(api,"viesValidate").mockResolvedValue({ok:true,validation:{normalized:"DE123456789",countryCode:"DE",vatNumber:"123456789",valid:true,name:"Synthetic EU customer",address:null,validatedAt:"2026-05-16T10:00:00Z",expiresAt:"2026-08-14T10:00:00Z"}});
  await userEvent.selectOptions(screen.getByLabelText("Momsbehandling"),"foreign_reverse_charge");
  await userEvent.clear(screen.getByLabelText("Valuta"));await userEvent.type(screen.getByLabelText("Valuta"),"EUR");
  await userEvent.click(screen.getByRole("button",{name:"Forhåndsvis"}));expect(preview).not.toHaveBeenCalled();expect(vies).not.toHaveBeenCalled();
  await userEvent.type(screen.getByLabelText("Valutakurs i DKK"),"7,45");
  await userEvent.type(screen.getByLabelText("Kunde CVR/moms"),"DE123456789");
  await userEvent.type(screen.getByLabelText("Note om omvendt betalingspligt"),"Reverse charge — EU B2B services");
  await userEvent.click(screen.getByRole("button",{name:"Verificér EU-momsnummer"}));
  expect(await screen.findByText(/Gyldigt EU-momsnummer/)).toBeInTheDocument();expect(vies).toHaveBeenCalledTimes(1);
  await userEvent.click(screen.getByRole("button",{name:"Forhåndsvis"}));await userEvent.click(screen.getByRole("button",{name:"Udsted faktura"}));
  const payload=issue.mock.calls[0]![1];expect(payload).toEqual(preview.mock.calls[0]![1]);
  expect(payload).toMatchObject({currency:"EUR",fxRateToDkk:7.45,vatTreatment:"foreign_reverse_charge",reverseChargeBasis:"EU_B2B_SERVICES",lines:[{taxClassification:"reverse_charge",vatRate:0,reverseChargeBasis:"EU_B2B_SERVICES"}]});
  expect(payload).not.toHaveProperty("viesValidation");expect(payload).not.toHaveProperty("vatRatePercent");
  expect(screen.getByText(/Bogføring i DKK/)).toBeInTheDocument();
});

test("native full private buyer needs no invented CVR and keeps full form wire",async()=>{
 const {issue}=await nativeTaxForm();
 await userEvent.type(screen.getByLabelText("Kunde"),"Synthetic private buyer");
 await userEvent.type(screen.getByLabelText("Kundeadresse"),"Synthetic street 78");
 await userEvent.click(screen.getByRole("button",{name:"Udsted faktura"}));
 expect(issue).toHaveBeenCalledTimes(1);
 expect(issue.mock.calls[0]![1].buyer).toEqual({name:"Synthetic private buyer",address:"Synthetic street 78",vatOrCvr:undefined});
 expect(issue.mock.calls[0]![1]).not.toHaveProperty("totals");
});

test("native simplified sends only gross and description, shows server-derived cents and retains optional buyer",async()=>{
 const {preview,issue}=await nativeTaxForm({invoiceType:"simplified",currency:"EUR",netAmount:80.06,vatRate:0.25,vatAmount:20.01,grossAmount:100.07,lines:[{description:"Synthetic gross service"}]});
 await userEvent.selectOptions(screen.getByLabelText("Fakturatype"),"simplified");
 expect(screen.queryByLabelText("Linje 1 antal")).not.toBeInTheDocument();
 expect(screen.queryByLabelText("Momssats (%)")).not.toBeInTheDocument();
 expect(screen.queryByLabelText("EAN-nummer")).not.toBeInTheDocument();
 expect(screen.getByText(/Maks. 3.000,00 kr./)).toBeInTheDocument();
 await userEvent.type(screen.getByLabelText("Beskrivelse"),"Synthetic gross service");
 await userEvent.type(screen.getByLabelText("Beløb inkl. moms"),"100,07");
 await userEvent.clear(screen.getByLabelText("Valuta"));await userEvent.type(screen.getByLabelText("Valuta"),"EUR");
 await userEvent.type(screen.getByLabelText("Valutakurs i DKK"),"7,45");
 await userEvent.type(screen.getByLabelText("Kunde (valgfrit)"),"Synthetic private buyer");
 await userEvent.click(screen.getByRole("button",{name:"Forhåndsvis"}));
 await screen.findByText(/Forhåndsvisningen er klar/);
 await userEvent.click(screen.getByRole("button",{name:"Udsted faktura"}));
 const input=issue.mock.calls[0]![1];expect(input).toEqual(preview.mock.calls[0]![1]);
 expect(input).toMatchObject({invoiceType:"simplified",vatTreatment:"standard",currency:"EUR",fxRateToDkk:7.45,lines:[{description:"Synthetic gross service"}],totals:{grossAmount:100.07,vatRate:0.25,vatComputationBasis:"VAT_20_OF_GROSS"},buyer:{name:"Synthetic private buyer"}});
 expect(input).not.toHaveProperty("vatRatePercent");expect(input).not.toHaveProperty("reverseChargeBasis");
 expect(screen.getByText(/80,06\s+EUR/)).toBeInTheDocument();expect(screen.getByText(/20,01\s+EUR/)).toBeInTheDocument();
 expect(screen.getByRole("link",{name:"Hent PDF"})).toBeInTheDocument();
});

test("switching to simplified clears incompatible preview, contact routing, and reverse-charge decisions",async()=>{
 const {preview,issue}=await nativeTaxForm();
 await userEvent.type(screen.getByLabelText("EAN-nummer"),"5790000000074");
 await userEvent.type(screen.getByLabelText("Købers reference"),"Synthetic reference");
 await userEvent.selectOptions(screen.getByLabelText("Momsbehandling"),"domestic_reverse_charge");
 await userEvent.selectOptions(screen.getByLabelText("Dansk lovgrundlag"),"DK_MOMSLOVEN_§46_STK_1_NR_7");
 await userEvent.type(screen.getByLabelText("Note om omvendt betalingspligt"),"Synthetic note");
 await userEvent.click(screen.getByRole("button",{name:"Forhåndsvis"}));await screen.findByText(/Forhåndsvisningen er klar/);
 await userEvent.selectOptions(screen.getByLabelText("Fakturatype"),"simplified");
 expect(screen.queryByText(/Forhåndsvisningen er klar/)).not.toBeInTheDocument();
 await userEvent.type(screen.getByLabelText("Beskrivelse"),"Synthetic reduced sale");await userEvent.type(screen.getByLabelText("Beløb inkl. moms"),"3000");
 await userEvent.click(screen.getByRole("button",{name:"Udsted faktura"}));
 expect(issue).toHaveBeenCalledTimes(1);const input=issue.mock.calls[0]![1];expect(input.buyer).toBeUndefined();expect(input.customerId).toBeUndefined();expect(input).not.toHaveProperty("reverseChargeNote");expect(input.vatTreatment).toBe("standard");expect(input.lines).toEqual([{description:"Synthetic reduced sale"}]);expect(preview).toHaveBeenCalledTimes(1);
});

test("legacy backend never offers the native simplified type",async()=>{
 mockFetch(companyRoute());render(<InvoiceIssueModal slug="acme-aps" onIssued={noop} onClose={noop}/>);
 await screen.findByDisplayValue("DK12345678");expect(screen.queryByLabelText("Fakturatype")).not.toBeInTheDocument();expect(screen.getByLabelText("Linje 1 antal")).toBeInTheDocument();
});

test("delivery period and explicit units use identical preview and issue input; edits invalidate preview", async () => {
  const {preview, issue} = await nativeTaxForm();
  await userEvent.clear(screen.getByLabelText("Kunde CVR/moms (valgfrit)"));
  await userEvent.type(screen.getByLabelText("Kunde CVR/moms (valgfrit)"), "DE123456789");
  await userEvent.selectOptions(screen.getByLabelText("Levering"), "period");
  fireEvent.change(screen.getByLabelText("Leveringsperiode fra"), {target:{value:"2026-10-01"}});
  fireEvent.change(screen.getByLabelText("Leveringsperiode til"), {target:{value:"2026-10-20"}});
  await userEvent.type(screen.getByLabelText("Enhedskode (valgfri)", {exact:true}), "HUR");
  await userEvent.type(screen.getByLabelText("Linje 1 enhedskode (valgfri)"), "H87");
  await userEvent.click(screen.getByRole("button", {name:"Forhåndsvis"}));
  await screen.findByRole("link", {name:"Åbn forhåndsvisning"});
  fireEvent.change(screen.getByLabelText("Leveringsperiode til"), {target:{value:"2026-10-21"}});
  expect(screen.queryByRole("link", {name:"Åbn forhåndsvisning"})).not.toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", {name:"Forhåndsvis"}));
  await screen.findByRole("link", {name:"Åbn forhåndsvisning"});
  await userEvent.click(screen.getByRole("button", {name:"Udsted faktura"}));
  const input = issue.mock.calls[0]![1];
  expect(input).toEqual(preview.mock.calls[1]![1]);
  expect(input).toMatchObject({deliveryPeriodStart:"2026-10-01", deliveryPeriodEnd:"2026-10-21", unitCode:"HUR", buyer:{vatOrCvr:"DE123456789"}, lines:[{unitCode:"H87"}], vatRatePercent:25});
  expect(input).not.toHaveProperty("deliveryDate");
  expect(input).not.toHaveProperty("vatTreatment");
});

test("optional metadata is absent by default and clearing delivery omits prior dates", async () => {
  const {preview} = await nativeTaxForm();
  await userEvent.click(screen.getByRole("button", {name:"Forhåndsvis"}));
  await screen.findByRole("link", {name:"Åbn forhåndsvisning"});
  const initial = preview.mock.calls[0]![1];
  for (const key of ["deliveryDate", "deliveryPeriodStart", "deliveryPeriodEnd", "unitCode"]) expect(initial).not.toHaveProperty(key);
  expect(initial.lines[0]).not.toHaveProperty("unitCode");
  await userEvent.selectOptions(screen.getByLabelText("Levering"), "date");
  fireEvent.change(screen.getByLabelText("Leveringsdato"), {target:{value:"2026-10-20"}});
  await userEvent.selectOptions(screen.getByLabelText("Levering"), "none");
  await userEvent.click(screen.getByRole("button", {name:"Forhåndsvis"}));
  await screen.findByRole("link", {name:"Åbn forhåndsvisning"});
  expect(preview.mock.calls[1]![1]).toEqual(initial);
});

test("simplified validates selected delivery and keeps reduced lines with only explicit document unit", async () => {
  const {preview, issue} = await nativeTaxForm();
  await userEvent.selectOptions(screen.getByLabelText("Fakturatype"), "simplified");
  await userEvent.type(screen.getByLabelText("Beskrivelse"), "Synthetic reduced service");
  await userEvent.type(screen.getByLabelText("Beløb inkl. moms"), "100,07");
  await userEvent.selectOptions(screen.getByLabelText("Levering"), "period");
  fireEvent.change(screen.getByLabelText("Leveringsperiode fra"), {target:{value:"2026-10-21"}});
  await userEvent.click(screen.getByRole("button", {name:"Forhåndsvis"}));
  expect(preview).not.toHaveBeenCalled();
  fireEvent.change(screen.getByLabelText("Leveringsperiode til"), {target:{value:"2026-10-20"}});
  await userEvent.click(screen.getByRole("button", {name:"Forhåndsvis"}));
  expect(preview).not.toHaveBeenCalled();
  await userEvent.selectOptions(screen.getByLabelText("Levering"), "date");
  await userEvent.click(screen.getByRole("button", {name:"Forhåndsvis"}));
  expect(preview).not.toHaveBeenCalled();
  fireEvent.change(screen.getByLabelText("Leveringsdato"), {target:{value:"2026-10-20"}});
  await userEvent.type(screen.getByLabelText("Enhedskode (valgfri)"), "HUR");
  expect(screen.queryByLabelText("Linje 1 enhedskode (valgfri)")).not.toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", {name:"Forhåndsvis"}));
  await screen.findByRole("link", {name:"Åbn forhåndsvisning"});
  await userEvent.click(screen.getByRole("button", {name:"Udsted faktura"}));
  const input = issue.mock.calls[0]![1];
  expect(input).toEqual(preview.mock.calls[0]![1]);
  expect(input).toMatchObject({invoiceType:"simplified", deliveryDate:"2026-10-20", unitCode:"HUR", lines:[{description:"Synthetic reduced service"}]});
  expect(input.lines[0]).not.toHaveProperty("quantity");
  expect(input).not.toHaveProperty("deliveryPeriodStart");
});
