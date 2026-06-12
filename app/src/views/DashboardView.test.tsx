import { describe, expect, test, vi } from "vitest";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { DashboardView } from "./DashboardView";
import { renderAt } from "../test/render";
import { overview, mockFetch } from "../test/fixtures";

// The P&L chart needs a real <canvas> 2D context, which happy-dom lacks —
// stub it so the view's data wiring is what the specs exercise.
vi.mock("../components/PnlChart", () => ({
  PnlChart: () => <div data-testid="pnl-chart" />,
}));

function overviewRoute(over = {}) {
  return {
    "GET /api/companies/acme-aps/overview": { overview: overview(over) },
  };
}

function renderDashboard() {
  return renderAt(<DashboardView />, {
    route: "/companies/acme-aps",
    path: "/companies/:slug",
  });
}

describe("DashboardView — Overblik", () => {
  test("renders the company header and the three KPI cards", async () => {
    mockFetch(overviewRoute());
    renderDashboard();
    expect(
      await screen.findByRole("heading", { name: "Acme ApS" }),
    ).toBeInTheDocument();
    expect(screen.getByText("Omsætning")).toBeInTheDocument();
    expect(screen.getByText("Udgifter")).toBeInTheDocument();
    expect(screen.getByText("Resultat")).toBeInTheDocument();
  });

  test("shows the ground-truth result figure", async () => {
    mockFetch(overviewRoute());
    renderDashboard();
    const result = (await screen.findByText("Resultat")).closest(".kpi")!;
    expect(
      within(result as HTMLElement).getByText(/13\.234,82/),
    ).toBeInTheDocument();
  });

  test("renders the P&L chart and the VAT status card", async () => {
    mockFetch(overviewRoute());
    renderDashboard();
    expect(await screen.findByTestId("pnl-chart")).toBeInTheDocument();
    const vat = screen
      .getByRole("heading", { name: "Moms" })
      .closest(".status-card")!;
    expect(within(vat as HTMLElement).getByText(/3\.371,00/)).toBeInTheDocument();
    expect(
      within(vat as HTMLElement).getByText(/Q2 2026/),
    ).toBeInTheDocument();
  });

  test("lists recent entries when present", async () => {
    mockFetch(
      overviewRoute({
        recentEntries: [
          {
            id: 1,
            entryNo: "2026-0001",
            date: "2026-02-27",
            text: "Momsafregning",
            amount: 5334,
          },
        ],
      }),
    );
    renderDashboard();
    expect(await screen.findByText("Momsafregning")).toBeInTheDocument();
  });

  test("the fiscal-year selector reloads the overview for the chosen year", async () => {
    mockFetch(overviewRoute());
    renderDashboard();
    const select = await screen.findByLabelText("Vælg regnskabsår");
    await userEvent.selectOptions(select, "2025");
    // The last fetch call must carry the chosen year.
    const calls = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls;
    const lastUrl = String(calls[calls.length - 1]![0]);
    expect(lastUrl).toContain("year=2025");
  });

  test("the Bank card shows the actual balance, booked balance and difference", async () => {
    mockFetch(overviewRoute());
    renderDashboard();
    await screen.findByRole("heading", { name: "Acme ApS" });
    const bankCard = screen
      .getByRole("heading", { name: "Bank" })
      .closest(".status-card")!;
    // Headline figure is the actual statement balance.
    const figure = bankCard.querySelector(".status-figure")!;
    expect(figure.textContent).toMatch(/23\.654,75/);
    // The note carries the booked figure and the unreconciled gap.
    const note = bankCard.querySelector(".status-note")!;
    expect(note.textContent).toMatch(/Bogført.*41\.388,03/);
    expect(
      within(bankCard as HTMLElement).getByText(/ikke afstemt/),
    ).toBeInTheDocument();
  });

  test("groups exceptions into one Danish, clickable summary line", async () => {
    mockFetch(
      overviewRoute({
        exceptions: {
          count: 362,
          rows: [],
          groups: [
            {
              type: "UNMATCHED_BANK_TRANSACTION",
              count: 362,
              severity: "medium",
              label: "362 banktransaktioner mangler afstemning",
              link: "bank",
            },
          ],
        },
      }),
    );
    renderDashboard();
    const line = await screen.findByText(
      "362 banktransaktioner mangler afstemning",
    );
    expect(line).toBeInTheDocument();
    // The line links to the Bank view.
    expect(line.closest("a")).toHaveAttribute(
      "href",
      "/companies/acme-aps/bank",
    );
  });

  test("an archived year renders the P&L overview under a read-only banner", async () => {
    mockFetch(
      overviewRoute({
        archived: true,
        archivedSource: "dinero",
        selectedYear: "2025",
        vat: null,
      }),
    );
    renderDashboard();
    expect(
      await screen.findByText(/Arkiveret regnskabsår 2025 — skrivebeskyttet/),
    ).toBeInTheDocument();
    // The KPI cards still render from the archived figures.
    expect(screen.getByText("Omsætning")).toBeInTheDocument();
    expect(screen.getByText("Resultat")).toBeInTheDocument();
    // Live-only data is honestly marked unavailable rather than faked.
    expect(
      screen.getByText(/ikke tilgængelige for et arkiveret regnskabsår/),
    ).toBeInTheDocument();
  });

  test("shows the 'Senest bogført pr.' date near the period header", async () => {
    mockFetch(overviewRoute({ lastPostedDate: "2026-04-30" }));
    renderDashboard();
    expect(
      await screen.findByText(/Senest bogført pr\. 2026-04-30/),
    ).toBeInTheDocument();
  });

  test("labels the resultat ÷ omsætning ratio Overskudsgrad, not Bruttomargin (#304)", async () => {
    mockFetch(
      overviewRoute({
        keyFigures: { bruttomargin: 0.7423, egenkapitalandel: 0.9186 },
      }),
    );
    renderDashboard();
    // The figure computes resultat ÷ omsætning — that is the profit margin
    // (overskudsgrad), not the gross margin. The label must match the maths.
    expect(await screen.findByText("Overskudsgrad")).toBeInTheDocument();
    expect(screen.queryByText("Bruttomargin")).not.toBeInTheDocument();
    expect(screen.getByText("Egenkapitalandel")).toBeInTheDocument();
    const margin = screen.getByText("Overskudsgrad").closest(".key-figure")!;
    expect(
      within(margin as HTMLElement).getByText(/74,2\s*%/),
    ).toBeInTheDocument();
  });

  test("the KPI cards drill into the Resultatopgørelse, carrying the year", async () => {
    mockFetch(overviewRoute());
    renderDashboard();
    const omsaetning = (await screen.findByText("Omsætning")).closest("a")!;
    expect(omsaetning).toHaveAttribute(
      "href",
      "/companies/acme-aps/resultatopgorelse?year=2026",
    );
  });

  test("the Bank card drills into the Bank view", async () => {
    mockFetch(overviewRoute());
    renderDashboard();
    const bankCard = (
      await screen.findByRole("heading", { name: "Bank" })
    ).closest("a")!;
    expect(bankCard).toHaveAttribute("href", "/companies/acme-aps/bank?year=2026");
  });
});

