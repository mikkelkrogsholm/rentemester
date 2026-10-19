import { afterEach, describe, expect, test, vi } from "bun:test";
import { StrictMode } from "react";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ImportModal } from "./ImportModal";
import { mockFetch } from "../test/fixtures";
import { ApiError } from "../lib/api/_shared";
import { api, type DataImportSummary } from "../lib/api";
afterEach(() => vi.restoreAllMocks());

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
      const calls = (globalThis.fetch as unknown as ReturnType<typeof vi.fn>).mock.calls;
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
      await screen.findByText(/Adgang til CVR-registeret er ikke konfigureret/),
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


test("StrictMode retains default-on enrichment and truthful mixed cache/failure role counts", async () => {
  mockFetch(importRoute({customersCreated:1,vendorsCreated:1,enriched:1,enrichmentFailures:2},[
    "CVR-berigelse af Synthetic kunde fejlede: CVR_USERNAME mangler",
    "CVR-berigelse af Synthetic leverandør fejlede: CVR_USERNAME mangler",
    "linje 4: betalingsfrist blev afvist",
  ]));
  render(<StrictMode><ImportModal slug="acme-aps" onImported={noop} onClose={noop}/></StrictMode>);
  expect(screen.getByRole("checkbox", {name:/Berig danske virksomheder/})).toBeChecked();
  await userEvent.upload(screen.getByLabelText("Fil"),csvFile());
  await userEvent.click(screen.getByRole("button",{name:"Importér"}));
  expect(await screen.findByText(/1 beriget med CVR-data/)).toBeInTheDocument();
  expect(screen.getByText(/CVR-berigelse mislykkedes i 2 forsøg/)).toHaveTextContent("Samme kontakt kan tælle to gange");
  expect(screen.getByText(/linje 4: betalingsfrist blev afvist/)).toBeInTheDocument();
  expect(screen.getByText("Bemærkninger til CVR-berigelse")).toBeInTheDocument();
  expect(screen.queryByText(/blev ikke kørt/)).toBeNull();
});

test("repeat receipt reports existing roles without claiming new creation or provider work", async () => {
  mockFetch(importRoute({customersCreated:0,vendorsCreated:0,skipped:4,enriched:0,enrichmentFailures:0}));
  render(<ImportModal slug="acme-aps" onImported={noop} onClose={noop}/>);
  await userEvent.upload(screen.getByLabelText("Fil"),csvFile());
  await userEvent.click(screen.getByRole("checkbox",{name:/Berig danske virksomheder/}));
  await userEvent.click(screen.getByRole("button",{name:"Importér"}));
  expect(await screen.findByText(/0 kontakter oprettet/)).toHaveTextContent("4 fandtes allerede");
  expect(screen.queryByText(/beriget med CVR-data/)).toBeNull();
  const calls=(fetch as unknown as ReturnType<typeof vi.fn>).mock.calls;
  expect(calls).toHaveLength(1);
  expect(JSON.parse(String(calls[0]![1].body)).enrichCvr).toBe(false);
});

test("late file read cannot replace a newer selected original", async () => {
  let resolve!: (text:string)=>void;
  const old=csvFile("old.csv");
  Object.defineProperty(old,"text",{value:()=>new Promise<string>(r=>{resolve=r;})});
  render(<ImportModal slug="acme-aps" onImported={noop} onClose={noop}/>);
  await userEvent.upload(screen.getByLabelText("Fil"),old);
  expect(screen.getByRole("button",{name:"Importér"})).toBeDisabled();
  await userEvent.upload(screen.getByLabelText("Fil"),csvFile("new.csv"));
  await screen.findByText("Valgt: new.csv");
  resolve("old CSV content");
  await Promise.resolve();
  expect(screen.getByText("Valgt: new.csv")).toBeInTheDocument();
  expect(screen.queryByText("Valgt: old.csv")).toBeNull();
});

test("company change resets the form and ignores the old company's late import receipt", async () => {
  let resolve!: (summary:DataImportSummary)=>void;
  const imported=vi.fn();
  vi.spyOn(api,"importData").mockImplementation(()=>new Promise<DataImportSummary>(r=>{resolve=r;}));
  const view=render(<ImportModal slug="acme-aps" onImported={imported} onClose={noop}/>);
  await userEvent.upload(screen.getByLabelText("Fil"),csvFile());
  await userEvent.click(screen.getByRole("button",{name:"Importér"}));
  view.rerender(<ImportModal slug="another-company" onImported={imported} onClose={noop}/>);
  expect(screen.getByRole("button",{name:"Importér"})).toBeDisabled();
  resolve(importRoute()["POST /api/companies/acme-aps/import"].import as DataImportSummary);
  await Promise.resolve();
  expect(imported).not.toHaveBeenCalled();
  expect(screen.queryByText(/Genkendt som/)).toBeNull();
  expect(screen.queryByText("Valgt: Kontakter.csv")).toBeNull();
});


test("unknown import blocks another write while a definite refusal keeps the selected file usable", async () => {
  const request=vi.spyOn(api,"importData").mockRejectedValueOnce(new ApiError("invalid","Synthetic row refused",422)).mockRejectedValueOnce(new ApiError("network","Synthetic connection lost",0));
  render(<ImportModal slug="acme-aps" onImported={noop} onClose={noop}/>);
  await userEvent.upload(screen.getByLabelText("Fil"),csvFile());
  await userEvent.click(screen.getByRole("button",{name:"Importér"}));
  await screen.findByText("Synthetic row refused");
  expect(screen.getByRole("button",{name:"Importér"})).not.toBeDisabled();
  await userEvent.click(screen.getByRole("button",{name:"Importér"}));
  await screen.findByText("Synthetic connection lost");
  expect(screen.getByRole("button",{name:"Importér"})).toBeDisabled();
  expect(screen.getByLabelText("Fil")).toBeDisabled();
  expect(request).toHaveBeenCalledTimes(2);
  expect(screen.queryByText(/Genkendt som/)).toBeNull();
});
