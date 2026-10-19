import { afterEach, beforeEach, expect, mock, test, vi } from "bun:test";
import { StrictMode, type ReactNode } from "react";
import { screen, waitFor, within, render } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { renderAt } from "../test/render";
import { mockFetch, payables } from "../test/fixtures";
import { restoreGlobals } from "../test/globals";
const user = {id:"synthetic-owner",email:"owner@example.invalid",emailVerified:true,twoFactorEnabled:true};
mock.module("../lib/auth-client",()=>({authClient:{getSession:async()=>({data:{user,session:{id:"synthetic-session"}},error:null})}}));
const {AuthProvider,useAuth}=await import("../lib/auth-context");
const {MutationMemoryProvider}=await import("../lib/mutation-memory");
const {DirectBankPayableCorrectionModal}=await import("./DirectBankPayableCorrectionModal");
const {PayablesView}=await import("../views/PayablesView");
const {api}=await import("../lib/api");
const candidate={documentId:201,documentNo:"DOC-SYN",invoiceNo:"SYN-201",invoiceDate:"2026-05-01",supplierName:"Synthetic supplier",amountIncVat:625,vatAmount:125,currency:"DKK",bankTransactionId:777,bankDate:"2026-05-20",originalJournalEntryId:9,originalJournalEntryNo:"2026-000009"};
const model=payables({unregisteredDocuments:[],directBankCorrectionCandidates:[candidate]});
const plan={documentId:201,bankTransactionId:777,billDate:"2026-05-01",dueDate:"2026-06-01",expenseAccountNo:"3000",vatTreatment:"standard",schemaVersion:"rentemester-direct-bank-purchase-payable-correction-v1",planHash:"a".repeat(64),documentHash:"b".repeat(64),originalJournalHash:"c".repeat(64),originalJournalEntryId:9,originalJournalEntryNo:"2026-000009",originalJournalDate:"2026-05-20",documentVatAmount:125,bankDate:"2026-05-20",bankAmount:625,summary:{document:{id:201,documentNo:"DOC-SYN",invoiceNo:"SYN-201",invoiceDate:"2026-05-01",supplierName:"Synthetic supplier",currency:"DKK",grossAmount:625,vatAmount:125},bank:{id:777,date:"2026-05-20",text:"Synthetic source payment",currency:"DKK",signedAmount:-625,amount:625},originalJournal:{id:9,entryNo:"2026-000009",date:"2026-05-20",hash:"c".repeat(64)},expenseAccount:{accountNo:"3000",name:"Synthetic expense"},effectiveVatTreatment:"standard"}};
const applied=vi.fn(), closed=vi.fn();
function Ready({children}:{children:ReactNode}) {return useAuth().context?<>{children}</>:null;}
function providers(children:ReactNode){return <StrictMode><AuthProvider hosted backend="rust"><MutationMemoryProvider><Ready>{children}</Ready></MutationMemoryProvider></AuthProvider></StrictMode>;}
function mount(){return renderAt(providers(<DirectBankPayableCorrectionModal slug="acme-aps" payables={model} onApplied={applied} onClose={closed}/>),{route:"/companies/acme-aps/leverandoerfaktura",path:"/companies/:slug/leverandoerfaktura"});}
function setup(extra:Record<string,unknown>={}) {mockFetch({"GET /api/me":{user,workspaceRole:"member",companies:[{slug:"acme-aps",name:"Synthetic",role:"owner",archived:false}]},"GET /api/companies/acme-aps/payables":{payables:model},"POST /api/companies/acme-aps/payables/direct-bank-correction/plan":{plan},"POST /api/companies/acme-aps/payables/direct-bank-correction/apply":{correction:{ok:true,idempotent:false,id:1,payableId:2,paymentId:3,settlementJournalEntryId:4,errors:[]}},...extra});}
const applies=()=>(fetch as any).mock.calls.filter(([u,i]:any[])=>String(u).endsWith("/direct-bank-correction/apply")&&i?.method==="POST");
async function choose(){await screen.findByRole("dialog",{name:"Ret direkte bankkøb"});await userEvent.selectOptions(screen.getByLabelText("Bilag"),"201:777");await userEvent.type(screen.getByLabelText("Forfaldsdato"),"2026-06-01");await userEvent.selectOptions(screen.getByLabelText("Udgiftskonto"),"3000");await userEvent.selectOptions(screen.getByLabelText("Momsbehandling"),"standard");await userEvent.type(screen.getByLabelText("Begrundelse"),"Reviewed synthetic invoice and settlement dates");}
async function review(){await choose();await userEvent.click(screen.getByRole("button",{name:"Opret plan"}));await screen.findByRole("region",{name:"Gennemgå korrektionen"});}
async function confirm(){await userEvent.click(screen.getByRole("checkbox",{name:"Jeg har gennemgået bilaget, den oprindelige postering og korrektionen"}));await userEvent.click(screen.getByRole("button",{name:"Bekræft korrektion"}));}
beforeEach(()=>{sessionStorage.clear();applied.mockClear();closed.mockClear();});
afterEach(()=>{vi.restoreAllMocks();restoreGlobals();sessionStorage.clear();});