// --------------------------------------------------------------------------
// #395 — "Sådan kommer du i gang" CTA on an empty fresh company
// --------------------------------------------------------------------------

describe("DashboardView — empty-state next-step CTA (#395)", () => {
  test("shows the 'Sådan kommer du i gang' card when the company is empty", async () => {
    mockFetch(
      overviewRoute({
        lastPostedDate: null,
        recentEntries: [],
        bank: { balance: 0, actualBalance: null, difference: null },
        profitAndLoss: {
          omsaetning: 0,
          udgifter: 0,
          resultat: 0,
          months: MONTHS_EMPTY,
        },
        receivables: { openCount: 0, openTotal: 0 },
      }),
    );
    renderDashboard();
    expect(
      await screen.findByRole("heading", { name: /Sådan kommer du i gang/ }),
    ).toBeInTheDocument();
    // Three concrete CTAs with the right routes.
    const ingest = screen.getByRole("link", { name: /Indlæs dit første bilag/ });
    expect(ingest).toHaveAttribute("href", "/companies/acme-aps/bilag");
    const bank = screen.getByRole("link", { name: /Importér bankudtog/ });
    expect(bank).toHaveAttribute("href", "/companies/acme-aps/bank");
    const invoice = screen.getByRole("link", { name: /Udsted din første faktura/ });
    expect(invoice).toHaveAttribute("href", "/companies/acme-aps/fakturaer");
  });

  test("the CTA disappears as soon as the first entry is booked", async () => {
    mockFetch(
      overviewRoute({
        lastPostedDate: "2026-03-01",
        recentEntries: [
          {
            id: 1,
            entryNo: "2026-0001",
            date: "2026-03-01",
            text: "Første salg",
            amount: 1000,
          },
        ],
      }),
    );
    renderDashboard();
    await screen.findByText("Første salg");
    expect(
      screen.queryByRole("heading", { name: /Sådan kommer du i gang/ }),
    ).not.toBeInTheDocument();
  });

  test("the CTA disappears as soon as a bank statement is imported", async () => {
    mockFetch(
      overviewRoute({
        lastPostedDate: null,
        recentEntries: [],
        // actualBalance != null ⇒ a statement has been imported.
        bank: { balance: 0, actualBalance: 12500, difference: -12500 },
      }),
    );
    renderDashboard();
    await screen.findByRole("heading", { name: "Acme ApS" });
    expect(
      screen.queryByRole("heading", { name: /Sådan kommer du i gang/ }),
    ).not.toBeInTheDocument();
  });

  test("an archived year never shows the empty-state CTA", async () => {
    mockFetch(
      overviewRoute({
        archived: true,
        archivedSource: "dinero",
        selectedYear: "2025",
        vat: null,
        lastPostedDate: null,
        recentEntries: [],
        bank: { balance: 0, actualBalance: null, difference: null },
      }),
    );
    renderDashboard();
    await screen.findByText(/Arkiveret regnskabsår 2025/);
    expect(
      screen.queryByRole("heading", { name: /Sådan kommer du i gang/ }),
    ).not.toBeInTheDocument();
  });
});

