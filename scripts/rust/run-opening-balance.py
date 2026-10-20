#!/usr/bin/env python3
"""Owned normal-MFA explicit opening and post-cutover reporting qualification.

Uses supplied artifacts and ordinary public APIs/UI, not SQL/auth/business seeds.
Real retained archive citations, original-backed expenses, reviewed shifted-year
opening, distinct group reviewer, durable lost reply and fresh-owner receipt mapping.
Company reconciliation uses existing CFO source sums and ordinary fiscal reports;
group closing uses the existing approved-profile asOf report, no invented dated API.
Only own independently verified signed SQLite copies are read query-only for proof.
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
PREFIX = 'Rentemester-Synthetic-Opening-Balance-'
BROWSER = r'''





import {createHmac,createHash} from 'node:crypto';
import {createConnection} from 'node:net';
import {spawnSync} from 'node:child_process';
import {createInterface} from 'node:readline';
import {readFileSync,readdirSync,writeFileSync,unlinkSync} from 'node:fs';
import {join} from 'node:path';
import {isDeepStrictEqual} from 'node:util';
const {chromium}=await import(process.env.RM_OPENING_PLAYWRIGHT);
const base=process.env.RM_OPENING_BASE,work=process.env.RM_OPENING_WORK;
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
const left='synthetic-opening-left',right='synthetic-opening-right',slugs=[left,right],cutOverDate='2026-10-01',beforeDate='2026-09-30',preExpenseDate='2026-10-10',postExpenseDate='2026-10-15';
const prefix=slug=>`/api/companies/${slug}/`,opening=(slug,action='')=>prefix(slug)+'opening-balance'+(action?'/'+action:''),hash=bytes=>createHash('sha256').update(bytes).digest('hex');
const archiveHeader='Konto;Kontonavn;Dato;Bilag;Bilagstype;Tekst;Momstype;Beløb;Saldo\r\n';
const files=[{path:'2024/Posteringer.csv',bytes:Buffer.from('\ufeff'+archiveHeader+'1864;Historical reference asset;2024-06-01;HIST;Archive;Historical reference only;;999.000000;\r\n8864;Historical reference equity;2024-06-01;HIST;Archive;Historical reference only;;-999.000000;\r\n')},{path:'2024/SaldoBalance.csv',bytes:Buffer.from('Konto;Kontonavn;Beløb\r\n1864;Historical reference asset;999.000000\r\n8864;Historical reference equity;-999.000000\r\n')},{path:'2025/Posteringer.csv',bytes:Buffer.from(archiveHeader+'1864;Excluded latest reference;2025-06-01;EXCLUDE;Archive;Excluded source control;;1234.000000;\r\n')}];
let asOf,fiscalYear,sourceInput,imports={},inputs={},firstPlan,currentPlan,applyBody,applied,lostBody,lostRecord,lostSourceReply,sourceOpening={},originals=[],expenses=[],canonical,freshOwner,restoredOwnerId,mappedReceipt,closedPeriod;const profileId='synthetic-opening-profile',groupId='synthetic-opening-group';
async function call(path,body,page=ownerPage){const r=await page.request.post(base+path,{headers:{origin:base},data:body});let value;try{value=await r.json();}catch{value=null;}trace.push({step,path,status:r.status(),keys:value&&typeof value==='object'?Object.keys(value).sort():[]});if(trace.length>32)trace.shift();return{status:r.status(),value};}
async function post(path,body,page=ownerPage){const r=await call(path,body,page);need(r.status>=200&&r.status<300&&r.value?.ok!==false,`canonical ${path} HTTP${r.status} ${r.value?.code??''}`);return r.value;}
async function read(path,page=ownerPage){return get(page,path);}
async function openingRead(slug){const v=await read(opening(slug));need(v.ok===true&&Array.isArray(v.errors)&&v.errors.length===0,'canonical opening read');return v;}
async function journal(slug){return(await read(prefix(slug)+'journal?'+new URLSearchParams({year:fiscalYear,source:'live'}))).journal;}
async function financial(slug){return{journal:await journal(slug),trialBalance:(await read(prefix(slug)+'trial-balance?'+new URLSearchParams({year:fiscalYear,source:'live'}))).trialBalance,balance:(await read(prefix(slug)+'balance?'+new URLSearchParams({year:fiscalYear,source:'live'}))).balance,income:(await read(prefix(slug)+'income-statement?'+new URLSearchParams({year:fiscalYear,source:'live'}))).incomeStatement};}
async function chartAccount(slug,accountNo,type,normalBalance){return post(prefix(slug)+'accounts',{accountNo,name:'Synthetic adoption '+accountNo,type,normalBalance});}
async function setup(){step='ordinary-two-current-owners-shifted-company-and-explicit-chart';await post('/api/workspace/members/access',{userId:memberId,action:'set-role',workspaceRole:'workspace_owner'});for(const slug of slugs){await post('/api/companies',{name:'Synthetic adoption '+slug,slug,fiscalYearStartMonth:'7',vatPeriodType:null});for(const userId of[ownerId,memberId])await post('/api/workspace/members/company',{userId,action:'grant',companySlug:slug,role:'owner'});for(const[a,t,n]of[['1864','asset','debit'],['2864','liability','credit'],['3864','income','credit'],['4864','expense','debit'],['8864','equity','credit']])await chartAccount(slug,a,t,n);const initial=(await read(prefix(slug)+'journal')).journal;if(slug===left)fiscalYear=initial.selectedYear;else need(initial.selectedYear===fiscalYear,'same explicit shifted fiscal calendar');need(initial.periodStart==='2026-07-01','actual server shifted fiscal year covers explicit fixture dates');need((await openingRead(slug)).openingBalance===null&&initial.entries.length===0,'fresh company has no native opening or journal history');}asOf=(await read(prefix(left)+'scheduled-recurring')).scheduledRecurring.asOfDate;need(asOf>=postExpenseDate&&/^\d{4}-\d{2}-\d{2}$/.test(asOf),'explicit fixture dates are nonfuture under trusted Copenhagen date');return{slugs,asOf,cutOverDate,fiscalYear};}
async function importArchive(){step='actual-public-archive-ui-retained-reference-not-automatic-opening';diagnosticPage=ownerPage;await ownerPage.goto(base+`/companies/${left}/arkiv`);await ownerPage.getByRole('heading',{name:'Importér afsluttede regnskabsår',exact:true}).waitFor();await ownerPage.getByLabel('Historisk basisvaluta',{exact:true}).fill('DKK');for(const[index,year]of['2024','2025'].entries()){await ownerPage.getByLabel(`Regnskabsår ${index+1}`,{exact:true}).fill(year);for(const[kind,label]of[['Posteringer','Posteringer CSV'],['SaldoBalance','Saldobalance CSV']]){const file=files.find(f=>f.path===`${year}/${kind}.csv`);if(file)await ownerPage.getByLabel(`${label} ${index+1}${kind==='SaldoBalance'?' (valgfri)':''}`,{exact:true}).setInputFiles({name:kind+'.csv',mimeType:'text/csv',buffer:file.bytes});}}const planned=ownerPage.waitForResponse(r=>new URL(r.url()).pathname===prefix(left)+'archive-import/plan'&&r.request().method()==='POST');await ownerPage.getByRole('button',{name:'Gennemgå arkivimport',exact:true}).click();const response=await planned;need(response.status()===200,'real reference archive plan');sourceInput=response.request().postDataJSON();const plan=(await response.json()).plan,pkg=JSON.parse(Buffer.from(sourceInput.contentBase64,'base64').toString('utf8'));need(plan.excludedLatestYear===2025&&pkg.currency==='DKK','explicit source-year exclusion and currency');for(const file of files)need(Buffer.from(pkg.files.find(f=>f.path===file.path).contentBase64,'base64').equals(file.bytes),'exact real reference original bytes');const dialog=ownerPage.getByRole('dialog',{name:'Bekræft arkivimport',exact:true});await dialog.getByText('Kilde og plan-hash',{exact:true}).click();await dialog.getByText(plan.planHash,{exact:true}).waitFor();await dialog.getByRole('checkbox',{name:'Jeg bekræfter valuta og de gennemgåede arkivår',exact:true}).check();const applied=ownerPage.waitForResponse(r=>new URL(r.url()).pathname===prefix(left)+'archive-import/apply'&&r.request().method()==='POST');await dialog.getByRole('button',{name:'Importér afsluttede år',exact:true}).click();const r=await applied;need(r.status()===200,'real reference archive retained');imports[left]=(await r.json()).archiveImport;const rightPlan=(await post(prefix(right)+'archive-import/plan',sourceInput)).plan;imports[right]=(await post(prefix(right)+'archive-import/apply',{...sourceInput,planHash:rightPlan.planHash,idempotencyKey:'synthetic-opening-archive-right',confirm:true})).archiveImport;for(const slug of slugs){need((await openingRead(slug)).openingBalance===null&&(await journal(slug)).entries.length===0,'archive reference alone never becomes opening/journal');inputs[slug]={cutOverDate,lines:slug===left?[{accountNo:'1864',debitAmount:'100.00',creditAmount:'0.00',text:'Explicit synthetic opening asset'},{accountNo:'2864',debitAmount:'0.00',creditAmount:'30.00',text:'Explicit synthetic opening liability'},{accountNo:'8864',debitAmount:'0.00',creditAmount:'70.00',text:'Explicit synthetic opening equity'}]:[{accountNo:'1864',debitAmount:'40.00',creditAmount:'0.00',text:'Explicit synthetic opening asset'},{accountNo:'8864',debitAmount:'0.00',creditAmount:'40.00',text:'Explicit synthetic opening equity'}],note:'Synthetic explicit adoption amounts, not copied from historical 999.00 reference',sourceArchiveImportIds:[imports[slug].importId]};}return{imports,inputs,sourceInput};}
async function expense(amount,date,key){step='real-original-backed-later-expense-'+amount;const bytes=Buffer.from(`Synthetic adopted-company expense ${amount}DKK dated ${date}; VAT explicitly0.00.\n`),uploaded=await post(prefix(left)+'documents/ingest',{confirm:true,fileName:key+'.txt',fileBase64:bytes.toString('base64'),metadata:{source:'Synthetic later expense original',documentType:'cash_register_receipt',issueDate:date,amountIncVat:Number(amount),vatAmount:0,currency:'DKK'}});originals.push({id:uploaded.document.id,sha256:hash(bytes)});const response=await post(prefix(left)+'manual-journal',{confirm:true,input:{format:'rentemester-manual-journal-post/1',expectedRuleVersion:(await journal(left)).nativePosting.ruleVersion,idempotencyKey:key,request:{transactionDate:date,text:'Synthetic later expense '+amount,currency:'DKK',documentId:String(uploaded.document.id),lines:[{accountNo:'4864',debitAmount:amount,creditAmount:'0.00'},{accountNo:'1864',debitAmount:'0.00',creditAmount:amount}]}}});need(Number.isSafeInteger(response.entryId),'genuine expense journal receipt');expenses.push(response);return response;}
async function planUi(){step='manage-explicit-opening-amounts-review';diagnosticPage=ownerPage;await ownerPage.goto(base+`/companies/${left}/manage`);const region=ownerPage.getByRole('region',{name:'Åbningsbalance',exact:true});await region.getByLabel('Skæringsdato',{exact:true}).fill(cutOverDate);await region.getByLabel('Note (valgfri)',{exact:true}).fill(inputs[left].note);await region.getByRole('button',{name:'Tilføj linje',exact:true}).click();for(const[index,line]of inputs[left].lines.entries()){await region.getByLabel(`Konto ${index+1}`,{exact:true}).selectOption(line.accountNo);if(line.debitAmount!=='0.00')await region.getByLabel(`Debet ${index+1}`,{exact:true}).fill(line.debitAmount);if(line.creditAmount!=='0.00')await region.getByLabel(`Kredit ${index+1}`,{exact:true}).fill(line.creditAmount);await region.getByLabel(`Linjetekst ${index+1} (valgfri)`,{exact:true}).fill(line.text);}await region.getByRole('checkbox',{name:'Arkivimport '+imports[left].importId,exact:true}).check();const response=ownerPage.waitForResponse(r=>new URL(r.url()).pathname===opening(left,'plan')&&r.request().method()==='POST');await region.getByRole('button',{name:'Gennemgå åbningsbalance',exact:true}).click();const r=await response;need(r.status()===200,'real readonly opening plan');currentPlan=(await r.json()).plan;need(isDeepStrictEqual(r.request().postDataJSON(),inputs[left]),'actual UI sends only explicitly selected exact canonical opening input');const dialog=ownerPage.getByRole('dialog',{name:'Bekræft åbningsbalance',exact:true});await dialog.getByText('Kilde og plan-hash',{exact:true}).click();await dialog.getByText(currentPlan.planHash,{exact:true}).waitFor();need(await dialog.getByRole('button',{name:'Bogfør åbningsbalance',exact:true}).isDisabled(),'explicit review alone does not apply');return currentPlan;}
async function closedRefusal(){step='ordinary-reviewed-empty-period-close-before-any-archive-or-opening';for(const[accountNo,type,normalBalance]of[['1865','asset','debit'],['2865','liability','credit']])await chartAccount(left,accountNo,type,normalBalance);for(const[role,accountNo]of[['bank','1864'],['debtors','1865'],['creditors','2865']])await post(prefix(left)+'account-roles',{role,accountNo,confirm:true});const bounds={periodStart:'2026-09-01',periodEnd:beforeDate,confirm:true},readiness=(await read(prefix(left)+'periods/close-readiness?from='+bounds.periodStart+'&to='+bounds.periodEnd)).packet;need(readiness.blockers===0,'actual empty-period controls ready without bypass');const review=(await post(prefix(left)+'periods/close-review',bounds)).review;closedPeriod=(await post(prefix(left)+'periods/close',{...bounds,kind:'custom',reviewId:review.id,packetHash:review.packet.hash})).period;need(closedPeriod.status==='closed','ordinary reviewed close receipt');closedPeriod=(await read(prefix(left)+'periods')).periods.periods.find(p=>p.id===closedPeriod.id);need(closedPeriod.effectiveStatus==='closed'&&closedPeriod.periodStart===bounds.periodStart&&closedPeriod.periodEnd===bounds.periodEnd,'current canonical period is actually closed');const input={cutOverDate:'2026-09-15',lines:[{accountNo:'1864',debitAmount:'10.00',creditAmount:'0.00',text:'Synthetic closed-period refusal'},{accountNo:'8864',debitAmount:'0.00',creditAmount:'10.00',text:'Synthetic closed-period refusal'}],note:'Synthetic real closed-period refusal',sourceArchiveImportIds:[]},plan=(await post(opening(left,'plan'),input)).plan;need(plan.status==='blocked'&&plan.blockers.length>0,'closed-period opening cannot be offered as ready');const refusal=await call(opening(left,'apply'),{input,planHash:plan.planHash,idempotencyKey:'synthetic-opening-closed-refusal',confirm:true});need(refusal.status===422&&refusal.value.ok===false&&(await openingRead(left)).openingBalance===null&&(await journal(left)).entries.length===0,'closed-period apply cannot partially post or mark opening');return{closedPeriod,blockedPlan:plan,refusal:{status:refusal.status,code:refusal.value.code}};}
function checkPlan(plan,slug,total){need(plan.status==='ready'&&plan.blockers.length===0&&plan.existing===null&&/^[0-9a-f]{64}$/.test(plan.planHash)&&isDeepStrictEqual(plan.input,inputs[slug])&&plan.totals.debit===total&&plan.totals.credit===total,'ready server plan binds exact explicit input and Money totals');need(plan.sourceBasis.kind==='operator_supplied'&&plan.sourceBasis.currency==='DKK'&&isDeepStrictEqual(plan.sourceBasis.sourceArchives,[{importId:imports[slug].importId,sourceHash:imports[slug].sourceSha256,parserContract:'rentemester-native-closed-history/1',declaredCurrency:'DKK'}]),'real archive citations prove identity, never amount copying');}
async function firstReview(){firstPlan=await planUi();checkPlan(firstPlan,left,'100.00');return{firstPlan};}
async function invariantBeforeOpening(expected){need((await openingRead(left)).openingBalance===null&&isDeepStrictEqual(await journal(left),expected),'failed opening leaves exact journal/head and singleton unchanged');}
async function refusedReview(){await expense('3.00',preExpenseDate,'synthetic-opening-pre-expense');const before=await journal(left);step='actual-current-head-stales-prior-review';const stale=await call(opening(left,'apply'),{input:inputs[left],planHash:firstPlan.planHash,idempotencyKey:'synthetic-opening-stale-plan',confirm:true});need(stale.status===422&&stale.value.ok===false,'source ledger changed after review, old plan refused');await invariantBeforeOpening(before);step='current-admin-revoked-between-review-and-apply';await post('/api/workspace/members/company',{userId:ownerId,action:'grant',companySlug:left,role:'bookkeeper'},memberPage);const fresh=(await post(opening(left,'plan'),inputs[left])).plan;const denied=await call(opening(left,'apply'),{input:inputs[left],planHash:fresh.planHash,idempotencyKey:'synthetic-opening-current-admin-refusal',confirm:true});need(denied.status===403&&denied.value.ok===false,'bookkeeper cannot use earlier owner authority');await post('/api/workspace/members/company',{userId:ownerId,action:'grant',companySlug:left,role:'owner'},memberPage);await invariantBeforeOpening(before);step='explicit-unbalanced-input-stays-blocked-and-atomic';const input=structuredClone(inputs[left]);input.lines[2].creditAmount='71.00';const unbalanced=(await post(opening(left,'plan'),input)).plan;need(unbalanced.status==='blocked'&&unbalanced.totals.debit==='100.00'&&unbalanced.totals.credit==='101.00'&&unbalanced.blockers.length>0,'unbalanced Money stays explicit, never ready zero');const bad=await call(opening(left,'apply'),{input,planHash:unbalanced.planHash,idempotencyKey:'synthetic-opening-unbalanced',confirm:true});need(bad.status===422&&bad.value.ok===false,'unbalanced apply refused');await invariantBeforeOpening(before);return{realLedgerHeadStale:true,currentCompanyAdminRefused:true,unbalancedAtomicRefused:true,before,unbalanced};}
async function reReview(){step='explicitly-discard-old-review-and-read-fresh-plan';let dialog=ownerPage.getByRole('dialog',{name:'Bekræft åbningsbalance',exact:true});await dialog.getByRole('button',{name:'Tilbage uden bogføring',exact:true}).click();await ownerPage.getByRole('dialog',{name:'Kassér ændringer?',exact:true}).getByRole('button',{name:'Kassér ændringer',exact:true}).click();await dialog.waitFor({state:'hidden'});const region=ownerPage.getByRole('region',{name:'Åbningsbalance',exact:true}),wait=ownerPage.waitForResponse(r=>new URL(r.url()).pathname===opening(left,'plan')&&r.request().method()==='POST');await region.getByRole('button',{name:'Gennemgå åbningsbalance',exact:true}).click();const response=await wait;need(response.status()===200,'fresh reviewed plan');currentPlan=(await response.json()).plan;checkPlan(currentPlan,left,'100.00');need(currentPlan.planHash!==firstPlan.planHash,'fresh current ledger head changes exact review hash');dialog=ownerPage.getByRole('dialog',{name:'Bekræft åbningsbalance',exact:true});await dialog.getByText('Kilde og plan-hash',{exact:true}).click();await dialog.getByText(currentPlan.planHash,{exact:true}).waitFor();return{currentPlan};}
function checkRecord(record,slug,plan){need(record.cutOverDate===cutOverDate&&Number.isSafeInteger(record.journalEntryId)&&record.journalEntryId>0&&/^[0-9a-f]{64}$/.test(record.journalEntryHash)&&record.planHash===plan.planHash&&record.actor==='user:'+ownerId&&isDeepStrictEqual(record.sourceBasis,plan.sourceBasis),'canonical immutable opening identity/hash/date/current actor/source citations');sourceOpening[slug]=record;}
async function applyUi(){step='actual-reviewed-manage-confirmation-posts-one-atomic-opening';const before=await journal(left),dialog=ownerPage.getByRole('dialog',{name:'Bekræft åbningsbalance',exact:true});await dialog.getByRole('checkbox',{name:'Jeg bekræfter skæringsdato, konti og åbningsbeløb',exact:true}).check();const wait=ownerPage.waitForResponse(r=>new URL(r.url()).pathname===opening(left,'apply')&&r.request().method()==='POST');await dialog.getByRole('button',{name:'Bogfør åbningsbalance',exact:true}).click();const response=await wait;need(response.status()===200,'actual confirmed opening apply');applyBody=response.request().postDataJSON();applied=await response.json();need(isDeepStrictEqual(applyBody.input,inputs[left])&&applyBody.planHash===currentPlan.planHash&&applyBody.confirm===true&&typeof applyBody.idempotencyKey==='string'&&applied.idempotency.replayed===false&&applied.errors.length===0,'one exact caller intent and durable original receipt');checkRecord(applied.openingBalance,left,currentPlan);const after=await journal(left);need(after.entries.length===before.entries.length+1&&after.entries.some(e=>e.id===sourceOpening[left].journalEntryId&&e.date===cutOverDate)&&after.entries.some(e=>e.id===expenses[0].entryId),'later existing entry retained; one explicit dated opening appended');const receipt=ownerPage.getByRole('region',{name:'Bogført åbningsbalance',exact:true});await receipt.waitFor();const link=receipt.getByRole('link',{name:'Vis åbningspostering '+sourceOpening[left].journalEntryNo,exact:true});const url=new URL(await link.getAttribute('href'),base);need(url.pathname===`/companies/${left}/posteringer`&&url.searchParams.get('journalEntryId')===String(sourceOpening[left].journalEntryId)&&url.searchParams.get('source')==='live'&&url.searchParams.get('year')===fiscalYear,'actual receipt links immutable native journal identity and server fiscal year');return{applied,applyBody};}
async function rightLostReply(){step='right-reviewed-exact-plan-before-genuine-lost-response';const plan=(await post(opening(right,'plan'),inputs[right])).plan;checkPlan(plan,right,'40.00');lostBody={input:inputs[right],planHash:plan.planHash,idempotencyKey:'synthetic-opening-right-accepted-lost-response',confirm:true};const cookie=(await ownerPage.context().cookies(base)).map(c=>`${c.name}=${c.value}`).join('; '),bytes=JSON.stringify(lostBody);await new Promise((resolve,reject)=>{const url=new URL(base),socket=createConnection({host:url.hostname,port:Number(url.port)},()=>socket.write(`POST ${opening(right,'apply')} HTTP/1.1\r\nHost: ${url.host}\r\nOrigin: ${base}\r\nContent-Type: application/json\r\nCookie: ${cookie}\r\nContent-Length: ${Buffer.byteLength(bytes)}\r\nConnection: close\r\n\r\n${bytes}`));socket.setTimeout(30000,()=>{socket.destroy();reject(Error('bounded discarded-reply socket'));});socket.on('error',reject);socket.on('data',()=>{});socket.on('end',()=>{socket.destroy();resolve();});});lostRecord=(await openingRead(right)).openingBalance;checkRecord(lostRecord,right,plan);need((await journal(right)).entries.length===1,'canonical state proves accepted once without reading mutation reply');lostSourceReply=await post(opening(right,'apply'),lostBody);need(lostSourceReply.idempotency.replayed===true&&isDeepStrictEqual(lostSourceReply.openingBalance,lostRecord),'deliberate exact-key recovery returns original result without another journal');return{plan,lostBody,lostRecord,recovered:lostSourceReply,discardedSuccessfulResponse:true};}
async function laterExpense(){await expense('7.00',postExpenseDate,'synthetic-opening-post-expense');return{expenses,originals};}
async function mutationRefusals(){step='singleton-conflicting-key-and-protected-opening-reversal';const before=await financial(left);const duplicate=await call(opening(left,'apply'),{...applyBody,idempotencyKey:'synthetic-opening-different-key-duplicate'});need(duplicate.status===422&&duplicate.value.ok===false,'new key cannot bypass authoritative singleton');const changed=structuredClone(applyBody);changed.input.note+=' Changed review.';const conflict=await call(opening(left,'apply'),changed);need(conflict.status===409&&conflict.value.ok===false,'same accepted key with changed body conflicts');const j=await journal(left),early=await call(prefix(left)+'manual-journal',{confirm:true,input:{format:'rentemester-manual-journal-post/1',expectedRuleVersion:j.nativePosting.ruleVersion,idempotencyKey:'synthetic-opening-precutover-post-refusal',request:{transactionDate:beforeDate,text:'Synthetic pre-cutover balance-only refusal',currency:'DKK',lines:[{accountNo:'1864',debitAmount:'1.00',creditAmount:'0.00'},{accountNo:'8864',debitAmount:'0.00',creditAmount:'1.00'}]}}});need(early.status===422&&early.value.ok===false,'shared posting path refuses any new entry before cutover');const reverse=await call(prefix(left)+'manual-journal-reversal',{confirm:true,input:{format:'rentemester-manual-journal-reversal/1',expectedRuleVersion:j.nativePosting.ruleVersion,idempotencyKey:'synthetic-opening-generic-reversal-refusal',request:{entryId:String(sourceOpening[left].journalEntryId),transactionDate:asOf,reason:'Synthetic generic reversal of protected opening must refuse'}}});need(reverse.status===422&&reverse.value.ok===false,'generic reversal cannot neutralize an authoritative opening marker');need(isDeepStrictEqual(await financial(left),before),'all rejected mutations leave exact journals/figures/singleton unchanged');need(isDeepStrictEqual((await openingRead(left)).openingBalance,sourceOpening[left]),'original opening retained');return{differentKeyDuplicateRefused:true,changedAcceptedKeyRefused:true,preCutoverPostingRefused:true,genericOpeningReversalRefused:true};}
async function prepareGroup(){step='authoritative-v1-group-and-independently-approved-report-profile';await post(prefix(left)+'intercompany/manifest',{version:1,confirm:true,groups:[{id:groupId,name:'Synthetic explicit adopted group',memberships:[{id:'opening-left',companySlug:left,validFrom:'2000-01-01'},{id:'opening-right',companySlug:right,validFrom:'2000-01-01'}],ownership:[{id:'opening-full-child',parentCompanySlug:left,childCompanySlug:right,basisPoints:10000,evidenceRefs:['synthetic-reviewed-group-authority'],validFrom:'2000-01-01'}]}]});const reportingLines=[['assets','Assets','asset'],['debt','Debt','liability'],['equity','Equity','equity'],['result','Current result','equity'],['income','Income','income'],['expense','Expense','expense']].map(([id,label,section],displayOrder)=>({id,label,section,displayOrder,...(id==='result'?{role:'current-result'}:{})}));const accountMappings=slugs.flatMap((companySlug,index)=>(companySlug===left?[['1864','assets'],['1865','assets'],['2864','debt'],['2865','debt'],['3864','income'],['4864','expense'],['8864','equity']]:[['1864','assets'],['2864','debt'],['3864','income'],['4864','expense'],['8864','equity']]).map(([accountNo,reportingLineId])=>({id:`opening-${index}-${accountNo}`,companySlug,accountNo,reportingLineId,validFrom:'2000-01-01'})));const profile={version:1,id:profileId,groupId,currency:'DKK',reportingLines,accountMappings,evidenceRefs:['synthetic-reviewed-adoption-report'],validFrom:'2000-01-01'},proposal=await post(prefix(left)+'consolidation/propose',{profile,confirm:true});const self=await call(prefix(left)+'consolidation/approve',{profileId,profileHash:proposal.profileHash,confirm:true});need(self.status===422&&self.value.ok===false,'group profile author cannot approve own proposal');const approved=await post(prefix(left)+'consolidation/approve',{profileId,profileHash:proposal.profileHash,confirm:true},memberPage);return{profile,proposal,approved,independentCurrentReviewer:true};}
function groupPath(date=asOf){return'/api/group-consolidated-report?'+new URLSearchParams({profileId,from:'2026-07-01',asOf:date});}
async function originalProof(){for(const slug of slugs){const r=await ownerPage.request.get(base+prefix(slug)+`archive-import/${imports[slug].importId}/original`);need(r.status()===200&&Buffer.from(sourceInput.contentBase64,'base64').equals(await r.body()),'cited source package remains exact separate historical evidence');const archived=(await read(prefix(slug)+'archives/2024')).years[0];need(archived.balances.some(row=>row.accountNo==='1864'&&row.amount==='999.00')&&archived.sourceHash===imports[slug].sourceSha256,'historical999.00 reference never rewritten to opening amounts');}for(const item of originals){const r=await ownerPage.request.get(base+prefix(left)+`documents/${item.id}/file`);need(r.status()===200&&hash(await r.body())===item.sha256,'genuine expense original bytes unchanged');}}
async function reports(){step='company-fiscal-coverage-cfo-and-approved-group-closing-reconcile';const values={};for(const slug of slugs){values[slug]=await financial(slug);for(const[name,body]of Object.entries(values[slug])){need(body.openingBalanceBasis?.basis==='live'&&body.openingBalanceBasis.cutOverDate===cutOverDate&&body.openingBalanceBasis.openingJournalEntryId===sourceOpening[slug].journalEntryId&&body.openingBalanceBasis.archiveAmountsIncluded===false,'explicit live opening basis for '+name);need(body.liveCoverage?.status==='partial'&&body.liveCoverage.reason==='native_history_begins_at_explicit_cutover'&&body.liveCoverage.periodStart==='2026-07-01'&&body.liveCoverage.cutOverDate===cutOverDate,'fiscal year spanning cutover never claims full pre-cutover coverage');}need(isDeepStrictEqual((await openingRead(slug)).openingBalance,sourceOpening[slug]),'historical opening identity/hash/actor/citations unchanged');}const l=values[left],r=values[right];need(l.journal.entries.length===3&&r.journal.entries.length===1&&l.balance.totalAssets===90&&l.balance.liabilities.total===30&&l.balance.equity.total===60&&l.balance.periodResult===-10&&l.balance.balanced===true&&l.income.totalExpense===10&&l.income.result===-10,'left explicit100 opening less actual3+7 expenses, no999 archive carry-forward');need(r.balance.totalAssets===40&&r.balance.equity.total===40&&r.balance.balanced===true,'right explicit40 opening remains separate legal ledger');const cfo=await read('/api/cfo-analytics?'+new URLSearchParams({scope:'company',companySlug:left,from:cutOverDate,to:asOf,account:'1864'}));need(cfo.ok===true&&cfo.status==='ready'&&cfo.rows.length===3&&cfo.rows.every(row=>row.sourceType==='ledger')&&cfo.reconciliation.amountByCurrency.DKK==='90.00','existing company dated canonical account sum reconciles opening+later activity');const group=await read(groupPath());need(group.status==='ready'&&group.blockers.length===0&&group.appliedEliminations.length===0&&group.sourceSnapshots.length===2,'genuine approved group closing report is ready');const figures=Object.fromEntries(group.consolidatedFigures.map(row=>[row.lineId,row.consolidatedAmount]));need(isDeepStrictEqual(figures,{assets:'130.00',debt:'30.00',equity:'110.00',result:'-10.00',income:'0.00',expense:'10.00'}),'exact current group closing figures, no historical archive amount added');const before=await read(groupPath(beforeDate));need(before.status==='blocked'&&before.consolidatedFigures===null&&before.blockers.length>0,'pre-cutover group closing unavailable, never ready zero');await originalProof();return{companies:values,cfo,group,preCutoverGroup:before,groupClosingOnlyNotInventedMovementRange:true};}
async function reportUi(){step='ordinary-post-cutover-live-report-with-explicit-partial-coverage';diagnosticPage=ownerPage;for(const[route,title]of[['balance','Balance'],['resultatopgorelse','Resultatopgørelse'],['posteringer','Posteringer']]){await ownerPage.goto(base+`/companies/${left}/${route}?year=${encodeURIComponent(fiscalYear)}&source=live`);await ownerPage.getByRole('heading',{name:title,exact:true}).waitFor();const status=ownerPage.getByRole('status',{name:'Aktivt regnskab efter skæringsdato',exact:true});await status.waitFor();need((await status.innerText()).includes(cutOverDate)&&(await status.innerText()).includes('Tallene dokumenterer ikke hele regnskabsåret før skæringsdatoen.'),'visible partial fiscal coverage beside exact live figures');}await ownerPage.screenshot({path:process.env.RM_OPENING_SCREENSHOT,fullPage:true});return{ordinaryLiveReportCoverageUi:true};}
async function client(kind,operation,slug,body){const file=join(work,'opening-client.cookie');writeFileSync(file,(await ownerPage.context().cookies(base)).map(c=>`${c.name}=${c.value}`).join('; '),{mode:0o600,flag:'wx'});try{const common=['--server',base,'--cookie-file',file,'--allow-loopback-http'];let argv,input;if(kind==='cli'){argv=['remote',operation,...common,'--company',slug];if(operation==='opening-balance-apply')argv.push('--confirm');if(body!==undefined)input=JSON.stringify(body);}else{argv=common;input=[{jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'synthetic-opening-proof',version:'1'}}},{jsonrpc:'2.0',method:'notifications/initialized'},{jsonrpc:'2.0',id:2,method:'tools/call',params:{name:operation.replaceAll('-','_'),arguments:{company:slug,...(body===undefined?{}:{body:JSON.stringify(body)}),...(operation==='opening-balance-apply'?{confirm:true}:{})}}}].map(v=>JSON.stringify(v)).join('\n')+'\n';}const r=spawnSync(join(work,kind==='cli'?'rentemester-rust':'rentemester-mcp'),argv,{input,encoding:'utf8',timeout:45000,maxBuffer:16*1024*1024});need(!r.error&&r.status===0,'native opening client terminal '+operation);const value=kind==='cli'?JSON.parse(r.stdout):r.stdout.trim().split('\n').map(line=>JSON.parse(line)).find(v=>v.id===2)?.result?.structuredContent;need(value?.ok===true&&value.data,'full canonical client envelope');return value.data;}finally{unlinkSync(file);}}
async function clientPlans(){step='http-cli-mcp-readonly-plan-full-parity-before-any-opening';const before=await journal(left),expected=await post(opening(left,'plan'),inputs[left]);for(const kind of['cli','mcp'])need(isDeepStrictEqual(await client(kind,'opening-balance-plan',left,inputs[left]),expected)&&isDeepStrictEqual(await client(kind,'opening-balance',left),await openingRead(left)),'canonical complete current plan/read parity');await invariantBeforeOpening(before);return{planReadHttpCliMcpParity:true};}
async function clients(){step='canonical-opening-read-and-explicit-accepted-key-replay-clients';for(const kind of['cli','mcp']){for(const slug of slugs)need(isDeepStrictEqual(await client(kind,'opening-balance',slug),await openingRead(slug)),'full current opening-read HTTPCLI/MCP parity');const expected=await post(opening(right,'apply'),lostBody);need(expected.idempotency.replayed===true&&isDeepStrictEqual(expected.openingBalance,sourceOpening[right])&&isDeepStrictEqual(await client(kind,'opening-balance-apply',right,lostBody),expected),'explicit retained accepted-key replay exact original result, no apply callback');}return{openingReadAndApplyReplayHttpCliMcpParity:true};}
async function snapshot(){const openings={},profiles={};for(const slug of slugs){openings[slug]=await openingRead(slug);profiles[slug]={company:await read(prefix(slug)+'company'),accounts:await read(prefix(slug)+'accounts'),archive:await read(prefix(slug)+'archives/2024')};}return{openings,profiles,reports:await reports()};}
async function saveStable(){canonical=await snapshot();return{canonical};}
async function assertStable(){need(isDeepStrictEqual(await snapshot(),canonical),'exact native opening/marker identity, ledger UUIDs, figures, group provenance, chart/profile and archived source reconstruct unchanged');await originalProof();}
async function reopen(){step='fresh-source-password-mfa-restart-exact-opening-and-group';await ownerPage.context().clearCookies();need((await login(ownerPage,owner)).user.id===ownerId,'same verified source owner after reopen');await assertStable();await clients();await assertStable();return{freshMfaRestart:true,exactOpeningAndClosingReconstructed:true};}
async function archive(name){step='explicit-signed-opening-marker-receipt-and-source-snapshot-'+name;const r=await ownerPage.request.post(base+'/api/workspace/backup',{headers:{origin:base},data:{confirm:true}});need(r.status()===200&&r.headers()['content-type']==='application/vnd.rentemester.native-backup+json','typed opening/fullgroup inventory must sign through real backup path');const bytes=await r.body();need(bytes.length>0&&bytes.length<=96*1024*1024,'full bounded signed inventory');writeFileSync(join(work,'opening-balance-'+name+'.native-backup.json'),bytes,{mode:0o600,flag:'wx'});await assertStable();return{sha256:hash(bytes),bytes:bytes.length};}
async function restoreReceipt(){step='fresh-owner-cannot-replay-historical-opening-key-before-explicit-mapping';const before=await snapshot(),refused=await call(opening(right,'apply'),lostBody);need(refused.status===409&&refused.value.ok===false,'old accepted key is not fresh-owner authority');need(isDeepStrictEqual(await snapshot(),before),'unmapped refusal does not duplicate/alter opening');const inventory=await read(prefix(right)+'restore/receipts'),row=inventory.restoredReceipts.rows.find(r=>r.operation==='opening_balance_post'&&r.keyHash===hash(Buffer.from(lostBody.idempotencyKey)));need(row&&row.sourcePrincipal.kind==='user'&&row.sourcePrincipal.subject===ownerId&&row.mapping===null&&row.resultAvailable===true&&isDeepStrictEqual(row.result.openingBalance,sourceOpening[right])&&row.ledgerUuid===before.reports.companies[right].journal.ledgerUuid,'retained actual lost-response receipt is inspectable under current Admin only');need(!JSON.stringify(inventory).includes(lostBody.idempotencyKey),'inventory never leaks original raw key');step='normal-web-explicit-owner-receipt-reconciliation';await ownerPage.goto(base+'/flytning');await ownerPage.getByRole('heading',{name:'Flyt workspace',exact:true}).waitFor();const region=ownerPage.getByRole('region',{name:'Importerede kvitteringer',exact:true});await region.getByLabel('Virksomhed til kvitteringer',{exact:true}).selectOption(right);await region.getByRole('article',{name:'Kvittering '+row.receiptId,exact:true}).waitFor();const reason='Synthetic fresh owner reviewed exact retained accepted opening and original command';await region.getByLabel('Begrundelse for tilknytning',{exact:true}).fill(reason);await region.getByRole('button',{name:'Gennemgå tilknytning af kvittering '+row.receiptId,exact:true}).click();const dialog=ownerPage.getByRole('dialog',{name:'Bekræft tilknytning af kvittering',exact:true});await dialog.getByRole('checkbox',{name:'Jeg bekræfter den gennemgåede handling',exact:true}).check();const wait=ownerPage.waitForResponse(r=>new URL(r.url()).pathname===prefix(right)+'restore/receipts/reconcile'&&r.request().method()==='POST');await dialog.getByRole('button',{name:'Tilknyt kvittering til min bruger',exact:true}).click();const response=await wait;need(response.status()===200,'actual explicit current-owner receipt mapping');mappedReceipt=(await response.json()).restoredReceipts.rows.find(r=>r.receiptId===row.receiptId);need(mappedReceipt.mapping.targetUserId===restoredOwnerId&&mappedReceipt.mapping.reason===reason&&mappedReceipt.mapping.archiveSha256===row.archiveSha256&&mappedReceipt.mapping.restoreBoundaryId===row.restoreBoundaryId,'mapping binds new verified principal and exact signed restore boundary');for(const key of['receiptId','operation','keyHash','payloadHash','sourcePrincipal','ledgerUuid','result','archiveSha256','restoreBoundaryId'])need(isDeepStrictEqual(mappedReceipt[key],row[key]),'receipt mapping preserves original '+key);const replay=await post(opening(right,'apply'),lostBody);need(replay.idempotency.replayed===true&&replay.idempotency.receiptId===row.receiptId&&isDeepStrictEqual(replay.openingBalance,sourceOpening[right])&&replay.idempotency.reconciliation.mappedUserId===restoredOwnerId&&replay.idempotency.reconciliation.archiveSha256===row.archiveSha256,'exact old opening response recovered under new explicit authority without posting');await assertStable();return{unmappedOldKeyRefused:true,original:row,mappedReceipt,replay,explicitFreshOwnerMappingWithoutNewOpening:true};}
async function restored(){step='source-free-fresh-owner-normal-email-verification-and-mfa';ownerPage=await newPage();freshOwner={email:'restored@example.invalid',password:'synthetic-restored-password-789',codes:[]};await ownerPage.goto((await mail('verification',freshOwner.email)).actionUrl);const me=await enroll(ownerPage,freshOwner);restoredOwnerId=me.user.id;need(restoredOwnerId!==ownerId&&restoredOwnerId!==memberId,'retained historical actor never imports current authentication');await assertStable();const receipt=await restoreReceipt();await clients();await reportUi();await assertStable();return{sourceFreeRestoreQualified:true,freshOwnerMfa:true,openingAndApprovedGroupReconstructed:true,receipt};}
async function restoredReopen(){step='fresh-restored-owner-reopen-and-mapped-original-opening-replay';await ownerPage.context().clearCookies();need((await login(ownerPage,freshOwner)).user.id===restoredOwnerId,'same fresh verified restored owner');await assertStable();await clients();await assertStable();const row=(await read(prefix(right)+'restore/receipts')).restoredReceipts.rows.find(r=>r.receiptId===mappedReceipt.receiptId);need(isDeepStrictEqual(row,mappedReceipt),'explicit mapping and original result preserved across target reopen');return{restoredTargetReopenQualified:true,noDuplicateOpening:true,receiptMappingDurable:true};}
for await(const line of createInterface({input:process.stdin,crlfDelay:Infinity})){try{const action=JSON.parse(line);let value;if(action==='authenticate')value=await authenticate();else if(action==='setup')value=await setup();else if(action==='closed-refusal')value=await closedRefusal();else if(action==='import')value=await importArchive();else if(action==='first-review')value=await firstReview();else if(action==='client-plans')value=await clientPlans();else if(action==='refused-review')value=await refusedReview();else if(action==='re-review')value=await reReview();else if(action==='apply-ui')value=await applyUi();else if(action==='lost-reply')value=await rightLostReply();else if(action==='later-expense')value=await laterExpense();else if(action==='group')value=await prepareGroup();else if(action==='save-stable')value=await saveStable();else if(action==='archive-before')value=await archive('before');else if(action==='mutation-refusals')value=await mutationRefusals();else if(action==='reports')value=await reports();else if(action==='ui')value=await reportUi();else if(action==='clients')value=await clients();else if(action==='reopen')value=await reopen();else if(action==='archive')value=await archive('after');else if(action==='restored')value=await restored();else if(action==='restored-reopen')value=await restoredReopen();else if(action==='finish'){await browser?.close();emit({ok:true,value:{finished:true}});break;}else throw Error('unknown phase');emit({ok:true,value});}catch(error){let pageState=null;try{pageState=await Promise.race([diagnosticPage?.evaluate(()=>({pathname:location.pathname,headings:[...document.querySelectorAll('h1,h2,h3,h4')].map(e=>e.textContent.slice(0,100)),buttons:[...document.querySelectorAll('button')].map(e=>e.textContent.slice(0,100)),alerts:document.querySelectorAll('[role=alert]').length})),new Promise(resolve=>setTimeout(()=>resolve(null),2000))]);}catch{}emit({ok:false,step,detail,errorType:error?.name,trace,pageState});await Promise.race([browser?.close().catch(()=>{}),new Promise(resolve=>setTimeout(resolve,2000))]);process.exitCode=1;break;}}

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
    report = {'status': 'running', 'sourceRevision': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=REPO, text=True).strip(), 'runnerSha256': digest(Path(__file__)), 'browserHelperSha256': hashlib.sha256(BROWSER.encode()).hexdigest(), 'canonicalOrigin': origin, 'noOfflineIdentitySeeds': True, 'ordinaryMacOsUser': True, 'ownedProfile': str(work), 'externalMailDelivered': False, 'workflow': 'explicit_opening_and_post_cutover_reporting', 'openingBalanceActualQualified': False, 'installedNativeQualified': False, 'screenshot': str(args.evidence.with_suffix('.png')),  'actions': [], 'results': {}, 'sourceFreeRestoreQualified': False}
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
                argv += ['--restore-backup', str(work / 'opening-balance-after.native-backup.json'), '--trusted-backup-key', str(work / 'trusted.pub'), '--confirm-target', str(root), '--source-unavailable']
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

        def inspect_snapshot(name):
            progress('operator:verify-and-read-owned-opening-snapshot-' + name)
            path = work / ('opening-balance-' + name + '.native-backup.json')
            receipt = report['results']['archive-before' if name == 'before' else 'archive']
            if digest(path) != receipt['sha256']:
                raise RuntimeError('Signed snapshot bytes changed')
            check = subprocess.run([str(pinned['server_binary']), '--verify-backup', str(path), '--trusted-backup-key', str(work / 'trusted.pub')], cwd=REPO, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=60)
            if check.returncode != 0:
                raise RuntimeError('Independent signature/inventory verification refused')
            wire = json.loads(path.read_bytes())
            records = {'synthetic-opening-left': report['results']['apply-ui']['applied']['openingBalance'], 'synthetic-opening-right': report['results']['lost-reply']['lostRecord']}
            result = {}
            for slug, record in records.items():
                entries = [entry for entry in wire['entries'] if entry['path'] == slug + '/data/ledger.sqlite']
                if len(entries) != 1:
                    raise RuntimeError('One exact signed company ledger required')
                raw = base64.b64decode(entries[0]['bytesBase64'], validate=True)
                if not 0 < len(raw) <= 32 * 1024 * 1024:
                    raise RuntimeError('Owned signed company snapshot bound')
                copy = work / ('readonly-' + slug + '-' + name + '.sqlite')
                try:
                    with copy.open('xb') as output:
                        output.write(raw)
                    # Never a live SQLite handle, seed, mutation or authority source.
                    db = sqlite3.connect(copy.as_uri() + '?mode=ro', uri=True)
                    try:
                        db.execute('PRAGMA query_only=ON')
                        tables = {}
                        for table in ['audit_log', 'opening_balances', 'journal_entries', 'journal_lines']:
                            cursor = db.execute('SELECT * FROM ' + table + ' ORDER BY id')
                            columns = [c[0] for c in cursor.description]
                            tables[table] = [dict(zip(columns, row)) for row in cursor.fetchall()]
                    finally:
                        db.close()
                finally:
                    copy.unlink(missing_ok=True)
                if len(tables['opening_balances']) != 1 or tables['opening_balances'][0]['journal_entry_id'] != record['journalEntryId'] or tables['opening_balances'][0]['cut_over_date'] != record['cutOverDate']:
                    raise RuntimeError('Singleton marker is not exact retained opening')
                typed = [row for row in tables['audit_log'] if row['event_type'] == 'native_opening_balance']
                if len(typed) != 1:
                    raise RuntimeError('Exactly one immutable typed opening audit required')
                payload = json.loads(typed[0]['message'])
                if payload['version'] != 'native_opening_balance/1' or payload['record'] != record or typed[0]['actor'] != record['actor']:
                    raise RuntimeError('Canonical opening audit/record/actor differs')
                if len(tables['journal_entries']) != (3 if slug.endswith('-left') else 1):
                    raise RuntimeError('Duplicate or missing opening/expense journal')
                result[slug] = tables
            report.setdefault('signedOpeningProofs', {})[name] = {slug: {'auditRows': len(tables['audit_log']), 'openingRows': len(tables['opening_balances']), 'journalRows': len(tables['journal_entries']), 'sha256': hashlib.sha256(json.dumps(tables, sort_keys=True).encode()).hexdigest()} for slug, tables in result.items()}
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
                ['openssl', 'req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', str(work / 'ca.key'), '-out', str(ca), '-days', '1', '-subj', '/CN=Synthetic Opening Auth CA', '-addext', 'basicConstraints=critical,CA:TRUE', '-addext', 'keyUsage=critical,keyCertSign,cRLSign'],
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
            config = {'runtimeProfile': 'local-user-macos', 'root': str(work / 'workspace'), 'brokerUid': os.getuid(), 'workspace': 'synthetic-native-opening-balance', 'companySlug': 'synthetic', 'companyName': 'Synthetic Opening Anchor', 'cookieKey': key(), 'mfaKey': key(), 'operatorActors': ['system:synthetic-installer'], 'accounts': [], 'firstOwner': {'actor': 'system:synthetic-installer', 'email': 'owner@example.invalid', 'name': 'Synthetic Owner', 'password': 'synthetic-owner-password-123'}, 'backupSigningKeyFile': str(work / 'signing.der'), 'authMail': {'endpoint': f'https://127.0.0.1:{mail_port}/mail', 'bearerToken': bearer, 'from': 'auth@example.invalid', 'idempotencySecret': secrets.token_urlsafe(32), 'trustedCaFile': str(ca)}}
            write_config()
            start(create=True)
            script = work / 'browser.mjs'
            script.write_text(BROWSER)
            controller = subprocess.Popen(['/opt/homebrew/bin/bun', str(script)], cwd=REPO, env={**os.environ, 'RM_OPENING_BASE': origin, 'RM_OPENING_WORK': str(work), 'RM_OPENING_SCREENSHOT': str(args.evidence.with_suffix('.png')), 'RM_OPENING_PLAYWRIGHT': str((REPO / 'node_modules/@playwright/test/index.mjs').resolve(strict=True))}, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, bufsize=1, start_new_session=True)
            report['browserControllerPid'] = controller.pid
            for action in ['authenticate', 'setup', 'closed-refusal', 'import', 'first-review', 'client-plans', 'refused-review', 're-review', 'apply-ui', 'lost-reply', 'later-expense', 'group', 'save-stable', 'archive-before', 'mutation-refusals', 'reports', 'ui', 'clients']:
                phase(action)
            before_audit = inspect_snapshot('before')
            stop()
            config.pop('firstOwner')
            write_config()
            start()
            phase('reopen')
            archive = phase('archive')
            report['signedSnapshot'] = archive
            after_audit = inspect_snapshot('after')
            if before_audit != after_audit:
                raise RuntimeError('Read/refusal/replay sequence changed company audit, opening marker or journal rows')
            report['companyReadRefusalReplayAtomicityQualified'] = True
            progress('operator:independent-signed-snapshot-verification')
            path = work / 'opening-balance-after.native-backup.json'
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
            report.update({'status': 'passed', 'currentAction': 'complete', 'openingBalanceActualQualified': True, 'currentRightsStalePlanAndAtomicRefusalsQualified': True, 'initialAndRestartQualified': True, 'snapshotCoverage': 'independently-verified-source-free-restored-and-reopened'})
            save()
            print('PASS: explicit opening/stale/current-rights/closed/singleton/lost-reply, post-cutover company+approvedgroup, source-free restore and fresh-owner receipt reconciliation; ' + str(args.evidence))
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
