import { describe, expect, test, vi } from "bun:test";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { DocumentIngestModal } from "./DocumentIngestModal";
import { mockFetch } from "../test/fixtures";

function noop() {}

function receiptFile(name = "kvittering.txt") {
  return new File(["Kasseboner\n12,00 DKK\n"], name, { type: "text/plain" });
}

function pendingOriginal(name: string, type = "text/plain") {
  let resolve!: (buffer: ArrayBuffer) => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<ArrayBuffer>((yes, no) => { resolve = yes; reject = no; });
  const file = new File(["not read yet"], name, { type });
  Object.defineProperty(file, "arrayBuffer", { value: () => promise });
  return {
    file,
    finish: (text: string) => act(async () => { resolve(new TextEncoder().encode(text).buffer as ArrayBuffer); }),
    fail: () => act(async () => { reject(new Error("Synthetic unreadable original")); }),
  };
}

async function originalPicker() {
  mockFetch({});
  const submit = vi.fn(async (_input: unknown, _mime: string) => ({ documentNo: "DOC-SELECTED" }));
  render(<DocumentIngestModal slug="acme-aps" onClose={noop} onIngested={noop} submit={submit} />);
  await pickReceiptType();
  return { submit, input: screen.getByLabelText("Bilagsfil"), button: screen.getByRole("button", { name: "Indlæs bilag" }) };
}

function expectSelectedOriginal(submit: ReturnType<typeof vi.fn>, name: string, bytes: string, mime: string) {
  expect(submit).toHaveBeenCalledTimes(1);
  const [input, sentMime] = submit.mock.calls[0] as unknown as [{ fileName: string; fileBase64: string }, string];
  expect(input.fileName).toBe(name);
  expect(atob(input.fileBase64)).toBe(bytes);
  expect(sentMime).toBe(mime);
}

