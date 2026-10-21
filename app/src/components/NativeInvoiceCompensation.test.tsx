import { expect, test, vi } from "bun:test";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { NativeInvoiceCompensationResult } from "../lib/api/invoices";
import { MutationMemoryProvider } from "../lib/mutation-memory";
import { stubGlobal } from "../test/globals";
import { renderAt } from "../test/render";
import { NativeInvoiceCompensation } from "./NativeInvoiceCompensation";

const accounts=[{accountNo:"R",name:"Synthetic receivable",type:"asset",normalBalance:"debit"},{accountNo:"I",name:"Synthetic income",type:"income",normalBalance:"credit"},{accountNo:"WRONG",name:"Synthetic liability",type:"liability",normalBalance:"credit"}];
const base:NativeInvoiceCompensationResult={invoiceDocumentId:7,invoiceNumber:"SYN-7",asOfDate:"2026-01-05",effectiveDueDate:"2026-01-02",overdueDays:3,principalOpenBalance:100,principalCurrency:"EUR",eligible:true,isCommercialTransaction:true,compensationAmountDkk:310,reason:null,appliedRules:["DK-INVOICE-COMPENSATION-001"],errors:[],claimId:null,claimDate:null,registeredAmountDkk:null,postedJournalEntryId:null,postedJournalEntryNo:null};
function render(){return renderAt(<MutationMemoryProvider><NativeInvoiceCompensation slug="synthetic" id={7} asOfDate="2026-01-05" accounts={accounts} onChanged={()=>{}}/></MutationMemoryProvider>,{route:"/companies/synthetic/fakturaer"});}

test("calculation shows ineligibility with original principal currency and DKK claim without writing",async()=>{
 sessionStorage.clear();const calls:Array<{url:string;body:unknown}>=[];
 stubGlobal("fetch",vi.fn(async(input:RequestInfo|URL,init?:RequestInit)=>{calls.push({url:String(input),body:JSON.parse(init?.body as string)});return Response.json({ok:true,compensation:{...base,eligible:false,reason:"Synthetic private buyer is not eligible"}});}));
 render();expect(calls).toEqual([]);
 await userEvent.click(screen.getByRole("button",{name:"Beregn kompensation"}));
 expect(await screen.findByText(/Synthetic private buyer is not eligible/)).toBeInTheDocument();
 expect(screen.getByText(/hovedstol.*EUR/)).toBeInTheDocument();expect(screen.getByText(/Kompensation: 310,00 kr/)).toBeInTheDocument();
 expect(screen.queryByRole("button",{name:"Gennemgå registrering af kompensation"})).not.toBeInTheDocument();
 expect(calls).toEqual([{url:"/api/companies/synthetic/invoices/compensation/calculate",body:{invoiceDocumentId:7,asOfDate:"2026-01-05"}}]);
});

test("registration and posting are separate confirmed writes with explicit compatible accounts and canonical readback",async()=>{
 sessionStorage.clear();let result={...base};const writes:Array<{url:string;body:unknown}>=[];
 stubGlobal("fetch",vi.fn(async(input:RequestInfo|URL,init?:RequestInit)=>{
  const url=String(input);const body=JSON.parse(init?.body as string);
  if(url.endsWith("/register")){writes.push({url,body});result={...result,eligible:false,claimId:8,claimDate:"2026-01-05",registeredAmountDkk:310};}
  if(url.endsWith("/post")){writes.push({url,body});result={...result,postedJournalEntryId:11,postedJournalEntryNo:"2026-00011"};}
  return Response.json({ok:true,compensation:result});
 }));
 render();await userEvent.click(screen.getByRole("button",{name:"Beregn kompensation"}));
 await userEvent.click(await screen.findByRole("button",{name:"Gennemgå registrering af kompensation"}));expect(writes.length).toBe(0);
 await userEvent.click(screen.getByRole("button",{name:"Registrér kompensation"}));
 await screen.findByText(/Registreret krav 8.*Ikke bogført/);expect(writes.length).toBe(1);
 expect(screen.getByRole("button",{name:"Gennemgå bogføring af kompensation"})).toBeDisabled();
 expect(screen.queryByRole("option",{name:/Synthetic liability/})).not.toBeInTheDocument();
 await userEvent.selectOptions(screen.getByLabelText("Konto til kompensationstilgodehavende"),"R");await userEvent.selectOptions(screen.getByLabelText("Konto til kompensationsindtægt"),"I");
 await userEvent.click(screen.getByRole("button",{name:"Gennemgå bogføring af kompensation"}));expect(writes.length).toBe(1);
 await userEvent.click(screen.getByRole("button",{name:"Bogfør kompensation"}));await screen.findByText(/Registreret krav 8.*Bogført i 2026-00011/);
 expect(writes).toEqual([{url:"/api/companies/synthetic/invoices/compensation/register",body:{invoiceDocumentId:7,asOfDate:"2026-01-05",compensationAmountDkk:310,note:"",confirm:true}},{url:"/api/companies/synthetic/invoices/compensation/post",body:{invoiceDocumentId:7,transactionDate:"2026-01-05",receivableAccountNo:"R",compensationIncomeAccountNo:"I",confirm:true}}]);
 expect(screen.getByText(/En faktisk indbetaling på kravet afstemmes i DKK/)).toBeInTheDocument();
});

test("an unknown posting outcome stays blocked after a canonical read and is never retried automatically",async()=>{
 sessionStorage.clear();let writes=0,reads=0;
 stubGlobal("fetch",vi.fn(async(input:RequestInfo|URL)=>{
  if(String(input).endsWith("/post")){writes++;throw new TypeError("response lost");}
  reads++;return Response.json({ok:true,compensation:{...base,eligible:false,claimId:8,claimDate:"2026-01-05",registeredAmountDkk:310}});
 }));
 render();await userEvent.click(screen.getByRole("button",{name:"Beregn kompensation"}));
 await userEvent.selectOptions(await screen.findByLabelText("Konto til kompensationstilgodehavende"),"R");await userEvent.selectOptions(screen.getByLabelText("Konto til kompensationsindtægt"),"I");
 await userEvent.click(screen.getByRole("button",{name:"Gennemgå bogføring af kompensation"}));await userEvent.click(screen.getByRole("button",{name:"Bogfør kompensation"}));
 await screen.findByText(/En ny skrivning er blokeret/);expect(screen.getByRole("button",{name:"Bogfør kompensation"})).toBeDisabled();
 await userEvent.click(screen.getByRole("button",{name:"Kontrollér status"}));await waitFor(()=>expect(reads).toBe(2));
 await screen.findByText(/En ny skrivning er blokeret/);expect(screen.getByRole("button",{name:"Bogfør kompensation"})).toBeDisabled();expect(writes).toBe(1);
});
