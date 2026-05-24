import { describe, expect, test } from "vitest";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ManageCompanyView } from "./ManageCompanyView";
import { renderAt } from "../test/render";
import { mockFetch, companySettings } from "../test/fixtures";

function companiesRoute(archived = false) {
  return {
    "GET /api/companies": {
      workspace: "/ws",
      count: 1,
      companies: [
        {
          slug: "acme-aps",
          name: "Acme ApS",
          createdAt: "2026-01-01T00:00:00.000Z",
          archived,
        },
      ],
    },
    "GET /api/companies/acme-aps/company": { company: companySettings() },
  };
}

describe("ManageCompanyView", () => {
  test("renders the rename form prefilled with the current name", async () => {
    mockFetch(companiesRoute());
    renderAt(<ManageCompanyView />, {
      route: "/companies/acme-aps/manage",
      path: "/companies/:slug/manage",
    });
    const input = (await screen.findByLabelText(
      /Visningsnavn/i,
    )) as HTMLInputElement;
    expect(input.value).toBe("Acme ApS");
  });

  test("PATCHes a new display name", async () => {
    mockFetch({
      ...companiesRoute(),
      "PATCH /api/companies/acme-aps": {
        company: { slug: "acme-aps", name: "Acme Holding ApS", archived: false },
      },
    });
    renderAt(<ManageCompanyView />, {
      route: "/companies/acme-aps/manage",
      path: "/companies/:slug/manage",
    });
    const input = await screen.findByLabelText(/Visningsnavn/i);
    await userEvent.clear(input);
    await userEvent.type(input, "Acme Holding ApS");
    await userEvent.click(screen.getByRole("button", { name: /Gem navn/i }));
    expect(await screen.findByText(/Visningsnavn opdateret/i)).toBeInTheDocument();
  });

  test("offers archive for an active company and restore for an archived one", async () => {
    mockFetch(companiesRoute(false));
    renderAt(<ManageCompanyView />, {
      route: "/companies/acme-aps/manage",
      path: "/companies/:slug/manage",
    });
    expect(
      await screen.findByRole("button", { name: /Arkivér virksomhed/i }),
    ).toBeInTheDocument();
  });

  test("404s for an unknown slug", async () => {
    mockFetch(companiesRoute());
    renderAt(<ManageCompanyView />, {
      route: "/companies/ghost/manage",
      path: "/companies/:slug/manage",
    });
    expect(await screen.findByRole("alert")).toHaveTextContent(/findes ikke/i);
  });

  test("syncs CVR stamdata and reports the updated fields", async () => {
    mockFetch({
      ...companiesRoute(),
      "GET /api/system/cvr-status": { cvrStatus: { configured: true } },
      "POST /api/companies/acme-aps/sync-cvr": {
        sync: {
          ok: true,
          cvr: "12345678",
          updatedFields: ["address", "city"],
          fiscalYearStartMonth: { current: 1, cvr: 1, matches: true },
          errors: [],
        },
      },
    });
    renderAt(<ManageCompanyView />, {
      route: "/companies/acme-aps/manage",
      path: "/companies/:slug/manage",
    });
    await userEvent.click(
      await screen.findByRole("button", { name: /Hent fra CVR/i }),
    );
    expect(
      await screen.findByText(/Opdaterede felter: address, city/i),
    ).toBeInTheDocument();
  });

  // #402 — when the server has no CVR-login the owner must see a friendly
  // explanation up front; the "Hent fra CVR" button must be disabled instead
  // of failing silently when clicked. The hint must avoid "miljøvariabel"
  // and similar developer-speak.
  test("disables Hent fra CVR and explains in plain Danish when CVR-login is missing", async () => {
    mockFetch({
      ...companiesRoute(),
      "GET /api/system/cvr-status": { cvrStatus: { configured: false } },
    });
    renderAt(<ManageCompanyView />, {
      route: "/companies/acme-aps/manage",
      path: "/companies/:slug/manage",
    });
    const button = (await screen.findByRole("button", {
      name: /Hent fra CVR/i,
    })) as HTMLButtonElement;
    // The status fetch resolves on the microtask queue — wait for the warning
    // banner before asserting the button is disabled.
    expect(
      await screen.findByText(/Cockpittet mangler dit virk.dk-login/i),
    ).toBeInTheDocument();
    expect(button.disabled).toBe(true);
    // The owner-facing hint must speak the owner's language.
    expect(
      screen.getByText(/Kræver dit virk\.dk-login/i),
    ).toBeInTheDocument();
    expect(
      screen.queryByText(/CVR_USERNAME/i),
    ).not.toBeInTheDocument();
    expect(screen.queryByText(/miljøvariabel/i)).not.toBeInTheDocument();
  });

  // #284 — the Cockpit owner must be able to set bank/payment details so
  // their invoices carry payment instructions.
  test("shows the profile / bank section", async () => {
    mockFetch(companiesRoute());
    renderAt(<ManageCompanyView />, {
      route: "/companies/acme-aps/manage",
      path: "/companies/:slug/manage",
    });
    expect(
      await screen.findByLabelText(/Kontonummer/i),
    ).toBeInTheDocument();
    expect(screen.getByLabelText(/Registreringsnummer/i)).toBeInTheDocument();
  });

  test("PATCHes the company profile with bank details", async () => {
    mockFetch({
      ...companiesRoute(),
      "PATCH /api/companies/acme-aps/company": {
        company: companySettings({
          payment: {
            bankName: "Danske Bank",
            registrationNo: "1234",
            accountNo: "0001234567",
            iban: null,
          },
        }),
      },
    });
    renderAt(<ManageCompanyView />, {
      route: "/companies/acme-aps/manage",
      path: "/companies/:slug/manage",
    });
    await userEvent.type(
      await screen.findByLabelText(/Registreringsnummer/i),
      "1234",
    );
    await userEvent.type(
      screen.getByLabelText(/Kontonummer/i),
      "0001234567",
    );
    await userEvent.click(
      screen.getByRole("button", { name: /Gem stamdata/i }),
    );
    expect(
      await screen.findByText(/Stamdata opdateret/i),
    ).toBeInTheDocument();
  });

  // #300 — the VAT settlement cadence is editable from the cockpit profile.
  test("the profile form shows the company's VAT cadence", async () => {
    mockFetch({
      ...companiesRoute(),
      "GET /api/companies/acme-aps/company": {
        company: companySettings({ vatPeriodType: "half-year" }),
      },
    });
    renderAt(<ManageCompanyView />, {
      route: "/companies/acme-aps/manage",
      path: "/companies/:slug/manage",
    });
    const select = (await screen.findByLabelText(
      /Momsperiode/i,
    )) as HTMLSelectElement;
    expect(select.value).toBe("half-year");
  });

  test("PATCHes the company profile with a changed VAT cadence", async () => {
    mockFetch({
      ...companiesRoute(),
      "PATCH /api/companies/acme-aps/company": {
        company: companySettings({ vatPeriodType: "month" }),
      },
    });
    renderAt(<ManageCompanyView />, {
      route: "/companies/acme-aps/manage",
      path: "/companies/:slug/manage",
    });
    await userEvent.selectOptions(
      await screen.findByLabelText(/Momsperiode/i),
      "month",
    );
    await userEvent.click(
      screen.getByRole("button", { name: /Gem stamdata/i }),
    );
    expect(
      await screen.findByText(/Stamdata opdateret/i),
    ).toBeInTheDocument();
    // The form reflects the persisted cadence after the round-trip.
    expect(
      (screen.getByLabelText(/Momsperiode/i) as HTMLSelectElement).value,
    ).toBe("month");
  });

  test("warns when no payment details are configured", async () => {
    mockFetch(companiesRoute());
    renderAt(<ManageCompanyView />, {
      route: "/companies/acme-aps/manage",
      path: "/companies/:slug/manage",
    });
    expect(
      await screen.findByText(/uden betalingsoplysninger/i),
    ).toBeInTheDocument();
  });
});
