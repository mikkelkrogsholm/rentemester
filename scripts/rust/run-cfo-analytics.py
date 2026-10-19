#!/usr/bin/env python3
"""Owned ordinary-MFA live CFO analytics; supplied artifacts, no builds or seeds.

Real originals, canonical parties, reviewed dimensions, separately posted legal
company journals and an actual cross-date reversal. Complete canonical filters,
pages, portfolio/current-rights/no-profile UI, clients, restart and signed recovery.
Retained posting-archive actual qualification remains explicitly OPEN until a
public native admitted archive-ingestion path exists; this is not full R28 proof.
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
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
PREFIX = 'Rentemester-Synthetic-CFO-'
BROWSER = r'''


import {createHmac,createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {createInterface} from 'node:readline';
import {readFileSync,readdirSync,writeFileSync,unlinkSync} from 'node:fs';
import {join} from 'node:path';
import {isDeepStrictEqual} from 'node:util';
const {chromium}=await import(process.env.RM_CFO_PLAYWRIGHT);
const base=process.env.RM_CFO_BASE,work=process.env.RM_CFO_WORK;
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
const left='synthetic-cfo-left',right='synthetic-cfo-right',slugs=[left,right],from='2025-12-01',originalDate='2025-12-30',earlyTo='2025-12-31',to='2026-01-02',dimension='synthetic-cfo-project',dimMember='synthetic-cfo-member',partyName='Synthetic CFO canonical party';
const prefix=slug=>`/api/companies/${slug}/`,hash=b=>createHash('sha256').update(b).digest('hex');
let partyId,originals=[],receipts={},reversal,dimensionPlan,sourceBooks,stable,queries={},freshOwner;
async function call(page,path,body){const r=await page.request.post(base+path,{headers:{origin:base},data:body});return{status:r.status(),value:await r.json()};}
async function post(path,body,page=ownerPage){const r=await call(page,path,body);trace.push({step,path,status:r.status,responseKeys:r.value&&typeof r.value==='object'?Object.keys(r.value).sort():[]});if(trace.length>24)trace.shift();if(!(r.status>=200&&r.status<300&&r.value?.ok!==false)){detail={path,status:r.status,response:r.value};throw Error('canonical domain response refused');}return r.value;}
async function read(path,page=ownerPage){return get(page,path);}
function path(q){const args=new URLSearchParams();for(const[k,v]of Object.entries(q)){if(v===undefined)continue;if(k==='companySlugs')for(const slug of v)args.append('companySlug',slug);else args.set(k,String(v));}return'/api/cfo-analytics?'+args;}
async function query(q,page=ownerPage){const full=await read(path(q),page);need(full.ok===true&&full.schemaVersion==='rentemester-cfo-analytics-v1','canonical native CFO schema');return full;}
const company=(slug,extra={})=>({scope:'company',companySlugs:[slug],from,to,...extra});
const portfolio=(extra={})=>({scope:'portfolio',companySlugs:slugs,from,to,...extra});
async function books(){const out={};for(const slug of slugs){out[slug]={};for(const year of['2025','2026']){const p=prefix(slug),q='?year='+year;out[slug][year]={journal:await read(p+'journal'+q),trialBalance:await read(p+'trial-balance'+q),balance:await read(p+'balance'+q)};}}return out;}
async function originalProof(){for(const item of originals){const response=await ownerPage.request.get(base+prefix(item.company)+`documents/${item.id}/file`);need(response.status()===200&&hash(await response.body())===item.sha256,'exact authorized original bytes and SHA retained');need(isDeepStrictEqual(await read(prefix(item.company)+`documents/${item.id}/party-links`),item.partyHistory),'exact immutable document-party link history retained');}}
async function unchanged(){need(isDeepStrictEqual(await books(),sourceBooks),'CFO reads/current-access changes never mutate either complete dated journal/UUID/trial balance/balance');await originalProof();need(isDeepStrictEqual((await read(prefix(right)+`dimensions/${dimensionPlan.journalLineId}`)).assignments,stableDimensionHistory),'dimension sidecar remains immutable');}
let stableDimensionHistory;
async function setup(){step='ordinary-explicit-company-and-independent-owner-grants';await post('/api/workspace/members/access',{userId:memberId,action:'set-role',workspaceRole:'workspace_owner'});for(const slug of slugs){await post('/api/companies',{name:'Synthetic CFO '+slug,slug,fiscalYearStartMonth:'1',vatPeriodType:null});for(const userId of[ownerId,memberId])await post('/api/workspace/members/company',{userId,action:'grant',companySlug:slug,role:'owner'});}need((await read(prefix(left)+'scheduled-recurring')).scheduledRecurring.asOfDate>=to,'historical synthetic dates are nonfuture');
 step='canonical-source-party-and-actual-retained-originals';partyId=(await post(prefix(left)+'workspace-parties',{kind:'organization',name:partyName,identifiers:[{country:'DK',identifierKind:'dk_cvr',identifier:'12345678'}],role:'vendor',source:'Synthetic actual CFO retained source',observedAt:new Date().toISOString(),reviewAssertion:'Synthetic ordinary owner reviewed exact identifier',confirm:true})).party.partyId;await post(prefix(right)+`workspace-parties/${partyId}/role`,{role:'vendor',confirm:true});
 for(const slug of slugs){const isLeft=slug===left,amount=isLeft?'12.34':'56.78',asset=isLeft?'1861':'1862',equity=isLeft?'8861':'8862';for(const[accountNo,type,normalBalance]of[[asset,'asset','debit'],[equity,'equity','credit']])await post(prefix(slug)+'accounts',{accountNo,name:'Synthetic CFO '+accountNo,type,normalBalance});const bytes=Buffer.from(`Synthetic ${slug} retained posting source\n${originalDate}\nDKK ${amount}\n`),fileName=slug+'.txt';const registered=await post(prefix(slug)+'documents/ingest',{confirm:true,fileName,fileBase64:bytes.toString('base64'),metadata:{source:'Synthetic ordinary CFO source',documentType:'purchase_sale',deliveryDescription:'Synthetic CFO source supplies',issueDate:originalDate,invoiceNo:'SYN-CFO-'+(isLeft?'LEFT':'RIGHT'),amountIncVat:isLeft?12.34:56.78,vatAmount:0,currency:'DKK',sender:{name:'Synthetic extracted source '+(isLeft?'left':'right'),address:'Synthetic source street 1',vatOrCvr:'DK12345678'},recipient:{name:'Synthetic CFO '+slug,address:'Synthetic buyer street 2',vatOrCvr:'DK87654321'}}});const documentId=registered.document.id;need(Number.isSafeInteger(documentId),'real original document identity');const linkInput={documentId,partyId,role:'vendor',jurisdiction:'DK',identifierKind:'dk_cvr',identifier:'12345678'},linkPlan=(await post(prefix(slug)+'documents/party-links/plan',linkInput)).plan;need(linkPlan.partyId===partyId&&linkPlan.documentSha256===hash(bytes)&&linkPlan.evidence.kind==='exact_identifier','actual reviewed original-party proof');await post(prefix(slug)+'documents/party-links/apply',{...linkInput,planHash:linkPlan.planHash,confirm:true,idempotencyKey:'synthetic-cfo-party-'+slug});originals.push({company:slug,id:documentId,sha256:hash(bytes),partyPlanHash:linkPlan.planHash,partyHistory:await read(prefix(slug)+`documents/${documentId}/party-links`)});const journal=(await read(prefix(slug)+'journal')).journal;receipts[slug]=await post(prefix(slug)+'manual-journal',{confirm:true,input:{format:'rentemester-manual-journal-post/1',expectedRuleVersion:journal.nativePosting.ruleVersion,idempotencyKey:'synthetic-cfo-source-'+slug,request:{transactionDate:originalDate,text:'Synthetic actual CFO source '+slug,currency:'DKK',documentId:String(documentId),lines:[{accountNo:asset,debitAmount:amount,creditAmount:'0.00'},{accountNo:equity,debitAmount:'0.00',creditAmount:amount}]}}});need(Number.isSafeInteger(receipts[slug].entryId)&&/^[0-9a-f]{64}$/.test(receipts[slug].entryHash),'actual posted source ID/hash');}
 step='real-cross-date-reversal-preserves-earlier-effects';reversal=await post(prefix(left)+'manual-journal-reversal',{confirm:true,input:{format:'rentemester-manual-journal-reversal/1',expectedRuleVersion:(await read(prefix(left)+'journal')).journal.nativePosting.ruleVersion,idempotencyKey:'synthetic-cfo-cross-date-reversal',request:{entryId:String(receipts[left].entryId),transactionDate:to,reason:'Synthetic actual cross-period source reversal'}}});need(reversal.entryId!==receipts[left].entryId&&/^[0-9a-f]{64}$/.test(reversal.entryHash),'real distinct reversal receipt');
 step='explicit-hash-reviewed-dimension-sidecar';await post(prefix(right)+'dimensions/define',{dimensionId:dimension,kind:'project',name:'Synthetic CFO project',confirm:true});await post(prefix(right)+'dimensions/member',{dimensionId:dimension,memberId:dimMember,name:'Synthetic CFO member',confirm:true});const j=(await read(prefix(right)+'journal?year=2025')).journal,e=j.entries.find(e=>e.id===receipts[right].entryId),line=e?.lines.find(l=>l.accountNo==='1862');need(Number.isSafeInteger(line?.journalLineId),'immutable real source line identity');const allocations=[{dimensionId:dimension,memberId:dimMember,amountMinor:5678,currency:'DKK'}];dimensionPlan=(await post(prefix(right)+'dimensions/plan',{journalLineId:line.journalLineId,allocations,source:'reviewed'})).plan;need(dimensionPlan.lineAmountMinor===5678&&dimensionPlan.journalEntryHash===receipts[right].entryHash&&isDeepStrictEqual(dimensionPlan.allocations,allocations),'server plan binds exact selected amount/source/hash');await post(prefix(right)+'dimensions/apply',{journalLineId:line.journalLineId,allocations:dimensionPlan.allocations,source:'reviewed',planHash:dimensionPlan.planHash,idempotencyKey:'synthetic-cfo-reviewed-dimension',confirm:true});stableDimensionHistory=(await read(prefix(right)+`dimensions/${line.journalLineId}`)).assignments;need(stableDimensionHistory.length===1&&stableDimensionHistory[0].plan_hash===dimensionPlan.planHash,'one actual reviewed dimension history');sourceBooks=await books();need(sourceBooks[left]['2025'].journal.journal.ledgerUuid!==sourceBooks[right]['2025'].journal.journal.ledgerUuid,'distinct legal ledgers');await unchanged();return{companySlugs:slugs,originalDate,reversalDate:to,receipts,reversal,originals,partyId,dimensionPlan,sourceBooks,archiveActualQualified:false};}
async function capture(){step='exact-dated-live-source-filter-projections';const definitions={leftEarly:company(left,{to:earlyTo}),leftAll:company(left),leftAssetEarly:company(left,{to:earlyTo,account:'1861'}),leftAssetAll:company(left,{account:'1861'}),right:company(right),rightAsset:company(right,{account:'1862'}),rightParty:company(right,{party:partyId}),rightDimension:company(right,{dimension}),rightMember:company(right,{dimension:dimension+':'+dimMember}),rightCurrency:company(right,{currency:'DKK'}),wrongCurrency:company(right,{currency:'EUR'}),portfolio:portfolio(),sum:portfolio({aggregate:'sum'}),uiPortfolio:{scope:'portfolio',from,to}};for(const[name,q]of Object.entries(definitions))queries[name]={query:q,response:await query(q)};
 const early=queries.leftEarly.response,all=queries.leftAll.response,ea=queries.leftAssetEarly.response,aa=queries.leftAssetAll.response;need(early.rows.length===2&&early.rows.every(r=>r.journalEntryId===receipts[left].entryId&&r.sourceHash===receipts[left].entryHash&&r.transactionDate===originalDate)&&all.rows.length===4&&all.rows.some(r=>r.journalEntryId===reversal.entryId&&r.sourceHash===reversal.entryHash&&r.transactionDate===to),'later actual reversal does not erase earlier originals');need(ea.reconciliation.amountByCurrency.DKK==='12.34'&&ea.rows[0].amount==='12.34'&&aa.reconciliation.amountByCurrency.DKK==='0.00'&&isDeepStrictEqual(aa.rows.map(r=>r.amount).sort(),['-12.34','12.34']),'exact positive earlier interval and real subsequent inverse effect');
 const r=queries.right.response,asset=queries.rightAsset.response;need(r.rows.length===2&&asset.rows.length===1&&asset.rows[0].amount==='56.78'&&asset.reconciliation.amountByCurrency.DKK==='56.78','right actual amount and whole-source filter total');const original=originals.find(o=>o.company===right);for(const row of r.rows){need(row.companySlug===right&&row.sourceType==='ledger'&&row.sourceHash===receipts[right].entryHash&&row.journalEntryId===receipts[right].entryId&&row.documentId===original.id&&row.documentHash===original.sha256&&row.partyId===partyId&&row.partyName===partyName&&row.partyLinkProvenance?.planHash===original.partyPlanHash&&row.partyLinkProvenance.evidenceKind==='exact_identifier'&&row.documentPartyName==='Synthetic extracted source right'&&row.currency==='DKK'&&typeof row.amount==='string','complete actual original/hash/immutable party-link/base-currency provenance');}
 need(isDeepStrictEqual(queries.rightParty.response.rows,r.rows)&&isDeepStrictEqual(queries.rightCurrency.response.rows,r.rows),'canonical party and currency select exact source rows');for(const name of['rightDimension','rightMember']){const filtered=queries[name].response;need(filtered.rows.length===1&&filtered.rows[0].sourceId===asset.rows[0].sourceId&&filtered.rows[0].amount==='56.78'&&isDeepStrictEqual(filtered.rows[0].dimensions,[{dimensionId:dimension,memberId:dimMember,amountMinor:5678,currency:'DKK'}])&&filtered.reconciliation.amountByCurrency.DKK==='56.78','dimension filter retains whole real line with exact existing sidecar');}need(queries.wrongCurrency.response.rows.length===0&&queries.wrongCurrency.response.reconciliation.rowCount===0,'absent currency yields genuinely empty matching source');need(queries.portfolio.response.mode==='juxtaposed-non-consolidated'&&queries.portfolio.response.aggregate==='none'&&!('amountByCurrency'in queries.portfolio.response.reconciliation),'portfolio has no implicit scalar total');need(queries.sum.response.rows.length===6&&queries.sum.response.reconciliation.rowCount===6&&queries.sum.response.reconciliation.amountByCurrency.DKK==='0.00'&&queries.sum.response.reconciliation.sourceHashes.length===3,'explicit caller-requested full-visible exact sum includes all real sources');for(const item of Object.values(queries)){need(item.response.rows.every(row=>row.sourceType==='ledger')&&item.response.status==='ready','supported live sources are verified ready; archive proof stays open');}await unchanged();return{queries,archiveActualQualified:false,datedReversalCorrectionQualified:true};}
async function pagination(){step='stable-source-cursor-pages-full-totals-before-limit';const q=portfolio({aggregate:'sum',limit:1}),full=queries.sum.response,collected=[],pages=[],seen=new Set();let cursor;for(let n=0;n<10;n++){const page=await query({...q,...(cursor?{cursor}:{})});need(page.rows.length===1&&isDeepStrictEqual(page.reconciliation,full.reconciliation),'each source page retains exact whole matching totals/hashes');const row=page.rows[0],key=[row.transactionDate,row.sourceType,row.companySlug,row.sourceId,row.accountNo].join('|');need(!seen.has(key),'stable cursor never duplicates a legal source row');seen.add(key);collected.push(row);pages.push({query:{...q,...(cursor?{cursor}:{})},response:page});if(!page.page.nextCursor){need(isDeepStrictEqual(collected,full.rows),'all pages equal deterministic complete unpaged source projection');queries.page=pages[0];await unchanged();return{pages,completeRows:collected,fullTotalsRetained:true};}need(Buffer.from(page.page.nextCursor,'base64url').toString('utf8')===key,'documented source cursor binds exact stable tuple');cursor=page.page.nextCursor;}throw Error('bounded source pagination did not terminate');}
async function uiCompany(slug,finalTo,expected){diagnosticPage=ownerPage;await ownerPage.goto(base+'/cfo');await ownerPage.getByRole('heading',{name:'CFO-overblik',exact:true}).waitFor();const controls=ownerPage.getByRole('group',{name:'CFO-afgrænsning',exact:true});await controls.getByLabel('Virksomhed',{exact:true}).selectOption(slug);await controls.getByLabel('Fra dato',{exact:true}).fill(from);const wait=ownerPage.waitForResponse(r=>{const u=new URL(r.url());return u.pathname==='/api/cfo-analytics'&&u.searchParams.get('scope')==='company'&&u.searchParams.get('companySlug')===slug&&u.searchParams.get('from')===from&&u.searchParams.get('to')===finalTo;});await controls.getByLabel('Pr. dato',{exact:true}).fill(finalTo);const response=await wait;need(response.status()===200&&isDeepStrictEqual(await response.json(),expected),'actual company/date UI uses identical full canonical projection');const rows=ownerPage.getByRole('region',{name:'Kildeposter',exact:true}).locator('tbody tr');await rows.first().waitFor();need(await rows.count()===expected.rows.length,'rendered actual source row count');for(const row of expected.rows){const entry=rows.filter({has:ownerPage.getByText('Kildedetaljer '+row.sourceId,{exact:true})});need(await entry.count()===1,'company+source ID scoped actual rendered row');const human=row.amount.replace('.',',')+' kr.';await entry.getByRole('cell',{name:human,exact:true}).waitFor();await entry.getByText('Kildedetaljer '+row.sourceId,{exact:true}).click();await entry.getByText(row.sourceHash,{exact:true}).waitFor();if(row.documentId!==null)need(await entry.getByRole('link',{name:'Åbn originalbilag',exact:true}).getAttribute('href')===prefix(slug)+`documents/${row.documentId}/file`,'original link uses actual company+document identity');}return{renderedRows:expected.rows.length,canonicalProjectionMatched:true};}
async function ui(){step='actual-cfo-company-date-source-drilldown';const views=[];views.push(await uiCompany(left,earlyTo,queries.leftEarly.response));views.push(await uiCompany(left,to,queries.leftAll.response));views.push(await uiCompany(right,to,queries.right.response));await ownerPage.screenshot({path:process.env.RM_CFO_SCREENSHOT,fullPage:true});step='actual-portfolio-warning-no-implicit-total';const controls=ownerPage.getByRole('group',{name:'CFO-afgrænsning',exact:true}),wait=ownerPage.waitForResponse(r=>{const u=new URL(r.url());return u.pathname==='/api/cfo-analytics'&&u.searchParams.get('scope')==='portfolio'&&u.searchParams.get('from')===from&&u.searchParams.get('to')===to;});await controls.getByLabel('Visning',{exact:true}).selectOption('portfolio');const response=await wait;need(response.status()===200&&isDeepStrictEqual(await response.json(),queries.uiPortfolio.response),'actual default portfolio source response');await ownerPage.getByText('Porteføljen er en sideordnet, ikke-konsolideret visning. Der vises ingen implicitte elimineringer eller valutaomregning.',{exact:true}).waitFor();need(await ownerPage.getByRole('region',{name:'Kildebaserede beløb',exact:true}).count()===0,'no implicit rendered portfolio total');step='actual-group-no-approved-profile-no-figures';const profiles=ownerPage.waitForResponse(r=>new URL(r.url()).pathname==='/api/group-report-profiles');await controls.getByLabel('Visning',{exact:true}).selectOption('group');const listed=await profiles;need(listed.status()===200&&(await listed.json()).profiles.length===0,'fresh fixture has no approved report profile');await ownerPage.getByText('Ingen godkendt koncernrapportprofil er tilgængelig pr. den valgte dato.',{exact:true}).waitFor();need(await ownerPage.getByRole('region',{name:'Koncernrapport',exact:true}).count()===0,'no manufactured consolidated figures');await unchanged();return{companyDateViews:views,defaultPortfolioNoImplicitSum:true,noApprovedGroupProfileHonest:true,approvedGroupActualQualified:false};}
async function refusals(){step='strict-unsupported-selector-date-and-cursor-refusals';const basePath=path(company(left));for(const suffix of['&cursor=not-a-valid-cursor','&limit=0','&account=1861&account=8861','&unexpected=source-facts']){const response=await ownerPage.request.get(base+basePath+suffix);need(response.status()===400&&(await response.json()).ok===false,'strict CFO selectors refuse without false ready projection');}const response=await ownerPage.request.get(base+path(company(left,{from:'2025-02-30'})));need(response.status()===400&&(await response.json()).ok===false,'impossible date refused');await unchanged();return{strictDateCursorSingletonUnknownSelectorRefused:true};}
async function rights(){step='normal-current-right-revocation-and-partial-redaction';await post('/api/workspace/members/company',{userId:ownerId,action:'revoke',companySlug:right},memberPage);const direct=await ownerPage.request.get(base+path(company(right)));need([403,404].includes(direct.status())&&(await direct.json()).ok===false,'current direct inaccessible company refuses');const partial=await query(portfolio({aggregate:'sum'}));need(partial.status==='incomplete'&&partial.partial===true&&isDeepStrictEqual(partial.companies,[left])&&partial.rows.length===4&&partial.rows.every(r=>r.companySlug===left)&&!('amountByCurrency'in partial.reconciliation)&&!JSON.stringify(partial).includes(right)&&!JSON.stringify(partial).includes(receipts[right].entryHash),'partial explicit portfolio hides actual right-company identities/hashes/counts/totals');need(partial.reconciliation.rowCount===4&&partial.reconciliation.sourceHashes.length===2,'only visible whole-source control counts/hashes');
 const currentDefault=await query({scope:'portfolio',from,to});await ownerPage.goto(base+'/cfo');await ownerPage.getByRole('heading',{name:'CFO-overblik',exact:true}).waitFor();const controls=ownerPage.getByRole('group',{name:'CFO-afgrænsning',exact:true});await controls.getByLabel('Fra dato',{exact:true}).fill(from);await controls.getByLabel('Pr. dato',{exact:true}).fill(to);const wait=ownerPage.waitForResponse(r=>{const u=new URL(r.url());return u.pathname==='/api/cfo-analytics'&&u.searchParams.get('scope')==='portfolio'&&u.searchParams.get('from')===from&&u.searchParams.get('to')===to;});await controls.getByLabel('Visning',{exact:true}).selectOption('portfolio');const response=await wait;need(response.status()===200&&isDeepStrictEqual(await response.json(),currentDefault),'actual current partial portfolio UI matches only authorized canonical source');await ownerPage.getByText('Ufuldstændigt udsnit: skjulte virksomheder og deres tal er ikke medtaget.',{exact:true}).waitFor();need(await ownerPage.getByRole('region',{name:'Kildebaserede beløb',exact:true}).count()===0,'partial actual UI has no scalar total');await post('/api/workspace/members/company',{userId:ownerId,action:'grant',companySlug:right,role:'owner'},memberPage);need(isDeepStrictEqual(await query(portfolio({aggregate:'sum'})),queries.sum.response),'current explicit regrant restores exact authorized projection');await unchanged();return{directStatus:direct.status,partial,revocationAndRegrantActualQualified:true,partyVisibilityPolicy:'existing-any-visible-associated-company'};}
async function client(kind,q){const file=join(work,'client.cookie');writeFileSync(file,(await ownerPage.context().cookies(base)).map(c=>`${c.name}=${c.value}`).join('; '),{mode:0o600,flag:'wx'});try{const common=['--server',base,'--cookie-file',file,'--allow-loopback-http'];let argv,input;if(kind==='cli'){argv=['remote','cfo-analytics',...common,'--scope',q.scope,'--from',q.from,'--to',q.to];if(q.companySlugs)argv.push(q.scope==='company'?'--company':'--company-slugs-json',q.scope==='company'?q.companySlugs[0]:JSON.stringify(q.companySlugs));for(const[key,flag]of[['groupProfileId','--group-profile-id'],['account','--account'],['party','--party'],['dimension','--dimension'],['currency','--currency'],['aggregate','--aggregate'],['cursor','--cursor'],['limit','--limit']])if(q[key]!==undefined)argv.push(flag,String(q[key]));}else{argv=common;input=[{jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'synthetic-cfo-proof',version:'1'}}},{jsonrpc:'2.0',method:'notifications/initialized'},{jsonrpc:'2.0',id:2,method:'tools/call',params:{name:'cfo_analytics',arguments:q}}].map(v=>JSON.stringify(v)).join('\n')+'\n';}const result=spawnSync(join(work,kind==='cli'?'rentemester-rust':'rentemester-mcp'),argv,{input,encoding:'utf8',timeout:45000,maxBuffer:16*1024*1024});if(result.error||result.status!==0){detail={kind,query:q,status:result.status,errorCode:result.error?.code,stdout:result.stdout?.slice(0,8192),stderr:result.stderr?.slice(0,8192)};throw Error('readonly native CFO client terminal');}const full=kind==='cli'?JSON.parse(result.stdout):result.stdout.trim().split('\n').map(v=>JSON.parse(v)).find(v=>v.id===2)?.result?.structuredContent;need(full?.ok===true&&full.data,'full canonical CFO client envelope');return full.data;}finally{unlinkSync(file);}}
async function clients(){step='full-http-cli-mcp-current-canonical-filter-and-page-parity';for(const item of Object.values(queries))for(const kind of['cli','mcp'])need(isDeepStrictEqual(await client(kind,item.query),item.response),'full canonical HTTP CLI MCP CFO parity including exact filters/pages/totals');await unchanged();return{httpCliMcpFullCanonicalParity:true,noClientMathOrImplicitSum:true};}
async function snapshot(){const views={};for(const[name,item]of Object.entries(queries))views[name]=await query(item.query);return{views,books:await books(),originals,party:await read(prefix(left)+`workspace-parties/${partyId}`),dimensionHistory:(await read(prefix(right)+`dimensions/${dimensionPlan.journalLineId}`)).assignments};}
async function saveStable(){step='save-complete-supported-source-projection';stable=await snapshot();await unchanged();return{stable,archiveActualQualified:false,archiveActualBlocker:'No public admitted native retained-posting archive ingestion; source TS CLI directly opens/migrates outside native Owner and fresh source namespaces have no adoption path.',fullR28Qualified:false};}
async function assertStable(){await unchanged();need(isDeepStrictEqual(await snapshot(),stable),'complete supported CFO source reconstruction/rows/totals/hashes/filters/pages/UUID/history unchanged');}
async function reopen(){step='fresh-password-mfa-restart-read-only-reconstruction';for(const[page,user,id]of[[ownerPage,owner,ownerId],[memberPage,member,memberId]]){await page.context().clearCookies();need((await login(page,user)).user.id===id,'same current normal MFA users');}await assertStable();await clients();await uiCompany(left,earlyTo,queries.leftEarly.response);return{freshTwoMfaRestart:true,sourceViewsRetained:true,noEffects:true};}
async function archive(name){step='explicit-signed-supported-live-cfo-'+name;const r=await ownerPage.request.post(base+'/api/workspace/backup',{headers:{origin:base},data:{confirm:true}});need(r.status()===200&&r.headers()['content-type']==='application/vnd.rentemester.native-backup+json','supported actual original/journal/party/dimension inventory must export honestly');const bytes=await r.body();need(bytes.length>0&&bytes.length<=96*1024*1024,'bounded exact signed artifact');writeFileSync(join(work,'cfo-'+name+'.native-backup.json'),bytes,{mode:0o600,flag:'wx'});await unchanged();return{supported:true,name,bytes:bytes.length,sha256:hash(bytes),archiveActualQualified:false};}
async function restored(){step='fresh-restored-owner-normal-email-verification-and-mfa';ownerPage=await newPage();freshOwner={email:'restored@example.invalid',password:'synthetic-restored-password-789',codes:[]};await ownerPage.goto((await mail('verification',freshOwner.email)).actionUrl);const me=await enroll(ownerPage,freshOwner);need(me.user.id!==ownerId&&me.user.id!==memberId,'historical identities are not restored authority');await assertStable();await clients();await uiCompany(right,to,queries.right.response);return{sourceFreeRestoreQualified:true,freshOwnerMfa:true,liveCfoReconstructed:true,archiveActualQualified:false};}
async function restoredReopen(){step='fresh-restored-owner-reopen-read-only';await ownerPage.context().clearCookies();await login(ownerPage,freshOwner);await assertStable();return{restoredTargetReopenQualified:true,noEffects:true};}
for await(const line of createInterface({input:process.stdin,crlfDelay:Infinity})){try{const action=JSON.parse(line);let value;if(action==='authenticate')value=await authenticate();else if(action==='setup')value=await setup();else if(action==='capture')value=await capture();else if(action==='pagination')value=await pagination();else if(action==='ui')value=await ui();else if(action==='refusals')value=await refusals();else if(action==='rights')value=await rights();else if(action==='clients')value=await clients();else if(action==='save-stable')value=await saveStable();else if(action==='reopen')value=await reopen();else if(action==='archive-before'||action==='archive-final')value=await archive(action.slice(8));else if(action==='restored')value=await restored();else if(action==='restored-reopen')value=await restoredReopen();else if(action==='finish'){await browser?.close();emit({ok:true,value:{finished:true}});break;}else throw Error('unknown phase');emit({ok:true,value});}catch(error){let pageState=null;try{pageState=await Promise.race([diagnosticPage?.evaluate(()=>({pathname:location.pathname,headings:[...document.querySelectorAll('h1,h2,h3')].map(e=>e.textContent.slice(0,100)),buttons:[...document.querySelectorAll('button')].map(e=>e.textContent.slice(0,100)),alerts:document.querySelectorAll('[role=alert]').length})),new Promise(resolve=>setTimeout(()=>resolve(null),2000))]);}catch{}emit({ok:false,step,detail,errorType:error?.name,trace,pageState});await Promise.race([browser?.close().catch(()=>{}),new Promise(resolve=>setTimeout(resolve,2000))]);process.exitCode=1;break;}}

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
    report = {'status': 'running', 'sourceRevision': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=REPO, text=True).strip(), 'runnerSha256': digest(Path(__file__)), 'browserHelperSha256': hashlib.sha256(BROWSER.encode()).hexdigest(), 'canonicalOrigin': origin, 'noOfflineIdentitySeeds': True, 'ordinaryMacOsUser': True, 'ownedProfile': str(work), 'externalMailDelivered': False, 'workflow': 'live_cfo_source_analytics', 'archiveActualQualified': False, 'archiveActualBlocker': 'No public admitted native retained-posting archive writer; source CLI uses direct openDb/migrate and no native adoption route is available.', 'fullR28Qualified': False, 'fullGroup61Qualified': False, 'approvedGroupActualQualified': False, 'uiPaginationClickActualQualified': False, 'installedNativeQualified': False, 'screenshot': str(args.evidence.with_suffix('.png')),  'actions': [], 'results': {}, 'sourceFreeRestoreQualified': False}
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
                argv += ['--restore-backup', str(work / 'cfo-final.native-backup.json'), '--trusted-backup-key', str(work / 'trusted.pub'), '--confirm-target', str(root), '--source-unavailable']
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

        def company_audit(name):
            progress('operator:verify-and-inspect-own-signed-company-audit-' + name)
            path = work / ('cfo-' + name + '.native-backup.json')
            receipt = report['results']['archive-' + name]
            if digest(path) != receipt['sha256']:
                raise RuntimeError('Downloaded signed bytes changed')
            verified = subprocess.run([str(pinned['server_binary']), '--verify-backup', str(path), '--trusted-backup-key', str(work / 'trusted.pub')], cwd=REPO, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=60)
            if verified.returncode != 0:
                raise RuntimeError('Separate trusted-key signature/inventory verification refused')
            entries = json.loads(path.read_bytes())['entries']
            result = {}
            for slug in ['synthetic-cfo-left', 'synthetic-cfo-right']:
                rows = [entry for entry in entries if entry['path'] == slug + '/data/ledger.sqlite']
                if len(rows) != 1:
                    raise RuntimeError('Exact signed company snapshot missing')
                decoded = base64.b64decode(rows[0]['bytesBase64'], validate=True)
                if not 0 < len(decoded) <= 32 * 1024 * 1024:
                    raise RuntimeError('Signed company snapshot byte bound')
                copy = work / (name + '-' + slug + '-readonly.sqlite')
                try:
                    with copy.open('xb') as output:
                        output.write(decoded)
                    # Read only the downloaded, independently verified synthetic
                    # archive, never a live database or a fixture seeding path.
                    db = sqlite3.connect(copy.as_uri() + '?mode=ro', uri=True)
                    try:
                        db.execute('PRAGMA query_only=ON')
                        query = db.execute('SELECT * FROM audit_log ORDER BY id')
                        columns = [column[0] for column in query.description]
                        result[slug] = [dict(zip(columns, row)) for row in query.fetchall()]
                    finally:
                        db.close()
                finally:
                    copy.unlink(missing_ok=True)
            report.setdefault('companyAuditProofs', {})[name] = result
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
                ['openssl', 'req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', str(work / 'ca.key'), '-out', str(ca), '-days', '1', '-subj', '/CN=Synthetic CFO Auth CA', '-addext', 'basicConstraints=critical,CA:TRUE', '-addext', 'keyUsage=critical,keyCertSign,cRLSign'],
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
            config = {'runtimeProfile': 'local-user-macos', 'root': str(work / 'workspace'), 'brokerUid': os.getuid(), 'workspace': 'synthetic-native-cfo', 'companySlug': 'synthetic', 'companyName': 'Synthetic CFO Anchor', 'cookieKey': key(), 'mfaKey': key(), 'operatorActors': ['system:synthetic-installer'], 'accounts': [], 'firstOwner': {'actor': 'system:synthetic-installer', 'email': 'owner@example.invalid', 'name': 'Synthetic Owner', 'password': 'synthetic-owner-password-123'}, 'backupSigningKeyFile': str(work / 'signing.der'), 'authMail': {'endpoint': f'https://127.0.0.1:{mail_port}/mail', 'bearerToken': bearer, 'from': 'auth@example.invalid', 'idempotencySecret': secrets.token_urlsafe(32), 'trustedCaFile': str(ca)}}
            write_config()
            start(create=True)
            script = work / 'browser.mjs'
            script.write_text(BROWSER)
            controller = subprocess.Popen(['/opt/homebrew/bin/bun', str(script)], cwd=REPO, env={**os.environ, 'RM_CFO_BASE': origin, 'RM_CFO_WORK': str(work), 'RM_CFO_SCREENSHOT': str(args.evidence.with_suffix('.png')), 'RM_CFO_PLAYWRIGHT': str((REPO / 'node_modules/@playwright/test/index.mjs').resolve(strict=True))}, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, bufsize=1, start_new_session=True)
            report['browserControllerPid'] = controller.pid
            phase('authenticate')
            phase('setup')
            phase('archive-before')
            original_audit = company_audit('before')
            for action in ['capture', 'pagination', 'ui', 'refusals', 'rights', 'clients', 'save-stable']:
                phase(action)
            stop()
            config.pop('firstOwner')
            write_config()
            start()
            phase('reopen')
            archive = phase('archive-final')
            if company_audit('final') != original_audit:
                raise RuntimeError('CFO reads/controls changed either company business audit')
            report['companyBusinessAuditUnchanged'] = True
            report['signedSnapshot'] = archive
            progress('operator:independent-signed-snapshot-verification')
            path = work / 'cfo-final.native-backup.json'
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
            report.update({'status': 'passed', 'currentAction': 'complete', 'normalTwoMfaLiveCfoQualified': True, 'initialAndRestartQualified': True, 'snapshotCoverage': 'supported-live-signed-source-free-restored-and-reopened'})
            save()
            print('PASS: supported ordinary two-MFA live CFO source/filter/portfolio/reversal journey and source-free reconstruction; archive ingestion qualification remains open; ' + str(args.evidence))
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