describe("selected original", () => {
  test("replacement reading blocks submission of an already loaded original", async () => {
    const { submit, input, button } = await originalPicker();
    await userEvent.upload(input, receiptFile("old.txt"));
    await waitFor(() => expect(button).not.toBeDisabled());
    const replacement = pendingOriginal("current.pdf", "application/pdf");
    await userEvent.upload(input, replacement.file);
    expect(button).toBeDisabled();
    fireEvent.click(button);
    expect(submit).not.toHaveBeenCalled();
    await replacement.finish("%PDF-current original");
    await userEvent.click(button);
    await screen.findByText("Bilaget er indlæst som DOC-SELECTED.");
    expectSelectedOriginal(submit, "current.pdf", "%PDF-current original", "application/pdf");
  });

  test("a superseded late success cannot replace the current original", async () => {
    const { submit, input, button } = await originalPicker();
    const old = pendingOriginal("old.txt");
    await userEvent.upload(input, old.file);
    await userEvent.upload(input, receiptFile("current.txt"));
    await waitFor(() => expect(button).not.toBeDisabled());
    await old.finish("obsolete bytes");
    expect(screen.getByText("Valgt: current.txt")).toBeInTheDocument();
    await userEvent.click(button);
    await screen.findByText("Bilaget er indlæst som DOC-SELECTED.");
    expectSelectedOriginal(submit, "current.txt", "Kasseboner\n12,00 DKK\n", "text/plain");
  });

  test("a superseded late failure cannot clear the current original", async () => {
    const { submit, input, button } = await originalPicker();
    const old = pendingOriginal("old.txt");
    await userEvent.upload(input, old.file);
    await userEvent.upload(input, receiptFile("current.txt"));
    await waitFor(() => expect(button).not.toBeDisabled());
    await old.fail();
    expect(screen.queryByText("Filen kunne ikke læses.")).toBeNull();
    expect(button).not.toBeDisabled();
    await userEvent.click(button);
    await screen.findByText("Bilaget er indlæst som DOC-SELECTED.");
    expectSelectedOriginal(submit, "current.txt", "Kasseboner\n12,00 DKK\n", "text/plain");
  });

  test("current read failure clears old bytes and a fresh choice recovers once", async () => {
    const { submit, input, button } = await originalPicker();
    await userEvent.upload(input, receiptFile("old.txt"));
    await waitFor(() => expect(button).not.toBeDisabled());
    const bad = pendingOriginal("unreadable.txt");
    await userEvent.upload(input, bad.file);
    await bad.fail();
    expect(screen.getByText("Filen kunne ikke læses.")).toBeInTheDocument();
    expect(button).toBeDisabled();
    expect(submit).not.toHaveBeenCalled();
    await userEvent.upload(input, receiptFile("recovered.txt"));
    await waitFor(() => expect(button).not.toBeDisabled());
    expect(screen.queryByText("Filen kunne ikke læses.")).toBeNull();
    await userEvent.click(button);
    await screen.findByText("Bilaget er indlæst som DOC-SELECTED.");
    expectSelectedOriginal(submit, "recovered.txt", "Kasseboner\n12,00 DKK\n", "text/plain");
  });

  test("clearing selection prevents a late read from becoming submittable", async () => {
    const { submit, input, button } = await originalPicker();
    const old = pendingOriginal("old.txt");
    await userEvent.upload(input, old.file);
    await userEvent.upload(input, []);
    await old.finish("cleared original");
    expect(screen.queryByText("Valgt: old.txt")).toBeNull();
    expect(button).toBeDisabled();
    fireEvent.click(button);
    expect(submit).not.toHaveBeenCalled();
  });
});

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
  test("renders the dialog with a file picker and metadata fields", async () => {
    render(
      <DocumentIngestModal slug="acme-aps" onIngested={noop} onClose={noop} />,
    );
    expect(
      screen.getByRole("dialog", { name: "Indlæs bilag" }),
    ).toBeInTheDocument();
    expect(screen.getByLabelText("Bilagsfil")).toBeInTheDocument();
    expect(screen.getByLabelText("Bilagstype")).toBeInTheDocument();
    expect(screen.getByLabelText("Kilde")).toBeInTheDocument();
    await waitFor(() => expect(screen.queryByText("Henter leverandører…")).not.toBeInTheDocument());
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

  test("#529 submits explicit reverse-charge wording evidence for a non-EU supplier", async () => {
    mockFetch(ingestRoute());
    render(
      <DocumentIngestModal slug="acme-aps" onIngested={noop} onClose={noop} />,
    );
    await userEvent.upload(screen.getByLabelText("Bilagsfil"), receiptFile("us-saas.txt"));
    await userEvent.selectOptions(screen.getByLabelText("Identitetstype"), "non_eu");
    const evidence = screen.getByLabelText("Jeg har kontrolleret, at bilaget indeholder ordlyd om omvendt betalingspligt");
    await userEvent.click(evidence);
    await userEvent.click(screen.getByRole("button", { name: "Indlæs bilag" }));
    await waitFor(() => {
      const calls = (globalThis.fetch as unknown as ReturnType<typeof vi.fn>).mock.calls;
      const ingestCall = calls.find((call) => String(call[0]).includes("/documents/ingest"));
      expect(ingestCall).toBeDefined();
      const sent = JSON.parse(String((ingestCall![1] as RequestInit).body));
      expect(sent.metadata.reverseChargeWordingConfirmed).toBe(true);
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

test("copies a selected supplier into the editable voucher snapshot", async () => {
  mockFetch({"GET /api/companies/acme-aps/contacts": {contacts:{vendors:[{id:7,name:"Synthetic Supplier",address:"Synthetic Street 1",vatOrCvr:"DK12345678",countryCode:"DK",identifierKind:"dk_cvr"}]}}});
  render(<DocumentIngestModal slug="acme-aps" onIngested={noop} onClose={noop} />);
  await userEvent.selectOptions(await screen.findByLabelText("Udfyld fra leverandør (valgfri)"),"7");
  expect(screen.getByLabelText("Afsender")).toHaveValue("Synthetic Supplier");
  expect(screen.getByLabelText("Afsenderadresse")).toHaveValue("Synthetic Street 1");
  expect(screen.getByLabelText("Afsender CVR/moms")).toHaveValue("DK12345678");
  await userEvent.clear(screen.getByLabelText("Afsenderadresse"));
  await userEvent.type(screen.getByLabelText("Afsenderadresse"),"Synthetic document override");
  expect(screen.getByLabelText("Afsenderadresse")).toHaveValue("Synthetic document override");
});

test("explicit alternate inbox intake receives user classification and exact original without company ingestion", async () => {
  mockFetch({});
  const submit = vi.fn(async () => ({ documentNo: null }));
  render(<DocumentIngestModal slug="acme-aps" onClose={noop} onIngested={noop} submit={submit} title="Synthetic inbox intake" successText="Source awaiting routing" allowInternalVoucher={false} />);
  expect(screen.queryByRole("option", { name: "Internt bilag" })).toBeNull();
  await pickReceiptType();
  await userEvent.upload(screen.getByLabelText("Bilagsfil"), receiptFile());
  await userEvent.click(screen.getByRole("button", { name: "Indlæs bilag" }));
  await screen.findByText("Source awaiting routing");
  expect(submit).toHaveBeenCalledTimes(1);
  const [input, mime] = submit.mock.calls[0] as unknown as [{ metadata: { documentType: string }; fileName: string; fileBase64: string }, string];
  expect(input.metadata.documentType).toBe("cash_register_receipt");
  expect(input.fileName).toBe("kvittering.txt");
  expect(atob(input.fileBase64)).toBe("Kasseboner\n12,00 DKK\n");
  expect(mime).toBe("text/plain");
  expect((globalThis.fetch as unknown as ReturnType<typeof vi.fn>).mock.calls.some(c => String(c[0]).includes("/documents/ingest"))).toBe(false);
});

test("retained attachment review has no upload or inferred classification/currency and sends metadata once", async () => {
  const saved=vi.fn(async (_metadata:unknown)=>{});
  mockFetch({"GET /api/companies/target/contacts":{contacts:{vendors:[],customers:[]}}});
  render(<DocumentIngestModal slug="target" onClose={noop} onIngested={noop} title="Gennemgå mailbilag" allowInternalVoucher={false} metadataReview={{filename:"invoice.pdf → Target",source:"workspace-inbox:mail-id",sourceHash:"a".repeat(64),originalUrl:"/original",save:saved}} />);
  expect(screen.queryByLabelText("Bilagsfil")).toBeNull();
  expect(screen.getByLabelText("Bilagstype")).toHaveValue("");
  expect(screen.getByLabelText("Valuta")).toHaveValue("");
  expect(screen.getByLabelText("Kilde")).toHaveAttribute("readonly");
  expect(screen.getByRole("button",{name:"Gem gennemgåede kildeoplysninger"})).toBeDisabled();
  await pickReceiptType();
  await userEvent.type(screen.getByLabelText("Valuta"),"DKK");
  await userEvent.type(screen.getByLabelText("Beløb inkl. moms (valgfri)"),"125,00");
  await userEvent.click(screen.getByLabelText("Jeg har gennemgået originalen, målvirksomheden og de indtastede kildeoplysninger."));
  await userEvent.click(screen.getByRole("button",{name:"Gem gennemgåede kildeoplysninger"}));
  await waitFor(()=>expect(saved).toHaveBeenCalledTimes(1));
  expect(saved.mock.calls[0][0]).toEqual({source:"workspace-inbox:mail-id",documentType:"cash_register_receipt",currency:"DKK",amountIncVat:125});
  const calls=(globalThis.fetch as unknown as {mock:{calls:Array<[unknown,RequestInit]>}}).mock.calls;
  expect(calls.some(c=>String(c[0]).includes("/documents/ingest"))).toBe(false);
});
