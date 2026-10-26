import { describe, expect, test } from "bun:test";
import { screen } from "@testing-library/react";
import { AttentionView } from "./AttentionView";
import { renderAt } from "../test/render";
import { mockFetch } from "../test/fixtures";

const attention = (items: any[] = [{ id: "exception:4", source: "exception", severity: "high", title: "Bogføring kræver opmærksomhed", reason: "Bogfør det manglende", actor: "agent:test", destination: "bank", sourceIdentity: "exception:4", evidence: { type: "UNMATCHED_BANK_TRANSACTION" } }]) => ({ slug: "acme-aps", company: { name: "Acme ApS", currency: "DKK" }, scope: { from: "2026-01-01", to: "2026-12-31" }, items, count: items.length, status: items.length ? "requires-attention" : "clear" });
function renderView() { return renderAt(<AttentionView />, { route: "/companies/acme-aps/opmaerksomhed", path: "/companies/:slug/opmaerksomhed" }); }

describe("AttentionView (#649)", () => {
  test("lets an owner find the missing booking without route-name knowledge", async () => {
    mockFetch({ "GET /api/companies/acme-aps/attention": { attention: attention() } }); renderView();
    expect(await screen.findByText("Bogfør det manglende")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Åbn næste opgave" })).toBeInTheDocument();
    expect(screen.getByText("Se grundlag for opgaven")).toBeInTheDocument();
  });
  test("has an honest empty state", async () => {
    mockFetch({ "GET /api/companies/acme-aps/attention": { attention: attention([]) } }); renderView();
    expect(await screen.findByText("Ingen opgaver kræver opmærksomhed")).toBeInTheDocument();
  });
  test("does not reveal a denied destination", async () => {
    mockFetch({ "GET /api/companies/acme-aps/attention": { ok: false, errors: ["forbudt"], code: "FORBIDDEN" } }); renderView();
    expect(await screen.findByText("Opgaver er blokeret", { selector: "[data-evidence-status]" })).toBeInTheDocument();
    expect(screen.getByText("Du har ikke adgang til disse opgaver.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Åbn næste opgave" })).not.toBeInTheDocument();
  });
});
