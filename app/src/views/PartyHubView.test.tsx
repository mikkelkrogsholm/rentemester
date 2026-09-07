import { describe, expect, test, vi } from "bun:test";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { PartyHubView, PartyProfileView } from "./PartyHubView";
import { renderAt } from "../test/render";
import { mockFetch } from "../test/fixtures";

const hub = { rows: [{ partyId: "party-1", name: "Leverandør ApS", roles: ["vendor"], recentActivity: "2026-01-15", computedSpend: 125, partyLink: { href: "/companies/acme-aps/parter/party-1" } }] };
const profile = (warnings: string[] = []) => ({ party: { name: "Leverandør ApS", kind: "organization", roles: [{ role: "vendor" }] }, computed: { period: { from: "2026-01-01", to: "2026-01-31" }, asOf: "2026-01-31", purchase: { amount: 125 }, sales: { amount: 0 }, sourceCoverage: { documents: 1 } }, research: { assertions: [{ field: "name", value: "Leverandør ApS", source: "register", observed_at: "2026-01-01", review_state: "conflicting" }], warnings }, links: { documents: [{ id: 1, label: "B-1", role: "vendor" }], relations: [] } });

function renderHub() { return renderAt(<PartyHubView />, { route: "/companies/acme-aps/parter", path: "/companies/:slug/parter" }); }
function renderProfile() { return renderAt(<PartyProfileView />, { route: "/companies/acme-aps/parter/party-1", path: "/companies/:slug/parter/:partyId" }); }

describe("Party Hub and profile (#653)", () => {
  test("renders the normal hub list and opens the selected profile with the keyboard", async () => {
    mockFetch({ "GET /api/companies/acme-aps/party-hub": hub });
    render(<MemoryRouter initialEntries={["/companies/acme-aps/parter"]}><Routes><Route path="/companies/:slug/parter" element={<PartyHubView />} /><Route path="/companies/:slug/parter/:partyId" element={<p>Profil åbnet</p>} /></Routes></MemoryRouter>);
    (await screen.findByRole("button", { name: /Leverandør ApS/ })).focus();
    await userEvent.keyboard("{Enter}");
    expect(await screen.findByText("Profil åbnet")).toBeInTheDocument();
  });

  test("renders loading, empty and API-error states with their matching actions", async () => {
    const originalFetch = globalThis.fetch;
    let release!: (value: Response) => void;
    globalThis.fetch = vi.fn(() => new Promise<Response>((resolve) => { release = resolve; })) as unknown as typeof fetch;
    try {
      renderHub();
      expect(screen.getByText("Henter parter…")).toBeInTheDocument();
      release(new Response(JSON.stringify({ ok: true, rows: [] }), { headers: { "content-type": "application/json" } }));
      expect(await screen.findByText("Ingen parter endnu")).toBeInTheDocument();
    } finally { globalThis.fetch = originalFetch; }

    mockFetch({ "GET /api/companies/acme-aps/party-hub": { __error: { code: "internal", message: "Hub fejlede" } } });
    renderHub();
    expect(await screen.findByText("Hub fejlede")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Prøv igen" })).toBeInTheDocument();
  });

  test("renders profile facts, proposed/conflicting research warning and profile error", async () => {
    mockFetch({ "GET /api/companies/acme-aps/party-hub/party-1": profile(["Foreslået kilde kræver review: import", "Modstridende kilde kræver review: register"]) });
    renderProfile();
    expect(await screen.findByRole("heading", { name: "Leverandør ApS" })).toBeInTheDocument();
    expect(screen.getByText("Beregnede Rentemester-tal")).toBeInTheDocument();
    expect(screen.getByText("Foreslået kilde kræver review: import")).toBeInTheDocument();
    expect(screen.getByText("Modstridende kilde kræver review: register")).toBeInTheDocument();

    mockFetch({ "GET /api/companies/acme-aps/party-hub/party-1": { __error: { code: "not_found", message: "Part findes ikke" } } });
    renderProfile();
    expect(await screen.findByText("Part findes ikke")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Prøv igen" })).toBeInTheDocument();
  });

  test("marks the profile as a detail route while keeping the hub filter resettable", async () => {
    mockFetch({ "GET /api/companies/acme-aps/party-hub": hub });
    renderHub();
    await screen.findByText("Leverandør ApS");
    await userEvent.type(screen.getByLabelText("Søg parter"), "leverandør");
    expect(screen.getByText(/Aktive filtre: Søgning: leverandør/i)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Nulstil filtre" }));
    expect(screen.getByDisplayValue("")).toBeInTheDocument();

    mockFetch({ "GET /api/companies/acme-aps/party-hub/party-1": profile() });
    renderProfile();
    expect((await screen.findByRole("heading", { name: "Leverandør ApS" })).closest("section")?.getAttribute("data-cockpit-detail-route")).toBe("party-profile");
  });
});
