import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import type { Page } from "@playwright/test";
import { readTar } from "../../src/core/tar";

const saved = "/tmp/rentemester-rust-reports/native-saft.json";
const profile = "rentemester-dk-saft-v3-ledger-sales-purchases-masterfiles";
function unpack(bytes: Buffer) {
  const entries = readTar(bytes), files = new Map(entries.map(e => [e.path.split("/").slice(1).join("/"), Buffer.from(e.content)]));
  if (files.size !== 3 || !files.has("README.txt") || !files.has("manifest.json") || !files.has("saft.xml")) throw new Error("SAF-T package changed file scope");
  const manifest = JSON.parse(files.get("manifest.json")!.toString("utf8"));
  for (const o of manifest.outputs as Array<{path:string;sha256:string;sizeBytes:number}>) {
    const file=files.get(o.path); if (!file || file.length!==o.sizeBytes || createHash("sha256").update(file).digest("hex")!==o.sha256) throw new Error("SAF-T manifest hash mismatch");
  }
  if (manifest.profileId!==profile || manifest.manifestVersion!==2 || manifest.generatedAtSource!=="trusted_runtime_clock" || manifest.provenance!==null || manifest.sourceContext.journalHashChain.ok!==true || !manifest.outOfScope.includes("official_schema_submission")) throw new Error("SAF-T profile/source/provenance contract mismatch");
  const xml=files.get("saft.xml")!.toString("utf8");
  return {manifest,xml,stableXml:xml.replace(/<DateCreated>[^<]*<\/DateCreated>/,"<DateCreated>trusted-runtime-clock</DateCreated>")};
}
/** Standalone acceptance: existing normal MFA browser session; call initial/restart, no harness edits. */
export async function checkNativeSaft(page:Page,base:string,restarted=false){
 const url=`${base}/api/companies/synthetic`,headers={origin:base,"content-type":"application/json"};
 const overview=(await(await page.request.get(`${url}/overview`)).json()).overview;
 const {periodStart,periodEnd,selectedYear}=overview;
 const get=async(path:string)=>{const r=await page.request.get(`${url}/${path}`);if(!r.ok())throw new Error(`SAF-T canonical read refused ${path}`);return r.json();};
 const before=(await get(`journal?year=${encodeURIComponent(selectedYear)}`)).journal;
 const tb=(await get(`trial-balance?year=${encodeURIComponent(selectedYear)}`)).trialBalance;
 const accounts=(await get("accounts")).accounts;
 for(const data of [{periodStart:"2026-02-30",periodEnd,confirm:true},{periodStart,periodEnd,confirm:false},{periodStart,periodEnd,confirm:true,generatedAt:"2000-01-01T00:00:00Z"}]){
  const r=await page.request.post(`${url}/saft-export`,{headers,data});if(![400,422].includes(r.status()))throw new Error("SAF-T invalid dates/confirmation/caller clock accepted");
 }
 const wrong=await page.request.post(`${base}/api/companies/nonexistent/saft-export`,{headers,data:{periodStart,periodEnd,confirm:true}});if(wrong.status()!==404)throw new Error("SAF-T company boundary failed");
 await page.goto(`${base}/companies/synthetic`);
 const card=page.locator(".accountant-export");await card.getByLabel("Fra",{exact:true}).fill(periodStart);await card.getByLabel("Til",{exact:true}).fill(periodEnd);
 const [download,response]=await Promise.all([page.waitForEvent("download"),page.waitForResponse(r=>r.url().endsWith("/saft-export")&&r.request().method()==="POST"),card.getByRole("button",{name:"SAF-T v3 — generér og download",exact:true}).click()]);
 if(!response.ok()||response.headers()["content-type"]!=="application/x-tar"||response.headers()["cache-control"]!=="private, no-store")throw new Error("Native SAF-T browser download failed");
 const temp=mkdtempSync("/tmp/rm-native-saft-");
 try{
  const path=join(temp,"package.tar");await download.saveAs(path);const browser=unpack(readFileSync(path));
  const parsed=await page.evaluate(xml=>{const doc=new DOMParser().parseFromString(xml,"application/xml");if(doc.querySelector("parsererror"))throw new Error("Malformed native SAF-T XML");const text=(tag:string)=>Array.from(doc.getElementsByTagName(tag),n=>n.textContent);return {accounts:text("AccountID"),transactions:text("TransactionID"),sources:text("SourceDocumentID"),profile:text("ProfileID"),quantities:text("Quantity"),sales:doc.getElementsByTagName("SalesInvoices")[0]?.children.length,purchases:doc.getElementsByTagName("PurchaseInvoices")[0]?.children.length};},browser.xml);
  if(parsed.profile[0]!==profile||parsed.transactions.length!==before.entries.length||browser.manifest.counts.journalEntries!==before.entries.length)throw new Error("SAF-T omitted selected ledger entries");
  const chart=Array.isArray(accounts)?accounts:accounts.accounts;
  for(const a of chart)if(!parsed.accounts.includes(a.accountNo))throw new Error("SAF-T omitted chart account");
  const source=browser.manifest.sourceContext;
  if(source.trialBalance.totalDebit!==String(tb.totalDebit.toFixed(2))||source.trialBalance.totalCredit!==String(tb.totalCredit.toFixed(2)))throw new Error("SAF-T differs from shared trial balance");
  for(const d of source.documents){const r=await page.request.get(`${url}/documents/${d.id}/file`);if(!r.ok()||createHash("sha256").update(await r.body()).digest("hex")!==d.sha256)throw new Error("SAF-T source-original evidence mismatch");}
  for(const row of source.journalEntries){if(!row.registrationDatetime||!row.createdBy||!row.ruleVersion||!row.hash||!Array.isArray(row.lines))throw new Error("SAF-T omitted original registration/audit/rule/hash context");if(row.documentId&&!parsed.sources.includes(row.documentNo))throw new Error("SAF-T missing source document reference");}
  // Each export has its own authoritative clock and audit event. Compare the XML accounting projection, never forge deterministic runtime timestamps.
  const cookieFile=join(temp,"session.cookie");writeFileSync(cookieFile,(await page.context().cookies(base)).map(c=>`${c.name}=${c.value}`).join("; "),{mode:0o600,flag:"wx"});
  const common=["--server",base,"--allow-loopback-http","--cookie-file",cookieFile],body=JSON.stringify({periodStart,periodEnd,confirm:true});
  const cli=spawnSync(process.env.RM_NATIVE_CLI_BIN??resolve("target/debug/rentemester-rust"),["remote","saft-export",...common,"--company","synthetic","--confirm"],{input:body,encoding:"utf8",maxBuffer:128*1024*1024});if(cli.error||cli.status!==0)throw new Error("SAF-T actual CLI failed");
  const input=[{jsonrpc:"2.0",id:1,method:"initialize",params:{protocolVersion:"2025-11-25",capabilities:{},clientInfo:{name:"synthetic-saft",version:"1"}}},{jsonrpc:"2.0",method:"notifications/initialized"},{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:"saft_export",arguments:{company:"synthetic",body,confirm:true}}}].map(v=>JSON.stringify(v)).join("\n")+"\n";
  const mcp=spawnSync(process.env.RM_NATIVE_MCP_BIN??resolve("target/debug/rentemester-mcp"),common,{input,encoding:"utf8",maxBuffer:128*1024*1024});if(mcp.error||mcp.status!==0)throw new Error("SAF-T actual MCP failed");const call=mcp.stdout.trim().split("\n").map(l=>JSON.parse(l)).find(v=>v.id===2);if(call?.error||call?.result?.isError)throw new Error("SAF-T MCP tool refused");
  for(const value of [JSON.parse(cli.stdout).data,call.result.structuredContent.data]){if(value.export.contentType!=="application/x-tar")throw new Error("SAF-T adapter MIME mismatch");const candidate=unpack(Buffer.from(value.export.bytes));if(candidate.stableXml!==browser.stableXml)throw new Error("SAF-T HTTP/browser/CLI/MCP accounting projection differs");}
  const after=(await get(`journal?year=${encodeURIComponent(selectedYear)}`)).journal;if(JSON.stringify(after)!==JSON.stringify(before))throw new Error("SAF-T export changed ledger");
  mkdirSync("/tmp/rentemester-rust-reports",{recursive:true});
  const evidence={periodStart,periodEnd,stableXml:browser.stableXml,journal:source.journalEntries,documents:source.documents,accounts:browser.manifest.counts.accounts};
  if(!restarted)writeFileSync(saved,JSON.stringify(evidence),{mode:0o600});else if(JSON.stringify(JSON.parse(readFileSync(saved,"utf8")))!==JSON.stringify(evidence))throw new Error("SAF-T source projection changed after restart");
  console.log(`PASS: native SAF-T v3 browser/CLI/MCP tar, exact XML/manifest/source originals/shared trial balance; ledger unchanged${restarted?" after restart":""}; no official validation/submission claim`);
 }finally{rmSync(temp,{recursive:true,force:true});}
}
