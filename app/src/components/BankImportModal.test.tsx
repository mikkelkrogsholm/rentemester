import { describe, expect, test, vi } from "bun:test";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { BankImportModal } from "./BankImportModal";
import { mockFetch } from "../test/fixtures";

function noop() {}

const CSV = [
  "transaction_date,booking_date,text,amount,currency,reference",
  "2026-05-16,2026-05-17,Card payment,-1250,DKK,REF-1",
].join("\n");

function csvFile(name = "kontoudtog.csv") {
  return new File([CSV], name, { type: "text/csv" });
}

/** Routes the bank-import POST to a success summary. */
function importRoute(over: Record<string, unknown> = {}) {
  return {
    "POST /api/companies/acme-aps/bank/import": {
      import: {
        importBatchId: "BANK-1",
        imported: 1,
        skippedDuplicates: 0,
        skippedDuplicateRows: [],
        balanceWarnings: [],
        exceptionsCreated: 1,
        ...over,
      },
    },
  };
}

describe("BankImportModal", () => {
  test("renders the dialog with a file picker", () => {
    render(
      <BankImportModal slug="acme-aps" onImported={noop} onClose={noop} />,
    );
    expect(
      screen.getByRole("dialog", { name: "Importér kontoudtog" }),
    ).toBeInTheDocument();
    expect(screen.getByLabelText("CSV-fil")).toBeInTheDocument();
  });

  test("the Importér button is disabled until a file is chosen", async () => {
    render(
      <BankImportModal slug="acme-aps" onImported={noop} onClose={noop} />,
    );
    expect(screen.getByRole("button", { name: "Importér" })).toBeDisabled();
    await userEvent.upload(screen.getByLabelText("CSV-fil"), csvFile());
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "Importér" }),
      ).not.toBeDisabled(),
    );
  });

  test("importing POSTs the original CSV bytes and reloads the view", async () => {
    mockFetch(importRoute());
    const onImported = vi.fn();
    render(
      <BankImportModal
        slug="acme-aps"
        onImported={onImported}
        onClose={noop}
      />,
    );
    await userEvent.upload(screen.getByLabelText("CSV-fil"), csvFile());
    await userEvent.click(screen.getByRole("button", { name: "Importér" }));

    // Original bytes reach the server without text decoding.
    await waitFor(() => {
      const calls = (globalThis.fetch as unknown as ReturnType<typeof vi.fn>).mock.calls;
      const importCall = calls.find((c) =>
        String(c[0]).includes("/bank/import"),
      );
      expect(importCall).toBeDefined();
      const init = importCall![1] as RequestInit;
      expect(init.method).toBe("POST");
      const sent = JSON.parse(String(init.body));
      expect(sent.csvBase64).toBe(btoa(CSV));
      expect(sent).not.toHaveProperty("csvContent");
      expect(sent.confirm).toBe(true);
      expect(sent.profile).toBeUndefined();
    });
    expect(onImported).toHaveBeenCalled();
  });

  test("sends explicitly selected generic statement order", async () => {
    mockFetch(importRoute());
    render(<BankImportModal slug="acme-aps" onImported={noop} onClose={noop} />);
    await userEvent.upload(screen.getByLabelText("CSV-fil"), csvFile());
    await userEvent.selectOptions(screen.getByLabelText("Rækkefølge i kontoudtoget (valgfri)"), "descending");
    await userEvent.click(screen.getByRole("button", { name: "Importér" }));
    await waitFor(() => {
      const call = (globalThis.fetch as unknown as ReturnType<typeof vi.fn>).mock.calls.find((c) => String(c[0]).includes("/bank/import"));
      expect(JSON.parse(String((call![1] as RequestInit).body)).statementOrder).toBe("descending");
    });
  });

  test("shows a receipt with the imported count after success", async () => {
    mockFetch(importRoute({ imported: 3, exceptionsCreated: 2 }));
    render(
      <BankImportModal slug="acme-aps" onImported={noop} onClose={noop} />,
    );
    await userEvent.upload(screen.getByLabelText("CSV-fil"), csvFile());
    await userEvent.click(screen.getByRole("button", { name: "Importér" }));
    expect(
      await screen.findByText(/3 transaktioner importeret/),
    ).toBeInTheDocument();
  });

  test("a 409 backup-lock conflict is shown as a kind lock banner", async () => {
    mockFetch({
      "POST /api/companies/acme-aps/bank/import": {
        __error: {
          code: "conflict",
          message: "Bogføring er låst: en ugentlig backup er overskredet.",
        },
      },
    });
    const onClose = vi.fn();
    render(
      <BankImportModal
        slug="acme-aps"
        onImported={noop}
        onClose={onClose}
      />,
    );
    await userEvent.upload(screen.getByLabelText("CSV-fil"), csvFile());
    await userEvent.click(screen.getByRole("button", { name: "Importér" }));
    expect(
      await screen.findByText("Bogføringen er låst"),
    ).toBeInTheDocument();
    // The modal stays open so the operator can read the lock message.
    expect(onClose).not.toHaveBeenCalled();
  });

  test("a non-conflict error is shown as an error banner", async () => {
    mockFetch({
      "POST /api/companies/acme-aps/bank/import": {
        __error: { code: "bad_request", message: "Ugyldig CSV." },
      },
    });
    render(
      <BankImportModal slug="acme-aps" onImported={noop} onClose={noop} />,
    );
    await userEvent.upload(screen.getByLabelText("CSV-fil"), csvFile());
    await userEvent.click(screen.getByRole("button", { name: "Importér" }));
    expect(await screen.findByText("Ugyldig CSV.")).toBeInTheDocument();
  });

  test("sends the chosen account and Danske profile without interpreting CSV dates", async () => {
    mockFetch(importRoute());
    render(<BankImportModal slug="acme-aps" onImported={noop} onClose={noop} />);
    const csv = "Dato;Tekst;Beløb;Valuta\n01-02-2026;Synthetic;-12,34;DKK";
    await userEvent.upload(screen.getByLabelText("CSV-fil"), new File([csv], "danske.csv", { type: "text/csv" }));
    await userEvent.type(screen.getByLabelText(/^Bankkonto \(valgfri\)/), "synthetic-account");
    await userEvent.type(screen.getByLabelText(/^Bankformat \(valgfri\)/), "danske-bank");
    await userEvent.click(screen.getByRole("button", { name: "Importér" }));
    await waitFor(() => {
      const calls = (globalThis.fetch as unknown as ReturnType<typeof vi.fn>).mock.calls;
      const call = calls.find(c => String(c[0]).includes("/bank/import"));
      expect(call).toBeDefined();
      expect(JSON.parse(String((call![1] as RequestInit).body))).toEqual({ csvBase64: btoa(String.fromCharCode(...new TextEncoder().encode(csv))), account: "synthetic-account", profile: "danske-bank", confirm: true });
    });
  });

  test("lists supported bank profiles before upload (#422)", () => {
    render(
      <BankImportModal slug="acme-aps" onImported={noop} onClose={noop} />,
    );
    // The owner can see what is supported before guessing a profile name.
    expect(
      screen.getByText(/Understøttede bankprofiler/i),
    ).toBeInTheDocument();
    // The Danske Bank profile is registered in the core, so it must show.
    expect(screen.getByText("danske-bank", { selector: "code" })).toBeInTheDocument();
  });

  test("shows hints for the Bankkonto and Bankformat fields (#422)", () => {
    render(
      <BankImportModal slug="acme-aps" onImported={noop} onClose={noop} />,
    );
    // Bankkonto hint avoids the "slug" jargon and explains when it matters.
    expect(
      screen.getByText(/flere bankkonti/i),
    ).toBeInTheDocument();
    // Generic dates remain explicit rather than promising bank-format guessing.
    expect(
      screen.getByText(/tvetydige datoer afvises/i),
    ).toBeInTheDocument();
  });

  test("links to the bank-import documentation page (#422)", () => {
    render(
      <BankImportModal slug="acme-aps" onImported={noop} onClose={noop} />,
    );
    const link = screen.getByRole("link", { name: /CSV-format|guide|læs mere/i });
    expect(link).toBeInTheDocument();
    expect(link.getAttribute("href")).toBeTruthy();
  });

  test("Annullér closes the modal without importing", async () => {
    const onClose = vi.fn();
    render(
      <BankImportModal
        slug="acme-aps"
        onImported={noop}
        onClose={onClose}
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: "Annullér" }));
    expect(onClose).toHaveBeenCalled();
  });
});

