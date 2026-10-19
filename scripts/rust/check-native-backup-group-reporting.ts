/** Opt-in J05 group source. Ordinary APIs/MFA only; no database/auth fixtures. */
import type { Page } from "@playwright/test";
import { basename, join, resolve } from "node:path";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { createHash } from "node:crypto";
import { isDeepStrictEqual } from "node:util";
const reviewerEmail="portable-group-reviewer@example.invalid";
const reviewerPassword="synthetic-portable-group-reviewer-789";
const groupId="synthetic-portable-reporting",profileId="synthetic-portable-report-profile";
const mapForward="portable-group-forward-map",mapReverse="portable-group-reverse-map";
const digest=(bytes:Buffer)=>createHash("sha256").update(bytes).digest("hex");
type Context={author:Page;base:string;work:string;root:string;slugs:string[];enroll:(page:Page,email:string,password:string)=>Promise<string>};
export type GroupReportingSource={asOf:string;from:string;slugs:string[];authorId:string;reviewerId:string;invitationId:string;views:any;originals:Array<{company:string;id:number;bytesBase64:string;sha256:string;contentType:string|undefined}>;receipts:{manifests:any[];mappings:any[];profiles:any[];eliminations:any[]};evidence:Record<string,any>};
function api(author:Page,base:string){
 if(!/^http:\/\/127\.0\.0\.1:\d+$/.test(base))throw Error("Owned loopback group fixture required");
 const headers={origin:base,"content-type":"application/json"};
 return{
  get:async(path:string)=>{const r=await author.request.get(base+path);if(!r.ok())throw Error(`Group read ${path} refused ${r.status()}`);return r.json();},
  post:async(path:string,data:unknown,status=200)=>{const r=await author.request.post(base+path,{headers,data});if(r.status()!==status)throw Error(`Group command ${path}: expected ${status}, received ${r.status()}`);return r.json();}
 };
}
async function reviewer(ctx:Context){
 if(!basename(ctx.work).startsWith("Rentemester-Synthetic-Portable-")||(statSync(ctx.work).mode&0o077)!==0||resolve(ctx.root)!==join(ctx.work,"source"))throw Error("Owned source namespace only");
 const a=api(ctx.author,ctx.base),spool=join(ctx.root,".rentemester");
 const files=(prefix:string)=>readdirSync(spool).filter(n=>n.startsWith(prefix)&&n.endsWith(".txt"));
 const delivery=(prefix:string,before:string[])=>{const added=files(prefix).filter(n=>!before.includes(n));if(added.length!==1)throw Error("Exactly one owned private group delivery required");const link=readFileSync(join(spool,added[0]),"utf8").trim();if(new URL(link).origin!==ctx.base)throw Error("Owned group delivery origin differs");return link;};
 const invitations=files("invitation-");
 const issued=await a.post("/api/workspace/invitations",{email:reviewerEmail,workspaceRole:"workspace_owner",companySlug:ctx.slugs[0],companyRole:"owner"});
 const page=await ctx.author.context().browser()!.newPage();page.setDefaultTimeout(45000);
 try{
  const before=files("verification-");await page.goto(delivery("invitation-",invitations));await page.getByRole("heading",{name:"Acceptér invitation",exact:true}).waitFor();
  if(new URL(page.url()).hash)throw Error("Group invitation bearer retained in URL");
  await page.getByLabel("Navn",{exact:true}).fill("Synthetic Portable Group Reviewer");await page.getByLabel("Adgangskode",{exact:true}).fill(reviewerPassword);
  const[claim]=await Promise.all([page.waitForResponse(r=>new URL(r.url()).pathname==="/api/invitations/claim"&&r.request().method()==="POST"),page.getByRole("button",{name:"Acceptér invitation",exact:true}).click()]);
  if(!claim.ok()||claim.headers()["set-cookie"])throw Error("Group claim failed or minted authenticated authority");await page.getByText("Invitationen er accepteret.",{exact:true}).waitFor();
  if((await page.request.get(`${ctx.base}/api/companies/${ctx.slugs[0]}/journal`)).ok())throw Error("Group invitation bypassed verification/MFA");
  const pending=(await a.get("/api/workspace/members")).members.find((m:any)=>m.email===reviewerEmail);if(pending?.accessReady!==false||pending.workspaceRole!=="workspace_owner")throw Error("Group invitation readiness/explicit workspace role differs");
  await page.goto(delivery("verification-",before));await ctx.enroll(page,reviewerEmail,reviewerPassword);
  const b=api(page,ctx.base),me=await b.get("/api/me"),author=await a.get("/api/me");if(!me.user?.id||me.user.id===author.user.id||me.user.id!==pending.userId)throw Error("Distinct ordinary group MFA reviewer required");
  await a.post("/api/workspace/members/company",{userId:me.user.id,action:"grant",companySlug:ctx.slugs[1],role:"owner"});
  const member=(await a.get("/api/workspace/members")).members.find((m:any)=>m.userId===me.user.id);
  if(member.workspaceRole!=="workspace_owner"||member.accessReady!==true||member.memberships.length!==2||ctx.slugs.some(s=>!member.memberships.some((m:any)=>m.companySlug===s&&m.role==="owner")))throw Error("Explicit group workspace-owner and both company-owner grants missing");
  return{page,id:me.user.id as string,authorId:author.user.id as string,invitationId:issued.invitation.invitationId as string};
 }catch(error){await page.context().close();throw error;}
}
async function originals(author:Page,base:string,rows:GroupReportingSource["originals"]){
 const out=[];
 for(const row of rows){const r=await author.request.get(`${base}/api/companies/${row.company}/documents/${row.id}/file`);if(!r.ok())throw Error("Retained group source original missing");const bytes=Buffer.from(await r.body());if(bytes.toString("base64")!==row.bytesBase64||digest(bytes)!==row.sha256||r.headers()["content-type"]!==row.contentType)throw Error("Retained group source original bytes/MIME differ");out.push(row);}
 return out;
}
export async function groupReportingViews(author:Page,base:string,source:Pick<GroupReportingSource,"asOf"|"from"|"slugs"|"originals">){
 const a=api(author,base),current:Record<string,unknown>={},journals:Record<string,any>={};
 for(const slug of source.slugs){current[slug]={intercompany:await a.get(`/api/companies/${slug}/intercompany`),profiles:await a.get(`/api/companies/${slug}/consolidation?asOf=${source.asOf}`),eliminations:await a.get(`/api/companies/${slug}/eliminations`)};journals[slug]=await a.get(`/api/companies/${slug}/journal`);}
 const query=new URLSearchParams({profileId,from:source.from,asOf:source.asOf});
 return{current,journals,reconciliation:await a.get(`/api/group-reconciliation?asOf=${source.asOf}`),applied:await a.get(`/api/group-eliminations?asOf=${source.asOf}`),available:await a.get(`/api/group-report-profiles?asOf=${source.asOf}`),report:await a.get(`/api/group-consolidated-report?${query}`),originals:await originals(author,base,source.originals)};
}
export async function prepareBackupGroupReporting(ctx:Context):Promise<GroupReportingSource>{
 if(!isDeepStrictEqual(ctx.slugs,["synthetic","synthetic-portable-second"]))throw Error("Explicit J05 two-company fixture required");
 const a=api(ctx.author,ctx.base),[left,right]=ctx.slugs,inter=`/api/companies/${left}/intercompany`,consol=`/api/companies/${left}/consolidation`,elim=`/api/companies/${left}/eliminations`;
 const prior=await a.get(inter);if(prior.manifest!==null||prior.manifestHash!==null)throw Error("Fresh explicit V1 group source required");
 const checker=await reviewer(ctx),b=api(checker.page,ctx.base);
 try{
  const asOf=new Date().toLocaleDateString("sv-SE",{timeZone:"Europe/Copenhagen"}),from=asOf.slice(0,4)+"-01-01";
  const retained:GroupReportingSource["originals"]=[],receipts:GroupReportingSource["receipts"]={manifests:[],mappings:[],profiles:[],eliminations:[]};
  const before:Record<string,any>={};for(const slug of ctx.slugs)before[slug]=await a.get(`/api/companies/${slug}/journal`);
  for(const slug of ctx.slugs){
   const accounts=slug===left?[["1871","asset","debit"],["2872","liability","credit"],["8871","equity","credit"]]:[["2871","liability","credit"],["1872","asset","debit"],["8871","equity","credit"]];
   for(const[accountNo,type,normalBalance]of accounts)await a.post(`/api/companies/${slug}/accounts`,{accountNo,name:`Synthetic portable group ${accountNo}`,type,normalBalance});
  }
  for(const[slug,orientation,lines]of [
   [left,"forward",[{accountNo:"1871",debitAmount:"12.34",creditAmount:"0.00"},{accountNo:"8871",debitAmount:"0.00",creditAmount:"12.34"}]],
   [right,"forward",[{accountNo:"8871",debitAmount:"12.34",creditAmount:"0.00"},{accountNo:"2871",debitAmount:"0.00",creditAmount:"12.34"}]],
   [left,"reverse",[{accountNo:"8871",debitAmount:"56.78",creditAmount:"0.00"},{accountNo:"2872",debitAmount:"0.00",creditAmount:"56.78"}]],
   [right,"reverse",[{accountNo:"1872",debitAmount:"56.78",creditAmount:"0.00"},{accountNo:"8871",debitAmount:"0.00",creditAmount:"56.78"}]],
  ]as const){
   const raw=Buffer.from(`Synthetic portable group source ${slug} ${orientation}\n`),created=await a.post(`/api/companies/${slug}/documents/ingest`,{confirm:true,fileName:`portable-group-${slug}-${orientation}.txt`,fileBase64:raw.toString("base64"),metadata:{source:"Synthetic portable group source",documentType:"cash_register_receipt"}});
   const r=await ctx.author.request.get(`${ctx.base}/api/companies/${slug}/documents/${created.document.id}/file`);if(!r.ok()||!Buffer.from(await r.body()).equals(raw))throw Error("Group source ingest/readback differs");
   retained.push({company:slug,id:created.document.id,bytesBase64:raw.toString("base64"),sha256:digest(raw),contentType:r.headers()["content-type"]});
   const journal=(await a.get(`/api/companies/${slug}/journal`)).journal;
   await a.post(`/api/companies/${slug}/manual-journal`,{confirm:true,input:{format:"rentemester-manual-journal-post/1",expectedRuleVersion:journal.nativePosting.ruleVersion,idempotencyKey:`synthetic-portable-group-${slug}-${orientation}`,request:{documentId:String(created.document.id),transactionDate:asOf,text:`Synthetic portable group ${orientation}`,currency:"DKK",lines}}});
  }
  const posted:Record<string,any>={};for(const slug of ctx.slugs){posted[slug]=await a.get(`/api/companies/${slug}/journal`);const old=before[slug].journal,newer=posted[slug].journal;if(old.ledgerUuid!==newer.ledgerUuid||newer.entries.length!==old.entries.length+2||old.entries.some((entry:any)=>!newer.entries.some((v:any)=>isDeepStrictEqual(entry,v)))||retained.filter(r=>r.company===slug).some(r=>!newer.entries.some((j:any)=>j.documentId===r.id)))throw Error("Explicit paired journals did not preserve prior ledger/retained source links");}
  const manifest={version:1,groups:[{id:groupId,name:"Synthetic portable group source",memberships:[{id:"portable-left",companySlug:left,validFrom:"2000-01-01"},{id:"portable-right",companySlug:right,validFrom:"2000-01-01"}],ownership:[{id:"portable-explicit-child",parentCompanySlug:left,childCompanySlug:right,basisPoints:10000,evidenceRefs:["synthetic-explicit-v1-group-source"],validFrom:"2000-01-01"}]}]};
  for(const name of ["Synthetic portable group source","Synthetic portable group reviewed"]){const input=structuredClone(manifest);input.groups[0].name=name;const receipt=await a.post(inter+"/manifest",{...input,confirm:true});receipts.manifests.push({input,receipt,actor:`user:${checker.authorId}`});}
  for(const reverse of [false,true]){
   const mapping={id:reverse?mapReverse:mapForward,groupId,leftCompanySlug:left,rightCompanySlug:right,leftAccountNos:[reverse?"2872":"1871"],rightAccountNos:[reverse?"1872":"2871"],leftPosition:reverse?"payable":"receivable",rightPosition:reverse?"receivable":"payable",evidenceRefs:["synthetic-paired-retained-journals"],validFrom:"2000-01-01"};
   for(const cycle of reverse?[0]:[0,1]){
    const p=await a.post(inter+"/propose",{...mapping,confirm:true});receipts.mappings.push({event:"proposed",mapping,hash:p.mappingHash,actor:`user:${checker.authorId}`});
    if(cycle===0)await a.post(inter+"/approve",{mappingId:mapping.id,mappingHash:p.mappingHash,confirm:true},422);
    await b.post(inter+"/approve",{mappingId:mapping.id,mappingHash:p.mappingHash,confirm:true});receipts.mappings.push({event:"approved",mapping,hash:p.mappingHash,actor:`user:${checker.id}`});
    if(!reverse&&cycle===0){await a.post(inter+"/revoke",{mappingId:mapping.id,mappingHash:p.mappingHash,confirm:true});receipts.mappings.push({event:"revoked",mapping,hash:p.mappingHash,actor:`user:${checker.authorId}`});}
   }
  }
  const reportingLines=[["assets","Assets","asset"],["debt","Debt","liability"],["equity","Equity","equity"],["result","Current result","equity"],["income","Income","income"],["expense","Expense","expense"]].map(([id,label,section],displayOrder)=>({id,label,section,displayOrder,...(id==="result"?{role:"current-result"}:{})}));
  const chosen=[[left,"1759","assets"],[left,"8759","equity"],[left,"1100","assets"],[left,"1000","income"],[left,"4200","debt"],[left,"1871","assets"],[left,"2872","debt"],[left,"8871","equity"],[right,"1759","assets"],[right,"8759","equity"],[right,"2871","debt"],[right,"1872","assets"],[right,"8871","equity"]];
  const profile={version:1,id:profileId,groupId,currency:"DKK",reportingLines,accountMappings:chosen.map(([companySlug,accountNo,reportingLineId],i)=>({id:`portable-report-${String(i).padStart(2,"0")}`,companySlug,accountNo,reportingLineId,validFrom:"2000-01-01"})),evidenceRefs:["synthetic-reviewed-explicit-chart"],validFrom:"2000-01-01"};
  for(const state of ["approved","revoked","proposed"]){
   const input={...profile,id:state==="approved"?profileId:`${profileId}-${state}`};const p=await a.post(consol+"/propose",{profile:input,confirm:true});receipts.profiles.push({event:"proposed",profile:input,hash:p.profileHash,actor:`user:${checker.authorId}`});
   if(state!=="proposed"){
    if(state==="approved")await a.post(consol+"/approve",{profileId:input.id,profileHash:p.profileHash,confirm:true},422);
    await b.post(consol+"/approve",{profileId:input.id,profileHash:p.profileHash,confirm:true});receipts.profiles.push({event:"approved",profile:input,hash:p.profileHash,actor:`user:${checker.id}`});
    if(state==="revoked"){await a.post(consol+"/revoke",{profileId:input.id,confirm:true});receipts.profiles.push({event:"revoked",profile:input,hash:p.profileHash,actor:`user:${checker.authorId}`});}
   }
  }
  async function eliminate(id:string,mappingId:string,reverse:boolean,reapply=false){
   const input={id,mappingId,asOf,evidenceRefs:["synthetic-paired-retained-journals"]};
   for(const cycle of reapply?[0,1]:[0]){
    const plan=(await a.post(elim+"/plan",input)).plan,p=plan.payload;
    if(p.version!==2||p.left.companySlug!==left||p.right.companySlug!==right||p.left.debitOre!==(reverse?"5678":"0")||p.left.creditOre!==(reverse?"0":"1234")||p.right.debitOre!==(reverse?"0":"1234")||p.right.creditOre!==(reverse?"5678":"0"))throw Error("Native V2 exact both-orientation plan differs");
    await a.post(elim+"/propose",{...input,expectedPayloadHash:plan.payloadHash,confirm:true});receipts.eliminations.push({event:"proposed",payload:p,hash:plan.payloadHash,actor:`user:${checker.authorId}`});
    const exact={eliminationId:id,payloadHash:plan.payloadHash,confirm:true};
    if(cycle===0)await a.post(elim+"/approve",exact,422);
    await b.post(elim+"/approve",exact);receipts.eliminations.push({event:"approved",payload:p,hash:plan.payloadHash,actor:`user:${checker.id}`});
    await b.post(elim+"/apply",exact);receipts.eliminations.push({event:"applied",payload:p,hash:plan.payloadHash,actor:`user:${checker.id}`});
    if(id==="portable-group-reversed"||(reapply&&cycle===0)){await a.post(elim+"/reverse",exact);receipts.eliminations.push({event:"reversed",payload:p,hash:plan.payloadHash,actor:`user:${checker.authorId}`});}
   }
  }
  await eliminate("portable-group-reversed",mapForward,false);await eliminate("portable-group-forward",mapForward,false,true);await eliminate("portable-group-reverse",mapReverse,true);
  const source:GroupReportingSource={asOf,from,slugs:ctx.slugs,authorId:checker.authorId,reviewerId:checker.id,invitationId:checker.invitationId,views:null,originals:retained,receipts,evidence:{}};
  source.views=await groupReportingViews(ctx.author,ctx.base,source);
  if(!isDeepStrictEqual(source.views.journals,posted)||source.views.report.status!=="ready"||source.views.report.appliedEliminations.length!==2||source.views.report.sourceSnapshots.length!==2||source.views.applied.rows.length!==2||source.views.reconciliation.rows.length!==2||source.views.reconciliation.rows.some((r:any)=>r.status!=="matched"))throw Error("Group report is not ready/exact or control-only lifecycle changed legal books");
  for(const proof of source.views.report.sourceSnapshots){const row=source.views.reconciliation.rows[0];const side=row.left.companySlug===proof.companySlug?row.left:row.right;if(proof.entryCount!==side.sourceSnapshot.entryCount||proof.ledgerHeadHash!==side.sourceSnapshot.ledgerHeadHash)throw Error("Group report and canonical reciprocal journal proof disagree");}
  const current=source.views.current[left];if(current.profiles.profiles.filter((p:any)=>p.status==="approved").length!==1||current.profiles.profiles.filter((p:any)=>p.status==="revoked").length!==1||current.profiles.profiles.filter((p:any)=>p.status==="proposed").length!==1||!current.eliminations.rows.some((e:any)=>e.status==="reversed"))throw Error("Explicit profile/reversed elimination histories not reflected in canonical source");
  if(!isDeepStrictEqual(await b.get(`/api/group-consolidated-report?${new URLSearchParams({profileId,from,asOf})}`),source.views.report))throw Error("Independent workspace owner does not see exact ready report");
  source.evidence={ordinaryDistinctInvitedWorkspaceOwnerMfa:true,authorId:source.authorId,reviewerId:source.reviewerId,explicitCompanyOwners:ctx.slugs,asOf,from,sourceViewsSha256:digest(Buffer.from(JSON.stringify(source.views))),readyReport:source.views.report,companyUuids:Object.fromEntries(ctx.slugs.map(s=>[s,source.views.journals[s].journal.ledgerUuid])),manifestEvents:2,mappingEvents:receipts.mappings.length,profileEvents:receipts.profiles.length,eliminationEvents:receipts.eliminations.length,appliedOrientations:2,reversedHistory:true,sourceOriginals:retained.map(r=>({company:r.company,id:r.id,sha256:r.sha256})),legacyEliminationCoverage:"Codec unit tests only; current native preparation creates V2",historySurface:"Exact signed native control rows compared to ordinary action receipts; HTTP exposes current state/report",groupControlAccountingEffect:"none; four explicit source journals prepared separately"};return source;
 }finally{await checker.page.context().close();}
}
export async function checkBackupGroupReporting(author:Page,base:string,source:GroupReportingSource,afterIntent=false){
 const actual=await groupReportingViews(author,base,source);
 if(afterIntent){
  if(!isDeepStrictEqual(actual.current,source.views.current)||!isDeepStrictEqual(actual.applied,source.views.applied)||!isDeepStrictEqual(actual.available,source.views.available)||actual.report.status!=="blocked"||actual.report.consolidatedFigures!==null||!actual.report.blockers.some((b:string)=>b.includes("applied elimination source snapshot or orientation changed")))throw Error("Deliberate new journal did not retain metadata and invalidate bound group proof honestly");
  return actual.report;
 }
 if(!isDeepStrictEqual(actual,source.views))throw Error("Source-ready group current/report/original/journal provenance changed after restore/restart");
 const a=api(author,base),members=await a.get("/api/workspace/members"),me=await a.get("/api/me");if(members.members.length!==1||members.members[0].userId!==me.user.id||[source.authorId,source.reviewerId].includes(me.user.id))throw Error("Historical group principals imported as live authority");
 const old=await author.request.post(`${base}/api/auth/sign-in/email`,{headers:{origin:base,"content-type":"application/json"},data:{email:reviewerEmail,password:reviewerPassword}});if(old.ok())throw Error("Historical group reviewer credentials imported");
 return{status:"ready",exactSourceViews:true,noHistoricalAuthority:true};
}
