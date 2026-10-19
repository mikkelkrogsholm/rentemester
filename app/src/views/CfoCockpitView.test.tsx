import { beforeEach, describe, expect, test, vi } from "bun:test";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { renderAt } from "../test/render";

const companies = vi.fn();
const cfoAnalytics = vi.fn();
const groupReportProfiles = vi.fn();
vi.mock("../lib/api", () => ({ api: { companies, cfoAnalytics, groupReportProfiles } }));

import { CfoCockpitView } from "./CfoCockpitView";

const companyAnalytics = {
  ok: true as const, schemaVersion: "rentemester-cfo-analytics-v1", scope: "company" as const,
  status: "ready" as const, asOf: "2026-08-30", from: "2026-01-01", to: "2026-08-30",
  companies: ["alpha"], partial: false, mode: "legal-company" as const, aggregate: "none" as const,
  limitations: [], page: { limit: 100, nextCursor: null }, freshness: [{ source: "ledger" as const, companySlug: "alpha", latestTransactionDate: "2026-08-29" }],
  evidenceCompleteness: [{ companySlug: "alpha", status: "ready" as const, postedWithoutDocument: 2, openExceptions: 1 }],
  reconciliation: { rowCount: 1, amountByCurrency: { DKK: 250 }, sourceHashes: ["a".repeat(64)], method: "sum" },
  rows: [{ companySlug: "alpha", sourceType: "ledger" as const, sourceHash: "a".repeat(64), sourceId: "journal:1:line:1", journalEntryId: 1, journalEntryNo: "J-1", documentId: 7, documentHash: "b".repeat(64), partyId: null, partyName: "Syntetisk leverandør", accountNo: "4200", accountName: "Køb", transactionDate: "2026-08-29", currency: "DKK", amount: 250, text: "Syntetisk køb" }],
};

describe("CfoCockpitView", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    companies.mockResolvedValue([{ slug: "alpha", name: "Alpha ApS", archived: false }]);
    cfoAnalytics.mockResolvedValue(companyAnalytics);
    groupReportProfiles.mockResolvedValue({ profiles: [] });
  });

  test("renders a source-linked company view with scope, as-of, freshness and authorized drill-downs", async () => {
    renderAt(<CfoCockpitView />);
    expect(await screen.findByRole("heading", { name: "CFO-overblik" })).toBeInTheDocument();
    expect(await screen.findByText("Virksomhed: Alpha ApS")).toBeInTheDocument();
    expect(screen.getByText(/Pr\. 2026-08-30/)).toBeInTheDocument();
    expect(screen.getByText(/Seneste ledger: 2026-08-29/)).toBeInTheDocument();
    expect(screen.getByText(/Stale-status kan ikke udledes/)).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Bank" })).toHaveAttribute("href", "/companies/alpha/bank");
    expect(screen.getByRole("link", { name: "Postering J-1" })).toHaveAttribute("href", "/companies/alpha/posteringer?account=4200");
    expect(cfoAnalytics).toHaveBeenCalledWith(expect.objectContaining({ scope: "company", companySlug: "alpha" }), expect.objectContaining({ signal: expect.anything() }));
  });

  test("labels portfolio as non-consolidated and never shows a hidden aggregate", async () => {
    cfoAnalytics.mockResolvedValue({ ...companyAnalytics, scope: "portfolio", status: "incomplete", partial: true, mode: "juxtaposed-non-consolidated", companies: ["alpha"], reconciliation: { rowCount: 1, sourceHashes: [], method: "sum", omitted: "access is partial" } });
    renderAt(<CfoCockpitView />);
    fireEvent.change(await screen.findByLabelText("Visning"), { target: { value: "portfolio" } });
    expect(await screen.findByText("Portefølje — ikke konsolideret")).toBeInTheDocument();
    expect(screen.getByText(/Ufuldstændigt udsnit/)).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Analyseret bevægelse" })).not.toBeInTheDocument();
  });

  test("shows unsupported group reports as blockers, not zero totals", async () => {
    cfoAnalytics.mockResolvedValue({ ok: true, schemaVersion: "rentemester-cfo-analytics-v1", scope: "group", status: "unsupported", asOf: "2026-08-30", limitations: ["blocked"], group: { status: "blocked", blockers: ["approved profile is incomplete"] } });
    groupReportProfiles.mockResolvedValue({ profiles: [{ id: "group-1", currency: "DKK" }] });
    renderAt(<CfoCockpitView />);
    fireEvent.change(await screen.findByLabelText("Visning"), { target: { value: "group" } });
    expect(await screen.findByText("Koncernrapporten understøttes ikke for dette udsnit.")).toBeInTheDocument();
    expect(screen.getByText("approved profile is incomplete")).toBeInTheDocument();
    expect(screen.queryByText("0,00 kr.")).not.toBeInTheDocument();
  });

  test("keeps an empty source result distinct from zero", async () => {
    cfoAnalytics.mockResolvedValue({ ...companyAnalytics, rows: [], reconciliation: { ...companyAnalytics.reconciliation, rowCount: 0, amountByCurrency: {} }, freshness: [] });
    renderAt(<CfoCockpitView />);
    expect(await screen.findByText("Ingen kildeposteringer i den valgte periode.")).toBeInTheDocument();
    expect(screen.queryByText("0,00 kr.")).not.toBeInTheDocument();
    expect(screen.getByText(/2 bogførte poster uden bilag · 1 åbne undtagelser/)).toBeInTheDocument();
  });
});


