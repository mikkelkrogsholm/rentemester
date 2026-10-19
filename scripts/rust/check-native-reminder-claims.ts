import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { isDeepStrictEqual } from "node:util";
import type { Page } from "@playwright/test";

const saved="/tmp/rentemester-rust-reports/native-reminder-claims.json";
type Evidence={invoiceId:number;invoiceNo:string;year:string;status:unknown;originalHash:string;pdfHash:string;journalIds:number[];journal:unknown[]};
/** Owned synthetic invoice/bank/accounts, ordinary MFA context. No sending/offline identity changes. */
export async function checkNativeReminderClaims(page:Page,base:string,restarted=false){
 if(!/^http:\/\/127\.0\.0\.1:\d+$/.test(base))throw new Error("Owned synthetic loopback fixture required");
 const url=`${base}/api/companies/synthetic`,headers={origin:base,"content-type":"application/json"};
 const get=async(path:string)=>{const r=await page.request.get(`${url}/${path}`);if(!r.ok())throw new Error(`Reminder canonical read refused ${path}: ${r.status()}`);return r.json();};
 const post=async(path:string,data:unknown)=>{const r=await page.request.post(`${url}/${path}`,{headers,data});if(!r.ok())throw new Error(`Reminder setup/write refused ${path}: ${r.status()}`);return r.json();};
 const refuse=async(path:string,data:unknown,status=422)=>{const r=await page.request.post(`${url}/${path}`,{headers,data});if(r.status()!==status)throw new Error(`Reminder definite refusal missing ${path}: ${r.status()}`);};
 const hash=(bytes:Buffer)=>createHash("sha256").update(bytes).digest("hex");
 const ownStatus=(value:Record<string,unknown>)=>{const {accounts:_,incomingBankTransactions:__,asOfDate:___,...own}=value;return own;};
 let evidence:Evidence;
 const temp=mkdtempSync("/tmp/rm-reminder-claims-");
 try{
  const cookie=join(temp,"session.cookie");writeFileSync(cookie,(await page.context().cookies(base)).map(c=>`${c.name}=${c.value}`).join("; "),{mode:0o600,flag:"wx"});
  const common=["--server",base,"--allow-loopback-http","--cookie-file",cookie];
  const rpc=(name:string,args:unknown)=>{const input=[{jsonrpc:"2.0",id:1,method:"initialize",params:{protocolVersion:"2025-11-25",capabilities:{},clientInfo:{name:"synthetic-reminder-claims",version:"1"}}},{jsonrpc:"2.0",method:"notifications/initialized"},{jsonrpc:"2.0",id:2,method:"tools/call",params:{name,arguments:args}}].map(v=>JSON.stringify(v)).join("\n")+"\n";const r=spawnSync(process.env.RM_NATIVE_MCP_BIN??resolve("target/debug/rentemester-mcp"),common,{input,encoding:"utf8",maxBuffer:32*1024*1024});if(r.error||r.status!==0)throw new Error("Reminder MCP subprocess failed");const call=r.stdout.trim().split("\n").map(l=>JSON.parse(l)).find(v=>v.id===2);if(call?.error||call?.result?.isError)throw new Error("Reminder MCP tool failed");return call.result.structuredContent.data;};
  if(!restarted){
   const day=(ago:number)=>new Date(Date.now()-ago*86400000).toLocaleDateString("sv-SE",{timeZone:"Europe/Copenhagen"}),today=day(0),issueDate=day(3),dueDate=day(2),reminderDate=day(1);
   for(const[accountNo,name,type,normalBalance]of[["1798","Synthetic reminder bank","asset","debit"],["8798","Synthetic reminder fee income","income","credit"],["8799","Synthetic reminder sales","income","credit"]])await post("accounts",{accountNo,name,type,normalBalance});
   const customer=(await post("customers",{name:"Synthetic Reminder Claim Buyer",address:"Synthetic Reminder Road 2",vatOrCvr:"DK12345678",paymentTermsDays:1})).customer;
   const issue={issueDate,dueDate,customerId:customer.id,lines:[{description:"Synthetic reminder claim original",quantity:1,unitPriceExVat:80}]};
   const preview=await page.request.post(`${url}/invoices/preview`,{headers,data:issue});if(!preview.ok()||!(await preview.body()).subarray(0,5).equals(Buffer.from("%PDF-")))throw new Error("Reminder original preview unavailable");
   const invoice=(await post("invoices/issue",issue)).invoice;if(invoice.grossAmount!==100)throw new Error("Synthetic source invoice gross differs from native engine");
   const posted=(await post("invoices/post",{invoiceDocumentId:invoice.documentId,revenueAccountNo:"8799",confirm:true})).posting;
   const bank=(await post("bank-accounts",{name:"Synthetic reminder claim bank",slug:"synthetic-reminder-claims",currency:"DKK",ledgerAccountNo:"1798"})).bankAccount;
   const csv=`date;text;amount;currency;reference\n${today};Synthetic reminder principal;100;DKK;SYN-REMINDER-PRINCIPAL\n${today};Synthetic reminder partial;20;DKK;SYN-REMINDER-PART\n${today};Synthetic reminder final;30;DKK;SYN-REMINDER-FINAL\n${today};Synthetic reminder outgoing;-1;DKK;SYN-REMINDER-OUT\n${today};Synthetic reminder excess;51;DKK;SYN-REMINDER-EXCESS\n`;
   const imported=(await post("bank/import",{account:bank.slug,csvContent:csv,confirm:true})).import;
   if(imported.imported!==5||imported.skippedDuplicates!==0)throw new Error("Own five synthetic bank rows not imported exactly once");
   const all=(await get("bank")).bank,rows=all.transactions as Array<{id:number;date:string;text:string;amount:number}>;
   const expectedBankRows=[
    ["SYN-REMINDER-PRINCIPAL","Synthetic reminder principal",100],
    ["SYN-REMINDER-PART","Synthetic reminder partial",20],
    ["SYN-REMINDER-FINAL","Synthetic reminder final",30],
    ["SYN-REMINDER-OUT","Synthetic reminder outgoing",-1],
    ["SYN-REMINDER-EXCESS","Synthetic reminder excess",51],
   ] as const;
   const ownIds=new Map<string,number>();
   const incoming=(await get(`invoices/${invoice.documentId}/reminder-claims`)).reminderClaims.incomingBankTransactions;
   for(const [reference,text,amount]of expectedBankRows){
    const matches=rows.filter(row=>row.date===today&&row.text===text&&row.amount===amount);
    if(matches.length!==1||!Number.isSafeInteger(matches[0].id))throw new Error("Own bank row missing or ambiguous by canonical date/text/amount");
    const id=matches[0].id;
    if(amount>0&&!incoming.some((row:{id:number;reference:string;bankAccountNo:string})=>row.id===id&&row.reference===reference&&row.bankAccountNo==="1798"))throw new Error("Incoming claim source does not corroborate own bank ID/reference/account");
    ownIds.set(reference,id);
   }
   if(new Set(ownIds.values()).size!==5)throw new Error("Synthetic bank rows do not have five distinct identities");
   const bid=(reference:string)=>{const id=ownIds.get(reference);if(id===undefined)throw new Error("Unknown owned bank reference");return id;};
   await page.goto(`${base}/companies/synthetic/fakturaer?year=${encodeURIComponent(all.selectedYear)}`);
   const row=page.getByRole("row").filter({hasText:invoice.invoiceNumber});await row.getByRole("button",{name:"Rykkergebyrer og krav",exact:true}).click();
   const panel=page.getByRole("region",{name:"Rykkergebyrer og krav",exact:true});await panel.getByLabel("Rykkerdato",{exact:true}).fill(reminderDate);await panel.getByLabel("Rykkergebyr i DKK",{exact:true}).fill("50");await panel.getByRole("button",{name:"Gennemgå registrering af gebyr",exact:true}).click();let dialog=page.getByRole("dialog");await dialog.getByRole("button",{name:"Registrér gebyr",exact:true}).click();await dialog.waitFor({state:"hidden"});
   let current=(await get(`invoices/${invoice.documentId}/reminder-claims`)).reminderClaims;const registered=current.reminders[0];if(current.ledgerBackedClaimBalance!==0||registered.feeAmount!==50)throw new Error("Reminder registration secretly posted or wrong amount");
   await refuse("invoices/reminders/register",{invoiceDocumentId:invoice.documentId,reminderDate,feeAmount:50,confirm:true});
   await panel.getByLabel("Registreret gebyr",{exact:true}).selectOption(String(registered.reminderId));await panel.getByLabel("Bogføringsdato for gebyr",{exact:true}).fill(today);await panel.getByLabel("Konto til gebyrtilgodehavende",{exact:true}).selectOption(current.originalReceivableAccountNo);await panel.getByLabel("Konto til gebyrindtægt",{exact:true}).selectOption("8798");
   await panel.getByRole("button",{name:"Gennemgå særskilt bogføring",exact:true}).click();dialog=page.getByRole("dialog");await dialog.getByRole("button",{name:"Bogfør gebyr",exact:true}).click();await dialog.waitFor({state:"hidden"});await panel.getByText(/Bogført i postering/).waitFor();
   current=(await get(`invoices/${invoice.documentId}/reminder-claims`)).reminderClaims;const feeEntry=current.reminders[0].journalEntryId;
   await refuse("invoices/reminders/post",{invoiceDocumentId:invoice.documentId,reminderId:registered.reminderId,receivableAccountNo:current.originalReceivableAccountNo,reminderIncomeAccountNo:"8798",confirm:true});
   await refuse("invoices/claims/settle",{invoiceDocumentId:invoice.documentId,bankTransactionId:bid("SYN-REMINDER-PART"),confirm:true});
   // Principal remains a separate ordinary invoice operation even after posting its fee.
   await row.getByRole("button",{name:"Afstem",exact:true}).click();dialog=page.getByRole("dialog");await dialog.getByLabel("Bankreference",{exact:true}).fill("SYN-REMINDER-PRINCIPAL");const principalResponse=page.waitForResponse(r=>r.url().endsWith("/invoices/settle")&&r.request().method()==="POST");await dialog.getByRole("button",{name:"Afstem faktura",exact:true}).click();const principalEntry=(await(await principalResponse).json()).settlement.entryId;await dialog.waitFor({state:"hidden"});
   await panel.getByRole("button",{name:"Opdatér kravstatus",exact:true}).click();await panel.getByRole("button",{name:"Gennemgå indbetaling på gebyrkrav",exact:true}).waitFor();
   await refuse("invoices/claims/settle",{invoiceDocumentId:invoice.documentId,bankTransactionId:bid("SYN-REMINDER-OUT"),confirm:true});await refuse("invoices/claims/settle",{invoiceDocumentId:invoice.documentId,bankTransactionId:bid("SYN-REMINDER-EXCESS"),confirm:true});
   await panel.getByLabel("Faktisk indbetaling på gebyrkrav",{exact:true}).selectOption(String(bid("SYN-REMINDER-PART")));await panel.getByRole("button",{name:"Gennemgå indbetaling på gebyrkrav",exact:true}).click();dialog=page.getByRole("dialog");await dialog.getByRole("button",{name:"Afstem gebyrindbetaling",exact:true}).click();await dialog.waitFor({state:"hidden"});
   current=(await get(`invoices/${invoice.documentId}/reminder-claims`)).reminderClaims;if(current.ledgerBackedClaimBalance!==30)throw new Error("Partial claim receipt lost remainder");await refuse("invoices/claims/settle",{invoiceDocumentId:invoice.documentId,bankTransactionId:bid("SYN-REMINDER-PART"),confirm:true});
   const final=rpc("settle_invoice_claims",{company:"synthetic",body:JSON.stringify({invoiceDocumentId:invoice.documentId,bankTransactionId:bid("SYN-REMINDER-FINAL"),confirm:true}),confirm:true}).reminderClaims;if(final.remainingClaimOpenBalance!==0)throw new Error("Final actual MCP claim receipt did not clear fee");
   const status=(await get(`invoices/${invoice.documentId}/reminder-claims`)).reminderClaims;
   const journal=(await get(`journal?year=${encodeURIComponent(all.selectedYear)}`)).journal.entries;
   const journalIds=[posted.entryId,feeEntry,principalEntry,...status.claimPayments.map((p:{journalEntryId:number})=>p.journalEntryId)];
   const own=journal.filter((e:{id:number})=>journalIds.includes(e.id));if(own.length!==5||new Set(journalIds).size!==5)throw new Error("Reminder workflow did not produce exactly five canonical journals");
   const original=await page.request.get(`${url}/documents/${invoice.documentId}/file`),pdf=await page.request.get(`${url}/invoices/${invoice.documentId}/pdf`);if(!original.ok()||!pdf.ok())throw new Error("Reminder original/PDF inaccessible");
   evidence={invoiceId:invoice.documentId,invoiceNo:invoice.invoiceNumber,year:all.selectedYear,status:ownStatus(status),originalHash:hash(await original.body()),pdfHash:hash(await pdf.body()),journalIds,journal:own};mkdirSync("/tmp/rentemester-rust-reports",{recursive:true});writeFileSync(saved,JSON.stringify(evidence),{mode:0o600});
  }else evidence=JSON.parse(readFileSync(saved,"utf8"));
  const expected=(await get(`invoices/${evidence.invoiceId}/reminder-claims`));if(!isDeepStrictEqual(ownStatus(expected.reminderClaims),evidence.status)||expected.reminderClaims.principalOpenBalance!==0||expected.reminderClaims.ledgerBackedClaimBalance!==0||expected.reminderClaims.delivery.sentByThisFlow!==false||expected.reminderClaims.delivery.priorDeliveryStatus!=="unavailable")throw new Error("Reminder canonical state differs or fabricates delivery");
  const cli=spawnSync(process.env.RM_NATIVE_CLI_BIN??resolve("target/debug/rentemester-rust"),["remote","invoice-reminder-claims",...common,"--company","synthetic","--id",String(evidence.invoiceId)],{encoding:"utf8",maxBuffer:32*1024*1024});if(cli.error||cli.status!==0||!isDeepStrictEqual(JSON.parse(cli.stdout).data,expected)||!isDeepStrictEqual(rpc("invoice_reminder_claims",{company:"synthetic",id:evidence.invoiceId}),expected))throw new Error("CLI/MCP full reminder status differs from canonical HTTP");
  const journal=(await get(`journal?year=${encodeURIComponent(evidence.year)}`)).journal.entries.filter((e:{id:number})=>evidence.journalIds.includes(e.id));if(!isDeepStrictEqual(journal,evidence.journal))throw new Error("Reminder journals changed or restart lost evidence");
  const original=await page.request.get(`${url}/documents/${evidence.invoiceId}/file`),pdf=await page.request.get(`${url}/invoices/${evidence.invoiceId}/pdf`);if(!original.ok()||!pdf.ok()||hash(await original.body())!==evidence.originalHash||hash(await pdf.body())!==evidence.pdfHash)throw new Error("Reminder flow changed immutable original/PDF");
  const tb=(await get(`trial-balance?year=${encodeURIComponent(evidence.year)}`)).trialBalance;if(tb.balanced!==true)throw new Error("Reminder shared trial balance is not balanced");
  console.log(`PASS: explicit native reminder register → separate post → principal settlement → partial UI/final MCP incoming claims; canonical CLI/MCP/HTTP, five journals, originals, refusals${restarted?" and restart":""}; no email sent`);
 }finally{rmSync(temp,{recursive:true,force:true});}
}
