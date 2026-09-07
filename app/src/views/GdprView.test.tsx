import { describe, expect, test } from "bun:test";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { GdprView } from "./GdprView";
import { renderAt } from "../test/render";
import { mockFetch } from "../test/fixtures";

function exportPayload(records: any[] = []) {
  return {
    ok: true as const,
    gdpr: {
      slug: "acme-aps",
      company: {
        name: "Acme ApS",
        cvr: "DK12345678",
        country: "DK",
        currency: "DKK",
      },
      export: {
        ok: true,
        asOf: "2026-05-25",
        appliedRules: ["DK-GDPR-ART15-001"],
        subject: { cvr: "DK99999999", name: null },
        records,
        errors: [],
      },
    },
  };
}

function renderView(record = exportPayload()) {
  mockFetch({
    "POST /api/companies/acme-aps/gdpr/export": record,
  });
  return renderAt(<GdprView />, {
    route: "/companies/acme-aps/gdpr",
    path: "/companies/:slug/gdpr",
  });
}

describe("GdprView (#334)", () => {
  test("kræver mindst ét felt før knappen er klikbar", async () => {
    renderView();
    const submit = await screen.findByRole("button", {
      name: /^Find oplysninger$/,
    });
    expect(submit).toBeDisabled();
  });

  test("søgning udfylder indsigtsrapport-panelet", async () => {
    const user = userEvent.setup();
    renderView();
    const cvr = await screen.findByPlaceholderText("DK…");
    await user.type(cvr, "DK99999999");
    await user.click(
      screen.getByRole("button", { name: /^Find oplysninger$/ }),
    );
    expect(
      await screen.findByText(/Ingen personoplysninger fundet/),
    ).toBeInTheDocument();
    expect(fetch).toHaveBeenCalledWith(
      "/api/companies/acme-aps/gdpr/export",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({ cvr: "DK99999999", confirm: true }),
      }),
    );
  });

  test("rapport-panel viser records med under-retention-status", async () => {
    const user = userEvent.setup();
    renderView(
      exportPayload([
        {
          source: "customers",
          sourceRowId: 1,
          label: "Acme Kunde",
          personalData: {
            name: "Acme Kunde",
            address: "Vej 1",
            email: "test@example.com",
            vatOrCvr: "DK99999999",
          },
          retainUntil: "2031-12-31",
          underRetention: true,
          erased: false,
          erasable: false,
        },
        {
          source: "vendors",
          sourceRowId: 2,
          label: "Acme Leverandør",
          personalData: {
            name: "Acme Leverandør",
            address: null,
            email: null,
            vatOrCvr: "DK99999999",
          },
          retainUntil: null,
          underRetention: false,
          erased: false,
          erasable: true,
        },
        {
          source: "journal_entries",
          sourceRowId: 3,
          label: "2020-0001",
          personalData: {
            name: "Historisk journaltekst",
            address: null,
            email: null,
            vatOrCvr: null,
          },
          retainUntil: "2025-12-31",
          underRetention: false,
          erased: false,
          erasable: false,
        },
      ]),
    );
    const cvr = await screen.findByPlaceholderText("DK…");
    await user.type(cvr, "DK99999999");
    await user.click(
      screen.getByRole("button", { name: /^Find oplysninger$/ }),
    );
    expect(
      await screen.findByText("Acme Kunde"),
    ).toBeInTheDocument();
    expect(screen.getByText("Acme Leverandør")).toBeInTheDocument();
    // "Under bogføringspligt" og "Kan anonymiseres" optræder både i
    // summary-tekst og som status-pill — tjek mindst én forekomst.
    expect(
      screen.getAllByText(/Under bogføringspligt/).length,
    ).toBeGreaterThan(0);
    expect(
      screen.getAllByText(/Kan anonymiseres/).length,
    ).toBeGreaterThan(0);
    expect(screen.getByText("Kan ikke anonymiseres")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Anonymisér de 1 mulige rækker" }),
    ).toBeInTheDocument();
    expect(screen.getByText(/Konsekvens: Tilladte personoplysninger/)).toBeInTheDocument();
  });

  test("anonymiserer ikke, når bekræftelsen annulleres", async () => {
    const user = userEvent.setup();
    renderView(exportPayload([{
      source: "customers", sourceRowId: 2, label: "Sikker Kunde",
      personalData: { name: "Sikker Kunde", address: null, email: null, vatOrCvr: "DK99999999" },
      retainUntil: null, underRetention: false, erased: false, erasable: true,
    }]));
    await user.type(await screen.findByPlaceholderText("DK…"), "DK99999999");
    await user.click(screen.getByRole("button", { name: /^Find oplysninger$/ }));
    await user.click(await screen.findByRole("button", { name: /Anonymisér de 1 mulige rækker/ }));
    expect(screen.getByRole("dialog", { name: /Bekræft anonymisering/ })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Annullér" }));
    const calls = (fetch as unknown as { mock: { calls: Array<[string, RequestInit]> } }).mock.calls;
    expect(calls.some(([url, init]) => url.includes("/gdpr/erase") && init?.method === "POST")).toBe(false);
  });
});
