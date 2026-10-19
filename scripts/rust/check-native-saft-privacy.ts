import type { Page } from "@playwright/test";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { isDeepStrictEqual } from "node:util";
import { readTar } from "../../src/core/tar";

type Contact = {id:number; name:string; address:string|null; email:string|null; vatOrCvr:string|null; gdprErased:boolean};
type Evidence = {customerId:number; vendorId:number; contacts:unknown; journal:unknown; documents:unknown; stableXml:string; audit:unknown[]; coverage:string};
/** Targeted empty-ledger privacy proof. Group23's nonempty accounting proof is separate. */
export async function checkNativeSaftPrivacy(page:Page,base:string,options:{isolated:true;company:string;work:string},restarted=false){
 if(options.isolated!==true||!/^http:\/\/127\.0\.0\.1:\d+$/.test(base))throw new Error("Explicit isolated loopback privacy fixture required");
 const url=`${base}/api/companies/${encodeURIComponent(options.company)}`, saved=join(options.work,"saft-privacy.json");
 const get=async(path:string)=>{const r=await page.request.get(`${url}/${path}`);if(!r.ok())throw new Error(`Privacy canonical read refused: ${path}`);return r.json();};
 const post=async(path:string,data:unknown)=>{const r=await page.request.post(`${url}/${path}`,{headers:{origin:base,"content-type":"application/json"},data});if(!r.ok())throw new Error(`Privacy fixture mutation refused: ${path} (${r.status()})`);return r.json();};
 const freeCustomer={name:"Synthetic SAF-T erased customer",vatOrCvr:"SYN-SAFT-PRIVACY-CUSTOMER",address:"Synthetic erased customer street",email:"saft-erased-customer@example.invalid"};
 // Supplier email/phone are deliberately outside the existing supplier erasure overlay.
 const freeVendor={name:"Synthetic SAF-T erased vendor",vatOrCvr:"SYN-SAFT-PRIVACY-VENDOR",address:"Synthetic erased vendor street"};
 const journal=(await get("journal")).journal, documents=await get("documents");
 if(journal.entries.length!==0)throw new Error("Targeted privacy fixture requires its fresh empty ledger");
 let evidence:Evidence|undefined=restarted?JSON.parse(readFileSync(saved,"utf8")):undefined;
 let customerId=evidence?.customerId, vendorId=evidence?.vendorId;
 if(!restarted){
  customerId=(await post("customers",freeCustomer)).customer.id;
  vendorId=(await post("vendors",freeVendor)).vendor.id;
  await post("customers",{name:"Synthetic SAF-T unrelated customer",vatOrCvr:"SYN-SAFT-UNRELATED-CUSTOMER",address:"Synthetic unchanged street",email:"saft-unchanged@example.invalid"});
  await post("vendors",{name:"Synthetic SAF-T unrelated vendor",vatOrCvr:"SYN-SAFT-UNRELATED-VENDOR",address:"Synthetic unchanged vendor street"});
  const before=(await get("contacts")).contacts;
  for(const [subject,source,id]of [[freeCustomer,"customers",customerId],[freeVendor,"vendors",vendorId]] as const){
   const body={name:subject.name,cvr:subject.vatOrCvr,confirm:true};
   const review=(await post("gdpr/export",body)).gdpr.export;
   if(!review.records.some((r:{source:string;sourceRowId:number;erasable:boolean})=>r.source===source&&r.sourceRowId===id&&r.erasable))throw new Error("Own unused subject missing reviewed erasable row");
   const result=(await post("gdpr/erase",body)).gdprErasure;
   if(result.erasedCount<1||result.coverage.complete!==false)throw new Error("Scoped erasure missing or complete-erasure claim");
  }
  const after=(await get("contacts")).contacts;
  for(const [table,id]of [["customers",customerId],["vendors",vendorId]] as const){
   if(!isDeepStrictEqual(before[table].filter((r:Contact)=>r.id!==id),after[table].filter((r:Contact)=>r.id!==id)))throw new Error("Unrelated contact changed during erasure");
  }
 }
 const contacts=(await get("contacts")).contacts;
 for(const [table,id]of [["customers",customerId],["vendors",vendorId]] as const){
  const row=contacts[table].find((r:Contact)=>r.id===id);
  if(!row?.gdprErased||row.name!=="[redigeret — GDPR]"||row.address!==null||row.vatOrCvr!==null||(table==="customers"&&row.email!==null))throw new Error("Canonical scoped contact overlay missing");
 }
 const overview=(await get("overview")).overview, body=JSON.stringify({periodStart:overview.periodStart,periodEnd:overview.periodEnd,confirm:true});
 await page.goto(`${base}/companies/${encodeURIComponent(options.company)}`);
 const card=page.locator(".accountant-export");await card.getByLabel("Fra",{exact:true}).fill(overview.periodStart);await card.getByLabel("Til",{exact:true}).fill(overview.periodEnd);
 const [download,response]=await Promise.all([page.waitForEvent("download"),page.waitForResponse(r=>r.url().endsWith("/saft-export")&&r.request().method()==="POST"),card.getByRole("button",{name:"SAF-T v3 — generér og download",exact:true}).click()]);
 if(!response.ok()||response.headers()["content-type"]!=="application/x-tar")throw new Error("Privacy browser SAF-T download refused");
 const tar=join(options.work,"saft-privacy-browser.tar");await download.saveAs(tar);
 const cookie=join(options.work,`saft-privacy${restarted?"-reopen":""}.cookie`);writeFileSync(cookie,(await page.context().cookies(base)).map(c=>`${c.name}=${c.value}`).join("; "),{mode:0o600,flag:"wx"});
 const common=["--server",base,"--allow-loopback-http","--cookie-file",cookie];
 if(!process.env.RM_NATIVE_CLI_BIN||!process.env.RM_NATIVE_MCP_BIN)throw new Error("Explicit pinned privacy clients required");
 const cli=spawnSync(process.env.RM_NATIVE_CLI_BIN,["remote","saft-export",...common,"--company",options.company,"--confirm"],{input:body,encoding:"utf8",maxBuffer:128*1024*1024});
 if(cli.error||cli.status!==0)throw new Error("Privacy actual CLI export failed");
 const input=[{jsonrpc:"2.0",id:1,method:"initialize",params:{protocolVersion:"2025-11-25",capabilities:{},clientInfo:{name:"synthetic-saft-privacy",version:"1"}}},{jsonrpc:"2.0",method:"notifications/initialized"},{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:"saft_export",arguments:{company:options.company,body,confirm:true}}}].map(v=>JSON.stringify(v)).join("\n")+"\n";
 const mcp=spawnSync(process.env.RM_NATIVE_MCP_BIN,common,{input,encoding:"utf8",maxBuffer:128*1024*1024});
 if(mcp.error||mcp.status!==0)throw new Error("Privacy actual MCP export failed");
 const call=mcp.stdout.trim().split("\n").map(l=>JSON.parse(l)).find(v=>v.id===2);
 if(call?.error||call?.result?.isError)throw new Error("Privacy MCP tool refused");
 const packages=[readFileSync(tar),...[JSON.parse(cli.stdout).data,call.result.structuredContent.data].map(value=>{if(value.export.contentType!=="application/x-tar")throw new Error("Privacy adapter MIME mismatch");return Buffer.from(value.export.bytes);})];
 let stableXml="",audit:unknown[]=[];
 for(const bytes of packages){
  const entries=readTar(bytes),files=new Map(entries.map(e=>[e.path.split("/").slice(1).join("/"),Buffer.from(e.content)]));
  if(entries.length!==3||files.size!==3||!files.has("README.txt")||!files.has("manifest.json")||!files.has("saft.xml"))throw new Error("Privacy SAF-T archive scope changed");
  const manifest=JSON.parse(files.get("manifest.json")!.toString("utf8")),xml=files.get("saft.xml")!.toString("utf8");
  for(const o of manifest.outputs){const file=files.get(o.path);if(!file||file.length!==o.sizeBytes||createHash("sha256").update(file).digest("hex")!==o.sha256)throw new Error("Privacy SAF-T manifest hash mismatch");}
  const parsed=await page.evaluate(xml=>{const doc=new DOMParser().parseFromString(xml,"application/xml");if(doc.querySelector("parsererror"))throw new Error("Invalid privacy XML");return ["Customer","Supplier"].map(tag=>Array.from(doc.querySelectorAll(`MasterFiles ${tag}`),node=>Object.fromEntries(Array.from(node.children,c=>[c.tagName,c.textContent??""]))));},xml);
  for(const [index,table,prefix]of [[0,"customers","Customer"],[1,"vendors","Supplier"]] as const){
   if(parsed[index].length!==contacts[table].length)throw new Error("Privacy export omitted contacts");
   for(const contact of contacts[table] as Contact[]){const row=parsed[index].find(r=>r[`${prefix}ID`]===String(contact.id));if(!row)throw new Error("Privacy XML contact identity missing");for(const [tag,field]of [[`${prefix}Name`,"name"],[`${prefix}TaxID`,"vatOrCvr"],["Address","address"],["Email","email"]] as const)if((row[tag]??"")!==String(contact[field]??""))throw new Error(`Privacy XML differs from canonical ${table}.${field}`);}
  }
  // Check only MasterFiles: retained audit history is intentionally not claimed erased.
  const master=JSON.stringify(parsed);for(const raw of [...Object.values(freeCustomer),...Object.values(freeVendor)])if(master.includes(raw))throw new Error("Erased master field resurrected in SAF-T");
  const normalized=xml.replace(/<DateCreated>[^<]*<\/DateCreated>/,"<DateCreated>trusted-runtime-clock</DateCreated>");if(stableXml&&normalized!==stableXml)throw new Error("Privacy browser/CLI/MCP XML differs");stableXml=normalized;
  const actualAudit=manifest.sourceContext.auditLog;if(!Array.isArray(actualAudit)||!isDeepStrictEqual(actualAudit.slice(0,audit.length),audit)||!isDeepStrictEqual(actualAudit.slice(0,evidence?.audit.length??0),evidence?.audit??[]))throw new Error("Protected audit prefix changed");audit=actualAudit;
  if(manifest.counts.journalEntries!==0||manifest.sourceContext.documents.length!==0)throw new Error("Targeted privacy export acquired accounting evidence");
 }
 if(!isDeepStrictEqual((await get("journal")).journal,journal)||!isDeepStrictEqual(await get("documents"),documents))throw new Error("Privacy mutations altered journal/original inventory");
 if(evidence){if(!isDeepStrictEqual(contacts,evidence.contacts)||stableXml!==evidence.stableXml||!isDeepStrictEqual(journal,evidence.journal)||!isDeepStrictEqual(documents,evidence.documents))throw new Error("Privacy canonical/export projection changed after restart");}
 else {evidence={customerId:customerId!,vendorId:vendorId!,contacts,journal,documents,stableXml,audit,coverage:"Targeted master-data overlay only; supplier email/phone outside overlay; retained audit prefix preserved; empty ledger/original inventory; Group23 nonempty accounting proof separate"};writeFileSync(saved,JSON.stringify(evidence),{mode:0o600,flag:"wx"});}
 console.log(`PASS: scoped GDPR customer/vendor master overlay equals actual browser/CLI/MCP SAF-T; unrelated contacts and protected audit/journal/original inventory preserved${restarted?" after fresh MFA restart":""}; bounded coverage`);
}
