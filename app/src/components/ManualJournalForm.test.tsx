import { expect, test } from "bun:test";
import { render, screen, fireEvent, within, act } from "@testing-library/react";
import { ManualJournalForm } from "./ManualJournalForm";
import { stubGlobal } from "../test/globals";
import { MemoryRouter } from "react-router-dom";
import { MutationMemoryProvider } from "../lib/mutation-memory";
const accounts = [
  { accountNo: "a", name: "Indtægt", type: "income", active: true, allowDirectPosting: true },
  { accountNo: "b", name: "Bank", type: "asset", active: true, allowDirectPosting: true },
  { accountNo: "c", name: "Ukendt", active: true, allowDirectPosting: true },
];
function setup(currency = "DKK") {
  const bodies: Array<{ input: { request: { lines: unknown[] } } }> = [];
  stubGlobal("fetch", async (_input: RequestInfo | URL, init?: RequestInit) => {
    if (String(_input).endsWith("/recovery")) return new Response(JSON.stringify({ok:true,receipts:[],acknowledgedReceiptId:1}));
    if (init?.method === "POST") { bodies.push(JSON.parse(String(init.body))); return new Response(JSON.stringify({ ok: true, receipt: {receiptId: 1} })); }
    return new Response(JSON.stringify({ ok: true, documents: { documents: [{ id: 1, documentNo: "D1", filename: "Bilag" }] } }));
  });
  render(<ManualJournalForm slug="synthetic" currency={currency} options={{ ruleVersion: "rules", accounts }} onSaved={() => {}} onClose={() => {}} />);
  return bodies;
}
async function fill(credit = "a", document = true) {
  await screen.findByText("D1 · Bilag");
  for (const [label, value] of [["Posteringstekst", "Salg"], ["Debetkonto", credit === "b" ? "a" : "b"], ["Kreditkonto", credit]]) fireEvent.change(screen.getByLabelText(label!), { target: { value } });
  fireEvent.change(screen.getByLabelText(/^Beløb i/), { target: { value: "1000" } });
  if (document) fireEvent.change(screen.getByLabelText("Bilag (valgfrit)"), { target: { value: "1" } });
}
function oss() { fireEvent.change(screen.getByLabelText("Momsklassifikation"), { target: { value: "oss" } }); }
function review() { fireEvent.click(screen.getByRole("button", { name: "Gennemse postering" })); }
async function confirm() { const button = within(await screen.findByRole("dialog")).getByRole("button", { name: "Bogfør postering" }); await act(async () => { fireEvent.click(button); }); }
test("explicit OSS is reviewed and sent only on the income credit line", async () => {
 const bodies = setup(); await fill(); oss(); review(); expect(await screen.findByText(/Momsklassifikation: OSS-salg til EU-forbruger/)).toBeInTheDocument(); await confirm();
 expect(bodies[0]!.input.request.lines).toEqual([{ accountNo: "b", debitAmount: "1000.00" }, { accountNo: "a", creditAmount: "1000.00", vatCode: "OSS_EU_CONSUMER" }]);
});
test("ordinary posting stays unclassified with an income account", async () => {
 const bodies = setup(); await fill(); review(); await confirm(); expect(bodies[0]!.input.request.lines).toEqual([{ accountNo: "b", debitAmount: "1000.00" }, { accountNo: "a", creditAmount: "1000.00" }]);
});
test("OSS requires the original document", async () => {
 setup(); await fill("a", false); oss(); review(); expect(await screen.findByRole("alert")).toHaveTextContent("Vælg et bilag"); expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
});
test.each(["b", "c"])("OSS refuses non-income or unknown account metadata %s", async credit => {
 setup(); await fill(credit); oss(); review(); expect(await screen.findByRole("alert")).toHaveTextContent("Vælg en indtægtskonto"); expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
});
test("OSS refuses foreign currency before confirmation", async () => {
 setup("EUR"); await fill(); oss(); review(); expect(await screen.findByRole("alert")).toHaveTextContent("kun i DKK"); expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
});

