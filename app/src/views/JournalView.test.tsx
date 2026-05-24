import { describe, expect, test, vi } from "vitest";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { JournalView } from "./JournalView";
import { renderAt } from "../test/render";
import { journal, mockFetch } from "../test/fixtures";

function route(over = {}) {
  return {
    "GET /api/companies/acme-aps/journal": { journal: journal(over) },
  };
}

function renderView() {
  return renderAt(<JournalView />, {
    route: "/companies/acme-aps/posteringer",
    path: "/companies/:slug/posteringer",
  });
}

describe("JournalView — Posteringer", () => {
  test("lists the posted journal entries", async () => {
    mockFetch(route());
    renderView();
    expect(
      await screen.findByRole("heading", { name: "Acme ApS" }),
    ).toBeInTheDocument();
    expect(screen.getByText("B-2026-0001")).toBeInTheDocument();
    expect(screen.getByText("Salg af ydelse")).toBeInTheDocument();
  });

  test("clicking an entry drills into its debit/credit lines", async () => {
    mockFetch(route());
    renderView();
    const summary = await screen.findByRole("button", {
      name: /Salg af ydelse/,
    });
    expect(screen.queryByText("Omsætning")).not.toBeInTheDocument();
    await userEvent.click(summary);
    expect(await screen.findByText("Omsætning")).toBeInTheDocument();
    expect(screen.getByText("Salgsmoms")).toBeInTheDocument();
  });

  test("the company sub-nav exposes the new tabs", async () => {
    mockFetch(route());
    renderView();
    const bankTab = await screen.findByRole("link", { name: "Bank" });
    expect(bankTab).toHaveAttribute(
      "href",
      expect.stringContaining("/companies/acme-aps/bank"),
    );
  });

  test("the fiscal-year selector reloads for the chosen year", async () => {
    mockFetch(route());
    renderView();
    const select = await screen.findByLabelText("Vælg regnskabsår");
    await userEvent.selectOptions(select, "2025");
    const calls = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls;
    const lastUrl = String(calls[calls.length - 1]![0]);
    expect(lastUrl).toContain("year=2025");
  });

  test("an archived year renders the archived entries under a read-only banner", async () => {
    mockFetch(
      route({ archived: true, archivedSource: "dinero", selectedYear: "2025" }),
    );
    renderView();
    expect(
      await screen.findByText(/Arkiveret regnskabsår 2025 — skrivebeskyttet/),
    ).toBeInTheDocument();
    // The archived Posteringer are still rendered as real entry rows.
    expect(screen.getByText(/posteringer/)).toBeInTheDocument();
  });

  test("an ?account= filter fetches the filtered journal and names the account", async () => {
    mockFetch(
      route({
        accountFilter: { accountNo: "55000", name: "Bank" },
      }),
    );
    renderAt(<JournalView />, {
      route: "/companies/acme-aps/posteringer?account=55000",
      path: "/companies/:slug/posteringer",
    });
    // The filter banner names the account.
    expect(await screen.findByText("Bank")).toBeInTheDocument();
    expect(screen.getByText("55000")).toBeInTheDocument();
    // The fetch carried the account param.
    const calls = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls;
    expect(calls.some((c) => String(c[0]).includes("account=55000"))).toBe(
      true,
    );
    // "Vis alle posteringer" clears the filter.
    expect(
      screen.getByRole("button", { name: "Vis alle posteringer" }),
    ).toBeInTheDocument();
  });
});

// --- #396: client-side filter-bar (fritekst, datointerval, beløbsspand) -----

