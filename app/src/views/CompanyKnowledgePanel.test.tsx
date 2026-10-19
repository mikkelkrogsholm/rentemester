import { afterEach, expect, test, vi } from "bun:test";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { restoreGlobals, stubGlobal } from "../test/globals";
import { renderAt } from "../test/render";
import { CompanyKnowledgePanel } from "./CompanyKnowledgePanel";

afterEach(restoreGlobals);
const row = { assertionId: "knowledge-synthetic", predicate: "business_description", reviewState: "proposed", value: "Synthetic service", source: { kind: "user", ref: "brief" }, history: [{ event_type: "proposed", principal_id: "synthetic-owner" }] };
test("reviews explicit server assertion and preserves full provenance/history", async () => {
 let reviewed = false; const writes: Array<Record<string, unknown>> = [];
 stubGlobal("fetch", vi.fn(async (_: RequestInfo | URL, init?: RequestInit) => {
  if (init?.method === "POST") { writes.push(JSON.parse(String(init.body))); reviewed = true; return Response.json({ ok: true, assertion: { ...row, reviewState: "approved" } }); }
  return Response.json({ ok: true, context: { assertions: [], conflicts: [] }, history: [{ ...row, reviewState: reviewed ? "approved" : "proposed" }] });
 }));
 renderAt(<CompanyKnowledgePanel slug="synthetic-company" />);
 await userEvent.click(await screen.findByText("knowledge-synthetic · business_description · proposed"));
 expect(screen.getByText(/synthetic-owner/)).toBeInTheDocument();
 const approve = screen.getByRole("button", { name: "Godkend knowledge-synthetic" }); expect(approve).toBeDisabled();
 await userEvent.click(screen.getByLabelText("Knowledge confirm")); await userEvent.click(approve);
 await waitFor(() => expect(writes).toEqual([{ assertionId: "knowledge-synthetic", decision: "approved", reason: "", confirm: true }]));
 expect(await screen.findByText("knowledge-synthetic · business_description · approved")).toBeInTheDocument();
});
test("sends chosen proposal without actor overrides and clears confirmation on edits", async () => {
 const writes: unknown[] = [];
 stubGlobal("fetch", vi.fn(async (_: RequestInfo | URL, init?: RequestInit) => { if (init?.method === "POST") writes.push(JSON.parse(String(init.body))); return Response.json({ ok: true, context: { assertions: [], conflicts: [] }, history: [] }); }));
 renderAt(<CompanyKnowledgePanel slug="synthetic-company" />);
 await screen.findByText("Effektiv kontekst");
 await userEvent.clear(screen.getByLabelText("Knowledge value")); await userEvent.type(screen.getByLabelText("Knowledge value"), '"Synthetic service"');
 await userEvent.type(screen.getByLabelText("Knowledge source ref"), "synthetic-brief");
 await userEvent.click(screen.getByLabelText("Knowledge confirm"));
 await userEvent.type(screen.getByLabelText("Knowledge source ref"), "-revised");
 expect(screen.getByRole("button", { name: "Registrér vidensforslag" })).toBeDisabled();
 await userEvent.click(screen.getByLabelText("Knowledge confirm")); await userEvent.click(screen.getByRole("button", { name: "Registrér vidensforslag" }));
 await waitFor(() => expect(writes.length).toBe(1));
 expect(writes[0]).toEqual({ predicate: "business_description", value: "Synthetic service", source: { kind: "user", ref: "synthetic-brief-revised" }, validFrom: new Date().toISOString().slice(0, 10), certainty: "confirmed", confirm: true });
});
test("unknown mutation blocks further writes until explicit reconciliation", async () => {
 let writes = 0;
 stubGlobal("fetch", vi.fn(async (_: RequestInfo | URL, init?: RequestInit) => { if (init?.method === "POST") { writes += 1; throw new TypeError("synthetic interrupted transport"); } return Response.json({ ok: true, context: { assertions: [], conflicts: ["operating_status"] }, history: [row] }); }));
 renderAt(<CompanyKnowledgePanel slug="synthetic-company" />);
 await userEvent.click(await screen.findByText("knowledge-synthetic · business_description · proposed"));
 await userEvent.click(screen.getByLabelText("Knowledge confirm")); await userEvent.click(screen.getByRole("button", { name: "Godkend knowledge-synthetic" }));
 await waitFor(() => expect(writes).toBe(1));
 await waitFor(() => expect(screen.getByRole("button", { name: "Godkend knowledge-synthetic" })).toBeDisabled());
 expect(screen.getByText(/Konflikt kræver review/)).toBeInTheDocument();
});
