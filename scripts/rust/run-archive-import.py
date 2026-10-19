#!/usr/bin/env python3
"""Owned normal-MFA closed-history import/CFO/source-free recovery acceptance.

Uses supplied native artifacts, genuine Archive file inputs, explicit historical
currency, immutable raw source CSV/package bytes, native CLI/MCP and signed restore.
Live books/profile/chart stay separate. No SQL/business/auth seeds or real mail.
Actual qualification exists only after every required phase finishes successfully.
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
PREFIX = 'Rentemester-Synthetic-Archive-Import-'
BROWSER = r'''



import {createHmac,createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {createInterface} from 'node:readline';
import {readFileSync,readdirSync,writeFileSync,unlinkSync} from 'node:fs';
import {join} from 'node:path';
import {isDeepStrictEqual} from 'node:util';
const {chromium}=await import(process.env.RM_ARCHIVE_PLAYWRIGHT);
const base=process.env.RM_ARCHIVE_BASE,work=process.env.RM_ARCHIVE_WORK;
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
async function newPage(){const context=await browser.newContext();const page=await context.newPage();page.setDefaultTimeout(45000);diagnosticPage=page;page.on('request',r=>{if(new URL(r.url()).pathname==='/api/auth/request-password-reset')trace.push({kind:'request',path:'/api/auth/request-password-reset',method:r.method()});});page.on('requestfailed',r=>{if(new URL(r.url()).pathname==='/api/auth/request-password-reset')trace.push({kind:'requestfailed',path:'/api/auth/request-password-reset',code:r.failure()?.errorText?.match(/net::[A-Z_]+/)?.[0]});});page.on('response',r=>{const path=new URL(r.url()).pathname;if(path.startsWith('/api/auth/')||path===prefix+'archives/2024'){trace.push({path:path.startsWith('/api/auth/reset-password/')?'/api/auth/reset-password/:token':path,status:r.status()});if(trace.length>24)trace.shift();}});return page;}
async function authenticate(){
 browser=await chromium.launch({headless:true});ownerPage=await newPage();memberPage=await newPage();
 step='owner-mail-verification';await ownerPage.goto((await mail('verification',owner.email)).actionUrl);const who=await enroll(ownerPage,owner);ownerId=who.user.id;
 step='ordinary-web-invitation';await ownerPage.goto(base+'/adgang');await ownerPage.getByRole('heading',{name:'Brugere og adgang',exact:true}).waitFor();await ownerPage.getByLabel('E-mail',{exact:true}).fill(member.email);await ownerPage.getByLabel(/^Rolle/).selectOption('reviewer');await ownerPage.getByRole('button',{name:'Send invitation',exact:true}).click();await ownerPage.getByText('Invitationen er sendt.',{exact:true}).waitFor();const invitation=await mail('workspace-invitation',member.email);const listed=await get(ownerPage,'/api/workspace/invitations');const item=listed.invitations.find(i=>i.email===member.email);need(item?.status==='delivery_confirmed','canonical invitation delivery');invitationId=item.invitationId;
 step='normal-invited-claim';await memberPage.goto(invitation.actionUrl);await memberPage.getByRole('heading',{name:'Acceptér invitation',exact:true}).waitFor();need(!new URL(memberPage.url()).hash,'invitation token removed from address');await memberPage.getByLabel('Navn',{exact:true}).fill('Synthetic Invited User');await memberPage.getByLabel('Adgangskode',{exact:true}).fill(member.password);const claim=memberPage.waitForResponse(r=>new URL(r.url()).pathname==='/api/invitations/claim'&&r.request().method()==='POST');await memberPage.getByRole('button',{name:'Acceptér invitation',exact:true}).click();const claimed=await claim;need(claimed.ok()&&!claimed.headers()['set-cookie'],'claim cannot mint session');await memberPage.getByText('Invitationen er accepteret.',{exact:true}).waitFor();need((await fetchJson(memberPage,'/api/companies/synthetic/trial-balance')).status===401,'claim not sufficient for business read');
 step='invited-mail-verification-existing-mfa';await memberPage.goto((await mail('verification',member.email)).actionUrl);const mwho=await enroll(memberPage,member);memberId=mwho.user.id;need(memberId!==ownerId,'independent ordinary users');baseline=await get(memberPage,'/api/companies/synthetic/trial-balance');
 return {ownerId,memberId,invitationId,ordinaryInvitationClaim:true,distinctNormalMfaUsers:true};
}
const prefix='/api/companies/synthetic/',route=prefix+'archive-import/',hash=b=>createHash('sha256').update(b).digest('hex'),from='2024-01-01',to='2026-01-31';
const postingHeader='Konto;Kontonavn;Dato;Bilag;Bilagstype;Tekst;Momstype;Beløb;Saldo\r\n';
const cases=[['6201','0,004999','0.004999','0.00','2024-02-01',''],['6200','0,005000','0.005000','0.01','2024-03-01','0,005000'],['6202','-0,004999','-0.004999','0.00','2024-04-01','-0,004999'],['6203','-0,005000','-0.005000','-0.01','2024-05-01','-0,005000'],['6204','1,234567','1.234567','1.23','2024-06-01','1,234567'],['6205','-1,235000','-1.235000','-1.24','2024-07-01','-1,235000'],['6200','0,005000','0.005000','0.01','2024-08-01','0,010000']];
const files=[{path:'2024/Posteringer.csv',bytes:Buffer.from('\ufeff'+postingHeader+cases.map((c,i)=>`${c[0]};Synthetic archive ${c[0]};${c[4]};A-${i};Archive;Synthetic raw ${i};;${c[1]};${c[5]}\r\n`).join(''))},{path:'2024/SaldoBalance.csv',bytes:Buffer.from('Konto;Kontonavn;Beløb\n7000;Synthetic reference debit;1,234567\n8000;Synthetic reference credit;-1,234567\n')},{path:'2025/Posteringer.csv',bytes:Buffer.from(postingHeader+'9999;Synthetic excluded control;2025-01-01;EXCLUDE;Archive;Excluded latest source;;999,00;999,00\r\n')}];
let baselineBooks,sourceInput,reviewed,applyBody,applied,liveOriginal,liveReceipt,canonical,queries={},freshOwner;
async function post(path,body,page=ownerPage){const r=await page.request.post(base+path,{headers:{origin:base},data:body});let value;try{value=await r.json();}catch{value=null;}trace.push({step,path,status:r.status(),keys:value&&typeof value==='object'?Object.keys(value).sort():[]});if(trace.length>24)trace.shift();need(r.status()>=200&&r.status()<300&&value?.ok!==false,`canonical ${path} HTTP${r.status()} ${value?.code??''}`);return value;}
async function read(path){return get(ownerPage,path);}
const noYears=value=>{const copy=structuredClone(value);delete copy.fiscalYears;return copy;};
async function books(){return{profile:await read(prefix+'company'),chart:await read(prefix+'accounts'),journal:noYears((await read(prefix+'journal?year=2026')).journal),trialBalance:noYears((await read(prefix+'trial-balance?year=2026')).trialBalance),balance:noYears((await read(prefix+'balance?year=2026')).balance)};}
async function liveOriginalProof(){const response=await ownerPage.request.get(base+prefix+`documents/${liveOriginal.id}/file`);need(response.status()===200&&hash(await response.body())===liveOriginal.sha256,'real live original retained unchanged');}
async function unchanged(){need(isDeepStrictEqual(await books(),baselineBooks),'archive actions never change live journal/UUID/financial rows/company profile/chart; fiscal-year discovery metadata is separate');await liveOriginalProof();}
async function setup(){step='normal-current-independent-admin-fixture';await post('/api/workspace/members/access',{userId:memberId,action:'set-role',workspaceRole:'workspace_owner'});await post('/api/workspace/members/company',{userId:memberId,action:'grant',companySlug:'synthetic',role:'owner'});need((await read(prefix+'scheduled-recurring')).scheduledRecurring.asOfDate>=to,'synthetic live source date is nonfuture');
 step='ordinary-live-ledger-and-actual-original-before-archive';for(const[accountNo,type,normalBalance]of[['1861','asset','debit'],['8861','equity','credit']])await post(prefix+'accounts',{accountNo,name:'Synthetic archive live '+accountNo,type,normalBalance});const bytes=Buffer.from('Synthetic live ledger source DKK10.00,2026-01-15; separate from closed history.\n');const registered=await post(prefix+'documents/ingest',{confirm:true,fileName:'synthetic-archive-live.txt',fileBase64:bytes.toString('base64'),metadata:{source:'Synthetic ordinary live original',documentType:'cash_register_receipt',issueDate:'2026-01-15',amountIncVat:10,vatAmount:0,currency:'DKK'}});liveOriginal={id:registered.document.id,sha256:hash(bytes)};liveReceipt=await post(prefix+'manual-journal',{confirm:true,input:{format:'rentemester-manual-journal-post/1',expectedRuleVersion:(await read(prefix+'journal')).journal.nativePosting.ruleVersion,idempotencyKey:'synthetic-archive-live-source',request:{transactionDate:'2026-01-15',text:'Synthetic separate live journal',currency:'DKK',documentId:String(liveOriginal.id),lines:[{accountNo:'1861',debitAmount:'10.00',creditAmount:'0.00'},{accountNo:'8861',debitAmount:'0.00',creditAmount:'10.00'}]}}});need(Number.isSafeInteger(liveReceipt.entryId)&&/^[0-9a-f]{64}$/.test(liveReceipt.entryHash),'genuine live source receipt');baselineBooks=await books();need(baselineBooks.journal.entries.length===1&&baselineBooks.profile.company.currency==='DKK','exact live baseline and explicit matching archive currency');const initial=await read(prefix+'archives');need(initial.ok===true&&initial.years.length===0,'no archive seeded into fixture');await unchanged();return{baselineBooks,liveOriginal,liveReceipt,sourceFiles:files.map(f=>({path:f.path,bytes:f.bytes.length,sha256:hash(f.bytes)}))};}
async function uiPlan(){diagnosticPage=ownerPage;step='actual-archive-ui-byte-exact-two-year-upload';await ownerPage.goto(base+'/companies/synthetic/arkiv');await ownerPage.getByRole('heading',{name:'Importér afsluttede regnskabsår',exact:true}).waitFor();need(await ownerPage.getByLabel('Historisk basisvaluta',{exact:true}).inputValue()==='','operator must explicitly choose historical currency');await ownerPage.getByLabel('Historisk basisvaluta',{exact:true}).fill('DKK');await ownerPage.getByLabel('Regnskabsår 1',{exact:true}).fill('2024');await ownerPage.getByLabel('Posteringer CSV 1',{exact:true}).setInputFiles({name:'Posteringer.csv',mimeType:'text/csv',buffer:files[0].bytes});await ownerPage.getByLabel('Saldobalance CSV 1 (valgfri)',{exact:true}).setInputFiles({name:'SaldoBalance.csv',mimeType:'text/csv',buffer:files[1].bytes});await ownerPage.getByLabel('Regnskabsår 2',{exact:true}).fill('2025');await ownerPage.getByLabel('Posteringer CSV 2',{exact:true}).setInputFiles({name:'Posteringer.csv',mimeType:'text/csv',buffer:files[2].bytes});const wait=ownerPage.waitForResponse(r=>new URL(r.url()).pathname===route+'plan'&&r.request().method()==='POST');await ownerPage.getByRole('button',{name:'Gennemgå arkivimport',exact:true}).click();const response=await wait;need(response.status()===200,'actual public readonly source plan');sourceInput=response.request().postDataJSON();reviewed=(await response.json()).plan;need(isDeepStrictEqual(Object.keys(sourceInput).sort(),['contentBase64','fileName']),'plan transports only original package');const bytes=Buffer.from(sourceInput.contentBase64,'base64'),pkg=JSON.parse(bytes.toString('utf8'));need(pkg.format==='rentemester-dinero-archive-source/1'&&pkg.currency==='DKK'&&pkg.files.length===3,'real native source package contract and explicit currency');for(const file of files){const part=pkg.files.find(f=>f.path===file.path);need(part&&Buffer.from(part.contentBase64,'base64').equals(file.bytes),'actual UI preserves every original BOM/CRLF/CSV byte');}need(reviewed.format==='rentemester-native-closed-history/1'&&reviewed.status==='ready'&&reviewed.source.sha256===hash(bytes)&&reviewed.source.sizeBytes===bytes.length&&reviewed.declaredCurrency==='DKK'&&reviewed.currencyDeclaration==='operator'&&reviewed.excludedLatestYear===2025&&reviewed.years.length===1&&reviewed.years[0].fiscalYear===2024&&reviewed.years[0].postingCount===7&&reviewed.years[0].balanceCount===2&&reviewed.years[0].hasBalances===true&&reviewed.years[0].status==='new','exact canonical reviewed source/counts/optional balances/exclusion');const dialog=ownerPage.getByRole('dialog',{name:'Bekræft arkivimport',exact:true});await dialog.getByText('Højeste år 2025 udelades',{exact:true}).waitFor();await dialog.getByText('Kilde og plan-hash',{exact:true}).click();await dialog.getByText(reviewed.planHash,{exact:true}).waitFor();await dialog.getByText(reviewed.source.sha256,{exact:true}).waitFor();need(await dialog.getByRole('button',{name:'Importér afsluttede år',exact:true}).isDisabled(),'separate explicit confirmation required');await unchanged();return{sourceInput,reviewed,sourcePackageSha256:hash(bytes)};}
function changedSource(year,fn){const pkg=JSON.parse(Buffer.from(sourceInput.contentBase64,'base64').toString('utf8')),file=pkg.files.find(f=>f.path===year+'/Posteringer.csv');file.contentBase64=Buffer.from(fn(Buffer.from(file.contentBase64,'base64').toString('utf8'))).toString('base64');return{...sourceInput,contentBase64:Buffer.from(JSON.stringify(pkg)).toString('base64')};}
async function refusals(){step='current-admin-revoked-after-readonly-review';await post('/api/workspace/members/company',{userId:ownerId,action:'grant',companySlug:'synthetic',role:'reader'},memberPage);const response=await ownerPage.request.post(base+route+'apply',{headers:{origin:base},data:{...sourceInput,planHash:reviewed.planHash,idempotencyKey:'synthetic-archive-denied',confirm:true}});need(response.status()===403&&(await response.json()).ok===false,'current reader cannot apply an earlier admin-reviewed import');await post('/api/workspace/members/company',{userId:ownerId,action:'grant',companySlug:'synthetic',role:'owner'},memberPage);need((await read(prefix+'archives')).years.length===0,'denial creates no archive');step='changed-excluded-original-invalidates-old-plan';const changed=changedSource('2025',text=>text.replace('Excluded latest source','Changed excluded source inventory'));const replan=(await post(route+'plan',changed)).plan;need(replan.source.sha256!==reviewed.source.sha256&&replan.planHash!==reviewed.planHash,'excluded-year bytes still bound by the complete source plan');const stale=await ownerPage.request.post(base+route+'apply',{headers:{origin:base},data:{...changed,planHash:reviewed.planHash,idempotencyKey:'synthetic-archive-stale-source',confirm:true}});need(stale.status()===422&&(await stale.json()).code==='archive_plan_changed','reviewed plan cannot authorize changed original bytes');need((await read(prefix+'archives')).years.length===0,'stale refusal remains atomic');await unchanged();return{currentRoleRefused:true,excludedSourceStalePlanRefused:true};}
async function apply(){step='actual-archive-ui-separate-confirmed-atomic-apply';const dialog=ownerPage.getByRole('dialog',{name:'Bekræft arkivimport',exact:true});await dialog.getByRole('checkbox',{name:'Jeg bekræfter valuta og de gennemgåede arkivår',exact:true}).check();const wait=ownerPage.waitForResponse(r=>new URL(r.url()).pathname===route+'apply'&&r.request().method()==='POST');await dialog.getByRole('button',{name:'Importér afsluttede år',exact:true}).click();const response=await wait;need(response.status()===200,'actual confirmed archive apply');applyBody=response.request().postDataJSON();applied=await response.json();need(applyBody.confirm===true&&typeof applyBody.idempotencyKey==='string'&&applyBody.planHash===reviewed.planHash&&applyBody.contentBase64===sourceInput.contentBase64,'one actual caller intent uses exact reviewed original/hash');need(applied.ok===true&&Number.isSafeInteger(applied.archiveImport.importId)&&applied.archiveImport.sourceSha256===reviewed.source.sha256&&applied.archiveImport.planHash===reviewed.planHash&&applied.archiveImport.excludedLatestYear===2025&&applied.archiveImport.declaredCurrency==='DKK'&&isDeepStrictEqual(applied.archiveImport.years,[{fiscalYear:2024,postingCount:7,balanceCount:2}])&&applied.idempotency?.replayed===false,'genuine import attestation/counts and durable caller receipt');await ownerPage.getByRole('heading',{name:'Arkivimport gennemført',exact:true}).waitFor();await unchanged();return{applied,applyBody};}
async function original(){const response=await ownerPage.request.get(base+route+applied.archiveImport.importId+'/original');const bytes=await response.body();need(response.status()===200&&response.headers()['content-type']==='application/json'&&bytes.equals(Buffer.from(sourceInput.contentBase64,'base64'))&&hash(bytes)===reviewed.source.sha256,'authorized raw original package bytes/SHA unchanged');return{ok:true,contentType:'application/json',bytes:[...bytes]};}
async function archiveRead(){const list=await read(prefix+'archives'),detail=await read(prefix+'archives/2024');need(list.ok===true&&list.years.length===1&&list.years[0].fiscalYear===2024&&detail.ok===true&&detail.years.length===1,'only installed2024 retained; highest2025 excluded');const year=detail.years[0];need(year.sourceSystem==='dinero'&&year.postingCount===7&&year.balanceCount===2&&year.importId===applied.archiveImport.importId&&year.sourceHash===reviewed.source.sha256&&year.parserContract==='rentemester-native-closed-history/1'&&year.declaredCurrency==='DKK'&&year.hasBalances===true,'real source attestation/original identity/currency provenance');need(year.postings.length===7&&year.balances.length===2,'complete exact year source rows');for(let i=0;i<cases.length;i++){const row=year.postings[i],c=cases[i];need(row.lineNo===i&&row.accountNo===c[0]&&row.accountName==='Synthetic archive '+c[0]&&row.transactionDate===c[4]&&row.voucher==='A-'+i&&row.voucherType==='Archive'&&row.text==='Synthetic raw '+i&&row.amount===c[3]&&row.sourceAmount===c[2],'signed exact six-place source and separately quantized Money row '+i);}need(year.postings[0].runningBalance===null&&year.postings[0].sourceRunningBalance===null&&year.postings[3].runningBalance==='-0.01'&&year.postings[3].sourceRunningBalance==='-0.005000'&&year.postings[6].runningBalance==='0.01'&&year.postings[6].sourceRunningBalance==='0.010000','nullable/raw running balance preserved independently of per-row reporting');need(year.balances[0].sourceAmount==='1.234567'&&year.balances[0].amount==='1.23'&&year.balances[1].sourceAmount==='-1.234567'&&year.balances[1].amount==='-1.23','closing reference balances preserve signed source micros');const years=(await read(prefix+'fiscal-years')).fiscalYears.years;need(years.some(y=>y.label==='2024'&&y.source==='archive')&&!years.some(y=>y.label==='2025'&&y.source==='archive'),'truthful native archive fiscal-year discovery');return{list,detail,original:await original(),fiscalYears:years};}
function cfoPath(q){const search=new URLSearchParams({scope:'company',companySlug:'synthetic',from,to,...q});return'/api/cfo-analytics?'+search;}
async function cfo(){step='actual-cfo-archive-and-live-full-source-projection';const definitions={all:{},halfcent:{account:'6200'},live:{account:'1861'},excluded:{from:'2025-01-01',to:'2025-12-31'},currency:{currency:'DKK'}};for(const[name,q]of Object.entries(definitions)){const response=await read(cfoPath(q));need(response.ok===true&&response.status==='ready'&&response.schemaVersion==='rentemester-cfo-analytics-v1','physically assured native CFO source');queries[name]={query:q,response};}const all=queries.all.response,archiveRows=all.rows.filter(r=>r.sourceType==='archive'),liveRows=all.rows.filter(r=>r.sourceType==='ledger');need(archiveRows.length===7&&liveRows.length===2&&all.reconciliation.rowCount===9&&all.reconciliation.amountByCurrency.DKK==='0.00'&&isDeepStrictEqual(all.reconciliation.sourceHashes,[reviewed.source.sha256,liveReceipt.entryHash].sort()),'complete real archive/live rows and exact whole matching totals');for(let i=0;i<archiveRows.length;i++){const row=archiveRows[i],c=cases[i];need(/^archive:[1-9][0-9]*:posting:[1-9][0-9]*$/.test(row.sourceId)&&row.sourceHash===reviewed.source.sha256&&row.accountNo===c[0]&&row.transactionDate===c[4]&&row.amount===c[3]&&row.currency==='DKK'&&row.journalEntryId===null&&row.documentId===null&&row.partyId===null&&row.partyName===null&&isDeepStrictEqual(row.dimensions,[]),'real attested archive identities/currency and truthful missing party/dimension');}need(liveRows.every(r=>r.sourceHash===liveReceipt.entryHash&&r.journalEntryId===liveReceipt.entryId&&r.documentHash===liveOriginal.sha256),'real live IDs/hashes separate from archive');need(queries.halfcent.response.rows.length===2&&queries.halfcent.response.rows.every(r=>r.amount==='0.01')&&queries.halfcent.response.reconciliation.amountByCurrency.DKK==='0.02','Money quantizes each half-cent before summing, never raw micros aggregate-round');need(queries.live.response.rows.length===1&&queries.live.response.rows[0].sourceType==='ledger'&&queries.live.response.reconciliation.amountByCurrency.DKK==='10.00','explicit account filter selects only genuine live source');need(queries.excluded.response.rows.length===0&&queries.excluded.response.reconciliation.rowCount===0,'highest-year marker is not installed or posted');need(isDeepStrictEqual(queries.currency.response.rows,all.rows),'explicit declared base-currency filter preserves source');const collected=[],pages=[];let cursor;for(let i=0;i<10;i++){const q={limit:2,...(cursor?{cursor}:{})},page=await read(cfoPath(q));need(isDeepStrictEqual(page.reconciliation,all.reconciliation),'full totals/hashes precede archive pagination');pages.push({query:q,response:page});collected.push(...page.rows);if(!page.page.nextCursor){need(isDeepStrictEqual(collected,all.rows),'stable pages reproduce every archive+live row exactly once');queries.page=pages[0];await unchanged();return{queries,pages,archiveActualQualified:true};}cursor=page.page.nextCursor;}throw Error('bounded archive CFO pages did not terminate');}
async function uiRead(){diagnosticPage=ownerPage;step='actual-archive-list-year-and-source-original-link';await ownerPage.goto(base+'/companies/synthetic/arkiv');const response=ownerPage.waitForResponse(r=>new URL(r.url()).pathname===prefix+'archives/2024');step='actual-archive-open-year';await ownerPage.getByRole('button',{name:'Vis arkivår 2024',exact:true}).click();need((await response).status()===200,'actual archive detail request');step='actual-archive-render-year';await ownerPage.getByRole('heading',{name:'Arkivår 2024',exact:true}).waitFor();step='actual-archive-original-download';const link=ownerPage.getByRole('link',{name:'Hent kildepakke '+applied.archiveImport.importId+' for år 2024',exact:true});need(await link.getAttribute('href')===route+applied.archiveImport.importId+'/original','actual source link bound to admitted company/import identity');const downloadWait=ownerPage.waitForEvent('download');await link.click();const download=await downloadWait,downloadPath=join(work,'source-download-'+responseSequence+'.json');await download.saveAs(downloadPath);need(readFileSync(downloadPath).equals(Buffer.from(sourceInput.contentBase64,'base64')),'actual browser download retains every original package byte');unlinkSync(downloadPath);step='actual-archive-negative-money-row';const detail=ownerPage.getByRole('region',{name:'Arkivår 2024',exact:true}),negative=detail.getByRole('row').filter({has:ownerPage.getByRole('cell',{name:'A-3 · Archive',exact:true})});need((await negative.innerText()).includes('-0.005000')&&(await negative.innerText()).includes('-0.01 DKK'),'rendered signed source six decimals and separate Money/currency');await original();await ownerPage.screenshot({path:process.env.RM_ARCHIVE_SCREENSHOT,fullPage:true});step='actual-cfo-archive-row-drilldown';await ownerPage.goto(base+'/cfo');await ownerPage.getByRole('heading',{name:'CFO-overblik',exact:true}).waitFor();const controls=ownerPage.getByRole('group',{name:'CFO-afgrænsning',exact:true});await controls.getByLabel('Virksomhed',{exact:true}).selectOption('synthetic');await controls.getByLabel('Fra dato',{exact:true}).fill(from);const wait=ownerPage.waitForResponse(r=>{const u=new URL(r.url());return u.pathname==='/api/cfo-analytics'&&u.searchParams.get('from')===from&&u.searchParams.get('to')===to;});await controls.getByLabel('Pr. dato',{exact:true}).fill(to);const got=await wait;need(got.status()===200&&isDeepStrictEqual(await got.json(),queries.all.response),'actual CFO company/date returns exact full attested archive/live projection');const rows=ownerPage.getByRole('region',{name:'Kildeposter',exact:true}).locator('tbody tr');await rows.first().waitFor();need(await rows.count()===9,'all archive and live source rows rendered');const one=queries.halfcent.response.rows[0],row=rows.filter({has:ownerPage.getByText('Kildedetaljer '+one.sourceId,{exact:true})});await row.getByRole('cell',{name:'0,01 kr.',exact:true}).waitFor();await row.getByText('Kildedetaljer '+one.sourceId,{exact:true}).click();await row.getByText(reviewed.source.sha256,{exact:true}).waitFor();await unchanged();return{actualArchiveYearUi:true,originalLinkAndBytes:true,actualCfoArchiveRows:true};}
async function replayAndConflict(){step='explicit-same-caller-key-replay-exact-retained-result';const before=await archiveRead();const result=await post(route+'apply',applyBody);need(isDeepStrictEqual(result.archiveImport,applied.archiveImport)&&result.idempotency?.replayed===true&&result.idempotency.receiptId===applied.idempotency.receiptId,'accepted exact durable receipt replay without another import');need(isDeepStrictEqual(await archiveRead(),before),'repeat retains all IDs/raw source/canonical rows');step='same-year-changed-micro-value-refuses-without-overwrite';const changed=changedSource('2024',text=>text.replace('0,004999','0,004998'));for(const[action,body]of[['plan',changed],['apply',{...changed,planHash:reviewed.planHash,idempotencyKey:'synthetic-archive-conflicting-year',confirm:true}]]){const response=await ownerPage.request.post(base+route+action,{headers:{origin:base},data:body});need(response.status()===422&&(await response.json()).ok===false,'changed same-year micros refuse even when quantized cents match');}need(isDeepStrictEqual(await archiveRead(),before),'conflict cannot modify retained history/originals/rows');await unchanged();return{sameKeyReceiptReplay:true,changedSameYearRawMicrosRefused:true};}
async function client(kind,op,body){const file=join(work,'client.cookie');writeFileSync(file,(await ownerPage.context().cookies(base)).map(c=>`${c.name}=${c.value}`).join('; '),{mode:0o600,flag:'wx'});try{const common=['--server',base,'--cookie-file',file,'--allow-loopback-http'];let argv,input;if(kind==='cli'){argv=['remote',op,...common,'--company','synthetic'];if(op==='archives'&&body?.fiscalYear)argv.push('--fiscal-year',String(body.fiscalYear));if(op==='archive-import-original')argv.push('--import-id',String(applied.archiveImport.importId));if(op==='archive-import-apply')argv.push('--confirm');if(['archive-import-plan','archive-import-apply'].includes(op))input=JSON.stringify(body);}else{argv=common;const name=op.replaceAll('-','_'),args={company:'synthetic',...(op==='archives'?body??{}:op==='archive-import-original'?{importId:applied.archiveImport.importId}:{body:JSON.stringify(body),...(op==='archive-import-apply'?{confirm:true}:{})})};input=[{jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'synthetic-archive-proof',version:'1'}}},{jsonrpc:'2.0',method:'notifications/initialized'},{jsonrpc:'2.0',id:2,method:'tools/call',params:{name,arguments:args}}].map(v=>JSON.stringify(v)).join('\n')+'\n';}const output=spawnSync(join(work,kind==='cli'?'rentemester-rust':'rentemester-mcp'),argv,{input,encoding:'utf8',timeout:45000,maxBuffer:32*1024*1024});need(!output.error&&output.status===0,'native archive client terminal '+op);const result=kind==='cli'?JSON.parse(output.stdout):output.stdout.trim().split('\n').map(v=>JSON.parse(v)).find(v=>v.id===2)?.result?.structuredContent;need(result?.ok===true&&result.data,'complete canonical archive client envelope');return result.data;}finally{unlinkSync(file);}}
async function clients(replay=true){step='exact-native-cli-mcp-archive-read-original-plan-parity';const current=await archiveRead(),plan=await post(route+'plan',sourceInput);need(plan.plan.years[0].status==='retained','fresh same-source plan truthfully identifies retained year');for(const kind of['cli','mcp']){for(const[op,body,expected]of[['archives',undefined,current.list],['archives',{fiscalYear:2024},current.detail],['archive-import-original',undefined,current.original],['archive-import-plan',sourceInput,plan]]){step='exact-'+kind+'-'+op+'-parity';const actual=await client(kind,op,body);need(isDeepStrictEqual(actual,expected),'full original/list/year/source-plan HTTP '+kind+' '+op+' parity');}if(replay){const r=await client(kind,'archive-import-apply',applyBody);need(isDeepStrictEqual(r.archiveImport,applied.archiveImport)&&r.idempotency?.replayed===true&&r.idempotency.receiptId===applied.idempotency.receiptId,'explicit native client accepted-key replay preserves exact original attestation');}}await unchanged();return{httpCliMcpFullArchiveParity:true,acceptedReceiptReplay:replay};}
async function snapshot(){const cfoViews={};for(const[name,item]of Object.entries(queries))cfoViews[name]=await read(cfoPath(item.query));return{archive:await archiveRead(),cfoViews,books:await books()};}
async function saveStable(){canonical=await snapshot();await unchanged();return{canonical,archiveActualQualified:true,fullSourcePackageRetained:true};}
async function assertStable(){need(isDeepStrictEqual(await snapshot(),canonical),'all archive rows/raw micros/source originals/IDs/CFO totals/pages and untouched live books/profile/chart reconstruct exactly');await unchanged();}
async function reopen(){step='fresh-password-mfa-restart-archive-and-cfo-reconstruction';await ownerPage.context().clearCookies();need((await login(ownerPage,owner)).user.id===ownerId,'same normal owner on reopen');await assertStable();await clients();return{freshMfaRestart:true,canonicalReconstruction:true};}
async function archive(){step='explicit-signed-retained-archive-live-source-snapshot';const r=await ownerPage.request.post(base+'/api/workspace/backup',{headers:{origin:base},data:{confirm:true}});need(r.status()===200&&r.headers()['content-type']==='application/vnd.rentemester.native-backup+json','nonempty native attested archive must remain supported in backup');const bytes=await r.body();need(bytes.length>0&&bytes.length<=96*1024*1024,'bounded full signed source inventory');writeFileSync(join(work,'archive-import.native-backup.json'),bytes,{mode:0o600,flag:'wx'});await assertStable();return{supported:true,sha256:hash(bytes),bytes:bytes.length};}
async function restored(){step='source-free-fresh-owner-normal-email-verification-mfa';ownerPage=await newPage();freshOwner={email:'restored@example.invalid',password:'synthetic-restored-password-789',codes:[]};await ownerPage.goto((await mail('verification',freshOwner.email)).actionUrl);const me=await enroll(ownerPage,freshOwner);need(me.user.id!==ownerId&&me.user.id!==memberId,'historical archive actor/receipt never grants current authority');await assertStable();await clients(false);await uiRead();return{sourceFreeRestoreQualified:true,freshOwnerMfa:true,originalRawMicrosAndCfoReconstructed:true,oldMutationReceiptReplayNotAttempted:true};}
async function restoredReopen(){step='restored-target-reopen-normal-mfa-no-new-actions';await ownerPage.context().clearCookies();await login(ownerPage,freshOwner);await assertStable();return{restoredTargetReopenQualified:true,immutableSourceReconstruction:true};}
for await(const line of createInterface({input:process.stdin,crlfDelay:Infinity})){try{const action=JSON.parse(line);let value;if(action==='authenticate')value=await authenticate();else if(action==='setup')value=await setup();else if(action==='plan')value=await uiPlan();else if(action==='refusals')value=await refusals();else if(action==='apply')value=await apply();else if(action==='read')value=await archiveRead();else if(action==='cfo')value=await cfo();else if(action==='ui')value=await uiRead();else if(action==='replay-conflict')value=await replayAndConflict();else if(action==='clients')value=await clients();else if(action==='save-stable')value=await saveStable();else if(action==='reopen')value=await reopen();else if(action==='archive')value=await archive();else if(action==='restored')value=await restored();else if(action==='restored-reopen')value=await restoredReopen();else if(action==='finish'){await browser?.close();emit({ok:true,value:{finished:true}});break;}else throw Error('unknown phase');emit({ok:true,value});}catch(error){let pageState=null;try{pageState=await Promise.race([diagnosticPage?.evaluate(()=>({pathname:location.pathname,headings:[...document.querySelectorAll('h1,h2,h3')].map(e=>e.textContent.slice(0,100)),buttons:[...document.querySelectorAll('button')].map(e=>e.textContent.slice(0,100)),alerts:document.querySelectorAll('[role=alert]').length})),new Promise(resolve=>setTimeout(()=>resolve(null),2000))]);}catch{}emit({ok:false,step,detail,errorType:error?.name,trace,pageState});await Promise.race([browser?.close().catch(()=>{}),new Promise(resolve=>setTimeout(resolve,2000))]);process.exitCode=1;break;}}

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
    report = {'status': 'running', 'sourceRevision': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=REPO, text=True).strip(), 'runnerSha256': digest(Path(__file__)), 'browserHelperSha256': hashlib.sha256(BROWSER.encode()).hexdigest(), 'canonicalOrigin': origin, 'noOfflineIdentitySeeds': True, 'ordinaryMacOsUser': True, 'ownedProfile': str(work), 'externalMailDelivered': False, 'workflow': 'closed_history_archive_import_and_cfo_reconstruction', 'archiveActualQualified': False, 'installedNativeQualified': False, 'screenshot': str(args.evidence.with_suffix('.png')),  'actions': [], 'results': {}, 'sourceFreeRestoreQualified': False}
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
                argv += ['--restore-backup', str(work / 'archive-import.native-backup.json'), '--trusted-backup-key', str(work / 'trusted.pub'), '--confirm-target', str(root), '--source-unavailable']
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
                ['openssl', 'req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', str(work / 'ca.key'), '-out', str(ca), '-days', '1', '-subj', '/CN=Synthetic Archive Import Auth CA', '-addext', 'basicConstraints=critical,CA:TRUE', '-addext', 'keyUsage=critical,keyCertSign,cRLSign'],
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
            config = {'runtimeProfile': 'local-user-macos', 'root': str(work / 'workspace'), 'brokerUid': os.getuid(), 'workspace': 'synthetic-native-archive-import', 'companySlug': 'synthetic', 'companyName': 'Synthetic Archive Company', 'cookieKey': key(), 'mfaKey': key(), 'operatorActors': ['system:synthetic-installer'], 'accounts': [], 'firstOwner': {'actor': 'system:synthetic-installer', 'email': 'owner@example.invalid', 'name': 'Synthetic Owner', 'password': 'synthetic-owner-password-123'}, 'backupSigningKeyFile': str(work / 'signing.der'), 'authMail': {'endpoint': f'https://127.0.0.1:{mail_port}/mail', 'bearerToken': bearer, 'from': 'auth@example.invalid', 'idempotencySecret': secrets.token_urlsafe(32), 'trustedCaFile': str(ca)}}
            write_config()
            start(create=True)
            script = work / 'browser.mjs'
            script.write_text(BROWSER)
            controller = subprocess.Popen(['/opt/homebrew/bin/bun', str(script)], cwd=REPO, env={**os.environ, 'RM_ARCHIVE_BASE': origin, 'RM_ARCHIVE_WORK': str(work), 'RM_ARCHIVE_SCREENSHOT': str(args.evidence.with_suffix('.png')), 'RM_ARCHIVE_PLAYWRIGHT': str((REPO / 'node_modules/@playwright/test/index.mjs').resolve(strict=True))}, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, bufsize=1, start_new_session=True)
            report['browserControllerPid'] = controller.pid
            for action in ['authenticate', 'setup', 'plan', 'refusals', 'apply', 'read', 'cfo', 'ui', 'replay-conflict', 'clients', 'save-stable']:
                phase(action)
            stop()
            config.pop('firstOwner')
            write_config()
            start()
            phase('reopen')
            archive = phase('archive')
            report['signedSnapshot'] = archive
            progress('operator:independent-signed-snapshot-verification')
            path = work / 'archive-import.native-backup.json'
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
            report.update({'status': 'passed', 'currentAction': 'complete', 'archiveActualQualified': True, 'currentRightsAndChangedSourceRefusalsQualified': True, 'initialAndRestartQualified': True, 'snapshotCoverage': 'independently-verified-source-free-restored-and-reopened'})
            save()
            print('PASS: real closed-history archive upload/review/import, canonical clients and CFO, source-free signed restore and reopen; ' + str(args.evidence))
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
