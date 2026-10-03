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
