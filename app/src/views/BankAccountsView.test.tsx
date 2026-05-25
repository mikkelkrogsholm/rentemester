import { describe, expect, test } from "vitest";
import { screen } from "@testing-library/react";
import { BankAccountsView } from "./BankAccountsView";
import { renderAt } from "../test/render";
import { mockFetch } from "../test/fixtures";

function payload(accounts: any[] = []) {
  return {
    ok: true as const,
    bankAccounts: {
      slug: "acme-aps",
      company: {
        name: "Acme ApS",
        cvr: "DK12345678",
        country: "DK",
        currency: "DKK",
      },
      accounts,
      profiles: [
        {
          name: "lunar",
          bankName: "Lunar Bank",
          separator: ";",
          encoding: "utf-8",
          dateOrder: "dmy",
        },
        {
          name: "danske-bank",
          bankName: "Danske Bank",
          separator: ";",
          encoding: "utf-8",
          dateOrder: "dmy",
        },
      ],
    },
  };
}

function renderView(body = payload()) {
  mockFetch({ "GET /api/companies/acme-aps/bank-accounts": body });
  return renderAt(<BankAccountsView />, {
    route: "/companies/acme-aps/bankkonti",
    path: "/companies/:slug/bankkonti",
  });
}

describe("BankAccountsView (#345)", () => {
  test("viser tom-state for konti og lister profiler", async () => {
    renderView();
    expect(
      await screen.findByText(/Ingen bankkonti endnu/),
    ).toBeInTheDocument();
    expect(screen.getByText("lunar")).toBeInTheDocument();
    expect(screen.getByText("Lunar Bank")).toBeInTheDocument();
    expect(screen.getByText("danske-bank")).toBeInTheDocument();
  });

  test("har en 'Opret bankkonto'-knap i page-head", async () => {
    renderView();
    expect(
      await screen.findByRole("button", { name: /Opret bankkonto/ }),
    ).toBeInTheDocument();
  });

  test("lister en registreret bankkonto", async () => {
    renderView(
      payload([
        {
          id: 1,
          slug: "lunar-driftskonto",
          name: "Lunar driftskonto",
          bankName: "Lunar Bank",
          registrationNo: "1234",
          accountNo: "5678901",
          iban: null,
          currency: "DKK",
          ledgerAccountNo: "2000",
          active: true,
          createdAt: "2026-05-20T10:00:00Z",
        },
      ]),
    );
    expect(await screen.findByText("Lunar driftskonto")).toBeInTheDocument();
    expect(screen.getByText("1234")).toBeInTheDocument();
    expect(screen.getByText("5678901")).toBeInTheDocument();
    expect(screen.getByText("2000")).toBeInTheDocument();
    expect(screen.getByText("Aktiv")).toBeInTheDocument();
  });
});
