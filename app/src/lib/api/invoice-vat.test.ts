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


test("EU invoice wire retains explicit FX without a caller VIES claim",async()=>{
 const calls:unknown[]=[];stubGlobal("fetch",async(input:RequestInfo|URL,init?:RequestInit)=>{calls.push(JSON.parse(String(init?.body)));return String(input).endsWith("/preview")?new Response("%PDF",{headers:{"content-type":"application/pdf"}}):new Response(JSON.stringify({ok:true,invoice:{documentId:1,invoiceNumber:"S-1",lines:[],netAmount:100,vatAmount:0,grossAmount:100,fxRateToDkk:7.45,grossAmountDkk:745}}));});
 const input:InvoiceIssueInput={issueDate:"2025-10-12",currency:"EUR",fxRateToDkk:7.45,vatTreatment:"foreign_reverse_charge",reverseChargeBasis:"EU_B2B_SERVICES",reverseChargeNote:"Reverse charge",buyer:{vatOrCvr:"DE123456789"},lines:[{description:"Synthetic EU service",quantity:1,unitPriceExVat:100,taxClassification:"reverse_charge",vatRate:0,reverseChargeBasis:"EU_B2B_SERVICES"}]};
 await invoicesApi.previewInvoice("synthetic",input);const issued=await invoicesApi.issueInvoice("synthetic",input);expect(calls).toEqual([input,input]);expect(issued.grossAmount).toBe(100);expect(issued.grossAmountDkk).toBe(745);
});

test("simplified preview and issue preserve reduced original-currency wire and server VAT",async()=>{
 const calls:unknown[]=[];
 stubGlobal("fetch",async(input:RequestInfo|URL,init?:RequestInit)=>{
   calls.push(JSON.parse(String(init?.body)));
   return String(input).endsWith("/preview")?new Response("%PDF-simplified",{headers:{"content-type":"application/pdf"}}):new Response(JSON.stringify({ok:true,invoice:{invoiceType:"simplified",documentId:78,invoiceNumber:"S-78",currency:"EUR",netAmount:80.06,vatAmount:20.01,grossAmount:100.07,lines:[{description:"Synthetic service"}]}}));
 });
 const input:InvoiceIssueInput={invoiceType:"simplified",issueDate:"2026-10-20",currency:"EUR",fxRateToDkk:7.45,vatTreatment:"standard",seller:{name:"Synthetic seller",address:"Synthetic street 1",vatOrCvr:"DK12345678"},lines:[{description:"Synthetic service"}],totals:{grossAmount:100.07,vatRate:0.25,vatComputationBasis:"VAT_20_OF_GROSS"}};
 await invoicesApi.previewInvoice("synthetic",input);const result=await invoicesApi.issueInvoice("synthetic",input);
 expect(calls).toEqual([input,input]);expect(result.invoiceType).toBe("simplified");expect(result.vatAmount).toBe(20.01);expect(result.lines).toEqual([{description:"Synthetic service"}]);
});

test("invoice metadata is serialized identically for preview and issue",async()=>{
 const calls:unknown[]=[];
 stubGlobal("fetch",async(input:RequestInfo|URL,init?:RequestInit)=>{
  calls.push(JSON.parse(String(init?.body)));
  return String(input).endsWith("/preview")?new Response("%PDF",{headers:{"content-type":"application/pdf"}}):new Response(JSON.stringify({ok:true,invoice:{documentId:80,invoiceNumber:"S-80",netAmount:200,vatAmount:50,grossAmount:250,lines:[]}}));
 });
 const input:InvoiceIssueInput={issueDate:"2026-10-20",deliveryPeriodStart:"2026-10-01",deliveryPeriodEnd:"2026-10-20",unitCode:"HUR",buyer:{vatOrCvr:"DE123456789"},lines:[{description:"Synthetic service",quantity:1,unitPriceExVat:200,unitCode:"H87"}]};
 await invoicesApi.previewInvoice("synthetic",input);await invoicesApi.issueInvoice("synthetic",input);
 expect(calls).toEqual([input,input]);
});
