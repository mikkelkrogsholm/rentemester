import type { Page } from "@playwright/test";
import { request } from "@playwright/test";
import { spawnSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { isDeepStrictEqual } from "node:util";

export function backupDestinationProof(page:Page,base:string,work:string){
 let operation="",canonical:any;
 const id="synthetic-destination",cookieFile=join(work,"backup-owner.cookie");
 const need=(v:unknown,s:string)=>{if(!v)throw Error(s);};
 const get=async(path:string)=>{const r=await page.request.get(base+path);need(r.ok(),`Backup GET${r.status()}`);return r.json();};
 const books=async()=>({journal:(await get("/api/companies/synthetic/journal")).journal,trial:(await get("/api/companies/synthetic/trial-balance")).trialBalance,bank:(await get("/api/companies/synthetic/bank")).bank});
 function client(kind:"cli"|"mcp",name:string,op?:string){
  const common=["--server",base,"--allow-loopback-http","--cookie-file",cookieFile];
  const args=kind==="cli"?["remote",name,...common,...(op?["--destination-id",id,"--operation-id",op]:[])]:common;
  const input=kind==="cli"?"":[
   {jsonrpc:"2.0",id:1,method:"initialize",params:{protocolVersion:"2025-11-25",capabilities:{},clientInfo:{name:"synthetic-backup",version:"1"}}},
   {jsonrpc:"2.0",method:"notifications/initialized"},
   {jsonrpc:"2.0",id:2,method:"tools/call",params:{name,arguments:op?{destinationId:id,operationId:op}:{}}},
  ].map(v=>JSON.stringify(v)).join("\n")+"\n";
  const r=spawnSync(join(work,kind==="cli"?"rentemester-rust":"rentemester-mcp"),args,{input,encoding:"utf8",timeout:30000});need(r.status===0,"Backup client process failed");
  if(kind==="cli")return JSON.parse(r.stdout).data;
  const reply=r.stdout.trim().split("\n").map(v=>JSON.parse(v)).find(v=>v.id===2);need(reply?.result?.isError===false,"MCP backup read failed");return reply.result.structuredContent.data;
 }
 async function verify(){
  const value=(await get(`/api/workspace/backup-destinations/${id}/placements/${operation}`)).placement;
  need(value.verified&&value.signatureVerified&&value.auditRecorded&&!value.remoteSyncVerified&&!value.destination.offsiteAttested,"Backup truthfulness/receipt failed");
  need(isDeepStrictEqual(client("cli","verify-backup-placement",operation).placement,value),"CLI readback differs");
  need(isDeepStrictEqual(client("mcp","verify_backup_placement",operation).placement,value),"MCP readback differs");
  return value;
 }
 return {
  async run(){
   let stage="prepare";
   try{
   const before=await books();
   writeFileSync(cookieFile,(await page.context().cookies(base)).map(c=>`${c.name}=${c.value}`).join("; ")+"\n",{mode:0o600,flag:"wx"});
   const listed=await get("/api/workspace/backup-destinations");need(listed.destinations.length===1&&listed.destinations[0].id===id,"One configured destination required");
   need(isDeepStrictEqual(client("cli","backup-destinations"),listed)&&isDeepStrictEqual(client("mcp","backup_destinations"),listed),"Destination list parity differs");
   const anonymous=await request.newContext();try{for(const method of ["GET","POST"]){const r=await anonymous.fetch(base+`/api/workspace/backup-destinations${method==="POST"?`/${id}/place`:""}`,{method,headers:{origin:base,"content-type":"application/json"},...(method==="POST"?{data:{operationId:crypto.randomUUID(),confirm:true}}:{})});need([401,403].includes(r.status()),"Anonymous backup admitted");}}finally{await anonymous.dispose();}
   stage="navigate-integrity"; await page.goto(base+"/companies/synthetic/integritet"); stage="show-destinations";await page.getByRole("button",{name:"Vis backupdestinationer",exact:true}).click();stage="wait-destination-selector"; await page.getByLabel("Backupdestination",{exact:true}).waitFor();
   stage="confirm-placement"; await page.getByRole("checkbox",{name:"Jeg vil danne og placere en backup af hele workspacet.",exact:true}).check();
   stage="publish-placement"; const response=page.waitForResponse(r=>r.url().endsWith(`/${id}/place`)&&r.request().method()==="POST");await page.getByRole("button",{name:"Dan og placér backup",exact:true}).click();const posted=await response;const result=await posted.json();need(posted.ok(),"UI placement refused");operation=result.placement.operationId;await page.getByText(/Backupfilen er genlæst/).waitFor();
   canonical=await verify();const path=join(work,"offsite",`${operation}.native-backup.json`),bytes=readFileSync(path);
   const independent=spawnSync(join(work,"rentemester-server"),["--verify-backup",path,"--trusted-backup-key",join(work,"trusted.pub")],{encoding:"utf8",timeout:60000});need(independent.status===0,"Independent signed/SQLite backup verification refused");
   const duplicate=await page.request.post(base+`/api/workspace/backup-destinations/${id}/place`,{headers:{origin:base},data:{operationId:operation,confirm:true}});need(duplicate.status()===422&&(await duplicate.json()).code==="backup_destination_exists_read_back","Existing placement overwritten");need(readFileSync(path).equals(bytes),"Duplicate changed exact archive");
   writeFileSync(path,Buffer.from("Synthetic tampered landed bytes"));try{const bad=await page.request.get(base+`/api/workspace/backup-destinations/${id}/placements/${operation}`);need(bad.status()===422,"Tampered backup accepted");}finally{writeFileSync(path,bytes);}
   need(isDeepStrictEqual(await verify(),canonical),"Exact landed receipt changed");need(isDeepStrictEqual(await books(),before),"Backup changed accounting");
   await page.reload();await page.getByRole("button",{name:"Vis backupdestinationer",exact:true}).click();await page.getByText(`Placering: ${operation}`,{exact:true}).waitFor();await page.getByRole("button",{name:"Kontrollér placeringen",exact:true}).click();await page.getByText(/Backupfilen er genlæst/).waitFor();
   return {operationId:operation,archiveSha256:canonical.archiveSha256,archiveSizeBytes:canonical.archiveSizeBytes,normalWebPlacement:true,cliMcpReadbackParity:true,independentArtifactAndSqliteVerification:true,noOverwrite:true,tamperRefused:true,anonymousRefused:true,uiReloadExactId:true,unchangedAccounting:true,cloudSyncClaim:false,attestationClaim:false};
   }catch(error){
    writeFileSync(join(work,"backup-diagnostic.json"),JSON.stringify({stage,pathname:new URL(page.url()).pathname,headings:await page.getByRole("heading").allTextContents(),buttons:await page.getByRole("button").allTextContents(),alerts:await page.getByRole("alert").allTextContents()}),{mode:0o600});throw error;
   }
  },
  async reopen(){const value=await verify();need(isDeepStrictEqual(value,canonical),"Placement/audit changed after restart");return{sameExactArchiveAndAuditAfterRestart:true,operationId:operation,archiveSha256:value.archiveSha256};},
 };
}
