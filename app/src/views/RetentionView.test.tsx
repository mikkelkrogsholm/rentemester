import { describe, expect, test } from "vitest";
import { screen, within } from "@testing-library/react";
import { RetentionView } from "./RetentionView";
import { renderAt } from "../test/render";
import { mockFetch } from "../test/fixtures";

const sample = {
  ok: true as const,
  retention: {
    slug: "acme-aps",
    company: {
      name: "Acme ApS",
      cvr: "DK12345678",
      country: "DK",
      currency: "DKK",
    },
    report: {
      ok: true,
      asOf: "2026-05-25",
      appliedRules: ["DK-BOOKKEEPING-RETENTION-001"],
      rows: [
        {
          table: "documents" as const,
          total: 42,
          expired: 0,
          nextExpiry: "2031-12-31",
          oldestExpired: null,
        },
        {
          table: "journal_entries" as const,
          total: 100,
          expired: 3,
          nextExpiry: "2031-12-31",
          oldestExpired: "2020-12-31",
        },
        {
          table: "bank_transactions" as const,
          total: 0,
          expired: 0,
          nextExpiry: null,
          oldestExpired: null,
        },
      ],
      errors: [],
    },
    legalCitation: {
      sourceId: "DK-BOGFORINGSLOVEN-2022-700",
      note:
        "Bogføringsloven § 12, stk. 1 — bilag og bogføringsmateriale skal opbevares i 5 år …",
    },
  },
};

function renderView() {
  mockFetch({ "GET /api/companies/acme-aps/retention": sample });
  return renderAt(<RetentionView />, {
    route: "/companies/acme-aps/retention",
    path: "/companies/:slug/retention",
  });
}

describe("RetentionView (#343)", () => {
  test("viser totals, udløbne og næste udløb pr. domæne", async () => {
    renderView();
    expect(await screen.findByText(/Acme ApS/)).toBeInTheDocument();
    // Bilag-rækken
    const bilagRow = (
      await screen.findByRole("cell", { name: "Bilag" })
    ).closest("tr")!;
    expect(within(bilagRow as HTMLElement).getByText("42")).toBeInTheDocument();
    expect(
      within(bilagRow as HTMLElement).getByText("2031-12-31"),
    ).toBeInTheDocument();

    // Posteringer-rækken har 3 udløbne (skal være highligted)
    const journalRow = (
      await screen.findByRole("cell", { name: "Posteringer" })
    ).closest("tr")!;
    expect(within(journalRow as HTMLElement).getByText("3")).toBeInTheDocument();
    expect(
      within(journalRow as HTMLElement).getByText("2020-12-31"),
    ).toBeInTheDocument();

    // Banktransaktioner-rækken er tom
    const bankRow = (
      await screen.findByRole("cell", { name: "Banktransaktioner" })
    ).closest("tr")!;
    expect(within(bankRow as HTMLElement).getAllByText("0").length).toBeGreaterThan(0);
  });

  test("highligher domæner med udløbne poster (callout + warn-klasse)", async () => {
    renderView();
    expect(
      await screen.findByText(/har overskredet den 5-årige opbevaringspligt/),
    ).toBeInTheDocument();
  });

  test("citerer bogføringslovens § 12 og linker til Lovgrundlag-viewet", async () => {
    renderView();
    expect(await screen.findByText(/§ 12, stk. 1/)).toBeInTheDocument();
    const link = screen.getByRole("link", {
      name: /Se DK-BOGFORINGSLOVEN-2022-700/,
    });
    expect(link).toHaveAttribute(
      "href",
      "/lovgrundlag#DK-BOGFORINGSLOVEN-2022-700",
    );
  });

  test("read-only — ingen oprettelses-/redigerings-knapper", async () => {
    renderView();
    await screen.findByText(/Acme ApS/);
    expect(
      screen.queryByRole("button", { name: /Opret|Rediger|Slet|Tilføj|Anonymis/i }),
    ).not.toBeInTheDocument();
  });
});
