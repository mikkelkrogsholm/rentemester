// Tests for the AssetsView — the Anlægskartotek-side of #336.
//
// The view consumes `/api/companies/:slug/assets` (read) plus a write
// endpoint per action. Tests focus on what the OWNER sees and on whether the
// one-click depreciation flow posts. Depreciation arithmetic itself is
// covered by the server-side tests; the frontend never re-implements it.

import { describe, expect, test, vi } from "bun:test";
import { screen, within, fireEvent, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MutationMemoryProvider } from "../lib/mutation-memory";
import { AssetsView } from "./AssetsView";
import { renderAt } from "../test/render";
import { assets, documents, mockFetch } from "../test/fixtures";
import manifest from "../../../protocol/http-v1.json";
import { withProtocol } from "../test/protocol";
import { stubGlobal } from "../test/globals";

function route(over = {}) {
  return {
    "GET /api/companies/acme-aps/assets": {
      assets: assets(over),
    },
    "GET /api/companies/acme-aps/documents": {
      documents: documents(),
    },
  };
}

function renderView() {
  return renderAt(<AssetsView />, {
    route: "/companies/acme-aps/anlaeg",
    path: "/companies/:slug/anlaeg",
  });
}

describe("AssetsView — Anlægskartotek (#336)", () => {
  test("lists capitalised assets with cost, accumulated depreciation and net book value", async () => {
    mockFetch(route());
    renderView();
    expect(
      await screen.findByRole("heading", { name: "Anlæg", level: 1 }),
    ).toBeInTheDocument();
    expect(screen.getByText("MacBook Pro")).toBeInTheDocument();
    expect(screen.getByText("Server rack")).toBeInTheDocument();
    // The "6/36 afskrevet" badge is the cockpit's read of postedPeriods.
    expect(screen.getByText(/6\/36 afskrevet/)).toBeInTheDocument();
    expect(screen.getByText(/Fuldt afskrevet/)).toBeInTheDocument();
  });

  test("shows the summary totals (kostpris, restværdi, straksafskrivninger)", async () => {
    mockFetch(route());
    renderView();
    await screen.findByText("Bogført kostpris");
    expect(screen.getByText("Bogført kostpris")).toBeInTheDocument();
    expect(screen.getByText("Restværdi (netto)")).toBeInTheDocument();
    // "Straksafskrivninger" labels both the summary card and the section
    // header — at least one of them must render.
    expect(screen.getAllByText("Straksafskrivninger").length).toBeGreaterThan(
      0,
    );
    // Kostpris-summen: 72.000 viser samme tal i kort og i totalrækken.
    expect(screen.getAllByText(/72\.000,00/).length).toBeGreaterThan(0);
  });

  test("lists straksafskrivninger with the threshold rule reference", async () => {
    mockFetch(route());
    renderView();
    expect(await screen.findByText("Tastatur")).toBeInTheDocument();
    expect(
      screen.getByText(/AL §6 stk\. 1 nr\. 2/),
    ).toBeInTheDocument();
  });

  test("a fresh ledger with no assets shows the empty state", async () => {
    mockFetch(
      route({
        assets: [],
        writeOffs: [],
        totals: {
          cost: 0,
          accumulatedDepreciation: 0,
          netBookValue: 0,
          activeCount: 0,
          fullyDepreciatedCount: 0,
          writeOffCount: 0,
          writeOffTotal: 0,
        },
      }),
    );
    renderView();
    expect(
      await screen.findByText(/Der er ingen kapitaliserede anlæg/),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/Ingen straksafskrivninger endnu/),
    ).toBeInTheDocument();
  });

  test("'Beregn afskrivning' calls the depreciate endpoint and reloads the list", async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input.toString();
      const path = url.replace(/^https?:\/\/[^/]+/, "").split("?")[0];
      const method = (init?.method ?? "GET").toUpperCase();
      if (
        method === "GET" &&
        path === "/api/companies/acme-aps/assets"
      ) {
        return new Response(
          JSON.stringify({ ok: true, assets: assets() }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (method === "GET" && path === "/api/companies/acme-aps/assets/1/next-depreciation") {
        return new Response(JSON.stringify({ ok: true, nextDepreciation: { assetId: 1, totalPeriods: 36, postedPeriods: 6, remainingPeriods: 30, nextPeriodIndex: 7, nextPeriodAmount: 1333.33 } }), { headers: { "content-type": "application/json" } });
      }
      if (
        method === "POST" &&
        path === "/api/companies/acme-aps/assets/1/depreciate"
      ) {
        return new Response(
          JSON.stringify({
            ok: true,
            depreciation: {
              entryId: 1,
              assetId: 1,
              periodIndex: 7,
              periodAmount: 1333.33,
            },
          }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      return new Response(JSON.stringify({ ok: true }), {
        status: 200,
        headers: { "content-type": "application/json" },
      });
    });
    stubGlobal("fetch", vi.fn(withProtocol(fetchMock, manifest.operations.map(op => [op.method, op.path] as const))));

    renderView();
    const row = (await screen.findByText("MacBook Pro")).closest("tr")!;
    const rowBtn = within(row as HTMLElement).getByRole("button", {
      name: /Beregn afskrivning for MacBook Pro/,
    });
    await userEvent.click(rowBtn);

    // The cockpit now opens a ConfirmDialog instead of using window.confirm.
    // The dialog's primary action is "Bogfør afskrivning".
    const dialog = await screen.findByRole("dialog", {
      name: /Bogfør afskrivning: MacBook Pro/,
    });
    expect(within(dialog as HTMLElement).getByText(/1.333,33/)).toBeInTheDocument();
    const confirmBtn = within(dialog as HTMLElement).getByRole("button", {
      name: /Bogfør afskrivning/,
    });
    await userEvent.click(confirmBtn);

    await waitFor(() => {
      const calls = fetchMock.mock.calls;
      const posted = calls.some(([url, init]) => {
        const u = typeof url === "string" ? url : url.toString();
        const m = (init?.method ?? "GET").toUpperCase();
        return (
          m === "POST" &&
          u.endsWith("/api/companies/acme-aps/assets/1/depreciate")
        );
      });
      expect(posted).toBe(true);
    });
  });

  test("'Beregn afskrivning' is disabled for a fully depreciated asset", async () => {
    mockFetch(route());
    renderView();
    const row = (await screen.findByText("Server rack")).closest("tr")!;
    const button = within(row as HTMLElement).getByRole("button", {
      name: /Beregn afskrivning/,
    });
    expect(button).toBeDisabled();
  });

  test("'Registrér anlæg' opens the modal with name + bilag picker", async () => {
    mockFetch(route());
    renderView();
    const open = await screen.findByRole("button", {
      name: "Registrér anlæg",
    });
    fireEvent.click(open);
    expect(
      await screen.findByRole("heading", { name: "Registrér nyt anlæg" }),
    ).toBeInTheDocument();
    // The dropdown reads from /documents so the bilag is selectable.
    expect(screen.getByText(/DOC-2026-000001/)).toBeInTheDocument();
  });

  test("'Straksafskriv' opens the straksafskrivnings-modal with hjemmel-felt", async () => {
    mockFetch(route());
    renderView();
    const open = await screen.findByRole("button", { name: "Straksafskriv" });
    fireEvent.click(open);
    expect(
      await screen.findByRole("heading", {
        name: "Straksafskriv småanskaffelse",
      }),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/Hjemmelshenvisning \(tærskelregel\)/),
    ).toBeInTheDocument();
  });
});


test("asset registration accepts Danish decimal and thousands separators", async () => {
  mockFetch({ ...route(), "POST /api/companies/acme-aps/assets": { asset: { id: 42 } } });
  renderView();
  await userEvent.click(await screen.findByRole("button", { name: "Registrér anlæg" }));
  const dialog = screen.getByRole("dialog", { name: "Registrér nyt anlæg" });
  await userEvent.type(within(dialog).getByLabelText("Navn"), "Synthetic equipment");
  await userEvent.type(within(dialog).getByLabelText("Kostpris (kr.)"), "1.234,56");
  await userEvent.selectOptions(within(dialog).getByLabelText("Bilag (købsdokument)"), "1");
  await userEvent.click(within(dialog).getByRole("button", { name: "Registrér anlæg" }));
  const create = (fetch as unknown as ReturnType<typeof vi.fn>).mock.calls.find(([url, init]) => String(url).endsWith("/assets") && init?.method === "POST");
  expect(create).toBeDefined();
  expect(JSON.parse(String((create![1] as RequestInit).body)).cost).toBe(1234.56);
  expect(screen.queryByRole("dialog", { name: "Kassér ændringer?" })).not.toBeInTheDocument();
});

describe("asset_disposal report", () => {
  test("disposed history is honest and later depreciation is disabled", async () => {
    const base = assets();
    mockFetch(route({assets:[{...base.assets[0],status:"disposed",netBookValue:0,remainingPeriods:0,disposalDate:"2026-10-20",disposalJournalEntryNo:"2026-20"}]}));
    renderView();
    expect(await screen.findByText("Afhændet")).toBeInTheDocument();
    expect(screen.getByText(/Postering 2026-20/)).toBeInTheDocument();
    expect(screen.getByRole("button",{name:/Beregn afskrivning for/})).toBeDisabled();
    expect(screen.queryByRole("button",{name:/^Afhænd /})).toBeNull();
  });
});

describe("native immediate write-off owner review", () => {
  function nativeRoutes() {
    return { ...route({ nativeAssetAccounts: true, writeOffAvailable: true }),
      "GET /api/companies/acme-aps/accounts": { accounts: { accounts: [
        { accountNo: "EX", name: "Synthetic expense", active: true, type: "expense", normalBalance: "debit" },
        { accountNo: "OLD", name: "Inactive", active: false, type: "expense", normalBalance: "debit" },
      ] } },
      "POST /api/companies/acme-aps/asset/original/write-off": { ok: true, entryId: 17, entryNo: "2026-17", entryHash: "a".repeat(64), writeOffId: 9, cost: 1234.56, thresholdDkk: 36000, appliedRules: ["DK-ASSET-WRITEOFF-001"], errors: [] },
    };
  }
  async function fillReview() {
    await userEvent.click(await screen.findByRole("button", { name: "Straksafskriv" }));
    const dialog = screen.getByRole("dialog", { name: "Straksafskriv småanskaffelse" });
    await userEvent.type(within(dialog).getByLabelText("Navn"), "Synthetic keyboard");
    await userEvent.type(within(dialog).getByLabelText("Beløb (kr.)"), "1.234,56");
    await userEvent.selectOptions(within(dialog).getByLabelText("Bilag (købsdokument)"), "1");
    expect(within(dialog).getByLabelText("Udgiftskonto")).toHaveValue("");
    expect(within(dialog).queryByRole("option", { name: /Inactive/ })).toBeNull();
    await userEvent.selectOptions(within(dialog).getByLabelText("Udgiftskonto"), "EX");
    expect(within(dialog).getByLabelText("Hjemmelshenvisning (tærskelregel)")).toHaveValue("");
    await userEvent.type(within(dialog).getByLabelText("Hjemmelshenvisning (tærskelregel)"), "Advisor judgment");
    await userEvent.click(within(dialog).getByRole("button", { name: "Gennemgå straksafskrivning" }));
    return screen.findByRole("dialog", { name: "Bekræft straksafskrivning" });
  }
  test("first submit only reviews; explicit confirmation sends once and shows actual journal identity", async () => {
    mockFetch(nativeRoutes()); renderView();
    const review = await fillReview();
    const transport = fetch as unknown as ReturnType<typeof vi.fn>;
    expect(transport.mock.calls.filter(([, init]) => init?.method === "POST")).toHaveLength(0);
    expect(within(review).getByText(/Bilag #1/)).toBeInTheDocument();
    await userEvent.click(within(review).getByRole("button", { name: "Bekræft og bogfør straksafskrivning" }));
    expect(await screen.findByText(/Bogført: 2026-17/)).toBeInTheDocument();
    const writes = transport.mock.calls.filter(([, init]) => init?.method === "POST");
    expect(writes).toHaveLength(1);
    expect(writes[0]?.[0]).toContain("/asset/original/write-off");
    expect(JSON.parse(String(writes[0]?.[1]?.body)).confirmImmediateWriteOff).toBe(true);
    await userEvent.click(screen.getByRole("button", { name: "Færdig" }));
    expect(screen.queryByRole("dialog", { name: "Kassér ændringer?" })).toBeNull();
    expect(screen.queryByRole("dialog", { name: "Straksafskriv småanskaffelse" })).toBeNull();
    expect(transport.mock.calls.filter(([url]) => String(url).endsWith("/assets")).length).toBeGreaterThan(1);
  });
  test("unknown receipt remains locked after read-only refresh, dismissal and provider reload", async () => {
    sessionStorage.removeItem("rentemester:uncertain-operations:v1");
    const routes: Record<string, any> = nativeRoutes();
    routes["POST /api/companies/acme-aps/asset/original/write-off"].entryHash = "truncated";
    routes["GET /api/companies/acme-aps/journal"] = { journal: {} };
    mockFetch(routes);
    const mount = () => renderAt(<MutationMemoryProvider><AssetsView /></MutationMemoryProvider>, { route: "/companies/acme-aps/anlaeg", path: "/companies/:slug/anlaeg" });
    const first = mount();
    try {
      const review = await fillReview();
      const confirm = within(review).getByRole("button", { name: "Bekræft og bogfør straksafskrivning" });
      fireEvent.click(confirm); fireEvent.click(confirm);
      await screen.findByText(/Resultatet er ukendt: kvitteringen/);
      const transport = fetch as unknown as ReturnType<typeof vi.fn>;
      expect(transport.mock.calls.filter(([, init]) => init?.method === "POST")).toHaveLength(1);
      await waitFor(() => expect(transport.mock.calls.some(([url]) => String(url).endsWith("/journal"))).toBe(true));
      first.unmount();
      mount();
      await userEvent.click(await screen.findByRole("button", { name: "Straksafskriv" }));
      expect(await screen.findByText(/Serverens resultat kunne ikke bekræftes/)).toBeInTheDocument();
      expect(screen.getByLabelText("Navn")).toBeDisabled();
      await userEvent.click(screen.getByRole("button", { name: "Kontrollér status" }));
      expect(screen.getByLabelText("Navn")).toBeDisabled();
      expect(transport.mock.calls.filter(([, init]) => init?.method === "POST")).toHaveLength(1);
      await userEvent.click(screen.getByRole("button", { name: "Resultatet er afklaret" }));
      expect(screen.getByLabelText("Navn")).toBeDisabled();
      await userEvent.click(screen.getByRole("button", { name: "Jeg har kontrolleret resultatet" }));
      expect(screen.getByLabelText("Navn")).toBeEnabled();
      expect(transport.mock.calls.filter(([, init]) => init?.method === "POST")).toHaveLength(1);
    } finally { sessionStorage.removeItem("rentemester:uncertain-operations:v1"); }
  });
  test("a fresh native form closes without discarding untouched fields", async () => {
    mockFetch(nativeRoutes()); renderView();
    await userEvent.click(await screen.findByRole("button", { name: "Straksafskriv" }));
    await userEvent.click(screen.getByRole("button", { name: "Annullér" }));
    expect(screen.queryByRole("dialog")).toBeNull();
  });
  test("cancel review sends nothing and allows editing", async () => {
    mockFetch(nativeRoutes()); renderView(); const review = await fillReview();
    await userEvent.click(within(review).getByRole("button", { name: "Annullér" }));
    expect(screen.getByLabelText("Navn")).toBeEnabled();
    expect((fetch as unknown as ReturnType<typeof vi.fn>).mock.calls.filter(([, init]) => init?.method === "POST")).toHaveLength(0);
  });
});