test("a lost posting reply stays blocked after reopening the form and reloading its memory", async () => {
  sessionStorage.clear();
  let writes = 0;
  let refreshes = 0;
  stubGlobal("fetch", async (_input: RequestInfo | URL, init?: RequestInit) => {
    if (String(_input).endsWith("/recovery")) return new Response(JSON.stringify({ok:true,receipts:[],acknowledgedReceiptId:1}));
    if (init?.method === "POST") { writes++; throw new TypeError("Lost accepted reply"); }
    return new Response(JSON.stringify({ ok: true, documents: { documents: [{ id: 1, documentNo: "D1", filename: "Bilag" }] } }));
  });
  const form = (path: string) => <MemoryRouter initialEntries={[path]}><MutationMemoryProvider><ManualJournalForm slug="synthetic" currency="DKK" options={{ ruleVersion: "rules", accounts }} onSaved={() => { refreshes++; }} onClose={() => {}} /></MutationMemoryProvider></MemoryRouter>;
  try {
    const first = render(form("/companies/synthetic/posteringer"));
    await fill(); review(); await confirm();
    expect(screen.getByRole("button", { name: "Bogfør postering" })).toBeDisabled();
    first.unmount();
    render(form("/companies/synthetic/posteringer/"));
    await fill(); review();
    expect(screen.getByRole("button", { name: "Bogfør postering" })).toBeDisabled();
    await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Kontrollér status" })); });
    expect(refreshes).toBe(1);
    expect(screen.getByRole("button", { name: "Bogfør postering" })).toBeDisabled();
    expect(writes).toBe(1);
    fireEvent.click(screen.getByRole("button", { name: "Resultatet er afklaret" }));
    fireEvent.click(screen.getByRole("button", { name: "Jeg har kontrolleret resultatet" }));
    expect(screen.getByRole("button", { name: "Bogfør postering" })).toBeEnabled();
    expect(writes).toBe(1);
  } finally { sessionStorage.clear(); }
});

test("server receipt blocks a fresh form after all tab memory is gone, and explicit acknowledgement sends no posting",async()=>{
  sessionStorage.clear();let acknowledged=false;const posts:string[]=[];
  stubGlobal("fetch",async(input:RequestInfo|URL,init?:RequestInit)=>{
    const path=String(input);
    if(init?.method==="POST"){posts.push(path);expect(JSON.parse(String(init.body))).toEqual({confirm:true,receiptId:17});acknowledged=true;return new Response(JSON.stringify({ok:true,acknowledgedReceiptId:17}));}
    if(path.endsWith("/recovery"))return new Response(JSON.stringify({ok:true,receipts:acknowledged?[]:[{receiptId:17,createdAt:"2026-10-20T10:00:00Z",resultAvailable:true,result:{entryId:9,entryNo:"S9",entryHash:"synthetic-hash"}}]}));
    return new Response(JSON.stringify({ok:true,documents:{documents:[]}}));
  });
  render(<ManualJournalForm slug="synthetic" currency="DKK" options={{ruleVersion:"rules",accounts}} onSaved={()=>{}} onClose={()=>{}}/>);
  await screen.findByText(/Postering S9/);
  expect(screen.queryByLabelText("Posteringstekst")).toBeNull();
  const ack=screen.getByRole("button",{name:"Bekræft kvittering"});expect(ack).toBeDisabled();
  fireEvent.click(screen.getByRole("checkbox"));
  await act(async()=>{fireEvent.click(ack);});
  await screen.findByLabelText("Posteringstekst");
  expect(posts).toEqual(["/api/companies/synthetic/manual-journal/recovery"]);
});
test("unavailable receipt read and lost acknowledgement keep new posting unavailable",async()=>{
  sessionStorage.clear();let available=false;let writes=0;
  stubGlobal("fetch",async(_input:RequestInfo|URL,init?:RequestInit)=>{
    if(init?.method==="POST"){writes++;throw new TypeError("ack reply lost");}
    if(!available)throw new TypeError("offline");
    return new Response(JSON.stringify({ok:true,receipts:[{receiptId:18,createdAt:"2026-10-20T10:00:00Z",resultAvailable:false,result:null}]}));
  });
  render(<ManualJournalForm slug="synthetic" currency="DKK" options={{ruleVersion:"rules",accounts}} onSaved={()=>{}} onClose={()=>{}}/>);
  await screen.findByText(/Tidligere bogføringskvitteringer kunne ikke kontrolleres/);expect(screen.queryByLabelText("Posteringstekst")).toBeNull();available=true;
  await act(async()=>{fireEvent.click(screen.getByRole("button",{name:"Kontrollér status"}));});
  await screen.findByText(/Den detaljerede kvittering er udløbet/);fireEvent.click(screen.getByRole("checkbox"));
  await act(async()=>{fireEvent.click(screen.getByRole("button",{name:"Bekræft kvittering"}));});
  await screen.findByText(/Kvitteringen kunne ikke bekræftes/);expect(writes).toBe(1);expect(screen.queryByLabelText("Posteringstekst")).toBeNull();
});
