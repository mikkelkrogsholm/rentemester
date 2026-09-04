import { describe, expect, test, vi } from "bun:test";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { DocumentIngestModal } from "./DocumentIngestModal";
import { mockFetch } from "../test/fixtures";

function noop() {}

function receiptFile(name = "kvittering.txt") {
  return new File(["Kasseboner\n12,00 DKK\n"], name, { type: "text/plain" });
}

/** Routes the document-ingest POST to a success result. */
function ingestRoute(over: Record<string, unknown> = {}) {
  return {
    "POST /api/companies/acme-aps/documents/ingest": {
      document: { id: 1, documentNo: "DOC-2026-000001", ...over },
    },
  };
}

/** Switches the modal to the cash-register-receipt type (minimal fields). */
async function pickReceiptType() {
  await userEvent.selectOptions(
    screen.getByLabelText("Bilagstype"),
    "cash_register_receipt",
  );
}

describe("DocumentIngestModal", () => {
  test("renders the dialog with a file picker and metadata fields", () => {
    render(
      <DocumentIngestModal slug="acme-aps" onIngested={noop} onClose={noop} />,
    );
    expect(
      screen.getByRole("dialog", { name: "Indlæs bilag" }),
    ).toBeInTheDocument();
    expect(screen.getByLabelText("Bilagsfil")).toBeInTheDocument();
    expect(screen.getByLabelText("Bilagstype")).toBeInTheDocument();
    expect(screen.getByLabelText("Kilde")).toBeInTheDocument();
  });

  test("the Indlæs button is disabled until a file is chosen", async () => {
    render(
      <DocumentIngestModal slug="acme-aps" onIngested={noop} onClose={noop} />,
    );
    expect(
      screen.getByRole("button", { name: "Indlæs bilag" }),
    ).toBeDisabled();
    await userEvent.upload(screen.getByLabelText("Bilagsfil"), receiptFile());
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "Indlæs bilag" }),
      ).not.toBeDisabled(),
    );
  });

  test("the purchase/sale party fields hide for a cash-register receipt", async () => {
    render(
      <DocumentIngestModal slug="acme-aps" onIngested={noop} onClose={noop} />,
    );
    // køb/salg (the default) shows the sender/recipient fields.
    expect(screen.getByLabelText("Afsender")).toBeInTheDocument();
    await pickReceiptType();
    expect(screen.queryByLabelText("Afsender")).not.toBeInTheDocument();
  });

  test("ingesting POSTs the file as base64 with metadata and confirm:true", async () => {
    mockFetch(ingestRoute());
    const onIngested = vi.fn();
    render(
      <DocumentIngestModal
        slug="acme-aps"
        onIngested={onIngested}
        onClose={noop}
      />,
    );
    await pickReceiptType();
    await userEvent.upload(screen.getByLabelText("Bilagsfil"), receiptFile());
    await userEvent.click(
      screen.getByRole("button", { name: "Indlæs bilag" }),
    );

    await waitFor(() => {
      const calls = (globalThis.fetch as unknown as ReturnType<typeof vi.fn>).mock.calls;
      const ingestCall = calls.find((c) =>
        String(c[0]).includes("/documents/ingest"),
      );
      expect(ingestCall).toBeDefined();
      const init = ingestCall![1] as RequestInit;
      expect(init.method).toBe("POST");
      const sent = JSON.parse(String(init.body));
      expect(sent.fileName).toBe("kvittering.txt");
      expect(typeof sent.fileBase64).toBe("string");
      expect(sent.fileBase64.length).toBeGreaterThan(0);
      expect(sent.metadata.documentType).toBe("cash_register_receipt");
      expect(sent.confirm).toBe(true);
    });
    expect(onIngested).toHaveBeenCalled();
  });

  test("#554 submits a no-VAT internal voucher bound to a bank transaction", async () => {
    mockFetch(ingestRoute());
    render(
      <DocumentIngestModal slug="acme-aps" onIngested={noop} onClose={noop} />,
    );
    await userEvent.selectOptions(screen.getByLabelText("Bilagstype"), "internal_voucher");
    await userEvent.upload(screen.getByLabelText("Bilagsfil"), receiptFile("bankgebyr.txt"));
    await userEvent.type(screen.getByLabelText("Bilagsdato"), "2026-07-31");
    await userEvent.type(screen.getByLabelText("Beløb"), "417");
    await userEvent.type(screen.getByLabelText("Beskrivelse"), "Bankgebyr");
    await userEvent.type(screen.getByLabelText("Banktransaktions-id"), "9");
    await userEvent.type(
      screen.getByLabelText("Regnskabsmæssig begrundelse"),
      "Bankgebyr ifølge importeret kontoudtog; ingen moms.",
    );
    await userEvent.click(screen.getByRole("button", { name: "Indlæs bilag" }));

    await waitFor(() => {
      const calls = (globalThis.fetch as unknown as ReturnType<typeof vi.fn>).mock.calls;
      const ingestCall = calls.find((call) => String(call[0]).includes("/documents/ingest"));
      expect(ingestCall).toBeDefined();
      const sent = JSON.parse(String((ingestCall![1] as RequestInit).body));
      expect(sent.metadata).toMatchObject({
        documentType: "internal_voucher",
        issueDate: "2026-07-31",
        deliveryDescription: "Bankgebyr",
        amountIncVat: 417,
        vatAmount: 0,
        sourceBankTransactionId: 9,
        accountingRationale: "Bankgebyr ifølge importeret kontoudtog; ingen moms.",
      });
    });
  });

  test("#530 sends a mixed taxable and exempt purchase split", async () => {
    mockFetch(ingestRoute());
    render(
      <DocumentIngestModal slug="acme-aps" onIngested={noop} onClose={noop} />,
    );
    await userEvent.upload(screen.getByLabelText("Bilagsfil"), receiptFile("advokat.txt"));
    await userEvent.type(screen.getByLabelText("Beløb inkl. moms"), "1888.75");
    await userEvent.type(screen.getByLabelText("Momsbeløb"), "243.75");

    await userEvent.click(screen.getByRole("button", { name: "Tilføj momslinje" }));
    await userEvent.type(screen.getByLabelText("Nettobeløb 1"), "975");
    await userEvent.type(screen.getByLabelText("Momsbeløb 1"), "243.75");
    await userEvent.click(screen.getByRole("button", { name: "Tilføj momslinje" }));
    await userEvent.selectOptions(screen.getByLabelText("Momsart 2"), "exempt");
    await userEvent.type(screen.getByLabelText("Nettobeløb 2"), "670");
    await userEvent.type(screen.getByLabelText("Momsbeløb 2"), "0");

    await userEvent.click(screen.getByRole("button", { name: "Indlæs bilag" }));
    await waitFor(() => {
      const calls = (globalThis.fetch as unknown as ReturnType<typeof vi.fn>).mock.calls;
      const ingestCall = calls.find((call) => String(call[0]).includes("/documents/ingest"));
      expect(ingestCall).toBeDefined();
      const sent = JSON.parse(String((ingestCall![1] as RequestInit).body));
      expect(sent.metadata.purchaseVatLines).toEqual([
        { classification: "dk_purchase_25", netAmount: 975, vatAmount: 243.75 },
        { classification: "exempt", netAmount: 670, vatAmount: 0 },
      ]);
    });
  });

  test("#619 submits source-cited reverse-charge wording evidence for a non-EU supplier", async () => {
    mockFetch(ingestRoute());
    render(
      <DocumentIngestModal slug="acme-aps" onIngested={noop} onClose={noop} />,
    );
    await userEvent.upload(screen.getByLabelText("Bilagsfil"), receiptFile("us-saas.txt"));
    await userEvent.selectOptions(screen.getByLabelText("Identitetstype"), "non_eu");
    await userEvent.type(screen.getByLabelText("Ordlyd om omvendt betalingspligt (valgfri)"), "Reverse charge applies");
    await userEvent.type(screen.getByLabelText("Placering på bilaget"), "side 1");
    await userEvent.click(screen.getByRole("button", { name: "Indlæs bilag" }));
    await waitFor(() => {
      const calls = (globalThis.fetch as unknown as ReturnType<typeof vi.fn>).mock.calls;
      const ingestCall = calls.find((call) => String(call[0]).includes("/documents/ingest"));
      expect(ingestCall).toBeDefined();
      const sent = JSON.parse(String((ingestCall![1] as RequestInit).body));
      expect(sent.metadata.reverseChargeWordingEvidence).toEqual({ excerpt: "Reverse charge applies", location: "side 1" });
    });
  });

  test("#621 submits a source-linked external payroll report without VAT", async () => {
    mockFetch(ingestRoute());
    render(<DocumentIngestModal slug="acme-aps" onIngested={noop} onClose={noop} />);
    await userEvent.selectOptions(screen.getByLabelText("Bilagstype"), "external_accounting_evidence");
    await userEvent.upload(screen.getByLabelText("Bilagsfil"), receiptFile("payroll.txt"));
    await userEvent.type(screen.getByLabelText("Bilagsdato"), "2026-08-31");
    await userEvent.type(screen.getByLabelText("Samlet lønrapport (debet/kredit)"), "40200");
    await userEvent.type(screen.getByLabelText("Lønperiode"), "2026-08");
    await userEvent.type(screen.getByLabelText("Ekstern lønreference"), "PAY-2026-08");
    await userEvent.type(screen.getByLabelText("Afsender"), "Synthetic Payroll Provider");
    await userEvent.type(screen.getByLabelText("Modtager"), "Synthetic Company");
    await userEvent.click(screen.getByRole("button", { name: "Indlæs bilag" }));
    await waitFor(() => {
      const calls = (globalThis.fetch as unknown as ReturnType<typeof vi.fn>).mock.calls;
      const ingestCall = calls.find((call) => String(call[0]).includes("/documents/ingest"));
      const sent = JSON.parse(String((ingestCall![1] as RequestInit).body));
      expect(sent.metadata).toMatchObject({ documentType: "external_accounting_evidence", vatAmount: 0, externalAccountingEvidence: { category: "payroll", accountingPeriod: "2026-08", externalReference: "PAY-2026-08", totals: { debitAmount: 40200, creditAmount: 40200 } } });
    });
  });

  test("shows a receipt with the document number after success", async () => {
    mockFetch(ingestRoute());
    render(
      <DocumentIngestModal slug="acme-aps" onIngested={noop} onClose={noop} />,
    );
    await pickReceiptType();
    await userEvent.upload(screen.getByLabelText("Bilagsfil"), receiptFile());
    await userEvent.click(
      screen.getByRole("button", { name: "Indlæs bilag" }),
    );
    expect(
      await screen.findByText(/DOC-2026-000001/),
    ).toBeInTheDocument();
  });

  test("a 409 backup-lock conflict is shown as a kind lock banner", async () => {
    mockFetch({
      "POST /api/companies/acme-aps/documents/ingest": {
        __error: {
          code: "conflict",
          message: "Bogføring er låst: en ugentlig backup er overskredet.",
        },
      },
    });
    const onClose = vi.fn();
    render(
      <DocumentIngestModal
        slug="acme-aps"
        onIngested={noop}
        onClose={onClose}
      />,
    );
    await pickReceiptType();
    await userEvent.upload(screen.getByLabelText("Bilagsfil"), receiptFile());
    await userEvent.click(
      screen.getByRole("button", { name: "Indlæs bilag" }),
    );
    expect(
      await screen.findByText("Bogføringen er låst"),
    ).toBeInTheDocument();
    expect(onClose).not.toHaveBeenCalled();
  });

  test("a validation error from the server is shown as an error banner", async () => {
    mockFetch({
      "POST /api/companies/acme-aps/documents/ingest": {
        __error: {
          code: "bad_request",
          message: "deliveryDescription is required",
        },
      },
    });
    render(
      <DocumentIngestModal slug="acme-aps" onIngested={noop} onClose={noop} />,
    );
    await userEvent.upload(screen.getByLabelText("Bilagsfil"), receiptFile());
    await userEvent.click(
      screen.getByRole("button", { name: "Indlæs bilag" }),
    );
    expect(
      await screen.findByText("deliveryDescription is required"),
    ).toBeInTheDocument();
  });

  test("Annullér closes the modal without ingesting", async () => {
    const onClose = vi.fn();
    render(
      <DocumentIngestModal
        slug="acme-aps"
        onIngested={noop}
        onClose={onClose}
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: "Annullér" }));
    expect(onClose).toHaveBeenCalled();
  });
});
