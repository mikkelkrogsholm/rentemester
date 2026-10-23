import { describe, expect, test, vi } from "bun:test";
import { screen, fireEvent, waitFor, within } from "@testing-library/react";
import { AssetsView } from "../views/AssetsView";
import { AssetDisposalDialog } from "./AssetDisposalDialog";
import { assets, documents, mockFetch } from "../test/fixtures";
import { disposalPreview, disposalReceipt } from "../test/fixtures/assets";
import { renderAt } from "../test/render";
import { assetsApi } from "../lib/api/assets";

function routes() { return {
  "GET /api/companies/acme-aps/documents": {documents:documents()},
  "GET /api/companies/acme-aps/accounts": {accounts:{accounts:[{accountNo:"6100",name:"Loss",type:"expense",active:true}]}},
  "POST /api/companies/acme-aps/assets/1/disposal/preview": {disposalPreview},
  "POST /api/companies/acme-aps/assets/1/dispose": {disposal:disposalReceipt},
  "GET /api/companies/acme-aps/assets/1/disposal-receipt": {disposal:disposalReceipt},
}; }
function show() {return renderAt(<AssetDisposalDialog slug="acme-aps" asset={assets().assets[0]!} currency="DKK" onClose={vi.fn()} onPosted={vi.fn()} />);}
async function review() {
  await screen.findByRole("option", {name:"6100 · Loss"});
  fireEvent.change(screen.getByLabelText("Bogføringsdato"),{target:{value:"2026-10-20"}});
  fireEvent.change(screen.getByLabelText("Bilag for afhændelse"),{target:{value:"1"}});
  fireEvent.change(screen.getByLabelText(/Tabskonto/),{target:{value:"6100"}});
  fireEvent.change(screen.getByLabelText("Begrundelse"),{target:{value:"Retired equipment"}});
  fireEvent.click(screen.getByRole("button",{name:"Forhåndsvis afhændelse"}));
  await screen.findByText(disposalPreview.planHash);
}
describe("asset_disposal frozen UI and wire",()=>{
 test("server preview is inert and explicit confirmation sends reviewed hash and sticky key",async()=>{
  mockFetch(routes());show();await review();
  const calls=()=> (fetch as unknown as ReturnType<typeof vi.fn>).mock.calls as [string,RequestInit][];
  expect(calls().filter(([u])=>u.endsWith("/dispose"))).toHaveLength(0);
  fireEvent.click(screen.getByRole("button",{name:"Gennemgå og bekræft"}));
  fireEvent.click(screen.getByRole("button",{name:"Bogfør afhændelse"}));
  await screen.findByText(/Ingen senere afskrivning/);
  const writes=calls().filter(([u])=>u.endsWith("/dispose"));expect(writes).toHaveLength(1);
  const body=JSON.parse(String(writes[0]![1].body));
  expect(body.expectedPlanHash).toBe(disposalPreview.planHash);expect(body.confirm).toBe(true);expect(body.idempotencyKey.length).toBeGreaterThanOrEqual(8);
  expect(body.cost).toBeUndefined();
 });
 test("malformed write success stays unknown and does not auto retry",async()=>{
  mockFetch({...routes(),"POST /api/companies/acme-aps/assets/1/dispose":{disposal:{assetId:1}}});show();await review();
  fireEvent.click(screen.getByRole("button",{name:"Gennemgå og bekræft"}));fireEvent.click(screen.getByRole("button",{name:"Bogfør afhændelse"}));
  await waitFor(()=>expect(screen.getByRole("button",{name:"Bogfør afhændelse"})).toBeDisabled());
  expect(screen.getAllByText(/Serverens resultat kunne ikke/).length).toBeGreaterThan(0);
 });
 test("strict request and receipt refuse unknown fields and wrong identity",async()=>{
  mockFetch({...routes(),"GET /api/companies/acme-aps/assets/1/disposal-receipt":{disposal:{...disposalReceipt,assetId:2}}});
  await expect(assetsApi.assetDisposalPreview("acme-aps",1,{transactionDate:"2026-10-20",documentId:1,reason:"Retired",amount:10} as never)).rejects.toThrow();
  await expect(assetsApi.assetDisposalReceipt("acme-aps",1,"synthetic-key")).rejects.toThrow();
 });
 test("zero carrying normalizes unused loss account and subcent outputs refuse",async()=>{
  const zero={...disposalPreview,accumulatedDepreciation:48000,carryingValue:0,lossAccountNo:null,lines:[{accountNo:"5800",debit:0,credit:48000},{accountNo:"5810",debit:48000,credit:0}]};
  mockFetch({...routes(),"POST /api/companies/acme-aps/assets/1/disposal/preview":{disposalPreview:zero}});
  const input={transactionDate:"2026-10-20",documentId:1,reason:"Retired equipment",lossAccountNo:"6100"};
  expect((await assetsApi.assetDisposalPreview("acme-aps",1,input)).lossAccountNo).toBeNull();
  mockFetch({...routes(),"POST /api/companies/acme-aps/assets/1/disposal/preview":{disposalPreview:{...zero,cost:0.001}}});
  await expect(assetsApi.assetDisposalPreview("acme-aps",1,input)).rejects.toThrow();
 });
 test("lost committed response then remount on disposed report retains read-only receipt recovery",async()=>{
  mockFetch({...routes(),"POST /api/companies/acme-aps/assets/1/dispose":{disposal:{assetId:1}}});
  const mounted=show();await review();
  fireEvent.click(screen.getByRole("button",{name:"Gennemgå og bekræft"}));fireEvent.click(screen.getByRole("button",{name:"Bogfør afhændelse"}));
  await waitFor(()=>expect(screen.getByRole("button",{name:"Bogfør afhændelse"})).toBeDisabled());
  const key=sessionStorage.getItem("asset-disposal-key:/companies/acme-aps/assets/1");expect(key).toBeTruthy();mounted.unmount();
  const report=assets();report.assets[0]={...report.assets[0]!,status:"disposed",netBookValue:0,disposalDate:"2026-10-20",disposalJournalEntryNo:"2026-20"};
  mockFetch({...routes(),"GET /api/companies/acme-aps/assets":{assets:report}});
  renderAt(<AssetsView/>,{route:"/companies/acme-aps/anlaeg",path:"/companies/:slug/anlaeg"});
  fireEvent.click(await screen.findByRole("button",{name:"Hent afhændelseskvittering"}));
  const dialog=within(screen.getByRole("dialog"));
  expect(dialog.getByRole("button",{name:"Forhåndsvis afhændelse"})).toBeDisabled();
  fireEvent.click(dialog.getByRole("button",{name:"Hent afhændelseskvittering"}));
  await dialog.findByText(/Afhændelse #1/);
  const calls=(fetch as unknown as ReturnType<typeof vi.fn>).mock.calls as [string,RequestInit][];
  expect(calls.some(([u])=>u.includes("disposal-receipt?key="+encodeURIComponent(key!)))).toBe(true);
  expect(calls.filter(([u])=>u.endsWith("/dispose"))).toHaveLength(0);
 });

});
