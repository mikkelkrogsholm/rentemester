import { withProtocol } from "../../test/protocol";
import {expect,test} from "bun:test";
import {stubGlobal} from "../../test/globals";
import {bankApi,type BankImportInput} from "./bank";

test("raw and compatible text import send exactly one source with existing options",async()=>{
 const calls:Array<{url:string;body:unknown}>=[];
 stubFetch(async(input:RequestInfo|URL,init?:RequestInit)=>{
  calls.push({url:String(input),body:JSON.parse(String(init?.body))});
  return new Response(JSON.stringify({ok:true,import:{imported:1,skippedDuplicates:0,skippedDuplicateRows:[],balanceWarnings:[],exceptionsCreated:1}}));
 });
 const raw:BankImportInput={csvBase64:"5vjpgA==",account:"synthetic",profile:"danske-bank"};
 const text:BankImportInput={csvContent:"transaction_date,text,amount,currency\n2026-10-20,Synthetic,-125,DKK"};
 await bankApi.importBank("synthetic-company",raw);
 await bankApi.importBank("synthetic-company",text);
 expect(calls).toEqual([
  {url:"/api/companies/synthetic-company/bank/import",body:{...raw,confirm:true}},
  {url:"/api/companies/synthetic-company/bank/import",body:{...text,confirm:true}},
 ]);
});

const stubFetch = (handler: Parameters<typeof withProtocol>[0]) => stubGlobal("fetch", withProtocol(handler, [["POST", "/api/companies/{slug}/bank/import"]]));
