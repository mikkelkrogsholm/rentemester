import { describe, expect, test } from "vitest";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { DocumentsView } from "./DocumentsView";
import { renderAt } from "../test/render";
import { documents, mockFetch } from "../test/fixtures";

const FISCAL_YEARS = {
  slug: "acme-aps",
  years: [
    { label: "2026", start: "2026-01-01", end: "2026-12-31", source: "live" },
    { label: "2025", start: null, end: null, source: "archive" },
  ],
};

function route(over = {}) {
  return {
    "GET /api/companies/acme-aps/documents": { documents: documents(over) },
    "GET /api/companies/acme-aps/fiscal-years": { fiscalYears: FISCAL_YEARS },
  };
}

function renderView(route = "/companies/acme-aps/bilag") {
  return renderAt(<DocumentsView />, {
    route,
    path: "/companies/:slug/bilag",
  });
}

describe("DocumentsView — Bilag", () => {
  test("lists ingested documents with their details", async () => {
    mockFetch(route());
    renderView();
    expect(
      await screen.findByRole("heading", { name: "Acme ApS" }),
    ).toBeInTheDocument();
    expect(screen.getByText("DOC-2026-000001")).toBeInTheDocument();
    expect(screen.getByText("Leverandør ApS")).toBeInTheDocument();
  });

  test("shows the linked journal entry for a booked document", async () => {
    mockFetch(route());
    renderView();
    expect(await screen.findByText(/B-2026-0002/)).toBeInTheDocument();
  });

  test("shows the linked entry's text and amount so the row is informative", async () => {
    mockFetch(route());
    renderView();
    // The voucher's posting text — what the receipt is for.
    expect(
      await screen.findByText("Køb af kontorartikler"),
    ).toBeInTheDocument();
  });

  test("falls back to the linked entry's total when the document amount is blank", async () => {
    mockFetch(
      route({
        documents: [
          {
            id: 5,
            documentNo: "DOC-2026-000005",
            source: "dinero-import",
            filename: "bilag.pdf",
            documentType: "purchase_sale",
            supplierName: null,
            invoiceNo: null,
            invoiceDate: null,
            amountIncVat: null,
            currency: "DKK",
            status: "ingested",
            voucherRef: "5",
            journalEntryNo: "B-2026-0005",
            journalEntryId: 5,
            journalEntryText: "Kontorartikler hos Lev ApS",
            journalEntryTotal: 412.5,
            hasFile: true,
          },
        ],
        linkedCount: 1,
        unlinkedCount: 0,
      }),
    );
    renderView();
    expect(
      await screen.findByText("Kontorartikler hos Lev ApS"),
    ).toBeInTheDocument();
    expect(screen.getByText(/412,50/)).toBeInTheDocument();
  });

  test("shows an unbooked marker when no journal entry is linked", async () => {
    mockFetch(
      route({
        documents: [
          {
            id: 9,
            documentNo: "DOC-2026-000009",
            source: "email",
            filename: "kvittering.pdf",
            documentType: "purchase_sale",
            supplierName: "Anden ApS",
            invoiceNo: null,
            invoiceDate: null,
            amountIncVat: null,
            currency: "DKK",
            status: "ingested",
            voucherRef: null,
            journalEntryNo: null,
            journalEntryId: null,
            journalEntryText: null,
            journalEntryTotal: null,
            hasFile: true,
          },
        ],
        linkedCount: 0,
        unlinkedCount: 1,
      }),
    );
    renderView();
    expect(await screen.findByText("Ikke bogført")).toBeInTheDocument();
  });

  // #213 slice 3 — the document-intake write action.

  test("a live year offers an Indlæs bilag action", async () => {
    mockFetch(route());
    renderView();
    await screen.findByRole("heading", { name: "Acme ApS" });
    expect(
      screen.getByRole("button", { name: "Indlæs bilag" }),
    ).toBeInTheDocument();
  });

  test("an archived year offers no intake action", async () => {
    mockFetch(route());
    // The URL pins the archived year 2025; its fiscal-year source is `archive`.
    renderView("/companies/acme-aps/bilag?year=2025");
    await screen.findByRole("heading", { name: "Acme ApS" });
    expect(
      screen.queryByRole("button", { name: "Indlæs bilag" }),
    ).not.toBeInTheDocument();
  });

  test("clicking Indlæs bilag opens the intake modal", async () => {
    mockFetch(route());
    renderView();
    await userEvent.click(
      await screen.findByRole("button", { name: "Indlæs bilag" }),
    );
    expect(
      screen.getByRole("dialog", { name: "Indlæs bilag" }),
    ).toBeInTheDocument();
  });

  test("offers a link to open a document's stored file", async () => {
    mockFetch(route());
    renderView();
    const link = await screen.findByRole("link", { name: "Åbn bilag" });
    expect(link).toHaveAttribute(
      "href",
      "/api/companies/acme-aps/documents/1/file",
    );
  });

  test("shows no file link when the document has no stored file", async () => {
    mockFetch(
      route({
        documents: [{ ...documents().documents[0]!, hasFile: false }],
        linkedCount: 1,
        unlinkedCount: 0,
      }),
    );
    renderView();
    await screen.findByRole("heading", { name: "Acme ApS" });
    expect(
      screen.queryByRole("link", { name: "Åbn bilag" }),
    ).not.toBeInTheDocument();
  });

  // #433 — filter-bar: fritekst, datointerval, status og type.
  const MULTI_DOCS = {
    documents: [
      {
        id: 1,
        documentNo: "DOC-2026-000001",
        source: "dinero-import",
        filename: "brother.pdf",
        documentType: "purchase_sale",
        supplierName: "Brother Nordic",
        invoiceNo: "F-1001",
        invoiceDate: "2026-03-15",
        amountIncVat: 4200,
        currency: "DKK",
        status: "ingested",
        voucherRef: "1",
        journalEntryNo: "B-2026-0001",
        journalEntryId: 1,
        journalEntryText: "Printer hos Brother",
        journalEntryTotal: 4200,
        hasFile: true,
      },
      {
        id: 2,
        documentNo: "DOC-2026-000002",
        source: "email",
        filename: "energinet.pdf",
        documentType: "purchase_sale",
        supplierName: "Energinet",
        invoiceNo: "EN-77",
        invoiceDate: "2026-01-10",
        amountIncVat: 980,
        currency: "DKK",
        status: "ingested",
        voucherRef: null,
        journalEntryNo: null,
        journalEntryId: null,
        journalEntryText: null,
        journalEntryTotal: null,
        hasFile: true,
      },
      {
        id: 3,
        documentNo: "DOC-2026-000003",
        source: "scan",
        filename: "bon.pdf",
        documentType: "cash_register_receipt",
        supplierName: "Føtex",
        invoiceNo: null,
        invoiceDate: "2026-05-04",
        amountIncVat: 132.5,
        currency: "DKK",
        status: "ingested",
        voucherRef: "3",
        journalEntryNo: "B-2026-0003",
        journalEntryId: 3,
        journalEntryText: "Kaffe og mælk",
        journalEntryTotal: 132.5,
        hasFile: false,
      },
    ],
    linkedCount: 2,
    unlinkedCount: 1,
  };

  test("filters by free-text on supplier name (#433)", async () => {
    mockFetch(route(MULTI_DOCS));
    renderView();
    await screen.findByText("Brother Nordic");
    const search = screen.getByPlaceholderText(
      /Søg på leverandør, bilagsnr/i,
    );
    await userEvent.type(search, "energinet");
    expect(screen.queryByText("Brother Nordic")).not.toBeInTheDocument();
    expect(screen.getByText("Energinet")).toBeInTheDocument();
    expect(screen.getByText(/1 af 3 bilag matcher/i)).toBeInTheDocument();
  });

  test("filters by status — ikke bogført (#433)", async () => {
    mockFetch(route(MULTI_DOCS));
    renderView();
    await screen.findByText("Brother Nordic");
    const status = screen.getByLabelText(/Status/i);
    await userEvent.selectOptions(status, "unbooked");
    expect(screen.queryByText("Brother Nordic")).not.toBeInTheDocument();
    expect(screen.getByText("Energinet")).toBeInTheDocument();
    expect(screen.queryByText("Føtex")).not.toBeInTheDocument();
  });

  test("filters by date range (#433)", async () => {
    mockFetch(route(MULTI_DOCS));
    renderView();
    await screen.findByText("Brother Nordic");
    const fromInput = screen.getByLabelText(/^Fra$/i);
    await userEvent.type(fromInput, "2026-02-01");
    expect(screen.queryByText("Energinet")).not.toBeInTheDocument();
    expect(screen.getByText("Brother Nordic")).toBeInTheDocument();
    expect(screen.getByText("Føtex")).toBeInTheDocument();
  });

  test("filter state is reflected in URL params (#433)", async () => {
    mockFetch(route(MULTI_DOCS));
    renderView("/companies/acme-aps/bilag?q=energinet&status=unbooked");
    await screen.findByText("Energinet");
    expect(screen.queryByText("Brother Nordic")).not.toBeInTheDocument();
    expect(screen.queryByText("Føtex")).not.toBeInTheDocument();
  });
});