const MONTHS_EMPTY = [
  { month: 1, label: "jan", income: 0, expense: 0 },
  { month: 2, label: "feb", income: 0, expense: 0 },
  { month: 3, label: "mar", income: 0, expense: 0 },
  { month: 4, label: "apr", income: 0, expense: 0 },
  { month: 5, label: "maj", income: 0, expense: 0 },
  { month: 6, label: "jun", income: 0, expense: 0 },
  { month: 7, label: "jul", income: 0, expense: 0 },
  { month: 8, label: "aug", income: 0, expense: 0 },
  { month: 9, label: "sep", income: 0, expense: 0 },
  { month: 10, label: "okt", income: 0, expense: 0 },
  { month: 11, label: "nov", income: 0, expense: 0 },
  { month: 12, label: "dec", income: 0, expense: 0 },
];

// --------------------------------------------------------------------------
// #213 slice 1 — the "Løs" write action on open exceptions
// --------------------------------------------------------------------------

const ONE_EXCEPTION = {
  exceptions: {
    count: 1,
    rows: [
      {
        id: 7,
        type: "UNMATCHED_BANK_TRANSACTION",
        severity: "medium",
        message: "Banktransaktion 12 mangler afstemning",
        requiredAction:
          "Find bilaget for indbetalingen på 2.500,00 kr. og bogfør den som indtægt.",
      },
    ],
    groups: [
      {
        type: "UNMATCHED_BANK_TRANSACTION",
        count: 1,
        severity: "medium" as const,
        label: "1 banktransaktion mangler afstemning",
        link: "bank",
      },
    ],
  },
};

const REVIEW_BUTTON = "Markér som gennemgået";
const REVIEW_DIALOG = "Markér opgave som gennemgået";