test("native signed money and immutable source provenance render exactly without float conversion", async () => {
  companies.mockResolvedValue([{slug:"alpha",name:"Alpha ApS",archived:false}]);
  cfoAnalytics.mockResolvedValue({...companyAnalytics,reconciliation:{...companyAnalytics.reconciliation,amountByCurrency:{EUR:"-90071992547409.93"}},rows:[{...companyAnalytics.rows[0],currency:"EUR",amount:"-90071992547409.93",documentPartyName:"Extracted source only",partyLinkProvenance:{linkId:19,role:"supplier",evidenceKind:"reviewed",planHash:"c".repeat(64),linkedAt:"2026-08-30T10:00:00Z"},dimensions:[{dimensionId:"project",memberId:"synthetic",amountMinor:-1234,currency:"EUR"}]}]});
  renderAt(<CfoCockpitView/>);
  const rows=await screen.findByRole("region",{name:"Kildeposter"});
  expect(within(rows).getByText("-90.071.992.547.409,93 EUR")).toBeInTheDocument();
  fireEvent.click(within(rows).getByText("Kildedetaljer journal:1:line:1"));
  expect(within(rows).getByText(/Konto: 4200 · Køb/)).toBeInTheDocument();
  expect(within(rows).getByText(/Udtrukket bilagsnavn.*Extracted source only/)).toBeInTheDocument();
  expect(within(rows).getByText(/Partstilknytning 19/)).toHaveTextContent("supplier");
  expect(within(rows).getByText(/Dimension project: synthetic/)).toHaveTextContent("-1234 mindsteenheder EUR");
  expect(within(rows).getByRole("link",{name:"Åbn originalbilag"})).toHaveAttribute("href","/api/companies/alpha/documents/7/file");
});

test("company and date changes suppress late previous source responses and reset pagination", async () => {
  companies.mockResolvedValue([{slug:"alpha",name:"Alpha ApS",archived:false},{slug:"beta",name:"Beta ApS",archived:false}]);
  let finishOld!:(value:unknown)=>void;
  cfoAnalytics.mockImplementation((query:any)=>query.companySlug==="alpha"?new Promise(r=>{finishOld=r;}):Promise.resolve({...companyAnalytics,companies:["beta"],rows:[{...companyAnalytics.rows[0],companySlug:"beta",text:"Current beta source"}],page:{limit:1,nextCursor:query.cursor?null:"source-cursor"}}));
  renderAt(<CfoCockpitView/>);
  await waitFor(()=>expect(cfoAnalytics).toHaveBeenCalled());
  fireEvent.change(screen.getByLabelText("Virksomhed"),{target:{value:"beta"}});
  expect(await screen.findByText("Virksomhed: Beta ApS")).toBeInTheDocument();
  finishOld(companyAnalytics);
  await Promise.resolve();
  expect(screen.queryByText("Virksomhed: Alpha ApS")).toBeNull();
  fireEvent.click(screen.getByRole("button",{name:"Næste kildeposter"}));
  await waitFor(()=>expect(cfoAnalytics).toHaveBeenLastCalledWith(expect.objectContaining({companySlug:"beta",cursor:"source-cursor"}),expect.anything()));
  fireEvent.change(screen.getByLabelText("Fra dato"),{target:{value:"2026-06-01"}});
  await waitFor(()=>expect(cfoAnalytics).toHaveBeenLastCalledWith(expect.objectContaining({companySlug:"beta",from:"2026-06-01"}),expect.anything()));
  expect(cfoAnalytics.mock.calls.at(-1)![0].cursor).toBeUndefined();
});

