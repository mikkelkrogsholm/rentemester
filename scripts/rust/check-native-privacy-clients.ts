import type { Page } from "@playwright/test";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { isDeepStrictEqual } from "node:util";
const saved = "/tmp/rentemester-rust-reports/native-privacy-clients.json";
type Evidence = {customerId:number; documentId:number; source:number[]; year:string; freeReport:unknown; retainedRecords:unknown; deadline:string};

/** Own free/retained subjects only; scoped overlay evidence, never physical deletion or backup claims. */
export async function checkNativePrivacyClients(page: Page, base: string, restarted = false) {
  const url = `${base}/api/companies/synthetic`;
  const temp = mkdtempSync("/tmp/rm-native-privacy-clients-");
  try {
    const cookie = join(temp, "session.cookie");
    writeFileSync(cookie, (await page.context().cookies(base)).map(c => `${c.name}=${c.value}`).join("; "), { mode: 0o600, flag: "wx" });
    const common = ["--server", base, "--allow-loopback-http", "--cookie-file", cookie];
    function cli(command: string, body?: string, flags: string[] = [], refused = false) {
      const run = spawnSync(process.env.RM_NATIVE_CLI_BIN ?? resolve("target/debug/rentemester-rust"), ["remote", command, ...common, "--company", "synthetic", ...flags, ...(body === undefined ? [] : ["--confirm"])], { input: body, encoding: "utf8", maxBuffer: 16 * 1024 * 1024 });
      if (run.error) throw new Error("Native privacy CLI unavailable; build rentemester-cli or set RM_NATIVE_CLI_BIN");
      const value = JSON.parse(run.stdout);
      if (refused) { if (run.status !== 1 || value.code !== "server_refused") throw new Error("Expected definite native privacy CLI refusal"); return value; }
      if (run.status !== 0) throw new Error(`Native privacy CLI ${command} failed`);
      return value.data;
    }
    function mcp(name: string, args: Record<string, unknown>, refused = false) {
      const input = [
        { jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-11-25", capabilities: {}, clientInfo: { name: "synthetic-accrual-client-check", version: "1" } } },
        { jsonrpc: "2.0", method: "notifications/initialized" },
        { jsonrpc: "2.0", id: 2, method: "tools/call", params: { name, arguments: { company: "synthetic", ...args } } },
      ].map(v => JSON.stringify(v)).join("\n") + "\n";
      const run = spawnSync(process.env.RM_NATIVE_MCP_BIN ?? resolve("target/debug/rentemester-mcp"), common, { input, encoding: "utf8", maxBuffer: 16 * 1024 * 1024 });
      if (run.error || run.status !== 0) throw new Error("Native privacy MCP unavailable; build rentemester-mcp or set RM_NATIVE_MCP_BIN");
      const call = run.stdout.trim().split("\n").map(line => JSON.parse(line)).find(v => v.id === 2);
      if (refused) { if (call?.result?.isError !== true || call.result.structuredContent.error.code !== "server_refused" || call.result.structuredContent.error.status !== 422) throw new Error("Expected definite native privacy MCP HTTP422 refusal"); return call.result; }
      if (call?.error || call?.result?.isError || !call?.result?.structuredContent?.data) throw new Error(`Native privacy MCP ${name} failed`);
      return call.result.structuredContent.data;
    }
    async function get(path: string) {
      const response = await page.request.get(`${url}/${path}`);
      if (!response.ok()) throw new Error(`Canonical privacy read ${path} failed (${response.status()})`);
      return response.json();
    }
    async function post(path: string, data: unknown) {
      const response = await page.request.post(`${url}/${path}`, { headers: { origin: base, "content-type": "application/json" }, data });
      if (!response.ok()) throw new Error(`Synthetic accrual setup ${path} failed: ${await response.text()}`);
      return response.json();
    }
    const free={name:"Synthetic Agent GDPR Free",cvr:"SYN-AGENT-GDPR-FREE"},retained={name:"Synthetic Agent GDPR Retained",cvr:"SYN-AGENT-GDPR-RETAINED"};
    const subjectBody=(subject:typeof free)=>JSON.stringify({...subject,confirm:true});
    // Each export appends an audit record (and can strengthen aliases). Only that documented,
    // growing audit_log collection is excluded from snapshot comparison; transports return every row.
    function comparable(value:unknown){const v=structuredClone(value) as {gdpr:{export:{records:Array<{source:string}>}}};v.gdpr.export.records=v.gdpr.export.records.filter(r=>r.source!=="audit_log");return v;}
    async function exported(subject:typeof free){return post("gdpr/export",{...subject,confirm:true});}
    let evidence:Evidence;
    const journal=(await get("journal")).journal;
    if(!restarted){
      const freeCustomer=cli("create-customer",JSON.stringify({name:free.name,vatOrCvr:free.cvr,address:"Synthetic Agent Privacy Street",email:"synthetic-agent-free@example.invalid"})).customer;
      cli("create-customer",JSON.stringify({name:retained.name,vatOrCvr:retained.cvr,address:"Synthetic Agent Retained Street",email:"synthetic-agent-retained@example.invalid"}));
      const retention=(await get("retention")).retention;
      const source=Buffer.from("Synthetic agent GDPR retained original; immutable evidence, not a physical-erasure target\n");
      const document=cli("ingest-document",JSON.stringify({confirm:true,fileName:"synthetic-agent-gdpr-retained.txt",fileBase64:source.toString("base64"),metadata:{source:"Synthetic agent GDPR retained fixture",documentType:"cash_register_receipt",issueDate:retention.report.asOf,sender:{name:retained.name,vatOrCvr:retained.cvr,address:"Synthetic Agent Retained Street"}}})).document;
      const initialFree=cli("gdpr-export",subjectBody(free));
      if(!initialFree.gdpr.export.records.some((r:{source:string;sourceRowId:number;erasable:boolean})=>r.source==="customers"&&r.sourceRowId===freeCustomer.id&&r.erasable))throw new Error("Own free subject has no reviewed erasable customer row");
      if(!isDeepStrictEqual(comparable(initialFree),comparable(await exported(free))))throw new Error("Full subject export differs beyond documented growing audit evidence");
      const before=cli("gdpr-export",subjectBody(retained));
      const record=before.gdpr.export.records.find((r:{source:string;sourceRowId:number})=>r.source==="documents"&&r.sourceRowId===document.id);
      if(!record?.underRetention||record.erasable||!record.retainUntil)throw new Error("Own original not retained in actual reviewed export");
      const future=await page.request.post(`${url}/gdpr/erase`,{headers:{origin:base,"content-type":"application/json"},data:{...retained,confirm:true,asOf:"9999-12-31"}});
      if(future.status()!==400)throw new Error("Server accepted caller-controlled erasure date");
      const refused=mcp("gdpr_erase",{body:subjectBody(retained),confirm:true}).gdprErasure;
      if(refused.erasedCount!==0||refused.refusedCount<2||refused.refused.length<2||refused.coverage.complete!==false)throw new Error("Retention-bounded overlay lost refused rows/partial coverage");
      cli("gdpr-export",JSON.stringify({cvr:free.cvr,name:retained.name,confirm:true}),[],true);
      // Explicit scoped overlay follows review of that same free subject, never an automatic action.
      const erased=mcp("gdpr_erase",{body:subjectBody(free),confirm:true}).gdprErasure;
      if(erased.erasedCount<1||erased.coverage.complete!==false)throw new Error("Confirmed free-subject overlay missing or advertised complete erasure");
      const after=await exported(free);
      if(!after.gdpr.export.records.some((r:{source:string;sourceRowId:number;erased:boolean;personalData:{email:unknown}})=>r.source==="customers"&&r.sourceRowId===freeCustomer.id&&r.erased&&r.personalData.email===null))throw new Error("Free customer report overlay not applied");
      const retainedNow=await exported(retained);
      evidence={customerId:freeCustomer.id,documentId:document.id,source:[...source],year:journal.selectedYear,freeReport:comparable(after),retainedRecords:retainedNow.gdpr.export.records.filter((r:{source:string})=>r.source!=="audit_log"),deadline:record.retainUntil};writeFileSync(saved,JSON.stringify(evidence));
    }else evidence=JSON.parse(readFileSync(saved,"utf8"));
    const http=await exported(free);
    const cliReport=cli("gdpr-export",subjectBody(free));
    const mcpReport=mcp("gdpr_export",{body:subjectBody(free),confirm:true});
    if(!isDeepStrictEqual(comparable(http),evidence.freeReport)||!isDeepStrictEqual(comparable(cliReport),comparable(http))||!isDeepStrictEqual(comparable(mcpReport),comparable(http))||http.gdpr.export.coverage.complete!==false)throw new Error("Subject-scoped overlay/partial coverage changed beyond documented export audit rows");
    const retainedNow=await exported(retained);
    if(!isDeepStrictEqual(retainedNow.gdpr.export.records.filter((r:{source:string})=>r.source!=="audit_log"),evidence.retainedRecords)||!retainedNow.gdpr.export.records.some((r:{source:string;sourceRowId:number;underRetention:boolean;erased:boolean;retainUntil:string})=>r.source==="documents"&&r.sourceRowId===evidence.documentId&&r.underRetention&&!r.erased&&r.retainUntil===evidence.deadline))throw new Error("Retained source facts/deadline changed");
    const retention=await get("retention");
    if(!isDeepStrictEqual(cli("retention"),retention)||!isDeepStrictEqual(mcp("retention",{}),retention))throw new Error("Full native retention/unknown/deadline report differs across HTTP/CLI/MCP");
    const contacts=(await get("contacts")).contacts;
    const customer=contacts.customers.find((c:{id:number})=>c.id===evidence.customerId);
    if(!customer||customer.name!=="[redigeret — GDPR]"||customer.email!==null||customer.vatOrCvr!==null)throw new Error("Scoped master overlay resurfaced private fields");
    const bypass=await page.request.patch(`${url}/customers/${evidence.customerId}`,{headers:{origin:base,"content-type":"application/json"},data:{name:"Synthetic agent identity bypass"}});
    if(bypass.status()!==422)throw new Error("Prior erased row identity fence bypassed");
    cli("create-customer",JSON.stringify({name:"Synthetic agent alternative erased label",vatOrCvr:free.cvr}),[],true);
    const original=await page.request.get(`${url}/documents/${evidence.documentId}/file`);
    if(!original.ok()||!isDeepStrictEqual([...await original.body()],evidence.source))throw new Error("Protected original bytes changed");
    for(const value of[cli("document-original",undefined,["--id",String(evidence.documentId)]),mcp("document_original",{id:evidence.documentId})])if(!isDeepStrictEqual(value.original.bytes,evidence.source))throw new Error("Agent protected-original bytes differ from canonical HTTP");
    const actual=(await get(`journal?year=${encodeURIComponent(evidence.year)}`)).journal.entries;
    if(!isDeepStrictEqual(actual,journal.entries))throw new Error("Privacy export/overlay altered financial journals");
    console.log(restarted?"PASS: scoped agent overlays and stable alias fences survive restart; retained originals/journals preserved and full retention/normalized export parity":"PASS: own free/retained subjects → audited CLI export/review → explicit MCP scoped overlay; retained refusal/partial coverage preserved; no caller erasure date, alias bypass or protected-original/journal mutation");
  }finally{rmSync(temp,{recursive:true,force:true});}
}
