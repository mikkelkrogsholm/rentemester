import type {Page} from "@playwright/test";
import {existsSync,writeFileSync} from "node:fs";
import {join} from "node:path";

/** Active synthetic GUI test handoff. Export only to caller-created private directory. */
export async function prepareNativeDesktopSession(page:Page,base:string,totpSecret?:string){
 const directory=process.env.RM_NATIVE_GUI_SESSION_DIR;if(!directory||process.env.RM_NATIVE_WEB_REOPEN)return;
 const cookieFile=join(directory,"session.cookie"),ready=join(directory,"ready.json"),done=join(directory,"done");
 const cookies=await page.context().cookies(base);
 writeFileSync(cookieFile,cookies.map(c=>`${c.name}=${c.value}`).join("; "),{mode:0o600,flag:"wx"});
 writeFileSync(ready,JSON.stringify({origin:base,cookieFile,email:"owner@example.invalid",password:"synthetic-password-123",totpSecret}),{mode:0o600,flag:"wx"});
 console.log("Synthetic MFA session prepared for active native desktop check");
 const deadline=Date.now()+300000;
 while(!existsSync(done)){
  if(Date.now()>deadline)throw new Error("Native desktop check did not signal completion within five minutes");
  await new Promise(resolve=>setTimeout(resolve,1000));
 }
}
