import { describe, expect, test, vi } from "bun:test";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { GdprView } from "./GdprView";
import { renderAt } from "../test/render";
import { mockFetch } from "../test/fixtures";
import { stubGlobal } from "../test/globals";

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
  test("invalidates a reviewed subject on edit and erases only the newly reviewed query", async () => {
    const erasures: Array<Record<string, unknown>> = [];
    stubGlobal("fetch", vi.fn(async (url: RequestInfo | URL, init?: RequestInit) => {
      const query = JSON.parse(String(init?.body));
      if (String(url).endsWith("/erase")) {
        erasures.push(query);
        return new Response(JSON.stringify({ ok: true, gdprErasure: { ok: true, erasedCount: 1, refusedCount: 0, alreadyErasedCount: 0, erased: [], refused: [], errors: [], subject: { cvr: query.cvr, name: null }, asOf: "2026-01-01" } }), { headers: { "content-type": "application/json" } });
      }
      const payload = exportPayload([{ source: "vendors", sourceRowId: 1, label: "Synthetic subject", personalData: { vatOrCvr: query.cvr }, retainUntil: null, underRetention: false, erased: false, erasable: true }]);
      payload.gdpr.export.subject.cvr = query.cvr;
      return new Response(JSON.stringify(payload), { headers: { "content-type": "application/json" } });
    }));
    renderAt(<GdprView />, { route: "/companies/acme-aps/gdpr", path: "/companies/:slug/gdpr" });
    const cvr = screen.getByPlaceholderText("DK…");
    await userEvent.type(cvr, "DK11111111");
    await userEvent.click(screen.getByRole("button", { name: "Hent indsigtsrapport" }));
    await screen.findByRole("button", { name: /Anonymisér/ });
    await userEvent.clear(cvr);
    await userEvent.type(cvr, "DK22222222");
    expect(screen.queryByRole("button", { name: /Anonymisér/ })).not.toBeInTheDocument();
    expect(erasures).toHaveLength(0);
    await userEvent.click(screen.getByRole("button", { name: "Hent indsigtsrapport" }));
    await userEvent.click(await screen.findByRole("button", { name: /Anonymisér/ }));
    expect(screen.getByRole("dialog")).toHaveTextContent("DK22222222");
    await userEvent.click(screen.getByRole("button", { name: "Anonymisér nu" }));
    expect(erasures).toEqual([{ cvr: "DK22222222", confirm: true }]);
  });
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
    expect(screen.getByText("Kan ikke anonymiseres")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Anonymisér de 1 mulige rækker" }),
    ).toBeInTheDocument();
  });
});