test("Windows-1252 bytes reach import unchanged without text decoding", async () => {
  mockFetch(importRoute());
  render(<BankImportModal slug="acme-aps" onImported={noop} onClose={noop}/>);
  const bytes = new Uint8Array([0x44,0x61,0x74,0x6f,0x3b,0xe6,0xf8,0xe5,0x80,0x0d,0x0a]);
  const file = new File([bytes], "original-1252.csv", {type:"text/csv"});
  const text = vi.spyOn(file, "text").mockRejectedValue(new Error("must not decode"));
  await userEvent.upload(screen.getByLabelText("CSV-fil"), file);
  await screen.findByText("Valgt: original-1252.csv");
  await userEvent.type(screen.getByLabelText(/^Bankformat \(valgfri\)/), "danske-bank");
  await userEvent.click(screen.getByRole("button", {name:"Importér"}));
  await screen.findByText(/1 transaktion importeret/);
  const call = (globalThis.fetch as unknown as ReturnType<typeof vi.fn>).mock.calls.find(c=>String(c[0]).includes("/bank/import"));
  expect(JSON.parse(String((call![1] as RequestInit).body))).toEqual({csvBase64:btoa(String.fromCharCode(...bytes)),profile:"danske-bank",confirm:true});
  expect(text).not.toHaveBeenCalled();
});

