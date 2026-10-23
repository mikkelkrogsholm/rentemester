import { withProtocol } from "../../test/protocol";
import { expect, test, vi } from "bun:test";
import { invoiceEmailApi, type InvoiceEmailRow } from "./invoice-email";
import { ApiError } from "./_shared";
import { stubGlobal } from "../../test/globals";
const row: InvoiceEmailRow = { invoiceDocumentId: 7, invoiceNumber: "SYN-7", kind: "invoice", recipient: "buyer@example.invalid", subject: "Faktura SYN-7", messageId: `<${"a".repeat(32)}@rentemester.local>`, bodySha256: "b".repeat(64), pdfSha256: "c".repeat(64), state: "accepted", stateHash: "d".repeat(64), attemptId: "synthetic", history: [] };
test("native fixed email routes preserve exact explicit bodies and canonical response", async () => {
  const fetcher=vi.fn(async(path:RequestInfo|URL)=>new Response(JSON.stringify(String(path).endsWith("/email-deliveries") ? {ok:true,invoiceEmail:{configured:true,rows:[row]}} : {ok:true,delivery:row}),{headers:{"content-type":"application/json"}}));
  stubFetch(fetcher);
  expect((await invoiceEmailApi.nativeInvoiceEmailStatus("synthetic-a")).rows[0]).toEqual(row);
  const send={invoiceDocumentId:7,to:row.recipient,kind:"invoice" as const,confirm:true as const};
  expect(await invoiceEmailApi.nativeInvoiceEmailSend("synthetic-a",send)).toEqual(row);
  const retry={messageId:row.messageId,expectedStateHash:row.stateHash,reason:"Reviewed fixed refusal",confirm:true as const};
  await invoiceEmailApi.nativeInvoiceEmailRetry("synthetic-a",retry);
  const reconcile={...retry,resolution:"accepted" as const,evidence:"SMTP log reference synthetic-7"};
  await invoiceEmailApi.nativeInvoiceEmailReconcile("synthetic-a",reconcile);
  expect(fetcher.mock.calls.map(c=>String(c[0]))).toEqual(["/api/companies/synthetic-a/invoices/email-deliveries","/api/companies/synthetic-a/invoices/send-email","/api/companies/synthetic-a/invoices/email-deliveries/retry","/api/companies/synthetic-a/invoices/email-deliveries/reconcile"]);
  const calls=fetcher.mock.calls as unknown as [RequestInfo | URL, RequestInit][];
  expect(JSON.parse(String(calls[1][1].body))).toEqual(send);expect(JSON.parse(String(calls[2][1].body))).toEqual(retry);expect(JSON.parse(String(calls[3][1].body))).toEqual(reconcile);
});
test("canonical refused422 retains its definite code, while malformed success refuses receipt adoption",async()=>{
  stubFetch(vi.fn(async()=>new Response(JSON.stringify({ok:false,code:"delivery_refused",errors:["SMTP refused"],delivery:{...row,state:"refused"}}),{status:422})));
  try { await invoiceEmailApi.nativeInvoiceEmailSend("synthetic",{invoiceDocumentId:7,to:row.recipient,kind:"invoice",confirm:true});throw new Error("expected refusal"); }
  catch(e){expect(e).toBeInstanceOf(ApiError);expect((e as ApiError).code).toBe("delivery_refused");expect((e as ApiError).status).toBe(422);}
  stubFetch(vi.fn(async()=>new Response(JSON.stringify({ok:true,delivery:{state:"accepted"}}))));
  await expect(invoiceEmailApi.nativeInvoiceEmailSend("synthetic",{invoiceDocumentId:7,to:row.recipient,kind:"invoice",confirm:true})).rejects.toMatchObject({code:"internal",status:200});
});

test("legacy accepted history permits absent PDF hash without inventing evidence",async()=>{
  stubFetch(vi.fn(async()=>new Response(JSON.stringify({ok:true,invoiceEmail:{configured:false,rows:[{...row,pdfSha256:null,legacy:true}]}}))));
  expect((await invoiceEmailApi.nativeInvoiceEmailStatus("synthetic")).rows[0]!.pdfSha256).toBeNull();
});

test("accepted receipt without exact native hashes or success envelope stays unconfirmed",async()=>{
  for(const body of [{ok:true,delivery:{...row,stateHash:""}},{ok:true,delivery:{...row,pdfSha256:"broken"}},{delivery:row}]) {
    stubFetch(vi.fn(async()=>new Response(JSON.stringify(body))));
    await expect(invoiceEmailApi.nativeInvoiceEmailSend("synthetic",{invoiceDocumentId:7,to:row.recipient,kind:"invoice",confirm:true})).rejects.toMatchObject({code:"internal",status:200});
  }
});

const stubFetch = (handler: Parameters<typeof withProtocol>[0]) => stubGlobal("fetch", withProtocol(handler, [["GET", "/api/companies/{slug}/invoices/email-deliveries"], ["POST", "/api/companies/{slug}/invoices/send-email"], ["POST", "/api/companies/{slug}/invoices/email-deliveries/retry"], ["POST", "/api/companies/{slug}/invoices/email-deliveries/reconcile"]]));