function multiEntryJournal() {
  return route({
    entries: [
      {
        id: 1,
        entryNo: "B-2026-0001",
        date: "2026-01-15",
        text: "Salg af ydelse",
        total: 22286.28,
        lines: [
          {
            accountNo: "55000",
            accountName: "Bank",
            debit: 22286.28,
            credit: 0,
            text: null,
          },
          {
            accountNo: "1000",
            accountName: "Omsætning",
            debit: 0,
            credit: 17829.02,
            text: null,
          },
        ],
      },
      {
        id: 2,
        entryNo: "B-2026-0042",
        date: "2026-03-10",
        text: "Køb af printer",
        total: 1200,
        lines: [
          {
            accountNo: "2400",
            accountName: "Kontorudstyr",
            debit: 1200,
            credit: 0,
            text: "HP LaserJet",
          },
          {
            accountNo: "55000",
            accountName: "Bank",
            debit: 0,
            credit: 1200,
            text: null,
          },
        ],
      },
      {
        id: 3,
        entryNo: "B-2026-0099",
        date: "2026-07-01",
        text: "Internet abonnement",
        total: 499,
        lines: [
          {
            accountNo: "2200",
            accountName: "Telefon og internet",
            debit: 499,
            credit: 0,
            text: null,
          },
          {
            accountNo: "55000",
            accountName: "Bank",
            debit: 0,
            credit: 499,
            text: null,
          },
        ],
      },
    ],
  });
}

