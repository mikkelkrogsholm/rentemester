import type { Page } from "@playwright/test";
import { readFileSync } from "node:fs";
import { randomUUID } from "node:crypto";
export async function checkNativePeriodReport(page:Page,base:string,restarted=false){
 const evidence=JSON.parse(readFileSync("/tmp/rentemester-rust-reports/native-period.json","utf8"));
 const url=`${base}/api/companies/synthetic`;
 const headers={origin:base,"content-type":"application/json"};
 const listing=await(await page.request.get(`${url}/periods`)).json();
 const original=listing.periods.periods.find((p:{id:number})=>p.id===evidence.periodId);
 if(!original?.closePacketHash)throw new Error("Missing original close proof");
 await page.goto(`${base}/companies/synthetic/periodelas`);
 const row=page.getByRole("row").filter({hasText:evidence.start});
 if(!restarted){
  await row.getByRole("button",{name:"Registrér indberetning",exact:true}).click();
  const dialog=page.getByRole("dialog");
  await dialog.getByLabel("Kvitteringsreference",{exact:true}).fill("SYNTHETIC-FILING-RECEIPT");
  const [response]=await Promise.all([page.waitForResponse(r=>r.url().endsWith("/periods/report")&&r.request().method()==="POST"),dialog.getByRole("button",{name:"Registrér indberetning",exact:true}).click()]);
  if(!response.ok())throw new Error(`Filing receipt failed: ${await response.text()}`);
  await dialog.waitFor({state:"hidden"});
 }
 await row.getByText("Indberettet — kan ikke genåbnes",{exact:true}).waitFor();
 await row.getByText("SYNTHETIC-FILING-RECEIPT",{exact:true}).waitFor();
 if(await row.getByRole("button").count())throw new Error("Reported period still offers mutations");
 const wire={periodStart:evidence.start,periodEnd:evidence.end,packetHash:original.closePacketHash,reference:"SYNTHETIC-FILING-RECEIPT",confirm:true};
 const replay=await page.request.post(`${url}/periods/report`,{headers,data:wire});
 if(!replay.ok())throw new Error(`Receipt replay failed: ${await replay.text()}`);
 for(const [path,data] of [["report",{...wire,reference:"CONFLICTING-RECEIPT"}],["reopen",{periodStart:evidence.start,periodEnd:evidence.end,reason:"Cannot reopen reported",confirm:true}]] as const){
  const denied=await page.request.post(`${url}/periods/${path}`,{headers,data});
  if(denied.status()!==422)throw new Error(`Reported period accepted ${path}: ${await denied.text()}`);
 }
 const state=JSON.parse(readFileSync(process.env.RM_NATIVE_WEB_STATE!,"utf8"));const posting=JSON.parse(state.postedBody);
 posting.input.idempotencyKey=`reported-period-${randomUUID()}`;posting.input.request.transactionDate=evidence.end;
 const denied=await page.request.post(`${url}/manual-journal`,{headers,data:posting});if(denied.status()!==422)throw new Error("Reported period accepted late posting");
 const vat=await(await page.request.get(`${url}/vat`)).json();
 if(vat.vat.periodStart<=evidence.end||vat.vat.periodStatus!=="open"||vat.vat.inputVat!==275||vat.vat.payable!==-275)throw new Error(`Next VAT period wrong: ${JSON.stringify(vat)}`);
 await page.screenshot({path:"/tmp/rentemester-rust-period-report.png",fullPage:true});
 console.log(restarted?"PASS: immutable filing receipt and next active VAT period persist after restart":"PASS: record external filing receipt in UI → terminal reported period → next active VAT period; replay safe, conflicting receipt/reopen/late posting refused");
}
