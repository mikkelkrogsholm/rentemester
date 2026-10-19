import { afterEach, beforeEach, expect, test, vi } from "bun:test";
import { StrictMode } from "react";
import { act, screen, within, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderAt } from "../test/render";
import { ArchiveImportPanel } from "./ArchiveImportPanel";
import { archiveImportApi } from "../lib/api/archive-import";
import { ApiError } from "../lib/api/_shared";
import { MutationMemoryProvider } from "../lib/mutation-memory";
import type { ArchiveSource, ArchiveImportPlan, ClosedArchiveYear } from "../lib/types/archive-import";
const csv="\uFEFFKonto;Kontonavn;Dato;Bilag;Bilagstype;Tekst;Momstype;Beløb;Saldo\r\n1000;Synthetic;2024-06-01;7;Finans;\"source;quoted\";;-1,235000;\r\n";
const h="a".repeat(64);let last:ArchiveSource;let lastPlan:ArchiveImportPlan;
const listYear=():ClosedArchiveYear=>({fiscalYear:2024,sourceSystem:"dinero",postingCount:1,balanceCount:0,importedAt:"2026-10-20T12:00:00Z",importId:41,sourceHash:lastPlan.source.sha256,parserContract:"rentemester-native-closed-history/1",declaredCurrency:"DKK",hasBalances:false});
const decoded=(value:string)=>Uint8Array.from(atob(value),c=>c.charCodeAt(0));
async function sha(bytes:Uint8Array){return Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256",bytes as Uint8Array<ArrayBuffer>)),v=>v.toString(16).padStart(2,"0")).join("");}
async function planned(_slug:string,source:ArchiveSource){last=source;const bytes=decoded(source.contentBase64);lastPlan={format:"rentemester-native-closed-history/1",status:"ready",planHash:h,source:{sha256:await sha(bytes),sizeBytes:bytes.length},declaredCurrency:"DKK",currencyDeclaration:"operator",excludedLatestYear:2025,years:[{fiscalYear:2024,postingCount:1,balanceCount:0,hasBalances:false,status:"new"}],limitations:[]};return structuredClone(lastPlan);}
function receipt(){return{importId:41,sourceSha256:lastPlan.source.sha256,planHash:h,excludedLatestYear:2025,declaredCurrency:"DKK",years:[{fiscalYear:2024,postingCount:1,balanceCount:0}]};}
function setup(){vi.spyOn(archiveImportApi,"plan").mockImplementation(planned);vi.spyOn(archiveImportApi,"list").mockResolvedValue({ok:true,years:[],errors:[]});vi.spyOn(archiveImportApi,"year").mockImplementation(async()=>({ok:true,years:[{...listYear(),postings:[{lineNo:0,accountNo:"1000",accountName:"Synthetic",transactionDate:"2024-06-01",voucher:"7",voucherType:"Finans",text:"Source text",vatType:"",amount:"-1.24",sourceAmount:"-1.235000",runningBalance:null,sourceRunningBalance:null}],balances:[]}],errors:[]}));vi.spyOn(archiveImportApi,"original").mockImplementation(async()=>({blob:new Blob([decoded(last.contentBase64)]),headers:new Headers({"content-type":"application/json"})}));return vi.spyOn(archiveImportApi,"apply").mockImplementation(async()=>receipt());}
function mount(slug="synthetic"){return renderAt(<StrictMode><MutationMemoryProvider><ArchiveImportPanel slug={slug} onChanged={()=>{}}/></MutationMemoryProvider></StrictMode>,{route:`/companies/${slug}/arkiv`});}
async function fill(){await userEvent.type(screen.getByLabelText("Historisk basisvaluta"),"DKK");await userEvent.type(screen.getByLabelText("Regnskabsår 1"),"2024");await userEvent.type(screen.getByLabelText("Regnskabsår 2"),"2025");await userEvent.upload(screen.getByLabelText("Posteringer CSV 1"),new File([csv],"Posteringer.csv",{type:"text/csv"}));await userEvent.upload(screen.getByLabelText("Posteringer CSV 2"),new File([csv.replace("2024-06-01","2025-06-01")],"Posteringer.csv",{type:"text/csv"}));}
async function review(){await fill();await userEvent.click(screen.getByRole("button",{name:"Gennemgå arkivimport"}));return screen.findByRole("dialog",{name:"Bekræft arkivimport"});}
async function confirm(dialog:HTMLElement,label="Importér afsluttede år"){await userEvent.click(within(dialog).getByRole("checkbox",{name:"Jeg bekræfter valuta og de gennemgåede arkivår"}));await userEvent.click(within(dialog).getByRole("button",{name:label}));}
beforeEach(()=>sessionStorage.clear());afterEach(()=>{vi.restoreAllMocks();sessionStorage.clear();});

test("real CSV bytes including BOM/CRLF are packaged once and highest excluded year is separately confirmed",async()=>{
 const apply=setup();mount();expect(screen.getByLabelText("Historisk basisvaluta")).toHaveValue("");const dialog=await review();expect(apply).not.toHaveBeenCalled();expect(within(dialog).getByText("Højeste år 2025 udelades")).toBeInTheDocument();expect(within(dialog).getByText("Ingen SaldoBalance-fil")).toBeInTheDocument();expect(within(dialog).getByText(/Valutaen er din erklæring/)).toBeInTheDocument();expect(within(dialog).getByRole("button",{name:"Importér afsluttede år"})).toBeDisabled();
 const packageBytes=decoded(last.contentBase64),pkg=JSON.parse(new TextDecoder().decode(packageBytes));expect(pkg.format).toBe("rentemester-dinero-archive-source/1");expect(pkg.currency).toBe("DKK");expect(pkg.files.map((f:any)=>f.path)).toEqual(["2024/Posteringer.csv","2025/Posteringer.csv"]);expect(Array.from(decoded(pkg.files[0].contentBase64))).toEqual(Array.from(new TextEncoder().encode(csv)));
 await confirm(dialog);await screen.findByRole("heading",{name:"Arkivimport gennemført"});expect(apply).toHaveBeenCalledTimes(1);expect(apply.mock.calls[0]![1]).toMatchObject({...last,planHash:h,confirm:true});expect(apply.mock.calls[0]![1].idempotencyKey).not.toBe("");expect(screen.getByRole("link",{name:"Hent kildepakke 41"})).toHaveAttribute("href","/api/companies/synthetic/archive-import/41/original");
});

test("unknown outcome requires canonical list and exact source verification before deliberate identical retry",async()=>{
 const apply=setup().mockRejectedValueOnce(new ApiError("network","Synthetic lost response",0));mount();const dialog=await review();await confirm(dialog);await screen.findByText("Synthetic lost response");const sent=structuredClone(apply.mock.calls[0]![1]);expect(within(dialog).getByRole("button",{name:"Prøv præcis samme arkivimport igen"})).toBeDisabled();vi.spyOn(archiveImportApi,"list").mockResolvedValue({ok:true,years:[listYear()],errors:[]});await userEvent.click(within(dialog).getByRole("button",{name:"Kontrollér bevaret arkiv og kilde"}));await screen.findByText(/Arkivlisten og den bevarede kilde er kontrolleret/);expect(archiveImportApi.original).toHaveBeenCalledWith("synthetic",41);expect(apply).toHaveBeenCalledTimes(1);await confirm(dialog,"Prøv præcis samme arkivimport igen");await screen.findByRole("heading",{name:"Arkivimport gennemført"});expect(apply.mock.calls[1]![1]).toEqual(sent);
});

test("reopen retains original key and plan even if a current read returns a changed retained plan",async()=>{
 const apply=setup().mockRejectedValueOnce(new ApiError("internal","Synthetic unknown",503));const first=mount();await confirm(await review());await screen.findByText("Synthetic unknown");const sent=structuredClone(apply.mock.calls[0]![1]);first.unmount();vi.spyOn(archiveImportApi,"plan").mockImplementation(async(slug,source)=>({...await planned(slug,source),planHash:"b".repeat(64)}));mount();const dialog=await review();await userEvent.click(within(dialog).getByRole("button",{name:"Kontrollér bevaret arkiv og kilde"}));await screen.findByText(/Arkivlisten viser endnu ingen installation/);await confirm(dialog,"Prøv præcis samme arkivimport igen");await screen.findByRole("heading",{name:"Arkivimport gennemført"});expect(apply.mock.calls[1]![1]).toEqual(sent);
});

test("known refusal is editable after explicit discard, malformed receipt stays uncertain",async()=>{
 const apply=setup().mockRejectedValueOnce(new ApiError("archive_conflict","Synthetic current conflict",422));mount();const dialog=await review();await confirm(dialog);await screen.findByText("Synthetic current conflict");expect(within(dialog).getByRole("button",{name:"Importér afsluttede år"})).not.toBeDisabled();apply.mockResolvedValueOnce({...receipt(),sourceSha256:"f".repeat(64)});await userEvent.click(within(dialog).getByRole("button",{name:"Importér afsluttede år"}));await screen.findByText(/Kvitteringen kunne ikke bekræftes/);expect(within(dialog).getByRole("button",{name:"Prøv præcis samme arkivimport igen"})).toBeDisabled();expect(screen.queryByRole("heading",{name:"Arkivimport gennemført"})).toBeNull();
});

test("archive list/year preserve exact six-place source values separately from report amounts",async()=>{
 setup();await planned("synthetic",{fileName:"x",contentBase64:btoa("{}")});vi.spyOn(archiveImportApi,"list").mockResolvedValue({ok:true,years:[listYear()],errors:[]});mount();await userEvent.click(await screen.findByRole("button",{name:"Vis arkivår 2024"}));const detail=await screen.findByRole("region",{name:"Arkivår 2024"});expect(within(detail).getByText("-1.235000")).toBeInTheDocument();expect(within(detail).getByText("-1.24 DKK")).toBeInTheDocument();expect(within(detail).getByText(/Ingen SaldoBalance-fil; der vises ingen opfundne/)).toBeInTheDocument();expect(within(detail).queryByText("0.00")).toBeNull();
});

test("missing declaration and mismatched plan cannot produce a write",async()=>{
 const apply=setup();mount();await userEvent.click(screen.getByRole("button",{name:"Gennemgå arkivimport"}));await screen.findByText(/Angiv den historiske basisvaluta/);expect(archiveImportApi.plan).not.toHaveBeenCalled();vi.spyOn(archiveImportApi,"plan").mockImplementation(async(slug,source)=>({...await planned(slug,source),source:{sha256:"f".repeat(64),sizeBytes:1}}));await fill();await userEvent.click(screen.getByRole("button",{name:"Gennemgå arkivimport"}));await screen.findByText(/Serverens plan kunne ikke bekræftes/);expect(apply).not.toHaveBeenCalled();expect(screen.queryByRole("dialog")).toBeNull();
});

test("company switch discards late readonly plan without exposing the old source",async()=>{
 const apply=setup();let resolve!:(plan:ArchiveImportPlan)=>void;vi.spyOn(archiveImportApi,"plan").mockImplementation(async(slug,source)=>{const plan=await planned(slug,source);return new Promise(r=>{resolve=()=>r(plan);});});const view=mount();await fill();await userEvent.click(screen.getByRole("button",{name:"Gennemgå arkivimport"}));await waitFor(()=>expect(resolve).toBeDefined());await act(async()=>{view.rerender(<StrictMode><MutationMemoryProvider><ArchiveImportPanel slug="another" onChanged={()=>{}}/></MutationMemoryProvider></StrictMode>);resolve(lastPlan);});expect(screen.getByLabelText("Historisk basisvaluta")).toHaveValue("");expect(screen.queryByRole("dialog")).toBeNull();expect(apply).not.toHaveBeenCalled();
});


test("mismatched retained original refuses reconciliation and never enables a retry",async()=>{
 const apply=setup().mockRejectedValueOnce(new ApiError("network","Synthetic loss",0));mount();const dialog=await review();await confirm(dialog);await screen.findByText("Synthetic loss");vi.spyOn(archiveImportApi,"list").mockResolvedValue({ok:true,years:[listYear()],errors:[]});vi.spyOn(archiveImportApi,"original").mockResolvedValue({blob:new Blob(["different source"]),headers:new Headers({"content-type":"application/json"})});await userEvent.click(within(dialog).getByRole("button",{name:"Kontrollér bevaret arkiv og kilde"}));await screen.findByText("Den bevarede kilde svarer ikke til det uafklarede forsøg.");expect(within(dialog).getByRole("button",{name:"Prøv præcis samme arkivimport igen"})).toBeDisabled();expect(apply).toHaveBeenCalledTimes(1);
});
