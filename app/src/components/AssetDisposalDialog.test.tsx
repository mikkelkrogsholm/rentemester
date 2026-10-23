import { describe, expect, test, vi } from "bun:test";
import { screen, fireEvent, waitFor } from "@testing-library/react";
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
function show() {renderAt(<AssetDisposalDialog slug="acme-aps" asset={assets().assets[0]!} currency="DKK" onClose={vi.fn()} onPosted={vi.fn()} />);}
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
});
