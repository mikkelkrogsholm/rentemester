#!/usr/bin/env python3
"""Ordinary-MFA explicit manual OSS base report, dated reversal and signed recovery qualification.
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
from pathlib import Path
REPO=Path(__file__).resolve().parents[2]
PREFIX='Rentemester-Synthetic-OSS-'
SIGNER="const c=require('node:crypto'),f=require('node:fs'),k=c.generateKeyPairSync('ed25519'),s=k.privateKey.export({format:'der',type:'pkcs8'}).subarray(-32),p=k.publicKey.export({format:'der',type:'spki'}).subarray(-32);f.writeFileSync(process.argv[1],Buffer.concat([Buffer.from('3051020101300506032b657004220420','hex'),s,Buffer.from('812100','hex'),p]),{mode:0o600});f.writeFileSync(process.argv[2],p,{mode:0o600});"
BROWSER = r'''








import {createHmac,createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {createInterface} from 'node:readline';
import {readFileSync,readdirSync,writeFileSync,unlinkSync,lstatSync,realpathSync} from 'node:fs';
import {join} from 'node:path';
import {isDeepStrictEqual} from 'node:util';
const {chromium}=await import(process.env.RM_OSS_PLAYWRIGHT);
const base=process.env.RM_OSS_BASE,work=process.env.RM_OSS_WORK;
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
let today,issueDate,finalDate,stable,freshOwner,sourceCookie,ruleVersion,ossEntryId,postBody,reverseBody;
const originalsById={},receipts={};
const money=v=>{need(typeof v==='number'||typeof v==='string','explicit monetary wire');const s=String(v);need(/^-?\d+(\.\d{1,2})?$/.test(s),'exact cent fixture amount');const negative=s.startsWith('-'),[whole,part='']=s.replace('-','').split('.');return(BigInt(whole)*100n+BigInt(part.padEnd(2,'0')))*(negative?-1n:1n);};
const ossPath=(from=issueDate,to=today)=>prefix+'oss-report?'+new URLSearchParams({from,to});
async function snapshot(){return{invoices:await read(prefix+'invoices'),documents:await read(prefix+'documents'),journal:await read(prefix+'journal'),trialBalance:await read(prefix+'trial-balance'),incomeStatement:await read(prefix+'income-statement'),vat:await read(prefix+'vat'),bank:await read(prefix+'bank'),periods:await read(prefix+'periods'),oss:await read(ossPath()),historicalOss:await read(ossPath(issueDate,issueDate)),reversalOss:await read(ossPath(finalDate,finalDate))};}
async function originals(){for(const[id,proof]of Object.entries(originalsById)){const r=await ownerPage.request.get(base+prefix+`documents/${id}/file`);need(r.status()===200,'ordinary original readable');const raw=await r.body();need(raw.toString('base64')===proof.bytesBase64&&hash(raw)===proof.sha256,'exact retained supporting original');}return originalsById;}
function checkReport(r,baseAmount,count,from=issueDate,to=today,output=25){need(r.ok===true&&isDeepStrictEqual(r.appliedRules,['DK-VAT-OSS-001'])&&r.submission===false&&r.periodStart===from&&r.periodEnd===to,'explicit exact dated base-only report');need(money(r.ossConsumerSalesBase)===money(baseAmount)&&r.entryCount===count,'exact signed OSS base and distinct entry count');need(money(r.vatReport.outputVat)===money(output)&&money(r.vatReport.rubrikker.rubrikB)===0n&&money(r.vatReport.rubrikker.rubrikC)===0n,'ordinary DK VAT preserved and OSS outside B/C');need(r.errors.length===0,'valid underlying VAT');if(baseAmount>0)need(r.warnings.length>0,'positive base carries explicit limited responsibility warning');return r;}
async function setup(){step='ordinary-profile-accounts-two-retained-originals';today=(await read(prefix+'scheduled-recurring')).scheduledRecurring.asOfDate;need(/^\d{4}-\d{2}-\d{2}$/.test(today),'trusted server date');const ago=n=>{const d=new Date(today+'T12:00:00Z');d.setUTCDate(d.getUTCDate()-n);return d.toISOString().slice(0,10);};issueDate=ago(2);finalDate=ago(1);need(issueDate.slice(0,4)===today.slice(0,4)&&Math.floor((Number(issueDate.slice(5,7))-1)/3)===Math.floor((Number(today.slice(5,7))-1)/3),'fixture requires same current year and VAT quarter');const changed=await ownerPage.request.patch(base+prefix+'company',{headers:{origin:base},data:{cvr:'87654321',address:'Synthetic OSS Street 79',postalCode:'1000',city:'Synthetic City',vatPeriodType:'quarter',paymentTermsDays:14}});need(changed.status()===200,'ordinary VAT-registered profile');for(const[accountNo,name,type,normalBalance]of[['1100','Synthetic receivables','asset','debit'],['1800','Synthetic DKK bank','asset','debit'],['1000','Synthetic service revenue','income','credit'],['4200','Synthetic output VAT','vat','credit']])await post(prefix+'accounts',{accountNo,name,type,normalBalance});for(const[role,accountNo]of[['debtors','1100'],['bank','1800'],['output_vat','4200']])await post(prefix+'account-roles',{role,accountNo,confirm:true});ruleVersion=(await read(prefix+'journal')).journal.nativePosting.ruleVersion;
 for(const[kind,amount,vat]of[['domestic',125,25],['oss',1000,0]]){const bytes=Buffer.from(`Synthetic supporting ${kind} original dated ${issueDate}; DKK${amount}; explicitly recorded VAT${vat}.\n`);const uploaded=await post(prefix+'documents/ingest',{confirm:true,fileName:`synthetic-${kind}-79.txt`,fileBase64:bytes.toString('base64'),metadata:{source:'Synthetic separately retained supporting original',documentType:'cash_register_receipt',issueDate,amountIncVat:amount,vatAmount:vat,currency:'DKK'}});need(uploaded.document.id>0,'real ingest receipt');originalsById[uploaded.document.id]={kind,bytesBase64:bytes.toString('base64'),sha256:hash(bytes)};}
 await originals();return{today,issueDate,finalDate,originals:originalsById};}
function documentId(kind){return Object.entries(originalsById).find(([id,proof])=>proof.kind===kind)[0];}
async function domestic(){step='ordinary-explicit-original-backed-domestic-VAT-context';const body={confirm:true,input:{format:'rentemester-manual-journal-post/1',expectedRuleVersion:ruleVersion,idempotencyKey:'synthetic-oss79-domestic-context',request:{transactionDate:issueDate,text:'Synthetic ordinary domestic VAT context',currency:'DKK',documentId:documentId('domestic'),lines:[{accountNo:'1100',debitAmount:'125.00'},{accountNo:'1000',creditAmount:'100.00',vatCode:'DK_SALE_25'},{accountNo:'4200',creditAmount:'25.00'}]}}};receipts.domestic=await post(prefix+'manual-journal',body);need(receipts.domestic.entryId>0,'ordinary context journal');const before=await snapshot();const replay=await post(prefix+'manual-journal',body);need(replay.entryId===receipts.domestic.entryId&&isDeepStrictEqual(await snapshot(),before),'same explicit domestic key creates no second effect');checkReport(before.oss,0,0);return{receipt:receipts.domestic,keyReplayPreservesState:true};}
async function ossUi(){step='actual-manual-form-explicit-OSS-review-and-confirm';await ownerPage.goto(base+'/companies/synthetic/posteringer');await ownerPage.getByRole('button',{name:'Ny postering',exact:true}).click();const form=ownerPage.getByRole('region',{name:'Ny manuel postering',exact:true});for(const[label,value]of[['Dato',issueDate],['Posteringstekst','Synthetic explicitly classified OSS consumer sale'],['Beløb i DKK','1000']])await form.getByLabel(label,{exact:true}).fill(value);for(const[label,value]of[['Bilag (valgfrit)',documentId('oss')],['Debetkonto','1800'],['Kreditkonto','1000']])await form.getByLabel(label,{exact:true}).selectOption(value);await form.getByLabel('Momsklassifikation',{exact:true}).selectOption({label:'OSS-salg til EU-forbruger'});const before=await snapshot();await form.getByRole('button',{name:'Gennemse postering',exact:true}).click();const d=ownerPage.getByRole('dialog',{name:'Bogfør postering',exact:true});await d.waitFor();need((await d.innerText()).includes('OSS'),'explicit OSS classification visible in review');need(isDeepStrictEqual(await snapshot(),before),'review creates no journal');const pending=ownerPage.waitForResponse(r=>new URL(r.url()).pathname===prefix+'manual-journal'&&r.request().method()==='POST');await d.getByRole('button',{name:'Bogfør postering',exact:true}).click();const r=await pending;postBody=r.request().postDataJSON();need(r.status()===200,'ordinary web manual post confirmed');receipts.oss=await r.json();ossEntryId=receipts.oss.entryId;need(ossEntryId>0&&postBody.input.request.lines.find(l=>l.accountNo==='1000')?.vatCode==='OSS_EU_CONSUMER','exact UI explicit classification on income only');await form.waitFor({state:'hidden'});const after=await snapshot();checkReport(after.oss,1000,1);checkReport(after.historicalOss,1000,1,issueDate,issueDate);const replay=await post(prefix+'manual-journal',postBody);need(replay.entryId===ossEntryId&&isDeepStrictEqual(await snapshot(),after),'exact accepted UI key replay has no new effect');return{receipt:receipts.oss,explicitUiClassification:true,reviewReadonly:true,keyReplayPreservesState:true,report:after.oss};}
async function reportUi(from,to,amount,count){await ownerPage.goto(base+'/companies/synthetic/moms');await ownerPage.getByLabel('OSS fra',{exact:true}).fill(from);await ownerPage.getByLabel('OSS til',{exact:true}).fill(to);const pending=ownerPage.waitForResponse(r=>new URL(r.url()).pathname===prefix+'oss-report'&&new URL(r.url()).searchParams.get('from')===from&&new URL(r.url()).searchParams.get('to')===to);await ownerPage.getByRole('button',{name:'Vis OSS-rapport',exact:true}).click();const r=await pending;need(r.status()===200,'explicit native OSS panel');const body=checkReport(await r.json(),amount,count,from,to,from===finalDate?0:25);const panel=ownerPage.getByRole('region',{name:'OSS-rapport',exact:true});await panel.getByText('OSS-salgsgrundlag',{exact:true}).waitFor();await panel.getByText('Antal posteringer',{exact:true}).waitFor();need((await panel.innerText()).includes(new Intl.NumberFormat('da-DK',{minimumFractionDigits:2,maximumFractionDigits:2}).format(amount)),'exact signed DKK base visibly rendered');return body;}
async function reverse(){step='ordinary-web-dated-OSS-only-reversal';await reportUi(issueDate,today,1000,1);await ownerPage.goto(base+'/companies/synthetic/posteringer');const row=ownerPage.locator('li.entry-item').filter({hasText:'Synthetic explicitly classified OSS consumer sale'});await row.getByRole('button',{expanded:false}).click();await row.getByRole('button',{name:'Tilbagefør postering',exact:true}).click();const form=row.getByRole('region',{name:'Tilbagefør postering',exact:true});await form.getByLabel('Tilbageførselsdato',{exact:true}).fill(finalDate);await form.getByLabel('Begrundelse',{exact:true}).fill('Synthetic owner explicitly reverses only OSS original');await form.getByRole('button',{name:'Gennemse tilbageførsel',exact:true}).click();const d=ownerPage.getByRole('dialog',{name:'Bekræft tilbageførsel',exact:true});const pending=ownerPage.waitForResponse(r=>new URL(r.url()).pathname===prefix+'manual-journal-reversal'&&r.request().method()==='POST');await d.getByRole('button',{name:'Bogfør tilbageførsel',exact:true}).click();const r=await pending;reverseBody=r.request().postDataJSON();need(r.status()===200,'ordinary UI reversal confirmed');receipts.reversal=await r.json();need(reverseBody.input.request.entryId===String(ossEntryId),'reviewed reversal exact source');await form.waitFor({state:'hidden'});const after=await snapshot();need(after.journal.journal.entries.length===3,'one original context, one OSS, one reversal');checkReport(after.oss,0,2);checkReport(after.historicalOss,1000,1,issueDate,issueDate);checkReport(after.reversalOss,-1000,1,finalDate,finalDate,0);const replay=await post(prefix+'manual-journal-reversal',reverseBody);need(replay.entryId===receipts.reversal.entryId&&isDeepStrictEqual(await snapshot(),after),'exact accepted reversal key has no new effects');await reportUi(issueDate,issueDate,1000,1);await reportUi(issueDate,today,0,2);return{receipts,historical:after.historicalOss,current:after.oss,reversalOnly:after.reversalOss,ordinaryWebReversal:true,keyReplayPreservesState:true};}
async function rights(){step='current-reviewer-read-write-and-revoked-read';const before=await snapshot();checkReport(await get(memberPage,ossPath()),0,2);for(const[path,body]of[['manual-journal',postBody],['manual-journal-reversal',reverseBody]])need((await fetchJson(memberPage,prefix+path,body)).status===403,'current reviewer cannot mutate even with accepted key');for(const query of[`from=${issueDate}&to=${today}&from=${issueDate}`,`from=${today}&to=${issueDate}`,`from=not-a-date&to=${today}`,`from=${issueDate}&to=${today}&unknown=true`])need((await fetchJson(ownerPage,prefix+'oss-report?'+query)).status===400,'exact date selector rejects malformed/duplicate/unordered/unknown');need(isDeepStrictEqual(await snapshot(),before),'report reads/refusals have no domain effects');await post('/api/workspace/members/company',{userId:memberId,action:'revoke',companySlug:'synthetic'});need((await fetchJson(memberPage,ossPath())).status===403,'current revoked report read refused');need(isDeepStrictEqual(await snapshot(),before),'revoked read preserves financial state');return{reviewerReadQualified:true,reviewerMutationDenied:true,revokedReadDenied:true,invalidSelectorsRefused:true};}
async function freeze(){step='canonical-OSS-and-retained-originals';sourceCookie=(await ownerPage.context().cookies(base)).map(c=>`${c.name}=${c.value}`).join('; ');stable=await snapshot();await originals();await reportUi(issueDate,today,0,2);await ownerPage.screenshot({path:process.env.RM_OSS_SCREENSHOT,fullPage:true});return{canonical:stable,receipts,originals:originalsById};}
async function archive(name){step='signed-oss-'+name;const r=await ownerPage.request.post(base+'/api/workspace/backup',{headers:{origin:base},data:{confirm:true}});const ct=r.headers()['content-type'];if(r.status()!==200||ct!=='application/vnd.rentemester.native-backup+json'){let body;try{body=await r.json();}catch{}detail={status:r.status(),contentType:ct,code:body?.code,errors:body?.errors};throw Error('signed OSS backup refused');}const bytes=await r.body();need(bytes.length&&bytes.length<=96*1024*1024,'bounded complete signed artifact');writeFileSync(join(work,`oss-${name}.native-backup.json`),bytes,{mode:0o600,flag:'wx'});need(isDeepStrictEqual(await snapshot(),stable),'backup changes no business evidence');return{sha256:hash(bytes),bytes:bytes.length};}
async function reopen(){step='source-normal-mfa-restart';await ownerPage.context().clearCookies();await login(ownerPage,owner);need(isDeepStrictEqual(await snapshot(),stable),'exact source restart domain state');await originals();return{freshMfaRestart:true};}
async function restored(){step='source-free-normal-new-owner';ownerPage=await newPage();freshOwner={email:'restored@example.invalid',password:'synthetic-restored-password-789',codes:[]};await ownerPage.goto((await mail('verification',freshOwner.email)).actionUrl);need((await enroll(ownerPage,freshOwner)).user.id!==ownerId,'historical actor is not live restored grant');const denied=await ownerPage.request.get(base+prefix+'journal',{headers:{cookie:sourceCookie}});need([401,403].includes(denied.status()),'old source session has no authority');need(isDeepStrictEqual(await snapshot(),stable),'source-free exact manual OSS/VAT and historical correction state');await originals();return {sourceFreeRestoreQualified:true,credentialsAbsent:true};}
async function restoredReopen(){step='target-normal-mfa-reopen';await ownerPage.context().clearCookies();await login(ownerPage,freshOwner);need(isDeepStrictEqual(await snapshot(),stable),'target reopen exact retained accounting');await originals();return {restoredTargetReopenQualified:true};}
let clientPhase=0;
async function clientReads(){step='current-session-cli-mcp-http-full-parity';const before=await snapshot(),cookiePath=join(realpathSync(work),`vat-client-${++clientPhase}.cookie`);writeFileSync(cookiePath,(await ownerPage.context().cookies(base)).map(c=>`${c.name}=${c.value}`).join('; '),{mode:0o600,flag:'wx'});const common=['--server',base,'--cookie-file',cookiePath,'--allow-loopback-http'];const cli=(command,body,selectors=[])=>{const run=spawnSync(join(work,'rentemester-rust'),['remote',command,...common,'--company','synthetic',...selectors,...(body?['--confirm']:[])],{input:body?JSON.stringify(body):undefined,encoding:'utf8',timeout:45000,maxBuffer:32*1024*1024});need(!run.error,'CLI invocation');return {code:run.status,data:JSON.parse(run.stdout)};};const rpc=(name,args)=>{const input=[{jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'synthetic-oss',version:'1'}}},{jsonrpc:'2.0',method:'notifications/initialized'},{jsonrpc:'2.0',id:2,method:'tools/call',params:{name,arguments:args}}].map(JSON.stringify).join('\n')+'\n';const run=spawnSync(join(work,'rentemester-mcp'),common,{input,encoding:'utf8',timeout:45000,maxBuffer:32*1024*1024});need(!run.error&&run.status===0,'MCP process terminal');const response=run.stdout.trim().split('\n').map(JSON.parse).find(v=>v.id===2);need(!response.error,'MCP protocol');return response.result;};
 try{for(const[command,path]of [['journal','journal'],['trial-balance','trial-balance'],['income-statement','income-statement'],['vat','vat']]){const canonical=await read(prefix+path),c=cli(command),m=rpc(command.replaceAll('-','_'),{company:'synthetic'});need(c.code===0&&c.data.ok===true&&isDeepStrictEqual(c.data.data,canonical),'full CLI canonical accounting read');need(!m.isError&&isDeepStrictEqual(m.structuredContent.data,canonical),'full MCP canonical accounting read');}for(const[from,to,amount,count]of[[issueDate,today,0,2],[issueDate,issueDate,1000,1],[finalDate,finalDate,-1000,1]]){const canonical=checkReport(await read(ossPath(from,to)),amount,count,from,to,from===finalDate?0:25),c=cli('oss-report',undefined,['--from',from,'--to',to]),m=rpc('vat_oss_report',{company:'synthetic',from,to});need(c.code===0&&c.data.ok===true&&isDeepStrictEqual(c.data.data,canonical),'full dated CLI OSS equality');need(!m.isError&&isDeepStrictEqual(m.structuredContent.data,canonical),'full dated MCP OSS equality');}need(isDeepStrictEqual(await snapshot(),before),'readonly clients append no domain effects');return{httpCliMcpCanonicalEquality:true,threeDatedOssWindowsQualified:true,readonlyReportPreservesState:true};}finally{unlinkSync(cookiePath);}}
for await(const line of createInterface({input:process.stdin,crlfDelay:Infinity})){try{const action=JSON.parse(line);let value;if(action==='authenticate')value=await authenticate();else if(action==='setup')value=await setup();else if(action==='domestic')value=await domestic();else if(action==='oss')value=await ossUi();else if(action==='reverse')value=await reverse();else if(action==='rights')value=await rights();else if(action==='freeze')value=await freeze();else if(action==='reopen')value=await reopen();else if(action==='restored')value=await restored();else if(action==='restored-reopen')value=await restoredReopen();else if(action.startsWith('client-reads'))value=await clientReads();else if(action.startsWith('archive-'))value=await archive(action.slice(8));else if(action==='finish'){await browser?.close();emit({ok:true,value:{finished:true}});break;}else throw Error('unknown phase');emit({ok:true,value});}catch(error){if(ownerId&&diagnosticPage)await ownerPage.screenshot({path:process.env.RM_OSS_SCREENSHOT,fullPage:true}).catch(()=>{});emit({ok:false,step,detail,errorType:error?.name,errorMessage:String(error?.message).slice(0,1200),trace});await browser?.close().catch(()=>{});process.exitCode=1;break;}}
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
    report = {'status':'running','sourceRevision':subprocess.check_output(['git','rev-parse','HEAD'],cwd=REPO,text=True).strip(),'runnerSha256':digest(Path(__file__)),'browserHelperSha256':hashlib.sha256(BROWSER.encode()).hexdigest(),'canonicalOrigin':origin,'noOfflineIdentitySeeds':True,'ordinaryMacOsUser':True,'ownedProfile':str(work),'externalMailDelivered':False,'workflow':'explicit_oss_manual_classification_and_dated_base_report','installedNativeQualified':False,'ossActualQualified':False,'sourceFreeRestoreQualified':False,'screenshot':str(args.evidence.with_suffix('.png')),'actions':[],'results':{}}
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
                argv += ['--restore-backup', str(work / 'oss-source.native-backup.json'), '--trusted-backup-key', str(work / 'trusted.pub'), '--confirm-target', str(root), '--source-unavailable']
            log = (work / 'server.log').open('ab')
            child = subprocess.Popen(argv, cwd=REPO, stdin=subprocess.DEVNULL, stdout=log, stderr=log)
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
            progress('operator:independent-signed-oss-' + name)
            path = work / ('oss-' + name + '.native-backup.json')
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
            copy = work / ('readonly-oss-' + name + '.sqlite')
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
            if len(result['journal_entries']) != 3 or len(result['documents']) != 2:
                raise RuntimeError('Expected exactly domestic/OSS/reversal journals and two originals')
            for table in ['issued_invoice_postings','credit_note_postings','invoice_payments','invoice_refunds','bank_transactions']:
                if result[table]:
                    raise RuntimeError('Unexpected invoice, payment, credit, refund or imported bank effect: '+table)
            signed_originals={name:hashlib.sha256(value).hexdigest() for name,value in entries.items() if '/documents/' in name}
            original_proofs=report['results']['freeze']['originals']
            for proof in original_proofs.values():
                wanted=base64.b64decode(proof['bytesBase64'],validate=True)
                if hashlib.sha256(wanted).hexdigest()!=proof['sha256'] or not any(value==wanted for name,value in entries.items() if '/documents/' in name):
                    raise RuntimeError('Exact supporting original missing from signed inventory')
            accounts={row['id']:row['account_no'] for row in result['accounts']}
            net={account:sum((Decimal(str(row['debit_amount']))-Decimal(str(row['credit_amount'])) for row in result['journal_lines'] if accounts[row['account_id']]==account),Decimal(0)) for account in ['1100','1000','1800','4200']}
            if net!={'1100':Decimal(125),'1000':Decimal(-100),'1800':Decimal(0),'4200':Decimal(-25)}:
                raise RuntimeError('Final domestic-only exact DKK cash/revenue/receivable/VAT effect differs')
            receipts=report['results']['freeze']['receipts']
            for kind,base_amount,debit,credit in [('domestic',Decimal(100),Decimal(0),Decimal(100)),('oss',Decimal(1000),Decimal(0),Decimal(1000)),('reversal',Decimal(-1000),Decimal(1000),Decimal(0))]:
                lines=[row for row in result['journal_lines'] if row['journal_entry_id']==receipts[kind]['entryId']]
                income=[row for row in lines if accounts[row['account_id']]=='1000']
                code='DK_SALE_25' if kind=='domestic' else 'OSS_EU_CONSUMER'
                if len(income)!=1 or income[0]['vat_code']!=code or Decimal(str(income[0]['debit_amount']))!=debit or Decimal(str(income[0]['credit_amount']))!=credit:
                    raise RuntimeError('Exact explicit original/reversal income classification differs')
                if sum((Decimal(str(row['credit_amount']))-Decimal(str(row['debit_amount'])) for row in income),Decimal(0))!=base_amount:
                    raise RuntimeError('Signed OSS/date provenance differs')
            if result.get('vies_validations'):
                raise RuntimeError('Manual OSS base report must not create external observations')
            result['signedOriginalHashes']=signed_originals
            report.setdefault('snapshots', {})[name] = {'sha256':receipt['sha256'],'trustedPublicKeySha256':digest(trusted),'privateCredentialsExcluded':True,'tables':result}
            save()
            return result

        def preserved(original, newer):
            if set(original)!=set(newer):
                raise RuntimeError('Restored full company table inventory changed')
            for table in original:
                if table!='audit_log' and original[table]!=newer[table]:
                    raise RuntimeError('Exact restored OSS evidence differs: '+table)
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
                ['openssl', 'req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', str(work / 'ca.key'), '-out', str(ca), '-days', '1', '-subj', '/CN=Synthetic OSS Auth CA', '-addext', 'basicConstraints=critical,CA:TRUE', '-addext', 'keyUsage=critical,keyCertSign,cRLSign'],
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
            config = {'runtimeProfile': 'local-user-macos', 'root': str(work / 'workspace'), 'brokerUid': os.getuid(), 'workspace': 'synthetic-native-oss', 'companySlug': 'synthetic', 'companyName': 'Synthetic OSS Company', 'cookieKey': key(), 'mfaKey': key(), 'operatorActors': ['system:synthetic-installer'], 'accounts': [], 'firstOwner': {'actor': 'system:synthetic-installer', 'email': 'owner@example.invalid', 'name': 'Synthetic Owner', 'password': 'synthetic-owner-password-123'}, 'backupSigningKeyFile': str(work / 'signing.der'), 'authMail': {'endpoint': f'https://127.0.0.1:{mail_port}/mail', 'bearerToken': bearer, 'from': 'auth@example.invalid', 'idempotencySecret': secrets.token_urlsafe(32), 'trustedCaFile': str(ca)}}
            write_config()
            start(create=True)
            script = work / 'browser.mjs'
            script.write_text(BROWSER)
            controller = subprocess.Popen(['/opt/homebrew/bin/bun', str(script)], cwd=REPO, env={**os.environ, 'RM_OSS_BASE': origin, 'RM_OSS_WORK': str(work), 'RM_OSS_SCREENSHOT': str(args.evidence.with_suffix('.png')), 'RM_OSS_PLAYWRIGHT': str((REPO / 'node_modules/@playwright/test/index.mjs').resolve(strict=True))}, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, bufsize=1, start_new_session=True)
            report['browserControllerPid'] = controller.pid
            phase('authenticate')
            phase('setup')
            phase('domestic')
            phase('oss')
            phase('reverse')
            phase('rights')
            phase('freeze')
            phase('client-reads-source')
            phase('archive-source')
            original = inspect_snapshot('source', work/'trusted.pub')
            stop()
            config.pop('firstOwner')
            write_config()
            start()
            phase('reopen')
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
                raise RuntimeError('Fresh target reopen changed retained full OSS evidence')
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
            report.update({'status':'passed','currentAction':'complete','ossActualQualified':True,'httpCliMcpReadParityQualified':True,'snapshotCoverage':'four-independently-verified-oss-full-ledger-original-archives-fresh-owner-restore-and-reopen'})
            save()
            print('PASS: ordinary MFA explicit manual OSS classification, dated correction, preserved DK VAT, canonical clients and signed source-free recovery; '+str(args.evidence))
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
