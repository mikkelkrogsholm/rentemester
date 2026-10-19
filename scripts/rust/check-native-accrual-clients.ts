import type { Page } from "@playwright/test";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { isDeepStrictEqual } from "node:util";
const saved = "/tmp/rentemester-rust-reports/native-accrual-clients.json";
type Evidence = { accrualId:number; documentId:number; body:string; accrual:unknown; schedule:unknown; entries:Array<{id:number}>; source:number[] };

/** Dedicated accrual original, explicit registration/recognition and canonical server schedule/accounting. */
export async function checkNativeAccrualClients(page: Page, base: string, restarted = false) {
  const url = `${base}/api/companies/synthetic`;
  const temp = mkdtempSync("/tmp/rm-native-accrual-clients-");
  try {
    const cookie = join(temp, "session.cookie");
    writeFileSync(cookie, (await page.context().cookies(base)).map(c => `${c.name}=${c.value}`).join("; "), { mode: 0o600, flag: "wx" });
    const common = ["--server", base, "--allow-loopback-http", "--cookie-file", cookie];
    function cli(command: string, body?: string, flags: string[] = [], refused = false) {
      const run = spawnSync(process.env.RM_NATIVE_CLI_BIN ?? resolve("target/debug/rentemester-rust"), ["remote", command, ...common, "--company", "synthetic", ...flags, ...(body === undefined ? [] : ["--confirm"])], { input: body, encoding: "utf8", maxBuffer: 16 * 1024 * 1024 });
      if (run.error) throw new Error("Native accrual CLI unavailable; build rentemester-cli or set RM_NATIVE_CLI_BIN");
      const value = JSON.parse(run.stdout);
      if (refused) { if (run.status !== 1 || value.code !== "server_refused") throw new Error("Expected definite native accrual CLI refusal"); return value; }
      if (run.status !== 0) throw new Error(`Native accrual CLI ${command} failed`);
      return value.data;
    }
    function mcp(name: string, args: Record<string, unknown>, refused = false) {
      const input = [
        { jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-11-25", capabilities: {}, clientInfo: { name: "synthetic-accrual-client-check", version: "1" } } },
        { jsonrpc: "2.0", method: "notifications/initialized" },
        { jsonrpc: "2.0", id: 2, method: "tools/call", params: { name, arguments: { company: "synthetic", ...args } } },
      ].map(v => JSON.stringify(v)).join("\n") + "\n";
      const run = spawnSync(process.env.RM_NATIVE_MCP_BIN ?? resolve("target/debug/rentemester-mcp"), common, { input, encoding: "utf8", maxBuffer: 16 * 1024 * 1024 });
      if (run.error || run.status !== 0) throw new Error("Native accrual MCP unavailable; build rentemester-mcp or set RM_NATIVE_MCP_BIN");
      const call = run.stdout.trim().split("\n").map(line => JSON.parse(line)).find(v => v.id === 2);
      if (refused) { if (call?.result?.isError !== true || call.result.structuredContent.error.code !== "server_refused" || call.result.structuredContent.error.status !== 422) throw new Error("Expected definite native accrual MCP HTTP422 refusal"); return call.result; }
      if (call?.error || call?.result?.isError || !call?.result?.structuredContent?.data) throw new Error(`Native accrual MCP ${name} failed`);
      return call.result.structuredContent.data;
    }
    async function get(path: string) {
      const response = await page.request.get(`${url}/${path}`);
      if (!response.ok()) throw new Error(`Canonical accrual read ${path} failed (${response.status()})`);
      return response.json();
    }
    async function post(path: string, data: unknown) {
      const response = await page.request.post(`${url}/${path}`, { headers: { origin: base, "content-type": "application/json" }, data });
      if (!response.ok()) throw new Error(`Synthetic accrual setup ${path} failed: ${await response.text()}`);
      return response.json();
    }
    let evidence:Evidence;
    if(!restarted){
      for(const[accountNo,name,type,normalBalance]of[["1328","Synthetic agent prepaid services","asset","debit"],["3928","Synthetic agent recognized expense","expense","debit"],["1828","Synthetic agent accrual settlement","asset","debit"]])await post("accounts",{accountNo,name,type,normalBalance});
      if(!isDeepStrictEqual(cli("accounts"),await get("accounts")))throw new Error("Accrual actual account choices differ across canonical reads");
      const date=new Date().toLocaleDateString("sv-SE",{timeZone:"Europe/Copenhagen"});
      const source=Buffer.from("Synthetic agent prepaid service evidence; DKK125.09, no VAT, explicit monthly recognition\n");
      const company=(await get("company")).company;
      const document=cli("ingest-document",JSON.stringify({confirm:true,fileName:"synthetic-agent-accrual.txt",fileBase64:source.toString("base64"),metadata:{source:"synthetic-native-accrual-clients",documentType:"purchase_sale",issueDate:date,invoiceNo:"SYN-AGENT-ACCRUAL-1",deliveryDescription:"Synthetic agent prepaid service",amountIncVat:125.09,vatAmount:0,currency:"DKK",sender:{name:"Synthetic Agent Service Supplier",address:"Synthetic Supplier 9",vatOrCvr:"DK12345678",countryCode:"DK",identifierKind:"dk_cvr"},recipient:{name:company.name,address:company.address,vatOrCvr:company.cvr}}})).document;
      const register={accrualType:"prepaid_expense",description:"Synthetic agent prepaid service",totalAmount:125.09,recognitionPeriods:3,firstRecognitionDate:date,registrationDate:date,periodStepMonths:1,resultAccountNo:"3928",balanceAccountNo:"1328",settlementAccountNo:"1828",documentId:document.id,confirm:true};
      const before=(await get("journal")).journal.entries;
      mcp("register_accrual",{body:JSON.stringify({...register,balanceAccountNo:"3928"}),confirm:true},true);
      if(!isDeepStrictEqual((await get("journal")).journal.entries,before))throw new Error("Wrong accrual account refusal posted journal");
      const created=cli("register-accrual",JSON.stringify(register)).accrual;
      const afterRegister=(await get("journal")).journal.entries;
      const registration=afterRegister.find((e:{id:number})=>e.id===created.entryId);
      if(!registration||registration.documentId!==document.id||afterRegister.length!==before.length+1)throw new Error("Registration did not post exactly one source-linked journal");
      const plan=await get(`accruals/${created.accrualId}/schedule`);
      if(!isDeepStrictEqual(created.schedule,plan.accrualSchedule.schedule)||!isDeepStrictEqual(cli("accrual-schedule",undefined,["--id",String(created.accrualId)]),plan)||!isDeepStrictEqual(mcp("accrual_schedule",{id:created.accrualId}),plan))throw new Error("Full server accrual schedule differs across register/HTTP/CLI/MCP");
      const selected=plan.accrualSchedule.schedule.find((p:{recognized:boolean})=>!p.recognized);
      if(!selected||selected.recognitionDate!==date)throw new Error("Explicit due first period unavailable");
      const request={periodIndex:selected.periodIndex,transactionDate:selected.recognitionDate,confirm:true};
      // Invalid period and a date preceding the actual schedule must refuse without a journal.
      cli("recognize-accrual",JSON.stringify({...request,periodIndex:0}),["--id",String(created.accrualId)],true);
      mcp("recognize_accrual",{id:created.accrualId,body:JSON.stringify({...request,transactionDate:"1999-01-01"}),confirm:true},true);
      if(!isDeepStrictEqual((await get("journal")).journal.entries,afterRegister))throw new Error("Wrong period/date recognition refusal changed journal");
      const body=JSON.stringify(request);
      const result=mcp("recognize_accrual",{id:created.accrualId,body,confirm:true}).recognition;
      const after=(await get("journal")).journal.entries;
      const recognized=after.find((e:{id:number})=>e.id===result.entryId);
      if(!recognized||recognized.documentId!==document.id||after.length!==afterRegister.length+1||result.periodIndex!==selected.periodIndex||result.periodAmount!==selected.amount||recognized.lines.length!==2||!recognized.lines.some((l:{accountNo:string;debit:number;credit:number})=>l.accountNo==="3928"&&l.debit===selected.amount&&l.credit===0)||!recognized.lines.some((l:{accountNo:string;debit:number;credit:number})=>l.accountNo==="1328"&&l.credit===selected.amount&&l.debit===0))throw new Error("Recognition did not post exactly one reviewed source/period/amount journal on current accounts");
      const accrual=(await get("accruals")).accruals.report.accruals.find((a:{accrualId:number})=>a.accrualId===created.accrualId);
      const schedule=await get(`accruals/${created.accrualId}/schedule`);
      if(accrual?.recognizedPeriods!==1||accrual.recognizedAmount!==selected.amount||accrual.remainingAmount<=0||accrual.fullyRecognized!==false||schedule.accrualSchedule.schedule.filter((p:{recognized:boolean})=>p.recognized).length!==1)throw new Error("Canonical recognized/remaining/schedule facts missing");
      evidence={accrualId:created.accrualId,documentId:document.id,body,accrual,schedule,entries:[registration,recognized],source:[...source]};
      writeFileSync(saved,JSON.stringify(evidence));
    }else evidence=JSON.parse(readFileSync(saved,"utf8"));
    const canonical=await get("accruals");
    if(!isDeepStrictEqual(cli("accruals"),canonical)||!isDeepStrictEqual(mcp("accruals",{}),canonical))throw new Error("Full HTTP/CLI/MCP accrual JSON differs");
    if(!isDeepStrictEqual(canonical.accruals.report.accruals.find((a:{accrualId:number})=>a.accrualId===evidence.accrualId),evidence.accrual))throw new Error("Own canonical recognized/remaining facts changed after restart");
    const schedule=await get(`accruals/${evidence.accrualId}/schedule`);
    if(!isDeepStrictEqual(schedule,evidence.schedule)||!isDeepStrictEqual(cli("accrual-schedule",undefined,["--id",String(evidence.accrualId)]),schedule)||!isDeepStrictEqual(mcp("accrual_schedule",{id:evidence.accrualId}),schedule))throw new Error("Canonical schedule did not persist unchanged");
    const before=(await get("journal")).journal.entries;
    for(const entry of evidence.entries)if(!isDeepStrictEqual(before.find((e:{id:number})=>e.id===entry.id),entry))throw new Error("Registration/recognition journal facts changed");
    cli("recognize-accrual",evidence.body,["--id",String(evidence.accrualId)],true);
    mcp("recognize_accrual",{id:evidence.accrualId,body:evidence.body,confirm:true},true);
    if(!isDeepStrictEqual((await get("journal")).journal.entries,before)||!isDeepStrictEqual(await get("accruals"),canonical)||!isDeepStrictEqual(await get(`accruals/${evidence.accrualId}/schedule`),schedule))throw new Error("Duplicate recognition changed ledger or canonical remaining/schedule");
    for(const[path,command,tool]of[["journal","journal","journal"],["trial-balance","trial-balance","trial_balance"],["income-statement","income-statement","income_statement"],["balance","balance","balance"]]){const http=await get(path);if(!isDeepStrictEqual(cli(command),http)||!isDeepStrictEqual(mcp(tool,{}),http))throw new Error(`Accrual full ${path} JSON differs across clients`);if(path==="trial-balance"&&http.trialBalance.balanced!==true)throw new Error("Accrual trial balance unbalanced");}
    const original=await page.request.get(`${url}/documents/${evidence.documentId}/file`);
    if(!original.ok()||!isDeepStrictEqual([...await original.body()],evidence.source))throw new Error("Accrual source original changed or unavailable");
    for(const read of[cli("document-original",undefined,["--id",String(evidence.documentId)]),mcp("document_original",{id:evidence.documentId})])if(read.original.contentType!=="text/plain"||!isDeepStrictEqual(read.original.bytes,evidence.source))throw new Error("Agent accrual source bytes differ from actual HTTP original");
    console.log(restarted?"PASS: agent accrual register/schedule/remaining/source and accounting facts survive restart; duplicate recognition refuses without journal":"PASS: own source/accounts → CLI registration exactly one journal → full server schedule parity → explicit MCP recognition exactly one journal → canonical remaining/report/source parity; wrong account/period/date and duplicate recognition refused");
  }finally{rmSync(temp,{recursive:true,force:true});}
}
