import { describe, expect, test } from "vitest";
import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { AccountsView } from "./AccountsView";
import { renderAt } from "../test/render";
import { mockFetch } from "../test/fixtures";

const sample = {
  ok: true as const,
  accounts: {
    slug: "acme-aps",
    company: {
      name: "Acme ApS",
      cvr: "DK12345678",
      country: "DK",
      currency: "DKK",
    },
    accounts: [
      {
        accountNo: "1000",
        name: "Omsætning, ydelser",
        type: "income",
        normalBalance: "credit",
        defaultVatCode: null,
        hasPostings: true,
      },
      {
        accountNo: "2000",
        name: "Bank",
        type: "asset",
        normalBalance: "debit",
        defaultVatCode: null,
        hasPostings: false,
      },
      {
        accountNo: "3000",
        name: "Software og SaaS",
        type: "expense",
        normalBalance: "debit",
        defaultVatCode: "DK_PURCHASE_25",
        hasPostings: false,
      },
      {
        accountNo: "7200",
        name: "Skyldig skat (skattekonto)",
        type: "liability",
        normalBalance: "credit",
        defaultVatCode: null,
        hasPostings: false,
      },
    ],
    byType: {
      income: 1,
      asset: 1,
      expense: 1,
      liability: 1,
    },
  },
};

function renderView() {
  mockFetch({ "GET /api/companies/acme-aps/accounts": sample });
  return renderAt(<AccountsView />, {
    route: "/companies/acme-aps/kontoplan",
    path: "/companies/:slug/kontoplan",
  });
}

describe("AccountsView (#344)", () => {
  test("lister kontoplanen med nummer, navn, type og normal-saldo", async () => {
    renderView();
    expect(await screen.findByText("Omsætning, ydelser")).toBeInTheDocument();
    expect(screen.getByText("Bank")).toBeInTheDocument();
    expect(screen.getByText("Software og SaaS")).toBeInTheDocument();
    expect(screen.getByText("Skyldig skat (skattekonto)")).toBeInTheDocument();
    expect(screen.getByText("DK_PURCHASE_25")).toBeInTheDocument();
  });

  test("viser type-summary med klik-til-filter", async () => {
    const user = userEvent.setup();
    renderView();
    expect(
      await screen.findByRole("button", { name: /Indtægt: 1/ }),
    ).toBeInTheDocument();
    // Klik filter til 'asset'
    await user.click(screen.getByRole("button", { name: /Aktiv: 1/ }));
    // Indtægts-konto skal være væk, asset-konto skal være tilbage
    expect(screen.queryByText("Omsætning, ydelser")).not.toBeInTheDocument();
    expect(screen.getByText("Bank")).toBeInTheDocument();
  });

  test("søg filtrerer på kontonummer, navn og vat-kode", async () => {
    const user = userEvent.setup();
    renderView();
    await screen.findByText("Bank");
    const input = screen.getByRole("searchbox");
    await user.type(input, "saas");
    expect(screen.getByText("Software og SaaS")).toBeInTheDocument();
    expect(screen.queryByText("Bank")).not.toBeInTheDocument();
  });

  test("read-only — ingen oprettelses-/redigerings-knapper", async () => {
    renderView();
    // Vent på at tabellen er renderet.
    await screen.findByText("Omsætning, ydelser");
    expect(
      screen.queryByRole("button", { name: /Opret konto|Tilføj konto|Rediger|Slet/i }),
    ).not.toBeInTheDocument();
  });

  test("indikerer hvilke konti der har bogføringslinjer", async () => {
    renderView();
    const incomeRow = (
      await screen.findByText("Omsætning, ydelser")
    ).closest("tr")!;
    expect(within(incomeRow as HTMLElement).getByText("Ja")).toBeInTheDocument();
    const bankRow = (
      await screen.findByText("Bank")
    ).closest("tr")!;
    expect(within(bankRow as HTMLElement).getByText("Nej")).toBeInTheDocument();
  });
});
