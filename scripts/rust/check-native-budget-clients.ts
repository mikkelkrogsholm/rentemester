import type { Page } from "@playwright/test";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { isDeepStrictEqual } from "node:util";
const saved = "/tmp/rentemester-rust-reports/native-budget-clients.json";
type Evidence = {year:string; period:string; firstId:number; lastId:number; row:unknown; entry:{id:number}};

/** Own source actual and dimension; account revisions and native comparisons create no journal. */
export async function checkNativeBudgetClients(page: Page, base: string, restarted = false) {
  const url = `${base}/api/companies/synthetic`;
  const temp = mkdtempSync("/tmp/rm-native-budget-clients-");
  try {
    const cookie = join(temp, "session.cookie");
    writeFileSync(cookie, (await page.context().cookies(base)).map(c => `${c.name}=${c.value}`).join("; "), { mode: 0o600, flag: "wx" });
    const common = ["--server", base, "--allow-loopback-http", "--cookie-file", cookie];
    function cli(command: string, body?: string, flags: string[] = [], refused = false) {
      const run = spawnSync(process.env.RM_NATIVE_CLI_BIN ?? resolve("target/debug/rentemester-rust"), ["remote", command, ...common, "--company", "synthetic", ...flags, ...(body === undefined ? [] : ["--confirm"])], { input: body, encoding: "utf8", maxBuffer: 16 * 1024 * 1024 });
      if (run.error) throw new Error("Native budget CLI unavailable; build rentemester-cli or set RM_NATIVE_CLI_BIN");
      const value = JSON.parse(run.stdout);
      if (refused) { if (run.status !== 1 || value.code !== "server_refused") throw new Error("Expected definite native budget CLI refusal"); return value; }
      if (run.status !== 0) throw new Error(`Native budget CLI ${command} failed`);
      return value.data;
    }
    function mcp(name: string, args: Record<string, unknown>, refused = false) {
      const input = [
        { jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-11-25", capabilities: {}, clientInfo: { name: "synthetic-budget-client-check", version: "1" } } },
        { jsonrpc: "2.0", method: "notifications/initialized" },
        { jsonrpc: "2.0", id: 2, method: "tools/call", params: { name, arguments: { company: "synthetic", ...args } } },
      ].map(v => JSON.stringify(v)).join("\n") + "\n";
      const run = spawnSync(process.env.RM_NATIVE_MCP_BIN ?? resolve("target/debug/rentemester-mcp"), common, { input, encoding: "utf8", maxBuffer: 16 * 1024 * 1024 });
      if (run.error || run.status !== 0) throw new Error("Native budget MCP unavailable; build rentemester-mcp or set RM_NATIVE_MCP_BIN");
      const call = run.stdout.trim().split("\n").map(line => JSON.parse(line)).find(v => v.id === 2);
      if (refused) { if (call?.result?.isError !== true || call.result.structuredContent.error.code !== "server_refused" || call.result.structuredContent.error.status !== 422) throw new Error("Expected definite native budget MCP HTTP422 refusal"); return call.result; }
      if (call?.error || call?.result?.isError || !call?.result?.structuredContent?.data) throw new Error(`Native budget MCP ${name} failed`);
      return call.result.structuredContent.data;
    }
    async function get(path: string) {
      const response = await page.request.get(`${url}/${path}`);
      if (!response.ok()) throw new Error(`Canonical budget read ${path} failed (${response.status()})`);
      return response.json();
    }
    async function post(path: string, data: unknown) {
      const response = await page.request.post(`${url}/${path}`, { headers: { origin: base, "content-type": "application/json" }, data });
      if (!response.ok()) throw new Error(`Synthetic budget setup ${path} failed: ${await response.text()}`);
      return response.json();
    }
    let evidence:Evidence;
    const account="3738",dimension="synthetic-agent-budget-project",member="owned";
    if(!restarted){
      for(const[accountNo,name,type,normalBalance]of[[account,"Synthetic agent budget expense","expense","debit"],["8838","Synthetic agent budget equity","equity","credit"]])await post("accounts",{accountNo,name,type,normalBalance});
      const date=new Date().toLocaleDateString("sv-SE",{timeZone:"Europe/Copenhagen"});
      const document=cli("ingest-document",JSON.stringify({confirm:true,fileName:"synthetic-agent-budget.txt",fileBase64:Buffer.from("Synthetic agent budget expense actual DKK20.11, no VAT, contributed by owner\n").toString("base64"),metadata:{source:"synthetic-native-budget-clients",documentType:"cash_register_receipt",issueDate:date,amountIncVat:20.11,vatAmount:0,currency:"DKK"}})).document;
      const state=JSON.parse(readFileSync(process.env.RM_NATIVE_WEB_STATE!,"utf8")),posting=JSON.parse(state.postedBody);
      posting.input.expectedRuleVersion=(await get("journal")).journal.nativePosting.ruleVersion;
      posting.input.idempotencyKey="synthetic-agent-budget-actual";
      posting.input.request={transactionDate:date,text:"Synthetic agent budget actual",currency:"DKK",documentId:String(document.id),lines:[{accountNo:account,debitAmount:"20.11",creditAmount:"0.00"},{accountNo:"8838",debitAmount:"0.00",creditAmount:"20.11"}]};
      await post("manual-journal",posting);
      const year=(await get("budget")).budget.selectedYear;
      const journal=(await get(`journal?year=${encodeURIComponent(year)}`)).journal;
      const entry=journal.entries.find((e:{text:string})=>e.text==="Synthetic agent budget actual");
      const line=entry?.lines.find((l:{accountNo:string})=>l.accountNo===account)?.journalLineId;
      if(!line)throw new Error("Own actual source journal line missing");
      await post("dimensions/define",{dimensionId:dimension,kind:"project",name:"Synthetic agent budget project",confirm:true});
      await post("dimensions/member",{dimensionId:dimension,memberId:member,name:"Synthetic agent budget owned",confirm:true});
      const assignment={journalLineId:line,source:"reviewed",allocations:[{dimensionId:dimension,memberId:member,amountMinor:2011,currency:"DKK"}]};
      const plan=(await post("dimensions/plan",assignment)).plan;
      await post("dimensions/apply",{...assignment,planHash:plan.planHash,confirm:true});
      const baseline=(await get(`journal?year=${encodeURIComponent(year)}`)).journal.entries;
      const period=date.slice(0,7);
      const first=cli("set-budget",JSON.stringify({accountNo:account,period,amount:25.09,notes:"Synthetic first account plan"})).budget;
      const last=mcp("set_budget",{body:JSON.stringify({accountNo:account,period,amount:30.19,notes:"Synthetic reviewed account revision"}),confirm:true}).budget;
      if(last.id<=first.id)throw new Error("Budget edit did not append revision");
      cli("set-budget",JSON.stringify({accountNo:account,period:"2026-13",amount:1}),[],true);
      if(!isDeepStrictEqual((await get(`journal?year=${encodeURIComponent(year)}`)).journal.entries,baseline))throw new Error("Budget revisions/refusal changed ledger");
      const scoped=await get(`budget-dimension-actuals?year=${encodeURIComponent(year)}&dimension=${dimension}&member=${member}`);
      const row=scoped.budgetDimensionActuals.comparison.find((r:{accountNo:string;period:string})=>r.accountNo===account&&r.period===period);
      if(!row||row.accountBudget!==30.19||row.accountActual!==20.11||row.actual!==20.11||row.budget!==null||row.variance!==null||!row.journalLineIds.includes(line))throw new Error("Native scoped actual/source/context or unknown dimension budget semantics incorrect");
      evidence={year,period,firstId:first.id,lastId:last.id,row,entry};writeFileSync(saved,JSON.stringify(evidence));
    }else evidence=JSON.parse(readFileSync(saved,"utf8"));
    const before=(await get(`journal?year=${encodeURIComponent(evidence.year)}`)).journal.entries;
    for(const[path,command,tool]of[["budget","budget","budget"],["budget-vs-actual","budget-vs-actual","budget_vs_actual"],["budget-dimension-actuals","budget-dimension-actuals","budget_dimension_actuals"]]){const http=await get(`${path}?year=${encodeURIComponent(evidence.year)}`);if(!isDeepStrictEqual(cli(command,undefined,["--year",evidence.year]),http)||!isDeepStrictEqual(mcp(tool,{year:evidence.year}),http))throw new Error(`Full native ${path} differs across HTTP/CLI/MCP`);}
    const scoped=await get(`budget-dimension-actuals?year=${encodeURIComponent(evidence.year)}&dimension=${dimension}&member=${member}`);
    if(!isDeepStrictEqual(cli("budget-dimension-actuals",undefined,["--year",evidence.year,"--dimension",dimension,"--member",member]),scoped)||!isDeepStrictEqual(mcp("budget_dimension_actuals",{year:evidence.year,dimension,member}),scoped))throw new Error("Full scoped comparison/advisory/nulls differ across clients");
    const row=scoped.budgetDimensionActuals.comparison.find((r:{accountNo:string;period:string})=>r.accountNo===account&&r.period===evidence.period);
    if(!isDeepStrictEqual(row,evidence.row)||scoped.budgetDimensionActuals.budgetScope!=="account"||scoped.budgetDimensionActuals.serverCalculated!==true)throw new Error("Own scoped budget facts changed after restart or inferred allocation appeared");
    const latest=(await get(`budget?year=${encodeURIComponent(evidence.year)}`)).budget.lines.find((l:{accountNo:string;period:string})=>l.accountNo===account&&l.period===evidence.period);
    if(latest?.id!==evidence.lastId||latest.amount!==30.19||!isDeepStrictEqual(before.find((e:{id:number})=>e.id===evidence.entry.id),evidence.entry))throw new Error("Latest account revision or own source actual changed");
    const invalid=await page.request.get(`${url}/budget-dimension-actuals?year=${encodeURIComponent(evidence.year)}&member=${member}`);
    if(invalid.status()!==422)throw new Error("Unscoped member accepted by native server");
    if(!isDeepStrictEqual((await get(`journal?year=${encodeURIComponent(evidence.year)}`)).journal.entries,before))throw new Error("Budget read journey produced journal effects");
    console.log(restarted?"PASS: agent budget latest revision, actual and source-bound scoped dimension comparison persist; no ledger effects":"PASS: own source actual/dimension → CLI account budget → MCP append revision → full native budget/actual/scoped parity; null unreviewed dimension variance retained; invalid month/member refused; no budget journal effects");
  }finally{rmSync(temp,{recursive:true,force:true});}
}
