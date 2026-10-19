import type { Page } from "@playwright/test";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { isDeepStrictEqual } from "node:util";

/** Own fresh workspace only: new ledgers and memberships must never alter another journey. */
export async function checkNativeCompanyCreationClients(page:Page,base:string,options:{isolated:true;primary:string;evidencePath:string},restarted=false){
  if(options.isolated!==true||!process.env.RM_NATIVE_CLI_BIN||!process.env.RM_NATIVE_MCP_BIN)throw new Error("Explicit isolated company creation fixture and built clients required");
  const work=mkdtempSync("/tmp/rm-company-creation-clients-");
  try{
    const cookie=join(work,"session.cookie");writeFileSync(cookie,(await page.context().cookies(base)).map(c=>`${c.name}=${c.value}`).join("; "),{mode:0o600,flag:"wx"});
    const common=["--server",base,"--allow-loopback-http","--cookie-file",cookie];
    function cli(command:string,company?:string,body?:string,refused=false){
      const run=spawnSync(process.env.RM_NATIVE_CLI_BIN!,["remote",command,...common,...(company?["--company",company]:[]),...(body===undefined?[]:["--confirm"])],{input:body,encoding:"utf8",maxBuffer:16*1024*1024});
      if(run.error)throw new Error("Company creation CLI unavailable");const value=JSON.parse(run.stdout);
      if(refused){if(run.status!==1||value.code!=="server_refused")throw new Error("Expected definite creation refusal");return value;}
      if(run.status!==0)throw new Error(`Company creation CLI ${command} failed`);return value.data;
    }
    function mcp(name:string,args:Record<string,unknown>,refused=false){
      const input=[{jsonrpc:"2.0",id:1,method:"initialize",params:{protocolVersion:"2025-11-25",capabilities:{},clientInfo:{name:"synthetic-company-creation",version:"1"}}},{jsonrpc:"2.0",method:"notifications/initialized"},{jsonrpc:"2.0",id:2,method:"tools/call",params:{name,arguments:args}}].map(v=>JSON.stringify(v)).join("\n")+"\n";
      const run=spawnSync(process.env.RM_NATIVE_MCP_BIN!,common,{input,encoding:"utf8",maxBuffer:16*1024*1024});
      if(run.error||run.status!==0)throw new Error("Company creation MCP unavailable");const call=run.stdout.trim().split("\n").map(v=>JSON.parse(v)).find(v=>v.id===2);
      if(refused){if(call?.result?.isError!==true||call.result.structuredContent.error.code!=="server_refused"||call.result.structuredContent.error.status!==422)throw new Error("Expected definite creation HTTP422 refusal");return call.result;}
      if(call?.error||call?.result?.isError||!call?.result?.structuredContent?.data)throw new Error(`Company creation MCP ${name} failed`);return call.result.structuredContent.data;
    }
    async function get(path:string){const r=await page.request.get(`${base}/api/${path}`);if(!r.ok())throw new Error(`Company creation canonical read refused (${r.status()})`);return r.json();}
    const slugs=["synthetic-agent-company-cli","synthetic-agent-company-mcp"];
    const before=(await get(`companies/${options.primary}/journal`)).journal;
    if(!Array.isArray(before.entries)||before.entries.length!==0)throw new Error("Fresh primary ledger required for company creation qualification");
    if(!restarted){
      const body=JSON.stringify({name:"Synthetic Agent CLI Company",slug:slugs[0],fiscalYearStartMonth:"7",fiscalYearLabelStrategy:"span",vatPeriodType:null});
      const created=cli("create-company",undefined,body);if(created.company.slug!==slugs[0])throw new Error("CLI creation returned wrong company");
      cli("create-company",undefined,body,true);
      mcp("create_company",{body:JSON.stringify({name:"Synthetic Agent MCP Company",slug:slugs[1],fiscalYearStartMonth:"1",fiscalYearLabelStrategy:"end-year",vatPeriodType:"quarter"}),confirm:true});
      mcp("create_company",{body:JSON.stringify({name:"Synthetic Invalid Calendar",slug:"synthetic-agent-invalid-calendar",fiscalYearStartMonth:"99"}),confirm:true},true);
    }
    const companies=await get("companies");
    if(companies.count!==3||!isDeepStrictEqual(cli("companies"),companies)||!isDeepStrictEqual(mcp("companies",{}),companies))throw new Error("Full HTTP/CLI/MCP company catalog parity missing");
    const snapshots=[];const uuids=new Set([before.ledgerUuid]);
    for(const slug of slugs){
      const profile=await get(`companies/${slug}/company`),accounts=await get(`companies/${slug}/accounts`),journal=await get(`companies/${slug}/journal`);
      if(!isDeepStrictEqual(profile,cli("company-profile",slug))||!isDeepStrictEqual(profile,mcp("company_profile",{company:slug}))||!isDeepStrictEqual(accounts,cli("accounts",slug))||!isDeepStrictEqual(accounts,mcp("accounts",{company:slug}))||!isDeepStrictEqual(journal,cli("journal",slug))||!isDeepStrictEqual(journal,mcp("journal",{company:slug})))throw new Error("Full new-company canonical parity missing");
      if(accounts.accounts.accounts.length!==0||journal.journal.entries.length!==0||typeof journal.journal.ledgerUuid!=="string"||uuids.has(journal.journal.ledgerUuid))throw new Error("New company copied accounts/journals or reused ledger UUID");
      uuids.add(journal.journal.ledgerUuid);snapshots.push({slug,profile,accounts,journal});
    }
    if(snapshots[0].profile.company.fiscalYearStartMonth!==7||snapshots[0].profile.company.fiscalYearLabelStrategy!=="span"||snapshots[0].profile.company.vatPeriodType!==null||snapshots[1].profile.company.vatPeriodType!=="quarter")throw new Error("Caller-selected company calendar/VAT settings not preserved");
    const after=(await get(`companies/${options.primary}/journal`)).journal;if(!isDeepStrictEqual(after,before))throw new Error("Creation changed the primary ledger");
    const evidence={primaryLedgerUuid:before.ledgerUuid,companies,snapshots,distinctLedgerUuids:uuids.size,noCopiedChartOrJournal:true};
    if(restarted){if(!isDeepStrictEqual(evidence,JSON.parse(readFileSync(options.evidencePath,"utf8"))))throw new Error("Company creation state changed after fresh MFA/restart");}
    else writeFileSync(options.evidencePath,JSON.stringify(evidence),{mode:0o600,flag:"wx"});
    console.log(restarted?"PASS: CLI/MCP-created distinct company UUIDs, exact calendars/VAT, empty separate ledgers and full catalog/profile/chart/journal parity survive fresh MFA restart":"PASS: CLI and MCP create actual separate companies; duplicate/invalid calendar refused, explicit calendars/VAT and distinct UUIDs preserved; full canonical reads, primary ledger unchanged");
  }finally{rmSync(work,{recursive:true,force:true});}
}
