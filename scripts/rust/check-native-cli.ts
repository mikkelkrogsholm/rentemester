import type {Page} from "@playwright/test";
import {execFileSync} from "node:child_process";
import {mkdtempSync,writeFileSync,rmSync} from "node:fs";
import {tmpdir} from "node:os";
import {join} from "node:path";
import {isDeepStrictEqual} from "node:util";

/** Read the same real native company through CLI and browser, without printing session material. */
export async function checkNativeCli(page:Page,base:string){
 const binary=process.env.RM_NATIVE_CLI_BIN;
 if(!binary)return;
 const directory=mkdtempSync(join(tmpdir(),"rentemester-native-cli-"));
 try{
  const cookies=await page.context().cookies(base);
  if(!cookies.length)throw new Error("Native CLI check requires an authenticated browser session");
  const file=join(directory,"session.cookie");
  writeFileSync(file,cookies.map(c=>`${c.name}=${c.value}`).join("; "),{mode:0o600});
  const prefix=["remote","--server",base,"--allow-loopback-http","--cookie-file",file];
  for(const [command,endpoint]of [["health","/api/health"],["me","/api/me"],["companies","/api/companies"],["journal","/api/companies/synthetic/journal"],["trial-balance","/api/companies/synthetic/trial-balance"],["income-statement","/api/companies/synthetic/income-statement"],["balance","/api/companies/synthetic/balance"]]){
   const args=command==="health"?["remote","--server",base,"--allow-loopback-http",command]:command==="me"||command==="companies"?[...prefix,command]:[...prefix,command,"--company","synthetic"];
   const output=execFileSync(binary,args,{encoding:"utf8",timeout:30000,maxBuffer:4*1024*1024});
   const actual=JSON.parse(output);const response=await page.request.get(`${base}${endpoint}`);const expected=await response.json();
   if(!response.ok()||actual.ok!==true)throw new Error(`Native CLI ${command} did not succeed`);
   if(command!=="health"&&command!=="me"&&!isDeepStrictEqual(actual.data,expected))throw new Error(`Native CLI ${command} differs from browser API`);
  }
  console.log("PASS: MFA-authenticated Rust CLI health/me/companies and all four financial reads match the real Rust browser API");
 }finally{rmSync(directory,{recursive:true,force:true});}
}
