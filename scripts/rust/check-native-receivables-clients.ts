// Real host clients against the native synthetic server. Run after company/account-role setup.
// Uses a distinct invoice/customer; the earlier domain fixture must remain open for delete refusal.
import type { Page } from "@playwright/test";
import { spawnSync } from "node:child_process";
import { mkdtempSync, writeFileSync, readFileSync, rmSync, mkdirSync } from "node:fs";
import { join, resolve } from "node:path";
import { isDeepStrictEqual } from "node:util";
const saved = "/tmp/rentemester-rust-reports/native-receivables-clients.json";
export async function checkNativeReceivablesClients(page: Page, base: string, restarted = false) {
 if (!/^http:\/\/127\.0\.0\.1:\d+$/.test(base)) throw new Error("Synthetic literal-loopback native server required");
 const temp=mkdtempSync("/tmp/rm-native-receivables-clients-"),cookieFile=join(temp,"session.cookie");
 writeFileSync(cookieFile,(await page.context().cookies(base)).map(c=>`${c.name}=${c.value}`).join("; "),{mode:0o600,flag:"wx"});
 const common=["--server",base,"--allow-loopback-http","--cookie-file",cookieFile];
 const cli=process.env.RM_NATIVE_CLI_BIN??resolve("target/debug/rentemester-rust"),mcp=process.env.RM_NATIVE_MCP_BIN??resolve("target/debug/rentemester-mcp");
 function cliCall(command:string,body?:object,allowFailure=false){const run=spawnSync(cli,["remote",command,...common,"--company","synthetic",...(body?["--confirm"]:[])],{input:body?JSON.stringify(body):undefined,encoding:"utf8",maxBuffer:16*1024*1024});if(run.error)throw new Error("Native receivables CLI unavailable");const value=JSON.parse(run.stdout);if(!allowFailure&&run.status!==0)throw new Error(`CLI ${command} refused (${value.code??"unknown"})`);return{status:run.status,value};}
 function rpc(name:string,args:Record<string,unknown>,allowFailure=false){const input=[{jsonrpc:"2.0",id:1,method:"initialize",params:{protocolVersion:"2025-11-25",capabilities:{},clientInfo:{name:"synthetic-receivables-check",version:"1"}}},{jsonrpc:"2.0",method:"notifications/initialized"},{jsonrpc:"2.0",id:2,method:"tools/call",params:{name,arguments:args}}].map(v=>JSON.stringify(v)).join("\n")+"\n";const run=spawnSync(mcp,common,{input,encoding:"utf8",maxBuffer:16*1024*1024});if(run.error||run.status!==0)throw new Error("Native receivables MCP unavailable");const result=run.stdout.trim().split("\n").map(s=>JSON.parse(s)).find(r=>r.id===2);if(result.error)throw new Error(`MCP ${name} protocol refusal`);if(!allowFailure&&result.result.isError)throw new Error(`MCP ${name} refused (${result.result.structuredContent?.error?.code})`);return result.result;}
 const mutate=(name:string,body:object,allowFailure=false)=>rpc(name,{company:"synthetic",body:JSON.stringify(body),confirm:true},allowFailure);
 const readInvoices=()=>cliCall("invoices").value.data.invoices;
 let evidence:{invoiceId:number;customerId:number;paymentEntry:number;creditEntries:number[];refundEntry:number;debtor:string;bank:string;revenue:string;outputVat:string;date:string;receipt:number;outgoing:number;invoiceState:unknown;entries:unknown[]};
 try {
  if(!restarted){
   const chart=cliCall("accounts").value.data.accounts;
   const role=(name:string)=>chart.accountRoles.resolutions.find((r:{role:string;ok:boolean})=>r.role===name&&r.ok)?.accountNo as string|undefined;
   const debtor=role("debtors"),outputVat=role("output_vat");
   if(!debtor||!outputVat)throw new Error("Confirm debtors/output_vat through native setup before receivables client check");
   const revenue=readInvoices().nativeSales.revenueAccounts[0]?.accountNo as string|undefined;
   const bank=chart.accounts.find((a:{accountNo:string;type:string;normalBalance:string;active:boolean})=>a.type==="asset"&&a.normalBalance==="debit"&&a.active&&a.accountNo!==debtor)?.accountNo as string|undefined;
   if(!bank||!revenue)throw new Error("Existing active bank asset and revenue account required");
   const customer=cliCall("create-customer",{name:"Synthetic Receivables Client Buyer",address:"Synthetic Buyer3",vatOrCvr:"DK12345678"}).value.data.customer;
   const date=new Date().toLocaleDateString("sv-SE",{timeZone:"Europe/Copenhagen"});
   const issue={issueDate:date,customerId:customer.id,lines:[{description:"Synthetic receivables client service",quantity:1,unitPriceExVat:1000}]};
   // Preview the identical body before caller attestation; no monetary arithmetic is done here.
   const preview=rpc("preview_invoice",{company:"synthetic",body:JSON.stringify(issue)}).structuredContent.data.preview;
   if(!Buffer.from(preview.bytes).subarray(0,5).equals(Buffer.from("%PDF-")))throw new Error("Native preview missing");
   const invoice=rpc("issue_invoice",{company:"synthetic",body:JSON.stringify(issue),confirm:true,previewReviewed:true}).structuredContent.data.invoice;
   if(invoice.grossAmount!==1250)throw new Error("Server invoice computation differs from synthetic fixture");
   cliCall("post-invoice",{invoiceDocumentId:invoice.documentId,revenueAccountNo:revenue,confirm:true});
   const account=cliCall("create-bank-account",{name:"Synthetic receivables client bank",slug:"synthetic-receivables-clients",currency:"DKK",ledgerAccountNo:bank}).value.data.bankAccount;
   const csv=`date;text;amount;currency;reference\n${date};Synthetic receivables client receipt;500;DKK;SYN-RECEIVABLE-RECEIPT\n${date};Synthetic receivables client refund;-500;DKK;SYN-RECEIVABLE-REFUND\n`;
   cliCall("import-bank",{account:account.slug,csvContent:csv,confirm:true});
   const rows=cliCall("bank").value.data.bank.transactions;
   const receipt=rows.find((r:{text:string})=>r.text==="Synthetic receivables client receipt").id;
   const outgoing=rows.find((r:{text:string})=>r.text==="Synthetic receivables client refund").id;
   const baseBody={invoiceDocumentId:invoice.documentId,confirm:true};
   const mixed=cliCall("settle-invoice",{...baseBody,bankTransactionId:receipt,bankTransactionReference:"SYN-RECEIVABLE-RECEIPT"},true);if(mixed.status!==2)throw new Error("Mixed bank selectors were not refused locally");
   const wrongBank=mutate("settle_invoice",{...baseBody,bankTransactionId:outgoing},true);if(wrongBank.structuredContent.error?.status!==422)throw new Error("Outgoing bank row was accepted as invoice receipt");
   const overcredit=cliCall("credit-invoice",{...baseBody,issueDate:date,reason:"Synthetic excessive correction",grossAmount:2000},true);if(overcredit.status!==1||!JSON.stringify(overcredit.value).includes("server_refused"))throw new Error("Over-credit was not refused");
   const settled=cliCall("settle-invoice",{...baseBody,bankTransactionId:receipt,amount:500,paymentDate:date}).value.data.settlement;
   if(settled.openBalance!==750||settled.principalAmount!==500)throw new Error("Partial receipt did not expose expected server balance");
   const duplicate=mutate("settle_invoice",{...baseBody,bankTransactionId:receipt},true);if(duplicate.structuredContent.error?.status!==422)throw new Error("Repeated bank receipt was not refused");
   const partial=mutate("credit_invoice",{...baseBody,issueDate:date,reason:"Synthetic partial correction",grossAmount:300}).structuredContent.data.creditNote;
   const full=cliCall("credit-invoice",{...baseBody,issueDate:date,reason:"Synthetic remaining correction"}).value.data.creditNote;
   const finalDuplicate=mutate("credit_invoice",{...baseBody,issueDate:date,reason:"Synthetic already fully credited"},true);if(finalDuplicate.structuredContent.error?.status!==422)throw new Error("Additional credit after full remaining credit was not refused");
   const refund=mutate("refund_invoice",{...baseBody,bankTransactionId:outgoing,amount:500,refundDate:date}).structuredContent.data.refund;
   if(refund.remainingCreditBalance!==0)throw new Error("Refund did not consume known credit balance");
   const duplicateRefund=cliCall("refund-invoice",{...baseBody,bankTransactionId:outgoing},true);if(duplicateRefund.status!==1)throw new Error("Repeated outgoing bank row accepted");
   const ids=[settled.entryId,partial.journalEntryId,full.journalEntryId,refund.entryId];
   const journal=cliCall("journal").value.data.journal;
   const entries=journal.entries.filter((e:{id:number})=>ids.includes(e.id));if(entries.length!==4)throw new Error("Expected four receivables lifecycle journal effects");
   evidence={invoiceId:invoice.documentId,customerId:customer.id,paymentEntry:settled.entryId,creditEntries:[partial.journalEntryId,full.journalEntryId],refundEntry:refund.entryId,debtor,bank,revenue,outputVat,date,receipt,outgoing,invoiceState:readInvoices().invoices.find((i:{documentId:number})=>i.documentId===invoice.documentId),entries};
   mkdirSync("/tmp/rentemester-rust-reports",{recursive:true});writeFileSync(saved,JSON.stringify(evidence,null,2),{mode:0o600});
  }else evidence=JSON.parse(readFileSync(saved,"utf8"));
  const invoice=readInvoices().invoices.find((i:{documentId:number})=>i.documentId===evidence.invoiceId);
  const viaMcp=rpc("invoices",{company:"synthetic"}).structuredContent.data.invoices.invoices.find((i:{documentId:number})=>i.documentId===evidence.invoiceId);
  if(invoice?.openBalance!==0||invoice?.creditableBalance!==0||invoice?.status!=="refunded"||!isDeepStrictEqual(invoice,viaMcp)||!isDeepStrictEqual(invoice,evidence.invoiceState))throw new Error("CLI/MCP invoice final readback differs or restart lost effects");
  const journal=cliCall("journal").value.data.journal;
  const ids=[evidence.paymentEntry,...evidence.creditEntries,evidence.refundEntry];
  const entries=journal.entries.filter((e:{id:number})=>ids.includes(e.id));if(!isDeepStrictEqual(entries,evidence.entries))throw new Error("Receivables original journal effects changed on reload");
  // Validate exact account identities from posted lines rather than current role guesses.
  for(const entry of entries){const lines=entry.lines as Array<{accountNo:string;debit:number;credit:number;debitAmount?:number;creditAmount?:number}>;if(!lines.some(l=>l.accountNo===evidence.debtor))throw new Error("Lifecycle journal lost original debtor");const isCash=[evidence.paymentEntry,evidence.refundEntry].includes(entry.id);if(isCash&&!lines.some(l=>l.accountNo===evidence.bank))throw new Error("Cash movement lost mapped bank account");if(!isCash&&(!lines.some(l=>l.accountNo===evidence.revenue)||!lines.some(l=>l.accountNo===evidence.outputVat)))throw new Error("Credit lost original revenue/VAT accounts");}
  console.log(restarted?"PASS: native CLI/MCP receipt, partial/full credit and refund readbacks/account identities persist after server restart":"PASS: distinct CLI/MCP invoice → partial imported receipt → partial credit → remaining full credit → outgoing refund; exact original accounts/readbacks, mixed selectors, wrong direction, duplicates and over-credit refused");
 }finally{rmSync(temp,{recursive:true,force:true});}
}
