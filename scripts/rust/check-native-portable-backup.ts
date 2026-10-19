import { createHmac, createHash } from "node:crypto";
import { createConnection } from "node:net";
import { spawnSync } from "node:child_process";
import { isDeepStrictEqual } from "node:util";
import { readdirSync, readFileSync, writeFileSync, statSync, realpathSync, unlinkSync } from "node:fs";
import { basename, join, resolve } from "node:path";
import { chromium, type Page } from "@playwright/test";
const work=resolve(process.env.RM_PORTABLE_WORK??"");
if(!basename(work).startsWith("Rentemester-Synthetic-Portable-")||(statSync(work).mode&0o077)!==0)throw Error("Owned private portable fixture required");
const config=JSON.parse(readFileSync(join(work,"server.json"),"utf8"));
if(config.workspace!=="synthetic-native-portable"||![join(work,"source"),join(work,"restored")].includes(config.root))throw Error("Synthetic fixture only");
const base=process.env.RM_PORTABLE_BASE!;
if(!/^http:\/\/127\.0\.0\.1:\d+$/.test(base))throw Error("Literal loopback only");
const phase=process.env.RM_PORTABLE_PHASE!;
if(!["source","restored","reopen"].includes(phase))throw Error("Explicit portable fixture phase required");
const browser=await chromium.launch({headless:true});
const page=await browser.newPage();page.setDefaultTimeout(45000);
  function totp(secret: string) {
    let bits = 0,
      count = 0;
    const bytes: number[] = [];
    for (const c of secret) {
      const v = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567".indexOf(c);
      if (v < 0) throw new Error("Invalid synthetic MFA key");
      bits = (bits << 5) | v;
      count += 5;
      if (count >= 8) {
        count -= 8;
        bytes.push((bits >>> count) & 255);
      }
    }
    const counter = Buffer.alloc(8);
    counter.writeBigUInt64BE(BigInt(Math.floor(Date.now() / 30000)));
    const d = createHmac("sha1", Buffer.from(bytes)).update(counter).digest();
    const o = d[19] & 15;
    return String((d.readUInt32BE(o) & 0x7fffffff) % 1000000).padStart(6, "0");
  }
  async function login(page: Page, email: string, password: string, recovery: string) {
    await page.goto(base);
    await page.getByLabel("E-mail", { exact: true }).fill(email);
    await page.getByLabel("Adgangskode", { exact: true }).fill(password);
    await page.getByRole("button", { name: "Log ind", exact: true }).click();
    await page.getByRole("button", { name: "Brug recovery code" }).click();
    await page.getByLabel("Recovery code", { exact: true }).fill(recovery);
    await page.getByRole("button", { name: "Bekræft", exact: true }).click();
    await page.getByRole("link", { name: "Åbn bogføring" }).first().waitFor();
  }
  async function enroll(page: Page, email: string, password: string) {
    await page.goto(base);
    await page.getByLabel("E-mail", { exact: true }).fill(email);
    await page.getByLabel("Adgangskode", { exact: true }).fill(password);
    await page.getByRole("button", { name: "Log ind", exact: true }).click();
    await page.getByRole("heading", { name: "Opsæt totrinsbekræftelse" }).waitFor();
    await page.getByLabel("Adgangskode", { exact: true }).fill(password);
    await page.getByRole("button", { name: "Start opsætning", exact: true }).click();
    const secret = await page.getByLabel("Opsætningsnøgle").inputValue();
    await page.getByLabel("Engangskode", { exact: true }).fill(totp(secret));
    await page.getByRole("button", { name: "Bekræft opsætning", exact: true }).click();
    await page.getByRole("region", { name: "Recovery codes" }).locator("li").first().waitFor();
    const codes = await page
      .getByRole("region", { name: "Recovery codes" })
      .locator("li")
      .allTextContents();
    if (codes.length < 2) throw new Error("Recovery codes missing");
    await page.getByRole("button", { name: "Jeg har gemt koderne", exact: true }).click();
    await page.getByRole("link", { name: "Åbn bogføring" }).first().waitFor();
    await page.context().clearCookies();
    await new Promise((r) => setTimeout(r, 11000));
    await login(page, email, password, codes[0]);
    return codes[1];
  }