test("portfolio default omits implicit sums and unavailable evidence never renders ready zeros", async () => {
  cfoAnalytics.mockResolvedValue({...companyAnalytics,scope:"portfolio",mode:"juxtaposed-non-consolidated",aggregate:"none",rows:[companyAnalytics.rows[0],{...companyAnalytics.rows[0],companySlug:"beta",amount:"-12.34"}],evidenceCompleteness:[{companySlug:"alpha",status:"unavailable",reason:"Current source cannot be verified"}]});
  renderAt(<CfoCockpitView/>);
  fireEvent.change(await screen.findByLabelText("Visning"),{target:{value:"portfolio"}});
  await screen.findByText("Portefølje — ikke konsolideret");
  expect(screen.queryByRole("heading",{name:"Analyseret bevægelse"})).toBeNull();
  expect(screen.getByText(/Ikke tilgængelig: Current source cannot be verified/)).toBeInTheDocument();
  expect(screen.queryByText(/0 bogførte poster uden bilag/)).toBeNull();
  expect(cfoAnalytics.mock.calls.at(-1)![0].aggregate).toBeUndefined();
});

test("group profile selection binds the requested profile and missing profiles never request a report", async () => {
  groupReportProfiles.mockResolvedValue({profiles:[]});
  const view=renderAt(<CfoCockpitView/>);
  fireEvent.change(await screen.findByLabelText("Visning"),{target:{value:"group"}});
  await screen.findByText("Ingen godkendt koncernrapportprofil er tilgængelig pr. den valgte dato.");
  expect(cfoAnalytics.mock.calls.some(([q]:any[])=>q.scope==="group")).toBe(false);
  view.unmount();
  groupReportProfiles.mockResolvedValue({profiles:[{id:"first",currency:"DKK"},{id:"second",currency:"EUR"}]});
  cfoAnalytics.mockResolvedValue({ok:true,scope:"group",status:"unsupported",asOf:"2026-08-30",limitations:[],group:{status:"blocked",blockers:["Current profile proof unavailable"],consolidatedFigures:null}});
  renderAt(<CfoCockpitView/>);
  fireEvent.change(await screen.findByLabelText("Visning"),{target:{value:"group"}});
  await screen.findByRole("option",{name:"second · EUR"});
  fireEvent.change(screen.getByLabelText("Koncernprofil"),{target:{value:"second"}});
  await waitFor(()=>expect(cfoAnalytics).toHaveBeenLastCalledWith(expect.objectContaining({scope:"group",groupProfileId:"second"}),expect.anything()));
  expect(await screen.findByText("Current profile proof unavailable")).toBeInTheDocument();
});


test("CFO transport preserves repeated company and source filters plus exact monetary response", async () => {
  const {cfoAnalyticsApi}=await import("../lib/api/cfo-analytics");
  const response={...companyAnalytics,rows:[{...companyAnalytics.rows[0],amount:"-90071992547409.93"}]};
  const fetcher=vi.spyOn(globalThis,"fetch").mockResolvedValue(new Response(JSON.stringify(response),{headers:{"content-type":"application/json"}}));
  const result=await cfoAnalyticsApi.cfoAnalytics({scope:"portfolio",from:"2026-01-01",to:"2026-08-30",companySlug:["alpha","beta"],account:"4200",party:"party:source",dimension:"project:one",currency:"EUR",aggregate:"sum",cursor:"exact-source-cursor",limit:1});
  const query=new URL(String(fetcher.mock.calls[0]![0]),"http://localhost").searchParams;
  expect(query.getAll("companySlug")).toEqual(["alpha","beta"]);
  expect(Object.fromEntries(query.entries())).toMatchObject({account:"4200",party:"party:source",dimension:"project:one",currency:"EUR",aggregate:"sum",cursor:"exact-source-cursor",limit:"1"});
  expect(result).toEqual(response);
});
