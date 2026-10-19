import type { Page } from "@playwright/test";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { isDeepStrictEqual } from "node:util";
const saved = "/tmp/rentemester-rust-reports/native-asset-clients.json";
type Evidence = { assetId:number; documentId:number; date:string; body:string; asset:unknown; next:unknown; journalEntry:{id:number}; source:number[]; amount:number; period:number };

/** Dedicated purchase original, actual capitalisation, reviewed depreciation and canonical accounting. */
export async function checkNativeAssetClients(page: Page, base: string, restarted = false) {
  const url = `${base}/api/companies/synthetic`;
  const temp = mkdtempSync("/tmp/rm-native-asset-clients-");
  try {
    const cookie = join(temp, "session.cookie");
    writeFileSync(cookie, (await page.context().cookies(base)).map(c => `${c.name}=${c.value}`).join("; "), { mode: 0o600, flag: "wx" });
    const common = ["--server", base, "--allow-loopback-http", "--cookie-file", cookie];
    function cli(command: string, body?: string, flags: string[] = [], refused = false) {
      const run = spawnSync(process.env.RM_NATIVE_CLI_BIN ?? resolve("target/debug/rentemester-rust"), ["remote", command, ...common, "--company", "synthetic", ...flags, ...(body === undefined ? [] : ["--confirm"])], { input: body, encoding: "utf8", maxBuffer: 16 * 1024 * 1024 });
      if (run.error) throw new Error("Native asset CLI unavailable; build rentemester-cli or set RM_NATIVE_CLI_BIN");
      const value = JSON.parse(run.stdout);
      if (refused) { if (run.status !== 1 || value.code !== "server_refused") throw new Error("Expected definite native asset CLI refusal"); return value; }
      if (run.status !== 0) throw new Error(`Native asset CLI ${command} failed`);
      return value.data;
    }
    function mcp(name: string, args: Record<string, unknown>, refused = false) {
      const input = [
        { jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-11-25", capabilities: {}, clientInfo: { name: "synthetic-asset-client-check", version: "1" } } },
        { jsonrpc: "2.0", method: "notifications/initialized" },
        { jsonrpc: "2.0", id: 2, method: "tools/call", params: { name, arguments: { company: "synthetic", ...args } } },
      ].map(v => JSON.stringify(v)).join("\n") + "\n";
      const run = spawnSync(process.env.RM_NATIVE_MCP_BIN ?? resolve("target/debug/rentemester-mcp"), common, { input, encoding: "utf8", maxBuffer: 16 * 1024 * 1024 });
      if (run.error || run.status !== 0) throw new Error("Native asset MCP unavailable; build rentemester-mcp or set RM_NATIVE_MCP_BIN");
      const call = run.stdout.trim().split("\n").map(line => JSON.parse(line)).find(v => v.id === 2);
      if (refused) { if (call?.result?.isError !== true || call.result.structuredContent.error.code !== "server_refused" || call.result.structuredContent.error.status !== 422) throw new Error("Expected definite native asset MCP HTTP422 refusal"); return call.result; }
      if (call?.error || call?.result?.isError || !call?.result?.structuredContent?.data) throw new Error(`Native asset MCP ${name} failed`);
      return call.result.structuredContent.data;
    }
    async function get(path: string) {
      const response = await page.request.get(`${url}/${path}`);
      if (!response.ok()) throw new Error(`Canonical asset read ${path} failed (${response.status()})`);
      return response.json();
    }
    async function post(path: string, data: unknown) {
      const response = await page.request.post(`${url}/${path}`, { headers: { origin: base, "content-type": "application/json" }, data });
      if (!response.ok()) throw new Error(`Synthetic asset setup ${path} failed: ${await response.text()}`);
      return response.json();
    }
    let evidence:Evidence;
    if(!restarted){
      for(const[accountNo,name,type,normalBalance]of[["1518","Synthetic agent asset","asset","debit"],["1598","Synthetic agent accumulated depreciation","asset","credit"],["3918","Synthetic agent depreciation expense","expense","debit"],["8818","Synthetic agent capital contribution","equity","credit"]])await post("accounts",{accountNo,name,type,normalBalance});
      // Read actual current choices rather than silently relying on a built-in chart.
      const accounts=cli("accounts");
      if(!isDeepStrictEqual(accounts,await get("accounts")))throw new Error("Actual asset account choices differ from canonical HTTP");
      const date=new Date().toLocaleDateString("sv-SE",{timeZone:"Europe/Copenhagen"});
      const source=Buffer.from("Synthetic agent equipment acquired for DKK123.45, no VAT; capital contribution\n");
      const company=(await get("company")).company;
      const uploaded=cli("ingest-document",JSON.stringify({confirm:true,fileName:"synthetic-agent-equipment.txt",fileBase64:source.toString("base64"),metadata:{source:"synthetic-native-asset-clients",documentType:"purchase_sale",issueDate:date,invoiceNo:"SYN-AGENT-ASSET-1",deliveryDescription:"Synthetic agent equipment",amountIncVat:123.45,vatAmount:0,currency:"DKK",sender:{name:"Synthetic Agent Equipment Supplier",address:"Synthetic Supplier 8",vatOrCvr:"DK12345678",countryCode:"DK",identifierKind:"dk_cvr"},recipient:{name:company.name,address:company.address,vatOrCvr:company.cvr}}}));
      const documentId=uploaded.document.id;
      const register=JSON.stringify({name:"Synthetic agent equipment",category:"equipment",acquisitionDate:date,cost:123.45,usefulLifeMonths:3,purchaseDocumentId:documentId,assetAccountNo:"1518",depreciationExpenseAccountNo:"3918",accumulatedDepreciationAccountNo:"1598",confirm:true});
      const before=(await get("journal")).journal.entries;
      cli("register-asset",register,[],true);
      if(!isDeepStrictEqual((await get("journal")).journal.entries,before))throw new Error("Uncapitalised asset refusal posted accounting");
      const state=JSON.parse(readFileSync(process.env.RM_NATIVE_WEB_STATE!,"utf8"));
      const capitalise=JSON.parse(state.postedBody);
      capitalise.input.expectedRuleVersion=(await get("journal")).journal.nativePosting.ruleVersion;
      capitalise.input.idempotencyKey="synthetic-agent-asset-capitalisation";
      capitalise.input.request={transactionDate:date,text:"Synthetic agent equipment capital contribution",currency:"DKK",documentId:String(documentId),lines:[{accountNo:"1518",debitAmount:"123.45",creditAmount:"0.00"},{accountNo:"8818",debitAmount:"0.00",creditAmount:"123.45"}]};
      await post("manual-journal",capitalise);
      mcp("register_asset",{body:JSON.stringify({...JSON.parse(register),depreciationExpenseAccountNo:"1518"}),confirm:true},true);
      const baseline=(await get("journal")).journal.entries;
      const assetId=cli("register-asset",register).asset.assetId;
      if(!isDeepStrictEqual((await get("journal")).journal.entries,baseline))throw new Error("Registration unexpectedly capitalised again");
      cli("register-asset",register,[],true);
      const preview=await get(`assets/${assetId}/next-depreciation`);
      if(!isDeepStrictEqual(cli("next-asset-depreciation",undefined,["--id",String(assetId)]),preview)||!isDeepStrictEqual(mcp("next_asset_depreciation",{id:assetId}),preview))throw new Error("Full next-depreciation preview differs across clients");
      const period=preview.nextDepreciation.nextPeriodIndex, amount=preview.nextDepreciation.nextPeriodAmount;
      if(!Number.isSafeInteger(period)||typeof amount!=="number")throw new Error("Server preview missing exact selected period and native amount");
      const body=JSON.stringify({transactionDate:date,periodIndex:period,confirm:true});
      const result=mcp("depreciate_asset",{id:assetId,body,confirm:true}).depreciation;
      const after=(await get("journal")).journal.entries;
      const entry=after.find((e:{id:number})=>e.id===result.entryId);
      if(after.length!==baseline.length+1||!entry||entry.documentId!==documentId||result.periodIndex!==period||result.periodAmount!==amount||entry.lines.length!==2||!entry.lines.some((l:{accountNo:string;debit:number;credit:number})=>l.accountNo==="3918"&&l.debit===amount&&l.credit===0)||!entry.lines.some((l:{accountNo:string;debit:number;credit:number})=>l.accountNo==="1598"&&l.credit===amount&&l.debit===0))throw new Error("Depreciation did not post exactly one source-linked journal on actual reviewed accounts/amount");
      const asset=(await get("assets")).assets.assets.find((a:{assetId:number})=>a.assetId===assetId);
      const next=await get(`assets/${assetId}/next-depreciation`);
      if(asset?.postedPeriods!==1||asset.accumulatedDepreciation!==amount||next.nextDepreciation.postedPeriods!==1)throw new Error("Canonical asset schedule/accounting readback missing");
      evidence={assetId,documentId,date,body,asset,next,journalEntry:entry,source:[...source],amount,period};
      writeFileSync(saved,JSON.stringify(evidence));
    }else evidence=JSON.parse(readFileSync(saved,"utf8"));
    const canonical=await get("assets");
    if(!isDeepStrictEqual(cli("assets"),canonical)||!isDeepStrictEqual(mcp("assets",{}),canonical))throw new Error("Full asset register JSON differs across HTTP/CLI/MCP");
    if(!isDeepStrictEqual(canonical.assets.assets.find((a:{assetId:number})=>a.assetId===evidence.assetId),evidence.asset))throw new Error("Own canonical asset/schedule changed after restart");
    const next=await get(`assets/${evidence.assetId}/next-depreciation`);
    if(!isDeepStrictEqual(next,evidence.next)||!isDeepStrictEqual(cli("next-asset-depreciation",undefined,["--id",String(evidence.assetId)]),next)||!isDeepStrictEqual(mcp("next_asset_depreciation",{id:evidence.assetId}),next))throw new Error("Next preview lost server-established schedule facts");
    const journalBefore=(await get("journal")).journal.entries;
    if(!isDeepStrictEqual(journalBefore.find((e:{id:number})=>e.id===evidence.journalEntry.id),evidence.journalEntry))throw new Error("Source-linked depreciation posting changed");
    cli("depreciate-asset",evidence.body,["--id",String(evidence.assetId)],true);
    mcp("depreciate_asset",{id:evidence.assetId,body:evidence.body,confirm:true},true);
    if(!isDeepStrictEqual((await get("journal")).journal.entries,journalBefore)||!isDeepStrictEqual(await get("assets"),canonical))throw new Error("Explicit duplicate period refusal changed accounting/register");
    for(const[path,command,tool]of[["journal","journal","journal"],["trial-balance","trial-balance","trial_balance"],["income-statement","income-statement","income_statement"],["balance","balance","balance"]]){const http=await get(path);if(!isDeepStrictEqual(cli(command),http)||!isDeepStrictEqual(mcp(tool,{}),http))throw new Error(`Canonical ${path} full JSON differs across agent clients`);if(path==="trial-balance"&&http.trialBalance.balanced!==true)throw new Error("Depreciation trial balance unbalanced");}
    const original=await page.request.get(`${url}/documents/${evidence.documentId}/file`);
    if(!original.ok()||!isDeepStrictEqual([...await original.body()],evidence.source))throw new Error("Own asset purchase original unavailable or changed");
    for(const value of[cli("document-original",undefined,["--id",String(evidence.documentId)]),mcp("document_original",{id:evidence.documentId})])if(value.original.contentType!=="text/plain"||!isDeepStrictEqual(value.original.bytes,evidence.source))throw new Error("Agent asset original differs from canonical source");
    console.log(restarted?"PASS: native asset original/register/next schedule/source-linked depreciation survive restart, full agent financial report parity and duplicate refusal":"PASS: own source/account choices → uncapitalised refusal → actual source-linked capitalisation → CLI register → reviewed next preview → confirmed MCP depreciation exactly one journal → canonical schedule/full reports; wrong account, duplicate allocation/period refused");
  }finally{rmSync(temp,{recursive:true,force:true});}
}