const headers={origin:base,"content-type":"application/json"};
const company=(slug:string,r:string)=>`${base}/api/companies/${slug}/${r}`;
async function get(slug:string,r:string){const x=await page.request.get(company(slug,r));if(!x.ok())throw Error(`${r}: ${x.status()} ${await x.text()}`);return x.json();}
async function post(slug:string,r:string,data:unknown){const x=await page.request.post(company(slug,r),{headers,data});if(!x.ok())throw Error(`${r}: ${x.status()} ${await x.text()}`);return x.json();}
const statePath=join(work,"private-state.json");
const digest=(b:Buffer)=>createHash("sha256").update(b).digest("hex");
const password="synthetic-portable-password-123";
const slugs=["synthetic","synthetic-portable-second"];
async function snapshot(){const out:any={};for(const s of slugs){const journal=(await get(s,"journal")).journal;out[s]={uuid:journal.ledgerUuid,entries:journal.entries,balance:(await get(s,`balance?year=${journal.selectedYear}`)).balance};}return out;}
const knowledgeEnabled=process.env.RM_PORTABLE_KNOWLEDGE==="1";
const knowledgeAsOf="2026-06-01";
async function knowledgeViews(){
 const views:Record<string,unknown>={};
 for(const slug of slugs)views[slug]={current:await get(slug,`knowledge?asOf=${knowledgeAsOf}&includeProposed=false`),withProposed:await get(slug,`knowledge?asOf=${knowledgeAsOf}&includeProposed=true`)};
 return views;
}
async function prepareKnowledge(){
 const source={kind:"user",ref:"synthetic-portable-knowledge-owner-brief"};
 const propose=(predicate:string,value:unknown,ref:string,slug="synthetic")=>post(slug,"knowledge/propose",{predicate,value,source:{...source,ref},validFrom:"2000-01-01",certainty:"confirmed",confirm:true});
 const review=(id:string,decision:"approved"|"rejected",reason:string,slug="synthetic")=>post(slug,"knowledge/review",{assertionId:id,decision,reason,confirm:true});
 const old=(await propose("products_services",["Synthetic previous service"],"synthetic-portable-old-service")).assertion;
 await review(old.assertionId,"approved","Synthetic original source reviewed");
 const changed=await post("synthetic","knowledge/supersede",{assertionId:old.assertionId,replacement:{predicate:"products_services",value:["Synthetic reviewed replacement service"],source:{...source,ref:"synthetic-portable-replacement-brief"},validFrom:"2026-02-01",certainty:"confirmed"},confirm:true});
 if(changed.superseded?.reviewState!=="superseded"||changed.replacement?.reviewState!=="proposed")throw Error("Knowledge supersession manufactured replacement approval");
 await review(changed.replacement.assertionId,"approved","Synthetic replacement source independently reviewed");
 const pending=(await propose("operational_systems",["Synthetic pending operating platform"],"synthetic-portable-pending-system")).assertion;
 const rejected=(await propose("document_sources",["Synthetic rejected evidence source"],"synthetic-portable-rejected-source")).assertion;
 await review(rejected.assertionId,"rejected","Synthetic source was not accepted");
 const other=(await propose("products_services",["Synthetic distinct second-company service"],"synthetic-portable-second-service",slugs[1])).assertion;
 await review(other.assertionId,"approved","Synthetic separate company source reviewed",slugs[1]);
 const views:any=await knowledgeViews();const sourceView=views.synthetic;
 if(sourceView.current.context.asOf!=="2026-06-01T00:00:00.000Z"||sourceView.current.history.length!==4)throw Error("Fixed knowledge date/full history missing");
 const expected=new Map([[old.assertionId,"superseded"],[changed.replacement.assertionId,"approved"],[pending.assertionId,"proposed"],[rejected.assertionId,"rejected"]]);
 for(const row of sourceView.current.history){if(expected.get(row.assertionId)!==row.reviewState||!row.source?.ref||!row.actor||!row.principal?.id||row.history.length===0)throw Error("Knowledge review/provenance/history fixture incomplete");}
 const ids=(v:any)=>v.context.assertions.map((a:any)=>a.assertionId);
 if(!isDeepStrictEqual(ids(sourceView.current),[changed.replacement.assertionId])||!ids(sourceView.withProposed).includes(pending.assertionId)||views[slugs[1]].current.history.length!==1||views[slugs[1]].current.context.assertions[0]?.assertionId!==other.assertionId)throw Error("Knowledge proposal/rejection/supersession/company isolation not reflected in actual context");
 return views;
}
async function exportClientArtifacts(){
 const maxBytes=96*1024*1024;
 const maxOutput=8*maxBytes+65536; // MCP text+structuredContent repeat the integer array.
 const cookie=join(work,"source-clients.cookie");
 writeFileSync(cookie,(await page.context().cookies(base)).map(c=>`${c.name}=${c.value}`).join("; "),{mode:0o600,flag:"wx"});
 const common=["--server",base,"--allow-loopback-http","--cookie-file",cookie];
 const before=await snapshot();const results:Record<string,unknown>={};
 try{
  for(const kind of ["cli","mcp"]){
   const executable=join(work,kind==="cli"?"rentemester-rust":"rentemester-mcp");
   if(realpathSync(executable)!==executable||!statSync(executable).isFile())throw Error("Only pinned owned client executable allowed");
   const messages=[{jsonrpc:"2.0",id:1,method:"initialize",params:{protocolVersion:"2025-11-25",capabilities:{},clientInfo:{name:"synthetic-portable-backup",version:"1"}}},{jsonrpc:"2.0",method:"notifications/initialized"},{jsonrpc:"2.0",id:2,method:"tools/list"},{jsonrpc:"2.0",id:3,method:"tools/call",params:{name:"workspace_backup",arguments:{confirm:true}}}];
   const result=spawnSync(executable,kind==="cli"?["remote","workspace-backup",...common,"--confirm"]:common,{input:kind==="cli"?"":messages.map(m=>JSON.stringify(m)).join("\n")+"\n",encoding:"utf8",maxBuffer:maxOutput,timeout:120000});
   if(result.error||result.status!==0)throw Error(`Pinned ${kind} workspace backup subprocess failed`);
   let native:any,toolsCount:number|undefined;
   if(kind==="cli")native=JSON.parse(result.stdout).data;
   else{
    const rows=result.stdout.trim().split("\n").map(row=>JSON.parse(row));const list=rows.find(row=>row.id===2)?.result?.tools;const call=rows.find(row=>row.id===3);
    const tool=list?.find((t:any)=>t.name==="workspace_backup");
    if(!tool||!isDeepStrictEqual(tool.inputSchema,{type:"object",required:["confirm"],properties:{confirm:{const:true}},additionalProperties:false})||call?.error||call?.result?.isError)throw Error("MCP workspace backup discovery/call contract failed");
    toolsCount=list.length;native=call.result.structuredContent?.data;
   }
   const bytes=native?.backup?.bytes;
   if(native?.ok!==true||native.backup.contentType!=="application/vnd.rentemester.native-backup+json"||!Array.isArray(bytes)||bytes.length===0||bytes.length>maxBytes||!bytes.every((b:unknown)=>typeof b==="number"&&Number.isInteger(b)&&b>=0&&b<=255))throw Error(`Invalid ${kind} raw backup byte envelope`);
   const artifact=Buffer.from(bytes);writeFileSync(join(work,`${kind}.native-backup.json`),artifact,{mode:0o600,flag:"wx"});
   if(!isDeepStrictEqual(await snapshot(),before))throw Error(`${kind} backup changed accounting`);
   results[kind]={bytes:artifact.length,sha256:digest(artifact),toolsCount,noLedgerEffects:true};
  }
 }finally{unlinkSync(cookie);}
 return results;
}