test("native correction is available for dedicated booked candidates even when ordinary registration list is empty",async()=>{
 setup();renderAt(providers(<PayablesView/>),{route:"/companies/acme-aps/leverandoerfaktura",path:"/companies/:slug/leverandoerfaktura"});const button=await screen.findByRole("button",{name:"Ret direkte bankkøb"});expect(button).toBeEnabled();await userEvent.click(button);expect(screen.getByLabelText("Bilag")).toHaveValue("");expect(screen.getByLabelText("Udgiftskonto")).toHaveValue("");expect(screen.getByLabelText("Momsbehandling")).toHaveValue("");expect(screen.getByRole("button",{name:"Opret plan"})).toBeDisabled();
});
for(const candidates of [undefined,[]]) test(`native ${candidates===undefined?"missing":"empty"} candidates never fall back to ordinary unregistered documents`,async()=>{
 setup({"GET /api/companies/acme-aps/payables":{payables:payables({directBankCorrectionCandidates:candidates})}});renderAt(providers(<PayablesView/>),{route:"/companies/acme-aps/leverandoerfaktura",path:"/companies/:slug/leverandoerfaktura"});expect(await screen.findByRole("button",{name:"Ret direkte bankkøb"})).toBeDisabled();expect(screen.getByRole("button",{name:"Registrér leverandørfaktura"})).toBeEnabled();
});
test("StrictMode reviews real source/date/account/VAT/full hash before one exact confirmed correction",async()=>{
 setup();mount();await review();expect(screen.getByLabelText("Banktransaktions-id")).toHaveValue("777");expect(screen.getByLabelText("Fakturadato")).toHaveValue("2026-05-01");expect(screen.getByLabelText("Plan-hash")).toHaveValue(plan.planHash);const region=screen.getByRole("region",{name:"Gennemgå korrektionen"});expect(region).toHaveTextContent("Synthetic source payment");expect(region).toHaveTextContent("2026-05-20");expect(region).toHaveTextContent("Synthetic expense");expect(within(region).getByRole("link",{name:"Åbn originalbilag"})).toHaveAttribute("href","/api/companies/acme-aps/documents/201/file");expect(within(region).getByRole("link",{name:"Åbn oprindelig postering"})).toHaveAttribute("href","/companies/acme-aps/posteringer?journalEntryId=9");expect(applies()).toHaveLength(0);expect(screen.getByRole("button",{name:"Bekræft korrektion"})).toBeDisabled();await confirm();await waitFor(()=>expect(closed).toHaveBeenCalledTimes(1));expect(applies()).toHaveLength(1);const init=applies()[0][1];expect(JSON.parse(init.body)).toMatchObject({documentId:201,bankTransactionId:777,billDate:"2026-05-01",dueDate:"2026-06-01",expenseAccountNo:"3000",vatTreatment:"standard",planHash:plan.planHash,reason:"Reviewed synthetic invoice and settlement dates",confirm:true});expect(init.headers["idempotency-key"]).toBe(JSON.parse(init.body).idempotencyKey);
});
test("changed source reply is not adopted as the reviewed plan",async()=>{
 setup({"POST /api/companies/acme-aps/payables/direct-bank-correction/plan":{plan:{...plan,bankTransactionId:778}}});mount();await choose();await userEvent.click(screen.getByRole("button",{name:"Opret plan"}));await screen.findByText(/Serverens plan svarer ikke til de valgte kilder/);expect(screen.queryByRole("region",{name:"Gennemgå korrektionen"})).toBeNull();expect(applies()).toHaveLength(0);
});
test("editing due date invalidates reviewed plan and confirmation",async()=>{
 setup();mount();await review();await userEvent.click(screen.getByRole("checkbox",{name:"Jeg har gennemgået bilaget, den oprindelige postering og korrektionen"}));await userEvent.clear(screen.getByLabelText("Forfaldsdato"));expect(screen.queryByRole("region",{name:"Gennemgå korrektionen"})).toBeNull();expect(screen.getByRole("button",{name:"Bekræft korrektion"})).toBeDisabled();expect(applies()).toHaveLength(0);
});
test("plan read failures are never uncertain mutations",async()=>{
 setup({"POST /api/companies/acme-aps/payables/direct-bank-correction/plan":{__error:{code:"network",message:"Source unavailable"}}});mount();await choose();await userEvent.click(screen.getByRole("button",{name:"Opret plan"}));await screen.findByText("Source unavailable");expect(screen.getByLabelText("Udgiftskonto")).toBeEnabled();expect(JSON.parse(sessionStorage.getItem("rentemester:uncertain-operations:v1")!)).toHaveLength(0);expect(applies()).toHaveLength(0);
});
for(const code of ["internal","conflict"]) test(`unknown ${code} correction freezes reviewed input/key and status read never repeats it`,async()=>{
 setup({"POST /api/companies/acme-aps/payables/direct-bank-correction/apply":{__error:{code,message:"Unconfirmed correction"}}});mount();await review();await confirm();await screen.findByText("Unconfirmed correction");expect(applies()).toHaveLength(1);const body=applies()[0][1].body;expect(screen.getByLabelText("Begrundelse")).toBeDisabled();expect(screen.getByLabelText("Udgiftskonto")).toBeDisabled();expect(screen.getByLabelText("Plan-hash")).toHaveValue(plan.planHash);await userEvent.click(screen.getByRole("button",{name:"Kontrollér status"}));expect(applied).toHaveBeenCalled();expect(applies()).toHaveLength(1);expect(applies()[0][1].body).toBe(body);expect(closed).not.toHaveBeenCalled();expect(sessionStorage.getItem("rentemester:uncertain-operations:v1")).not.toContain("Reviewed synthetic");
});
test("definite business refusal permits correction of inputs without an unknown outcome block",async()=>{
 setup({"POST /api/companies/acme-aps/payables/direct-bank-correction/apply":{__error:{code:"source_changed",message:"Current period refused"}}});mount();await review();await confirm();await screen.findByText("Current period refused");expect(screen.getByLabelText("Udgiftskonto")).toBeEnabled();expect(screen.queryByRole("region",{name:"Gennemgå korrektionen"})).toBeNull();expect(JSON.parse(sessionStorage.getItem("rentemester:uncertain-operations:v1")!)).toHaveLength(0);expect(applies()).toHaveLength(1);
});
test("late plan result after company switch is discarded",async()=>{
 setup();let resolve!:(value:any)=>void;vi.spyOn(api,"planDirectBankPayableCorrection").mockImplementation(()=>new Promise(r=>{resolve=r;}));const content=(slug:string)=><MemoryRouter>{providers(<DirectBankPayableCorrectionModal slug={slug} payables={model} onApplied={applied} onClose={closed}/>)}</MemoryRouter>;const view=render(content("acme-aps"));await choose();await userEvent.click(screen.getByRole("button",{name:"Opret plan"}));view.rerender(content("synthetic-other"));resolve(plan);await waitFor(()=>expect(screen.getByLabelText("Bilag")).toHaveValue(""));expect(screen.queryByRole("region",{name:"Gennemgå korrektionen"})).toBeNull();expect(applies()).toHaveLength(0);
});
