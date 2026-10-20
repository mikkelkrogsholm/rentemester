import {expect,test,vi} from "bun:test";
import {render,screen} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import {BankMatchSuggestionsModal} from "./BankMatchSuggestionsModal";
import {mockFetch} from "../test/fixtures";
import {bankMatchesApi} from "../lib/api/bank-matches";
import type {BankMatchKind,BankMatchRow} from "../lib/types/bank-matches";

const transaction={id:7,date:"2026-10-20",text:"Synthetic bank transaction",amount:-125,currency:"DKK"};
const path="GET /api/companies/acme-aps/bank/match-suggestions";
const row:BankMatchRow={bankTransactionId:7,date:transaction.date,text:transaction.text,amount:-125,currency:"DKK",reference:"S-7",suggestions:[],unsafeMatchReason:null};
function show(onClose=()=>{}) {return render(<BankMatchSuggestionsModal slug="acme-aps" transaction={transaction} onClose={onClose}/>);}

test("all advisory families show document links, exact scores above one and source reasons without write actions",async()=>{
 const kinds:BankMatchKind[]=["issued_invoice","purchase_sale","credit_note_refund","supplier_credit_refund"];
 mockFetch({[path]:{count:1,rows:[{...row,suggestions:kinds.map((kind,i)=>({kind,documentId:i+20,invoiceNo:`S-${i}`,confidence:i===0?1.2:0.55,reasons:[`Synthetic source reason ${i}`]}))}],errors:[]}});
 const close=vi.fn();show(close);
 await screen.findByRole("heading",{name:"Kundefaktura"});
 for(const label of ["Købsbilag","Tilbagebetaling af kreditnota","Mulig leverandørrefusion"]) expect(screen.getByRole("heading",{name:label})).toBeInTheDocument();
 expect(screen.getByText("Matchscore: 1.2")).toBeInTheDocument();
 for(let i=0;i<4;i++) expect(screen.getByText(`Synthetic source reason ${i}`)).toBeInTheDocument();
 expect(screen.getAllByRole("link",{name:"Åbn bilag"}).map(link=>link.getAttribute("href"))).toEqual([20,21,22,23].map(id=>`/companies/acme-aps/bilag?documentId=${id}`));
 expect(screen.getByText(/De bekræfter ikke en kreditnota/)).toBeInTheDocument();
 expect(screen.queryByRole("button",{name:/Bogfør|Anvend|Godkend/})).not.toBeInTheDocument();
 const calls=(fetch as unknown as ReturnType<typeof vi.fn>).mock.calls;
 expect(calls).toHaveLength(1);expect(String(calls[0]![0])).toBe("/api/companies/acme-aps/bank/match-suggestions?bankTransactionId=7");
 expect((calls[0]![1] as RequestInit).method).toBeUndefined();expect((calls[0]![1] as RequestInit).body).toBeUndefined();
 await userEvent.click(screen.getByRole("button",{name:"Luk"}));expect(close).toHaveBeenCalledTimes(1);
});

test("unsafe ambiguity is visible and empty advice never chooses a document",async()=>{
 mockFetch({[path]:{count:1,rows:[{...row,unsafeMatchReason:"Synthetic ambiguous amount has multiple documents"}],errors:[]}});show();
 expect(await screen.findByText("Synthetic ambiguous amount has multiple documents")).toBeInTheDocument();
 expect(screen.getByText(/Ingen sikre matchforslag/)).toBeInTheDocument();
 expect(screen.queryByRole("link",{name:"Åbn bilag"})).not.toBeInTheDocument();
});

test("source domain refusal and transport errors stay visible",async()=>{
 mockFetch({[path]:{ok:false,count:0,rows:[],errors:["unmatched bank transaction 7 does not exist"]}});show();
 expect(await screen.findByText("unmatched bank transaction 7 does not exist")).toBeInTheDocument();
 expect(screen.queryByText(/Ingen sikre matchforslag/)).not.toBeInTheDocument();
});

test("loading state stays read only and mismatched selected row is not displayed",async()=>{
 let finish!: (value:{ok:boolean;count:number;rows:BankMatchRow[];errors:string[]})=>void;
 vi.spyOn(bankMatchesApi,"bankSuggestMatches").mockImplementation(()=>new Promise(resolve=>{finish=resolve;}));
 show();expect(screen.getByText("Henter matchforslag…")).toBeInTheDocument();
 finish({ok:true,count:1,rows:[{...row,bankTransactionId:99,unsafeMatchReason:"Unrelated row secret"}],errors:[]});
 expect(await screen.findByText("Serveren returnerede ikke den valgte bankpost.")).toBeInTheDocument();
 expect(screen.queryByText("Unrelated row secret")).not.toBeInTheDocument();
});
