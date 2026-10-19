import { chromium, type Page } from "@playwright/test";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync, readdirSync, lstatSync } from "node:fs";
import { join, resolve, basename } from "node:path";
import { isDeepStrictEqual } from "node:util";

type Evidence = {profile:unknown; chart:unknown; ledgerUuid:string; journal:unknown[]};

/** Mandatory standalone owned fixture; current-company role changes are never composed with other journeys. */
export async function checkNativeCompanySetupClients(page: Page, base: string, options:{isolated:true; company:string; evidencePath:string}, restarted = false) {
  if(options?.isolated!==true||!options.company)throw new Error("Explicit isolated setup fixture required");
  const saved=options.evidencePath;
  const url = `${base}/api/companies/${encodeURIComponent(options.company)}`;
  const temp = mkdtempSync("/tmp/rm-native-company-setup-clients-");
  try {
    const cookie = join(temp, "session.cookie");
    writeFileSync(cookie, (await page.context().cookies(base)).map(c => `${c.name}=${c.value}`).join("; "), { mode: 0o600, flag: "wx" });
    const common = ["--server", base, "--allow-loopback-http", "--cookie-file", cookie];
    function cli(command: string, body?: string, flags: string[] = [], refused = false) {
      const run = spawnSync(process.env.RM_NATIVE_CLI_BIN ?? resolve("target/debug/rentemester-rust"), ["remote", command, ...common, "--company", options.company, ...flags, ...(body === undefined ? [] : ["--confirm"])], { input: body, encoding: "utf8", maxBuffer: 16 * 1024 * 1024 });
      if (run.error) throw new Error("Native setup CLI unavailable; build rentemester-cli or set RM_NATIVE_CLI_BIN");
      const value = JSON.parse(run.stdout);
      if (refused) { if (run.status !== 1 || value.code !== "server_refused") throw new Error("Expected definite native setup CLI refusal"); return value; }
      if (run.status !== 0) throw new Error(`Native setup CLI ${command} failed`);
      return value.data;
    }
    function mcp(name: string, args: Record<string, unknown>, refused = false) {
      const input = [
        { jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-11-25", capabilities: {}, clientInfo: { name: "synthetic-accrual-client-check", version: "1" } } },
        { jsonrpc: "2.0", method: "notifications/initialized" },
        { jsonrpc: "2.0", id: 2, method: "tools/call", params: { name, arguments: { company: options.company, ...args } } },
      ].map(v => JSON.stringify(v)).join("\n") + "\n";
      const run = spawnSync(process.env.RM_NATIVE_MCP_BIN ?? resolve("target/debug/rentemester-mcp"), common, { input, encoding: "utf8", maxBuffer: 16 * 1024 * 1024 });
      if (run.error || run.status !== 0) throw new Error("Native setup MCP unavailable; build rentemester-mcp or set RM_NATIVE_MCP_BIN");
      const call = run.stdout.trim().split("\n").map(line => JSON.parse(line)).find(v => v.id === 2);
      if (refused) { if (call?.result?.isError !== true || call.result.structuredContent.error.code !== "server_refused" || call.result.structuredContent.error.status !== 422) throw new Error("Expected definite native setup MCP HTTP422 refusal"); return call.result; }
      if (call?.error || call?.result?.isError || !call?.result?.structuredContent?.data) throw new Error(`Native setup MCP ${name} failed`);
      return call.result.structuredContent.data;
    }
    async function get(path: string) {
      const response = await page.request.get(`${url}/${path}`);
      if (!response.ok()) throw new Error(`Canonical setup read ${path} failed (${response.status()})`);
      return response.json();
    }
    async function post(path: string, data: unknown) {
      const response = await page.request.post(`${url}/${path}`, { headers: { origin: base, "content-type": "application/json" }, data });
      if (!response.ok()) throw new Error(`Synthetic accrual setup ${path} failed: ${await response.text()}`);
      return response.json();
    }
    const journal=(await get("journal")).journal;
    if(journal.entries.length!==0)throw new Error("Company setup acceptance requires its fresh empty ledger");
    let evidence:Evidence;
    if(!restarted){
      const updated=cli("update-company-profile",JSON.stringify({cvr:"87654321",address:"Synthetic Setup Street 9",postalCode:"1000",city:"Synthetic Setup City",vatPeriodType:"quarter",paymentTermsDays:14}));
      if(updated.company.cvr!=="DK87654321"||updated.company.address!=="Synthetic Setup Street 9"||updated.company.vatPeriodType!=="quarter")throw new Error("Explicit profile normalization/readback missing");
      for(const[accountNo,name,type,normalBalance]of[["1838","Synthetic setup chosen bank","asset","debit"],["4718","Synthetic setup chosen input VAT","vat","debit"],["4728","Synthetic setup chosen output VAT","vat","credit"],["3938","Synthetic setup chosen expense","expense","debit"]])cli("create-account",JSON.stringify({accountNo,name,type,normalBalance}));
      cli("create-account",JSON.stringify({accountNo:"4718",name:"Synthetic overwrite attempt",type:"vat",normalBalance:"debit"}),[],true);
      // Actual server validation: wrong type, wrong normal balance and nonexistent account.
      for(const accountNo of["1838","4728","synthetic-nonexistent-account"])mcp("confirm_account_role",{body:JSON.stringify({role:"input_vat",accountNo,confirm:true}),confirm:true},true);
      // Unknown role must reach the actual native boundary, whose compatibility authority is tested.
      const unknown=await page.request.post(`${url}/account-roles`,{headers:{origin:base,"content-type":"application/json"},data:{role:"synthetic-unknown-role",accountNo:"4718",confirm:true}});
      if(unknown.status()!==422)throw new Error("Unknown native role accepted");
      const bank=cli("confirm-account-role",JSON.stringify({role:"bank",accountNo:"1838",confirm:true})).resolution;
      const vat=mcp("confirm_account_role",{body:JSON.stringify({role:"input_vat",accountNo:"4718",confirm:true}),confirm:true}).resolution;
      if(bank.ok!==true||bank.accountNo!=="1838"||vat.ok!==true||vat.accountNo!=="4718"||vat.confirmationSource!=="explicit")throw new Error("Explicit selected native role resolutions missing");
      const before=await get("company");
      cli("update-company-profile",JSON.stringify({address:"Must roll back",cvr:"invalid-synthetic-cvr"}),[],true);
      if(!isDeepStrictEqual(await get("company"),before))throw new Error("Refused profile update partially changed fields");
      evidence={profile:await get("company"),chart:await get("accounts"),ledgerUuid:journal.ledgerUuid,journal:journal.entries};writeFileSync(saved,JSON.stringify(evidence),{mode:0o600,flag:"wx"});
    }else evidence=JSON.parse(readFileSync(saved,"utf8"));
    const profile=await get("company"),chart=await get("accounts");
    if(!isDeepStrictEqual(profile,evidence.profile)||!isDeepStrictEqual(chart,evidence.chart)||!isDeepStrictEqual(cli("company-profile"),profile)||!isDeepStrictEqual(mcp("company_profile",{}),profile)||!isDeepStrictEqual(cli("accounts"),chart)||!isDeepStrictEqual(mcp("accounts",{}),chart))throw new Error("Full authoritative profile/chart/role facts differ or changed after restart");
    for(const[role,account]of[["bank","1838"],["input_vat","4718"]])if(!chart.accounts.accountRoles.resolutions.some((r:{role:string;accountNo:string;ok:boolean})=>r.role===role&&r.accountNo===account&&r.ok))throw new Error("Chosen current role not persisted");
    const after=(await get("journal")).journal;
    if(after.ledgerUuid!==evidence.ledgerUuid||!isDeepStrictEqual(after.entries,evidence.journal))throw new Error("Setup writes created or changed journal entries");
    console.log(restarted?"PASS: isolated native setup profile/chart/explicit roles and full agent parity survive fresh MFA login/restart; ledger unchanged":"PASS: isolated profile → chosen accounts → actual wrong-type/balance/missing/unknown role refusals → CLI bank/MCP VAT confirmation → canonical profile/chart parity; no journal effects");
  }finally{rmSync(temp,{recursive:true,force:true});}
}

// Standalone launch only: private synthetic fixture is created by the paired Python runner.
if(import.meta.main){
 const work=resolve(process.env.RM_SETUP_WORK??""),base=process.env.RM_SETUP_BASE??"";
 const info=lstatSync(work);
 if(!basename(work).startsWith("Rentemester-Synthetic-Company-Setup-")||!info.isDirectory()||info.isSymbolicLink()||(info.mode&0o077)!==0||info.uid!==process.getuid?.())throw new Error("Private owned synthetic setup fixture required");
 const config=JSON.parse(readFileSync(join(work,"server.json"),"utf8"));
 if(config.workspace!=="synthetic-native-company-setup"||config.companySlug!=="synthetic"||resolve(config.root)!==join(work,"workspace")||!/^http:\/\/127\.0\.0\.1:\d+$/.test(base))throw new Error("Owned literal-loopback setup fixture only");
 const statePath=join(work,"browser-state.json"),saved=join(work,"journey.json");
 const ownerEmail="owner@example.invalid",password="synthetic-owner-password-123";
 const {createHmac}=await import("node:crypto");
 function totp(secret:string){let bits=0,count=0;const bytes:number[]=[];for(const c of secret){const v="ABCDEFGHIJKLMNOPQRSTUVWXYZ234567".indexOf(c);if(v<0)throw new Error("Invalid synthetic MFA key");bits=(bits<<5)|v;count+=5;if(count>=8){count-=8;bytes.push((bits>>>count)&255);}}const counter=Buffer.alloc(8);counter.writeBigUInt64BE(BigInt(Math.floor(Date.now()/30000)));const d=createHmac("sha1",Buffer.from(bytes)).update(counter).digest(),o=d[19]&15;return String((d.readUInt32BE(o)&0x7fffffff)%1000000).padStart(6,"0");}
 async function login(page:Page,recovery:string){await page.goto(base);await page.getByLabel("E-mail",{exact:true}).fill(ownerEmail);await page.getByLabel("Adgangskode",{exact:true}).fill(password);await page.getByRole("button",{name:"Log ind",exact:true}).click();await page.getByRole("button",{name:"Brug recovery code"}).click();await page.getByLabel("Recovery code",{exact:true}).fill(recovery);await page.getByRole("button",{name:"Bekræft",exact:true}).click();await page.getByRole("link",{name:"Åbn bogføring"}).waitFor();}
 const browser=await chromium.launch({headless:true});try{
  const page=await browser.newPage();page.setDefaultTimeout(15000);const reopened=process.argv.includes("--reopen");
  if(reopened){const state=JSON.parse(readFileSync(statePath,"utf8"));await login(page,state.ownerRecovery);}else{
   const spool=join(config.root,".rentemester"),files=readdirSync(spool).filter(n=>n.startsWith("verification-")&&n.endsWith(".txt"));if(files.length!==1)throw new Error("Exactly one fresh private owner verification delivery required");const delivery=join(spool,files[0]),meta=lstatSync(delivery);if(!meta.isFile()||meta.isSymbolicLink()||(meta.mode&0o077)!==0||meta.uid!==process.getuid?.())throw new Error("Private owned owner delivery required");const link=readFileSync(delivery,"utf8").trim();if(new URL(link).origin!==base)throw new Error("Verification delivery origin mismatch");await page.goto(link);
   await page.goto(base);await page.getByLabel("E-mail",{exact:true}).fill(ownerEmail);await page.getByLabel("Adgangskode",{exact:true}).fill(password);await page.getByRole("button",{name:"Log ind",exact:true}).click();await page.getByRole("heading",{name:"Opsæt totrinsbekræftelse"}).waitFor();await page.getByLabel("Adgangskode",{exact:true}).fill(password);await page.getByRole("button",{name:"Start opsætning",exact:true}).click();const secret=await page.getByLabel("Opsætningsnøgle").inputValue();await page.getByLabel("Engangskode",{exact:true}).fill(totp(secret));await page.getByRole("button",{name:"Bekræft opsætning",exact:true}).click();await page.getByRole("region",{name:"Recovery codes"}).locator("li").first().waitFor();const codes=await page.getByRole("region",{name:"Recovery codes"}).locator("li").allTextContents();if(codes.length<2)throw new Error("Synthetic MFA recovery codes missing");await page.getByRole("button",{name:"Jeg har gemt koderne",exact:true}).click();await page.getByRole("link",{name:"Åbn bogføring"}).waitFor();await page.context().clearCookies();await new Promise(r=>setTimeout(r,11000));await login(page,codes[0]);writeFileSync(statePath,JSON.stringify({ownerRecovery:codes[1]}),{mode:0o600,flag:"wx"});
  }
  await checkNativeCompanySetupClients(page,base,{isolated:true,company:config.companySlug,evidencePath:saved},reopened);
  if(process.env.RM_NATIVE_COMPANY_CREATION==="1") await (await import("./check-native-company-creation-clients")).checkNativeCompanyCreationClients(page,base,{isolated:true,primary:config.companySlug,evidencePath:join(work,"company-creation-journey.json")},reopened);
 }finally{await browser.close();}
}
