import {expect,test} from "bun:test";
import {stubGlobal} from "../../test/globals";
import {invoicesApi,type InvoiceIssueInput} from "./invoices";

test("invoice preview and issue preserve explicit native tax facts without rewriting fractional rates",async()=>{
  const calls:Array<{path:string;body:unknown}>=[];
  stubGlobal("fetch",async(input:RequestInfo|URL,init?:RequestInit)=>{
    const path=String(input);calls.push({path,body:JSON.parse(String(init?.body))});
    return path.endsWith("/preview")?new Response("%PDF-synthetic",{headers:{"content-type":"application/pdf"}}):new Response(JSON.stringify({ok:true,invoice:{documentId:75,invoiceNumber:"synthetic-75",vatTreatment:"domestic_reverse_charge",netAmount:100,grossAmount:100,lines:[]}}),{headers:{"content-type":"application/json"}});
  });
  const input:InvoiceIssueInput={issueDate:"2026-10-20",vatTreatment:"domestic_reverse_charge",reverseChargeBasis:"DK_MOMSLOVEN_§46_STK_1_NR_7",reverseChargeNote:"Synthetic reviewed eligibility",lines:[{description:"Synthetic supply",quantity:1,unitPriceExVat:100,taxClassification:"reverse_charge",vatRate:0,reverseChargeBasis:"DK_MOMSLOVEN_§46_STK_1_NR_7"}]};
  await invoicesApi.previewInvoice("synthetic",input);
  const result=await invoicesApi.issueInvoice("synthetic",input);
  expect(calls).toHaveLength(2);
  expect(calls[0]!.body).toEqual(input);
  expect(calls[1]!.body).toEqual(input);
  expect(calls[0]!.path).toBe("/api/companies/synthetic/invoices/preview");
  expect(calls[1]!.path).toBe("/api/companies/synthetic/invoices/issue");
  expect(result).not.toHaveProperty("vatRate");
  expect(result).not.toHaveProperty("vatAmount");
});
