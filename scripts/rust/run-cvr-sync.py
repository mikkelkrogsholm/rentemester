#!/usr/bin/env python3
"""Ordinary-MFA interactive CVR contact lookup and explicit own-company synchronization and signed recovery qualification.
Supplied binaries/web are privately pinned; only a fresh synthetic macOS profile is used.
All business changes use ordinary authorized UI/APIs. Signed SQLite is inspected read-only.
Only a private verified-TLS synthetic mail gateway is used; no external calls, identity seeds, implicit retry, or product build.
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
import sqlite3
import socket
import ssl
import subprocess
import threading
import time
import uuid
from decimal import Decimal
from datetime import datetime, timedelta
from collections import Counter
from pathlib import Path
REPO=Path(__file__).resolve().parents[2]
PREFIX='Rentemester-Synthetic-CVR-Sync85-'
SIGNER="const c=require('node:crypto'),f=require('node:fs'),k=c.generateKeyPairSync('ed25519'),s=k.privateKey.export({format:'der',type:'pkcs8'}).subarray(-32),p=k.publicKey.export({format:'der',type:'spki'}).subarray(-32);f.writeFileSync(process.argv[1],Buffer.concat([Buffer.from('3051020101300506032b657004220420','hex'),s,Buffer.from('812100','hex'),p]),{mode:0o600});f.writeFileSync(process.argv[2],p,{mode:0o600});"
BROWSER = r'''








import {createHmac,createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {createInterface} from 'node:readline';
import {readFileSync,readdirSync,writeFileSync,unlinkSync,lstatSync,realpathSync} from 'node:fs';
import {join} from 'node:path';
import {isDeepStrictEqual} from 'node:util';
const {chromium}=await import(process.env.RM_CVR_SYNC_PLAYWRIGHT);
const base=process.env.RM_CVR_SYNC_BASE,work=process.env.RM_CVR_SYNC_WORK;
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
async function newPage(){const context=await browser.newContext();const page=await context.newPage();page.setDefaultTimeout(45000);diagnosticPage=page;page.on('request',r=>{if(new URL(r.url()).pathname==='/api/auth/request-password-reset')trace.push({kind:'request',path:'/api/auth/request-password-reset',method:r.method()});});page.on('requestfailed',r=>{if(new URL(r.url()).pathname==='/api/auth/request-password-reset')trace.push({kind:'requestfailed',path:'/api/auth/request-password-reset',code:r.failure()?.errorText?.match(/net::[A-Z_]+/)?.[0]});});page.on('response',r=>{const path=new URL(r.url()).pathname;if(path.startsWith('/api/auth/')){trace.push({path:path.startsWith('/api/auth/reset-password/')?'/api/auth/reset-password/:token':path,status:r.status()});if(trace.length>24)trace.shift();}});page.on('response',r=>{const u=new URL(r.url());if(/^\/api\/companies\/[a-z0-9-]+\/(invoices|customers|documents|journal|trial-balance)(\/|$)/.test(u.pathname)){trace.push({step,path:u.pathname,status:r.status(),method:r.request().method(),year:u.searchParams.get('year'),source:u.searchParams.get('source'),format:u.searchParams.get('format')});if(trace.length>32)trace.shift();}});return page;}
async function authenticate(){
 browser=await chromium.launch({headless:true});ownerPage=await newPage();memberPage=await newPage();
 step='owner-mail-verification';await ownerPage.goto((await mail('verification',owner.email)).actionUrl);const who=await enroll(ownerPage,owner);ownerId=who.user.id;
 step='ordinary-web-invitation';await ownerPage.goto(base+'/adgang');await ownerPage.getByRole('heading',{name:'Brugere og adgang',exact:true}).waitFor();await ownerPage.getByLabel('E-mail',{exact:true}).fill(member.email);await ownerPage.getByLabel(/^Rolle/).selectOption('reviewer');await ownerPage.getByRole('button',{name:'Send invitation',exact:true}).click();await ownerPage.getByText('Invitationen er sendt.',{exact:true}).waitFor();const invitation=await mail('workspace-invitation',member.email);const listed=await get(ownerPage,'/api/workspace/invitations');const item=listed.invitations.find(i=>i.email===member.email);need(item?.status==='delivery_confirmed','canonical invitation delivery');invitationId=item.invitationId;
 step='normal-invited-claim';await memberPage.goto(invitation.actionUrl);await memberPage.getByRole('heading',{name:'Acceptér invitation',exact:true}).waitFor();need(!new URL(memberPage.url()).hash,'invitation token removed from address');await memberPage.getByLabel('Navn',{exact:true}).fill('Synthetic Invited User');await memberPage.getByLabel('Adgangskode',{exact:true}).fill(member.password);const claim=memberPage.waitForResponse(r=>new URL(r.url()).pathname==='/api/invitations/claim'&&r.request().method()==='POST');await memberPage.getByRole('button',{name:'Acceptér invitation',exact:true}).click();const claimed=await claim;need(claimed.ok()&&!claimed.headers()['set-cookie'],'claim cannot mint session');await memberPage.getByText('Invitationen er accepteret.',{exact:true}).waitFor();need((await fetchJson(memberPage,'/api/companies/synthetic/trial-balance')).status===401,'claim not sufficient for business read');
 step='invited-mail-verification-existing-mfa';await memberPage.goto((await mail('verification',member.email)).actionUrl);const mwho=await enroll(memberPage,member);memberId=mwho.user.id;need(memberId!==ownerId,'independent ordinary users');baseline=await get(memberPage,'/api/companies/synthetic/trial-balance');
 return {ownerId,memberId,invitationId,ordinaryInvitationClaim:true,distinctNormalMfaUsers:true};
}
const prefix='/api/companies/synthetic/',hash=b=>createHash('sha256').update(b).digest('hex');
const read=p=>get(ownerPage,p);
const post=async(p,b,page=ownerPage)=>{const r=await fetchJson(page,p,b);if(r.status!==200||r.body?.ok!==true){detail={path:p,status:r.status,code:r.body?.code,errors:r.body?.errors};throw Error('confirmed domain mutation refused');}return r.body;};
let stable,freshOwner,sourceCookie,protectedSettings;
const receipts={},lookupProof={};
const cvrPath=cvr=>prefix+'cvr-lookup?'+new URLSearchParams({cvr});
async function accounting(){const value={journal:await read(prefix+'journal'),trialBalance:await read(prefix+'trial-balance'),bank:await read(prefix+'bank'),accounts:await read(prefix+'accounts')};return JSON.parse(JSON.stringify(value,(key,item)=>key==='company'?undefined:item));}
async function snapshot(){return{accounting:await accounting(),profile:await read(prefix+'company'),contacts:await read(prefix+'contacts')};}
function protectedProfile(value){const result={...value.company};for(const field of ['name','address','postalCode','city','companyForm','industryCode','industryText','cvrStatus','auditWaived','cvrSyncedAt'])delete result[field];return result;}
async function setup(){step='ordinary-manual-company-protected-accounting-and-payment-settings';const profile=await ownerPage.request.patch(base+prefix+'company',{headers:{origin:base},data:{cvr:'87654321',address:'Manual Street85',postalCode:'1000',city:'Manual City85',vatPeriodType:'quarter',paymentTermsDays:21}});need(profile.status()===200,'ordinary own CVR/profile');for(const[accountNo,name,type,normalBalance]of[['1800','Synthetic bank85','asset','debit'],['4200','Synthetic output VAT','vat','credit'],['4300','Synthetic input VAT','vat','debit']])await post(prefix+'accounts',{accountNo,name,type,normalBalance});for(const[role,accountNo]of[['bank','1800'],['output_vat','4200'],['input_vat','4300']])await post(prefix+'account-roles',{role,accountNo,confirm:true});await post(prefix+'bank-accounts',{name:'Synthetic protected bank85',slug:'synthetic-bank85',currency:'DKK',ledgerAccountNo:'1800',bankName:'Synthetic Bank85',registrationNo:'1234',accountNo:'1234567890'});protectedSettings=protectedProfile(await read(prefix+'company'));need(protectedSettings.fiscalYearStartMonth===1&&protectedSettings.paymentTermsDays===21,'configured fiscal/payment facts');return{protectedSettings,accounting:await accounting()};}
async function contacts(){step='interactive-contact-lookup-blank-prefill-cache-and-manual-preservation-separate-save';const before=await read(prefix+'contacts');await ownerPage.goto(base+'/companies/synthetic/kontakter');await ownerPage.getByRole('button',{name:'Tilføj kunde',exact:true}).first().click();const form=ownerPage.getByRole('dialog',{name:'Tilføj kunde',exact:true});await form.getByLabel(/^CVR \/ moms-nr\./).fill('12345678');let pending=ownerPage.waitForResponse(r=>new URL(r.url()).pathname===prefix+'cvr-lookup');await form.getByRole('button',{name:'Slå CVR op',exact:true}).click();let response=await pending;need(response.status()===200,'ordinary interactive native lookup');const first=(await response.json()).cvr;need(first.ok&&first.cached===false&&first.company.cvr==='12345678'&&first.company.name==='Synthetic Contact Company85','fresh trusted requested contact company');lookupProof.contact=first;need(await form.getByLabel('Navn',{exact:true}).inputValue()===first.company.name,'blank contact name prefilled');need((await form.getByLabel('Adresse',{exact:true}).inputValue()).includes(first.company.address),'blank address prefilled');need(isDeepStrictEqual(await read(prefix+'contacts'),before),'lookup does not save contact');await form.getByLabel('Navn',{exact:true}).fill('Manually Reviewed Contact85');await form.getByLabel('E-mail',{exact:true}).fill('manual85@example.invalid');pending=ownerPage.waitForResponse(r=>new URL(r.url()).pathname===prefix+'cvr-lookup');await form.getByRole('button',{name:'Slå CVR op',exact:true}).click();response=await pending;const second=(await response.json()).cvr;need(second.ok&&second.cached===true&&second.fetchedAt===first.fetchedAt&&isDeepStrictEqual(second.company,first.company),'cache hit exact original timestamp/company');need(await form.getByLabel('Navn',{exact:true}).inputValue()==='Manually Reviewed Contact85'&&await form.getByLabel('E-mail',{exact:true}).inputValue()==='manual85@example.invalid','lookup preserves manually reviewed fields');pending=ownerPage.waitForResponse(r=>new URL(r.url()).pathname===prefix+'customers'&&r.request().method()==='POST');await form.getByRole('button',{name:'Opret',exact:true}).click();need((await pending).status()===200,'separate explicit contact save');await form.waitFor({state:'hidden'});receipts.contact={first,second};return{lookupProof,manualFieldsPreserved:true,lookupAndSaveSeparated:true};}
const updatedFields=['name','address','postalCode','city','companyForm','industryCode','industryText','cvrStatus','auditWaived'];
async function syncUi(){step='explicit-own-CVR-sync-nine-fields-fiscal-warning-and-protected-settings';const before=await accounting();await ownerPage.goto(base+'/companies/synthetic/manage');let pending=ownerPage.waitForResponse(r=>new URL(r.url()).pathname===prefix+'sync-cvr'&&r.request().method()==='POST');await ownerPage.getByRole('button',{name:'Hent fra CVR',exact:true}).click();let response=await pending;need(response.status()===200,'ordinary explicit own CVR sync');const first=(await response.json()).sync;need(first.ok&&first.cached===false&&isDeepStrictEqual(first.updatedFields,updatedFields),'exact ordered nine own-CVR fields');need(isDeepStrictEqual(first.fiscalYearStartMonth,{current:1,cvr:7,matches:false}),'fiscal discrepancy is warning, never automatic change');await ownerPage.getByText(/CVR har regnskabsår der starter i måned 7/).waitFor();need(isDeepStrictEqual(protectedProfile(await read(prefix+'company')),protectedSettings)&&isDeepStrictEqual(await accounting(),before),'accounting/payment/fiscal/VAT/bank/role facts protected');pending=ownerPage.waitForResponse(r=>new URL(r.url()).pathname===prefix+'sync-cvr');await ownerPage.getByRole('button',{name:'Hent fra CVR',exact:true}).click();response=await pending;const second=(await response.json()).sync;need(second.ok&&second.cached===true&&second.updatedFields.length===0,'repeat explicit sync reads cache, no changed fields');receipts.sync={first,second};lookupProof.own=await get(ownerPage,cvrPath('87654321'));lookupProof.own=lookupProof.own.cvr;return{receipts,protectedSettings};}
async function probes(){step='provider-failure-mismatch-invalid-input-and-current-rights-before-provider';const before=await snapshot();for(const cvr of ['bad','55555555','44444444']){const response=await fetchJson(ownerPage,cvrPath(cvr));need(response.status===200&&response.body.cvr.ok===false&&response.body.cvr.company===null&&response.body.cvr.errors.length>0,'invalid/failing/mismatched provider result not invented success');}for(const path of [cvrPath('55555555'),cvrPath('12345678')])need((await fetchJson(memberPage,path)).status===403,'reviewer lookup denied before cache/provider');need((await fetchJson(memberPage,prefix+'sync-cvr',{})).status===403,'reviewer sync denied');need(isDeepStrictEqual(await snapshot(),before),'provider failures and denied calls preserve caches/profile/contact/accounting');await post('/api/workspace/members/company',{userId:memberId,action:'revoke',companySlug:'synthetic'});need((await fetchJson(memberPage,cvrPath('12345678'))).status===403,'revoked lookup denied');return{failureMismatchAndInvalidRefused:true,reviewerLookupSyncDenied:true,revokedLookupDenied:true,staleAndMidflightActualQualified:false};}
async function clientMutations(){step='explicit-CLI-MCP-sync-inner-canonical-parity-and-confirmation-gates';const session=await clientSession();try{const canonical=receipts.sync.second,c=session.cli('sync-company-cvr',undefined,[],true),m=session.rpc('company_sync_cvr',{company:'synthetic',confirm:true});need(c.code===0&&c.data.ok===true&&isDeepStrictEqual(c.data.data,canonical),'CLI exact inner sync result');need(!m.isError&&isDeepStrictEqual(m.structuredContent.data,canonical),'MCP exact inner sync result');const refused=session.cli('cvr-lookup',undefined,['--cvr','12345678']);need(refused.code!==0,'CLI confirmation required before network');const mDenied=session.rpc('cvr_lookup',{company:'synthetic',cvr:'12345678'},true);need(mDenied.isError===true,'MCP explicit confirmation required');const invalid=session.cli('cvr-lookup',undefined,['--cvr','bad'],true);need(invalid.code===1&&invalid.data.ok===false&&invalid.data.data.ok===false,'source invalid-CVR business failure remains visible');receipts.clientSync={cli:c.data,mcp:m.structuredContent};}finally{unlinkSync(session.cookiePath);}need(isDeepStrictEqual(protectedProfile(await read(prefix+'company')),protectedSettings),'client sync protects non-CVR settings');return{receipts};}
async function freeze(){step='canonical-CVR-cache-profile-contact-and-protected-accounting';sourceCookie=(await ownerPage.context().cookies(base)).map(c=>`${c.name}=${c.value}`).join('; ');stable=await snapshot();await ownerPage.goto(base+'/companies/synthetic/manage');await ownerPage.screenshot({path:process.env.RM_CVR_SYNC_SCREENSHOT,fullPage:true});return{canonical:stable,receipts,lookupProof,protectedSettings};}
async function offline(){step='missing-provider-credentials-honest-status-cache-and-uncached-failure';need((await get(ownerPage,'/api/system/cvr-status')).cvrStatus.configured===false,'credentials truly absent after normal restart');const before=await snapshot();for(const cvr of ['12345678','87654321']){const value=(await get(ownerPage,cvrPath(cvr))).cvr;need(value.ok&&value.cached&&value.fetchedAt===(cvr==='12345678'?lookupProof.contact:lookupProof.own).fetchedAt,'offline fresh cache retains source timestamp');}const missing=(await get(ownerPage,cvrPath('33333333'))).cvr;need(missing.ok===false&&missing.company===null&&missing.errors.length>0,'uncached missing credentials remains truthful failure');const status=ownerPage.waitForResponse(r=>new URL(r.url()).pathname==='/api/system/cvr-status');await ownerPage.goto(base+'/companies/synthetic/manage');await status;need(await ownerPage.getByRole('button',{name:'Hent fra CVR',exact:true}).isDisabled(),'missing credentials visibly disable own sync action');need(isDeepStrictEqual(await snapshot(),before),'missing config/cache reads do not rewrite saved provenance');return{missingCredentialsActualQualified:true,freshOfflineCacheQualified:true,staleFallbackActualQualified:false};}

function clientSession(){const cookiePath=join(realpathSync(work),`bank-client-${Date.now()}-${Math.random().toString(16).slice(2)}.cookie`);return ownerPage.context().cookies(base).then(cookies=>{writeFileSync(cookiePath,cookies.map(c=>`${c.name}=${c.value}`).join('; '),{mode:0o600,flag:'wx'});const common=['--server',base,'--cookie-file',cookiePath,'--allow-loopback-http'];return{cookiePath,cli(command,body,selectors=[],confirmed=false){const run=spawnSync(join(work,'rentemester-rust'),['remote',command,...common,...(command==='cvr-status'?[]:['--company','synthetic']),...selectors,...(body||confirmed?['--confirm']:[])],{input:body?JSON.stringify(body):undefined,encoding:'utf8',timeout:45000,maxBuffer:32*1024*1024});need(!run.error,'CLI terminal');return{code:run.status,data:JSON.parse(run.stdout)};},rpc(name,args,allowProtocolFailure=false){const input=[{jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'synthetic-cvr-sync85',version:'1'}}},{jsonrpc:'2.0',method:'notifications/initialized'},{jsonrpc:'2.0',id:2,method:'tools/call',params:{name,arguments:args}}].map(JSON.stringify).join('\n')+'\n';const run=spawnSync(join(work,'rentemester-mcp'),common,{input,encoding:'utf8',timeout:45000,maxBuffer:32*1024*1024});need(!run.error&&run.status===0,'MCP terminal');const response=run.stdout.trim().split('\n').map(JSON.parse).find(v=>v.id===2);if(response.error&&allowProtocolFailure)return{isError:true,protocolError:response.error};need(!response.error,'MCP protocol');return response.result;}};});}
async function archive(name){step='signed-cvr-sync85-'+name;const r=await ownerPage.request.post(base+'/api/workspace/backup',{headers:{origin:base},data:{confirm:true}});const ct=r.headers()['content-type'];if(r.status()!==200||ct!=='application/vnd.rentemester.native-backup+json'){let body;try{body=await r.json();}catch{}detail={status:r.status(),contentType:ct,code:body?.code,errors:body?.errors};throw Error('signed bank-task backup refused');}const bytes=await r.body();need(bytes.length&&bytes.length<=96*1024*1024,'bounded complete signed artifact');writeFileSync(join(work,`cvr-sync85-${name}.native-backup.json`),bytes,{mode:0o600,flag:'wx'});need(isDeepStrictEqual(await snapshot(),stable),'backup changes no business evidence');return{sha256:hash(bytes),bytes:bytes.length};}
async function reopen(){step='source-normal-mfa-restart';await ownerPage.context().clearCookies();await login(ownerPage,owner);need(isDeepStrictEqual(await snapshot(),stable),'exact source restart domain state');return{freshMfaRestart:true};}
async function restored(){step='source-free-normal-new-owner';ownerPage=await newPage();freshOwner={email:'restored@example.invalid',password:'synthetic-restored-password-789',codes:[]};await ownerPage.goto((await mail('verification',freshOwner.email)).actionUrl);need((await enroll(ownerPage,freshOwner)).user.id!==ownerId,'historical actor is not live restored grant');const denied=await ownerPage.request.get(base+prefix+'journal',{headers:{cookie:sourceCookie}});need([401,403].includes(denied.status()),'old source session has no authority');need(isDeepStrictEqual(await snapshot(),stable),'source-free exact CVR/contact/protected accounting state');return {sourceFreeRestoreQualified:true,credentialsAbsent:true};}
async function restoredReopen(){step='target-normal-mfa-reopen';await ownerPage.context().clearCookies();await login(ownerPage,freshOwner);need(isDeepStrictEqual(await snapshot(),stable),'target reopen exact retained accounting');return {restoredTargetReopenQualified:true};}
async function clientReads(){step='four-phase-current-session-inner-CVR-client-and-protected-state-parity';const before=await snapshot(),session=await clientSession();try{const canonical=(await get(ownerPage,'/api/system/cvr-status')).cvrStatus,c=session.cli('cvr-status'),m=session.rpc('cvr_status',{});need(c.code===0&&isDeepStrictEqual(c.data.data,canonical)&&!m.isError&&isDeepStrictEqual(m.structuredContent.data,canonical),'status is exact inner configured boolean only');for(const cvr of ['12345678','87654321']){const inner=(await get(ownerPage,cvrPath(cvr))).cvr,c=session.cli('cvr-lookup',undefined,['--cvr',cvr==='12345678'?' dk 12 34 56 78 ':cvr],true),m=session.rpc('cvr_lookup',{company:'synthetic',cvr,confirm:true});need(c.code===0&&c.data.ok&&isDeepStrictEqual(c.data.data,inner)&&!m.isError&&isDeepStrictEqual(m.structuredContent.data,inner),'full exact source inner company/cache/timestamp/error equality');}need(isDeepStrictEqual(await snapshot(),before),'fresh cache and status client reads preserve protected state');return{httpCliMcpInnerCanonicalEquality:true,cacheFreshnessAndRawFieldsQualified:true};}finally{unlinkSync(session.cookiePath);}}

for await(const line of createInterface({input:process.stdin,crlfDelay:Infinity})){try{const action=JSON.parse(line);let value;if(action==='authenticate')value=await authenticate();else if(action==='setup')value=await setup();else if(action==='contacts')value=await contacts();else if(action==='sync')value=await syncUi();else if(action==='probes')value=await probes();else if(action==='client-mutations')value=await clientMutations();else if(action==='offline')value=await offline();else if(action==='freeze')value=await freeze();else if(action==='reopen')value=await reopen();else if(action==='restored')value=await restored();else if(action==='restored-reopen')value=await restoredReopen();else if(action.startsWith('client-reads'))value=await clientReads();else if(action.startsWith('archive-'))value=await archive(action.slice(8));else if(action==='finish'){await browser?.close();emit({ok:true,value:{finished:true}});break;}else throw Error('unknown phase');emit({ok:true,value});}catch(error){if(ownerId&&diagnosticPage)await ownerPage.screenshot({path:process.env.RM_CVR_SYNC_SCREENSHOT,fullPage:true}).catch(()=>{});emit({ok:false,step,detail,errorType:error?.name,errorMessage:String(error?.message).slice(0,1200),trace});await browser?.close().catch(()=>{});process.exitCode=1;break;}}
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
    cvr_requests=[]
    cvr_credentials=True
    def entity(cvr,name):
        period={'gyldigFra':'2026-01-01','gyldigTil':None}
        return {'cvrNummer':int(cvr),'virksomhedMetadata':{'nyesteNavn':{'navn':name},'nyesteBeliggenhedsadresse':{'vejnavn':'Synthetic CVR Road85','husnummerFra':10,'postnummer':2100,'postdistrikt':'Synthetic CVR City85'},'nyesteVirksomhedsform':{'virksomhedsformkode':80,'kortBeskrivelse':'ApS','langBeskrivelse':'Anpartsselskab'},'nyesteHovedbranche':{'branchekode':'620100','branchetekst':'Synthetic CVR Industry85'},'sammensatStatus':'NORMAL'},'attributter':[{'type':kind,'vaerdier':[{'vaerdi':value,'periode':period}]} for kind,value in [('REGNSKABSÅR_START','--07-01'),('REGNSKABSÅR_SLUT','--06-30'),('REVISION_FRAVALGT','true')]],'elektroniskPost':[{'kontaktoplysning':'cvr85@example.invalid','periode':period}],'telefonNummer':[{'kontaktoplysning':'12345678','periode':period}],'hjemmeside':[{'kontaktoplysning':'https://example.invalid','periode':period}]}
    entities={'87654321':entity('87654321','Synthetic CVR Company85'),'12345678':entity('12345678','Synthetic Contact Company85')}
    gateway_lock = threading.Lock()
    bearer = secrets.token_urlsafe(32)
    backend_port, mail_port = free_port(), free_port()
    if backend_port == mail_port:
        raise RuntimeError('Distinct owned ports required')
    origin = f'http://127.0.0.1:{backend_port}'
    report = {'status':'running','sourceRevision':subprocess.check_output(['git','rev-parse','HEAD'],cwd=REPO,text=True).strip(),'runnerSha256':digest(Path(__file__)),'browserHelperSha256':hashlib.sha256(BROWSER.encode()).hexdigest(),'canonicalOrigin':origin,'noOfflineIdentitySeeds':True,'ordinaryMacOsUser':True,'ownedProfile':str(work),'externalMailDelivered':False,'workflow':'explicit_cvr_lookup_contact_preservation_and_company_sync','installedNativeQualified':False,'cvrSyncActualQualified':False,'sourceFreeRestoreQualified':False,'screenshot':str(args.evidence.with_suffix('.png')),'actions':[],'results':{}}
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

            def reply(self, status, value):
                raw=value if isinstance(value,bytes) else json.dumps(value).encode()
                self.send_response(status)
                self.send_header('content-type','application/json')
                self.send_header('content-length',str(len(raw)))
                self.end_headers()
                try:
                    self.wfile.write(raw)
                except (BrokenPipeError,ConnectionResetError,ssl.SSLError):
                    pass

            def do_GET(self):
                self.send_error(404)

            def do_POST(self):
                try:
                    length = int(self.headers.get('content-length', '0'))
                    if self.path == '/cvr/virksomhed/_search':
                        if not 0 < length <= 32768 or self.headers.get('authorization') != 'Basic '+base64.b64encode(b'synthetic-cvr85:synthetic-password85').decode():
                            self.reply(403, {'error':'synthetic operator credential refused'})
                            return
                        request=json.loads(self.rfile.read(length))
                        cvr=str(request.get('query',{}).get('term',{}).get('Vrvirksomhed.cvrNummer',''))
                        if cvr not in ['87654321','12345678','55555555','44444444']:
                            raise ValueError('Unexpected CVR request outside explicit synthetic corpus')
                        with gateway_lock:
                            cvr_requests.append(cvr)
                        if cvr=='55555555':
                            self.reply(503, {'error':'synthetic provider unavailable'})
                            return
                        entity=entities['87654321' if cvr=='44444444' else cvr]
                        self.reply(200, {'hits':{'hits':[{'_source':{'Vrvirksomhed':entity}}]}})
                        return
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
                argv += ['--restore-backup', str(work / 'cvr-sync85-source.native-backup.json'), '--trusted-backup-key', str(work / 'trusted.pub'), '--confirm-target', str(root), '--source-unavailable']
            log = (work / 'server.log').open('ab')
            environment={k:v for k,v in os.environ.items() if k not in ['CVR_USERNAME','CVR_PASSWORD','RENTEMESTER_CVR_ENDPOINT','RENTEMESTER_CVR_TRUSTED_CA_FILE']}
            environment.update({'RENTEMESTER_CVR_ENDPOINT':f'https://127.0.0.1:{mail_port}/cvr','RENTEMESTER_CVR_TRUSTED_CA_FILE':str(work/'ca.pem')})
            if cvr_credentials:
                environment.update({'CVR_USERNAME':'synthetic-cvr85','CVR_PASSWORD':'synthetic-password85'})
            child = subprocess.Popen(argv, cwd=REPO, env=environment, stdin=subprocess.DEVNULL, stdout=log, stderr=log)
            report.setdefault('serverPids', []).append(child.pid)
            save()
            for _ in range(300):
                if child.poll() is not None:
                    diagnostic=(work/'server.log').read_bytes()
                    report['startupDiagnostic']={'stderrSha256':hashlib.sha256(diagnostic).hexdigest(),'unknownFields':re.findall(r'unknown field `([A-Za-z_]+)`',diagnostic.decode('utf-8',errors='replace'))}
                    save()
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
            with gateway_lock:
                report['privateGatewayErrors'] = list(errors)
            report['actions'].append({'action': action, 'status': 'complete'})
            report['results'][action] = value['value']
            save()
            return value['value']

        def inspect_snapshot(name, trusted):
            progress('operator:independent-signed-cvr-sync85-' + name)
            path = work / ('cvr-sync85-' + name + '.native-backup.json')
            receipt = report['results']['archive-' + name]
            if digest(path) != receipt['sha256']:
                raise RuntimeError('Exact signed archive bytes changed')
            verified = subprocess.run([str(pinned['server_binary']), '--verify-backup', str(path), '--trusted-backup-key', str(trusted)], cwd=REPO, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=60)
            if verified.returncode:
                raise RuntimeError('Independent trusted-key verification refused')
            wire = json.loads(path.read_bytes())
            entries = {e['path']: base64.b64decode(e['bytesBase64'], validate=True) for e in wire['entries']}
            if len(entries) != len(wire['entries']):
                raise RuntimeError('Duplicate signed entry path')
            for raw in [*entries.values(), base64.b64decode(wire['manifestBase64'], validate=True)]:
                if any(secret.encode() in raw for secret in [bearer, config['authMail']['idempotencySecret'], config['cookieKey'], config['mfaKey']] if secret):
                    raise RuntimeError('Private credential present in signed inventory')
            raw = entries.get('synthetic/data/ledger.sqlite')
            if not raw or len(raw) > 32*1024*1024:
                raise RuntimeError('Bounded complete signed company database missing')
            copy = work / ('readonly-cvr85-' + name + '.sqlite')
            result = {}
            def cell(value):
                return {'bytesBase64': base64.b64encode(value).decode()} if isinstance(value, bytes) else value
            try:
                with copy.open('xb') as f:
                    f.write(raw)
                db = sqlite3.connect(copy.as_uri()+'?mode=ro', uri=True)
                try:
                    db.execute('PRAGMA query_only=ON')
                    tables = [row[0] for row in db.execute("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")]
                    for table in tables:
                        if not re.fullmatch('[A-Za-z_][A-Za-z0-9_]*',table):
                            raise RuntimeError('Unexpected signed table name')
                        names = [row[1] for row in db.execute('PRAGMA table_info('+table+')')]
                        rows = db.execute('SELECT * FROM '+table+' ORDER BY '+','.join('"'+name+'"' for name in names))
                        result[table] = [{k:cell(v) for k,v in zip([c[0] for c in rows.description], row)} for row in rows.fetchall()]
                finally:
                    db.close()
            finally:
                copy.unlink(missing_ok=True)
            for table in ['journal_entries','journal_lines','documents','bank_transactions','issued_invoice_postings','credit_note_postings','invoice_payments','invoice_refunds']:
                if result[table]:
                    raise RuntimeError('CVR/contact sync created unexpected accounting effect: '+table)
            if len(result['customers'])!=1 or result['vendors'] or len(result['cvr_lookups'])!=2 or len(result['accounts'])!=3 or len(result['account_role_mappings'])!=3 or len(result['bank_accounts'])!=1:
                raise RuntimeError('Exact saved contact, cache and protected account/bank inventory differs')
            contact=result['customers'][0]
            if contact['name']!='Manually Reviewed Contact85' or contact['email']!='manual85@example.invalid':
                raise RuntimeError('Manual contact fields overwritten by cache lookup')
            frozen=report['results']['freeze']
            cache={row['cvr']:row for row in result['cvr_lookups']}
            if set(cache)!=set(entities):
                raise RuntimeError('Failure/mismatched/unauthorized CVR was cached')
            for cvr,row in cache.items():
                raw=json.loads(row['raw_response'])
                if raw!=entities[cvr]:
                    raise RuntimeError('Exact original provider entity not retained')
                fetched=datetime.fromisoformat(row['fetched_at'].replace('Z','+00:00'))
                expiry=datetime.fromisoformat(row['expires_at'].replace('Z','+00:00'))
                expected=frozen['lookupProof']['contact' if cvr=='12345678' else 'own']['fetchedAt']
                if row['fetched_at']!=expected or expiry-fetched!=timedelta(days=30):
                    raise RuntimeError('Original provider fetched timestamp or exact 30day expiry changed')
            company=result['companies'][0]
            own=frozen['receipts']['sync']['first']['company']
            fields={'name':'name','address':'address','postal_code':'postalCode','city':'city','company_form':'companyFormShort','industry_code':'industryCode','industry_text':'industryText','cvr_status':'status','audit_waived':'auditWaived'}
            if any(company[column]!=own[field] for column,field in fields.items()) or not company['cvr_synced_at']:
                raise RuntimeError('Nine synchronized CVR identity fields or sync timestamp missing')
            if company['fiscal_year_start_month']!=1 or company['payment_terms_days']!=21 or company['vat_period_type']!='quarter' or company['currency']!='DKK':
                raise RuntimeError('CVR sync changed protected accounting/fiscal/payment configuration')
            audits=[row for row in result['audit_log'] if row['event_type']=='company_cvr_sync']
            if len(audits)!=4 or any(row['actor']!='user:'+report['results']['authenticate']['ownerId'] for row in audits):
                raise RuntimeError('Exactly four explicit current-owner sync audit events required')
            if Counter(cvr_requests)!=Counter({'12345678':1,'87654321':1,'55555555':1,'44444444':1}):
                raise RuntimeError('Provider requests repeated, cache bypassed or unauthorized request escaped')
            report['cvrProvider']={'requests':list(cvr_requests),'verifiedHttps':True,'externalRequests':0,'rawEntities':entities}
            signed_originals={}
            result['signedOriginalHashes']=signed_originals
            report.setdefault('snapshots', {})[name] = {'sha256':receipt['sha256'],'trustedPublicKeySha256':digest(trusted),'privateCredentialsExcluded':True,'tables':result}
            save()
            return result

        def preserved(original, newer):
            if set(original)!=set(newer):
                raise RuntimeError('Restored full company table inventory changed')
            for table in original:
                if table!='audit_log' and original[table]!=newer[table]:
                    raise RuntimeError('Exact restored bank-task evidence differs: '+table)
            retained = {row['id']:row for row in newer['audit_log']}
            if any(retained.get(row['id']) != row for row in original['audit_log']):
                raise RuntimeError('Full original historical audit/provenance not retained')
            prior={row['id'] for row in original['audit_log']}
            if any(row['event_type'] not in ['native_restore_receipt_boundary','native_restore_scheduled_boundary'] or row['actor']!='system:portable-restore' for row in newer['audit_log'] if row['id'] not in prior):
                raise RuntimeError('Unexpected restore-boundary audit mutation')

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
                ['openssl', 'req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', str(work / 'ca.key'), '-out', str(ca), '-days', '1', '-subj', '/CN=Synthetic CVR85 Auth CA', '-addext', 'basicConstraints=critical,CA:TRUE', '-addext', 'keyUsage=critical,keyCertSign,cRLSign'],
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
            config = {'runtimeProfile': 'local-user-macos', 'root': str(work / 'workspace'), 'brokerUid': os.getuid(), 'workspace': 'synthetic-native-cvr-sync85', 'companySlug': 'synthetic', 'companyName': 'Synthetic Manual Company85', 'cookieKey': key(), 'mfaKey': key(), 'operatorActors': ['system:synthetic-installer'], 'accounts': [], 'firstOwner': {'actor': 'system:synthetic-installer', 'email': 'owner@example.invalid', 'name': 'Synthetic Owner', 'password': 'synthetic-owner-password-123'}, 'backupSigningKeyFile': str(work / 'signing.der'), 'authMail': {'endpoint': f'https://127.0.0.1:{mail_port}/mail', 'bearerToken': bearer, 'from': 'auth@example.invalid', 'idempotencySecret': secrets.token_urlsafe(32), 'trustedCaFile': str(ca)}}
            write_config()
            start(create=True)
            script = work / 'browser.mjs'
            script.write_text(BROWSER)
            controller = subprocess.Popen(['/opt/homebrew/bin/bun', str(script)], cwd=REPO, env={**os.environ, 'RM_CVR_SYNC_BASE': origin, 'RM_CVR_SYNC_WORK': str(work), 'RM_CVR_SYNC_SCREENSHOT': str(args.evidence.with_suffix('.png')), 'RM_CVR_SYNC_PLAYWRIGHT': str((REPO / 'node_modules/@playwright/test/index.mjs').resolve(strict=True))}, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, bufsize=1, start_new_session=True)
            report['browserControllerPid'] = controller.pid
            phase('authenticate')
            phase('setup')
            phase('contacts')
            phase('sync')
            phase('probes')
            phase('client-mutations')
            phase('freeze')
            phase('client-reads-source')
            phase('archive-source')
            original = inspect_snapshot('source', work/'trusted.pub')
            stop()
            config.pop('firstOwner')
            cvr_credentials=False
            write_config()
            start()
            phase('reopen')
            phase('offline')
            phase('client-reads-restarted')
            phase('archive-restarted')
            preserved(original, inspect_snapshot('restarted', work/'trusted.pub'))
            stop()
            source = work/'workspace'
            if not owned() or source.is_symlink() or not source.is_dir():
                raise RuntimeError('Exact owned source required for source-free removal')
            shutil.rmtree(source)
            (work/'signing.der').unlink()
            report['sourceWorkspaceAndPrivateSignerRemoved'] = not source.exists() and not (work/'signing.der').exists()
            for name in ['source','restarted']:
                inspect_snapshot(name, work/'trusted.pub')
            subprocess.run(['/opt/homebrew/bin/bun','-e',SIGNER,str(work/'target-signing.der'),str(work/'target-trusted.pub')],check=True,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
            config.update({'root':str(work/'restored'),'backupSigningKeyFile':str(work/'target-signing.der'),'cookieKey':key(),'mfaKey':key(),'restoreActor':'system:synthetic-installer','firstOwner':{'actor':'system:synthetic-installer','email':'restored@example.invalid','name':'Synthetic Restored Owner','password':'synthetic-restored-password-789'}})
            write_config()
            start(restore=True)
            phase('restored')
            phase('client-reads-restored')
            report['sourceFreeRestoreQualified'] = True
            phase('archive-restored')
            preserved(original, inspect_snapshot('restored',work/'target-trusted.pub'))
            stop()
            config.pop('firstOwner');config.pop('restoreActor');write_config();start()
            phase('restored-reopen')
            phase('client-reads-reopened')
            phase('archive-reopened')
            if inspect_snapshot('reopened',work/'target-trusted.pub') != report['snapshots']['restored']['tables']:
                raise RuntimeError('Fresh target reopen changed retained full bank-task evidence')
            phase('finish',20)
            controller.stdin.close();controller.wait(timeout=10)
            if controller.returncode:
                raise RuntimeError('Browser controller nonzero terminal')
            controller = None
            stop()
            # All four native artifacts receive independent verification with
            # their actual signer unavailable; do not claim target moved/frozen.
            (work/'target-signing.der').unlink()
            target = Path(config['root'])
            if not owned() or target != work/'restored' or target.is_symlink() or not target.is_dir():
                raise RuntimeError('Exact owned stopped target required before final independent verification')
            shutil.rmtree(target)
            report['allArchiveSourcesAndPrivateSignersUnavailable'] = not source.exists() and not target.exists() and not (work/'signing.der').exists() and not (work/'target-signing.der').exists()
            for name in ['source','restarted','restored','reopened']:
                inspect_snapshot(name,work/('trusted.pub' if name in ['source','restarted'] else 'target-trusted.pub'))
            with gateway_lock:
                if errors:
                    raise RuntimeError('Private auth gateway protocol error')
                report['gateway']={'verifiedHttps':True,'acceptedMessages':len(gateway_rows),'requests':list(gateway_requests),'externalRequests':0}
            report.update({'status':'passed','currentAction':'complete','cvrSyncActualQualified':True,'httpCliMcpReadParityQualified':True,'snapshotCoverage':'four-independently-verified-cvr-sync85-full-ledger-original-archives-fresh-owner-restore-and-reopen'})
            save()
            print('PASS: ordinary MFA interactive CVR lookup/manual-contact preservation, protected own-company sync, cache provenance and missing-credential recovery, canonical clients and signed source-free recovery; '+str(args.evidence))
        except BaseException as error:
            with gateway_lock:
                report['privateGatewayErrors'] = list(errors)
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
