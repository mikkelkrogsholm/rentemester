import { describe, expect, test, vi } from "bun:test";
import userEvent from "@testing-library/user-event";
import { screen, within } from "@testing-library/react";
import { PeriodsView } from "./PeriodsView";
import { renderAt } from "../test/render";
import { mockFetch } from "../test/fixtures";

function payload(rows: any[] = []) {
  return {
    ok: true as const,
    periods: {
      slug: "acme-aps",
      company: {
        name: "Acme ApS",
        cvr: "DK12345678",
        country: "DK",
        currency: "DKK",
      },
      periods: rows,
      byStatus: {
        open: rows.filter((r) => r.effectiveStatus === "open").length,
        closed: rows.filter((r) => r.effectiveStatus === "closed").length,
        reported: rows.filter((r) => r.effectiveStatus === "reported").length,
      },
    },
  };
}

function renderView(body = payload()) {
  mockFetch({ "GET /api/companies/acme-aps/periods": body });
  return renderAt(<PeriodsView />, {
    route: "/companies/acme-aps/periodelas",
    path: "/companies/:slug/periodelas",
  });
}

describe("PeriodsView (#342)", () => {
  test("viser tom-state når der ikke er nogen perioder", async () => {
    renderView();
    expect(
      await screen.findByText(/Ingen lukkede perioder endnu/),
    ).toBeInTheDocument();
  });

  test("har en 'Luk periode'-knap i page-head", async () => {
    renderView();
    expect(
      await screen.findByRole("button", { name: /Luk periode/ }),
    ).toBeInTheDocument();
  });

  test("kan kontrollere før første packet og respekterer derefter blokeringer", async () => {
    mockFetch({
      "GET /api/companies/acme-aps/periods": payload(),
      "GET /api/companies/acme-aps/periods/close-readiness": {ok:true,packet:{hash:"synthetic",blockers:1,warnings:0,items:[{code:"SYNTHETIC_BLOCKER",status:"blocked",waivable:false,count:1}]}},
    });
    renderAt(<PeriodsView />, { route: "/companies/acme-aps/periodelas", path: "/companies/:slug/periodelas" });
    const user=userEvent.setup();
    await user.click(await screen.findByRole("button", {name:"Luk periode …"}));
    const dialog=within(screen.getByRole("dialog"));
    await user.type(dialog.getByLabelText("Start (YYYY-MM-DD)"), "2020-01-01");
    await user.type(dialog.getByLabelText("Slut (YYYY-MM-DD)"), "2020-12-31");
    expect(dialog.getByRole("button",{name:"Kontrollér"})).toBeEnabled();
    await user.click(dialog.getByRole("button",{name:"Kontrollér"}));
    expect(await dialog.findByText(/SYNTHETIC_BLOCKER/)).toBeInTheDocument();
    expect(dialog.getByRole("button",{name:"Gem review og luk"})).toBeDisabled();
    expect((fetch as unknown as ReturnType<typeof vi.fn>).mock.calls.filter(([,init])=>init?.method==="POST")).toHaveLength(0);
  });

  test("lister en lukket periode med effective status", async () => {
    renderView(
      payload([
        {
          id: 1,
          periodStart: "2026-01-01",
          periodEnd: "2026-03-31",
          kind: "vat_quarter",
          rowStatus: "closed",
          effectiveStatus: "closed",
          closedAt: "2026-04-15T12:00:00Z",
          closedBy: "user:owner",
          reference: "Q1 2026",
        },
      ]),
    );
    expect(await screen.findByText("2026-01-01")).toBeInTheDocument();
    expect(screen.getByText("2026-03-31")).toBeInTheDocument();
    expect(screen.getByText("Momsperiode")).toBeInTheDocument();
    expect(screen.getByText("Lukket")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /Genåbn/ }),
    ).toBeInTheDocument();
  });

  test("en indberettet periode kan ikke genåbnes", async () => {
    renderView(
      payload([
        {
          id: 2,
          periodStart: "2025-01-01",
          periodEnd: "2025-12-31",
          kind: "fiscal_year",
          rowStatus: "reported",
          effectiveStatus: "reported",
          closedAt: "2026-04-15T12:00:00Z",
          closedBy: "user:owner",
          reference: null,
        },
      ]),
    );
    // "Indberettet — kan ikke genåbnes"-labelet sidder i tabel-rækken;
    // intro-paragraffen nævner også at indberettede perioder ikke kan
    // genåbnes, så vi matcher det specifikke 'Indberettet —'-prefix.
    expect(
      await screen.findByText(/Indberettet — kan ikke genåbnes/),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /Genåbn/ }),
    ).not.toBeInTheDocument();
  });
  test("registrerer en ekstern kvittering mod det oprindelige lukkegrundlag", async () => {
    const body = payload([{ id: 1, periodStart: "2026-07-01", periodEnd: "2026-09-30", kind: "vat_period", rowStatus: "closed", effectiveStatus: "closed", closedAt: "2026-10-01", closedBy: "user:synthetic", reference: null, closePacketHash: "original-proof" }]);
    mockFetch({ "GET /api/companies/acme-aps/periods": { ...body, periods: { ...body.periods, reportingAvailable: true } }, "POST /api/companies/acme-aps/periods/report": { period: { status: "reported" } } });
    renderAt(<PeriodsView />, { route: "/companies/acme-aps/periodelas", path: "/companies/:slug/periodelas" });
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Registrér indberetning" }));
    const dialog = within(screen.getByRole("dialog"));
    await user.click(dialog.getByRole("button", { name: "Registrér indberetning" }));
    expect(await dialog.findByText("Angiv kvitteringsreferencen.")).toBeInTheDocument();
    expect((fetch as unknown as ReturnType<typeof vi.fn>).mock.calls.filter(([, init]) => init?.method === "POST")).toHaveLength(0);
    await user.type(dialog.getByLabelText("Kvitteringsreference"), "SYN-RECEIPT");
    await user.click(dialog.getByRole("button", { name: "Registrér indberetning" }));
    const call = (fetch as unknown as ReturnType<typeof vi.fn>).mock.calls.find(([, init]) => init?.method === "POST")!;
    expect(JSON.parse(call[1]!.body as string)).toEqual({ periodStart: "2026-07-01", periodEnd: "2026-09-30", kind: "vat_period", packetHash: "original-proof", reference: "SYN-RECEIPT", confirm: true });
  });

});
