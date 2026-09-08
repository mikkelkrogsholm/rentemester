import { describe, expect, test, vi } from "bun:test";
import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { VatView } from "./VatView";
import { renderAt } from "../test/render";
import { vat, vatNotRegistered, mockFetch } from "../test/fixtures";

function route(over = {}) {
  return {
    "GET /api/companies/acme-aps/vat": { vat: vat(over) },
    "GET /api/companies/acme-aps/periods/close-readiness": { packet: { hash: "a".repeat(64), blockers: 0, warnings: 0, items: [] } },
    "POST /api/companies/acme-aps/periods/close-review": { review: { id: 1, packet: { hash: "a".repeat(64), blockers: 0, warnings: 0, items: [] } } },
  };
}

function renderView() {
  return renderAt(<VatView />, {
    route: "/companies/acme-aps/moms",
    path: "/companies/:slug/moms",
  });
}

describe("VatView — Moms", () => {
  test("keeps the close-readiness summary as the natural keyboard core action", async () => {
    mockFetch(route());
    renderView();

    const summary = await screen.findByText("Gennemgå momsparathed");
    const details = summary.closest("details")!;
    expect(summary.tagName).toBe("SUMMARY");
    expect(summary).toHaveAttribute("data-evidence-core-action");
    expect(details).toHaveAttribute("data-evidence-progressive");
    expect(details).not.toHaveAttribute("data-evidence-core-action");
    expect(details).not.toHaveAttribute("open");

    const user = userEvent.setup();
    for (let tabs = 0; tabs < 20 && document.activeElement !== summary; tabs++)
      await user.tab();
    expect(document.activeElement).toBe(summary);

    await user.click(summary);
    expect((details as HTMLDetailsElement).open).toBe(true);
    expect(screen.getByText("Se lukkegrundlag")).toBeVisible();
  });

  test("shows the output, input and payable VAT figures", async () => {
    mockFetch(route());
    renderView();
    expect(
      await screen.findByRole("heading", { name: "Acme ApS" }),
    ).toBeInTheDocument();
    expect(
      screen.getByText("Udgående moms før tab (kontrol)"),
    ).toBeInTheDocument();
    expect(
      screen.getByText("Købsmoms (indgående moms)"),
    ).toBeInTheDocument();
    const payable = (await screen.findByText("Moms at betale")).closest("tr")!;
    expect(
      within(payable as HTMLElement).getByText(/3\.371,00/),
    ).toBeInTheDocument();
  });

  test("shows the VAT period label", async () => {
    mockFetch(route());
    renderView();
    // The period label is surfaced in several places for an open period
    // (the sub-heading and the provisional-figures notes), so match all.
    expect(
      (await screen.findAllByText(/Q1 2026/)).length,
    ).toBeGreaterThan(0);
  });

  test("shows the full SKAT momsangivelse rubrics", async () => {
    mockFetch(route());
    renderView();
    expect(
      await screen.findByText(/SKAT-rubrikker/),
    ).toBeInTheDocument();
    // The foreign-trade rubrics the static figures lacked are now present.
    expect(screen.getByText("Salgsmoms")).toBeInTheDocument();
    expect(
      screen.getByText(/Rubrik A — varer købt i EU/),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/Rubrik B — varer \/ EU-salg uden moms/),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/Rubrik C — øvrige momsfrie salg/),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        /Moms af ydelseskøb i udlandet med omvendt betalingspligt/,
      ),
    ).toBeInTheDocument();
    // The statutory total row carries the filing figure.
    const tilsvar = screen.getByText("Moms i alt").closest("tr")!;
    expect(
      within(tilsvar as HTMLElement).getByText(/3\.371,00/),
    ).toBeInTheDocument();
  });

  test("an archived year shows an honest 'not available' state", async () => {
    mockFetch(route({ archived: true, selectedYear: "2025" }));
    renderView();
    expect(
      await screen.findByText(/Moms er ikke tilgængelig for 2025/),
    ).toBeInTheDocument();
  });

  // A NOT VAT-registered company renders the explanation card instead of the
  // period/rubrikker block — and must render WITHOUT crashing. Regression
  // guard: the not-registered branch previously mis-wired CompanyNav (years
  // undefined → YearSelector .map threw) and used an invalid Banner kind.
  test("a not-VAT-registered company shows the explanation card and renders the year selector", async () => {
    mockFetch({
      "GET /api/companies/acme-aps/vat": { vat: vatNotRegistered() },
    });
    renderView();
    expect(
      await screen.findByText(/ikke momsregistreret/i),
    ).toBeInTheDocument();
    // The CompanyNav year selector must be present — it threw on undefined
    // `years` before the fix, crashing the whole view.
    expect(
      screen.getByLabelText("Vælg regnskabsår"),
    ).toBeInTheDocument();
    // No synthetic momsangivelse rubrics for a non-registered company.
    expect(screen.queryByText("Moms at betale")).not.toBeInTheDocument();
  });

  // #271: a bad-debt write-off books a debit on the output-VAT account. The
  // VAT card must surface that relief on its own clearly-labelled line —
  // never let it drag the output-VAT control headline negative.
  test("a bad-debt adjustment is its own line, output-VAT control stays positive", async () => {
    mockFetch(
      route({
        outputVat: 250,
        outputVatAdjustment: -300,
        inputVat: 100,
        payable: -150,
      }),
    );
    renderView();
    // The gross output-VAT control remains positive before the relief.
    const salgsmomsRow = (
      await screen.findByText("Udgående moms før tab (kontrol)")
    ).closest("tr")!;
    expect(
      within(salgsmomsRow as HTMLElement).getByText(/250,00/),
    ).toBeInTheDocument();
    // It is NOT shown as a confusing negative output-VAT control amount.
    expect(
      within(salgsmomsRow as HTMLElement).queryByText(/-250,00/),
    ).not.toBeInTheDocument();
    // The bad-debt relief sits on its own dedicated line.
    const adjustmentRow = screen
      .getByText(/Regulering for tab på debitorer/)
      .closest("tr")!;
    expect(
      within(adjustmentRow as HTMLElement).getByText(/-300,00/),
    ).toBeInTheDocument();
  });

  test("no adjustment line is shown when there is no bad-debt write-off", async () => {
    mockFetch(route({ outputVatAdjustment: 0 }));
    renderView();
    await screen.findByText("Udgående moms før tab (kontrol)");
    expect(
      screen.queryByText(/Regulering for tab på debitorer/),
    ).not.toBeInTheDocument();
  });

  // #287: a momsangivelse requires a CLOSED vat_quarter period. The VAT view
  // must offer a "close period" action so the owner can finish a VAT return
  // entirely from the Cockpit.
  test("offers a close-period action", async () => {
    mockFetch(route());
    renderView();
    expect(
      await screen.findByRole("button", { name: /Luk momsperiode/i }),
    ).toBeInTheDocument();
  });

  test("shows VAT integrity errors and does not offer to lock the period", async () => {
    mockFetch(route({
      vatReportErrors: ["Journalpost 17 mangler momskode på grundlinjen."],
      momsangivelseReady: false,
    }));
    renderView();
    expect(
      await screen.findByText(/Journalpost 17 mangler momskode/),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /Luk momsperiode/i }),
    ).not.toBeInTheDocument();
  });

  test("surfaces canonical VAT report warnings before filing", async () => {
    mockFetch(route({
      vatReportWarnings: ["Historisk momskorrektion kræver kontrol."],
    }));
    renderView();
    expect(
      await screen.findByText(/Historisk momskorrektion kræver kontrol/),
    ).toBeInTheDocument();
  });

  test("closes the VAT period and confirms it", async () => {
    mockFetch({
      ...route(),
      "POST /api/companies/acme-aps/periods/close": {
        period: {
          id: 1,
          periodStart: "2026-01-01",
          periodEnd: "2026-03-31",
          kind: "vat_quarter",
          status: "closed",
          reference: null,
        },
      },
    });
    renderView();
    await userEvent.click(
      await screen.findByRole("button", { name: /Luk momsperiode/i }),
    );
    // A confirm step guards the irreversible close.
    await userEvent.click(
      await screen.findByRole("button", { name: /Luk perioden/i }),
    );
    expect(
      await screen.findByText(/Momsperioden er lukket/i),
    ).toBeInTheDocument();
  });

  test("a backup-lock 409 on close is shown kindly", async () => {
    mockFetch({
      ...route(),
      "POST /api/companies/acme-aps/periods/close": {
        __error: { code: "conflict", message: "Bogføring er låst: backup mangler." },
      },
    });
    renderView();
    await userEvent.click(
      await screen.findByRole("button", { name: /Luk momsperiode/i }),
    );
    await userEvent.click(
      await screen.findByRole("button", { name: /Luk perioden/i }),
    );
    expect(
      await screen.findByText(/Bogføring er låst/i),
    ).toBeInTheDocument();
  });

  // #303: an OPEN VAT period's figures are provisional — the cockpit must say
  // so honestly rather than presenting a ready-to-file momsangivelse.
  test("an open period is marked provisional, not filing-ready", async () => {
    mockFetch(route({ periodStatus: "open", momsangivelseReady: false }));
    renderView();
    expect(
      await screen.findByText(/Åben periode — foreløbige tal/i),
    ).toBeInTheDocument();
    // The rubrics heading is flagged provisional.
    expect(
      screen.getByText(/SKAT-rubrikker \(foreløbige — åben periode\)/i),
    ).toBeInTheDocument();
  });

  // #303: a closed period's figures ARE final — no provisional banner, and the
  // rubrics carry the normal "ready momsangivelse" heading.
  test("a closed period shows final figures with no provisional banner", async () => {
    mockFetch(route({ periodStatus: "closed", momsangivelseReady: true }));
    renderView();
    await screen.findByText(/SKAT-rubrikker \(momsangivelse\)/i);
    expect(
      screen.queryByText(/Åben periode — foreløbige tal/i),
    ).not.toBeInTheDocument();
    // A closed period offers no "close" action — it is already closed.
    expect(
      screen.queryByRole("button", { name: /Luk momsperiode/i }),
    ).not.toBeInTheDocument();
  });

  // #413: the rubrics explanation must not leak CLI jargon ("vat momsangivelse",
  // "i terminalen") to the SMB owner. The cockpit is the authoritative surface;
  // referring to a terminal command undermines trust and confuses non-technical
  // owners. Both the open- and closed-period notes must be CLI-free.
  test("rubrics explanation does not leak CLI jargon to the SMB owner (closed period)", async () => {
    mockFetch(route({ periodStatus: "closed", momsangivelseReady: true }));
    renderView();
    const heading = await screen.findByText(
      /SKAT-rubrikker \(momsangivelse\)/i,
    );
    const card = heading.closest(".statement-card") as HTMLElement;
    expect(card).not.toBeNull();
    expect(card.textContent ?? "").not.toMatch(/vat momsangivelse/i);
    expect(card.textContent ?? "").not.toMatch(/i terminalen/i);
    // The replacement text reassures the owner without jargon.
    expect(card.textContent ?? "").toMatch(/TastSelv-formens rækkefølge/i);
    expect(card.textContent ?? "").toMatch(/hele kroner/i);
  });

  test("rubrics explanation does not leak CLI jargon (open period)", async () => {
    mockFetch(route({ periodStatus: "open", momsangivelseReady: false }));
    renderView();
    const heading = await screen.findByText(
      /SKAT-rubrikker \(foreløbige — åben periode\)/i,
    );
    const card = heading.closest(".statement-card") as HTMLElement;
    expect(card).not.toBeNull();
    expect(card.textContent ?? "").not.toMatch(/vat momsangivelse/i);
    expect(card.textContent ?? "").not.toMatch(/i terminalen/i);
    // The replacement text explains provisional-vs-final without jargon.
    expect(card.textContent ?? "").toMatch(/foreløbige/i);
    expect(card.textContent ?? "").toMatch(/lukket|endelige/i);
  });

  // #301: a closed period can be reopened from the cockpit — the controlled,
  // audit-logged recovery path for a period closed too early.
  test("offers a reopen action for a closed period and reopens it", async () => {
    mockFetch({
      ...route({ periodStatus: "closed", momsangivelseReady: true }),
      "POST /api/companies/acme-aps/periods/reopen": {
        period: {
          id: 1,
          periodStart: "2026-01-01",
          periodEnd: "2026-03-31",
          kind: "vat_quarter",
          effectiveStatus: "open",
          reopenedBy: "user:test",
          reason: "bilag bogført for sent",
        },
      },
    });
    renderView();
    await userEvent.click(
      await screen.findByRole("button", { name: /Genåbn momsperiode/i }),
    );
    // The reopen requires a free-text reason recorded in the audit log.
    const reason = await screen.findByRole("textbox");
    await userEvent.type(reason, "bilag bogført for sent");
    await userEvent.click(
      await screen.findByRole("button", { name: /Genåbn perioden/i }),
    );
    expect(
      await screen.findByText(/er genåbnet/i),
    ).toBeInTheDocument();
  });

  // #301: a reopen with no reason is blocked — a reopen must be traceable.
  test("a reopen with an empty reason is blocked", async () => {
    mockFetch(route({ periodStatus: "closed", momsangivelseReady: true }));
    renderView();
    await userEvent.click(
      await screen.findByRole("button", { name: /Genåbn momsperiode/i }),
    );
    await userEvent.click(
      await screen.findByRole("button", { name: /Genåbn perioden/i }),
    );
    expect(
      await screen.findByText(/Angiv en begrundelse/i),
    ).toBeInTheDocument();
  });

  // #301: closing a period whose end date is still in the future warns clearly
  // and requires an explicit second acknowledgement before the close goes
  // through — closing a not-yet-ended period is almost always a mistake.
  test("closing a not-yet-ended period warns and needs an acknowledgement", async () => {
    mockFetch({
      // A period that ends far in the future — it has not ended yet.
      ...route({ periodEnd: "2099-12-31" }),
      "POST /api/companies/acme-aps/periods/close": {
        period: {
          id: 1,
          periodStart: "2026-01-01",
          periodEnd: "2099-12-31",
          kind: "vat_quarter",
          status: "closed",
          reference: null,
        },
      },
    });
    renderView();
    await userEvent.click(
      await screen.findByRole("button", { name: /Luk momsperiode/i }),
    );
    // The dialog warns the period is not over yet.
    expect(
      await screen.findByText(/ikke afsluttet endnu/i),
    ).toBeInTheDocument();
    // Confirming WITHOUT ticking the acknowledgement is blocked.
    await userEvent.click(
      await screen.findByRole("button", { name: /Luk perioden/i }),
    );
    expect(
      await screen.findByText(/Bekræft først/i),
    ).toBeInTheDocument();
    // After ticking the acknowledgement the close goes through.
    await userEvent.click(screen.getByRole("checkbox"));
    await userEvent.click(
      screen.getByRole("button", { name: /Luk perioden/i }),
    );
    expect(
      await screen.findByText(/Momsperioden er lukket/i),
    ).toBeInTheDocument();
  });

  // #401: every rubric value needs a "Kopier"-button that copies ONLY the raw
  // number in TastSelv format (no thousand separators, no "kr."), so the owner
  // can paste it straight into TastSelv Erhverv without strip-formatting by
  // hand. The formatted text (with `.` separators) MUST NOT end on the
  // clipboard.
  test("rubrikker rows have a Kopier button that copies the raw TastSelv number", async () => {
    mockFetch(route({ periodStatus: "closed", momsangivelseReady: true }));
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
    renderView();
    // The filing Salgsmoms row's Kopier-button copies the raw TastSelv amount;
    // use the fixture's actual canonical projection below.
    const salgsmomsRow = (
      await screen.findByText(/^Salgsmoms$/)
    ).closest("tr")!;
    const copyBtn = within(salgsmomsRow as HTMLElement).getByRole("button", {
      name: /Kopier/i,
    });
    expect(copyBtn).not.toBeDisabled();
    await userEvent.click(copyBtn);
    expect(writeText).toHaveBeenCalledTimes(1);
    const pasted = writeText.mock.calls[0][0] as string;
    // The pasted text is the raw TastSelv number — no thousand separators,
    // no "kr." suffix. It may carry a decimal comma for øre, but never `.`.
    expect(pasted).not.toMatch(/\./);
    expect(pasted).not.toMatch(/kr/i);
    expect(pasted).toMatch(/^-?\d+(,\d{2})?$/);
    // Visual feedback after a successful copy.
    expect(
      await within(salgsmomsRow as HTMLElement).findByText(/Kopieret/i),
    ).toBeInTheDocument();
  });

  test("Kopier-knappen er deaktiveret for en åben (provisorisk) periode", async () => {
    mockFetch(route({ periodStatus: "open", momsangivelseReady: false }));
    renderView();
    const salgsmomsRow = (
      await screen.findByText(/^Salgsmoms$/)
    ).closest("tr")!;
    const copyBtn = within(salgsmomsRow as HTMLElement).getByRole("button", {
      name: /Kopier/i,
    });
    expect(copyBtn).toBeDisabled();
    // The disabled button explains why via its title attribute.
    expect(copyBtn).toHaveAttribute(
      "title",
      expect.stringMatching(/Periode ikke lukket|luk først/i),
    );
  });

  test("rubrikker-kortet har en 'Kopier alle som CSV'-knap der kopierer label;beløb-par", async () => {
    mockFetch(route({ periodStatus: "closed", momsangivelseReady: true }));
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
    renderView();
    const heading = await screen.findByText(
      /SKAT-rubrikker \(momsangivelse\)/i,
    );
    const card = heading.closest(".statement-card") as HTMLElement;
    const csvBtn = within(card).getByRole("button", {
      name: /Kopier alle som CSV/i,
    });
    await userEvent.click(csvBtn);
    expect(writeText).toHaveBeenCalledTimes(1);
    const pasted = writeText.mock.calls[0][0] as string;
    // Semicolon-separated label;beløb pairs, one per line — no thousand
    // separators, no "kr." suffix in the numeric column.
    expect(pasted).toMatch(/Salgsmoms;/);
    expect(pasted).toMatch(/Moms i alt;/);
    expect(pasted).toMatch(/Rubrik A/);
    // No tusindtalsseparator-punktum and no "kr." anywhere in the CSV.
    expect(pasted).not.toMatch(/\./);
    expect(pasted).not.toMatch(/kr/i);
  });
});
