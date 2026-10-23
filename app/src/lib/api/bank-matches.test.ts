import { withProtocol } from "../../test/protocol";
import {expect,test} from "bun:test";
import {stubGlobal} from "../../test/globals";
import type {BankMatchSuggestions} from "../types/bank-matches";
import {bankMatchesApi} from "./bank-matches";

test("bank advice sends exact selectors and preserves flat result without score clamping",async()=>{
 const calls:Array<{url:string;init?:RequestInit}>=[];
 const result:BankMatchSuggestions={ok:true,count:1,rows:[{bankTransactionId:7,date:"2026-10-20",text:"Synthetic bank",amount:200,currency:"DKK",reference:null,suggestions:[{kind:"issued_invoice",documentId:3,invoiceNo:"SYN-3",confidence:1.2,reasons:["Synthetic exact source reason"]}],unsafeMatchReason:null}],errors:[]};
 stubFetch(async(input:RequestInfo|URL,init?:RequestInit)=>{calls.push({url:String(input),init});return new Response(JSON.stringify(result));});
 const controller=new AbortController();
 expect(await bankMatchesApi.bankSuggestMatches("synthetic-company",7,2,{signal:controller.signal})).toEqual(result);
 expect(calls[0]!.url).toBe("/api/companies/synthetic-company/bank/match-suggestions?bankTransactionId=7&max=2");
 expect(calls[0]!.init?.signal).toBe(controller.signal);expect(calls[0]!.init?.body).toBeUndefined();
 await bankMatchesApi.bankSuggestMatches("synthetic");expect(calls[1]!.url).toBe("/api/companies/synthetic/bank/match-suggestions");
});
test("source ok:false is surfaced as an error rather than successful empty advice",async()=>{
 stubFetch(async()=>new Response(JSON.stringify({ok:false,count:0,rows:[],errors:["unmatched bank transaction 7 does not exist"]})));
 await expect(bankMatchesApi.bankSuggestMatches("synthetic",7)).rejects.toThrow("unmatched bank transaction 7 does not exist");
});

const stubFetch = (handler: Parameters<typeof withProtocol>[0]) => stubGlobal("fetch", withProtocol(handler, [["GET", "/api/companies/{slug}/bank/match-suggestions"]]));
