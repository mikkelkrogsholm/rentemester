import { describe, expect, test, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ImportModal } from "./ImportModal";
import { mockFetch } from "../test/fixtures";

function noop() {}

const CSV =
  "Kontaktnavn;CVR-nummer;Kontakttype\nAcme ApS;12345678;Company";

function csvFile(name = "Kontakter.csv") {
  return new File([CSV], name, { type: "text/csv" });
}

/** Routes the file-import POST to a success summary. */
function importRoute(
  summaryOver: Record<string, unknown> = {},
  errors: string[] = [],
) {
  return {
    "POST /api/companies/acme-aps/import": {
      import: {
        detected: {
          id: "dinero-contacts",
          label: "Dinero — Kontakter (kunder og leverandører)",
          system: "Dinero",
          dataType: "contacts",
        },
        summary: {
          parsed: 2,
          customersCreated: 1,
          vendorsCreated: 1,
          skipped: 0,
          enriched: 0,
          enrichmentFailures: 0,
          ...summaryOver,
        },
        errors,
      },
    },
  };
}

describe("ImportModal", () => {
  test("renders the dialog with a file picker", () => {
    render(<ImportModal slug="acme-aps" onImported={noop} onClose={noop} />);
    expect(
      screen.getByRole("dialog", { name: "Importér fil" }),
    ).toBeInTheDocument();
    expect(screen.getByLabelText("Fil")).toBeInTheDocument();
  });

  test("the Importér button is disabled until a file is chosen", async () => {
    render(<ImportModal slug="acme-aps" onImported={noop} onClose={noop} />);
    expect(screen.getByRole("button", { name: "Importér" })).toBeDisabled();
    await userEvent.upload(screen.getByLabelText("Fil"), csvFile());
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "Importér" }),
      ).not.toBeDisabled(),
    );
  });

  test("importing POSTs the file content and reloads the view", async () => {
    mockFetch(importRoute());
    const onImported = vi.fn();
    render(
      <ImportModal slug="acme-aps" onImported={onImported} onClose={noop} />,
    );
    await userEvent.upload(screen.getByLabelText("Fil"), csvFile());
    await userEvent.click(screen.getByRole("button", { name: "Importér" }));

    await waitFor(() => {
      const calls = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls;
      const importCall = calls.find(
        (c) =>
          String(c[0]).endsWith("/import") &&
          (c[1] as RequestInit | undefined)?.method === "POST",
      );
      expect(importCall).toBeDefined();
      const sent = JSON.parse(String((importCall![1] as RequestInit).body));
      expect(sent.fileName).toBe("Kontakter.csv");
      expect(sent.content).toContain("Kontaktnavn");
      expect(sent.enrichCvr).toBe(true);
      expect(sent.confirm).toBe(true);
    });
    expect(onImported).toHaveBeenCalled();
  });

  test("shows a receipt naming the recognised source and the counts", async () => {
    mockFetch(importRoute({ vendorsCreated: 25, customersCreated: 0 }));
    render(<ImportModal slug="acme-aps" onImported={noop} onClose={noop} />);
    await userEvent.upload(screen.getByLabelText("Fil"), csvFile());
    await userEvent.click(screen.getByRole("button", { name: "Importér" }));
    expect(
      await screen.findByText(/Genkendt som Dinero — Kontakter/),
    ).toBeInTheDocument();
    expect(screen.getByText(/25 kontakter oprettet/)).toBeInTheDocument();
  });

  test("a CVR-credentials gap is shown as a calm setup note", async () => {
    mockFetch(
      importRoute({ enrichmentFailures: 20 }, [
        "CVR-berigelse af 'Acme' fejlede: CVR-opslag kræver miljøvariablerne " +
          "CVR_USERNAME og CVR_PASSWORD — opret adgang på virk.dk",
      ]),
    );
    render(<ImportModal slug="acme-aps" onImported={noop} onClose={noop} />);
    await userEvent.upload(screen.getByLabelText("Fil"), csvFile());
    await userEvent.click(screen.getByRole("button", { name: "Importér" }));
    expect(
      await screen.findByText(/CVR-berigelse blev ikke kørt/),
    ).toBeInTheDocument();
  });

  test("rows that could not be imported are surfaced, not just counted", async () => {
    mockFetch(
      importRoute({ parsed: 2, customersCreated: 1, vendorsCreated: 0 }, [
        "linje 3: kontakt uden navn — sprunget over",
      ]),
    );
    render(<ImportModal slug="acme-aps" onImported={noop} onClose={noop} />);
    await userEvent.upload(screen.getByLabelText("Fil"), csvFile());
    await userEvent.click(screen.getByRole("button", { name: "Importér" }));
    expect(
      await screen.findByText(/kontakt uden navn/),
    ).toBeInTheDocument();
  });

  test("a 409 backup-lock conflict is shown as a kind lock banner", async () => {
    mockFetch({
      "POST /api/companies/acme-aps/import": {
        __error: {
          code: "conflict",
          message: "Bogføring er låst: en ugentlig backup er overskredet.",
        },
      },
    });
    const onClose = vi.fn();
    render(
      <ImportModal slug="acme-aps" onImported={noop} onClose={onClose} />,
    );
    await userEvent.upload(screen.getByLabelText("Fil"), csvFile());
    await userEvent.click(screen.getByRole("button", { name: "Importér" }));
    expect(
      await screen.findByText("Bogføringen er låst"),
    ).toBeInTheDocument();
    expect(onClose).not.toHaveBeenCalled();
  });

  test("an unrecognised file is shown as an error banner", async () => {
    mockFetch({
      "POST /api/companies/acme-aps/import": {
        __error: {
          code: "bad_request",
          message: "Filen blev ikke genkendt som et understøttet eksportformat.",
        },
      },
    });
    render(<ImportModal slug="acme-aps" onImported={noop} onClose={noop} />);
    await userEvent.upload(screen.getByLabelText("Fil"), csvFile());
    await userEvent.click(screen.getByRole("button", { name: "Importér" }));
    expect(
      await screen.findByText(/Filen blev ikke genkendt/),
    ).toBeInTheDocument();
  });

  test("Annullér closes the modal without importing", async () => {
    const onClose = vi.fn();
    render(
      <ImportModal slug="acme-aps" onImported={noop} onClose={onClose} />,
    );
    await userEvent.click(screen.getByRole("button", { name: "Annullér" }));
    expect(onClose).toHaveBeenCalled();
  });
});
