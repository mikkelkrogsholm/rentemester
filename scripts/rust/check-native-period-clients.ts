// Build host CLI/MCP binaries first (or RM_NATIVE_CLI_BIN/RM_NATIVE_MCP_BIN).
// The helper uses the real native server and leaves shared build caches alone.

import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import type { Page } from "@playwright/test";

const saved = "/tmp/rentemester-rust-reports/native-period-clients.json";
export async function checkNativePeriodClients(
	page: Page,
	base: string,
	restarted = false,
) {
	const temp = mkdtempSync("/tmp/rm-native-period-clients-"),
		cookieFile = join(temp, "session.cookie");
	const cookies = (await page.context().cookies(base))
		.map((c) => `${c.name}=${c.value}`)
		.join("; ");
	writeFileSync(cookieFile, cookies, { mode: 0o600, flag: "wx" });
	const common = [
			"--server",
			base,
			"--allow-loopback-http",
			"--cookie-file",
			cookieFile,
		],
		cli =
			process.env.RM_NATIVE_CLI_BIN ?? resolve("target/debug/rentemester-rust"),
		mcp =
			process.env.RM_NATIVE_MCP_BIN ?? resolve("target/debug/rentemester-mcp");
	function cliCall(
		command: string,
		body?: string,
		flags: string[] = [],
		allowFailure = false,
	) {
		const child = spawnSync(
			cli,
			["remote", command, ...common, "--company", "synthetic", ...flags],
			{ input: body, encoding: "utf8", maxBuffer: 16 * 1024 * 1024 },
		);
		if (child.error)
			throw new Error(
				"Host Rust CLI unavailable; build rentemester-cli or configure RM_NATIVE_CLI_BIN",
			);
		if (!allowFailure && child.status !== 0)
			throw new Error(`CLI ${command} failed: ${child.stdout.slice(0, 1000)}`);
		const value = JSON.parse(child.stdout);
		return { status: child.status, value };
	}
	function rpc(name: string, args: Record<string, unknown>, allowFailure = false) {
		const input =
			[
				{
					jsonrpc: "2.0",
					id: 1,
					method: "initialize",
					params: {
						protocolVersion: "2025-11-25",
						capabilities: {},
						clientInfo: { name: "synthetic-native-period-check", version: "1" },
					},
				},
				{ jsonrpc: "2.0", method: "notifications/initialized" },
				{
					jsonrpc: "2.0",
					id: 2,
					method: name === "tools/list" ? name : "tools/call",
					params: name === "tools/list" ? {} : { name, arguments: args },
				},
			]
				.map((v) => JSON.stringify(v))
				.join("\n") + "\n";
		const child = spawnSync(mcp, common, {
			input,
			encoding: "utf8",
			maxBuffer: 16 * 1024 * 1024,
		});
		if (child.error)
			throw new Error(
				"Host Rust MCP unavailable; build rentemester-mcp or configure RM_NATIVE_MCP_BIN",
			);
		if (child.status !== 0) throw new Error("MCP subprocess failed");
		const replies = child.stdout
			.trim()
			.split("\n")
			.map((line) => JSON.parse(line));
		const result = replies.find((r) => r.id === 2);
		if (!allowFailure && (result?.error || result?.result?.isError))
			throw new Error(
				`MCP ${name} failed: ${JSON.stringify(result).slice(0, 1000)}`,
			);
		if (allowFailure) return result;
		return name === "tools/list"
			? result.result
			: result.result.structuredContent.data;
	}

 let evidence: { start:string;end:string;periodId:number;hash:string;reference:string;year:string };
 try {
 const periodRows=()=>cliCall("periods").value.data.periods.periods as Array<{id:number;periodStart:string;effectiveStatus:string;closePacketHash:string;reference:string}>;
 if(!restarted){
  // A historical quarter before the normal smoke corpus; do not lock today's work.
  const today=new Date().toLocaleDateString("sv-SE",{timeZone:"Europe/Copenhagen"});
  const year=Number(today.slice(0,4))-2;
  evidence={start:`${year}-01-01`,end:`${year}-03-31`,periodId:0,hash:"",reference:"SYNTHETIC-CLIENT-PERIOD-RECEIPT",year:String(year)};
  if(periodRows().some(p=>p.periodStart===evidence.start))throw new Error("Synthetic historical period already exists; reconcile instead of resubmitting");
  const chart=cliCall("accounts").value.data.accounts;
  // Preserve every confirmed role; create only absent control accounts.
  await page.goto(`${base}/companies/synthetic/kontoplan`);
  const accounts=[ ["8711","Synthetic period equity debit","equity","credit"], ["8712","Synthetic period equity credit","equity","credit"] ];
  const missing:Array<[string,string]>=[];
  for(const [role,no,type,normal] of [["bank","1712","asset","debit"],["debtors","1713","asset","debit"],["creditors","7012","liability","credit"]]){
   if(chart.accountRoles.ambiguous.includes(role))throw new Error(`Ambiguous ${role}; manual reconciliation required`);
   if(!chart.accountRoles.resolutions.some((r:{role:string;ok:boolean})=>r.role===role&&r.ok)){accounts.push([no,`Synthetic period ${role}`,type,normal]);missing.push([role,no]);}
  }
  for(const [no,name,type,normal] of accounts){
   const form=page.getByRole("form",{name:"Opret konto",exact:true});
   await form.getByLabel("Kontonummer",{exact:true}).fill(no);await form.getByLabel("Kontonavn",{exact:true}).fill(name);await form.getByLabel(/^Kontotype/).selectOption(type);await form.getByLabel(/^Normal saldo/).selectOption(normal);await form.getByRole("button",{name:"Opret konto",exact:true}).click();await page.getByRole("row").filter({hasText:name}).waitFor();
  }
  for(const[role,no]of missing){const form=page.getByRole("form",{name:"Bekræft kontorolle",exact:true});await form.getByLabel(/^Kontorolle/).selectOption(role);await form.getByLabel(/^Konto til rollen/).selectOption(no);await form.getByRole("button",{name:"Bekræft kontorolle",exact:true}).click();await page.getByRole("row").filter({hasText:new RegExp(`Konto ${no}`)}).waitFor();}
  const bounds={periodStart:evidence.start,periodEnd:evidence.end,confirm:true};
  const readiness=()=>cliCall("period-close-readiness",undefined,["--from",evidence.start,"--to",evidence.end]).value.data.packet;
  const ready=readiness();if(ready.blockers!==0)throw new Error(`Historical close controls blocked: ${JSON.stringify(ready)}`);
  const first=rpc("review_period",{company:"synthetic",body:JSON.stringify(bounds),confirm:true}).review;
  const state=JSON.parse(readFileSync(process.env.RM_NATIVE_WEB_STATE!,"utf8"));
  const posting=JSON.parse(state.postedBody);
  posting.input.request={transactionDate:evidence.end,text:"Synthetic client period proof change",currency:"DKK",lines:[{accountNo:"8711",debitAmount:"3.25",creditAmount:"0.00"},{accountNo:"8712",debitAmount:"0.00",creditAmount:"3.25"}]};
  posting.input.idempotencyKey="synthetic-period-client-source-change";
  cliCall("post-journal",JSON.stringify(posting));
  const changed=readiness();
  if(typeof changed.hash!=="string"||changed.hash===first.packet.hash)throw new Error("Source posting did not change reviewed readiness proof");
  const periodsBeforeStale=JSON.stringify(periodRows());
  const stale=cliCall("close-period",JSON.stringify({...bounds,reviewId:first.id,packetHash:first.packet.hash}),["--confirm"],true);
  // The public client intentionally redacts native refusal bodies; verify its contract.
  if(stale.status!==1||stale.value.ok!==false||stale.value.code!=="server_refused"||!Array.isArray(stale.value.errors)||!stale.value.errors.includes("server refused request (HTTP 422)"))throw new Error(`Stale close was not explicitly refused: ${JSON.stringify(stale)}`);
  const periodsAfterStale=periodRows();
  if(JSON.stringify(periodsAfterStale)!==periodsBeforeStale||periodsAfterStale.some(p=>p.periodStart===evidence.start))throw new Error("Stale close changed the period register");
  const review=()=>cliCall("review-period",JSON.stringify(bounds),["--confirm"]).value.data.review;
  const close=(r: {id:number;packet:{hash:string}})=>rpc("close_period",{company:"synthetic",body:JSON.stringify({...bounds,reviewId:r.id,packetHash:r.packet.hash}),confirm:true}).period;
  const closed=close(review());evidence.periodId=closed.id;
  posting.input.idempotencyKey="synthetic-period-client-lock-refusal";
  const locked=cliCall("post-journal",JSON.stringify(posting),[],true);if(locked.status!==1||!JSON.stringify(locked.value).includes("closed"))throw new Error("Closed quarter allowed posting");
  const opened=cliCall("reopen-period",JSON.stringify({...bounds,reason:"Synthetic additional evidence reviewed before filing"}),["--confirm"]).value.data.period;
  if(opened.effectiveStatus!=="open")throw new Error("Justified reopen failed");
  if(close(review()).id!==closed.id)throw new Error("Reclose changed original period identity");
  evidence.hash=periodRows().find(p=>p.id===evidence.periodId)!.closePacketHash;
  cliCall("report-period",JSON.stringify({...bounds,packetHash:evidence.hash,reference:evidence.reference}),["--confirm"]);
  const refused=rpc("reopen_period",{company:"synthetic",body:JSON.stringify({...bounds,reason:"Synthetic reported lock check"}),confirm:true},true);
  if(!refused.result?.isError)throw new Error("Reported period allowed reopening");
  const conflict=cliCall("report-period",JSON.stringify({...bounds,packetHash:evidence.hash,reference:"CONFLICTING-SYNTHETIC-RECEIPT"}),["--confirm"],true);if(conflict.status!==1)throw new Error("Reported receipt was mutable");
  writeFileSync(saved,JSON.stringify(evidence));
 }else evidence=JSON.parse(readFileSync(saved,"utf8"));
 const period=periodRows().find(p=>p.id===evidence.periodId);
 if(period?.effectiveStatus!=="reported"||period.closePacketHash!==evidence.hash||period.reference!==evidence.reference)throw new Error("Original close proof/receipt did not persist");
 const vat=rpc("vat",{company:"synthetic",year:evidence.year});if(!vat.vat)throw new Error("Native VAT read unavailable");
 await page.goto(`${base}/companies/synthetic/periodelas`);await page.getByRole("row").filter({hasText:evidence.start}).getByText(evidence.reference,{exact:true}).waitFor();
 console.log(restarted?"PASS: client period lock, original proof and receipt survive restart":"PASS: CLI/MCP VAT/readiness → review → source change/stale refusal → confirmed close/locked posting → justified reopen → same-period reclose → immutable external receipt");
 }finally{rmSync(temp,{recursive:true,force:true});}
}