test("cancel selection prevents an old pending read from becoming importable", async () => {
  render(<BankImportModal slug="acme-aps" onImported={noop} onClose={noop}/>);
  const file = csvFile("pending.csv");
  let complete!: (value:ArrayBuffer) => void;
  vi.spyOn(file,"arrayBuffer").mockImplementation(()=>new Promise(resolve=>{complete=resolve;}));
  const picker = screen.getByLabelText("CSV-fil");
  fireEvent.change(picker,{target:{files:[file]}});
  expect(screen.getByRole("button",{name:"Importér"})).toBeDisabled();
  fireEvent.change(picker,{target:{files:[]}});
  complete(new TextEncoder().encode(CSV).buffer);
  await new Promise(resolve=>setTimeout(resolve,0));
  expect(screen.queryByText("Valgt: pending.csv")).not.toBeInTheDocument();
  expect(screen.getByRole("button",{name:"Importér"})).toBeDisabled();
});

test("failed replacement clears selected file and ignores an older pending read", async () => {
  render(<BankImportModal slug="acme-aps" onImported={noop} onClose={noop}/>);
  const picker = screen.getByLabelText("CSV-fil");
  await userEvent.upload(picker,csvFile("first.csv"));
  await screen.findByText("Valgt: first.csv");
  const pending=csvFile("old-pending.csv");
  let complete!: (value:ArrayBuffer) => void;
  vi.spyOn(pending,"arrayBuffer").mockImplementation(()=>new Promise(resolve=>{complete=resolve;}));
  fireEvent.change(picker,{target:{files:[pending]}});
  const failed=csvFile("failed.csv");
  vi.spyOn(failed,"arrayBuffer").mockRejectedValue(new Error("Filen kunne ikke læses."));
  fireEvent.change(picker,{target:{files:[failed]}});
  await screen.findByText("Filen kunne ikke læses.");
  complete(new TextEncoder().encode(CSV).buffer);
  await new Promise(resolve=>setTimeout(resolve,0));
  expect(screen.queryByText(/Valgt:/)).not.toBeInTheDocument();
  expect(screen.getByRole("button",{name:"Importér"})).toBeDisabled();
});

test("oversized files refuse before reading and empty files cannot import", async () => {
  render(<BankImportModal slug="acme-aps" onImported={noop} onClose={noop}/>);
  const oversized=csvFile();
  Object.defineProperty(oversized,"size",{value:25*1024*1024+1});
  const read=vi.spyOn(oversized,"arrayBuffer");
  fireEvent.change(screen.getByLabelText("CSV-fil"),{target:{files:[oversized]}});
  await screen.findByText("CSV-filen må højst fylde 25 MiB.");
  expect(read).not.toHaveBeenCalled();
  fireEvent.change(screen.getByLabelText("CSV-fil"),{target:{files:[new File([],"empty.csv",{type:"text/csv"})]}});
  await screen.findByText("CSV-filen er tom.");
  expect(screen.getByRole("button",{name:"Importér"})).toBeDisabled();
});
