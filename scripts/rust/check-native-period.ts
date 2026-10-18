import type { Page } from "@playwright/test";
import { readFileSync,writeFileSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { randomUUID } from "node:crypto";
const saved="/tmp/rentemester-rust-reports/native-period.json";
export async function checkNativePeriod(page:Page,base:string,reopened=false){
 const url=`${base}/api/companies/synthetic`;
 const headers={origin:base,"content-type":"application/json"};
 let evidence:{start:string;end:string;periodId:number};
 async function blockedPosting(end:string){
  const state=JSON.parse(readFileSync(process.env.RM_NATIVE_WEB_STATE!,"utf8"));const wire=JSON.parse(state.postedBody);
  wire.input.idempotencyKey=`native-period-lock-${randomUUID()}`;wire.input.request.transactionDate=end;wire.input.request.text="Synthetic closed-period refusal";
  const denied=await page.request.post(`${url}/manual-journal`,{headers,data:wire});
  const body=await denied.text();if(denied.status()!==422||!body.includes("closed"))throw new Error(`Period lock did not reject journal: ${denied.status()} ${body}`);
 }
 async function closeUi(){
  await page.goto(`${base}/companies/synthetic/moms`);
  await page.getByRole("button",{name:"Luk momsperiode",exact:true}).click();
  const dialog=page.getByRole("dialog");
  const [response]=await Promise.all([page.waitForResponse(r=>r.url().endsWith("/periods/close")&&r.request().method()==="POST"),dialog.getByRole("button",{name:"Luk perioden",exact:true}).click()]);
  if(!response.ok())throw new Error(`Period close failed: ${await response.text()}`);
  await dialog.waitFor({state:"hidden"});return(await response.json()).period.id as number;
 }
 if(!reopened){
  const now=new Date();const end=new Date(Date.UTC(now.getUTCFullYear(),Math.floor(now.getUTCMonth()/3)*3,0));
  const start=new Date(Date.UTC(end.getUTCFullYear(),Math.floor(end.getUTCMonth()/3)*3,1));
  evidence={start:start.toISOString().slice(0,10),end:end.toISOString().slice(0,10),periodId:0};
  await page.goto(`${base}/companies/synthetic/kontoplan`);
  const chart=await(await page.request.get(`${url}/accounts`)).json();
  if(!chart.accounts.accountRoles.resolutions.some((r:{role:string;ok:boolean;accountNo:string})=>r.role==="bank"&&r.ok&&r.accountNo==="1800")){
   const role=page.getByRole("form",{name:"Bekræft kontorolle",exact:true});await role.getByLabel(/^Kontorolle/).selectOption("bank");await role.getByLabel(/^Konto til rollen/).selectOption("1800");await role.getByRole("button",{name:"Bekræft kontorolle",exact:true}).click();await page.getByRole("row").filter({hasText:/^Bank/}).getByText(/Konto 1800/).waitFor();
  }
  await page.goto(`${base}/companies/synthetic/bank`);
  await page.getByRole("button",{name:"Importér kontoudtog",exact:true}).click();const bank=page.getByRole("dialog");
  await bank.getByLabel("CSV-fil",{exact:true}).setInputFiles({name:"synthetic-closed-quarter.csv",mimeType:"text/csv",buffer:Buffer.from(`date;text;amount;currency;balance_after\n${evidence.end};Synthetic prior-quarter supplies;-125;DKK;-125\n`)});
  await bank.getByLabel(/^Bankkonto \(valgfri\)/).fill("synthetic-operating-bank");await bank.getByRole("button",{name:"Importér",exact:true}).click();await bank.getByText(/1 transaktion importeret/).waitFor();await bank.getByRole("button",{name:"Luk",exact:true}).click();
  await page.goto(`${base}/companies/synthetic/bilag`);await page.getByRole("button",{name:"Indlæs bilag",exact:true}).click();const upload=page.getByRole("dialog");
  await upload.getByLabel("Bilagsfil",{exact:true}).setInputFiles({name:"synthetic-prior-quarter.txt",mimeType:"text/plain",buffer:Buffer.from("Synthetic prior quarter invoice: net100 VAT25 gross125\n")});
  await upload.getByLabel(/^Udfyld fra leverandør/).selectOption({label:"Synthetic Updated Supplier"});
  for(const [label,value] of Object.entries({"Bilagsdato":evidence.end,"Fakturanr. (valgfri)":"SYN-CLOSE-1","Beløb inkl. moms":"125,00","Momsbeløb":"25,00","Beskrivelse af leverance":"Synthetic prior-quarter supplies","Modtager":"Synthetic Company","Modtager CVR/moms":"DK87654321","Modtageradresse":"Synthetic Street 1"}))await upload.getByLabel(label,{exact:true}).fill(value);
  await upload.getByRole("button",{name:"Indlæs bilag",exact:true}).click();await upload.getByText(/Bilaget er indlæst som DOC-/).waitFor();await upload.getByRole("button",{name:"Luk",exact:true}).click();
  const docs=await(await page.request.get(`${url}/documents`)).json();const document=docs.documents.documents.find((d:{filename:string})=>d.filename==="synthetic-prior-quarter.txt");
  const blocked=await(await page.request.get(`${url}/periods/close-readiness?from=${evidence.start}&to=${evidence.end}`)).json();
  if(!["BANK_UNRECONCILED","DOCUMENT_OUTSTANDING"].every(code=>blocked.packet?.items.some((i:{code:string;status:string})=>i.code===code&&i.status==="blocked")))throw new Error(`Unmatched bank did not block close: ${JSON.stringify(blocked)}`);
  await page.getByRole("row").filter({hasText:document.documentNo}).getByRole("link",{name:"Bogfør bilag",exact:true}).click();
  await page.getByLabel(/^Udgiftskonto/).selectOption("3100");
  await page.getByRole("button",{name:"Bogfør",exact:true}).click();await page.getByText(/Bilaget blev bogført som journalpost/).waitFor();
  const ready=await(await page.request.get(`${url}/periods/close-readiness?from=${evidence.start}&to=${evidence.end}`)).json();
  if(ready.packet?.blockers!==0)throw new Error(`Native close controls not ready: ${JSON.stringify(ready)}`);
  evidence.periodId=await closeUi();await blockedPosting(evidence.end);
  await page.getByRole("button",{name:"Genåbn momsperiode",exact:true}).click();const dialog=page.getByRole("dialog");
  await dialog.getByLabel("Begrundelse (påkrævet)",{exact:true}).fill("Synthetic reviewed reopening to check correction access");
  const [response]=await Promise.all([page.waitForResponse(r=>r.url().endsWith("/periods/reopen")&&r.request().method()==="POST"),dialog.getByRole("button",{name:"Genåbn perioden",exact:true}).click()]);
  if(!response.ok())throw new Error(`Period reopen failed: ${await response.text()}`);await dialog.waitFor({state:"hidden"});
  const opened=await(await page.request.get(`${url}/vat`)).json();if(opened.vat.periodStatus!=="open"||opened.vat.momsangivelseReady)throw new Error("Reopen did not revoke filing readiness");
  if(await closeUi()!==evidence.periodId)throw new Error("Reclose replaced immutable original period");
  writeFileSync(saved,JSON.stringify(evidence));
 }else evidence=JSON.parse(readFileSync(saved,"utf8"));
 const vat=await(await page.request.get(`${url}/vat`)).json();
 if(vat.vat.periodStart!==evidence.start||vat.vat.periodEnd!==evidence.end||vat.vat.periodStatus!=="closed"||!vat.vat.momsangivelseReady||vat.vat.inputVat!==25||vat.vat.payable!==-25||vat.vat.vatReportErrors.length)throw new Error(`Closed VAT return wrong: ${JSON.stringify(vat)}`);
 await blockedPosting(evidence.end);
 const periods=await(await page.request.get(`${url}/periods`)).json();const period=periods.periods.periods.find((p:{id:number})=>p.id===evidence.periodId);
 if(!period||period.rowStatus!=="closed"||period.effectiveStatus!=="closed")throw new Error("Period lock not persisted");
 const pdf=await page.request.get(`${url}/vat/export?format=pdf`);if(!pdf.ok())throw new Error("Closed VAT PDF unavailable");const bytes=await pdf.body();writeFileSync("/tmp/rentemester-rust-reports/native-closed-vat.pdf",bytes);
 const text=execFileSync("pdftotext",["-","-"],{input:bytes}).toString("utf8");if(!text.includes("Klar til indberetning")||!text.includes(evidence.start)||!text.includes("25,00"))throw new Error(`Closed VAT PDF wrong: ${text}`);
 await page.goto(`${base}/companies/synthetic/moms`);await page.getByRole("button",{name:"Genåbn momsperiode",exact:true}).waitFor();await page.screenshot({path:"/tmp/rentemester-rust-closed-vat.png",fullPage:true});
 console.log(reopened?"PASS: reviewed period lock, filing-ready VAT/PDF and rejected late posting persist after restart":"PASS: real browser prior-quarter purchase → bank/VAT/ledger controls → reviewed close → filing-ready report → posting lock → reasoned reopen → reclose with same original period");
}
