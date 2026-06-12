import { describe, expect, test } from "vitest";
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
    "GET /api/companies/acme-aps/gdpr/export": record,
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
      name: /Hent indsigtsrapport/,
    });
    expect(submit).toBeDisabled();
  });

  test("søgning udfylder indsigtsrapport-panelet", async () => {
    const user = userEvent.setup();
    renderView();
    const cvr = await screen.findByPlaceholderText("DK…");
    await user.type(cvr, "DK99999999");
    await user.click(
      screen.getByRole("button", { name: /Hent indsigtsrapport/ }),
    );
    expect(
      await screen.findByText(/Ingen personoplysninger fundet/),
    ).toBeInTheDocument();
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
        },
      ]),
    );
    const cvr = await screen.findByPlaceholderText("DK…");
    await user.type(cvr, "DK99999999");
    await user.click(
      screen.getByRole("button", { name: /Hent indsigtsrapport/ }),
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
  });
});
