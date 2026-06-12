import { describe, expect, test, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
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

  test("importing POSTs the CSV content and reloads the view", async () => {
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

    // The import endpoint was called with the CSV text and confirm:true.
    await waitFor(() => {
      const calls = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls;
      const importCall = calls.find((c) =>
        String(c[0]).includes("/bank/import"),
      );
      expect(importCall).toBeDefined();
      const init = importCall![1] as RequestInit;
      expect(init.method).toBe("POST");
      const sent = JSON.parse(String(init.body));
      expect(sent.csvContent).toContain("Card payment");
      expect(sent.confirm).toBe(true);
    });
    expect(onImported).toHaveBeenCalled();
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

  test("lists supported bank profiles before upload (#422)", () => {
    render(
      <BankImportModal slug="acme-aps" onImported={noop} onClose={noop} />,
    );
    // The owner can see what is supported before guessing a profile name.
    expect(
      screen.getByText(/Understøttede bankprofiler/i),
    ).toBeInTheDocument();
    // The Danske Bank profile is registered in the core, so it must show.
    expect(screen.getByText(/danske-bank/)).toBeInTheDocument();
  });

  test("shows hints for the Bankkonto and Bankformat fields (#422)", () => {
    render(
      <BankImportModal slug="acme-aps" onImported={noop} onClose={noop} />,
    );
    // Bankkonto hint avoids the "slug" jargon and explains when it matters.
    expect(
      screen.getByText(/flere bankkonti/i),
    ).toBeInTheDocument();
    // Bankformat hint explains auto-detection when the field is empty.
    expect(
      screen.getByText(/automatisk at genkende formatet/i),
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
