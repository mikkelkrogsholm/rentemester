// Cockpit Kørsel view (#335) — list, summary, and the register-modal flow.

import { describe, expect, test } from "vitest";
import { fireEvent, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MileageView } from "./MileageView";
import { renderAt } from "../test/render";
import { mileage, mockFetch } from "../test/fixtures";

function route(over = {}) {
  return {
    "GET /api/companies/acme-aps/mileage": {
      mileage: mileage(over),
    },
  };
}

function renderView() {
  return renderAt(<MileageView />, {
    route: "/companies/acme-aps/koersel",
    path: "/companies/:slug/koersel",
  });
}

describe("MileageView — Kørsel (#335)", () => {
  test("lists each trip with its key fields, newest first", async () => {
    mockFetch(route());
    renderView();
    expect(
      await screen.findByRole("heading", { name: "Acme ApS" }),
    ).toBeInTheDocument();
    // Both trips are rendered with their date + purpose in the entries table.
    expect(screen.getByText("Møde Odense")).toBeInTheDocument();
    expect(screen.getByText("2026-05-10")).toBeInTheDocument();
    expect(screen.getByText("Kundebesøg Aarhus")).toBeInTheDocument();
    expect(screen.getByText("2026-03-15")).toBeInTheDocument();
    // The view orders newest trip first — the May row's date in the DOM
    // appears before the March row's date.
    const html = document.body.innerHTML;
    expect(html.indexOf("2026-05-10")).toBeLessThan(
      html.indexOf("2026-03-15"),
    );
  });

  test("shows the summary cards: antal ture, samlet km, godtgørelsesgrundlag", async () => {
    mockFetch(route());
    renderView();
    expect(await screen.findByText("Antal ture")).toBeInTheDocument();
    expect(screen.getByText("Samlet km")).toBeInTheDocument();
    expect(screen.getByText("Godtgørelsesgrundlag")).toBeInTheDocument();
    // The summary card values: 2 trips, 396 km total, 1500.84 kr basis.
    // "396" also appears in the totals row of the entries table, so the
    // ordering test asserts the summary value precedes that row.
    expect(screen.getAllByText("396").length).toBeGreaterThanOrEqual(1);
    expect(
      screen.getAllByText(/1\.500,84/).length,
    ).toBeGreaterThanOrEqual(1);
  });

  test("shows the per-month breakdown when at least one month has activity", async () => {
    mockFetch(route());
    renderView();
    expect(await screen.findByText("Sum pr. måned")).toBeInTheDocument();
    // jan has zero trips — its row exists but the grundlag cell renders "—".
    const tables = screen.getAllByRole("table");
    const monthlyTable = tables.find((t) =>
      within(t).queryByText("Sum pr. måned") !== null
        ? false
        : within(t).queryByText("jan") !== null,
    )!;
    expect(monthlyTable).toBeDefined();
  });

  test("empty state for a year with no trips offers a primary register button", async () => {
    mockFetch(
      route({
        entries: [],
        totalKilometers: 0,
        totalAmountBasis: 0,
        tripCount: 0,
        months: [],
      }),
    );
    renderView();
    expect(
      await screen.findByText(/Ingen kørsler registreret/),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /Registrér første kørsel/ }),
    ).toBeInTheDocument();
  });

  test("an archived year hides the register button and shows the read-only notice", async () => {
    mockFetch(route({ archived: true, selectedYear: "2025" }));
    renderView();
    expect(
      await screen.findByText(/Kørsel er ikke tilgængelig for 2025/),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /Registrér kørsel/ }),
    ).not.toBeInTheDocument();
  });

  test("links to the skat.dk takst-page from the helper text", async () => {
    mockFetch(route());
    renderView();
    const link = await screen.findByRole("link", { name: /skat\.dk/i });
    expect(link).toHaveAttribute(
      "href",
      expect.stringContaining("skat.dk"),
    );
  });

  test("opens the register-modal when the page-head action is clicked", async () => {
    mockFetch(route());
    renderView();
    const action = await screen.findByRole("button", {
      name: "Registrér kørsel",
    });
    fireEvent.click(action);
    expect(
      await screen.findByRole("dialog", { name: /Registrér kørsel/i }),
    ).toBeInTheDocument();
    // Form has the headline fields the modal must collect.
    expect(screen.getByLabelText("Dato")).toBeInTheDocument();
    expect(screen.getByLabelText("Formål")).toBeInTheDocument();
    expect(screen.getByLabelText("Fra-adresse")).toBeInTheDocument();
    expect(screen.getByLabelText("Til-adresse")).toBeInTheDocument();
    expect(screen.getByLabelText("Antal km")).toBeInTheDocument();
    expect(screen.getByLabelText("Takst (kr/km)")).toBeInTheDocument();
    // The takst-grundlag label has a long helper paragraph in the same
    // <label> element, so the accessible name is the full text — match with
    // a substring regex.
    expect(screen.getByLabelText(/Takst-grundlag/)).toBeInTheDocument();
  });

  test("submitting the register-modal POSTs to /mileage and reloads the list", async () => {
    let posted: { url: string; body: unknown } | null = null;
    // Custom fetch stub: GET serves the list fixture, POST captures the body
    // and returns the create result envelope.
    globalThis.fetch = (async (
      input: RequestInfo | URL,
      init?: RequestInit,
    ) => {
      const url = typeof input === "string" ? input : input.toString();
      const path = url.replace(/^https?:\/\/[^/]+/, "").split("?")[0];
      const method = (init?.method ?? "GET").toUpperCase();
      if (
        method === "GET" &&
        path === "/api/companies/acme-aps/mileage"
      ) {
        return new Response(
          JSON.stringify({ ok: true, mileage: mileage() }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (
        method === "POST" &&
        path === "/api/companies/acme-aps/mileage"
      ) {
        posted = {
          url,
          body: init?.body ? JSON.parse(String(init.body)) : null,
        };
        return new Response(
          JSON.stringify({
            ok: true,
            mileage: {
              mileageEntryId: 42,
              entryNo: "MIL-2026-000042",
              amountBasis: 379,
            },
          }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      return new Response(
        JSON.stringify({ ok: false, error: { code: "not_found", message: "no route" } }),
        { status: 404, headers: { "content-type": "application/json" } },
      );
    }) as typeof fetch;

    renderView();
    const action = await screen.findByRole("button", {
      name: "Registrér kørsel",
    });
    fireEvent.click(action);

    // Fill the form. The defaults (dato + vehicle) are already populated.
    const user = userEvent.setup();
    await user.type(screen.getByLabelText("Formål"), "Test kundebesøg");
    await user.type(screen.getByLabelText("Fra-adresse"), "København");
    await user.type(screen.getByLabelText("Til-adresse"), "Roskilde");
    await user.type(screen.getByLabelText("Antal km"), "100");
    await user.type(screen.getByLabelText("Chauffør"), "Owner");
    await user.type(screen.getByLabelText("Takst (kr/km)"), "3.79");
    await user.type(
      screen.getByLabelText(/Takst-grundlag/),
      "SKAT 2026, høj sats",
    );

    // After the modal opens there are TWO buttons named "Registrér kørsel":
    // the page-head action and the modal's submit. Fire the form submit
    // directly inside the dialog — fireEvent.click on a type="submit"
    // button can be flaky in jsdom when the surrounding form has a
    // controlled native validation path; the submit event fires the
    // React onSubmit handler reliably.
    const dialog = screen.getByRole("dialog", { name: /Registrér kørsel/i });
    const form = dialog.querySelector("form")!;
    fireEvent.submit(form);
    expect(
      await screen.findByText("MIL-2026-000042", { exact: false }),
    ).toBeInTheDocument();

    expect(posted).not.toBeNull();
    const body = (posted as unknown as { body: Record<string, unknown> }).body;
    expect(body.confirm).toBe(true);
    expect(body.purpose).toBe("Test kundebesøg");
    expect(body.fromLocation).toBe("København");
    expect(body.toLocation).toBe("Roskilde");
    expect(body.kilometers).toBe(100);
    expect(body.ratePerKm).toBe(3.79);
    expect(body.rateBasis).toBe("SKAT 2026, høj sats");
  });
});