try{
 let saved:any=phase==="source"?{}:JSON.parse(readFileSync(statePath,"utf8"));
 if(phase==="reopen")await login(page,"restored-owner@example.invalid",password,saved.recovery);
 else{
  const spool=join(config.root,".rentemester");const deliveries=readdirSync(spool).filter(n=>n.startsWith("verification-")&&n.endsWith(".txt"));
  if(deliveries.length!==1)throw Error("Expected one ordinary first-owner verification");
  await page.goto(readFileSync(join(spool,deliveries[0]),"utf8").trim());
  saved.recovery=await enroll(page,phase==="source"?"source-owner@example.invalid":"restored-owner@example.invalid",password);
 }
 if(phase==="source"){
  const created=await page.request.post(`${base}/api/companies`,{headers,data:{name:"Synthetic portable second",slug:slugs[1]}});if(created.status()!==201)throw Error(await created.text());
  const date=new Date().toLocaleDateString("sv-SE",{timeZone:"Europe/Copenhagen"});
  for(const slug of slugs){
   for(const [accountNo,type,normalBalance]of [["1759","asset","debit"],["8759","equity","credit"]])await post(slug,"accounts",{accountNo,name:`Synthetic portable ${accountNo}`,type,normalBalance});
  }
  const j=(await get("synthetic","journal")).journal;
  saved.original={confirm:true,input:{format:"rentemester-manual-journal-post/1",expectedRuleVersion:j.nativePosting.ruleVersion,idempotencyKey:"synthetic-portable-lost-source-response",request:{transactionDate:date,text:"Synthetic source accepted original",currency:"DKK",lines:[{accountNo:"1759",debitAmount:"12.34",creditAmount:"0.00"},{accountNo:"8759",debitAmount:"0.00",creditAmount:"12.34"}]}}};
  // Deliberately never read the response: reconcile acceptance through the journal.
  const body=JSON.stringify(saved.original);const cookie=(await page.context().cookies(base)).map(c=>`${c.name}=${c.value}`).join("; ");
  await new Promise<void>((ok,no)=>{const u=new URL(base);const socket=createConnection({host:u.hostname,port:Number(u.port)},()=>{socket.write(`POST /api/companies/synthetic/manual-journal HTTP/1.1\r\nHost: ${u.host}\r\nOrigin: ${base}\r\nContent-Type: application/json\r\nCookie: ${cookie}\r\nContent-Length: ${Buffer.byteLength(body)}\r\nConnection: close\r\n\r\n${body}`);});socket.on("error",no);socket.on("data",()=>{});socket.on("end",()=>{socket.destroy();ok();});});
  if(!(await get("synthetic","journal")).journal.entries.some((e:any)=>e.text===saved.original.input.request.text))throw Error("Lost-response source request did not commit");
  const second=structuredClone(saved.original);second.input.idempotencyKey="synthetic-portable-second-source";second.input.request.text="Synthetic second independent ledger";await post(slugs[1],"manual-journal",second);
  const bytes=Buffer.from("Synthetic portable document original\nNo personal or production data\n");
  await post("synthetic","documents/ingest",{confirm:true,fileName:"synthetic-portable-original.txt",fileBase64:bytes.toString("base64"),metadata:{source:"Synthetic portable fixture",documentType:"cash_register_receipt",issueDate:date}});
  const docs=(await get("synthetic","documents")).documents.documents;
  saved.document=docs.find((d:any)=>d.filename==="synthetic-portable-original.txt");saved.documentHash=digest(bytes);
  await post("synthetic","bank-accounts",{name:"Synthetic portable bank",slug:"synthetic-portable-bank",currency:"DKK",ledgerAccountNo:"1759"});
  await post("synthetic","bank/import",{confirm:true,account:"synthetic-portable-bank",csvContent:`transaction_date,text,amount,currency,balance_after\n${date},Synthetic portable bank source,12.34,DKK,12.34\n`});
  for(const [accountNo,type,normalBalance]of [["1100","asset","debit"],["1000","income","credit"],["4200","vat","credit"]])await post("synthetic","accounts",{accountNo,name:`Synthetic portable invoice ${accountNo}`,type,normalBalance});
  for(const [role,accountNo]of [["debtors","1100"],["output_vat","4200"]])await post("synthetic","account-roles",{confirm:true,role,accountNo});
  const update=await page.request.patch(company("synthetic","company"),{headers,data:{cvr:"87654321",address:"Synthetic Seller Street 1"}});if(!update.ok())throw Error(await update.text());
  const customer=await post("synthetic","customers",{name:"Synthetic portable customer",address:"Synthetic Buyer Street 2",vatOrCvr:"DK12345678"});
  const contacts=(await get("synthetic","contacts")).contacts.customers;const id=contacts.find((c:any)=>c.name==="Synthetic portable customer").id;
  const invoiceInput={issueDate:date,customerId:id,seller:{vatOrCvr:"DK87654321"},lines:[{description:"Synthetic portable service",quantity:1,unitPriceExVat:100}]};
  const preview=await page.request.post(company("synthetic","invoices/preview"),{headers,data:invoiceInput});if(!preview.ok())throw Error(await preview.text());
  saved.invoice=(await post("synthetic","invoices/issue",invoiceInput)).invoice;
  await post("synthetic","invoices/post",{confirm:true,invoiceDocumentId:saved.invoice.documentId,revenueAccountNo:"1000"});
  const pdf=await page.request.get(company("synthetic",`invoices/${saved.invoice.documentId}/pdf`));if(!pdf.ok())throw Error("Source PDF missing");saved.pdfHash=digest(await pdf.body());
  saved.bank=(await get("synthetic","bank")).bank;saved.snapshot=await snapshot();
  if(knowledgeEnabled){
   saved.knowledge=await prepareKnowledge();
   if(!isDeepStrictEqual(await snapshot(),saved.snapshot))throw Error("Knowledge lifecycle changed accounting");
   saved.knowledgeEvidence={asOf:knowledgeAsOf,sourceViewsSha256:digest(Buffer.from(JSON.stringify(saved.knowledge))),primaryHistoryRecords:4,secondHistoryRecords:1};
  }
  if(process.env.RM_PORTABLE_CLIENTS==="1")saved.clientEvidence=await exportClientArtifacts();
  if(knowledgeEnabled&&!isDeepStrictEqual(await knowledgeViews(),saved.knowledge))throw Error("Backup client exports changed knowledge history");
  if(process.env.RM_PORTABLE_WEB_DOWNLOAD==="1"){
   await page.goto(`${base}/companies/synthetic/integritet`);const card=page.locator(".native-backup");
   await card.getByLabel("Jeg vil hente en backup af hele workspacet.",{exact:true}).check();
   const [download,response]=await Promise.all([page.waitForEvent("download"),page.waitForResponse(r=>new URL(r.url()).pathname==="/api/workspace/backup"&&r.request().method()==="POST"),card.getByRole("button",{name:"Dan og download backup",exact:true}).click()]);
   if(!response.ok()||response.headers()["content-type"]!=="application/vnd.rentemester.native-backup+json")throw Error(`Live web backup failed ${response.status()}`);
   const path=join(work,"portable.native-backup.json");await download.saveAs(path);
   const downloaded=readFileSync(path),observed=await response.body();
   saved.download={bytes:downloaded.length,sha256:digest(downloaded),responseBytes:observed.length,responseSha256:digest(observed)};
   console.log(`Backup transport observation: ${JSON.stringify(saved.download)}`);
   // The saved browser download itself is independently signature/inventory/DB
   // verified after source removal. CDP body retrieval is diagnostic, not trust.
   if(downloaded.length===0)throw Error("Empty browser download");
   await card.getByRole("status").filter({hasText:"Download startet."}).waitFor();
  }else{
   const archive=await page.request.post(`${base}/api/workspace/backup`,{headers,data:{confirm:true}});if(!archive.ok())throw Error(`Live backup ${archive.status()}: ${await archive.text()}`);
   writeFileSync(join(work,"portable.native-backup.json"),await archive.body(),{mode:0o600});
  }
 }else{
  if(knowledgeEnabled){
   if(!saved.knowledge||!isDeepStrictEqual(await knowledgeViews(),saved.knowledge))throw Error("Restored knowledge context/history/provenance differs from original fixed-date API response");
   saved.knowledgeEvidence[phase==="reopen"?"exactAfterRestart":"exactAfterRestore"]=true;
  }
  const now=await snapshot();for(const s of slugs){if(now[s].uuid!==saved.snapshot[s].uuid||JSON.stringify(now[s].entries)!==JSON.stringify(saved.snapshot[s].entries)||JSON.stringify(now[s].balance)!==JSON.stringify(saved.snapshot[s].balance))throw Error(`Restored ledger/report changed: ${s}`);}
  const me=await(await page.request.get(`${base}/api/me`)).json();if(me.companies.length!==2||me.companies.some((c:any)=>c.role!=="owner"))throw Error("Explicit restored creator grants absent");
  const oldLogin=await page.request.post(`${base}/api/auth/sign-in/email`,{headers,data:{email:"source-owner@example.invalid",password}});if(oldLogin.ok())throw Error("Source credentials inherited");
  const doc=await page.request.get(company("synthetic",`documents/${saved.document.id}/file`));if(!doc.ok()||digest(await doc.body())!==saved.documentHash)throw Error("Restored original changed");
  const pdf=await page.request.get(company("synthetic",`invoices/${saved.invoice.documentId}/pdf`));if(!pdf.ok()||digest(await pdf.body())!==saved.pdfHash)throw Error("Restored invoice PDF changed");
  const bank=(await get("synthetic","bank")).bank;if(JSON.stringify(bank)!==JSON.stringify(saved.bank))throw Error("Restored bank projection changed");
  const before=(await get("synthetic","journal")).journal.entries;
  const refused=await page.request.post(company("synthetic","manual-journal"),{headers,data:saved.original});const refusal=await refused.json();
  if(refused.status()!==400||refusal.code!=="RESTORED_KEY_RECONCILIATION_REQUIRED"||JSON.stringify((await get("synthetic","journal")).journal.entries)!==JSON.stringify(before))throw Error(`Imported key became executable: ${refused.status()} ${JSON.stringify(refusal)}`);
  if(phase==="restored"){
   const fresh=structuredClone(saved.original);fresh.input.idempotencyKey="synthetic-portable-new-restored-intent";fresh.input.request.text="Synthetic restored fresh intended operation";await post("synthetic","manual-journal",fresh);
   const afterBank=(await get("synthetic","bank")).bank;
   if(JSON.stringify(afterBank.transactions)!==JSON.stringify(saved.bank.transactions)||afterBank.actualBalance!==saved.bank.actualBalance||Math.round(afterBank.bookedBalance*100)!==Math.round(saved.bank.bookedBalance*100)+1234)throw Error("Fresh intended post changed bank source or failed exact ledger effect");
   saved.bank=afterBank;saved.snapshot=await snapshot();
  }
 }
 writeFileSync(statePath,JSON.stringify(saved),{mode:0o600});
 console.log(`PASS: portable ${phase} ordinary verified/MFA owner, immutable ledger/original/PDF/bank and receipt boundary`);
}catch(e){console.error(await page.locator("body").innerText());await page.screenshot({path:join(work,`failure-${phase}.png`),fullPage:true});throw e;}finally{await browser.close();}
