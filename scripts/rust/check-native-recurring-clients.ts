import type { Page } from "@playwright/test";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { isDeepStrictEqual } from "node:util";
const saved = "/tmp/rentemester-rust-reports/native-recurring-clients.json";
type Evidence = { templateId:number; documentId:number; date:string; template:unknown; invoice:unknown; pdf:number[] };

/** Dedicated explicit manual invoice generation, canonical original/PDF and retirement. */
export async function checkNativeRecurringClients(page: Page, base: string, restarted = false) {
  const url = `${base}/api/companies/synthetic`;
  const temp = mkdtempSync("/tmp/rm-native-recurring-clients-");
  try {
    const cookie = join(temp, "session.cookie");
    writeFileSync(cookie, (await page.context().cookies(base)).map(c => `${c.name}=${c.value}`).join("; "), { mode: 0o600, flag: "wx" });
    const common = ["--server", base, "--allow-loopback-http", "--cookie-file", cookie];
    function cli(command: string, body?: string, flags: string[] = [], refused = false) {
      const run = spawnSync(process.env.RM_NATIVE_CLI_BIN ?? resolve("target/debug/rentemester-rust"), ["remote", command, ...common, "--company", "synthetic", ...flags, ...(body === undefined ? [] : ["--confirm"])], { input: body, encoding: "utf8", maxBuffer: 16 * 1024 * 1024 });
      if (run.error) throw new Error("Native recurring CLI unavailable; build rentemester-cli or set RM_NATIVE_CLI_BIN");
      const value = JSON.parse(run.stdout);
      if (refused) { if (run.status !== 1 || value.code !== "server_refused") throw new Error("Expected definite native recurring CLI refusal"); return value; }
      if (run.status !== 0) throw new Error(`Native recurring CLI ${command} failed`);
      return value.data;
    }
    function mcp(name: string, args: Record<string, unknown>, refused = false) {
      const input = [
        { jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-11-25", capabilities: {}, clientInfo: { name: "synthetic-recurring-client-check", version: "1" } } },
        { jsonrpc: "2.0", method: "notifications/initialized" },
        { jsonrpc: "2.0", id: 2, method: "tools/call", params: { name, arguments: { company: "synthetic", ...args } } },
      ].map(v => JSON.stringify(v)).join("\n") + "\n";
      const run = spawnSync(process.env.RM_NATIVE_MCP_BIN ?? resolve("target/debug/rentemester-mcp"), common, { input, encoding: "utf8", maxBuffer: 16 * 1024 * 1024 });
      if (run.error || run.status !== 0) throw new Error("Native recurring MCP unavailable; build rentemester-mcp or set RM_NATIVE_MCP_BIN");
      const call = run.stdout.trim().split("\n").map(line => JSON.parse(line)).find(v => v.id === 2);
      if (refused) { if (call?.result?.isError !== true || call.result.structuredContent.error.code !== "server_refused" || call.result.structuredContent.error.status !== 422) throw new Error("Expected definite native recurring MCP HTTP422 refusal"); return call.result; }
      if (call?.error || call?.result?.isError || !call?.result?.structuredContent?.data) throw new Error(`Native recurring MCP ${name} failed`);
      return call.result.structuredContent.data;
    }
    async function get(path: string) {
      const response = await page.request.get(`${url}/${path}`);
      if (!response.ok()) throw new Error(`Canonical recurring read ${path} failed (${response.status()})`);
      return response.json();
    }
    async function post(path: string, data: unknown) {
      const response = await page.request.post(`${url}/${path}`, { headers: { origin: base, "content-type": "application/json" }, data });
      if (!response.ok()) throw new Error(`Synthetic recurring setup ${path} failed: ${await response.text()}`);
      return response.json();
    }
    let evidence: Evidence;
    const journalBefore=(await get("journal")).journal.entries;
    if(!restarted){
      cli("create-customer",JSON.stringify({name:"Synthetic agent recurring buyer",address:"Synthetic Agent Street 8",vatOrCvr:"DK12345678"}));
      const contacts=await get("contacts");
      const customer=contacts.contacts.customers.find((c:{name:string})=>c.name==="Synthetic agent recurring buyer");
      if(!customer)throw new Error("Own recurring customer not found");
      const date=new Date().toLocaleDateString("sv-SE",{timeZone:"Europe/Copenhagen"});
      const body=JSON.stringify({name:"Synthetic agent manual recurring",interval:"monthly",intervalCount:1,deliveryChannel:"manual",firstIssueDate:date,paymentTermsDays:14,deliveryPeriodMode:"issue_month",vatRatePercent:25,currency:"DKK",customerId:customer.id,lines:[{description:"Synthetic agent recurring service",quantity:2,unitPriceExVat:17.19}],confirm:true});
      const templateId=cli("create-recurring-invoice",body).template.templateId;
      const canonical=await get("recurring-invoices");
      if(!isDeepStrictEqual(cli("recurring-invoices"),canonical)||!isDeepStrictEqual(mcp("recurring_invoices",{}),canonical))throw new Error("Full recurring canonical lists differ before generation");
      const request=JSON.stringify({asOfDate:date,confirm:true});
      const generation=cli("generate-recurring-invoice",request,["--id",String(templateId)]).generation;
      if(generation.created!==true||generation.templateId!==templateId||generation.periodIndex!==0)throw new Error("Explicit CLI generation did not issue expected first period");
      // Read canonical evidence before an explicit same-period request; no adapter performs a retry.
      const after=await get("recurring-invoices");
      const own=after.recurringInvoices.templates.find((t:{id:number})=>t.id===templateId);
      if(own?.generations.length!==1||own.generations[0].documentId!==generation.documentId)throw new Error("Canonical generation evidence missing");
      const repeat=mcp("generate_recurring_invoice",{id:templateId,body:request,confirm:true}).generation;
      const expected={...generation,created:false};
      if(!isDeepStrictEqual(repeat,expected))throw new Error("Supported explicit same-period generation did not return same invoice/full facts");
      const invoice=(await get("invoices")).invoices.invoices.find((i:{documentId:number})=>i.documentId===generation.documentId);
      if(invoice?.posted!==false||invoice.customerName!==customer.name)throw new Error("Generation was hidden-posted or used wrong customer");
      const response=await page.request.get(`${url}/invoices/${generation.documentId}/pdf`);
      if(!response.ok())throw new Error("Issued recurring PDF unavailable");
      const pdf=[...await response.body()];
      mcp("retire_recurring_invoice",{id:templateId,body:JSON.stringify({reason:"Synthetic explicit lifecycle complete",confirm:true}),confirm:true});
      const retired=(await get("recurring-invoices")).recurringInvoices.templates.find((t:{id:number})=>t.id===templateId);
      if(retired?.active!==false||retired.generations.length!==1)throw new Error("Retirement changed or lost generation");
      evidence={templateId,documentId:generation.documentId,date,template:retired,invoice,pdf};
      writeFileSync(saved,JSON.stringify(evidence));
    }else evidence=JSON.parse(readFileSync(saved,"utf8"));
    const canonical=await get("recurring-invoices");
    if(!isDeepStrictEqual(cli("recurring-invoices"),canonical)||!isDeepStrictEqual(mcp("recurring_invoices",{}),canonical))throw new Error("Full native recurring HTTP/CLI/MCP read differs");
    const own=canonical.recurringInvoices.templates.find((t:{id:number})=>t.id===evidence.templateId);
    if(!isDeepStrictEqual(own,evidence.template))throw new Error("Own retired template/generation facts changed across restart");
    const invoice=(await get("invoices")).invoices.invoices.find((i:{documentId:number})=>i.documentId===evidence.documentId);
    if(!isDeepStrictEqual(invoice,evidence.invoice))throw new Error("Own issued unposted invoice facts changed");
    const original=await page.request.get(`${url}/invoices/${evidence.documentId}/pdf`);
    if(!original.ok())throw new Error("Canonical recurring original unavailable");
    const bytes=[...await original.body()];
    if(!isDeepStrictEqual(bytes,evidence.pdf))throw new Error("Immutable issued PDF changed after restart");
    for(const read of [cli("invoice-pdf",undefined,["--id",String(evidence.documentId)]),mcp("invoice_pdf",{id:evidence.documentId})])if(read.pdf.contentType!=="application/pdf"||!isDeepStrictEqual(read.pdf.bytes,bytes))throw new Error("CLI/MCP PDF differs from actual authorized original");
    const body=JSON.stringify({asOfDate:evidence.date,confirm:true});
    cli("generate-recurring-invoice",body,["--id",String(evidence.templateId)],true);
    mcp("generate_recurring_invoice",{id:evidence.templateId,body,confirm:true},true);
    if(!isDeepStrictEqual((await get("recurring-invoices")),canonical))throw new Error("Retired generation refusal changed evidence");
    if(!isDeepStrictEqual((await get("journal")).journal.entries,journalBefore))throw new Error("Manual recurring lifecycle produced journal effects");
    console.log(restarted?"PASS: native agent recurring retired template/single generation/PDF survive restart, retired same-period calls refuse, no journal effects":"PASS: distinct customer → CLI template → full canonical parity → explicit CLI generation → supported same-period MCP dedup → exact PDF → MCP retire → retired refusal; no hidden posting");
  }finally{rmSync(temp,{recursive:true,force:true});}
}
