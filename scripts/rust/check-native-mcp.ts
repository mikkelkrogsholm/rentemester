import type {Page} from "@playwright/test";
import {spawn} from "node:child_process";
import {mkdtempSync,writeFileSync,readFileSync,rmSync} from "node:fs";
import {tmpdir} from "node:os";
import {join} from "node:path";
import {createInterface} from "node:readline";
import {isDeepStrictEqual} from "node:util";

/** Exercise the real stdio MCP process and the same current-principal native server. */
export async function checkNativeMcp(page:Page,base:string){
 const binary=process.env.RM_NATIVE_MCP_BIN;if(!binary)return;
 const directory=mkdtempSync(join(tmpdir(),"rentemester-native-mcp-"));
 const file=join(directory,"session.cookie");
 const cookies=await page.context().cookies(base);
 writeFileSync(file,cookies.map(c=>`${c.name}=${c.value}`).join("; "),{mode:0o600});
 const child=spawn(binary,["--server",base,"--allow-loopback-http","--cookie-file",file],{stdio:["pipe","pipe","pipe"]});
 const pending=new Map<number,{resolve:(value:any)=>void;reject:(error:Error)=>void}>();let next=0;
 const lines=createInterface({input:child.stdout});
 lines.on("line",line=>{try{const response=JSON.parse(line);const waiter=pending.get(response.id);if(waiter){pending.delete(response.id);if(response.error)waiter.reject(new Error(`MCP protocol refusal ${response.error.code}`));else waiter.resolve(response.result);}}catch{for(const w of pending.values())w.reject(new Error("MCP emitted invalid JSON"));pending.clear();}});
 child.stderr.on("data",()=>{});
 child.on("error",()=>{for(const w of pending.values())w.reject(new Error("MCP process unavailable"));pending.clear();});
 child.on("exit",()=>{for(const w of pending.values())w.reject(new Error("MCP exited before reply"));pending.clear();});
 function request(method:string,params:unknown):Promise<any>{const id=++next;return new Promise((resolve,reject)=>{const timer=setTimeout(()=>{pending.delete(id);reject(new Error(`MCP ${method} timed out`));},30000);pending.set(id,{resolve:value=>{clearTimeout(timer);resolve(value);},reject:error=>{clearTimeout(timer);reject(error);}});child.stdin.write(JSON.stringify({jsonrpc:"2.0",id,method,params})+"\n");});}
 try{
  const initialized=await request("initialize",{protocolVersion:"2025-11-25",capabilities:{},clientInfo:{name:"rentemester-native-smoke",version:"1"}});if(initialized.protocolVersion!=="2025-11-25")throw new Error("MCP negotiation failed");
  child.stdin.write(JSON.stringify({jsonrpc:"2.0",method:"notifications/initialized"})+"\n");
  const listed=await request("tools/list",{});if(!["me","companies","journal","trial_balance","income_statement","balance","post_journal","reverse_journal"].every(name=>listed.tools.some((t:{name:string})=>t.name===name)))throw new Error("MCP tools missing");
  for(const [name,endpoint]of [["companies","/api/companies"],["journal","/api/companies/synthetic/journal"],["trial_balance","/api/companies/synthetic/trial-balance"],["income_statement","/api/companies/synthetic/income-statement"],["balance","/api/companies/synthetic/balance"]]){
   const result=await request("tools/call",{name,arguments:name==="companies"?{}:{company:"synthetic"}});const expected=await(await page.request.get(`${base}${endpoint}`)).json();
   if(result.isError||!isDeepStrictEqual(result.structuredContent?.data,expected))throw new Error(`MCP ${name} differs from native browser API`);
  }
  const state=JSON.parse(readFileSync(process.env.RM_NATIVE_WEB_STATE!,"utf8"));
  const before=await(await page.request.get(`${base}/api/companies/synthetic/journal`)).json();
  const replay=await request("tools/call",{name:"post_journal",arguments:{company:"synthetic",envelope:state.postedBody}});
  if(replay.isError||replay.structuredContent?.ok!==true)throw new Error("MCP idempotent posting replay failed");
  const after=await(await page.request.get(`${base}/api/companies/synthetic/journal`)).json();if(!isDeepStrictEqual(before,after))throw new Error("MCP replay duplicated or changed journal");
  console.log("PASS: real Rust MCP stdio negotiation/tools, authenticated company/financial reads and idempotent posting replay match native server without another journal");
 }finally{child.stdin.end();child.kill();lines.close();rmSync(directory,{recursive:true,force:true});}
}
