import { afterEach, describe, expect, test } from "bun:test";
import { fireEvent, screen, waitFor } from "@testing-library/react";
import { WorkspaceInboxView } from "./WorkspaceInboxView";
import { mockFetch } from "../test/fixtures";
import { restoreGlobals } from "../test/globals";
import { renderAt } from "../test/render";

afterEach(() => restoreGlobals());

describe("WorkspaceInboxView", () => {
  test("shows the single explicit review and handoff flow", async () => {
    mockFetch({ "GET /api/companies": { companies: [{ slug: "target-company", name: "Target company", archived: false }] }, "GET /api/companies/synthetic-company/workspace-inbox": { rows:[{sourceId:"inbox-1",filename:"synthetic.txt",transport:"upload",sha256:"a".repeat(64),assignments:[],exception:{code:"INBOX_NO_CANDIDATE"}}] } });
    renderAt(<WorkspaceInboxView />, { route:"/companies/synthetic-company/workspace-inbox", path:"/companies/:slug/workspace-inbox" });
    expect(await screen.findByText("Fælles dokumentindbakke")).toBeTruthy();
    expect(screen.getByText("synthetic.txt")).toBeTruthy();
    await screen.findByRole("option", { name: "Target company" });
    fireEvent.change(screen.getByLabelText("Inbox målvirksomhed"),{target:{value:"target-company"}});
    expect(screen.getByText("Godkend ruting")).toBeTruthy();
    expect(screen.getByText("Fuldfør handoff")).toBeTruthy();
    await waitFor(()=>expect(screen.getByDisplayValue("Target company")).toBeTruthy());
  });
});

test("routing requires explicit target review and sends only that target after confirmation", async () => {
  mockFetch({
    "GET /api/companies": { companies: [{ slug: "target-company", name: "Target company", archived: false }] },
    "GET /api/companies/synthetic-company/workspace-inbox": { rows: [{ sourceId: "inbox-review", filename: "reviewed-source.txt", transport: "upload", sha256: "b".repeat(64), receivedAt: "2026-01-01T00:00:00.000Z", metadata: { documentType: "cash_register_receipt" }, assignments: [] }] },
    "POST /api/companies/synthetic-company/workspace-inbox/inbox-review/assign": { source: { sourceId: "inbox-review" } },
  });
  renderAt(<WorkspaceInboxView />, { route: "/companies/synthetic-company/workspace-inbox", path: "/companies/:slug/workspace-inbox" });
  await screen.findByText("reviewed-source.txt");
  fireEvent.click(screen.getByRole("button", { name: "Godkend ruting" }));
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(screen.getByRole("button", { name: "Fuldfør handoff" })).toBeDisabled();
  await screen.findByRole("option", { name: "Target company" });
  fireEvent.change(screen.getByLabelText("Inbox målvirksomhed"), { target: { value: "target-company" } });
  fireEvent.click(screen.getByRole("button", { name: "Godkend ruting" }));
  const dialog = await screen.findByRole("dialog", { name: "Bekræft ruting" });
  expect(dialog.textContent).toContain("reviewed-source.txt → Target company");
  expect(dialog.textContent).toContain("b".repeat(64));
  const calls = () => (globalThis.fetch as unknown as { mock: { calls: Array<[unknown, RequestInit]> } }).mock.calls.filter(c => String(c[0]).endsWith("/assign"));
  expect(calls().length).toBe(0);
  fireEvent.click(screen.getByRole("button", { name: "Bekræft ruting" }));
  await waitFor(() => expect(calls().length).toBe(1));
  expect(JSON.parse(String(calls()[0][1].body))).toEqual({ companySlug: "target-company", confirm: true });
});