describe("DashboardView — resolve exception (#213)", () => {
  test("each open exception row carries a review action", async () => {
    mockFetch(overviewRoute(ONE_EXCEPTION));
    renderDashboard();
    expect(
      await screen.findByText("Banktransaktion 12 mangler afstemning"),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: REVIEW_BUTTON }),
    ).toBeInTheDocument();
  });

  test("each exception row shows its requiredAction guidance (#254)", async () => {
    mockFetch(overviewRoute(ONE_EXCEPTION));
    renderDashboard();
    expect(
      await screen.findByText(/Sådan løser du den:/),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        /Find bilaget for indbetalingen på 2\.500,00 kr\./,
      ),
    ).toBeInTheDocument();
  });

  test("the confirm modal makes clear that nothing is booked (#253)", async () => {
    mockFetch(overviewRoute(ONE_EXCEPTION));
    renderDashboard();
    await userEvent.click(
      await screen.findByRole("button", { name: REVIEW_BUTTON }),
    );
    expect(
      screen.getByRole("dialog", { name: REVIEW_DIALOG }),
    ).toBeInTheDocument();
    // The dialog must state, unmistakably, that resolving books nothing.
    expect(
      screen.getByText(/dette bogfører ikke noget/),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/skal stadig bogføres/),
    ).toBeInTheDocument();
    // The note field is present for recording what was actually booked.
    expect(
      screen.getByText(/hvad blev bogført/),
    ).toBeInTheDocument();
  });

  test("confirming the modal POSTs the resolve and reloads the overview", async () => {
    mockFetch({
      "GET /api/companies/acme-aps/overview": { overview: overview(ONE_EXCEPTION) },
      "POST /api/companies/acme-aps/exceptions/7/resolve": {
        exception: { id: 7, resolved: true },
      },
    });
    renderDashboard();
    await userEvent.click(
      await screen.findByRole("button", { name: REVIEW_BUTTON }),
    );
    const dialog = screen.getByRole("dialog", { name: REVIEW_DIALOG });
    await userEvent.click(
      within(dialog).getByRole("button", { name: REVIEW_BUTTON }),
    );
    // The resolve endpoint was called.
    const calls = (globalThis.fetch as ReturnType<typeof vi.fn>).mock.calls;
    const resolveCall = calls.find((c) =>
      String(c[0]).includes("/exceptions/7/resolve"),
    );
    expect(resolveCall).toBeDefined();
    expect((resolveCall![1] as RequestInit).method).toBe("POST");
    // The modal closes after success.
    await waitFor(() =>
      expect(
        screen.queryByRole("dialog", { name: REVIEW_DIALOG }),
      ).not.toBeInTheDocument(),
    );
  });

  test("a 409 backup-lock conflict is shown kindly inside the modal", async () => {
    mockFetch({
      "GET /api/companies/acme-aps/overview": { overview: overview(ONE_EXCEPTION) },
      "POST /api/companies/acme-aps/exceptions/7/resolve": {
        __error: {
          code: "conflict",
          message:
            "Bogføring er låst: en ugentlig backup er overskredet. Kør en backup.",
        },
      },
    });
    renderDashboard();
    await userEvent.click(
      await screen.findByRole("button", { name: REVIEW_BUTTON }),
    );
    const lockDialog = screen.getByRole("dialog", { name: REVIEW_DIALOG });
    await userEvent.click(
      within(lockDialog).getByRole("button", { name: REVIEW_BUTTON }),
    );
    expect(
      await screen.findByText("Bogføringen er låst"),
    ).toBeInTheDocument();
    // The modal stays open so the operator can read the lock message.
    expect(
      screen.getByRole("dialog", { name: REVIEW_DIALOG }),
    ).toBeInTheDocument();
  });

  test("an archived year offers no review action", async () => {
    mockFetch(
      overviewRoute({
        ...ONE_EXCEPTION,
        archived: true,
        archivedSource: "dinero",
        selectedYear: "2025",
        vat: null,
      }),
    );
    renderDashboard();
    await screen.findByText(/Arkiveret regnskabsår 2025/);
    expect(
      screen.queryByRole("button", { name: REVIEW_BUTTON }),
    ).not.toBeInTheDocument();
  });
});

// --------------------------------------------------------------------------
// #373 — Revisor-eksport is surfaced from Overblik, not only from Administrér
// --------------------------------------------------------------------------

describe("DashboardView — revisor-eksport discoverable (#373)", () => {
  test("shows the Revisor-eksport card on Overblik for a live year", async () => {
    mockFetch(overviewRoute());
    renderDashboard();
    // Wait for the page to load.
    await screen.findByRole("heading", { name: "Acme ApS" });
    // The card surfaces a clear heading and the "Generér og download" button.
    expect(
      screen.getByRole("heading", { name: /Revisor-eksport/i }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /Generér og download/i }),
    ).toBeInTheDocument();
  });

  test("an archived year does not offer the Revisor-eksport (live-only)", async () => {
    mockFetch(
      overviewRoute({
        archived: true,
        archivedSource: "dinero",
        selectedYear: "2025",
        vat: null,
      }),
    );
    renderDashboard();
    await screen.findByText(/Arkiveret regnskabsår 2025/);
    expect(
      screen.queryByRole("heading", { name: /Revisor-eksport/i }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /Generér og download/i }),
    ).not.toBeInTheDocument();
  });
});
