/** Private disposable synthetic enrollment fixture; never copies a cookie or an MFA cipher. */
import { Database } from "bun:sqlite";
import { readFileSync, writeFileSync, mkdirSync, fstatSync, statSync } from "node:fs";
import { join, basename } from "node:path";
import { createHmac, randomUUID } from "node:crypto";
import { chromium, type Page } from "@playwright/test";
import { hashPassword } from "../../node_modules/better-auth/dist/crypto/password.mjs";
import { checkNativeReviewedBookkeeping } from "./check-native-reviewed-bookkeeping";
const ownerPassword = "synthetic-password-123", reviewerPassword = "synthetic-reviewer-password-456";
const evidencePath = "/tmp/rentemester-rust-reports/native-reviewed-bookkeeping.json";
if (process.argv.includes("--seed")) {
 const config = JSON.parse(readFileSync(process.env.RM_REVIEW_CONFIG!, "utf8"));
 if (config.workspace !== "synthetic-reviewed-bank" || !basename(join(config.root, "..")).startsWith("Rentemester-Synthetic-Review-")) throw new Error("Disposable synthetic fixture only");
 const held = fstatSync(Number(process.env.RM_REVIEW_OFFLINE_FD)); const root = statSync(config.root);
 if (held.ino !== root.ino || held.dev !== root.dev || !held.isDirectory()) throw new Error("Offline launcher root lease descriptor required");
 const password = await hashPassword(reviewerPassword); const id = randomUUID(), now = new Date().toISOString();
 const db = new Database(join(config.root, ".rentemester/workspace-control.sqlite"));
 try {
  db.exec("PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL");
  db.transaction(() => {
   if ((db.query("SELECT count(*) AS n FROM user").get() as {n:number}).n !== 1) throw new Error("Only freshly bootstrapped synthetic owner expected");
   db.query("INSERT INTO user(id,name,email,emailVerified,createdAt,updatedAt,twoFactorEnabled) VALUES(?,'Synthetic Reviewer','reviewer@example.invalid',1,?,?,0)").run(id,now,now);
   db.query("INSERT INTO account(id,userId,accountId,providerId,issuer,password,createdAt,updatedAt) VALUES(?,?,?,'credential','local:credential',?,?,?)").run(randomUUID(),id,id,password,now,now);
   db.query("INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor) VALUES(?,'activate','member','system:synthetic-fixture')").run(id);
   db.query("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES(?,'synthetic','grant','reviewer','system:synthetic-fixture')").run(id);
  }).immediate();
 } finally { db.close(); }
 const ledger = new Database(join(config.root,"synthetic/data/ledger.sqlite"));
 try { ledger.exec("PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL");ledger.transaction(() => {
  ledger.query("INSERT INTO bank_accounts(id,slug,name,currency,ledger_account_no) VALUES(1,'synthetic-review','Synthetic reviewed bank','DKK','1800')").run();
  ledger.query("INSERT INTO bank_transactions(id,transaction_date,text,amount,currency,bank_account_id) VALUES(1,?,'Synthetic separately reviewed capital',100.01,'DKK',1)").run(new Date().toISOString().slice(0,10));
 }).immediate(); } finally {ledger.close();}
 console.log("PASS: stopped owned synthetic workspace seeded with separate unenrolled credential/current reviewer grant; no sessions or MFA material inserted");
} else {
 const base = process.env.RM_REVIEW_BASE!; if (!/^http:\/\/127\.0\.0\.1:\d+$/.test(base)) throw new Error("Literal loopback fixture origin required");
 const browser = await chromium.launch({headless:true});
 function totp(secret:string){let bits=0,count=0;const bytes:number[]=[];for(const char of secret){const value="ABCDEFGHIJKLMNOPQRSTUVWXYZ234567".indexOf(char);if(value<0)throw new Error("Invalid synthetic TOTP key");bits=(bits<<5)|value;count+=5;if(count>=8){count-=8;bytes.push((bits>>>count)&255);}}const counter=Buffer.alloc(8);counter.writeBigUInt64BE(BigInt(Math.floor(Date.now()/30000)));const d=createHmac("sha1",Buffer.from(bytes)).update(counter).digest();const offset=d[19]&15;return String((d.readUInt32BE(offset)&0x7fffffff)%1000000).padStart(6,"0");}
 async function enrollAndLogin(page:Page,email:string,password:string){
  await page.goto(base);await page.getByLabel("E-mail",{exact:true}).fill(email);await page.getByLabel("Adgangskode",{exact:true}).fill(password);await page.getByRole("button",{name:"Log ind",exact:true}).click();
  await page.getByRole("heading",{name:"Opsæt totrinsbekræftelse"}).waitFor();await page.getByLabel("Adgangskode",{exact:true}).fill(password);await page.getByRole("button",{name:"Start opsætning",exact:true}).click();
  const key=await page.getByLabel("Opsætningsnøgle").inputValue();await page.getByLabel("Engangskode",{exact:true}).fill(totp(key));await page.getByRole("button",{name:"Bekræft opsætning",exact:true}).click();
  const recovery=await page.getByRole("region",{name:"Recovery codes"}).locator("li").first().textContent();const nextRecovery=await page.getByRole("region",{name:"Recovery codes"}).locator("li").nth(1).textContent();if(!recovery||!nextRecovery)throw new Error("Synthetic native recovery codes missing");
  await page.getByRole("button",{name:"Jeg har gemt koderne",exact:true}).click();await page.getByRole("link",{name:"Åbn bogføring"}).waitFor();await page.context().clearCookies();
  // Preserve native 3-per-10-second sign-in budget across the two local browser contexts.
  await new Promise(resolve=>setTimeout(resolve,11000));
  await login(page,email,password,recovery);console.log("PASS: separate native MFA enrollment then fresh password/recovery login");return nextRecovery;
 }
 async function login(page:Page,email:string,password:string,code:string){
  await page.goto(base);await page.getByLabel("E-mail",{exact:true}).fill(email);await page.getByLabel("Adgangskode",{exact:true}).fill(password);const [loginResponse]=await Promise.all([page.waitForResponse(r=>r.url().includes("/sign-in/email")&&r.request().method()==="POST"),page.getByRole("button",{name:"Log ind",exact:true}).click()]);
  const loginBody=await loginResponse.json();if(loginResponse.status()!==200||loginBody.twoFactorRedirect!==true)throw new Error(`Synthetic native password challenge refused (${loginResponse.status()})`);
  await page.getByRole("button",{name:"Brug recovery code"}).click();await page.getByLabel("Recovery code",{exact:true}).fill(code);await page.getByRole("button",{name:"Bekræft",exact:true}).click();await page.getByRole("link",{name:"Åbn bogføring"}).waitFor();
 }
 try {
  const author=await browser.newPage(),reviewer=await browser.newPage();const failures:string[]=[];for(const page of [author,reviewer]){page.setDefaultTimeout(15000);page.on("pageerror",e=>failures.push(e.message));}
  if(process.argv.includes("--reopen")){
   const saved=JSON.parse(readFileSync(evidencePath,"utf8"));await login(author,"owner@example.invalid",ownerPassword,saved.ownerRecovery);await login(reviewer,"reviewer@example.invalid",reviewerPassword,saved.reviewerRecovery);
   const read=await reviewer.request.get(`${base}/api/companies/synthetic/accounting-drafts`);const posted=(await read.json()).accountingDrafts.find((d:{id:string})=>d.id===saved.draftId);if(posted?.journalEntryId!==saved.journalEntryId||posted?.status!=="approved_posted")throw new Error("Restart lost reviewed journal effect");
   const replay=await reviewer.request.post(`${base}/api/companies/synthetic/accounting-drafts/${saved.draftId}/approve-and-post`,{headers:{origin:base},data:{expectedEventHash:saved.submittedHash,confirm:true}});if(!replay.ok()||(await replay.json()).accountingDraft.journalEntryId!==saved.journalEntryId)throw new Error("Restart exact receipt replay failed");
   const journalResponse=await reviewer.request.get(`${base}/api/companies/synthetic/journal`);const journal=(await journalResponse.json()).journal;
   if(journal.entries.length!==1||journal.entries[0].id!==saved.journalEntryId)throw new Error("Isolated restart did not preserve exactly one journal effect");saved.exactJournalCount=journal.entries.length;
   delete saved.ownerRecovery;delete saved.reviewerRecovery;writeFileSync(evidencePath,JSON.stringify(saved,null,2),{mode:0o600});console.log("PASS: both distinct native password/MFA logins and exact posting receipt persist across owned server restart");
  }else{
   await author.goto(process.env.RM_REVIEW_VERIFY!);const ownerRecovery=await enrollAndLogin(author,"owner@example.invalid",ownerPassword);const reviewerRecovery=await enrollAndLogin(reviewer,"reviewer@example.invalid",reviewerPassword);
   const result=await checkNativeReviewedBookkeeping(author,reviewer,base,{bankTransactionId:1,counterAccount:"8000"});if(failures.length)throw new Error("Browser page errors during synthetic journey");
   mkdirSync("/tmp/rentemester-rust-reports",{recursive:true});await reviewer.screenshot({path:"/tmp/rentemester-rust-reports/native-reviewed-bookkeeping.png",fullPage:true});writeFileSync(evidencePath,JSON.stringify({...result,ownerRecovery,reviewerRecovery,twoDistinctNativeMfaLogins:true},null,2),{mode:0o600});
  }
 }finally{await browser.close();}
}
