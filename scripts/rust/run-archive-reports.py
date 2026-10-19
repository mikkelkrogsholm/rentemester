#!/usr/bin/env python3
"""Owned normal-MFA ordinary archive reporting and source-free restore acceptance.

Public CSV import, explicit current chart, five views, live/archive collision,
exact raw/Money/large amounts, real CSV/PDF/client parity and current-source fences.
Only supplied artifacts and private synthetic profiles; no SQL/auth/business seeds.
Reads own independently verified signed SQLite snapshots query-only for audit proof.
Actual qualification requires every phase, fresh-owner restore and reopen to pass.
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
import sqlite3
import ssl
import subprocess
import threading
import time
import uuid
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
PREFIX = 'Rentemester-Synthetic-Archive-Reports-'
BROWSER = r'''




import {createHmac,createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {createInterface} from 'node:readline';
import {readFileSync,readdirSync,writeFileSync,unlinkSync} from 'node:fs';
import {join} from 'node:path';
import {isDeepStrictEqual} from 'node:util';
const {chromium}=await import(process.env.RM_REPORTS_PLAYWRIGHT);
const base=process.env.RM_REPORTS_BASE,work=process.env.RM_REPORTS_WORK;
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
async function newPage(){const context=await browser.newContext();const page=await context.newPage();page.setDefaultTimeout(45000);diagnosticPage=page;page.on('request',r=>{if(new URL(r.url()).pathname==='/api/auth/request-password-reset')trace.push({kind:'request',path:'/api/auth/request-password-reset',method:r.method()});});page.on('requestfailed',r=>{if(new URL(r.url()).pathname==='/api/auth/request-password-reset')trace.push({kind:'requestfailed',path:'/api/auth/request-password-reset',code:r.failure()?.errorText?.match(/net::[A-Z_]+/)?.[0]});});page.on('response',r=>{const path=new URL(r.url()).pathname;if(path.startsWith('/api/auth/')){trace.push({path:path.startsWith('/api/auth/reset-password/')?'/api/auth/reset-password/:token':path,status:r.status()});if(trace.length>24)trace.shift();}});page.on('response',r=>{const u=new URL(r.url());if(/^\/api\/companies\/[a-z0-9-]+\/(journal|trial-balance|income-statement|balance|overview|archives|archive-import)(\/|$)/.test(u.pathname)){trace.push({step,path:u.pathname,status:r.status(),method:r.request().method(),year:u.searchParams.get('year'),source:u.searchParams.get('source'),format:u.searchParams.get('format')});if(trace.length>32)trace.shift();}});return page;}
async function authenticate(){
 browser=await chromium.launch({headless:true});ownerPage=await newPage();memberPage=await newPage();
 step='owner-mail-verification';await ownerPage.goto((await mail('verification',owner.email)).actionUrl);const who=await enroll(ownerPage,owner);ownerId=who.user.id;
 step='ordinary-web-invitation';await ownerPage.goto(base+'/adgang');await ownerPage.getByRole('heading',{name:'Brugere og adgang',exact:true}).waitFor();await ownerPage.getByLabel('E-mail',{exact:true}).fill(member.email);await ownerPage.getByLabel(/^Rolle/).selectOption('reviewer');await ownerPage.getByRole('button',{name:'Send invitation',exact:true}).click();await ownerPage.getByText('Invitationen er sendt.',{exact:true}).waitFor();const invitation=await mail('workspace-invitation',member.email);const listed=await get(ownerPage,'/api/workspace/invitations');const item=listed.invitations.find(i=>i.email===member.email);need(item?.status==='delivery_confirmed','canonical invitation delivery');invitationId=item.invitationId;
 step='normal-invited-claim';await memberPage.goto(invitation.actionUrl);await memberPage.getByRole('heading',{name:'Acceptér invitation',exact:true}).waitFor();need(!new URL(memberPage.url()).hash,'invitation token removed from address');await memberPage.getByLabel('Navn',{exact:true}).fill('Synthetic Invited User');await memberPage.getByLabel('Adgangskode',{exact:true}).fill(member.password);const claim=memberPage.waitForResponse(r=>new URL(r.url()).pathname==='/api/invitations/claim'&&r.request().method()==='POST');await memberPage.getByRole('button',{name:'Acceptér invitation',exact:true}).click();const claimed=await claim;need(claimed.ok()&&!claimed.headers()['set-cookie'],'claim cannot mint session');await memberPage.getByText('Invitationen er accepteret.',{exact:true}).waitFor();need((await fetchJson(memberPage,'/api/companies/synthetic/trial-balance')).status===401,'claim not sufficient for business read');
 step='invited-mail-verification-existing-mfa';await memberPage.goto((await mail('verification',member.email)).actionUrl);const mwho=await enroll(memberPage,member);memberId=mwho.user.id;need(memberId!==ownerId,'independent ordinary users');baseline=await get(memberPage,'/api/companies/synthetic/trial-balance');
 return {ownerId,memberId,invitationId,ordinaryInvitationClaim:true,distinctNormalMfaUsers:true};
}
// Only these synthetic companies are admitted to this fixture.
const company='synthetic-archive-reports',shifted='synthetic-archive-shifted',slugs=[company,shifted],prefix=slug=>`/api/companies/${slug}/`,hash=b=>createHash('sha256').update(b).digest('hex');
const resources=[['journal','journal','posteringer','Posteringer'],['trial-balance','trialBalance','saldobalance','Saldobalance'],['income-statement','incomeStatement','resultatopgorelse','Resultatopgørelse'],['balance','balance','balance','Balance'],['overview','overview','','Overblik']];
const header='Konto;Kontonavn;Dato;Bilag;Bilagstype;Tekst;Momstype;Beløb;Saldo\r\n';
const sourceRows=[['1863','100000000000000.000000','100000000000000.00','LARGE','2024-02-01'],['8863','-100000000000000.000000','-100000000000000.00','LARGE','2024-02-01'],['4863','1.234567','1.23','MICRO','2024-03-01'],['3863','-1.234567','-1.23','MICRO','2024-03-01'],['4863','0.005000','0.01','HALF1','2024-04-01'],['3863','-0.005000','-0.01','HALF1','2024-04-01'],['4863','0.005000','0.01','HALF2','2024-04-01'],['3863','-0.005000','-0.01','HALF2','2024-04-01'],['1863','-0.004999','0.00','ZERO','2024-05-01'],['8863','0.004999','0.00','ZERO','2024-05-01']];
const postings=Buffer.from('\ufeff'+header+sourceRows.map((r,i)=>`${r[0]};Synthetic original ${r[0]};${r[4]};${r[3]};Archive;Synthetic source line ${i};;${r[1]};\r\n`).join(''));
const balances=Buffer.from('Konto;Kontonavn;Beløb\r\n1863;Synthetic original asset;100000000000000.000000\r\n2863;Synthetic original liability;-20.000000\r\n3863;Synthetic original income;-20.000000\r\n4863;Synthetic original expense;5.000000\r\n8863;Synthetic original equity;-99999999999967.000000\r\n9876;Synthetic unclassified closing;2.000000\r\n');
const files=[{path:'2023/Posteringer.csv',bytes:Buffer.from(header+'1863;Synthetic prior;2023-07-15;PRIOR;Archive;Observed prior source;;2.000000;\r\n8863;Synthetic prior offset;2023-07-15;PRIOR;Archive;Observed prior source;;-2.000000;\r\n')},{path:'2024/Posteringer.csv',bytes:postings},{path:'2024/SaldoBalance.csv',bytes:balances},{path:'2025/Posteringer.csv',bytes:Buffer.from(header+'9999;Excluded source;2025-01-01;EXCLUDE;Archive;Excluded latest year;;999.000000;\r\n')}];
let sourceInput,reviewed,applied,shiftedImport,liveOriginal,liveReceipt,baselineBooks,canonical,freshOwner,downloadSeq=0,archiveBeforeClassification;const stableExports={},liveYears={},currentDefaults={};
async function post(path,body,page=ownerPage){const response=await page.request.post(base+path,{headers:{origin:base},data:body});let value;try{value=await response.json();}catch{value=null;}trace.push({step,path,status:response.status(),keys:value&&typeof value==='object'?Object.keys(value).sort():[]});if(trace.length>24)trace.shift();need(response.status()>=200&&response.status()<300&&value?.ok!==false,`canonical ${path} HTTP${response.status()} ${value?.code??''}`);return value;}
async function read(path,page=ownerPage){return get(page,path);}
function path(resource,{slug=company,year='2024',source='archive',account}={}){return prefix(slug)+resource+'?'+new URLSearchParams({year,source,...(account?{account}:{})});}
async function view(resource,options){const envelope=await read(path(resource[0],options));need(envelope.ok===true&&envelope[resource[1]],'canonical '+resource[0]+' outer envelope');return envelope;}
// B's live adapter changes only discovery and its explicit no-carry-forward notice.
function noYears(value){const copy=structuredClone(value);delete copy.fiscalYears;delete copy.limitations;return copy;}
async function books(){const out={};for(const slug of slugs){out[slug]={profile:await read(prefix(slug)+'company'),chart:await read(prefix(slug)+'accounts')};for(const r of resources){const live=await view(r,{slug,source:'live',year:liveYears[slug]});out[slug][r[1]]=noYears(live[r[1]]);}}return out;}
async function unchanged(){need(isDeepStrictEqual(await books(),baselineBooks),'all report reads leave live books/UUIDs/current profile/chart unchanged apart from explicit fiscal-year discovery and native-ledger basis notice');const original=await ownerPage.request.get(base+prefix(company)+`documents/${liveOriginal.id}/file`);need(original.status()===200&&hash(await original.body())===liveOriginal.sha256,'real live original bytes unchanged');}
async function createAccount(slug,accountNo,type,normalBalance){return post(prefix(slug)+'accounts',{accountNo,name:'Synthetic current chart '+accountNo,type,normalBalance});}
async function setup(){step='ordinary-two-owner-explicit-company-and-chart-setup';await post('/api/workspace/members/access',{userId:memberId,action:'set-role',workspaceRole:'workspace_owner'});for(const slug of slugs){await post('/api/companies',{name:'Synthetic archived report '+slug,slug,fiscalYearStartMonth:slug===shifted?'7':'1',vatPeriodType:null});for(const userId of[ownerId,memberId])await post('/api/workspace/members/company',{userId,action:'grant',companySlug:slug,role:'owner'});for(const[a,t,n]of[['1863','asset','debit'],['2863','liability','credit'],['3863','income','credit'],['4863','expense','debit'],['8863','equity','credit']])await createAccount(slug,a,t,n);}
 step='real-original-backed-live2024-collision';const bytes=Buffer.from('Synthetic separate live source, 2024-08-15, DKK10.00; archive does not create this posting.\n');const uploaded=await post(prefix(company)+'documents/ingest',{confirm:true,fileName:'synthetic-archive-reports-live.txt',fileBase64:bytes.toString('base64'),metadata:{source:'Synthetic separate live evidence',documentType:'cash_register_receipt',issueDate:'2024-08-15',amountIncVat:10,vatAmount:0,currency:'DKK'}});liveOriginal={id:uploaded.document.id,sha256:hash(bytes)};liveReceipt=await post(prefix(company)+'manual-journal',{confirm:true,input:{format:'rentemester-manual-journal-post/1',expectedRuleVersion:(await read(prefix(company)+'journal')).journal.nativePosting.ruleVersion,idempotencyKey:'synthetic-archive-report-live',request:{transactionDate:'2024-08-15',text:'Synthetic LIVE COLLISION 10.00',currency:'DKK',documentId:String(liveOriginal.id),lines:[{accountNo:'1863',debitAmount:'10.00',creditAmount:'0.00'},{accountNo:'8863',debitAmount:'0.00',creditAmount:'10.00'}]}}});need(Number.isSafeInteger(liveReceipt.entryId),'genuine live journal receipt');for(const slug of slugs)currentDefaults[slug]=(await read(prefix(slug)+'journal')).journal.selectedYear;liveYears[company]='2024';liveYears[shifted]=currentDefaults[shifted];baselineBooks=await books();need(baselineBooks[company].journal.entries.length===1&&baselineBooks[company].journal.ledgerUuid!==baselineBooks[shifted].journal.ledgerUuid,'one live entry and separate company UUIDs');return{slugs,liveOriginal,liveReceipt,baselineBooks,sourceFiles:files.map(f=>({path:f.path,sha256:hash(f.bytes),bytes:f.bytes.length}))};}
async function importUi(){step='real-three-year-original-upload-separate-review-confirm';await ownerPage.goto(base+`/companies/${company}/arkiv`);await ownerPage.getByRole('heading',{name:'Importér afsluttede regnskabsår',exact:true}).waitFor();await ownerPage.getByLabel('Historisk basisvaluta',{exact:true}).fill('DKK');await ownerPage.getByRole('button',{name:'Tilføj regnskabsår',exact:true}).click();for(const[index,year]of['2023','2024','2025'].entries()){await ownerPage.getByLabel(`Regnskabsår ${index+1}`,{exact:true}).fill(year);for(const[kind,label]of[['Posteringer','Posteringer CSV'],['SaldoBalance','Saldobalance CSV']]){const f=files.find(f=>f.path===`${year}/${kind}.csv`);if(f)await ownerPage.getByLabel(`${label} ${index+1}${kind==='SaldoBalance'?' (valgfri)':''}`,{exact:true}).setInputFiles({name:kind+'.csv',mimeType:'text/csv',buffer:f.bytes});}}const planResponse=ownerPage.waitForResponse(r=>new URL(r.url()).pathname===prefix(company)+'archive-import/plan'&&r.request().method()==='POST');await ownerPage.getByRole('button',{name:'Gennemgå arkivimport',exact:true}).click();const planned=await planResponse;need(planned.status()===200,'genuine readonly original plan');sourceInput=planned.request().postDataJSON();reviewed=(await planned.json()).plan;const pkg=JSON.parse(Buffer.from(sourceInput.contentBase64,'base64').toString('utf8'));need(pkg.currency==='DKK'&&reviewed.source.sha256===hash(Buffer.from(sourceInput.contentBase64,'base64'))&&reviewed.excludedLatestYear===2025,'explicit immutable declaration and excluded latest original');for(const f of files)need(Buffer.from(pkg.files.find(x=>x.path===f.path).contentBase64,'base64').equals(f.bytes),'every original BOM/CRLF byte bound');const dialog=ownerPage.getByRole('dialog',{name:'Bekræft arkivimport',exact:true});await dialog.getByText('Højeste år 2025 udelades',{exact:true}).waitFor();await dialog.getByText('Kilde og plan-hash',{exact:true}).click();await dialog.getByText(reviewed.planHash,{exact:true}).waitFor();await dialog.getByText(reviewed.source.sha256,{exact:true}).waitFor();await dialog.getByRole('checkbox',{name:'Jeg bekræfter valuta og de gennemgåede arkivår',exact:true}).check();const appliedResponse=ownerPage.waitForResponse(r=>new URL(r.url()).pathname===prefix(company)+'archive-import/apply'&&r.request().method()==='POST');await dialog.getByRole('button',{name:'Importér afsluttede år',exact:true}).click();const response=await appliedResponse;need(response.status()===200,'confirmed public archive ingestion');applied=await response.json();need(applied.archiveImport.sourceSha256===reviewed.source.sha256&&applied.archiveImport.years.length===2,'both earlier years retained');await ownerPage.getByRole('heading',{name:'Arkivimport gennemført',exact:true}).waitFor();need(isDeepStrictEqual((await read(prefix(company)+'archives')).years.map(y=>y.fiscalYear).sort(),[2023,2024]),'highest source year is retained as inventory only, never installed');need(reviewed.years.some(y=>y.fiscalYear===2023&&y.postingCount===2&&y.balanceCount===0&&y.hasBalances===false)&&reviewed.years.some(y=>y.fiscalYear===2024&&y.postingCount===10&&y.balanceCount===6&&y.hasBalances===true),'reviewed complete counts and honest missing optional file');const shiftedPlan=(await post(prefix(shifted)+'archive-import/plan',sourceInput)).plan;shiftedImport=await post(prefix(shifted)+'archive-import/apply',{...sourceInput,planHash:shiftedPlan.planHash,idempotencyKey:'synthetic-archive-report-shifted',confirm:true});await unchanged();return{reviewed,applied,shiftedImport,sourceInput};}
function evidence(value,importId=applied.archiveImport.importId){need(value.source==='archive'&&value.selectedSource==='archive'&&value.archived===true&&value.archivedSource==='dinero'&&value.periodStart===null&&value.periodEnd===null&&value.asOfDate===null,'explicit archive selection never fabricates fiscal/closing bounds');need(value.evidence.sourceHash===reviewed.source.sha256&&value.evidence.importId===importId&&value.evidence.classificationSource==='current_chart'&&value.evidence.parserContract==='rentemester-native-closed-history/1'&&value.declaredCurrency==='DKK'&&value.companyIdentityBasis==='current','physical package provenance separate from current classification/identity');need(value.comparison.status==='unavailable'&&value.comparison.reason==='archive_period_bounds_unknown','no invented archived prior comparison');}
async function missingAndUnknown(){archiveBeforeClassification=await read(prefix(company)+'archives/2024');step='truthful-missing-closing-and-visible-unknown-classification';const result={};for(const r of resources){const value=(await view(r))[r[1]],missing=(await view(r,{year:'2023'}))[r[1]];evidence(value);evidence(missing);result[r[0]]={unknown:value,missing};if(r[0]==='journal'){need(value.completeness.status==='ready'&&missing.entries.length===1,'source journal available without closing balances');}else if(r[0]==='trial-balance'){need(value.totalDebit==='100000000000007.00'&&value.totalCredit==='100000000000007.00'&&value.balanced===true&&value.rows.some(x=>x.accountNo==='9876'&&x.type===null&&x.balance==='2.00'),'closing TB keeps unclassified actual source and exact beyond-safe-cent totals');need(missing.totalDebit===null&&missing.totalCredit===null&&missing.balanced===null&&missing.completeness.reasons.includes('missing_closing_balances'),'missing file is unavailable, not ready zero');}else{need(value.completeness.status==='incomplete'&&value.completeness.unclassifiedAccounts.some(x=>(typeof x==='string'?x:x.accountNo)==='9876'),'unknown classification remains visible');need(missing.completeness.status==='unavailable'&&missing.completeness.reasons.includes('missing_closing_balances'),'missing closing basis explicit');if(r[0]==='income-statement')need(value.totalIncome===null&&value.totalExpense===null&&value.result===null&&missing.result===null,'no partial P&L aggregate');if(r[0]==='balance')need(value.totalAssets===null&&value.periodResult===null&&value.balanced===null&&missing.totalAssets===null,'no partial balance scalar');if(r[0]==='overview')need(value.profitAndLoss.resultat===null&&missing.profitAndLoss.resultat===null,'no fabricated overview summary');}}await uiView(resources[2]);const notice=ownerPage.getByRole('status').filter({hasText:'Uklassificeret konto 9876'});await notice.waitFor();await notice.getByText('Kildebeløb for konto 9876',{exact:true}).click();await notice.getByText(/2\.000000/).waitFor();for(const r of[resources[2],resources[3]]){const refused=await ownerPage.request.get(base+exportPath(r[0],'csv'));need(refused.status()===400,'unknown classification refuses incomplete financial export');}await unchanged();return result;}
async function classify(){step='explicit-current-chart-resolves-unknown-no-historical-rewrite';for(const slug of slugs)await createAccount(slug,'9876','expense','debit');need(isDeepStrictEqual(await read(prefix(company)+'archives/2024'),archiveBeforeClassification),'current classification never rewrites historical original names/raw rows/provenance');baselineBooks=await books();const result=await reportAssertions();return{explicitCurrentChartClassification:true,result};}
async function reportAssertions(){const reports={};for(const r of resources){const envelope=await view(r),value=envelope[r[1]];evidence(value);need(value.completeness.status==='ready','complete supported closing source after explicit chart resolution');reports[r[0]]=envelope;}const j=reports.journal.journal;need(isDeepStrictEqual(j.fiscalYears.filter(y=>y.label==='2024').map(y=>y.source).sort(),['archive','live']),'same-label year sources remain distinct in inventory');const defaultRead=(await read(prefix(company)+'journal')).journal;need(defaultRead.archived===false&&defaultRead.selectedYear===currentDefaults[company],'omitted year keeps current live default after import');need(j.entries.length===5&&j.entries.every(e=>e.id===null&&e.journalEntryId===null&&e.documentId===null&&e.sourceId.startsWith('archive:'+reviewed.source.sha256+':2024:'))&&!('nativePosting'in j),'voucher grouping is source identity only, no native financial action identity');const lines=j.entries.flatMap(e=>e.lines).sort((a,b)=>a.lineNo-b.lineNo);need(lines.length===sourceRows.length,'all raw rows preserved');for(let i=0;i<lines.length;i++){const line=lines[i],expected=sourceRows[i];need(line.lineNo===i&&line.accountNo===expected[0]&&line.sourceAmount===expected[1]&&line.balance===expected[2]&&line.date===expected[4]&&line.journalLineId===null&&line.sourceId===`archive:${reviewed.source.sha256}:2024:posting:${i}`,'exact source micro/Money/null immutable-journal-line row '+i);}const drill=await view(resources[0],{account:'4863'});need(drill.journal.entries.length===3&&drill.journal.entries.every(e=>e.lines.some(l=>l.accountNo==='4863')&&e.lines.length===2)&&drill.journal.accountFilter.accountNo==='4863','account drilldown retains whole actual vouchers');reports.drill=drill;const t=reports['trial-balance'].trialBalance;need(t.basis==='closing_balance'&&t.totalDebit==='100000000000007.00'&&t.totalCredit==='100000000000007.00'&&t.rows.length===6&&t.balanced===true,'closing reference basis is not movement turnover');const p=reports['income-statement'].incomeStatement;need(p.totalIncome==='20.00'&&p.totalExpense==='7.00'&&p.result==='13.00'&&p.priorYearLabel===null&&p.priorTotalIncome===null&&p.priorTotalExpense===null&&p.priorResult===null&&p.income.every(x=>x.priorAmount===null)&&p.expense.every(x=>x.priorAmount===null),'exact closing classification, no false archived prior');const b=reports.balance.balance;need(b.totalAssets==='100000000000000.00'&&b.liabilities.total==='20.00'&&b.periodResult==='13.00'&&b.equity.total==='99999999999980.00'&&b.totalLiabilitiesAndEquity===b.totalAssets&&b.balanced===true&&b.equity.lines.filter(x=>x.name==='Årets resultat').length===1&&b.priorAsOfDate===null&&b.priorTotalLiabilitiesAndEquity===null&&b.assets.priorTotal===null&&b.liabilities.priorTotal===null&&b.equity.priorTotal===null,'result folded once into exact balanced equity');const o=reports.overview.overview;need(o.profitAndLoss.omsaetning==='20.00'&&o.profitAndLoss.udgifter==='7.00'&&o.profitAndLoss.resultat==='13.00'&&isDeepStrictEqual(o.profitAndLoss.months.map(x=>x.month),['2024-02','2024-03','2024-04','2024-05'])&&o.operationalState==='unavailable'&&o.bank.balance===null&&o.receivables.openTotal===null&&o.vat===null&&o.exceptions.count===null&&o.keyFigures.bruttomargin===null&&o.keyFigures.egenkapitalandel===null&&o.bank.actualBalance===null&&o.bank.difference===null&&o.bank.bankStatementStatus==='unavailable'&&o.receivables.openCount===null&&o.exceptions.status==='unavailable','observed dated activity only; no invented operational/tax/receivable/ratio evidence');for(const r of resources){const implicit=await read(prefix(company)+r[0]+'?year=2024'),live=await view(r,{source:'live'});need(isDeepStrictEqual(implicit,live)&&live[r[1]].archived===false,'omitted source keeps live precedence for colliding year '+r[0]);reports['live-'+r[0]]=live;need(isDeepStrictEqual(await read(prefix(company)+r[0]+'?year=2023'),await view(r,{year:'2023'})),'omitted source resolves genuine archive-only year without invented live figures');const shiftedView=(await view(r,{slug:shifted}))[r[1]];evidence(shiftedView,shiftedImport.archiveImport.importId);need(shiftedView.company.fiscalYearStartMonth===7&&shiftedView.periodStart===null&&shiftedView.periodEnd===null,'shifted current settings never invent archive interval');reports['shifted-'+r[0]]=shiftedView;}const liveP=reports['live-income-statement'].incomeStatement;need(liveP.priorYearLabel===null&&liveP.priorTotalIncome===null&&liveP.priorTotalExpense===null&&liveP.priorResult===null,'archive2023 cannot fabricate previous native live comparison');return reports;}
async function canonicalOriginal(slug=company,importId=applied.archiveImport.importId){const response=await ownerPage.request.get(base+prefix(slug)+`archive-import/${importId}/original`),bytes=await response.body();need(response.status()===200&&response.headers()['content-type']==='application/json'&&bytes.equals(Buffer.from(sourceInput.contentBase64,'base64'))&&hash(bytes)===reviewed.source.sha256,'authorized retained package exact original bytes');return{sha256:hash(bytes),bytes:bytes.length};}
async function download(page,link,label){const href=await link.getAttribute('href');need(href?.startsWith('/api/companies/'+company+'/'),'company-scoped actual download');const waiting=page.waitForEvent('download');await link.click();const file=await waiting,local=join(work,`report-download-${++downloadSeq}`);await file.saveAs(local);const bytes=readFileSync(local);unlinkSync(local);need(bytes.length>0&&bytes.length<=32*1024*1024,'bounded complete '+label+' download');return{href,bytes};}
function exportPath(report,format,options={}){const{account,...rest}=options;return path(report+'/export',{...rest,account})+'&format='+format;}
function exportText(format,bytes){if(format==='csv'){need(bytes.subarray(0,3).equals(Buffer.from([239,187,191])),'CSV UTF8BOM');return bytes.toString('utf8');}need(bytes.subarray(0,5).toString()==='%PDF-','real PDF bytes');const read=spawnSync('/opt/homebrew/bin/pdftotext',['-','-'],{input:bytes,timeout:15000,maxBuffer:8*1024*1024});need(!read.error&&read.status===0,'actual PDF accounting text extraction');return read.stdout.toString('utf8');}
function normalizedExport(text){return text.replace(/Udtrukket[; :]+\d{4}-\d{2}-\d{2}/g,'Udtrukket <current-export-date>').replace(/\s+/g,' ').trim();}
async function exportChecks(){step='real-source-preserving-four-report-csv-and-pdf-downloads';const result={};for(const r of resources.filter(r=>r[0]!=='overview'))for(const format of['csv','pdf']){const response=await ownerPage.request.get(base+exportPath(r[0],format)),bytes=await response.body();need(response.status()===200&&response.headers()['content-type'].split(';')[0]===(format==='csv'?'text/csv':'application/pdf'),'actual authorized '+r[0]+' '+format+' export');const text=exportText(format,bytes);need(text.includes('2024')&&text.includes('DKK'),'source year/currency in export');const expected=r[0]==='income-statement'?'13,00':r[0]==='trial-balance'?(format==='pdf'?'100.000.000.000.007,00':'100000000000007,00'):(format==='pdf'?'100.000.000.000.000,00':'100000000000000,00');need(text.includes(reviewed.source.sha256),'export includes exact source package provenance');need(text.includes(expected),'export exact decimal accounting figure, never unsafe Number rounding');const key=r[0]+':'+format,proof={sha256:hash(bytes),bytes:bytes.length,contentType:response.headers()['content-type'],text:normalizedExport(text)};if(stableExports[key])need(stableExports[key].text===proof.text,'accounting export reconstructs unchanged source excluding current export date');else stableExports[key]=proof;result[key]=proof;}
 for(const r of resources.filter(r=>!['journal','overview'].includes(r[0])))for(const options of[{year:'2023'}]){const response=await ownerPage.request.get(base+exportPath(r[0],'csv',options));need(response.status()===400,'missing closing basis refuses export instead of exporting live/zero figures');}await unchanged();return result;}
async function uiView(r,year='2024'){step='ui-'+r[0]+'-'+year+'-navigate';diagnosticPage=ownerPage;const response=ownerPage.waitForResponse(x=>{const u=new URL(x.url());return u.pathname===prefix(company)+r[0]&&u.searchParams.get('year')===year&&u.searchParams.get('source')==='archive';});await ownerPage.goto(base+`/companies/${company}/${r[2]}?year=${year}&source=archive`);need((await response).status()===200,'ordinary '+r[0]+' explicit archive request');step='ui-'+r[0]+'-'+year+'-render';await ownerPage.getByRole('heading',{name:r[3],exact:true}).waitFor();await ownerPage.getByRole('region',{name:'Rapportgrundlag',exact:true}).getByText(/Arkiveret afslutningssnapshot; dato og regnskabsperiodens grænser er ikke oplyst/).waitFor();await ownerPage.getByText(/Sammenligningstal er ikke tilgængelige:/).waitFor();need(new URL(ownerPage.url()).searchParams.get('source')==='archive','archive source stays in route');await ownerPage.getByText(`Arkiveret regnskabsår ${year} — skrivebeskyttet.`,{exact:true}).waitFor();return ownerPage;}
async function ui(){step='all-five-ordinary-archive-views-and-source-preserving-drilldown';const result={};for(const r of resources){await uiView(r);const report=(await view(r))[r[1]];need(report.archived===true,'visible archive source is canonical');const links=ownerPage.getByRole('navigation',{name:'Virksomhedsnavigation',exact:true}).locator('a[href*="year=2024"]');need(await links.count()>0,'ordinary company navigation present');for(const link of await links.all()){const href=await link.getAttribute('href');need(new URL(href,base).searchParams.get('source')==='archive','all company report navigation preserves explicit source');}if(r[0]==='trial-balance'||r[0]==='balance')need((await Promise.all((await ownerPage.getByText('100.000.000.000.000,00 kr.',{exact:true}).all()).map(x=>x.isVisible()))).some(Boolean),'visible exact beyond-safe-cent amount');if(r[0]==='income-statement')need((await Promise.all((await ownerPage.getByText('13,00 kr.',{exact:true}).all()).map(x=>x.isVisible()))).some(Boolean),'visible exact result');if(r[0]!=='overview'){for(const format of['csv','pdf']){step='ui-'+r[0]+'-download-'+format;const link=ownerPage.getByRole('link',{name:format==='csv'?'Hent CSV':'Hent PDF',exact:true});const actual=await download(ownerPage,link,r[0]+' '+format),url=new URL(actual.href,base);need(url.searchParams.get('source')==='archive'&&url.searchParams.get('year')==='2024'&&url.searchParams.get('format')===format,'UI export URL preserves exact source/year/format');need(normalizedExport(exportText(format,actual.bytes))===stableExports[r[0]+':'+format].text,'actual browser downloaded same canonical source export');}}result[r[0]]={ordinaryView:true,canonicalSource:true};}
 await uiView(resources[1]);const accountLink=ownerPage.locator(`a[href*="account=4863"][href*="source=archive"]`);need(await accountLink.count()===1,'one exact source/account drilldown link');const wait=ownerPage.waitForResponse(x=>{const u=new URL(x.url());return u.pathname===prefix(company)+'journal'&&u.searchParams.get('account')==='4863'&&u.searchParams.get('source')==='archive';});await accountLink.click();need((await wait).status()===200,'real account drilldown request');await ownerPage.getByRole('heading',{name:'Posteringer',exact:true}).waitFor();const url=new URL(ownerPage.url());need(url.searchParams.get('year')==='2024'&&url.searchParams.get('source')==='archive'&&url.searchParams.get('account')==='4863','drilldown retains source/year/account');await uiView(resources[2],'2023');need((await Promise.all((await ownerPage.getByText(/missing_closing_balances|SaldoBalance/).all()).map(x=>x.isVisible()))).some(Boolean),'missing closing evidence visible');await uiView(resources[0]);const sourceSelect=ownerPage.getByRole('combobox',{name:'Vælg regnskabsår',exact:true});need(await sourceSelect.inputValue()==='archive:2024','explicit colliding archive selector');let switched=ownerPage.waitForResponse(r=>{const u=new URL(r.url());return u.pathname===prefix(company)+'journal'&&u.searchParams.get('year')==='2024'&&u.searchParams.get('source')==='live';});await sourceSelect.selectOption('live:2024');need((await switched).status()===200,'ordinary selector switches to distinct colliding live source');await ownerPage.getByRole('button',{name:/Synthetic LIVE COLLISION 10\.00/}).waitFor();switched=ownerPage.waitForResponse(r=>{const u=new URL(r.url());return u.pathname===prefix(company)+'journal'&&u.searchParams.get('year')==='2024'&&u.searchParams.get('source')==='archive';});await sourceSelect.selectOption('archive:2024');need((await switched).status()===200,'ordinary selector restores archive source');step='ui-archive-journal-raw-source-voucher';const micro=ownerPage.getByRole('listitem').filter({has:ownerPage.getByText('MICRO',{exact:true})});need(await micro.count()===1,'exact archived voucher scope');await micro.getByRole('button',{name:/MICRO/}).click();const rawRow=micro.getByRole('row').filter({has:ownerPage.getByRole('cell',{name:'4863',exact:true})});await rawRow.getByText('Original kildepost',{exact:true}).click();await rawRow.getByText(/Originalbeløb: 1\.234567/).waitFor();need(await micro.getByRole('button',{name:'Tilbagefør postering',exact:true}).count()===0,'archive has no reversal action');step='ui-archive-provenance-original-download';const basis=ownerPage.getByRole('region',{name:'Rapportgrundlag',exact:true});await basis.getByText('Arkivkildens dokumentation',{exact:true}).click();await basis.getByText(reviewed.source.sha256,{exact:true}).waitFor();const sourceDownload=await download(ownerPage,basis.getByRole('link',{name:'Hent original arkivkilde',exact:true}),'retained archive package');need(sourceDownload.href===prefix(company)+`archive-import/${applied.archiveImport.importId}/original`&&sourceDownload.bytes.equals(Buffer.from(sourceInput.contentBase64,'base64')),'real provenance original download uses exact admitted source package');await ownerPage.screenshot({path:process.env.RM_REPORTS_SCREENSHOT,fullPage:true});await canonicalOriginal();await unchanged();return{...result,drilldownSourcePreserved:true,missingClosingVisible:true};}
async function rights(){step='current-revocation-refuses-all-five-views-exports-and-original';await post('/api/workspace/members/company',{userId:ownerId,action:'revoke',companySlug:company},memberPage);for(const endpoint of[...resources.map(r=>path(r[0])),exportPath('balance','csv'),exportPath('journal','pdf'),prefix(company)+`archive-import/${applied.archiveImport.importId}/original`]){const response=await ownerPage.request.get(base+endpoint);need([403,404].includes(response.status()),'current revoked access refuses '+endpoint);}await post('/api/workspace/members/company',{userId:ownerId,action:'grant',companySlug:company,role:'owner'},memberPage);await unchanged();return{currentRevocationAllReportsOriginalExports:true};}
async function tampered(){step='physical-source-tamper-refuses-never-falls-back-to-live';const refusals=[];for(const endpoint of[...resources.map(r=>path(r[0])),exportPath('journal','csv'),exportPath('balance','pdf'),prefix(company)+`archive-import/${applied.archiveImport.importId}/original`]){const response=await ownerPage.request.get(base+endpoint);need(response.status()===503,'corrupt original refuses '+endpoint);const value=await response.json();need(value.ok===false,'corrupt original failure envelope');refusals.push({path:endpoint,status:response.status(),code:value.code});}return{physicalOriginalTamperRefused:true,noLiveFallback:true,refusals};}
async function snapshot(){const reports={};for(const r of resources)for(const year of['2023','2024'])reports[r[0]+':'+year]=await view(r,{year});return{reports,ready:await reportAssertions(),archive:await read(prefix(company)+'archives/2024'),original:await canonicalOriginal(),shiftedOriginal:await canonicalOriginal(shifted,shiftedImport.archiveImport.importId),books:await books()};}
async function saveStable(){canonical=await snapshot();await unchanged();return{canonical,originalSha256:reviewed.source.sha256};}
async function assertStable(){need(isDeepStrictEqual(await snapshot(),canonical),'all exact source/report/UUID/config/current-chart projections reconstructed');await unchanged();}
async function reopen(){step='fresh-normal-mfa-reopen-reconstructs-archive-and-source-collision';await ownerPage.context().clearCookies();need((await login(ownerPage,owner)).user.id===ownerId,'same verified source owner');await assertStable();await exportChecks();await clients();return{freshMfaRestart:true,canonicalReconstruction:true};}
async function archive(name){step='explicit-signed-report-snapshot-'+name;const r=await ownerPage.request.post(base+'/api/workspace/backup',{headers:{origin:base},data:{confirm:true}});need(r.status()===200&&r.headers()['content-type']==='application/vnd.rentemester.native-backup+json','supported full retained archive report snapshot');const bytes=await r.body();need(bytes.length>0&&bytes.length<=96*1024*1024,'complete signed inventory bounded');writeFileSync(join(work,'archive-reports-'+name+'.native-backup.json'),bytes,{mode:0o600,flag:'wx'});return{sha256:hash(bytes),bytes:bytes.length};}
async function restored(){step='source-free-fresh-owner-normal-verification-mfa-report-reconstruction';ownerPage=await newPage();freshOwner={email:'restored@example.invalid',password:'synthetic-restored-password-789',codes:[]};await ownerPage.goto((await mail('verification',freshOwner.email)).actionUrl);const me=await enroll(ownerPage,freshOwner);need(me.user.id!==ownerId&&me.user.id!==memberId,'historical report actor is not fresh authority');await assertStable();await clients();await exportChecks();await ui();return{sourceFreeRestoreQualified:true,freshOwnerMfa:true,allFiveArchiveReportsReconstructed:true};}
async function restoredReopen(){step='fresh-restored-owner-reopen-no-new-actions';await ownerPage.context().clearCookies();await login(ownerPage,freshOwner);await assertStable();await clients();return{restoredTargetReopenQualified:true};}
async function client(kind,resource,options={}){const cookie=join(work,'report-client.cookie');writeFileSync(cookie,(await ownerPage.context().cookies(base)).map(c=>`${c.name}=${c.value}`).join('; '),{mode:0o600,flag:'wx'});try{const common=['--server',base,'--cookie-file',cookie,'--allow-loopback-http'];const args={company,year:'2024',source:'archive',...options};let argv,input;if(kind==='cli'){argv=['remote',resource,...common,'--company',args.company,'--year',args.year,'--source',args.source];for(const name of['account','report','format'])if(args[name])argv.push('--'+name,args[name]);}else{argv=common;input=[{jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'synthetic-archive-reports',version:'1'}}},{jsonrpc:'2.0',method:'notifications/initialized'},{jsonrpc:'2.0',id:2,method:'tools/call',params:{name:resource.replaceAll('-','_'),arguments:args}}].map(v=>JSON.stringify(v)).join('\n')+'\n';}const process=spawnSync(join(work,kind==='cli'?'rentemester-rust':'rentemester-mcp'),argv,{input,encoding:'utf8',timeout:45000,maxBuffer:32*1024*1024});need(!process.error&&process.status===0,'native read client terminal '+resource);const value=kind==='cli'?JSON.parse(process.stdout):process.stdout.trim().split('\n').map(v=>JSON.parse(v)).find(v=>v.id===2)?.result?.structuredContent;need(value?.ok===true&&value.data,'complete canonical native read envelope');return value.data;}finally{unlinkSync(cookie);}}
async function clients(){step='full-http-cli-mcp-source-and-export-parity';for(const kind of['cli','mcp']){for(const r of resources)for(const source of['archive','live']){const current=await view(r,{source});need(isDeepStrictEqual(await client(kind,r[0],{source}),current),'full '+kind+' '+source+' '+r[0]+' parity');}need(isDeepStrictEqual(await client(kind,'journal',{account:'4863'}),await view(resources[0],{account:'4863'})),'exact source account drilldown client parity');for(const r of resources.filter(r=>r[0]!=='overview'))for(const format of['csv','pdf']){const http=await ownerPage.request.get(base+exportPath(r[0],format));need(http.status()===200,'current native export read');const expected=await http.body(),actual=await client(kind,'report-export',{report:r[0],format});need(actual.ok===true&&actual.contentType===http.headers()['content-type']&&Buffer.from(actual.bytes).equals(expected),'byte-exact '+kind+' '+r[0]+' '+format+' source-preserving export');}}await unchanged();return{fiveReportsBothSources:true,accountDrilldown:true,eightByteExactExports:true,httpCliMcpCanonicalParity:true};}
for await(const line of createInterface({input:process.stdin,crlfDelay:Infinity})){try{const action=JSON.parse(line);let value;if(action==='authenticate')value=await authenticate();else if(action==='setup')value=await setup();else if(action==='import')value=await importUi();else if(action==='unknown-missing')value=await missingAndUnknown();else if(action==='classify')value=await classify();else if(action==='reports')value=await reportAssertions();else if(action==='exports')value=await exportChecks();else if(action==='ui')value=await ui();else if(action==='rights')value=await rights();else if(action==='clients')value=await clients();else if(action==='save-stable')value=await saveStable();else if(action==='tampered')value=await tampered();else if(action==='reopen')value=await reopen();else if(action==='archive-before')value=await archive('before');else if(action==='archive')value=await archive('after');else if(action==='restored')value=await restored();else if(action==='restored-reopen')value=await restoredReopen();else if(action==='finish'){await browser?.close();emit({ok:true,value:{finished:true}});break;}else throw Error('unknown phase');emit({ok:true,value});}catch(error){let pageState=null;try{pageState=await Promise.race([diagnosticPage?.evaluate(()=>({pathname:location.pathname,headings:[...document.querySelectorAll('h1,h2,h3,h4')].map(e=>e.textContent.slice(0,100)),buttons:[...document.querySelectorAll('button')].map(e=>e.textContent.slice(0,100)),alerts:document.querySelectorAll('[role=alert]').length})),new Promise(resolve=>setTimeout(()=>resolve(null),2000))]);}catch{}emit({ok:false,step,detail,errorType:error?.name,trace,pageState});await Promise.race([browser?.close().catch(()=>{}),new Promise(resolve=>setTimeout(resolve,2000))]);process.exitCode=1;break;}}

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
    if not Path('/opt/homebrew/bin/pdftotext').is_file():
        parser.error('Existing pdftotext required for actual exported accounting text checks')
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
    report = {'status': 'running', 'sourceRevision': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=REPO, text=True).strip(), 'runnerSha256': digest(Path(__file__)), 'browserHelperSha256': hashlib.sha256(BROWSER.encode()).hexdigest(), 'canonicalOrigin': origin, 'noOfflineIdentitySeeds': True, 'ordinaryMacOsUser': True, 'ownedProfile': str(work), 'externalMailDelivered': False, 'workflow': 'ordinary_archived_company_reports', 'archiveReportsActualQualified': False, 'installedNativeQualified': False, 'screenshot': str(args.evidence.with_suffix('.png')),  'actions': [], 'results': {}, 'sourceFreeRestoreQualified': False}
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
                argv += ['--restore-backup', str(work / 'archive-reports-after.native-backup.json'), '--trusted-backup-key', str(work / 'trusted.pub'), '--confirm-target', str(root), '--source-unavailable']
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

        def inspect_audit(name):
            progress('operator:verify-and-read-own-signed-company-audit-' + name)
            path = work / ('archive-reports-' + name + '.native-backup.json')
            receipt = report['results']['archive-before' if name == 'before' else 'archive']
            if digest(path) != receipt['sha256']:
                raise RuntimeError('Downloaded signed snapshot changed')
            verified = subprocess.run([str(pinned['server_binary']), '--verify-backup', str(path), '--trusted-backup-key', str(work / 'trusted.pub')], cwd=REPO, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=60)
            if verified.returncode != 0:
                raise RuntimeError('Independent snapshot signature/inventory refused')
            wire = json.loads(path.read_bytes())
            result = {}
            for slug in ['synthetic-archive-reports', 'synthetic-archive-shifted']:
                entries = [e for e in wire['entries'] if e['path'] == slug + '/data/ledger.sqlite']
                if len(entries) != 1:
                    raise RuntimeError('Exact signed owned company ledger required')
                raw = base64.b64decode(entries[0]['bytesBase64'], validate=True)
                if not 0 < len(raw) <= 32 * 1024 * 1024:
                    raise RuntimeError('Signed company snapshot bound')
                copy = work / ('readonly-' + slug + '-' + name + '.sqlite')
                try:
                    with copy.open('xb') as output:
                        output.write(raw)
                    # Never a live SQLite handle and never a source fixture writer.
                    db = sqlite3.connect(copy.as_uri() + '?mode=ro', uri=True)
                    try:
                        db.execute('PRAGMA query_only=ON')
                        cursor = db.execute('SELECT * FROM audit_log ORDER BY id')
                        columns = [c[0] for c in cursor.description]
                        result[slug] = [dict(zip(columns, row)) for row in cursor.fetchall()]
                    finally:
                        db.close()
                finally:
                    copy.unlink(missing_ok=True)
            report.setdefault('auditProof', {})[name] = {slug: {'rows': len(rows), 'sha256': hashlib.sha256(json.dumps(rows, sort_keys=True).encode()).hexdigest()} for slug, rows in result.items()}
            save()
            return result

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
                ['openssl', 'req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', str(work / 'ca.key'), '-out', str(ca), '-days', '1', '-subj', '/CN=Synthetic Archive Reports Auth CA', '-addext', 'basicConstraints=critical,CA:TRUE', '-addext', 'keyUsage=critical,keyCertSign,cRLSign'],
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
            config = {'runtimeProfile': 'local-user-macos', 'root': str(work / 'workspace'), 'brokerUid': os.getuid(), 'workspace': 'synthetic-native-archive-reports', 'companySlug': 'synthetic', 'companyName': 'Synthetic Archive Reports Anchor', 'cookieKey': key(), 'mfaKey': key(), 'operatorActors': ['system:synthetic-installer'], 'accounts': [], 'firstOwner': {'actor': 'system:synthetic-installer', 'email': 'owner@example.invalid', 'name': 'Synthetic Owner', 'password': 'synthetic-owner-password-123'}, 'backupSigningKeyFile': str(work / 'signing.der'), 'authMail': {'endpoint': f'https://127.0.0.1:{mail_port}/mail', 'bearerToken': bearer, 'from': 'auth@example.invalid', 'idempotencySecret': secrets.token_urlsafe(32), 'trustedCaFile': str(ca)}}
            write_config()
            start(create=True)
            script = work / 'browser.mjs'
            script.write_text(BROWSER)
            controller = subprocess.Popen(['/opt/homebrew/bin/bun', str(script)], cwd=REPO, env={**os.environ, 'RM_REPORTS_BASE': origin, 'RM_REPORTS_WORK': str(work), 'RM_REPORTS_SCREENSHOT': str(args.evidence.with_suffix('.png')), 'RM_REPORTS_PLAYWRIGHT': str((REPO / 'node_modules/@playwright/test/index.mjs').resolve(strict=True))}, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, bufsize=1, start_new_session=True)
            report['browserControllerPid'] = controller.pid
            for action in ['authenticate', 'setup', 'import', 'unknown-missing', 'classify', 'reports', 'save-stable', 'archive-before', 'exports', 'ui', 'rights', 'clients']:
                phase(action)
            before_audit = inspect_audit('before')
            # Controlled corruption of the retained synthetic original only. No
            # database writes, source replacement, identity seed or guard bypass.
            stop()
            config.pop('firstOwner')
            source_file = Path(config['root']) / 'synthetic-archive-reports/documents/originals' / (report['results']['import']['reviewed']['source']['sha256'] + '.json')
            source_bytes = base64.b64decode(report['results']['import']['sourceInput']['contentBase64'], validate=True)
            if not owned() or any(p.is_symlink() for p in [source_file, *source_file.parents]) or source_file.read_bytes() != source_bytes:
                raise RuntimeError('Exact owned canonical original required for corruption refusal')
            source_stat = source_file.stat()
            def replace_original(data):
                fd = os.open(source_file, os.O_RDWR | os.O_NOFOLLOW)
                try:
                    observed = os.fstat(fd)
                    if observed.st_uid != os.getuid() or not stat.S_ISREG(observed.st_mode) or (observed.st_dev, observed.st_ino) != (source_stat.st_dev, source_stat.st_ino):
                        raise RuntimeError('Owned original identity changed')
                    with os.fdopen(fd, 'r+b', closefd=False) as output:
                        output.seek(0)
                        output.write(data)
                        output.truncate()
                        output.flush()
                        os.fsync(output.fileno())
                finally:
                    os.close(fd)
            progress('operator:controlled-owned-original-corruption')
            try:
                replace_original(b'!' + source_bytes[1:])
                write_config()
                start()
                phase('tampered')
            finally:
                stop()
                replace_original(source_bytes)
            write_config()
            start()
            phase('reopen')
            archive = phase('archive')
            report['signedSnapshot'] = archive
            after_audit = inspect_audit('after')
            if before_audit != after_audit:
                raise RuntimeError('Read-only report/UI/export/client/refusal sequence changed company audit')
            report['companyAuditReadOnlyQualified'] = True
            progress('operator:independent-signed-snapshot-verification')
            path = work / 'archive-reports-after.native-backup.json'
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
            report.update({'status': 'passed', 'currentAction': 'complete', 'archiveReportsActualQualified': True, 'currentRightsAndPhysicalOriginalRefusalsQualified': True, 'initialAndRestartQualified': True, 'snapshotCoverage': 'independently-verified-source-free-restored-and-reopened'})
            save()
            print('PASS: five exact archived reports/source collision/exports/clients/refusals, unchanged books and audit, source-free signed restore and reopen; ' + str(args.evidence))
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
