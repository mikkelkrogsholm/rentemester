import { describe, expect, test, vi } from "bun:test";
import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { BankAccountsView } from "./BankAccountsView";
import { renderAt } from "../test/render";
import { mockFetch } from "../test/fixtures";
import { stubGlobal } from "../test/globals";

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
  for (const outcome of ["success", "failure"] as const) {
    test(`legacy binding reload retains account data while pending and after ${outcome}`, async () => {
      const user = userEvent.setup();
      const account = {
        id: 1, slug: "synthetic-bank", name: "Synthetic legacy bank", bankName: "Synthetic Bank",
        registrationNo: null, accountNo: null, iban: null, currency: "DKK",
        ledgerAccountNo: null, active: true, createdAt: "2026-05-20T10:00:00Z",
      };
      let reads = 0;
      let resolveReload!: (response: Response) => void;
      let rejectReload!: (error: Error) => void;
      const pendingReload = new Promise<Response>((resolve, reject) => {
        resolveReload = resolve;
        rejectReload = reject;
      });
      stubGlobal("fetch", vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
        const url = String(input);
        if (url.endsWith("/bank-accounts") && init?.method !== "POST") {
          reads += 1;
          return reads === 1 ? Response.json(payload([account])) : pendingReload;
        }
        if (url.endsWith("/legacy-binding/plan")) return Response.json({ ok: true, plan: { planHash: "b".repeat(64) } });
        if (url.endsWith("/legacy-binding/apply")) return Response.json({ ok: true, binding: {} });
        throw new Error(`Unexpected synthetic request: ${url}`);
      }));
      renderAt(<BankAccountsView />, { route: "/companies/acme-aps/bankkonti", path: "/companies/:slug/bankkonti" });
      expect(await screen.findByText(account.name)).toBeInTheDocument();
      await user.click(screen.getByText(/Avanceret: legacy-binding/));
      await user.click(screen.getByRole("button", { name: "Bind ældre bankkonto" }));
      await user.type(screen.getByLabelText("Finanskonto"), "2000");
      fireEvent.change(screen.getByLabelText("Cutoff"), { target: { value: "2026-05-20" } });
      await user.click(screen.getByRole("button", { name: "Opret plan" }));
      await screen.findByText(/Reviewet plan/);
      await user.click(screen.getByRole("button", { name: "Bekræft binding" }));
      await waitFor(() => expect(reads).toBe(2));
      expect(screen.getByText(account.name)).toBeInTheDocument();
      if (outcome === "success") {
        await act(async () => { resolveReload(Response.json(payload([{ ...account, ledgerAccountNo: "2000" }]))); });
        expect(await screen.findByText("2000")).toBeInTheDocument();
        expect(screen.getByRole("button", { name: "Bind ældre bankkonto" })).toBeDisabled();
      } else {
        await act(async () => { rejectReload(new Error("Synthetic reload failure")); });
        expect(await screen.findByRole("alert")).toHaveTextContent("Status kunne ikke opdateres");
        expect(screen.getByText(account.name)).toBeInTheDocument();
      }
    });
  }

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
