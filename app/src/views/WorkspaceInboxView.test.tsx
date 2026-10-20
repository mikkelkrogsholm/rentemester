import { afterEach, describe, expect, test, vi } from "bun:test";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { WorkspaceInboxView } from "./WorkspaceInboxView";
import { mockFetch } from "../test/fixtures";
import { restoreGlobals, stubGlobal } from "../test/globals";
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

test("mail containers and refusal receipts are download-only", async () => {
  mockFetch({"GET /api/companies":{companies:[]},"GET /api/companies/synthetic-company/workspace-inbox":{rows:[{sourceId:"mail/eml",filename:"original.eml",sha256:"a".repeat(64),transport:"imap",receivedAt:"2026-10-01",assignments:[],mail:{role:"eml"}},{sourceId:"mail-refusal",filename:"oversized.json",sha256:"b".repeat(64),transport:"imap",receivedAt:"2026-10-01",assignments:[],mail:{role:"refusal"}}]}});
  renderAt(<WorkspaceInboxView />,{route:"/companies/synthetic-company/workspace-inbox",path:"/companies/:slug/workspace-inbox"});
  const download=await screen.findByRole("link",{name:"Hent original original.eml"});
  expect(download.getAttribute("href")).toBe("/api/companies/synthetic-company/workspace-inbox/mail%2Feml/original");
  expect(download.hasAttribute("download")).toBe(true);
  expect(screen.queryByRole("button",{name:"Godkend ruting"})).toBeNull();
  expect(screen.queryByRole("button",{name:"Fuldfør handoff"})).toBeNull();
  expect(screen.queryByRole("button",{name:"Gennemgå kildeoplysninger"})).toBeNull();
});

test("a mail review for another target cannot enable handoff", async () => {
  const source={sourceId:"mail-attachment",filename:"invoice.pdf",sha256:"a".repeat(64),transport:"imap",receivedAt:"2026-10-01",assignments:[{companySlug:"first",state:"approved"}],mail:{role:"attachment"},metadataReview:{companySlug:"first",sourceHash:"a".repeat(64),ledgerUuid:"ledger-first",metadataHash:"b".repeat(64),metadata:{source:"workspace-inbox:mail-attachment",documentType:"cash_register_receipt",currency:"DKK"},reviewedBy:"user:synthetic",reviewedAt:"2026-10-01"}};
  mockFetch({"GET /api/companies":{companies:[{slug:"first",name:"First target"},{slug:"second",name:"Second target"}]},"GET /api/companies/synthetic-company/workspace-inbox":{rows:[source]}});
  renderAt(<WorkspaceInboxView />,{route:"/companies/synthetic-company/workspace-inbox",path:"/companies/:slug/workspace-inbox"});
  await screen.findByRole("option",{name:"First target"});
  fireEvent.change(screen.getByLabelText("Inbox målvirksomhed"),{target:{value:"first"}});
  expect(screen.getByRole("button",{name:"Fuldfør handoff"})).not.toBeDisabled();
  fireEvent.change(screen.getByLabelText("Inbox målvirksomhed"),{target:{value:"second"}});
  expect(screen.getByRole("button",{name:"Fuldfør handoff"})).toBeDisabled();
  expect(screen.getByText(/Gennemgangen gælder ikke det valgte mål/)).toBeTruthy();
});


test("one explicit mail review binds target and source, approves routing and reads canonical receipt", async () => {
  const source:any={sourceId:"review-mail",filename:"review.pdf",sha256:"c".repeat(64),transport:"imap",receivedAt:"2026-10-01",assignments:[],mail:{role:"attachment"},metadataReview:null};
  mockFetch({"GET /api/companies":{companies:[{slug:"target",name:"Target"}]},"GET /api/companies/synthetic-company/workspace-inbox":{rows:[source]},"GET /api/companies/synthetic-company/workspace-inbox/review-mail":{source},"GET /api/companies/target/contacts":{contacts:{vendors:[],customers:[]}},"POST /api/companies/synthetic-company/workspace-inbox/review-mail/review":{source}});
  const delegate=globalThis.fetch;
  stubGlobal("fetch",vi.fn(async(input:RequestInfo|URL,init?:RequestInit)=>{
    if(String(input).endsWith("/review")&&init?.method==="POST"){
      const body=JSON.parse(String(init.body));
      source.metadataReview={companySlug:body.companySlug,ledgerUuid:"synthetic-ledger",sourceHash:body.sourceHash,metadataHash:"d".repeat(64),metadata:body.metadata,reviewedBy:"user:reviewer",reviewedAt:"2026-10-01"};source.assignments=[{companySlug:body.companySlug,state:"approved"}];
    }
    return delegate(input,init);
  }));
  renderAt(<WorkspaceInboxView />,{route:"/companies/synthetic-company/workspace-inbox",path:"/companies/:slug/workspace-inbox"});
  await screen.findByRole("option",{name:"Target"});
  fireEvent.change(screen.getByLabelText("Inbox målvirksomhed"),{target:{value:"target"}});
  fireEvent.click(screen.getByRole("button",{name:"Gennemgå kildeoplysninger"}));
  const dialog=await screen.findByRole("dialog",{name:"Gennemgå mailbilag"});
  expect(within(dialog).queryByLabelText("Bilagsfil")).toBeNull();
  fireEvent.change(within(dialog).getByLabelText("Bilagstype"),{target:{value:"cash_register_receipt"}});
  fireEvent.change(within(dialog).getByLabelText("Valuta"),{target:{value:"DKK"}});
  fireEvent.click(within(dialog).getByLabelText("Jeg har gennemgået originalen, målvirksomheden og de indtastede kildeoplysninger."));
  fireEvent.click(within(dialog).getByRole("button",{name:"Gem gennemgåede kildeoplysninger"}));
  await within(dialog).findByText(/Kildeoplysninger og ruting er godkendt/);
  const calls=(globalThis.fetch as unknown as {mock:{calls:Array<[unknown,RequestInit]>}}).mock.calls.filter(c=>String(c[0]).endsWith("/review"));
  expect(calls).toHaveLength(1);
  expect(JSON.parse(String(calls[0][1].body))).toEqual({companySlug:"target",sourceHash:"c".repeat(64),metadata:{source:"workspace-inbox:review-mail",documentType:"cash_register_receipt",currency:"DKK"},confirm:true});
  fireEvent.click(within(dialog).getByRole("button",{name:"Luk"}));
  await waitFor(()=>expect(screen.getByRole("button",{name:"Fuldfør handoff"})).not.toBeDisabled());
});
