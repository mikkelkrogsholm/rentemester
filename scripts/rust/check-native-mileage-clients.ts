import type { Page } from "@playwright/test";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { isDeepStrictEqual } from "node:util";
const saved = "/tmp/rentemester-rust-reports/native-mileage-clients.json";
type Evidence = {year:string; entries:Array<{id:number; amountBasis:number}>; receipts:unknown[]};

/** Own mileage trips; native amount/advisories and full reports, no accounting or tax inference. */
export async function checkNativeMileageClients(page: Page, base: string, restarted = false) {
  const url = `${base}/api/companies/synthetic`;
  const temp = mkdtempSync("/tmp/rm-native-mileage-clients-");
  try {
    const cookie = join(temp, "session.cookie");
    writeFileSync(cookie, (await page.context().cookies(base)).map(c => `${c.name}=${c.value}`).join("; "), { mode: 0o600, flag: "wx" });
    const common = ["--server", base, "--allow-loopback-http", "--cookie-file", cookie];
    function cli(command: string, body?: string, flags: string[] = [], refused = false) {
      const run = spawnSync(process.env.RM_NATIVE_CLI_BIN ?? resolve("target/debug/rentemester-rust"), ["remote", command, ...common, "--company", "synthetic", ...flags, ...(body === undefined ? [] : ["--confirm"])], { input: body, encoding: "utf8", maxBuffer: 16 * 1024 * 1024 });
      if (run.error) throw new Error("Native mileage CLI unavailable; build rentemester-cli or set RM_NATIVE_CLI_BIN");
      const value = JSON.parse(run.stdout);
      if (refused) { if (run.status !== 1 || value.code !== "server_refused") throw new Error("Expected definite native mileage CLI refusal"); return value; }
      if (run.status !== 0) throw new Error(`Native mileage CLI ${command} failed`);
      return value.data;
    }
    function mcp(name: string, args: Record<string, unknown>, refused = false) {
      const input = [
        { jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-11-25", capabilities: {}, clientInfo: { name: "synthetic-mileage-client-check", version: "1" } } },
        { jsonrpc: "2.0", method: "notifications/initialized" },
        { jsonrpc: "2.0", id: 2, method: "tools/call", params: { name, arguments: { company: "synthetic", ...args } } },
      ].map(v => JSON.stringify(v)).join("\n") + "\n";
      const run = spawnSync(process.env.RM_NATIVE_MCP_BIN ?? resolve("target/debug/rentemester-mcp"), common, { input, encoding: "utf8", maxBuffer: 16 * 1024 * 1024 });
      if (run.error || run.status !== 0) throw new Error("Native mileage MCP unavailable; build rentemester-mcp or set RM_NATIVE_MCP_BIN");
      const call = run.stdout.trim().split("\n").map(line => JSON.parse(line)).find(v => v.id === 2);
      if (refused) { if (call?.result?.isError !== true || call.result.structuredContent.error.code !== "server_refused" || call.result.structuredContent.error.status !== 422) throw new Error("Expected definite native mileage MCP HTTP422 refusal"); return call.result; }
      if (call?.error || call?.result?.isError || !call?.result?.structuredContent?.data) throw new Error(`Native mileage MCP ${name} failed`);
      return call.result.structuredContent.data;
    }
    async function get(path: string) {
      const response = await page.request.get(`${url}/${path}`);
      if (!response.ok()) throw new Error(`Canonical mileage read ${path} failed (${response.status()})`);
      return response.json();
    }
    async function post(path: string, data: unknown) {
      const response = await page.request.post(`${url}/${path}`, { headers: { origin: base, "content-type": "application/json" }, data });
      if (!response.ok()) throw new Error(`Synthetic mileage setup ${path} failed: ${await response.text()}`);
      return response.json();
    }
    let evidence:Evidence;
    const beforeJournal=(await get("journal")).journal.entries;
    if(!restarted){
      const year=(await get("mileage")).mileage.selectedYear;
      const date=new Date().toLocaleDateString("sv-SE",{timeZone:"Europe/Copenhagen"});
      const common={tripDate:date,fromLocation:"Synthetic Agent Origin",toLocation:"Synthetic Agent Destination",vehicle:"Synthetic agent private vehicle",driver:"Synthetic agent driver",rateBasis:"Explicit synthetic user-chosen basis; not a claim of tax eligibility",rateSource:"https://example.invalid/synthetic-agent-rate",notes:"Synthetic documentation only",confirm:true};
      const first=cli("register-mileage",JSON.stringify({...common,purpose:"Synthetic agent mileage precision visit",kilometers:12.345,ratePerKm:3.791})).mileage;
      const second=mcp("register_mileage",{body:JSON.stringify({...common,purpose:"Synthetic agent mileage advisory visit",kilometers:0.15,ratePerKm:9}),confirm:true}).mileage;
      const canonical=(await get(`mileage?year=${encodeURIComponent(year)}`)).mileage;
      const entries=canonical.entries.filter((e:{id:number})=>[first.mileageEntryId,second.mileageEntryId].includes(e.id));
      if(entries.length!==2||entries.find((e:{id:number})=>e.id===first.mileageEntryId)?.amountBasis!==first.amountBasis||entries.find((e:{id:number})=>e.id===second.mileageEntryId)?.amountBasis!==second.amountBasis||!Array.isArray(second.warnings)||second.warnings.length===0||!Array.isArray(second.appliedRules)||second.appliedRules.length===0)throw new Error("Mileage server receipt amount/advisory/rules missing from actual entries");
      // Invalid values remain server refusals; adapters never invent a replacement rate or route.
      cli("register-mileage",JSON.stringify({...common,purpose:"Synthetic refused mileage",kilometers:0,ratePerKm:3.791}),[],true);
      mcp("register_mileage",{body:JSON.stringify({...common,purpose:"Synthetic refused rate",kilometers:1,ratePerKm:0}),confirm:true},true);
      evidence={year,entries,receipts:[first,second]};writeFileSync(saved,JSON.stringify(evidence));
    }else evidence=JSON.parse(readFileSync(saved,"utf8"));
    const http=await get(`mileage?year=${encodeURIComponent(evidence.year)}`);
    if(!isDeepStrictEqual(cli("mileage",undefined,["--year",evidence.year]),http)||!isDeepStrictEqual(mcp("mileage",{year:evidence.year}),http))throw new Error("Full mileage fiscal/month totals, chosen basis and entry facts differ across clients");
    for(const entry of evidence.entries)if(!isDeepStrictEqual(http.mileage.entries.find((e:{id:number})=>e.id===entry.id),entry))throw new Error("Own mileage amount/rate/basis/number/timestamp changed after restart");
    if(!isDeepStrictEqual((await get("journal")).journal.entries,beforeJournal))throw new Error("Mileage registration/read/refusal produced journal effects");
    console.log(restarted?"PASS: own mileage original entries and full native fiscal/month reports agree across HTTP/CLI/MCP after restart, journal unchanged":"PASS: CLI precision trip and MCP high-rate advisory trip preserve server amounts/rules/chosen basis; canonical fiscal/month report parity, invalid km/rate refused, no ledger posting");
  }finally{rmSync(temp,{recursive:true,force:true});}
}
