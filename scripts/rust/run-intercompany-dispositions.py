#!/usr/bin/env python3
"""Owned two-MFA intercompany disposition evidence lifecycle; supplied artifacts only.

Separately posted real company sources, retained Corporate originals, independent
review, explicit current-head links and evidence-only settlement. Signed V2 backup,
source-free fresh-owner restore and reopen are required, never silently skipped.
No offline identity/business seeds, external mail or financial lifecycle writes.
"""
import argparse
import base64
import hashlib
import http.client
import http.server
import json
import os
import re
import secrets
import selectors
import shutil
import signal
import stat
import socket
import ssl
import subprocess
import threading
import time
import uuid
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
PREFIX = 'Rentemester-Synthetic-Dispositions-'
BROWSER = r'''


import {createHmac,createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {createInterface} from 'node:readline';
import {readFileSync,readdirSync,writeFileSync,unlinkSync} from 'node:fs';
import {join} from 'node:path';
import {isDeepStrictEqual} from 'node:util';
const {chromium}=await import(process.env.RM_DISPOSITIONS_PLAYWRIGHT);
const base=process.env.RM_DISPOSITIONS_BASE,work=process.env.RM_DISPOSITIONS_WORK;
if(!/^http:\/\/127\.0\.0\.1:\d+$/.test(base))throw Error('owned literal loopback origin required');
const owner={email:'owner@example.invalid',password:'synthetic-owner-password-123',codes:[]},member={email:'invited@example.invalid',password:'synthetic-invited-password-456',codes:[]};


let responseSequence=0;
function emit(value){const bytes=Buffer.from(JSON.stringify(value)),name=`browser-response-${String(++responseSequence).padStart(4,'0')}.json`;writeFileSync(join(work,name),bytes,{mode:0o600,flag:'wx'});console.log(JSON.stringify({sequence:responseSequence,bytes:bytes.length,sha256:createHash('sha256').update(bytes).digest('hex')}));}
let browser,ownerPage,memberPage,step='boot',detail=null,baseline,ownerId,memberId,invitationId,lastLogin=0;const trace=[];let diagnosticPage;
const need=(v,s)=>{if(!v){detail=s;throw Error(s);}};
function totp(secret){let bits=0,count=0,bytes=[];for(const c of secret){const v='ABCDEFGHIJKLMNOPQRSTUVWXYZ234567'.indexOf(c);need(v>=0,'MFA setup key syntax');bits=(bits<<5)|v;count+=5;if(count>=8){count-=8;bytes.push((bits>>>count)&255);}}const counter=Buffer.alloc(8);counter.writeBigUInt64BE(BigInt(Math.floor(Date.now()/30000)));const d=createHmac('sha1',Buffer.from(bytes)).update(counter).digest(),o=d[19]&15;return String((d.readUInt32BE(o)&0x7fffffff)%1000000).padStart(6,'0');}
async function cooldown(){const wait=11000-(Date.now()-lastLogin);if(wait>0)await new Promise(r=>setTimeout(r,wait));lastLogin=Date.now();}
async function fetchJson(page,path,data){return page.evaluate(async({path,data})=>{const r=await fetch(path,data===undefined?{}:{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify(data)});let body;try{body=await r.json();}catch{body=null;}return{status:r.status,body};},{path,data});}
async function get(page,path){const r=await fetchJson(page,path);need(r.status===200,`canonical ${path} HTTP${r.status}`);return r.body;}
function mails(kind,to){return readdirSync(join(work,'mail')).filter(n=>n.endsWith('.json')).map(n=>JSON.parse(readFileSync(join(work,'mail',n),'utf8'))).filter(m=>m.kind===kind&&m.to===to);}
async function mail(kind,to,previous=0){for(let i=0;i<150;i++){const found=mails(kind,to);if(found.length===previous+1){const m=found[previous];need(new URL(m.actionUrl).origin===base,'mail action canonical loopback origin');return m;}need(found.length<=previous+1,'unexpected additional mail token');await new Promise(r=>setTimeout(r,100));}throw Error('bounded private gateway receipt wait expired');}
async function login(page,user,password=user.password){await cooldown();await page.goto(base);await page.getByLabel('E-mail',{exact:true}).fill(user.email);await page.getByLabel('Adgangskode',{exact:true}).fill(password);await page.getByRole('button',{name:'Log ind',exact:true}).click();await page.getByRole('button',{name:'Brug recovery code',exact:true}).click();need(user.codes.length,'unused existing MFA recovery code');await page.getByLabel('Recovery code',{exact:true}).fill(user.codes.shift());await page.getByRole('button',{name:'Bekræft',exact:true}).click();await page.locator('a[href="/companies/synthetic/posteringer"]').waitFor();const me=await get(page,'/api/me');need(me.user.email===user.email&&me.user.twoFactorEnabled,'normal current verified MFA identity');const cookies=await page.context().cookies(base);need(cookies.some(c=>c.httpOnly),'ordinary HttpOnly session cookie');return me;}
async function enroll(page,user){const label=user===owner?'owner':'invited';step=label+'-enroll-login';await cooldown();await page.goto(base);await page.getByLabel('E-mail',{exact:true}).fill(user.email);await page.getByLabel('Adgangskode',{exact:true}).fill(user.password);await page.getByRole('button',{name:'Log ind',exact:true}).click();step=label+'-enroll-heading';await page.getByRole('heading',{name:'Opsæt totrinsbekræftelse',exact:true}).waitFor();await page.getByLabel('Adgangskode',{exact:true}).fill(user.password);await page.getByRole('button',{name:'Start opsætning',exact:true}).click();step=label+'-enroll-setup-key';const key=await page.getByLabel('Opsætningsnøgle',{exact:true}).inputValue();await page.getByLabel('Engangskode',{exact:true}).fill(totp(key));await page.getByRole('button',{name:'Bekræft opsætning',exact:true}).click();step=label+'-enroll-recovery-inventory';const codes=page.getByRole('region',{name:'Recovery codes'}).locator('li');await codes.first().waitFor();user.codes=await codes.allTextContents();need(user.codes.length>=4,'existing MFA recovery inventory');await page.getByRole('button',{name:'Jeg har gemt koderne',exact:true}).click();await page.locator('a[href="/companies/synthetic/posteringer"]').waitFor();step=label+'-enroll-acknowledgement';await page.context().clearCookies();return login(page,user);}
async function newPage(){const context=await browser.newContext();const page=await context.newPage();page.setDefaultTimeout(45000);diagnosticPage=page;page.on('request',r=>{if(new URL(r.url()).pathname==='/api/auth/request-password-reset')trace.push({kind:'request',path:'/api/auth/request-password-reset',method:r.method()});});page.on('requestfailed',r=>{if(new URL(r.url()).pathname==='/api/auth/request-password-reset')trace.push({kind:'requestfailed',path:'/api/auth/request-password-reset',code:r.failure()?.errorText?.match(/net::[A-Z_]+/)?.[0]});});page.on('response',r=>{const path=new URL(r.url()).pathname;if(path.startsWith('/api/auth/')){trace.push({path:path.startsWith('/api/auth/reset-password/')?'/api/auth/reset-password/:token':path,status:r.status()});if(trace.length>24)trace.shift();}});return page;}
async function authenticate(){
 browser=await chromium.launch({headless:true});ownerPage=await newPage();memberPage=await newPage();
 step='owner-mail-verification';await ownerPage.goto((await mail('verification',owner.email)).actionUrl);const who=await enroll(ownerPage,owner);ownerId=who.user.id;
 step='ordinary-web-invitation';await ownerPage.goto(base+'/adgang');await ownerPage.getByRole('heading',{name:'Brugere og adgang',exact:true}).waitFor();await ownerPage.getByLabel('E-mail',{exact:true}).fill(member.email);await ownerPage.getByLabel(/^Rolle/).selectOption('reviewer');await ownerPage.getByRole('button',{name:'Send invitation',exact:true}).click();await ownerPage.getByText('Invitationen er sendt.',{exact:true}).waitFor();const invitation=await mail('workspace-invitation',member.email);const listed=await get(ownerPage,'/api/workspace/invitations');const item=listed.invitations.find(i=>i.email===member.email);need(item?.status==='delivery_confirmed','canonical invitation delivery');invitationId=item.invitationId;
 step='normal-invited-claim';await memberPage.goto(invitation.actionUrl);await memberPage.getByRole('heading',{name:'Acceptér invitation',exact:true}).waitFor();need(!new URL(memberPage.url()).hash,'invitation token removed from address');await memberPage.getByLabel('Navn',{exact:true}).fill('Synthetic Invited User');await memberPage.getByLabel('Adgangskode',{exact:true}).fill(member.password);const claim=memberPage.waitForResponse(r=>new URL(r.url()).pathname==='/api/invitations/claim'&&r.request().method()==='POST');await memberPage.getByRole('button',{name:'Acceptér invitation',exact:true}).click();const claimed=await claim;need(claimed.ok()&&!claimed.headers()['set-cookie'],'claim cannot mint session');await memberPage.getByText('Invitationen er accepteret.',{exact:true}).waitFor();need((await fetchJson(memberPage,'/api/companies/synthetic/trial-balance')).status===401,'claim not sufficient for business read');
 step='invited-mail-verification-existing-mfa';await memberPage.goto((await mail('verification',member.email)).actionUrl);const mwho=await enroll(memberPage,member);memberId=mwho.user.id;need(memberId!==ownerId,'independent ordinary users');baseline=await get(memberPage,'/api/companies/synthetic/trial-balance');
 return {ownerId,memberId,invitationId,ordinaryInvitationClaim:true,distinctNormalMfaUsers:true};
}
const left='synthetic-dispositions-left',right='synthetic-dispositions-right',slugs=[left,right];
const prefix=slug=>`/api/companies/${slug}/`,endpoint=action=>prefix(left)+'group-dispositions/'+action,hash=b=>createHash('sha256').update(b).digest('hex');
let economicDate,year,partyId,agreementId,ackId,sourceBooks,sourceOriginals=[],journalReceipts={},planned,primary,secondary,replacement,stable,freshOwner;
async function call(page,path,body){const r=await page.request.post(base+path,{headers:{origin:base,'content-type':'application/json'},data:body});return{status:r.status(),value:await r.json()};}
async function post(path,body,page=ownerPage){const r=await call(page,path,body);need(r.status>=200&&r.status<300&&r.value?.ok!==false,`canonical ${path} HTTP${r.status} ${r.value?.code??''}`);return r.value;}
async function read(path,page=ownerPage){return get(page,path);}
async function status(id,page=ownerPage){return read('/api/group-dispositions/'+encodeURIComponent(id)+'?asOf='+economicDate,page);}
const identity=s=>({dispositionId:s.disposition.dispositionId,payloadHash:s.payloadHash});
async function books(){const result={};for(const slug of slugs){const p=prefix(slug),q='?year='+year;result[slug]={journal:await read(p+'journal'+q),trialBalance:await read(p+'trial-balance'+q),balance:await read(p+'balance'+q)};}return result;}
async function unchanged(){need(isDeepStrictEqual(await books(),sourceBooks),'every lifecycle action leaves both complete journals/UUIDs/trial balances/balance sheets unchanged');for(const original of sourceOriginals){const record=await read(prefix(left)+`corporate-records/${original.id}`);const downloaded=await read(prefix(left)+`corporate-records/${original.id}/download`);need(isDeepStrictEqual(record,original.record)&&Buffer.from(downloaded.bytesBase64,'base64').equals(Buffer.from(original.bytes))&&record.record.sha256===hash(Buffer.from(original.bytes)),'corporate original bytes/SHA/metadata/history unchanged');}}
async function corporate(id,bytes){const created=await post(prefix(left)+'corporate-records',{recordId:id,type:'intercompany_agreement',bytesBase64:Buffer.from(bytes).toString('base64'),filename:id+'.txt',source:'Synthetic ordinary disposition evidence',receivedAt:new Date().toISOString(),uploader:'Synthetic verified owner',sensitivity:'normal',links:[{type:'company',id:left},{type:'company',id:right},{type:'party',id:partyId}],confirm:true});need(created.record.recordId===id,'caller-selected exact retained record identity');sourceOriginals.push({id,bytes,record:await read(prefix(left)+'corporate-records/'+id)});return id;}
async function setup(){step='explicit-current-owner-grants-and-source-companies';await post('/api/workspace/members/access',{userId:memberId,action:'set-role',workspaceRole:'workspace_owner'});for(const slug of slugs){await post('/api/companies',{name:'Synthetic disposition '+slug,slug,fiscalYearStartMonth:'1',vatPeriodType:null});for(const userId of[ownerId,memberId])await post('/api/workspace/members/company',{userId,action:'grant',companySlug:slug,role:'owner'});}
 economicDate=(await read(prefix(left)+'scheduled-recurring')).scheduledRecurring.asOfDate;need(/^\d{4}-\d{2}-\d{2}$/.test(economicDate),'trusted server Copenhagen date');year=economicDate.slice(0,4);
 step='canonical-related-party-and-real-corporate-agreement';partyId=(await post(prefix(left)+'workspace-parties',{kind:'organization',name:'Synthetic disposition counterparty',source:'Synthetic retained agreement',observedAt:new Date().toISOString(),reviewAssertion:'Explicit synthetic source reviewed',role:'related_company',confirm:true})).party.partyId;await post(prefix(right)+`workspace-parties/${partyId}/role`,{role:'related_company',confirm:true});agreementId=await corporate('synthetic-disposition-agreement','Synthetic loan agreement: separately held company ledgers; DKK 12.34. Evidence only.\n');
 step='separate-ordinary-real-journals-before-any-links';for(const slug of slugs){const isLeft=slug===left,accountNo=isLeft?'1851':'2851';for(const [n,type,normalBalance]of[[accountNo,isLeft?'asset':'liability',isLeft?'debit':'credit'],['8851','equity','credit']])await post(prefix(slug)+'accounts',{accountNo:n,name:'Synthetic disposition '+n,type,normalBalance});const journal=(await read(prefix(slug)+'journal')).journal;need(journal.entries.length===0,'new owned company starts with no journal');journalReceipts[slug]=await post(prefix(slug)+'manual-journal',{confirm:true,input:{format:'rentemester-manual-journal-post/1',expectedRuleVersion:journal.nativePosting.ruleVersion,idempotencyKey:'synthetic-disposition-source-'+slug,request:{transactionDate:economicDate,text:'Synthetic separately reviewed disposition source',currency:'DKK',lines:[{accountNo,debitAmount:isLeft?'12.34':'0.00',creditAmount:isLeft?'0.00':'12.34'},{accountNo:'8851',debitAmount:isLeft?'0.00':'12.34',creditAmount:isLeft?'12.34':'0.00'}]}}});need(Number.isSafeInteger(journalReceipts[slug].entryId)&&/^[0-9a-f]{64}$/.test(journalReceipts[slug].entryHash),'genuine source posting receipt identity/hash');}
 sourceBooks=await books();need(slugs.every(slug=>/^[0-9a-f]{32}$/.test(sourceBooks[slug].journal.journal.ledgerUuid))&&sourceBooks[left].journal.journal.ledgerUuid!==sourceBooks[right].journal.journal.ledgerUuid,'separate stable legal entity ledger UUIDs');for(const slug of slugs){need(sourceBooks[slug].journal.journal.entries.length===1&&sourceBooks[slug].trialBalance.trialBalance.balanced===true,'exactly one balanced original posting on each ledger');}await unchanged();return{companySlugs:slugs,ownerId,memberId,economicDate,partyId,agreementId,journalReceipts,sourceBooks,sourceOriginals,reviewerRoleSpecificActualQualified:false};}
async function pane(page){diagnosticPage=page;await page.goto(base+'/koncernstruktur');const region=page.getByRole('region',{name:'Intercompany dispositioner',exact:true});await region.getByRole('heading',{name:'Intercompany dispositioner',exact:true}).waitFor();return region;}
async function currentPane(page,id){const r=await pane(page);await r.getByLabel('Disposition-ID',{exact:true}).fill(id);await r.getByLabel('Status pr. dato',{exact:true}).fill(economicDate);const response=page.waitForResponse(x=>new URL(x.url()).pathname==='/api/group-dispositions/'+id&&x.request().method()==='GET');await r.getByRole('button',{name:'Vis status',exact:true}).click();need((await response).status()===200,'visible exact-ID current status loaded');return r;}
async function confirm(page,region,button,dialog,final,action){const response=page.waitForResponse(r=>new URL(r.url()).pathname===endpoint(action)&&r.request().method()==='POST');await region.getByRole('button',{name:button,exact:true}).click();const d=page.getByRole('dialog',{name:dialog,exact:true});await d.getByRole('checkbox',{name:'Jeg bekræfter den gennemgåede handling',exact:true}).check();await d.getByRole('button',{name:final,exact:true}).click();const r=await response;need(r.status()===200,`reviewed UI ${action} HTTP${r.status()}`);await d.waitFor({state:'hidden'});await unchanged();return r.json();}
async function readonlyPlan(){step='actual-ui-readonly-plan-adopts-server-generated-id';const r=await pane(ownerPage);await r.getByText('Opret ny disposition',{exact:true}).click();for(const[label,value]of[['Dispositionstype','loan'],['Venstre selskab',left],['Højre selskab',right],['Venstre side','receivable'],['Højre side','payable']])await r.getByLabel(label,{exact:true}).selectOption(value);for(const[label,value]of[['Økonomisk dato',economicDate],['Beløb','12.34'],['Valuta','DKK'],['Venstre rolle','lender'],['Højre rolle','borrower']])await r.getByLabel(label,{exact:true}).fill(value);await r.getByLabel('Parter',{exact:true}).selectOption([partyId]);await r.getByLabel('Dokumentation',{exact:true}).selectOption([agreementId]);need(await r.getByLabel('Nyt dispositions-ID (valgfrit)',{exact:true}).inputValue()==='','explicit plan ID omission');const response=ownerPage.waitForResponse(x=>new URL(x.url()).pathname===endpoint('plan')&&x.request().method()==='POST');await r.getByRole('button',{name:'Opret plan',exact:true}).click();const result=await response;need(result.status()===200,'readonly UI plan response');planned=await result.json();need(planned.ok===true&&planned.mode==='dry-run'&&planned.ledgerEffects.length===0&&planned.disposition.dispositionId&&/^[0-9a-f]{64}$/.test(planned.payloadHash),'server normalized ID/hash and zero effects');for(const [side,slug]of[['left',left],['right',right]]){const rows=planned.journalCandidates[side],receipt=journalReceipts[slug];need(rows.length===1&&rows[0].journalEntryId===receipt.entryId&&rows[0].journalEntryHash===receipt.entryHash&&rows[0].ledgerHeadHash===receipt.entryHash&&rows[0].amount===12.34&&rows[0].transactionDate===economicDate&&rows[0].expectedSide===(side==='left'?'receivable':'payable'),'both candidates actual existing source IDs/hash/current heads/amounts');}await unchanged();primary=await confirm(ownerPage,r,'Foreslå disposition','Bekræft disposition','Gem disposition','propose');need(primary.status==='proposed'&&primary.disposition.dispositionId===planned.disposition.dispositionId&&primary.payloadHash===planned.payloadHash,'proposal preserves reviewed normalized ID/hash');return{planned,primary};}
async function refusal(page,action,body,expected=422){const before=await status(body.dispositionId);const r=await call(page,endpoint(action),body);need(r.status===expected&&r.value?.ok===false,`source ${action} refusal HTTP${r.status}`);need(isDeepStrictEqual(await status(body.dispositionId),before),'refusal leaves all exact status/history unchanged');await unchanged();return{status:r.status,code:r.value.code};}
async function approval(){step='self-review-refused-distinct-current-human-approves';need(await ownerPage.getByRole('region',{name:'Intercompany dispositioner',exact:true}).getByRole('button',{name:'Godkend disposition',exact:true}).isDisabled(),'product self-review control disabled');const self=await refusal(ownerPage,'approve',{...identity(primary),confirm:true});const region=await currentPane(memberPage,primary.disposition.dispositionId);primary=await confirm(memberPage,region,'Godkend disposition','Bekræft godkendelse','Godkend præcis disposition','approve');need(primary.status==='approved'&&primary.events.some(e=>e.event_type==='proposed'&&e.principal_kind==='user'&&e.principal_id===ownerId&&e.actor==='user:'+ownerId)&&primary.events.some(e=>e.event_type==='approved'&&e.principal_kind==='user'&&e.principal_id===memberId&&e.actor==='user:'+memberId),'distinct authenticated reviewer and original author immutable attribution');const before=await status(primary.disposition.dispositionId);await post(endpoint('approve'),{...identity(primary),confirm:true},memberPage);need(isDeepStrictEqual(before,await status(primary.disposition.dispositionId)),'supported exact duplicate approval does not append history');await unchanged();return{self,approved:primary};}
async function links(){step='left-link-one-sided-current-source-proof';let r=await currentPane(ownerPage,primary.disposition.dispositionId);await r.getByLabel('Venstre postering',{exact:true}).selectOption(String(journalReceipts[left].entryId));primary=await confirm(ownerPage,r,'Tilknyt venstre postering','Bekræft venstre postering','Gem venstre tilknytning','link');let current=await status(primary.disposition.dispositionId);need(current.status==='partly_posted'&&current.balanced===false&&current.exceptions.some(e=>e.kind==='one_sided_posting'&&e.waivable===false),'left-only truthful nonwaivable exception');const leftLink={...identity(primary),side:'left',journalEntryId:journalReceipts[left].entryId,ledgerHeadHash:journalReceipts[left].entryHash,confirm:true};await post(endpoint('link'),leftLink);need(isDeepStrictEqual(current,await status(primary.disposition.dispositionId)),'supported partly-posted same-side repeat adds no history');await unchanged();step='wrong-right-head-refusal';const rightLink={...identity(primary),side:'right',journalEntryId:journalReceipts[right].entryId,ledgerHeadHash:journalReceipts[right].entryHash,confirm:true};await refusal(ownerPage,'link',{...rightLink,ledgerHeadHash:'0'.repeat(64)});await refusal(ownerPage,'link',{...rightLink,payloadHash:'0'.repeat(64)});step='right-link-genuine-two-ledger-balanced-status';r=await currentPane(ownerPage,primary.disposition.dispositionId);await r.getByLabel('Højre postering',{exact:true}).selectOption(String(journalReceipts[right].entryId));primary=await confirm(ownerPage,r,'Tilknyt højre postering','Bekræft højre postering','Gem højre tilknytning','link');current=await status(primary.disposition.dispositionId);need(current.status==='posted'&&current.balanced===true&&current.exceptions.length===0&&current.links.length===2,'real posted balanced both-source proof');for(const link of current.links){const slug=link.side==='left'?left:right;need(link.company_slug===slug&&link.journal_entry_id===journalReceipts[slug].entryId&&link.journal_entry_hash===journalReceipts[slug].entryHash&&link.ledger_head_hash===journalReceipts[slug].entryHash&&link.principal_id===ownerId,'exact admitted source link and actual actor');}await refusal(ownerPage,'link',rightLink);return{posted:current,supportedDuplicateWhilePartlyPosted:true,postedLinkRetryRefused:true};}
async function settlement(){step='actual-separate-retained-settlement-evidence-no-payment';ackId=await corporate('synthetic-disposition-settlement','Synthetic parties acknowledge disposition evidence. This record is not proof of cash transfer.\n');await unchanged();const r=await currentPane(ownerPage,primary.disposition.dispositionId);await r.getByLabel('Afviklingsdokumentation',{exact:true}).selectOption([ackId]);primary=await confirm(ownerPage,r,'Registrér afviklingsevidens','Bekræft afviklingsevidens','Gem afviklingsevidens','settle');const settled=await status(primary.disposition.dispositionId);need(settled.status==='settled'&&settled.balanced===false&&settled.exceptions.length===0&&settled.links.length===2,'settled evidence status is not balanced posted status');await post(endpoint('settle'),{...identity(primary),settlementEvidenceRecordIds:[ackId],confirm:true});need(isDeepStrictEqual(settled,await status(primary.disposition.dispositionId)),'supported settlement duplicate exact unchanged history');await unchanged();await r.getByText('Fuld evidens og historik',{exact:true}).click();await ownerPage.screenshot({path:process.env.RM_DISPOSITIONS_SCREENSHOT,fullPage:true});return{settled,settlementEvidenceOnly:true,noCashTransferVerified:true};}
async function secondaryLifecycle(){step='separate-unlinked-reasoned-supersession-and-reopen';for(const id of['synthetic-disposition-secondary','synthetic-disposition-replacement']){const p=await post(endpoint('plan'),{disposition:{...planned.disposition,dispositionId:id}});const s=await post(endpoint('propose'),{disposition:p.disposition,confirm:true});if(id.endsWith('secondary'))secondary=s;else replacement=s;await unchanged();}secondary=await post(endpoint('approve'),{...identity(secondary),confirm:true},memberPage);await unchanged();let r=await currentPane(ownerPage,secondary.disposition.dispositionId);await r.getByLabel('Erstatningsdisposition-ID',{exact:true}).fill(replacement.disposition.dispositionId);await r.getByLabel('Begrundelse',{exact:true}).fill('Synthetic independently reviewed replacement preserves original history');secondary=await confirm(ownerPage,r,'Erstat disposition','Bekræft erstatning','Gem erstatning','supersede');need(secondary.status==='superseded','reasoned supersession');await refusal(ownerPage,'supersede',{...identity(secondary),replacementDispositionId:replacement.disposition.dispositionId,reason:'Synthetic repeat must refuse unchanged state',confirm:true});r=await currentPane(ownerPage,secondary.disposition.dispositionId);await r.getByLabel('Begrundelse',{exact:true}).fill('Synthetic explicit reopening of retained source history');secondary=await confirm(ownerPage,r,'Genåbn disposition','Bekræft genåbning','Genåbn præcis disposition','reopen');need(secondary.status==='approved'&&secondary.links.length===0,'reopen folds retained approved unlinked state');await refusal(ownerPage,'reopen',{...identity(secondary),reason:'Synthetic repeated reopen must refuse unchanged state',confirm:true});await unchanged();return{secondary:await status(secondary.disposition.dispositionId),replacement:await status(replacement.disposition.dispositionId)};}
async function client(kind,id){const file=join(work,'client.cookie');writeFileSync(file,(await ownerPage.context().cookies(base)).map(c=>`${c.name}=${c.value}`).join('; '),{mode:0o600,flag:'wx'});try{const common=['--server',base,'--cookie-file',file,'--allow-loopback-http'];const input=kind==='mcp'?[{jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'synthetic-disposition-proof',version:'1'}}},{jsonrpc:'2.0',method:'notifications/initialized'},{jsonrpc:'2.0',id:2,method:'tools/call',params:{name:'group_disposition_status',arguments:{id,asOf:economicDate}}}].map(v=>JSON.stringify(v)).join('\n')+'\n':undefined;const argv=kind==='cli'?['remote','group-disposition-status',...common,'--id',id,'--as-of',economicDate]:common;const out=spawnSync(join(work,kind==='cli'?'rentemester-rust':'rentemester-mcp'),argv,{input,encoding:'utf8',timeout:45000,maxBuffer:32*1024*1024});need(!out.error&&out.status===0,'native readonly client terminal');const result=kind==='cli'?JSON.parse(out.stdout):out.stdout.trim().split('\n').map(l=>JSON.parse(l)).find(v=>v.id===2)?.result?.structuredContent;need(result?.ok===true&&result.data,'native full canonical envelope');return result.data;}finally{unlinkSync(file);}}
async function snapshot(){const states={};for(const s of[primary,secondary,replacement])states[s.disposition.dispositionId]=await status(s.disposition.dispositionId);return{states,sourceBooks,sourceOriginals,party:await read(prefix(left)+`workspace-parties/${partyId}`)};}
async function capture(){step='complete-canonical-source-and-clients-snapshot';await unchanged();stable=await snapshot();for(const kind of['cli','mcp'])for(const[id,s]of Object.entries(stable.states))need(isDeepStrictEqual(await client(kind,id),s),'complete current HTTP CLI MCP status parity');await unchanged();return{stable,httpCliMcpStatusParity:true,financialWritesOnlySeparateInitialSourcePosts:true,permissionRevocationAndReversedSourceActualQualified:false};}
async function assertStable(){await unchanged();need(isDeepStrictEqual(await snapshot(),stable),'exact full disposition histories/links/status/exceptions/UUID/Party/Corporate originals persist');}
async function reopen(){step='restart-both-normal-users-fresh-password-and-mfa';for(const[page,user,id]of[[ownerPage,owner,ownerId],[memberPage,member,memberId]]){await page.context().clearCookies();need((await login(page,user)).user.id===id,'same current distinct MFA user on reopen');}await assertStable();for(const[id,s]of Object.entries(stable.states)){need(isDeepStrictEqual(await status(id,memberPage),s),'independent current reviewer reads exact retained state');for(const kind of['cli','mcp'])need(isDeepStrictEqual(await client(kind,id),s),'restart full canonical client parity');}await currentPane(ownerPage,primary.disposition.dispositionId);return{freshTwoMfaRestart:true,fullCanonicalHistoriesUnchanged:true,noActionsReplayed:true};}
async function archive(){step='explicit-signed-nonempty-intercompany-v2-snapshot';const response=await ownerPage.request.post(base+'/api/workspace/backup',{headers:{origin:base},data:{confirm:true}});need(response.status()===200&&response.headers()['content-type']==='application/vnd.rentemester.native-backup+json','nonempty signed disposition coverage required; refusal remains blocker');const bytes=await response.body();need(bytes.length>0&&bytes.length<=96*1024*1024,'bounded complete signed artifact');const wire=JSON.parse(bytes.toString('utf8'));const signed=JSON.parse(Buffer.from(wire.manifestBase64,'base64').toString('utf8'));for(const slug of slugs)need(signed.context.companies.find(c=>c.slug===slug)?.ledgerUuid===sourceBooks[slug].journal.journal.ledgerUuid,'signed manifest binds exact admitted company ledger UUID');const entry=wire.entries.find(e=>e.path==='control/intercompany.json');need(entry,'signed intercompany section required');const section=JSON.parse(Buffer.from(entry.bytesBase64,'base64').toString('utf8'));need(section.format==='rentemester-native-intercompany/2','actual signed Intercompany version2 required');
 const decode=name=>{const table=section.tables[name];need(table&&Array.isArray(table.columns)&&Array.isArray(table.rows),'complete signed disposition table '+name);return table.rows.map(row=>{need(row.length===table.columns.length,'complete exact signed row');return Object.fromEntries(row.map((cell,i)=>{need(['Null','Integer','Text'].includes(cell.type),'typed signed cell');return[table.columns[i],cell.type==='Null'?null:cell.value];}));});};
 const dispositions=decode('rm_intercompany_dispositions');need(dispositions.length===Object.keys(stable.states).length,'all signed disposition identities retained');
 for(const row of dispositions){const state=stable.states[row.disposition_id];need(state&&isDeepStrictEqual(JSON.parse(row.canonical_payload),state.disposition)&&row.payload_hash===state.payloadHash&&hash(Buffer.from(row.canonical_payload))===state.payloadHash,'signed original canonical disposition payload/hash retained');}
 for(const[table,key]of[['rm_intercompany_disposition_events','events'],['rm_intercompany_disposition_lifecycle_events','lifecycleEvents'],['rm_intercompany_disposition_journal_links','links']]){const expected=Object.values(stable.states).flatMap(state=>state[key]).sort((a,b)=>a.id-b.id);need(expected.length>0&&isDeepStrictEqual(decode(table),expected),'complete signed original global chains/links/principals '+table);}
writeFileSync(join(work,'dispositions.native-backup.json'),bytes,{mode:0o600,flag:'wx'});await assertStable();return{supported:true,sha256:hash(bytes),bytes:bytes.length,intercompanyVersion:2,sourceFreeRestoreQualified:false};}
async function restored(){step='source-free-fresh-owner-normal-email-verification-and-mfa';ownerPage=await newPage();freshOwner={email:'restored@example.invalid',password:'synthetic-restored-password-789',codes:[]};await ownerPage.goto((await mail('verification',freshOwner.email)).actionUrl);const me=await enroll(ownerPage,freshOwner);need(me.user.id!==ownerId&&me.user.id!==memberId,'historical principals never imported as current authority');for(const slug of slugs)need(me.companies.some(c=>c.slug===slug&&c.role==='owner'),'fresh owner current explicit restored company access');await assertStable();await currentPane(ownerPage,primary.disposition.dispositionId);return{sourceFreeRestoreQualified:true,freshOwnerMfa:true,originalAndFullHistoriesAndCompanyUuidsPreserved:true,noImportedHistoricalAuthority:true};}
async function restoredReopen(){step='restored-target-reopen-new-owner-fresh-mfa-no-actions';await ownerPage.context().clearCookies();await login(ownerPage,freshOwner);await assertStable();await currentPane(ownerPage,primary.disposition.dispositionId);return{restoredTargetReopenQualified:true,fullHistoricalStateUnchanged:true,noFinancialOrLifecycleWrites:true};}
for await(const line of createInterface({input:process.stdin,crlfDelay:Infinity})){try{const action=JSON.parse(line);let value;if(action==='authenticate')value=await authenticate();else if(action==='setup')value=await setup();else if(action==='plan-propose')value=await readonlyPlan();else if(action==='approve')value=await approval();else if(action==='links')value=await links();else if(action==='settlement')value=await settlement();else if(action==='secondary')value=await secondaryLifecycle();else if(action==='capture')value=await capture();else if(action==='reopen')value=await reopen();else if(action==='archive')value=await archive();else if(action==='restored')value=await restored();else if(action==='restored-reopen')value=await restoredReopen();else if(action==='finish'){await browser?.close();emit({ok:true,value:{finished:true}});break;}else throw Error('unknown phase');emit({ok:true,value});}catch(error){let pageState=null;try{pageState=await Promise.race([diagnosticPage?.evaluate(()=>({pathname:location.pathname,headings:[...document.querySelectorAll('h1,h2')].map(e=>e.textContent.slice(0,100)),buttons:[...document.querySelectorAll('button')].map(e=>e.textContent.slice(0,100)),alerts:document.querySelectorAll('[role=alert]').length})),new Promise(resolve=>setTimeout(()=>resolve(null),2000))]);}catch{}emit({ok:false,step,detail,errorType:error?.name,trace,pageState});await Promise.race([browser?.close().catch(()=>{}),new Promise(resolve=>setTimeout(resolve,2000))]);process.exitCode=1;break;}}

'''


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def free_port():
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0))
        return sock.getsockname()[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for option in ['server-binary', 'web-dir', 'cli-binary', 'mcp-binary', 'evidence']:
        parser.add_argument('--' + option, required=True, type=Path)
    args = parser.parse_args()
    if args.evidence.exists() or args.evidence.with_suffix('.png').exists():
        parser.error('New evidence and screenshot paths required')
    if os.uname().sysname != 'Darwin' or os.getuid() == 0 or os.getuid() != os.geteuid():
        parser.error('Ordinary macOS fixture owner required')
    os.umask(0o077)
    parent = (Path.home() / 'Library/Application Support').resolve(strict=True)
    work = parent / (PREFIX + uuid.uuid4().hex)
    identity = None
    child = controller = None
    log = None
    servers = []
    gateway_rows, gateway_requests, errors = {}, [], []
    gateway_lock = threading.Lock()
    bearer = secrets.token_urlsafe(32)
    backend_port, mail_port = free_port(), free_port()
    if backend_port == mail_port:
        raise RuntimeError('Distinct owned ports required')
    origin = f'http://127.0.0.1:{backend_port}'
    report = {'status': 'running', 'sourceRevision': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=REPO, text=True).strip(), 'runnerSha256': digest(Path(__file__)), 'browserHelperSha256': hashlib.sha256(BROWSER.encode()).hexdigest(), 'canonicalOrigin': origin, 'noOfflineIdentitySeeds': True, 'ordinaryMacOsUser': True, 'ownedProfile': str(work), 'externalMailDelivered': False, 'workflow': 'intercompany_disposition_evidence_lifecycle', 'reviewerRoleSpecificActualQualified': False, 'revocationAndStaleSourceActualQualified': False, 'installedNativeQualified': False, 'screenshot': str(args.evidence.with_suffix('.png')),  'actions': [], 'results': {}, 'sourceFreeRestoreQualified': False}
    args.evidence.parent.mkdir(parents=True, exist_ok=True)
    with args.evidence.open('x') as evidence:
        def save():
            evidence.seek(0)
            evidence.truncate()
            json.dump(report, evidence, indent=2)
            evidence.flush()
            os.fsync(evidence.fileno())

        def progress(action):
            report['currentAction'] = action
            save()

        def owned():
            if identity is None or work.is_symlink() or not work.is_dir():
                return False
            stat = work.stat()
            return work.parent == parent and work.name.startswith(PREFIX) and (stat.st_dev, stat.st_ino, stat.st_uid) == (*identity, os.getuid())

        class QuietServer(http.server.ThreadingHTTPServer):
            daemon_threads = True

        class Gateway(http.server.BaseHTTPRequestHandler):
            def log_message(self, *_args):
                pass

            def do_POST(self):
                try:
                    length = int(self.headers.get('content-length', '0'))
                    if self.path != '/mail' or not 0 < length <= 32768 or self.headers.get('authorization') != 'Bearer ' + bearer:
                        self.send_error(403)
                        return
                    body = json.loads(self.rfile.read(length))
                    event = self.headers.get('x-rentemester-auth-event')
                    key = self.headers.get('idempotency-key', '')
                    if not re.fullmatch('[0-9a-f]{64}', key) or body.get('version') != 'rentemester-auth-email-v1' or body.get('kind') != event or event not in ('verification', 'workspace-invitation') or body.get('to') not in ('owner@example.invalid', 'invited@example.invalid', 'restored@example.invalid') or body.get('from') != 'auth@example.invalid' or not body.get('actionUrl', '').startswith(origin + '/'):
                        raise ValueError('Private synthetic gateway envelope refused')
                    with gateway_lock:
                        previous = gateway_rows.get(key)
                        if previous is not None and previous != body:
                            raise ValueError('Idempotency envelope changed')
                        gateway_requests.append({'kind': event, 'idempotencyHash': hashlib.sha256(key.encode()).hexdigest()})
                        if previous is None:
                            gateway_rows[key] = body
                            temporary = work / 'mail' / (str(len(gateway_rows)) + '.pending')
                            with temporary.open('x') as output:
                                json.dump(body, output)
                                output.flush()
                                os.fsync(output.fileno())
                            temporary.rename(temporary.with_suffix('.json'))
                    self.send_response(202)
                    self.send_header('content-length', '0')
                    self.end_headers()
                except (BrokenPipeError, ConnectionResetError):
                    pass
                except Exception:
                    errors.append('gateway-envelope-or-idempotency-refusal')
                    self.send_error(400)

        def write_config():
            if not owned():
                raise RuntimeError('Owned runtime identity changed')
            (work / 'server.json').write_text(json.dumps(config))

        def start(create=False, restore=False):
            nonlocal child, log
            if child is not None or not owned():
                raise RuntimeError('Exact owned child must stop before reopen')
            progress('operator:restore' if restore else 'operator:start')
            argv = [str(pinned['server_binary']), '--config', str(work / 'server.json'), '--web-dir', str(web), '--listen', f'127.0.0.1:{backend_port}', '--origin', origin]
            if create:
                argv.append('--create')
            if restore:
                root = Path(config['root'])
                if os.path.lexists(root):
                    raise RuntimeError('Fresh restore only; refuse overwrite')
                argv += ['--restore-backup', str(work / 'dispositions.native-backup.json'), '--trusted-backup-key', str(work / 'trusted.pub'), '--confirm-target', str(root), '--source-unavailable']
            log = (work / 'server.log').open('ab')
            child = subprocess.Popen(argv, cwd=REPO, stdin=subprocess.DEVNULL, stdout=log, stderr=log)
            report.setdefault('serverPids', []).append(child.pid)
            save()
            for _ in range(300):
                if child.poll() is not None:
                    raise RuntimeError('Owned server exited before readiness')
                connection = http.client.HTTPConnection('127.0.0.1', backend_port, timeout=.3)
                try:
                    connection.request('GET', '/api/health')
                    response = connection.getresponse()
                    if response.status == 200 and json.loads(response.read()).get('workspaceReady'):
                        return
                except (OSError, ValueError, http.client.HTTPException):
                    pass
                finally:
                    connection.close()
                time.sleep(.1)
            raise RuntimeError('Owned server readiness deadline')

        def stop():
            nonlocal child, log
            if child is not None:
                if child.poll() is None:
                    child.terminate()
                try:
                    child.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait(timeout=5)
                report.setdefault('serverExits', []).append({'pid': child.pid, 'code': child.returncode})
                child = None
            if log:
                log.close()
                log = None
            save()

        def phase(action, timeout=300):
            progress('browser:' + action)
            controller.stdin.write(json.dumps(action) + '\n')
            controller.stdin.flush()
            # stdout carries only a small envelope. Full source proofs can exceed
            # pipe capacity; store them completely before announcing the response.
            line = bytearray()
            deadline = time.monotonic() + timeout
            with selectors.DefaultSelector() as ready:
                ready.register(controller.stdout, selectors.EVENT_READ)
                while not line.endswith(b'\n'):
                    remaining = deadline - time.monotonic()
                    if remaining <= 0 or not ready.select(remaining):
                        raise RuntimeError('Bounded browser phase expired: ' + action)
                    chunk = os.read(controller.stdout.fileno(), 4096)
                    if not chunk:
                        raise RuntimeError('Controller ended without sanitized response')
                    line.extend(chunk)
                    if len(line) > 4096:
                        raise RuntimeError('Oversized browser response envelope')
            try:
                envelope = json.loads(line)
                sequence = len(report['actions']) + 1
                if envelope['sequence'] != sequence:
                    raise ValueError('response sequence mismatch')
                response = work / f'browser-response-{sequence:04d}.json'
                if not owned() or response.is_symlink():
                    raise ValueError('response namespace mismatch')
                info = response.stat()
                if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid() or not 0 < info.st_size <= 32 * 1024 * 1024:
                    raise ValueError('response file invalid')
                raw = response.read_bytes()
                if len(raw) != envelope['bytes'] or hashlib.sha256(raw).hexdigest() != envelope['sha256']:
                    raise ValueError('response digest mismatch')
                value = json.loads(raw)
            except (ValueError, KeyError, OSError):
                raise RuntimeError('Controller response envelope or proof file invalid') from None
            if not value.get('ok'):
                report['browserFailure'] = value
                save()
                raise RuntimeError('Browser assertion failed at ' + value.get('step', action))
            report['actions'].append({'action': action, 'status': 'complete'})
            report['results'][action] = value['value']
            save()
            return value['value']

        try:
            work.mkdir(mode=0o700)
            identity = (work.stat().st_dev, work.stat().st_ino)
            (work / 'mail').mkdir(mode=0o700)
            pinned = {}
            for field, name in [('server_binary', 'rentemester-server'), ('cli_binary', 'rentemester-rust'), ('mcp_binary', 'rentemester-mcp')]:
                path = work / name
                shutil.copyfile(getattr(args, field).resolve(strict=True), path)
                path.chmod(0o700)
                pinned[field] = path
            web = work / 'web'
            shutil.copytree(args.web_dir.resolve(strict=True), web)
            report['artifacts'] = {name: digest(path) for name, path in pinned.items()}
            report['webFiles'] = {str(p.relative_to(web)): digest(p) for p in sorted(web.rglob('*')) if p.is_file()}
            progress('operator:private-auth-gateway-trust-and-backup-signer')
            ca, cert, private = work / 'ca.pem', work / 'tls.pem', work / 'tls.key'
            (work / 'tls.ext').write_text('basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\nsubjectAltName=DNS:localhost,IP:127.0.0.1\n')
            commands = [
                ['openssl', 'req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', str(work / 'ca.key'), '-out', str(ca), '-days', '1', '-subj', '/CN=Synthetic Disposition Auth CA', '-addext', 'basicConstraints=critical,CA:TRUE', '-addext', 'keyUsage=critical,keyCertSign,cRLSign'],
                ['openssl', 'req', '-new', '-newkey', 'rsa:2048', '-nodes', '-keyout', str(private), '-out', str(work / 'tls.csr'), '-subj', '/CN=localhost'],
                ['openssl', 'x509', '-req', '-in', str(work / 'tls.csr'), '-CA', str(ca), '-CAkey', str(work / 'ca.key'), '-CAcreateserial', '-out', str(cert), '-days', '1', '-extfile', str(work / 'tls.ext')],
            ]
            with (work / 'openssl.log').open('wb') as tls_log:
                for argv in commands:
                    subprocess.run(argv, check=True, stdout=tls_log, stderr=tls_log, timeout=30)
            tls = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
            tls.load_cert_chain(cert, private)
            gateway = QuietServer(('127.0.0.1', mail_port), Gateway)
            gateway.socket = tls.wrap_socket(gateway.socket, server_side=True)
            threading.Thread(target=gateway.serve_forever, daemon=True).start()
            servers.append(gateway)
            subprocess.run(['/opt/homebrew/bin/bun', '-e', "const c=require('node:crypto'),f=require('node:fs'),k=c.generateKeyPairSync('ed25519'),s=k.privateKey.export({format:'der',type:'pkcs8'}).subarray(-32),p=k.publicKey.export({format:'der',type:'spki'}).subarray(-32);f.writeFileSync(process.argv[1],Buffer.concat([Buffer.from('3051020101300506032b657004220420','hex'),s,Buffer.from('812100','hex'),p]),{mode:0o600});f.writeFileSync(process.argv[2],p,{mode:0o600});", str(work / 'signing.der'), str(work / 'trusted.pub')], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            key = lambda: base64.urlsafe_b64encode(secrets.token_bytes(32)).decode().rstrip('=')
            config = {'runtimeProfile': 'local-user-macos', 'root': str(work / 'workspace'), 'brokerUid': os.getuid(), 'workspace': 'synthetic-native-dispositions', 'companySlug': 'synthetic', 'companyName': 'Synthetic Disposition Anchor', 'cookieKey': key(), 'mfaKey': key(), 'operatorActors': ['system:synthetic-installer'], 'accounts': [], 'firstOwner': {'actor': 'system:synthetic-installer', 'email': 'owner@example.invalid', 'name': 'Synthetic Owner', 'password': 'synthetic-owner-password-123'}, 'backupSigningKeyFile': str(work / 'signing.der'), 'authMail': {'endpoint': f'https://127.0.0.1:{mail_port}/mail', 'bearerToken': bearer, 'from': 'auth@example.invalid', 'idempotencySecret': secrets.token_urlsafe(32), 'trustedCaFile': str(ca)}}
            write_config()
            start(create=True)
            script = work / 'browser.mjs'
            script.write_text(BROWSER)
            controller = subprocess.Popen(['/opt/homebrew/bin/bun', str(script)], cwd=REPO, env={**os.environ, 'RM_DISPOSITIONS_BASE': origin, 'RM_DISPOSITIONS_WORK': str(work), 'RM_DISPOSITIONS_SCREENSHOT': str(args.evidence.with_suffix('.png')), 'RM_DISPOSITIONS_PLAYWRIGHT': str((REPO / 'node_modules/@playwright/test/index.mjs').resolve(strict=True))}, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, bufsize=1, start_new_session=True)
            report['browserControllerPid'] = controller.pid
            for action in ['authenticate', 'setup', 'plan-propose', 'approve', 'links', 'settlement', 'secondary', 'capture']:
                phase(action)
            stop()
            config.pop('firstOwner')
            write_config()
            start()
            phase('reopen')
            archive = phase('archive')
            report['signedSnapshot'] = archive
            progress('operator:independent-signed-snapshot-verification')
            path = work / 'dispositions.native-backup.json'
            if digest(path) != archive['sha256']:
                raise RuntimeError('Signed artifact changed after download')
            stop()
            source = work / 'workspace'
            if not owned() or source.is_symlink() or not source.is_dir():
                raise RuntimeError('Exact owned source required before source-free removal')
            shutil.rmtree(source)
            (work / 'signing.der').unlink()
            verified = subprocess.run([str(pinned['server_binary']), '--verify-backup', str(path), '--trusted-backup-key', str(work / 'trusted.pub')], cwd=REPO, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=60)
            if verified.returncode != 0:
                raise RuntimeError('Independent signature/inventory verification refused')
            report['trustedSourcePublicKeySha256'] = digest(work / 'trusted.pub')
            report['independentArchiveVerification'] = True
            config.pop('backupSigningKeyFile')
            config.update({'root': str(work / 'restored'), 'cookieKey': key(), 'mfaKey': key(), 'restoreActor': 'system:synthetic-installer', 'firstOwner': {'actor': 'system:synthetic-installer', 'email': 'restored@example.invalid', 'name': 'Synthetic Restored Owner', 'password': 'synthetic-restored-password-789'}})
            write_config()
            start(restore=True)
            restored = phase('restored')
            report['sourceFreeRestoreQualified'] = restored['sourceFreeRestoreQualified']
            report['sourceWorkspaceAndPrivateSignerRemoved'] = not source.exists() and not (work / 'signing.der').exists()
            stop()
            config.pop('firstOwner')
            config.pop('restoreActor')
            write_config()
            start()
            phase('restored-reopen')
            phase('finish', 20)
            controller.stdin.close()
            controller.wait(timeout=10)
            if controller.returncode != 0:
                raise RuntimeError('Controller nonzero terminal')
            controller = None
            with gateway_lock:
                if errors:
                    raise RuntimeError('Private gateway protocol error')
                report['gateway'] = {'https': True, 'trustedCaSha256': digest(ca), 'acceptedMessages': len(gateway_rows), 'requests': list(gateway_requests)}
            report.update({'status': 'passed', 'currentAction': 'complete', 'normalTwoMfaDispositionLifecycleQualified': True, 'initialAndRestartQualified': True, 'snapshotCoverage': 'signed-v2-source-free-restored-and-reopened'})
            save()
            print('PASS: ordinary two-MFA disposition review/link/evidence lifecycle, canonical clients, source-free signed restore and reopen; ' + str(args.evidence))
        except BaseException as error:
            report['status'] = 'failed'
            report['failureType'] = type(error).__name__
            if isinstance(error, RuntimeError):
                report['failureLabel'] = str(error)[:240]
            save()
            raise
        finally:
            if controller is not None and controller.poll() is None:
                os.killpg(controller.pid, signal.SIGTERM)
                try:
                    controller.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    os.killpg(controller.pid, signal.SIGKILL)
                    controller.wait(timeout=5)
            stop()
            for server in servers:
                server.shutdown()
                server.server_close()
            report['ownedProcessesStopped'] = child is None and (controller is None or controller.poll() is not None)
            if owned():
                shutil.rmtree(work)
                report['ownedRuntimeCleaned'] = True
            else:
                report['ownedRuntimeCleaned'] = False
            save()


if __name__ == '__main__':
    main()
