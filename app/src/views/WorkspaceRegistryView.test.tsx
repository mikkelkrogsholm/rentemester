import { afterEach, describe, expect, test, vi } from "bun:test";
import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { mockFetch } from "../test/fixtures";
import { restoreGlobals, stubGlobal } from "../test/globals";
import { renderAt } from "../test/render";
import { WorkspaceRegistryView } from "./WorkspaceRegistryView";

const parties = { rows: [{ partyId: "party-visible", name: "Visible supplier", kind: "organisation", roles: [{ companySlug: "synthetic-company", role: "vendor" }] }], count: 1, nextCursor: null };
const records = { rows: [{ recordId: "record-visible", filename: "articles.pdf", type: "articles", sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef", sensitivity: "normal" }], count: 1, nextCursor: null };
afterEach(() => restoreGlobals());

describe("WorkspaceRegistryView", () => {
  test("shows loading, then the empty registry", async () => {
    mockFetch({ "GET /api/companies/synthetic-company/workspace-parties": { rows: [], count: 0, nextCursor: null }, "GET /api/companies/synthetic-company/corporate-records": { rows: [], count: 0, nextCursor: null }, "GET /api/companies/synthetic-company/knowledge": { context:{assertions:[],conflicts:[]} } });
    renderAt(<WorkspaceRegistryView />, { route: "/virksomheder/synthetic-company/workspace-register", path: "/virksomheder/:slug/workspace-register" });
    expect(screen.getByText("Henter workspace-register…")).toBeInTheDocument();
    expect(await screen.findByText("Ingen synlige parter endnu.")).toBeInTheDocument();
    expect(screen.getByText("Ingen synlige governance-records endnu.")).toBeInTheDocument();
  });

  test("shows a fetch error", async () => {
    mockFetch({ "GET /api/companies/synthetic-company/workspace-parties": { __error: { code: "forbidden", message: "Ingen adgang" } }, "GET /api/companies/synthetic-company/corporate-records": records });
    renderAt(<WorkspaceRegistryView />, { route: "/virksomheder/synthetic-company/workspace-register", path: "/virksomheder/:slug/workspace-register" });
    expect(await screen.findByText("Ingen adgang")).toBeInTheDocument();
  });

  test("lists visible data and refreshes it through the compact action", async () => {
    let calls = 0;
    stubGlobal("fetch", vi.fn(async (input: RequestInfo | URL) => {
      calls += 1;
      const path = String(input).split("?")[0];
      const payload = path.endsWith("corporate-records") ? records : calls > 2 ? { ...parties, rows: [{ ...parties.rows[0], name: "Refreshed supplier" }] } : parties;
      return new Response(JSON.stringify({ ok: true, ...payload }), { headers: { "content-type": "application/json" } });
    }));
    renderAt(<WorkspaceRegistryView />, { route: "/virksomheder/synthetic-company/workspace-register", path: "/virksomheder/:slug/workspace-register" });
    expect(await screen.findByText("Visible supplier")).toBeInTheDocument();
    expect(screen.getByText("articles.pdf")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Opdater" }));
    expect(await screen.findByText("Refreshed supplier")).toBeInTheDocument();
  });

  test("keeps ownership state compact and makes redaction explicit", async () => {
    mockFetch({
      "GET /api/companies/synthetic-company/workspace-parties": parties,
      "GET /api/companies/synthetic-company/corporate-records": records,
      "GET /api/companies/synthetic-company/knowledge": { context: { assertions: [], conflicts: [] } },
      "GET /api/companies/synthetic-company/ownership": { asOf: "2026-08-30", facts: [], partial: true, consolidation: { eligible: false, reason: "hidden endpoints" } },
      "GET /api/companies/synthetic-company/ownership/history": { history: [{ snapshotId: "synthetic-snapshot", state: "approved", snapshotHash: "0123456789abcdef" }] },
    });
    renderAt(<WorkspaceRegistryView />, { route: "/virksomheder/synthetic-company/workspace-register", path: "/virksomheder/:slug/workspace-register" });
    expect(await screen.findByText("Delvist synlige relationer; skjulte endpoints og antal vises ikke.")).toBeInTheDocument();
    expect(screen.getByText("Ingen synlige, godkendte facts.")).toBeInTheDocument();
    expect(screen.getByText("Forslag, review og apply")).toBeInTheDocument();
    expect(screen.queryByText("hidden endpoints")).not.toBeInTheDocument();
  });

  test("requires an explicit confirmation before a canonical party mutation", async () => {
    mockFetch({
      "GET /api/companies/synthetic-company/workspace-parties": parties,
      "GET /api/companies/synthetic-company/corporate-records": records,
      "GET /api/companies/synthetic-company/knowledge": { context:{assertions:[],conflicts:[]} },
      "GET /api/companies/synthetic-company/ownership": { asOf:"2026-08-30",facts:[],partial:false,consolidation:{eligible:false,reason:"not supported"} },
      "GET /api/companies/synthetic-company/ownership/history": { history:[] },
      "POST /api/companies/synthetic-company/workspace-parties": {party:{...parties.rows[0]}},
    });
    renderAt(<WorkspaceRegistryView />, { route: "/virksomheder/synthetic-company/workspace-register", path: "/virksomheder/:slug/workspace-register" });
    await screen.findByText("Visible supplier");
    await userEvent.type(screen.getByLabelText("Party name"),"Synthetic created");
    await userEvent.type(screen.getByLabelText("Party source"),"Synthetic source");
    await userEvent.type(screen.getByLabelText("Party review assertion"),"Synthetic reviewed source");
    const create=screen.getByRole("button",{name:"Registrér partændring"});
    expect(create).toBeDisabled();
    await userEvent.click(screen.getByLabelText("Party confirm"));
    expect(create).not.toBeDisabled();
    await userEvent.click(create);
    expect(await screen.findByText("Registreret. Se kilder, assertions og historik i serverens aktuelle læsning.")).toBeInTheDocument();
  });

  test("keeps the control visible but reports a server-side role denial", async () => {
    mockFetch({
      "GET /api/companies/synthetic-company/workspace-parties": parties,
      "GET /api/companies/synthetic-company/corporate-records": records,
      "GET /api/companies/synthetic-company/knowledge": { context:{assertions:[],conflicts:[]} },
      "GET /api/companies/synthetic-company/ownership": { asOf:"2026-08-30",facts:[],partial:false,consolidation:{eligible:false,reason:"not supported"} },
      "GET /api/companies/synthetic-company/ownership/history": { history:[] },
      "POST /api/companies/synthetic-company/workspace-parties": { __error:{code:"forbidden",message:"Din rolle har ikke adgang"} },
    });
    renderAt(<WorkspaceRegistryView />, { route: "/virksomheder/synthetic-company/workspace-register", path: "/virksomheder/:slug/workspace-register" });
    await screen.findByText("Visible supplier");
    await userEvent.type(screen.getByLabelText("Party name"),"Synthetic created");
    await userEvent.type(screen.getByLabelText("Party source"),"Synthetic source");
    await userEvent.type(screen.getByLabelText("Party review assertion"),"Synthetic reviewed source");
    await userEvent.click(screen.getByLabelText("Party confirm"));
    await userEvent.click(screen.getByRole("button",{name:"Registrér partændring"}));
    expect(await screen.findByRole("alert")).toHaveTextContent("Din rolle har ikke adgang");
  });
});

test("corporate metadata stays visible while sensitive detail denial exposes no original", async () => {
 mockFetch({"GET /api/companies/synthetic-company/ownership":{asOf:"2026-01-01",facts:[],partial:false,consolidation:{eligible:false,reason:"existing v1 manifest remains authoritative"}},"GET /api/companies/synthetic-company/ownership/history":{history:[]},"GET /api/companies/synthetic-company/workspace-parties":{rows:[],count:0},"GET /api/companies/synthetic-company/corporate-records":records,"GET /api/companies/synthetic-company/corporate-records/record-visible":{__error:{code:"not_found",message:"Record kan ikke vises med aktuel adgang"}},"GET /api/companies/synthetic-company/knowledge":{context:{assertions:[],conflicts:[]}}});
 renderAt(<WorkspaceRegistryView/>,{route:"/companies/synthetic-company/workspace-register",path:"/companies/:slug/workspace-register"});
 await screen.findByText("articles.pdf");await userEvent.click(screen.getByRole("button",{name:"Vis record record-visible"}));expect(await screen.findByRole("alert")).toHaveTextContent("Record kan ikke vises");expect(screen.queryByRole("button",{name:"Hent original record-visible"})).not.toBeInTheDocument();
 expect(screen.getByRole("combobox",{name:"Corporate record type"})).toBeInTheDocument();expect(screen.getByLabelText("Bekræft corporate record upload")).not.toBeChecked();
});


test("ownership review exposes full exact hashes and dated direct facts before confirmed mutation", async () => {
 const hash="a".repeat(64),diffHash="b".repeat(64);
 mockFetch({"GET /api/companies/synthetic-company/workspace-parties":{rows:[],count:0},"GET /api/companies/synthetic-company/corporate-records":{rows:[],count:0},"GET /api/companies/synthetic-company/knowledge":{context:{assertions:[],conflicts:[]}},"GET /api/companies/synthetic-company/ownership":{asOf:"2000-12-31",partial:false,consolidation:{eligible:false,reason:"existing v1 manifest remains authoritative"},facts:[{owner:{kind:"party",partyId:"party-owner"},ownedCompanySlug:"synthetic-company",validFrom:"2000-01-01",validToExclusive:"2001-01-01",economicBasisPoints:1000,controlType:"equity",jurisdiction:"DK",evidenceRefs:["record-reviewed"]}]},"GET /api/companies/synthetic-company/ownership/history":{history:[{snapshotId:"ownership-reviewed",state:"approved",snapshotHash:hash,diffHash,source:"synthetic",observedAt:"2000-01-01",principal:{kind:"user",id:"maker"},diff:{version:2,add:[],endings:[]},history:[{event_type:"approved",actor:"user:reviewer",principal_id:"reviewer",principal_kind:"user",created_at:"2000-01-01"}]}]}});
 renderAt(<WorkspaceRegistryView/>,{route:"/companies/synthetic-company/workspace-register",path:"/companies/:slug/workspace-register"});
 const region=await screen.findByRole("region",{name:"Ejer- og kontrolforhold"});
 expect(await within(region).findByText("party-owner")).toBeInTheDocument();expect(region).toHaveTextContent("1000 bp");expect(region).toHaveTextContent("2001-01-01 (slutdato eksklusiv)");expect(region).toHaveTextContent("record-reviewed");
 await userEvent.click(within(region).getByText("Forslag, review og apply"));await userEvent.click(within(region).getByText("ownership-reviewed · approved"));
 expect(within(region).getByText(hash)).toBeInTheDocument();expect(within(region).getByText(diffHash)).toBeInTheDocument();expect(region).toHaveTextContent("anden autentificeret person");
 const form=within(region).getByRole("form",{name:"Apply eksakt diff"});expect(within(form).getByRole("button",{name:"Apply eksakt diff"})).toBeDisabled();await userEvent.click(within(form).getByRole("checkbox"));expect(within(form).getByRole("button",{name:"Apply eksakt diff"})).not.toBeDisabled();
});