describe("JournalView — #396 filter-bar", () => {
  test("a filter-bar with fritekstsøgning, datointerval and beløb is rendered", async () => {
    mockFetch(multiEntryJournal());
    renderView();
    await screen.findByRole("heading", { name: "Acme ApS" });
    expect(screen.getByPlaceholderText(/Søg/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Fra/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Til/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Beløb min/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Beløb maks/i)).toBeInTheDocument();
  });

  test("fritekstsøgning matches entry text", async () => {
    mockFetch(multiEntryJournal());
    renderView();
    await screen.findByText("Salg af ydelse");
    expect(screen.getByText("Køb af printer")).toBeInTheDocument();
    expect(screen.getByText("Internet abonnement")).toBeInTheDocument();
    const search = screen.getByPlaceholderText(/Søg/i);
    await userEvent.type(search, "printer");
    expect(screen.queryByText("Salg af ydelse")).not.toBeInTheDocument();
    expect(screen.getByText("Køb af printer")).toBeInTheDocument();
    expect(screen.queryByText("Internet abonnement")).not.toBeInTheDocument();
    // Count: "1 af 3 posteringer matcher"
    expect(screen.getByText(/1 af 3 posteringer/)).toBeInTheDocument();
  });

  test("fritekstsøgning matches the bilagsnummer (entryNo)", async () => {
    mockFetch(multiEntryJournal());
    renderView();
    await screen.findByText("Salg af ydelse");
    const search = screen.getByPlaceholderText(/Søg/i);
    await userEvent.type(search, "0042");
    expect(screen.queryByText("Salg af ydelse")).not.toBeInTheDocument();
    expect(screen.getByText("Køb af printer")).toBeInTheDocument();
  });

  test("fritekstsøgning matches line text (HP LaserJet)", async () => {
    mockFetch(multiEntryJournal());
    renderView();
    await screen.findByText("Salg af ydelse");
    const search = screen.getByPlaceholderText(/Søg/i);
    await userEvent.type(search, "laserjet");
    expect(screen.queryByText("Salg af ydelse")).not.toBeInTheDocument();
    expect(screen.getByText("Køb af printer")).toBeInTheDocument();
  });

  test("datointerval skærer korrekt: fra=2026-02-01 fjerner januar-entry", async () => {
    mockFetch(multiEntryJournal());
    renderView();
    await screen.findByText("Salg af ydelse");
    const fromInput = screen.getByLabelText(/Fra/i);
    await userEvent.type(fromInput, "2026-02-01");
    expect(screen.queryByText("Salg af ydelse")).not.toBeInTheDocument();
    expect(screen.getByText("Køb af printer")).toBeInTheDocument();
    expect(screen.getByText("Internet abonnement")).toBeInTheDocument();
  });

  test("datointerval skærer korrekt: til=2026-06-30 fjerner juli-entry", async () => {
    mockFetch(multiEntryJournal());
    renderView();
    await screen.findByText("Salg af ydelse");
    const toInput = screen.getByLabelText(/Til/i);
    await userEvent.type(toInput, "2026-06-30");
    expect(screen.getByText("Salg af ydelse")).toBeInTheDocument();
    expect(screen.getByText("Køb af printer")).toBeInTheDocument();
    expect(screen.queryByText("Internet abonnement")).not.toBeInTheDocument();
  });

  test("beløbsspand filtrerer på total", async () => {
    mockFetch(multiEntryJournal());
    renderView();
    await screen.findByText("Salg af ydelse");
    const minInput = screen.getByLabelText(/Beløb min/i);
    await userEvent.type(minInput, "1000");
    // 22286 og 1200 består; 499 fjernes
    expect(screen.getByText("Salg af ydelse")).toBeInTheDocument();
    expect(screen.getByText("Køb af printer")).toBeInTheDocument();
    expect(screen.queryByText("Internet abonnement")).not.toBeInTheDocument();
    const maxInput = screen.getByLabelText(/Beløb maks/i);
    await userEvent.type(maxInput, "5000");
    // 22286 fjernes nu også; kun 1200 består
    expect(screen.queryByText("Salg af ydelse")).not.toBeInTheDocument();
    expect(screen.getByText("Køb af printer")).toBeInTheDocument();
  });

  test("tomt resultat vises pænt", async () => {
    mockFetch(multiEntryJournal());
    renderView();
    await screen.findByText("Salg af ydelse");
    const search = screen.getByPlaceholderText(/Søg/i);
    await userEvent.type(search, "intet-der-matcher-noget");
    expect(screen.queryByText("Salg af ydelse")).not.toBeInTheDocument();
    expect(
      screen.getByText(/Ingen posteringer matcher filtrene/),
    ).toBeInTheDocument();
  });

  test("filtre læses fra URL-params ved første render (q + datointerval)", async () => {
    mockFetch(multiEntryJournal());
    renderAt(<JournalView />, {
      route:
        "/companies/acme-aps/posteringer?q=printer&from=2026-01-01&to=2026-12-31&amountMin=100&amountMax=5000",
      path: "/companies/:slug/posteringer",
    });
    await screen.findByRole("heading", { name: "Acme ApS" });
    const search = screen.getByPlaceholderText(/Søg/i) as HTMLInputElement;
    expect(search.value).toBe("printer");
    expect((screen.getByLabelText(/Fra/i) as HTMLInputElement).value).toBe(
      "2026-01-01",
    );
    expect((screen.getByLabelText(/Til/i) as HTMLInputElement).value).toBe(
      "2026-12-31",
    );
    expect((screen.getByLabelText(/Beløb min/i) as HTMLInputElement).value).toBe(
      "100",
    );
    expect(
      (screen.getByLabelText(/Beløb maks/i) as HTMLInputElement).value,
    ).toBe("5000");
    // And the filter is actually applied — only "Køb af printer" survives.
    expect(screen.queryByText("Salg af ydelse")).not.toBeInTheDocument();
    expect(screen.getByText("Køb af printer")).toBeInTheDocument();
    expect(screen.queryByText("Internet abonnement")).not.toBeInTheDocument();
  });

  test("Ryd filtre-knap nulstiller alle filtre", async () => {
    mockFetch(multiEntryJournal());
    renderView();
    await screen.findByText("Salg af ydelse");
    const search = screen.getByPlaceholderText(/Søg/i);
    await userEvent.type(search, "printer");
    expect(screen.queryByText("Salg af ydelse")).not.toBeInTheDocument();
    const clear = screen.getByRole("button", { name: /Ryd filtre/i });
    await userEvent.click(clear);
    expect(screen.getByText("Salg af ydelse")).toBeInTheDocument();
    expect(screen.getByText("Køb af printer")).toBeInTheDocument();
    expect(screen.getByText("Internet abonnement")).toBeInTheDocument();
    expect((search as HTMLInputElement).value).toBe("");
  });
});
