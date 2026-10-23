#!/usr/bin/env python3
"""Owned native atomic purchase batch proof; supplied artifacts, no builds.

Normal invitation/email/MFA, real archived originals/CSV, independent reviewed
rule and batch, canonical receipts/financial controls, native clients and restart.
Only a successful signed export can enter the source-free fresh-owner branch.
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
import sqlite3
import subprocess
import threading
import time
import uuid
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
PREFIX = 'Rentemester-Synthetic-Batch-'
BROWSER = r'''

import {createHmac,createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {createInterface} from 'node:readline';
import {readFileSync,readdirSync,writeFileSync,unlinkSync} from 'node:fs';
import {join} from 'node:path';
import {isDeepStrictEqual} from 'node:util';
const {chromium}=await import(process.env.RM_BATCH_PLAYWRIGHT);
const base=process.env.RM_BATCH_BASE,work=process.env.RM_BATCH_WORK;
if(!/^http:\/\/127\.0\.0\.1:\d+$/.test(base))throw Error('owned literal loopback origin required');
const owner={email:'owner@example.invalid',password:'synthetic-owner-password-123',codes:[]};
const member={email:'invited@example.invalid',password:'synthetic-invited-password-456',codes:[]};

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

const prefix='/api/companies/synthetic/',batch=prefix+'bookkeeping-batch',hash=b=>createHash('sha256').update(b).digest('hex');
let originalPostingRulesProof, originalDimensionsProof, originalCommitmentProof, originalPartyProof, originalExpenseProof, originalAccrualProof, originalAssetProof, originalMileageProof, originalBudgetProof, originalVatProof, originalVatPurchaseProof, historicalVatProof, restoredUser;
async function originalPostingRulesSetup(){const m=await import(process.env.RM_POSTING_RULES_ORIGINAL_HELPER);originalPostingRulesProof=m.createOriginalPostingRulesProof(base,work,name=>{step='original-posting-rules:'+name;detail=null;});return originalPostingRulesProof.setup(ownerPage,memberPage);}
async function originalDimensionsRun(){const m=await import(process.env.RM_DIMENSIONS_ORIGINAL_HELPER);originalDimensionsProof=m.createOriginalDimensionsProof(base,work,name=>{step='original-dimensions:'+name;detail=null;});return originalDimensionsProof.run(ownerPage,memberPage);}
async function originalCommitmentRun(){const m=await import(process.env.RM_COMMITMENT_ORIGINAL_HELPER);originalCommitmentProof=m.createOriginalCommitmentProof(base,work,name=>{step='original-commitment:'+name;detail=null;});return originalCommitmentProof.run(ownerPage,memberPage);}
async function historicalVatRun(){const m=await import(process.env.RM_HISTORICAL_VAT_HELPER);historicalVatProof=m.createHistoricalVatProof(base,work,name=>{step='historical-vat:'+name;detail=null;});return historicalVatProof.run(ownerPage,memberPage);}
async function originalVatPurchaseRun(){const m=await import(process.env.RM_VAT_PURCHASE_ORIGINAL_HELPER);originalVatPurchaseProof=m.createOriginalVatPurchaseProof(base,work,name=>{step='original-vat-purchase:'+name;detail=null;});return originalVatPurchaseProof.run(ownerPage,memberPage);}
async function originalVatRun(){const m=await import(process.env.RM_VAT_ORIGINAL_HELPER);originalVatProof=m.createOriginalVatProof(base,work,name=>{step='original-vat:'+name;detail=null;});return originalVatProof.run(ownerPage,memberPage);}
async function originalBudgetRun(){const m=await import(process.env.RM_BUDGET_ORIGINAL_HELPER);originalBudgetProof=m.createOriginalBudgetProof(base,work,name=>{step='original-budget:'+name;detail=null;});return originalBudgetProof.run(ownerPage,memberPage);}
async function originalMileageRun(){const m=await import(process.env.RM_MILEAGE_ORIGINAL_HELPER);originalMileageProof=m.createOriginalMileageProof(base,work,name=>{step='original-mileage:'+name;detail=null;});return originalMileageProof.run(ownerPage,memberPage);}
async function originalAssetRun(){const m=await import(process.env.RM_ASSET_ORIGINAL_HELPER);originalAssetProof=m.createOriginalAssetProof(base,work,name=>{step='original-asset:'+name;detail=null;});return originalAssetProof.run(ownerPage,originals,today,memberPage);}
async function originalAccrualRun(){const m=await import(process.env.RM_ACCRUAL_ORIGINAL_HELPER);originalAccrualProof=m.createOriginalAccrualProof(base,work,name=>{step='original-accrual:'+name;detail=null;});return originalAccrualProof.run(ownerPage,originals,today,memberPage);}
async function originalExpenseRun(){const m=await import(process.env.RM_EXPENSE_ORIGINAL_HELPER);originalExpenseProof=m.createOriginalExpenseProof(base,work,name=>{step='original-expense:'+name;detail=null;});return originalExpenseProof.run(ownerPage,originals,today);}
async function originalPartyRun(){const m=await import(process.env.RM_DOCUMENT_PARTY_ORIGINAL_HELPER);originalPartyProof=m.createDocumentPartyOriginalProof(base,work,name=>{step="document-party-original:"+name;detail=null;});return originalPartyProof.run(ownerPage,originals,today);}
let today,scope,plan,persisted,approval,applied,stable,originals=[],rule,originalPlanResponse,supplier,linkedOriginals=[];
const reviewLabel='Jeg har gennemgået alle poster, kildebeviser og den præcise plan-hash';
async function call(page,path,body,method='POST'){const response=await page.request.fetch(base+path,{method,headers:{origin:base,'content-type':'application/json'},data:body});let value;try{value=await response.json();}catch{value=null;}return{status:response.status(),value};}
async function post(page,path,body,method='POST'){const r=await call(page,path,body,method);need(r.status===200&&r.value?.ok===true,`${path} canonical HTTP${r.status} ${r.value?.code??''}`);return r.value;}
async function exactGet(page,path){const r=await page.request.get(base+path);need(r.status()===200,`${path} read HTTP${r.status()}`);return r.json();}
async function responseClick(page,locator,path){const pending=page.waitForResponse(r=>new URL(r.url()).pathname===path&&r.request().method()==='POST');pending.catch(()=>{});await locator.click();const r=await pending;need(r.status()===200,'explicit confirmed '+path+' HTTP'+r.status());const value=await r.json();need(value.ok===true,'canonical confirmed success');return{value,body:r.request().postDataJSON()};}
async function originalProof(page){const proof=[];for(const original of originals){const r=await page.request.get(base+prefix+`documents/${original.id}/file`);need(r.ok(),'authorized original readable');need(hash(await r.body())===original.sha256,'exact original bytes immutable');proof.push(original);}return proof;}
async function reports(page){return{journal:await exactGet(page,prefix+'journal'),trialBalance:await exactGet(page,prefix+'trial-balance'),bank:await exactGet(page,prefix+'bank'),vat:await exactGet(page,prefix+'vat')};}
async function intake(){diagnosticPage=ownerPage;step='ordinary-explicit-profile-chart-bank-and-originals';await post(ownerPage,prefix+'company',{cvr:'87654321',address:'Synthetic Batch Street 1',postalCode:'1000',city:'Synthetic City',vatPeriodType:'quarter',paymentTermsDays:14},'PATCH');for(const[accountNo,type,normalBalance]of[['1800','asset','debit'],['3000','expense','debit'],['4700','vat','debit']])await post(ownerPage,prefix+'accounts',{accountNo,name:'Synthetic Batch '+accountNo,type,normalBalance});for(const[role,accountNo]of[['bank','1800'],['input_vat','4700']])await post(ownerPage,prefix+'account-roles',{role,accountNo,confirm:true});await post(ownerPage,prefix+'bank-accounts',{slug:'synthetic-batch-bank',name:'Synthetic Batch Bank',currency:'DKK',ledgerAccountNo:'1800'});today=(await exactGet(ownerPage,prefix+'scheduled-recurring')).scheduledRecurring.asOfDate;need(/^\d{4}-\d{2}-\d{2}$/.test(today),'server Copenhagen date');
for(const[number,gross,vat]of[[1,125,25],[2,250,50]]){const bytes=Buffer.from(`Synthetic batch original ${number}\nSupplier DK12345678\nGross ${gross}.00 VAT ${vat}.00 DKK\n`),filename=`synthetic-batch-purchase-${number}.txt`;const meta={source:'Synthetic batch ordinary intake',documentType:'purchase_sale',issueDate:today,invoiceNo:'SYN-BATCH-'+number,deliveryDescription:'Synthetic supplies '+number,amountIncVat:gross,vatAmount:vat,currency:'DKK',sender:{name:'Synthetic supplier',address:'Synthetic supplier street 1',vatOrCvr:'DK12345678'},recipient:{name:'Synthetic Batch Company',address:'Synthetic Batch Street 1',vatOrCvr:'DK87654321'}};await post(ownerPage,prefix+'documents/ingest',{confirm:true,fileName:filename,fileBase64:bytes.toString('base64'),metadata:meta});const listed=(await exactGet(ownerPage,prefix+'documents')).documents.documents.filter(d=>d.filename===filename);need(listed.length===1,'one canonical original registration');originals.push({id:listed[0].id,documentNo:listed[0].documentNo,filename,sha256:hash(bytes),gross,vat});}
const csv=`dato;tekst;beløb;valuta;saldo\n${today};SYN-BATCH-1;-125,00;DKK;-125,00\n${today};SYN-BATCH-2;-250,00;DKK;-375,00\n`;const imported=await post(ownerPage,prefix+'bank/import',{confirm:true,csvContent:csv,account:'synthetic-batch-bank'});need(imported.import.imported===2&&imported.import.skippedDuplicates===0,'two real archived CSV rows');const bank=(await exactGet(ownerPage,prefix+'bank')).bank;need(bank.transactions.length===2&&bank.unmatchedCount===2&&bank.actualBalance===-375&&bank.bookedBalance===0,'real statement balance independent of books');need((await exactGet(ownerPage,prefix+'journal')).journal.entries.length===0,'clean pre-batch journal');await originalProof(ownerPage);scope={accountingFrom:today,accountingTo:today,bankFrom:today,bankTo:today};return{today,scope,originals,csvSha256:hash(Buffer.from(csv)),import:imported};}
async function rules(){diagnosticPage=ownerPage;step='web-author-explicit-purchase-rule';await ownerPage.goto(base+'/companies/synthetic/posteringsregler');await ownerPage.getByRole('heading',{name:'Posteringsregler',exact:true}).waitFor();await ownerPage.getByLabel('Regeltype',{exact:true}).selectOption('purchase');for(const[label,value]of[['Regel-id','synthetic-batch-purchases'],['Leverandørens præcise navn','Synthetic supplier'],['Leverandørens momsnummer','DK12345678'],['Foreslået modkonto','3000'],['Gælder fra',today],['Begrundelse','Synthetic owner reviewed exact originals and explicit VAT allocation']])await ownerPage.getByLabel(label,{exact:true}).fill(value);for(const[label,value]of[['Bilagstype','purchase_sale'],['Leverandørland','DK'],['Moms i bilagets kilde','positive'],['Momsbehandling','standard']])await ownerPage.getByLabel(label,{exact:true}).selectOption(value);await ownerPage.getByRole('button',{name:'Gennemgå forslag',exact:true}).click();const d=ownerPage.getByRole('dialog',{name:'Gem regelforslag',exact:true});await d.waitFor();need((await d.innerText()).includes('standard')&&(await d.innerText()).includes('DK12345678'),'full explicit rule review rendered');await responseClick(ownerPage,d.getByRole('button',{name:'Bekræft',exact:true}),prefix+'posting-rules/propose');await d.waitFor({state:'hidden'});let listed=await exactGet(ownerPage,prefix+'posting-rules');rule=listed.postingRules.find(r=>r.ruleId==='synthetic-batch-purchases');need(rule?.state==='proposed'&&rule.outcome.vatTreatment==='standard','real proposed VAT rule');const self=await call(ownerPage,prefix+'posting-rules/approve',{confirm:true,ruleId:rule.ruleId,version:rule.version,expectedPayloadHash:rule.payloadHash,rationale:'Synthetic forbidden same author approval',provenance:'Synthetic actual fixture'});need(self.status===422,'rule author cannot approve own rule');need(isDeepStrictEqual(await exactGet(ownerPage,prefix+'posting-rules'),listed),'self-review refusal preserves rule history');diagnosticPage=memberPage;step='independent-web-rule-review';await memberPage.goto(base+'/companies/synthetic/posteringsregler');await memberPage.getByRole('heading',{name:'Posteringsregler',exact:true}).waitFor();const row=memberPage.getByRole('row').filter({hasText:rule.ruleId});await row.getByRole('button',{name:'Godkend regel',exact:true}).click();const dialog=memberPage.getByRole('dialog',{name:'Godkend præcis regelversion',exact:true});await dialog.getByLabel('Begrundelse',{exact:true}).fill('Synthetic independent reviewer checked supplier originals and exact rule hash');const approved=await responseClick(memberPage,dialog.getByRole('button',{name:'Bekræft',exact:true}),prefix+'posting-rules/approve');need(approved.body.expectedPayloadHash===rule.payloadHash,'independent exact hash approval');await dialog.waitFor({state:'hidden'});rule=(await exactGet(ownerPage,prefix+'posting-rules')).postingRules.find(r=>r.ruleId==='synthetic-batch-purchases');need(rule.state==='approved','canonical rule approved');return{rule,selfApprovalRefused:true};}
async function expandProof(surface){const summary=surface.locator('summary').filter({hasText:/^Præcis plan, poster og kildebeviser$/});await summary.waitFor();const details=summary.locator('..');if(await details.getAttribute('open')===null)await summary.click();}
async function inspectHash(page){await expandProof(page);await page.getByText(plan.planHash,{exact:true}).waitFor();}
async function linkParties(){diagnosticPage=ownerPage;step='explicit-canonical-supplier-and-vendor-company-role';const created=await post(ownerPage,prefix+'workspace-parties',{kind:'organization',name:'Synthetic supplier',identifiers:[{country:'DK',identifierKind:'dk_cvr',identifier:'12345678'}],source:'Synthetic batch retained supplier originals',observedAt:today,reviewAssertion:'Synthetic owner explicitly reviewed supplier CVR against both retained originals',role:'supplier',confirm:true});need(typeof created.party?.partyId==='string','real canonical supplier identity');const partyId=created.party.partyId;await post(ownerPage,prefix+`workspace-parties/${encodeURIComponent(partyId)}/role`,{role:'vendor',confirm:true});supplier=await exactGet(ownerPage,prefix+`workspace-parties/${encodeURIComponent(partyId)}`);need(supplier.party.roles.some(r=>r.companySlug==='synthetic'&&r.role==='supplier')&&supplier.party.roles.some(r=>r.companySlug==='synthetic'&&r.role==='vendor'),'explicit supplier and vendor roles, no inferred default');for(const original of originals){step='review-exact-identifier-party-plan-'+original.id;const path=prefix+`documents/${original.id}/party-links`,before=await exactGet(ownerPage,path);need(before.links.length===0,'fresh original has no preselected party');const input={documentId:original.id,partyId,role:'vendor',jurisdiction:'DK',identifierKind:'dk_cvr',identifier:'12345678'};const reviewed=await post(ownerPage,prefix+'documents/party-links/plan',input);const plan=reviewed.plan;need(plan?.documentId===original.id&&plan.partyId===partyId&&plan.role==='vendor'&&plan.documentSha256===original.sha256&&/^[a-f0-9]{64}$/.test(plan.documentPayloadSha256)&&/^[a-f0-9]{64}$/.test(plan.planHash),'exact immutable document/canonical-party hash binding');need(isDeepStrictEqual(plan.evidence,{kind:'exact_identifier',jurisdiction:'DK',identifierKind:'dk_cvr',identifier:'DK12345678'}),'reviewed exact normalized identifier evidence, never name inference');need(plan.partySnapshot.partyId===partyId&&plan.partySnapshot.roles.length===1&&plan.partySnapshot.roles[0].companySlug==='synthetic'&&plan.partySnapshot.roles[0].role==='vendor','reviewed visible company-specific vendor role');need(isDeepStrictEqual(await exactGet(ownerPage,path),before),'party plan is readonly');need(isDeepStrictEqual(await post(ownerPage,prefix+'documents/party-links/plan',input),reviewed),'repeat pure plan exactsourcehash stable');const applied=await post(ownerPage,prefix+'documents/party-links/apply',{...input,planHash:plan.planHash,confirm:true,idempotencyKey:'synthetic-batch-party-link-'+original.id});need(Number.isSafeInteger(applied.id)&&applied.id>0&&applied.idempotent===false&&applied.planHash===plan.planHash,'real explicitly confirmed party-link receipt');const history=await exactGet(ownerPage,path);need(history.links.length===1&&history.links[0].id===applied.id&&history.links[0].party_id===partyId&&history.links[0].party_role==='vendor'&&history.links[0].event_type==='linked'&&history.links[0].evidence_kind==='exact_identifier'&&history.links[0].plan_hash===plan.planHash,'exact immutable source party-link history');linkedOriginals.push({documentId:original.id,plan:reviewed,applied,history});}await originalProof(ownerPage);need((await exactGet(ownerPage,prefix+'journal')).journal.entries.length===0,'party decisions never book');return{supplier,links:linkedOriginals,reviewMode:'explicit-canonical-http-plan-and-confirmed-apply'};}
function statusPath(){return batch+'/runs/'+persisted.runId;}
function query(){return new URLSearchParams(scope).toString();}
async function workbench(){diagnosticPage=ownerPage;step='web-complete-batch-source-review';await ownerPage.goto(base+'/companies/synthetic/batchbogfoering');await ownerPage.getByRole('heading',{name:'Bogføringskø',exact:true}).waitFor();await ownerPage.getByLabel('Fra dato',{exact:true}).fill(today);await ownerPage.getByLabel('Til dato',{exact:true}).fill(today);await ownerPage.getByRole('button',{name:'Vis arbejdskø',exact:true}).click();const queue=(await exactGet(ownerPage,prefix+'bookkeeping-workbench?'+new URLSearchParams({from:today,to:today}))).workbench;need(queue.state==='available'&&queue.population.total===2&&queue.population.ready===2&&queue.population.blockers===0&&queue.rows.length===2&&queue.page.nextCursor===null,'complete two-item workbench, no truncation/filter bypass');originalPlanResponse=await exactGet(ownerPage,batch+'?'+query());plan=originalPlanResponse.plan;need(originalPlanResponse.dryRun===true&&plan.executionPolicy==='atomic_v1'&&plan.items.length===2&&plan.items.every(i=>i.partition==='ready'&&originals.some(d=>d.id===i.documentId)&&i.ruleApplication?.payloadHash===rule.payloadHash),'two fully source-bound ready items');need(plan.sourceIdentities&&/^[a-f0-9]{64}$/.test(plan.planHash),'full candidate universe/hash');await inspectHash(ownerPage);for(const text of['SYN-BATCH-1','SYN-BATCH-2']){const row=ownerPage.getByRole('region',{name:'Planens regnskabsmæssige oversigt',exact:true}).getByRole('row').filter({hasText:text});await row.waitFor();need((await row.innerText()).includes('Synthetic supplier')&&(await row.innerText()).includes('Konto: 3000')&&(await row.innerText()).includes('Almindeligt momsfradrag'),'readable actual accounting summary');}const before=await reports(ownerPage);need(before.journal.journal.entries.length===0,'readonly planning does not post');const reviewed=ownerPage.getByLabel(reviewLabel,{exact:true});need(await ownerPage.getByRole('button',{name:'Gem eksakt plan',exact:true}).isDisabled(),'unreviewed plan not writable');await reviewed.check();await ownerPage.getByRole('button',{name:'Gem eksakt plan',exact:true}).click();const d=ownerPage.getByRole('dialog',{name:'Gem præcis batchplan',exact:true});await d.waitFor();await expandProof(d);need((await d.innerText()).includes(plan.planHash),'separate exact-plan confirmation');const saved=await responseClick(ownerPage,d.getByRole('button',{name:'Gem gennemgået plan',exact:true}),batch+'/persist');need(saved.body.expectedPlanHash===plan.planHash&&typeof saved.body.runKey==='string'&&saved.body.runKey.length>0,'reviewed CAS hash plus chosen UI run key');step='web-persist-response-source-equality';persisted=saved.value;need(persisted.runId>0&&isDeepStrictEqual(persisted.plan,plan)&&persisted.state.revisions.length===1&&persisted.state.revisions[0].plannerSubjectId===ownerId,'immutable current-human revision');step='web-persist-dialog-closed';await d.waitFor({state:'hidden'});step='web-status-exact-run-key';const byKey=await exactGet(ownerPage,batch+'/runs?'+new URLSearchParams({runKey:saved.body.runKey}));need(isDeepStrictEqual(byKey,await exactGet(ownerPage,statusPath())),'exact runKey recovers same runId without resend');step='web-stale-persist-and-self-review-refusals';const beforeRefusal=await exactGet(ownerPage,statusPath());const stale=await call(ownerPage,batch+'/persist',{...scope,runKey:saved.body.runKey,expectedPlanHash:'0'.repeat(64),confirm:true});need(stale.status===422,'changed reviewed hash persistence refused');need(isDeepStrictEqual(beforeRefusal,await exactGet(ownerPage,statusPath())),'stale persist creates no additional revision');const self=await call(ownerPage,batch+'/approve',{runId:persisted.runId,planHash:plan.planHash,confirm:true});need(self.status===422,'planner cannot approve own batch');need(isDeepStrictEqual(beforeRefusal,await exactGet(ownerPage,statusPath())),'self-review leaves immutable revision unchanged');step='web-recover-run-key';await ownerPage.getByLabel('Kørselsnøgle til status',{exact:true}).fill(saved.body.runKey);await ownerPage.getByRole('button',{name:'Hent kørsel via nøgle',exact:true}).click();await inspectHash(ownerPage);return{workbench:queue,plan,persisted,runKey:saved.body.runKey,statusByKey:byKey,selfReviewRefused:true,staleHashRefused:true};}
async function approveApply(){diagnosticPage=memberPage;step='independent-web-exact-batch-approval';await memberPage.goto(base+`/companies/synthetic/batchbogfoering?runId=${persisted.runId}`);await memberPage.getByRole('heading',{name:'Bogføringskø',exact:true}).waitFor();await inspectHash(memberPage);await memberPage.getByLabel(reviewLabel,{exact:true}).check();await memberPage.getByRole('button',{name:'Godkend',exact:true}).click();const d=memberPage.getByRole('dialog',{name:'Godkend præcis batchplan',exact:true});await d.waitFor();await expandProof(d);need((await d.innerText()).includes(plan.planHash),'reviewer sees exact stored plan');const approvalReceipt=await responseClick(memberPage,d.getByRole('button',{name:'Godkend gennemgået plan',exact:true}),batch+'/approve');need(isDeepStrictEqual(approvalReceipt.body,{runId:persisted.runId,planHash:plan.planHash,confirm:true}),'exact independent approval request');approval=approvalReceipt.value;need(approval.state.revisions.some(r=>r.planHash===plan.planHash&&r.plannerSubjectId===ownerId&&r.approverSubjectId===memberId&&r.approvedAt),'distinct current human approval retained');await d.waitFor({state:'hidden'});diagnosticPage=ownerPage;step='owner-web-reviewed-atomic-apply';await ownerPage.goto(base+`/companies/synthetic/batchbogfoering?runId=${persisted.runId}`);await inspectHash(ownerPage);await ownerPage.getByLabel(reviewLabel,{exact:true}).check();await ownerPage.getByRole('button',{name:'Anvend',exact:true}).click();const applyDialog=ownerPage.getByRole('dialog',{name:'Anvend atomisk batchplan',exact:true});await applyDialog.waitFor();await expandProof(applyDialog);need((await applyDialog.innerText()).includes(plan.planHash),'separate exact atomic application confirmation');applied=(await responseClick(ownerPage,applyDialog.getByRole('button',{name:'Bogfør gennemgået batch',exact:true}),batch+'/apply')).value;await applyDialog.waitFor({state:'hidden'});need(applied.runId===persisted.runId&&applied.planHash===plan.planHash&&applied.results.length===2&&applied.results.every(r=>r.outcome==='applied'),'exact two committed results');for(const name of['audit_chain','trial_balance','reconciliation','vat'])need(applied.checks.some(c=>c.name===name&&c.ok===true&&c.detail!==null&&typeof c.detail==='object'),'real mandatory '+name+' proof');for(const item of plan.items){const result=applied.results.find(r=>r.actionKey===item.actionKey),r=result?.receipt;need(r&&r.evidenceHash===item.evidenceHash&&r.planHash===plan.planHash&&r.documentId===item.documentId&&r.bankTransactionId===item.bankTransactionId,'receipt exact source binding');for(const name of['journalEntryId','vatPreflightId','postingRuleApplicationId','purchaseApplicationId'])need(Number.isSafeInteger(r[name])&&r[name]>0,'real persisted '+name);need(/^[a-f0-9]{64}$/.test(r.journalEntryHash),'real immutable journal hash');}
stable={state:await exactGet(ownerPage,statusPath()),reports:await reports(ownerPage),originals:await originalProof(ownerPage),supplier,partyLinks:linkedOriginals.map(x=>({documentId:x.documentId,history:x.history}))};need(stable.reports.journal.journal.entries.length===2&&stable.state.state.receipts.length===2,'exact durable journal+receipt set');need(stable.reports.trialBalance.trialBalance.totalDebit===375&&stable.reports.trialBalance.trialBalance.totalCredit===375,'exact real trial balance totals');for(const original of originals){const entries=stable.reports.journal.journal.entries.filter(e=>e.documentId===original.id);need(entries.length===1&&entries[0].lines.length===3&&entries[0].total===original.gross,'one balanced native VAT posting per original');}const bank=stable.reports.bank.bank;need(bank.transactions.length===2&&bank.matchedCount===2&&bank.unmatchedCount===0&&bank.actualBalance===-375&&bank.bookedBalance===-375&&bank.difference===0,'actual independent bank and journal reconciliation');need(stable.reports.vat.vat.inputVat===75&&stable.reports.vat.vat.payable===-75&&stable.reports.vat.vat.vatReportErrors.length===0,'canonical exact native VAT readback');await ownerPage.getByRole('list',{name:'Afsluttende kontroller',exact:true}).waitFor();await ownerPage.screenshot({path:process.env.RM_BATCH_SCREENSHOT,fullPage:true});return{approval,applied,stable};}
async function assertStable(page){if(originalPostingRulesProof)return originalPostingRulesProof.assertStable(page);if(originalDimensionsProof)return originalDimensionsProof.assertStable(page);if(originalCommitmentProof)return originalCommitmentProof.assertStable(page);if(historicalVatProof)return historicalVatProof.assertStable(page);if(originalVatPurchaseProof)return originalVatPurchaseProof.assertStable(page);if(originalVatProof)return originalVatProof.assertStable(page);if(originalBudgetProof)return originalBudgetProof.assertStable(page);if(originalMileageProof)return originalMileageProof.assertStable(page);if(originalAssetProof)return originalAssetProof.assertStable(page);if(originalAccrualProof)return originalAccrualProof.assertStable(page);if(originalExpenseProof)return originalExpenseProof.assertStable(page);if(originalPartyProof)return originalPartyProof.assertStable(page);need(isDeepStrictEqual(await exactGet(page,prefix+`workspace-parties/${encodeURIComponent(supplier.party.partyId)}`),stable.supplier),'canonical supplier and explicit roles changed');for(const link of stable.partyLinks)need(isDeepStrictEqual(await exactGet(page,prefix+`documents/${link.documentId}/party-links`),link.history),'immutable party-link history changed');need(isDeepStrictEqual(await reports(page),stable.reports),'full report/journal/UUID changed');need(isDeepStrictEqual(await exactGet(page,statusPath()),stable.state),'immutable run/review/receipts/history changed');await originalProof(page);}
async function replay(){const exact={runId:persisted.runId,planHash:plan.planHash,confirm:true};const repeated=await post(ownerPage,batch+'/apply',exact);need(repeated.duplicate===true&&repeated.results.length===2&&repeated.results.every(r=>r.outcome==='duplicate'),'same reviewed revision replays complete receipt set');for(const r of repeated.results)need(isDeepStrictEqual(r.receipt,applied.results.find(x=>x.actionKey===r.actionKey).receipt),'exact replayed receipt preserved');await assertStable(ownerPage);const wrong=await call(ownerPage,batch+'/apply',{...exact,planHash:'0'.repeat(64)});need(wrong.status===422,'changed hash apply refused');await assertStable(ownerPage);return{sameRevisionReplayed:true,changedHashRefused:true,results:repeated.results};}
async function client(page,kind,name,args={},body){const file=join(work,'client.cookie');writeFileSync(file,(await page.context().cookies(base)).map(c=>`${c.name}=${c.value}`).join('; '),{mode:0o600,flag:'wx'});try{const common=['--server',base,'--cookie-file',file,'--allow-loopback-http'];let argv,input;if(kind==='cli'){const flags=Object.entries(args).flatMap(([key,value])=>['--'+key,String(value)]);argv=['remote',name,...common,'--company','synthetic',...flags,...(body?['--confirm']:[])];input=body?JSON.stringify(body):'';}else{argv=common;input=[{jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'synthetic-batch-proof',version:'1'}}},{jsonrpc:'2.0',method:'notifications/initialized'},{jsonrpc:'2.0',id:2,method:'tools/call',params:{name,arguments:{company:'synthetic',...args,...(body?{body:JSON.stringify(body),confirm:true}:{})}}}].map(v=>JSON.stringify(v)).join('\n')+'\n';}const out=spawnSync(join(work,kind==='cli'?'rentemester-rust':'rentemester-mcp'),argv,{input,encoding:'utf8',timeout:45000,maxBuffer:16*1024*1024});need(!out.error&&out.status===0,kind+' exact native terminal');const v=kind==='cli'?JSON.parse(out.stdout):out.stdout.trim().split('\n').map(l=>JSON.parse(l)).find(v=>v.id===2)?.result?.structuredContent;need(v?.ok===true&&v.data,'full native canonical client result');return v.data;}finally{unlinkSync(file);}}
async function clients(){step='real-cli-mcp-canonical-read-and-explicit-replay';const flags={'accounting-from':scope.accountingFrom,'accounting-to':scope.accountingTo,'bank-from':scope.bankFrom,'bank-to':scope.bankTo};const current=await exactGet(ownerPage,batch+'?'+query());need(isDeepStrictEqual(await client(ownerPage,'cli','bookkeeping-batch-plan',flags),current)&&isDeepStrictEqual(await client(ownerPage,'mcp','bookkeeping_batch_plan',{scope}),current),'full current plan parity after actual bookkeeping');const key=stable.state.state.run.runKey;for(const[kind,name,args]of[['cli','bookkeeping-batch-status',{id:persisted.runId}],['cli','bookkeeping-batch-status',{'run-key':key}],['mcp','bookkeeping_batch_status',{runId:persisted.runId}],['mcp','bookkeeping_batch_status',{runKey:key}]])need(isDeepStrictEqual(await client(ownerPage,kind,name,args),stable.state),'full status ID/key parity');for(const[kind,name]of[['cli','bookkeeping-batch-apply'],['mcp','bookkeeping_batch_apply']]){const r=await client(ownerPage,kind,name,{}, {runId:persisted.runId,planHash:plan.planHash,confirm:true});need(r.duplicate===true&&r.results.every(x=>x.outcome==='duplicate'),'explicit native client replay is duplicate');for(const result of r.results)need(isDeepStrictEqual(result.receipt,applied.results.find(x=>x.actionKey===result.actionKey).receipt),'native client exact receipt projection');}await assertStable(ownerPage);return{httpCliMcpPlanAndStatusParity:true,exactRunKeyRecovery:true,cliMcpReceiptReplay:true};}
async function reopen(){step='normal-two-user-fresh-mfa-after-restart';await ownerPage.context().clearCookies();await memberPage.context().clearCookies();need((await login(ownerPage,owner)).user.id===ownerId,'same owner after restart');need((await login(memberPage,member)).user.id===memberId,'same independent reviewer after restart');await assertStable(ownerPage);await assertStable(memberPage);if(originalPostingRulesProof)await originalPostingRulesProof.reopen(ownerPage);else if(originalDimensionsProof)await originalDimensionsProof.reopen(ownerPage);else if(originalCommitmentProof)await originalCommitmentProof.reopen(ownerPage);else if(historicalVatProof)await historicalVatProof.reopen(ownerPage);else if(originalVatPurchaseProof)await originalVatPurchaseProof.reopen(ownerPage);else if(originalVatProof)await originalVatProof.reopen(ownerPage);else if(originalBudgetProof)await originalBudgetProof.reopen(ownerPage);else if(originalMileageProof)await originalMileageProof.reopen(ownerPage);else if(originalAssetProof)await originalAssetProof.reopen(ownerPage);else if(originalAccrualProof)await originalAccrualProof.reopen(ownerPage);else if(originalExpenseProof)await originalExpenseProof.reopen(ownerPage);else if(!originalPartyProof){await replay();await clients();}return{restartNormalMfa:true,exactHistoricalApprovalReceiptsAndOriginals:true,noNewJournalEffects:true};}
async function archive(){step='explicit-signed-nonempty-batch-snapshot';const r=await ownerPage.request.post(base+'/api/workspace/backup',{headers:{origin:base,'content-type':'application/json'},data:{confirm:true}});if(r.status()===422){const refusal=await r.json();need(refusal.ok===false&&Array.isArray(refusal.errors)&&refusal.errors.some(e=>/nonempty unsupported|unsupported.*(?:batch|schema|inventory)|(?:batch|schema|inventory).*unsupported/i.test(e)),'backup refusal must state concrete unsupported inventory');await assertStable(ownerPage);return{supported:false,status:r.status(),code:refusal.code,errors:refusal.errors,sourceFreeRestoreQualified:false};}need(r.status()===200&&r.headers()['content-type']==='application/vnd.rentemester.native-backup+json','real configured signed snapshot');const bytes=await r.body();need(bytes.length>0&&bytes.length<=96*1024*1024,'bounded full signed bytes');writeFileSync(join(work,'batch.native-backup.json'),bytes,{mode:0o600,flag:'wx'});await assertStable(ownerPage);return{supported:true,sha256:hash(bytes),bytes:bytes.length,sourceFreeRestoreQualified:false};}
async function restored(){step='fresh-restored-owner-normal-verification-mfa';ownerPage=await newPage();const fresh=restoredUser={email:'restored@example.invalid',password:'synthetic-restored-password-789',codes:[]};await ownerPage.goto((await mail('verification',fresh.email)).actionUrl);const me=await enroll(ownerPage,fresh);need(me.user.id!==ownerId&&me.user.id!==memberId,'fresh owner authority, historical actors not imported');await assertStable(ownerPage);return{sourceFreeRestoreQualified:true,freshOwnerMfa:true,ledgerUuid:(originalPostingRulesProof||originalDimensionsProof||originalCommitmentProof||originalPartyProof||originalExpenseProof||originalAccrualProof||originalAssetProof||originalMileageProof||originalBudgetProof||originalVatProof||originalVatPurchaseProof||historicalVatProof)?null:stable.reports.journal.journal.ledgerUuid,historicalBatchAndOriginalsExact:true,noRestoreMutationOrAuthorityImported:true};}
for await(const line of createInterface({input:process.stdin,crlfDelay:Infinity})){try{const action=JSON.parse(line);let value;if(action==='authenticate')value=await authenticate();else if(action==='posting-rules-setup')value=await originalPostingRulesSetup();else if(action==='original-posting-rules')value=await originalPostingRulesProof.run();else if(action==='posting-rules-readonly')value=await originalPostingRulesProof.readOnly();else if(action==='dimensions-readonly')value=await originalDimensionsProof.readOnly();else if(action==='original-dimensions')value=await originalDimensionsRun();else if(action==='commitment-readonly')value=await originalCommitmentProof.readOnly();else if(action==='budget-readonly')value=await originalBudgetProof.readOnly();else if(action==='original-commitment')value=await originalCommitmentRun();else if(action==='historical-vat')value=await historicalVatRun();else if(action==='original-vat-purchase')value=await originalVatPurchaseRun();else if(action==='original-vat')value=await originalVatRun();else if(action==='original-budget')value=await originalBudgetRun();else if(action==='original-mileage')value=await originalMileageRun();else if(action==='original-asset')value=await originalAssetRun();else if(action==='original-accrual')value=await originalAccrualRun();else if(action==='original-expense')value=await originalExpenseRun();else if(action==='expense-hold-begin')value=await originalExpenseProof.holdBegin();else if(action==='expense-revoke')value=await originalExpenseProof.revoke();else if(action==='expense-hold-finish')value=await originalExpenseProof.holdFinish();else if(action==='original-parties')value=await originalPartyRun();else if(action==='intake')value=await intake();else if(action==='party-links')value=await linkParties();else if(action==='rules')value=await rules();else if(action==='workbench')value=await workbench();else if(action==='approve-apply')value=await approveApply();else if(action==='replay')value=await replay();else if(action==='clients')value=await clients();else if(action==='reopen')value=await reopen();else if(action==='archive')value=await archive();else if(action==='restored')value=await restored();else if(action==='restored-reopen'){await ownerPage.context().clearCookies();await login(ownerPage,restoredUser);await assertStable(ownerPage);value={restoredReopenNormalMfa:true};}else if(action==='finish'){await browser?.close();emit({ok:true,value:{finished:true}});break;}else throw Error('unknown phase');emit({ok:true,value});}catch(error){let pageState=null;try{pageState=await Promise.race([diagnosticPage?.evaluate(()=>({pathname:location.pathname,headings:[...document.querySelectorAll('h1,h2')].map(e=>e.textContent.slice(0,100)),buttons:[...document.querySelectorAll('button')].map(e=>e.textContent.slice(0,100)),alerts:document.querySelectorAll('[role=alert]').length})),new Promise(resolve=>setTimeout(()=>resolve(null),2000))]);}catch{}emit({ok:false,step,detail:(process.env.RM_POSTING_RULES_ORIGINAL_HELPER||process.env.RM_DIMENSIONS_ORIGINAL_HELPER||process.env.RM_COMMITMENT_ORIGINAL_HELPER||process.env.RM_HISTORICAL_VAT_HELPER||process.env.RM_VAT_PURCHASE_ORIGINAL_HELPER||process.env.RM_VAT_ORIGINAL_HELPER||process.env.RM_BUDGET_ORIGINAL_HELPER||process.env.RM_MILEAGE_ORIGINAL_HELPER||process.env.RM_EXPENSE_ORIGINAL_HELPER||process.env.RM_ACCRUAL_ORIGINAL_HELPER||process.env.RM_ASSET_ORIGINAL_HELPER)?error?.message:process.env.RM_DOCUMENT_PARTY_ORIGINAL_HELPER?(error?.message?.startsWith('document_party_original:')?error.message:detail):detail,errorType:error?.name,trace,pageState});await Promise.race([browser?.close().catch(()=>{}),new Promise(resolve=>setTimeout(resolve,2000))]);process.exitCode=1;break;}}
'''


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def verify_restored_resolution_inventory(source, restored, manifest_hash, receipt_high_water):
    if source.keys() != restored.keys():
        raise RuntimeError('Restored resolution table inventory differs')
    for table, rows in source.items():
        current = restored[table]
        if table != 'audit_log':
            if current != rows:
                raise RuntimeError('Restored resolution table differs: ' + table)
            continue
        if len(current) != len(rows) + 1 or current[:len(rows)] != rows:
            raise RuntimeError('Restored resolution audit prefix or boundary count differs')
        boundary = dict(current[-1])
        stamp = boundary.pop('created_at', None)
        expected = {'id': max((row['id'] for row in rows), default=0) + 1,
                    'event_type': 'native_restore_receipt_boundary',
                    'entity_type': 'mutation_idempotency_tombstones',
                    'entity_id': str(receipt_high_water),
                    'message': 'Imported receipt high-water mark; archive ' + manifest_hash + '; original keys require manual reconciliation under fresh identities',
                    'actor': 'system:portable-restore'}
        if boundary != expected or not isinstance(stamp, str) or not re.fullmatch(r'\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}', stamp):
            raise RuntimeError('Unexpected restored resolution audit boundary')


def free_port():
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0))
        return sock.getsockname()[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for option in ['server-binary', 'web-dir', 'cli-binary', 'mcp-binary', 'evidence']:
        parser.add_argument('--' + option, required=True, type=Path)
    parser.add_argument('--original-document-parties', action='store_true', help='Isolated original seven document-party CLI/MCP operations; no batch posting (Work-Issue #717)')
    parser.add_argument('--original-expense', action='store_true', help='Bounded source original expense 2CLI/3MCP DK/EU human/service proof (Work-Issue #717)')
    parser.add_argument('--original-accrual', action='store_true', help='Bounded original accrual 3CLI/3MCP, three documented types, exact 8-journal recovery proof (Work-Issue #717)')
    parser.add_argument('--original-asset', action='store_true', help='Bounded original asset 4CLI/4MCP, 5 journals/10 lines, exact signed recovery (Work-Issue #717)')
    parser.add_argument('--original-mileage', action='store_true', help='Bounded original mileage human/service cross-year trips, two local exports and exact signed recovery (Work-Issue #717)')
    parser.add_argument('--original-budget', action='store_true', help='Original budget two revisions, actual comparison, monthly/weekly source planning and signed recovery (Group172)')
    parser.add_argument('--original-posting-rules', action='store_true', help='Group178 source six CLI/three MCP posting-rule operations, exact six-event recovery (Work-Issue #696)')
    parser.add_argument('--original-dimensions', action='store_true', help='Group177 original expense dimensions/replacement and nonempty budget signed recovery (Work-Issue #717)')
    parser.add_argument('--original-commitment', action='store_true', help='Group176 one scanned original/vendor, source approval/match/lifecycle and signed recovery (Work-Issue #717)')
    parser.add_argument('--original-vat', action='store_true', help='Group173 one ordinary scanned DK purchase, source reports, reviewed close, filing aliases and signed recovery (Work-Issue #717)')
    parser.add_argument('--original-vat-purchase', action='store_true', help='Group174 scanned EU/DK/nonEU originals, no-bank EU CLI, bank representation MCP and signed recovery (Work-Issue #717)')
    parser.add_argument('--historical-vat', action='store_true', help='Group175 ordinary Dinero plan/apply, VAT reports, reviewed close/filing and signed recovery (Work-Issue #689)')
    args = parser.parse_args()
    if sum([args.original_expense, args.original_document_parties, args.original_accrual, args.original_asset, args.original_mileage, args.original_budget, args.original_vat, args.original_vat_purchase, args.original_commitment, args.original_dimensions, args.original_posting_rules, args.historical_vat]) > 1:
        parser.error('Select only one bounded original opt-in')
    if (args.original_expense or args.original_accrual or args.original_asset or args.original_mileage or args.original_budget or args.original_vat or args.original_vat_purchase or args.original_commitment or args.original_dimensions or args.original_posting_rules or args.historical_vat) and subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=REPO, text=True).strip()[:12] not in args.evidence.name:
        parser.error('Original bounded evidence filename must contain current HEAD first 12 hex digits')
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
    scanner_token = secrets.token_urlsafe(32)
    scanner_requests = []
    vies_requests = []
    held_received, held_release = threading.Event(), threading.Event()
    backend_port, mail_port = free_port(), free_port()
    if backend_port == mail_port:
        raise RuntimeError('Distinct owned ports required')
    origin = f'http://127.0.0.1:{backend_port}'
    report = {'status': 'running', 'sourceRevision': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=REPO, text=True).strip(), 'runnerSha256': digest(Path(__file__)), 'browserHelperSha256': hashlib.sha256(BROWSER.encode()).hexdigest(), 'canonicalOrigin': origin, 'noOfflineIdentitySeeds': True, 'ordinaryMacOsUser': True, 'ownedProfile': str(work), 'externalMailDelivered': False, 'executionPolicy': 'atomic_v1', 'legacyPartialExecutionParityQualified': False, 'providerTaxParityQualified': False, 'installedNativeQualified': False, 'faultInjectedInActualRunner': False, 'screenshot': str(args.evidence.with_suffix('.png')),  'actions': [], 'results': {}, 'sourceFreeRestoreQualified': False}
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

            def do_GET(self):
                match = re.fullmatch(r'/vies/ms/DE/vat/([0-9]{9})', self.path) if (args.original_expense or args.original_vat_purchase) else None
                if not match:
                    self.send_error(404)
                    return
                vat = match.group(1)
                if args.original_vat_purchase and vat != '168000001':
                    self.send_error(400)
                    return
                if vat not in ('168000001', '987654321', '168000003'):
                    self.send_error(400)
                    return
                with gateway_lock:
                    vies_requests.append({'method': 'GET', 'countryCode': 'DE', 'vatNumber': vat})
                    # Provider trace is durable BEFORE the browser can assert a response.
                    with (work / ('vat174-provider-trace.json' if args.original_vat_purchase else 'expense168-provider-trace.json')).open('w') as trace_file:
                        json.dump(vies_requests, trace_file)
                        trace_file.flush()
                        os.fsync(trace_file.fileno())
                status = 200
                if vat == '168000003':
                    held_received.set()
                    if not held_release.wait(20):
                        status = 503
                elif not args.original_vat_purchase and not (work / ('expense168-recover-' + vat)).exists():
                    status = 503
                payload = json.dumps({'userError': 'MS_UNAVAILABLE'} if status == 503 else {'countryCode': 'DE', 'vatNumber': vat, 'isValid': True, 'userError': 'VALID', 'name': 'Synthetic EU supplier168', 'address': 'Synthetic Supplier Street 168'}).encode()
                self.send_response(status)
                self.send_header('content-type', 'application/json')
                self.send_header('content-length', str(len(payload)))
                self.end_headers()
                try:
                    self.wfile.write(payload)
                except (BrokenPipeError, ConnectionResetError, ssl.SSLError):
                    pass

            def do_POST(self):
                if args.original_expense and self.path == '/extract':
                    length = int(self.headers.get('content-length', '0'))
                    if not 0 < length <= 14 * 1024 * 1024 or self.headers.get('authorization') != 'Bearer ' + scanner_token:
                        self.send_error(403)
                        return
                    request = json.loads(self.rfile.read(length))
                    raw = base64.b64decode(request['pdfBytesBase64'], validate=True)
                    if set(request) != {'sha256', 'pdfBytesBase64'} or hashlib.sha256(raw).hexdigest() != request['sha256'] or b'SYN168-CONFLICT' not in raw:
                        self.send_error(400)
                        return
                    with gateway_lock:
                        (work / 'expense168-extraction-trace.json').write_text(json.dumps([{'sha256': request['sha256'], 'bytes': len(raw)}]))
                    payload = b'{"fields":[]}'
                    self.send_response(200)
                    self.send_header('content-type', 'application/json')
                    self.send_header('content-length', str(len(payload)))
                    self.end_headers()
                    self.wfile.write(payload)
                    return
                try:
                    length = int(self.headers.get('content-length', '0'))
                    if (args.original_document_parties or args.original_expense or args.original_accrual or args.original_asset or args.original_budget or args.original_vat or args.original_vat_purchase or args.original_commitment or args.original_dimensions or args.original_posting_rules) and self.path == '/scan':
                        if not 0 < length <= 35 * 1024 * 1024 or self.headers.get('authorization') != 'Bearer ' + scanner_token:
                            raise ValueError('Private scanner authority/size')
                        request = json.loads(self.rfile.read(length))
                        if set(request) != {'sha256', 'mimeType', 'filename', 'bytesBase64'} or not re.fullmatch((r'synthetic-batch-purchase-1\.txt' if args.original_posting_rules else r'synthetic-dimensions177-source\.txt' if args.original_dimensions else r'synthetic-commitment176-source\.txt' if args.original_commitment else r'synthetic-vat174-(?:eu|dk|noneu)\.txt' if args.original_vat_purchase else r'synthetic-(?:batch-purchase-[12]|expense168-(?:eu-human|eu-service|held)|expense168-conflict)\.(?:txt|pdf)' if args.original_expense else r'synthetic-budget172-(?:actual|payable|commitment)\.txt' if args.original_budget else r'synthetic-vat173-purchase\.txt' if args.original_vat else r'synthetic-(?:batch-purchase-[12]|party166-internal)\.txt'), request['filename']):
                            raise ValueError('Private scanner envelope')
                        raw = base64.b64decode(request['bytesBase64'], validate=True)
                        if request['sha256'] != hashlib.sha256(raw).hexdigest() or request['mimeType'] != ('application/pdf' if request['filename'].endswith('.pdf') else 'text/plain'):
                            raise ValueError('Private scanner exact bytes')
                        scanner_requests.append({'sha256': request['sha256'], 'filename': request['filename']})
                        payload = json.dumps({'ok': True, 'scannerId': 'Synthetic Scanner', 'scannerVersion': '166.1', 'evidenceRef': 'synthetic:' + request['sha256']}).encode()
                        self.send_response(200)
                        self.send_header('content-type', 'application/json')
                        self.send_header('content-length', str(len(payload)))
                        self.end_headers()
                        self.wfile.write(payload)
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
                argv += ['--restore-backup', str(work / 'batch.native-backup.json'), '--trusted-backup-key', str(work / 'trusted.pub'), '--confirm-target', str(root), '--source-unavailable']
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
            if args.original_expense or args.original_vat_purchase or args.original_commitment or args.original_dimensions or args.original_posting_rules or args.historical_vat:
                report['providerRequests'] = list(vies_requests)
                report['scannerRequests'] = list(scanner_requests)
                save()
            if not value.get('ok'):
                report['browserFailure'] = value
                save()
                raise RuntimeError('Browser assertion failed at ' + value.get('step', action))
            report['actions'].append({'action': action, 'status': 'complete'})
            report['results'][action] = value['value']
            save()
            return value['value']

        def bank_original():
            if args.historical_vat:
                return  # Retained import package is the only physical source; no bank CSV.
            if args.original_posting_rules:
                expected = report['results']['posting-rules-setup']['csvSha256']
            elif args.original_vat_purchase:
                expected = report['results']['original-vat-purchase']['csvSha256']
            elif args.original_vat:
                expected = report['results']['original-vat']['csvSha256']
            elif args.original_mileage or args.original_budget or args.original_commitment or args.original_dimensions or args.original_posting_rules:
                return  # No bank intake in the bounded mileage journey.
            else:
                expected = report['results']['intake']['csvSha256']
            root = Path(config['root'])
            path = root / 'synthetic/bank/processed' / (expected + '.txt')
            if not owned() or root.parent != work or any(p.is_symlink() for p in [root, root / 'synthetic', root / 'synthetic/bank', path.parent, path]):
                raise RuntimeError('Exact owned bank namespace required')
            info = path.stat()
            if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid() or digest(path) != expected:
                raise RuntimeError('Actual archived bank bytes differ from ordinary CSV intake')
            report['archivedBankOriginal'] = {'sha256': expected, 'bytes': info.st_size, 'verified': True}
            save()

        def source_pins():
            return {str(p.relative_to(REPO)): digest(p) for folder in ['crates', 'src', 'app/src', 'scripts/rust'] for p in sorted((REPO / folder).rglob('*')) if p.is_file() and p.suffix in ('.rs', '.ts', '.tsx', '.py', '.json', '.toml', '.lock')}

        def verify_pins():
            if args.historical_vat and digest(work / 'check-native-dinero-import.ts') != report['dineroPackagingHelperSha256']:
                raise RuntimeError('Copied Dinero packaging helper changed')
            if source_pins() != report['sourceFiles'] or digest(Path(__file__)) != report['runnerSha256'] or digest(helper_copy) != report['originalHelperSha256']:
                raise RuntimeError('Source/helper pins changed during actual acceptance')
            if {name: digest(path) for name, path in pinned.items()} != report['artifacts'] or {str(p.relative_to(web)): digest(p) for p in sorted(web.rglob('*')) if p.is_file()} != report['webFiles']:
                raise RuntimeError('Supplied binaries/web changed during actual acceptance')
            report['sourceAndArtifactPinsStable'] = True

        def inspect_resolution_inventory(path, setup=False):
            if path.is_symlink() or not path.is_file() or not 0 < path.stat().st_size <= 32 * 1024 * 1024:
                raise RuntimeError('Bounded regular resolution ledger required')
            db = sqlite3.connect(path.resolve(strict=True).as_uri() + '?mode=ro', uri=True)
            try:
                db.execute('PRAGMA query_only=ON')
                rows = {}
                tables = ['documents', 'document_party_link_events', 'document_party_resolution_events', 'internal_voucher_evidence', 'audit_log', 'journal_entries', 'journal_lines']
                if args.original_vat or args.historical_vat:
                    tables += ['accounting_periods', 'period_close_decisions', 'period_close_reviews', 'period_close_readiness_packets', 'vat_validation_events', 'exceptions', 'bank_transactions', 'bank_journal_reconciliations']
                if args.original_posting_rules:
                    tables += ['posting_rule_versions', 'posting_rule_lifecycle_events', 'posting_rule_applications', 'purchase_posting_applications', 'exceptions', 'payables', 'payable_payments', 'bank_transactions', 'bank_journal_reconciliations']
                if args.original_dimensions:
                    tables += ['budget_lines', 'accounting_dimension_definition_events', 'accounting_dimension_member_events', 'accounting_dimension_assignment_events', 'accounting_dimension_budget_events', 'payables', 'payable_payments', 'bank_transactions', 'bank_journal_reconciliations']
                if args.original_commitment:
                    tables += ['vendors', 'supplier_commitment_events', 'supplier_commitment_occurrence_matches', 'payables', 'payable_payments', 'bank_transactions', 'bank_journal_reconciliations']
                if args.original_budget:
                    tables += ['budget_lines', 'payables', 'payable_payments', 'supplier_commitment_events', 'supplier_commitment_occurrence_matches', 'recurring_invoice_templates', 'recurring_invoice_generations', 'recurring_invoice_generation_claims', 'recurring_invoice_delivery_events', 'bank_transactions', 'bank_journal_reconciliations', 'issued_invoice_postings']
                if args.original_mileage:
                    tables += ['mileage_entries']
                if args.original_asset:
                    tables += ['assets', 'asset_depreciation_entries', 'asset_writeoffs', 'exceptions']
                if args.original_accrual:
                    tables += ['accruals', 'accrual_schedule_postings']
                if args.original_vat_purchase:
                    tables += ['vat_validation_events', 'vies_validations', 'exceptions', 'bank_transactions', 'bank_journal_reconciliations']
                if args.original_expense:
                    tables += ['vat_validation_events', 'vies_validations', 'exceptions', 'bank_transactions', 'bank_journal_reconciliations', 'invoice_extraction_documents', 'invoice_extraction_attempts', 'invoice_extraction_results', 'invoice_extraction_fields', 'invoice_extraction_resolutions']
                if args.historical_vat:
                    tables += ['dinero_import_sources', 'dinero_import_inventories', 'dinero_import_inventory_entries', 'dinero_import_attempts', 'imported_receivable_headers', 'imported_receivable_events']
                for table in tables:
                    cursor = db.execute('SELECT * FROM ' + table + (' ORDER BY document_id' if table == 'internal_voucher_evidence' else ' ORDER BY country_code,vat_number' if table == 'vies_validations' else ' ORDER BY bank_transaction_id,journal_entry_id' if table == 'bank_journal_reconciliations' else ' ORDER BY invoice_document_id' if table == 'issued_invoice_postings' else ' ORDER BY id'))
                    rows[table] = [dict(zip([c[0] for c in cursor.description], r)) for r in cursor.fetchall()]
                    if args.original_expense:
                        for row in rows[table]:
                            for field, value in list(row.items()):
                                if isinstance(value, bytes):
                                    row[field] = {'bytesBase64': base64.b64encode(value).decode(), 'sha256': hashlib.sha256(value).hexdigest()}
                if args.original_posting_rules:
                    expected_setup = {'documents': 1, 'bank_transactions': 1, 'exceptions': 1}
                    empty = ['journal_entries', 'journal_lines', 'posting_rule_applications', 'purchase_posting_applications', 'payables', 'payable_payments', 'bank_journal_reconciliations', 'document_party_link_events', 'document_party_resolution_events', 'internal_voucher_evidence']
                    if any(len(rows[t]) != n for t, n in expected_setup.items()) or any(rows[t] for t in empty):
                        raise RuntimeError('Posting rules exact source and zero financial effects required')
                    setup_events = ['company_profile_update'] + ['account_created'] * 3 + ['account_role_confirmed'] * 2 + ['bank_account_add', 'document_ingest', 'bank_import', 'exception_record']
                    audits = rows['audit_log']
                    if [r['event_type'] for r in audits[:10]] != setup_events:
                        raise RuntimeError('Posting rules fixed ten setup audit events differ')
                    if setup:
                        if len(audits) != 10 or rows['posting_rule_versions'] or rows['posting_rule_lifecycle_events']:
                            raise RuntimeError('Posting rules baseline must be AFTER setup and BEFORE proposals')
                        return rows
                    proof = report['results']['original-posting-rules']
                    versions, events = rows['posting_rule_versions'], rows['posting_rule_lifecycle_events']
                    if len(versions) != 2 or [v['version'] for v in versions] != [1, 2] or len(events) != 6:
                        raise RuntimeError('Exactly two versions and six lifecycle events required')
                    if [e['state'] for e in events] != ['proposed', 'approved', 'disabled', 'proposed', 'approved', 'superseded']:
                        raise RuntimeError('Posting rules frozen terminal lifecycle differs')
                    actors = [proof['serviceActor'], proof['reviewerActor'], proof['humanActor']] * 2
                    if [e['actor'] for e in events] != actors or any(v['created_by'] != proof['serviceActor'] for v in versions):
                        raise RuntimeError('Posting rules current typed actors differ')
                    baseline = report['postingRulesSetupInventory']
                    for table in rows:
                        if table not in ('posting_rule_versions', 'posting_rule_lifecycle_events', 'audit_log') and rows[table] != baseline[table]:
                            raise RuntimeError('Posting rules mutated setup table: ' + table)
                    if audits[:10] != baseline['audit_log'] or len(audits) not in (16, 17) or [r['actor'] for r in audits[10:16]] != actors or any(r['entity_type'] != 'posting_rule' for r in audits[10:16]):
                        raise RuntimeError('Posting rules exact six audit delta required')
                    if len(audits) == 17 and audits[-1]['event_type'] != 'native_restore_receipt_boundary':
                        raise RuntimeError('Only restore boundary may follow six rule audits')
                    for v, receipt in zip(versions, proof['versions']):
                        payload = {'ruleId': v['rule_id'], 'companyId': v['company_id'], 'effectiveFrom': v['effective_from'], 'effectiveTo': v['effective_to'], 'conditions': json.loads(v['conditions_json']), 'outcome': json.loads(v['outcome_json']), 'provenance': v['provenance'], 'rationale': v['rationale']}
                        actual_hash = hashlib.sha256(json.dumps(payload, sort_keys=True, separators=(',', ':'), ensure_ascii=False).encode()).hexdigest()
                        if actual_hash != v['payload_hash'] or actual_hash != receipt['payloadHash']:
                            raise RuntimeError('Independent canonical source payload hash differs')
                    return rows
                if args.historical_vat:
                    expected = report['independentExpectedFacts']['inventory']
                    if any(len(rows[t]) != n for t, n in expected.items()):
                        raise RuntimeError('Frozen historical VAT inventory differs')
                    accounts = dict(db.execute('SELECT id,account_no FROM accounts'))
                    lines = [[accounts[r['account_id']], r['debit_amount'], r['credit_amount'], r['vat_code']] for r in rows['journal_lines']]
                    if lines != [['1500', 10, 0, None], ['60010', 0, 10, None], ['2000', 100, 0, 'DK_PURCHASE_25'], ['64060', 25, 0, None], ['61000', 0, 125, None]]:
                        raise RuntimeError('Frozen historical VAT opening/purchase amounts differ')
                    return rows
                if args.original_vat_purchase:
                    expected = report['independentExpectedFacts']
                    for table, key in [('documents', 'documents'), ('journal_entries', 'journalEntries'), ('journal_lines', 'journalLines'), ('bank_transactions', 'bankTransactions'), ('bank_journal_reconciliations', 'bankReconciliations'), ('exceptions', 'resolvedExceptions'), ('vies_validations', 'viesValidations')]:
                        if len(rows[table]) != expected[key]:
                            raise RuntimeError('Original VAT purchase independent inventory differs: ' + table)
                    proof = report['results']['original-vat-purchase']
                    entries = rows['journal_entries']
                    if [r['created_by'] for r in entries] != [proof['humanActor'], proof['serviceActor']] or entries[0]['source_bank_transaction_id'] is not None or entries[1]['source_bank_transaction_id'] != rows['bank_transactions'][0]['id']:
                        raise RuntimeError('Exact typed actors / genuine no-bank and linked bank provenance required')
                    for entry, receipt in zip(entries, proof['receipts']):
                        if entry['id'] != receipt['entryId'] or entry['entry_no'] != receipt['entryNo'] or entry['entry_hash'] != receipt['entryHash'] or not entry['created_by_program']:
                            raise RuntimeError('Source immutable receipt/provenance differs')
                    account_numbers = dict(db.execute('SELECT id,account_no FROM accounts'))
                    actual_lines = [[account_numbers[r['account_id']], r['debit_amount'], r['credit_amount'], r['vat_code']] for r in rows['journal_lines']]
                    expected_lines = [['3000', 100, 0, 'EU_SERVICE_REVERSE_CHARGE'], ['4700', 25, 0, None], ['1800', 0, 100, None], ['4710', 0, 25, None], ['3100', 100, 0, 'REPRESENTATION_SPECIAL'], ['3100', 18.75, 0, 'REPRESENTATION_NON_DEDUCTIBLE_VAT'], ['4700', 6.25, 0, None], ['1800', 0, 125, None]]
                    if actual_lines != expected_lines:
                        raise RuntimeError('Independent exact VAT lines / canonical writer order differ')
                    exception = rows['exceptions'][0]
                    if exception['type'] != 'UNMATCHED_BANK_TRANSACTION' or exception['status'] != 'resolved' or exception['related_bank_transaction_id'] != rows['bank_transactions'][0]['id'] or exception['resolution_note'] != 'Resolved automatically by posted journal entry ' + entries[1]['entry_no']:
                        raise RuntimeError('Resolved bank exception history must be preserved')
                    return rows
                if args.original_vat:
                    expected = {'documents': 1, 'journal_entries': 1, 'journal_lines': 3, 'bank_transactions': 1, 'bank_journal_reconciliations': 1, 'accounting_periods': 1, 'period_close_decisions': 1, 'period_close_reviews': 1, 'exceptions': 1}
                    if any(len(rows[t]) != n for t, n in expected.items()):
                        raise RuntimeError('Exact original VAT source/journal/reconciliation/review/close inventory differs')
                    exception = rows['exceptions'][0]
                    journal = rows['journal_entries'][0]
                    bank = rows['bank_transactions'][0]
                    document = rows['documents'][0]
                    evidence = json.loads(exception['source_evidence'])
                    if (exception['type'] != 'UNMATCHED_BANK_TRANSACTION' or exception['status'] != 'resolved'
                            or exception['related_bank_transaction_id'] != bank['id']
                            or exception['related_document_id'] != document['id']
                            or evidence['bankTransactionId'] != bank['id'] or evidence['candidateBilagId'] != document['id']
                            or not exception['resolved_at'] or not exception['resolved_by']
                            or exception['resolution_note'] != 'Resolved automatically by posted journal entry ' + journal['entry_no']):
                        raise RuntimeError('Original VAT resolved bank task must retain exact source links and posting resolution')
                    return rows
                if args.original_dimensions:
                    expected = {'documents': 1, 'journal_entries': 1, 'journal_lines': 2, 'budget_lines': 1, 'accounting_dimension_definition_events': 2, 'accounting_dimension_member_events': 4, 'accounting_dimension_assignment_events': 5, 'accounting_dimension_budget_events': 6}
                    if any(len(rows[t]) != n for t, n in expected.items()) or any(rows[t] for t in ['payables', 'payable_payments', 'bank_transactions', 'bank_journal_reconciliations', 'document_party_link_events', 'document_party_resolution_events', 'internal_voucher_evidence']):
                        raise RuntimeError('Frozen nonempty dimensions company inventory differs')
                    proof = report['results']['original-dimensions']
                    assignments = rows['accounting_dimension_assignment_events']
                    budgets = rows['accounting_dimension_budget_events']
                    identities = [proof['serviceActor']] + [proof['humanActor']] * 4
                    if [r['actor'] for r in assignments] != identities or [r['principal'] for r in assignments] != identities or [r['event_type'] for r in assignments] != ['assigned', 'superseded', 'assigned', 'superseded', 'assigned']:
                        raise RuntimeError('Exact five typed assignment events required')
                    if [r['source'] for r in assignments] != ['reviewed'] * 4 + ['imported'] or assignments[-1]['source_ref'] != proof['externalAllocationSourceRef'] or not proof['externalAllocationScheduleRetained'] or not proof['explicitImportReview']:
                        raise RuntimeError('Final assignment must reference the actual retained external schedule and explicit review')
                    if [r['event_type'] for r in budgets] != ['allocated', 'allocated', 'superseded', 'superseded', 'allocated', 'allocated'] or [r['amount'] for r in budgets] != [12, 8, 12, 8, 13, 7] or any(r['actor'] != proof['humanActor'] or r['principal'] != proof['humanActor'] for r in budgets):
                        raise RuntimeError('Exact six human-reviewed budget events required')
                    current = db.execute('SELECT member_id,amount FROM current_accounting_dimension_budgets ORDER BY member_id').fetchall()
                    if current != [('alpha', 13), ('beta', 7)] or db.execute('SELECT COUNT(*) FROM current_accounting_dimension_assignments').fetchone()[0] != 1:
                        raise RuntimeError('Current assignment and complete budget replacement set differ')
                    accounts = dict(db.execute('SELECT id,account_no FROM accounts'))
                    if [[accounts[r['account_id']], r['debit_amount'], r['credit_amount'], r['vat_code']] for r in rows['journal_lines']] != [['3729', 12.5, 0, None], ['8729', 0, 12.5, None]] or rows['budget_lines'][0]['amount'] != 20:
                        raise RuntimeError('Frozen original no-VAT expense and account budget differ')
                    return rows
                if args.original_commitment:
                    if len(rows['vendors']) != 1 or len(rows['documents']) != 1 or len(rows['supplier_commitment_events']) != 4 or len(rows['supplier_commitment_occurrence_matches']) != 1 or any(rows[t] for t in ['journal_entries', 'journal_lines', 'payables', 'payable_payments', 'bank_transactions', 'bank_journal_reconciliations', 'document_party_link_events', 'document_party_resolution_events', 'internal_voucher_evidence']):
                        raise RuntimeError('Frozen commitment original/history/match and zero financial inventory differs')
                    proof = report['results']['original-commitment']
                    events = rows['supplier_commitment_events']
                    match = rows['supplier_commitment_occurrence_matches'][0]
                    if [r['event_type'] for r in events] != ['approved', 'paused', 'approved', 'ended'] or [r['actor'] for r in events] != [proof['humanActor'], proof['serviceActor'], proof['humanActor'], proof['humanActor']] or match['actor'] != proof['serviceActor']:
                        raise RuntimeError('Exact commitment typed human/service mutation provenance differs')
                    if [r['principal'] for r in events] != [proof['humanPrincipal'], proof['servicePrincipal'], proof['humanPrincipal'], proof['humanPrincipal']] or match['principal'] != proof['servicePrincipal']:
                        raise RuntimeError('Exact commitment principal provenance differs')
                    if events[0]['payload_hash'] != proof['approvalHash'] or match['payload_hash'] != proof['matchHash']:
                        raise RuntimeError('Immutable source commitment receipts differ')
                    return rows
                if args.original_budget:
                    if len(rows['documents']) != 5 or len(rows['budget_lines']) != 2 or len(rows['journal_entries']) != 3 or len(rows['journal_lines']) != 6 or len(rows['payables']) != 1 or len(rows['supplier_commitment_events']) != 1 or len(rows['recurring_invoice_templates']) != 1 or rows['recurring_invoice_generations'] or rows['supplier_commitment_occurrence_matches'] or any(rows[t] for t in ['payable_payments', 'recurring_invoice_generation_claims', 'recurring_invoice_delivery_events', 'bank_transactions', 'bank_journal_reconciliations', 'issued_invoice_postings']):
                        raise RuntimeError('Exact budget fixture financial/generation/revision inventory differs')
                    proof = report['results']['original-budget']
                    budgets = rows['budget_lines']
                    if [r['id'] for r in budgets] != [proof['firstId'], proof['lastId']] or [r['amount'] for r in budgets] != [80, 90]:
                        raise RuntimeError('Two exact budget revisions required')
                    audits = [r for r in rows['audit_log'] if r['event_type'] == 'budget_set']
                    if len(audits) != 2 or [r['actor'] for r in audits] != [proof['humanActor'], proof['serviceActor']]:
                        raise RuntimeError('Exactly two typed human/service budget audits required')
                    return rows
                if args.original_mileage:
                    if len(rows['mileage_entries']) != 3 or any(rows[t] for t in tables if t not in ('mileage_entries', 'audit_log')):
                        raise RuntimeError('Exact three mileage entries, no intake/resolution/journals required')
                    trips = rows['mileage_entries']
                    if [r['entry_no'] for r in trips] != ['MIL-2025-000001', 'MIL-2026-000001', 'MIL-2026-000002'] or [r['amount_basis'] for r in trips] != [46.82, 22.81, 2]:
                        raise RuntimeError('Exact cross-year mileage numbers/amounts differ')
                    audits = rows['audit_log']
                    if [r['event_type'] for r in audits[:5]] != ['mileage_entry_create'] * 3 + ['mileage_log_export'] * 2 or len(audits) not in (5, 6):
                        raise RuntimeError('Exactly three log and two prepared export audits required')
                    proof = report['results']['original-mileage']
                    if [r['actor'] for r in audits[:5]] != [proof['humanActor'], proof['serviceActor'], proof['humanActor'], proof['humanActor'], proof['serviceActor']]:
                        raise RuntimeError('Independent typed human/service mileage audit attribution differs')
                    if any('prepared' not in r['message'].lower() for r in audits[3:5]):
                        raise RuntimeError('Export audit must attest preparation, not local publication')
                    return rows
                if args.original_asset:
                    expected = {'documents': 2, 'assets': 1, 'asset_depreciation_entries': 3, 'asset_writeoffs': 1, 'exceptions': 4, 'journal_entries': 5, 'journal_lines': 10}
                    if any(len(rows[table]) != count for table, count in expected.items()):
                        raise RuntimeError('Exact asset/source/depreciation/writeoff/exception/journal inventory differs')
                    return rows
                if args.original_accrual:
                    if len(rows['accruals']) != 3 or len(rows['accrual_schedule_postings']) != 5 or len(rows['journal_entries']) != 8 or len(rows['journal_lines']) != 16:
                        raise RuntimeError('Exact accrual header/posting/journal/line inventory differs')
                    return rows
                if args.original_expense:
                    if len(rows['journal_entries']) != 4 or len(rows['bank_journal_reconciliations']) != 4 or len(rows['vat_validation_events']) != 14:
                        raise RuntimeError('Exact expense journal/reconciliation/evidence inventory differs')
                    return rows
                if rows['journal_entries'] or rows['journal_lines'] or len(rows['document_party_link_events']) != 3 or len(rows['document_party_resolution_events']) != 3 or len(rows['internal_voucher_evidence']) != 1:
                    raise RuntimeError('Expected both nonempty resolution histories and zero journals')
                docs = {r['id']: r for r in rows['documents']}
                for row in rows['document_party_link_events'] + rows['document_party_resolution_events']:
                    doc = docs[row['document_id']]
                    payload = json.dumps(json.loads(doc['payload_json']), sort_keys=True, separators=(',', ':'), ensure_ascii=False)
                    if row['document_sha256'] != doc['sha256_hash'] or row['document_payload_sha256'] != hashlib.sha256(payload.encode()).hexdigest():
                        raise RuntimeError('Independent original/canonical payload binding differs')
                    if not row['actor'] or not row['principal']:
                        raise RuntimeError('Missing historical attribution')
                canonical_hash = lambda value: hashlib.sha256(json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=False).encode()).hexdigest()
                for row in rows['document_party_link_events']:
                    binding = {'documentId': row['document_id'], 'documentSha256': row['document_sha256'], 'documentPayloadSha256': row['document_payload_sha256'], 'partyId': row['party_id'], 'partySnapshot': json.loads(row['party_snapshot_json']), 'role': row['party_role'], 'evidence': json.loads(row['evidence_json'])}
                    if canonical_hash(binding) != row['plan_hash']:
                        raise RuntimeError('Independent plan hash differs')
                for row in rows['document_party_resolution_events']:
                    if row['state'] == 'internal_no_external_party':
                        binding = {'documentId': row['document_id'], 'documentSha256': row['document_sha256'], 'documentPayloadSha256': row['document_payload_sha256'], 'state': row['state'], 'reason': row['reason']}
                        if canonical_hash(binding) != row['decision_hash']:
                            raise RuntimeError('Independent internal decision hash differs')
                return rows
            finally:
                db.close()

        def budget_control_inventory():
            # Immutable business history only; live credentials/usage legitimately change.
            path = Path(config['root']) / '.rentemester/workspace-control.sqlite'
            if path.is_symlink() or not path.is_file():
                raise RuntimeError('Exact owned control database required')
            with sqlite3.connect(path.resolve(strict=True).as_uri() + '?mode=ro', uri=True) as db:
                db.execute('PRAGMA query_only=ON')
                result = {}
                for table, order in [('rm_intercompany_dispositions', 'disposition_id'), ('rm_intercompany_disposition_events', 'id'), ('rm_intercompany_disposition_lifecycle_events', 'id'), ('rm_intercompany_disposition_journal_links', 'id'), ('rm_corporate_record_events', 'id'), ('rm_corporate_record_bytes', 'record_id'), ('rm_corporate_record_links', 'record_id,link_type,link_id')]:
                    cursor = db.execute('SELECT * FROM ' + table + ' ORDER BY ' + order)
                    result[table] = [{c[0]: ({'bytesBase64': base64.b64encode(v).decode(), 'sha256': hashlib.sha256(v).hexdigest()} if isinstance(v, bytes) else v) for c, v in zip(cursor.description, row)} for row in cursor.fetchall()]
                if args.original_vat or args.original_vat_purchase or args.original_commitment or args.original_dimensions or args.original_posting_rules or args.historical_vat:
                    if any(result.values()):
                        raise RuntimeError('VAT report fixture must not create control business effects')
                    return result
                if len(result['rm_intercompany_dispositions']) != 1 or len(result['rm_intercompany_disposition_events']) != 2 or result['rm_intercompany_disposition_lifecycle_events'] or result['rm_intercompany_disposition_journal_links']:
                    raise RuntimeError('One proposed/approved unposted intercompany scenario required')
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
            if args.original_document_parties or args.original_expense or args.original_accrual or args.original_asset or args.original_mileage or args.original_budget or args.original_vat or args.original_vat_purchase or args.original_commitment or args.original_dimensions or args.original_posting_rules or args.historical_vat:
                helper = REPO / ('scripts/rust/check-native-posting-rules-original.ts' if args.original_posting_rules else 'scripts/rust/check-native-dimensions-original.ts' if args.original_dimensions else 'scripts/rust/check-native-supplier-commitment-original.ts' if args.original_commitment else 'scripts/rust/check-native-historical-vat.ts' if args.historical_vat else 'scripts/rust/check-native-vat-purchase-original.ts' if args.original_vat_purchase else 'scripts/rust/check-native-vat-original.ts' if args.original_vat else 'scripts/rust/check-native-budget-original.ts' if args.original_budget else 'scripts/rust/check-native-mileage-original.ts' if args.original_mileage else 'scripts/rust/check-native-asset-original.ts' if args.original_asset else 'scripts/rust/check-native-accrual-original.ts' if args.original_accrual else 'scripts/rust/check-native-expense-original.ts' if args.original_expense else 'scripts/rust/check-native-document-party-original.ts')
                helper_copy = work / helper.name
                shutil.copyfile(helper, helper_copy)
                report['originalHelperSha256'] = digest(helper_copy)
                if args.historical_vat:
                    dependency = REPO / 'scripts/rust/check-native-dinero-import.ts'
                    shutil.copyfile(dependency, work / dependency.name)
                    report['dineroPackagingHelperSha256'] = digest(work / dependency.name)
                if args.original_document_parties:
                    report['originalPartyHelperSha256'] = digest(helper_copy)
                report['sourceFiles'] = source_pins()
                report['workflow'] = 'original_posting_rules178' if args.original_posting_rules else 'original_dimensions177' if args.original_dimensions else 'original_commitment176' if args.original_commitment else 'historical_vat175' if args.historical_vat else 'original_vat_purchase174' if args.original_vat_purchase else 'original_vat173' if args.original_vat else 'original_budget172' if args.original_budget else 'original_mileage171' if args.original_mileage else 'original_asset170' if args.original_asset else 'original_accrual169' if args.original_accrual else 'original_expense168' if args.original_expense else 'original_document_parties_no_batch_financial_posting'
                report['reviewedLegacyActualPositiveQualified'] = False
            report['webFiles'] = {str(p.relative_to(web)): digest(p) for p in sorted(web.rglob('*')) if p.is_file()}
            if args.original_posting_rules:
                report['independentExpectedFacts'] = {'documents': 1, 'gross': 125, 'vat': 25, 'bankTransactions': 1, 'versions': 2, 'events': 6, 'ruleAuditDelta': 6, 'setupAudits': 10, 'setupExceptions': 1, 'exceptionDelta': 0, 'journalEntries': 0, 'journalLines': 0, 'applications': 0, 'payables': 0, 'payments': 0, 'scannerRequests': 1, 'providerRequests': 0}
                save()
            if args.original_dimensions:
                report['independentExpectedFacts'] = {'documents': 1, 'journalEntries': 1, 'journalLines': 2, 'definitionEvents': 2, 'memberEvents': 4, 'assignmentEvents': 5, 'budgetEvents': 6, 'currentBudgets': 2, 'actual': [5, 7.5], 'budget': [13, 7], 'variance': [8, -0.5], 'scannerRequests': 1, 'providerRequests': 0}
                save()
            if args.original_commitment:
                report['independentExpectedFacts'] = {'amount': 10.01, 'canonicalAmount': 12.34, 'variance': 2.33, 'documents': 1, 'commitmentEvents': 4, 'matches': 1, 'scannerRequests': 1, 'providerRequests': 0, 'journalEntries': 0, 'journalLines': 0, 'payables': 0, 'payments': 0, 'bankTransactions': 0}
                save()
            if args.historical_vat:
                report['independentExpectedFacts'] = {'gross': 125, 'purchaseBase25': 100, 'inputVat': 25, 'outputVat': 0, 'netVatPayable': -25, 'scannerRequests': 0, 'providerRequests': 0, 'inventory': {'documents': 0, 'journal_entries': 2, 'journal_lines': 5, 'bank_transactions': 0, 'bank_journal_reconciliations': 0, 'accounting_periods': 1, 'period_close_decisions': 1, 'period_close_reviews': 1, 'exceptions': 0, 'dinero_import_sources': 1, 'dinero_import_inventories': 1, 'dinero_import_inventory_entries': 4, 'dinero_import_attempts': 1, 'imported_receivable_headers': 0, 'imported_receivable_events': 0}}
                save()
            if args.original_vat_purchase:
                report['independentExpectedFacts'] = {'documents': 3, 'journalEntries': 2, 'journalLines': 8, 'bankTransactions': 1, 'bankReconciliations': 1, 'resolvedExceptions': 1, 'scannerRequests': 3, 'providerRequests': 1, 'viesValidations': 1, 'euNoBank': True, 'euExpense': 100, 'euInputVat': 25, 'euReverseChargeVat': 25, 'representationExpense': 118.75, 'representationInputVat': 6.25, 'representationPayment': 125}
                save()
            if args.original_vat:
                report['independentExpectedFacts'] = {'gross': 125, 'purchaseBase25': 100, 'inputVat': 25, 'outputVat': 0, 'netVatPayable': -25, 'journalEntries': 1, 'journalLines': 3, 'scannerRequests': 1, 'providerRequests': 0, 'resolvedExceptions': 1, 'euTotalValue': 0, 'ossConsumerSalesBase': 0}
                report['reusedUnchangedNonemptyEvidence'] = ['scripts/rust/run-eu-sales.py: EUR740, dated partial592/final0 credits, FX gain3', 'scripts/rust/run-oss.py: manual OSS base and dated reversal']
                report['newNonemptyActual'] = 'ordinary DK purchase; EU/OSS projections empty, accepted nonempty journeys reused, not rerun'
                save()
            progress('operator:private-auth-gateway-trust-and-backup-signer')
            ca, cert, private = work / 'ca.pem', work / 'tls.pem', work / 'tls.key'
            (work / 'tls.ext').write_text('basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\nsubjectAltName=DNS:localhost,IP:127.0.0.1\n')
            commands = [
                ['openssl', 'req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', str(work / 'ca.key'), '-out', str(ca), '-days', '1', '-subj', '/CN=Synthetic Batch Auth CA', '-addext', 'basicConstraints=critical,CA:TRUE', '-addext', 'keyUsage=critical,keyCertSign,cRLSign'],
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
            config = {'runtimeProfile': 'local-user-macos', 'root': str(work / 'workspace'), 'brokerUid': os.getuid(), 'workspace': 'synthetic-native-batch', 'companySlug': 'synthetic', 'companyName': 'Synthetic Batch Company', 'cookieKey': key(), 'mfaKey': key(), 'operatorActors': ['system:synthetic-installer'], 'accounts': [], 'firstOwner': {'actor': 'system:synthetic-installer', 'email': 'owner@example.invalid', 'name': 'Synthetic Owner', 'password': 'synthetic-owner-password-123'}, 'backupSigningKeyFile': str(work / 'signing.der'), 'authMail': {'endpoint': f'https://127.0.0.1:{mail_port}/mail', 'bearerToken': bearer, 'from': 'auth@example.invalid', 'idempotencySecret': secrets.token_urlsafe(32), 'trustedCaFile': str(ca)}}
            if args.original_expense:
                config['invoiceExtractor'] = {'provider': 'http-json-v1', 'url': f'https://127.0.0.1:{mail_port}/extract', 'bearerToken': scanner_token, 'timeoutMs': 10000, 'trustedCaFile': str(ca)}
                config['vies'] = {'baseUrl': f'https://127.0.0.1:{mail_port}/vies', 'trustedCaPem': ca.read_text()}
            if args.original_document_parties or args.original_expense or args.original_accrual or args.original_asset or args.original_budget or args.original_vat or args.original_vat_purchase or args.original_commitment or args.original_dimensions or args.original_posting_rules:
                config['documentScanner'] = {'policy': 'required', 'provider': 'http-json-v1', 'url': f'https://127.0.0.1:{mail_port}/scan', 'bearerToken': scanner_token, 'timeoutMs': 10000, 'trustedCaFile': str(ca)}
            if args.original_vat_purchase:
                config['vies'] = {'baseUrl': f'https://127.0.0.1:{mail_port}/vies', 'trustedCaPem': ca.read_text()}
            write_config()
            start(create=True)
            script = work / 'browser.mjs'
            script.write_text(BROWSER)
            controller = subprocess.Popen(['/opt/homebrew/bin/bun', str(script)], cwd=REPO, env={**os.environ, 'RM_BATCH_BASE': origin, **({'RM_POSTING_RULES_ORIGINAL_HELPER': str(helper_copy)} if args.original_posting_rules else {}), **({'RM_DIMENSIONS_ORIGINAL_HELPER': str(helper_copy)} if args.original_dimensions else {}), **({'RM_COMMITMENT_ORIGINAL_HELPER': str(helper_copy)} if args.original_commitment else {}), **({'RM_HISTORICAL_VAT_HELPER': str(helper_copy)} if args.historical_vat else {}), **({'RM_VAT_PURCHASE_ORIGINAL_HELPER': str(helper_copy)} if args.original_vat_purchase else {}), **({'RM_VAT_ORIGINAL_HELPER': str(helper_copy)} if args.original_vat else {}), **({'RM_BUDGET_ORIGINAL_HELPER': str(helper_copy)} if args.original_budget else {}), **({'RM_MILEAGE_ORIGINAL_HELPER': str(helper_copy)} if args.original_mileage else {}), **({'RM_ASSET_ORIGINAL_HELPER': str(helper_copy)} if args.original_asset else {}), **({'RM_ACCRUAL_ORIGINAL_HELPER': str(helper_copy)} if args.original_accrual else {}), **({'RM_EXPENSE_ORIGINAL_HELPER': str(helper_copy)} if args.original_expense else {}), **({'RM_DOCUMENT_PARTY_ORIGINAL_HELPER': str(helper_copy)} if args.original_document_parties else {}), 'RM_BATCH_WORK': str(work), 'RM_BATCH_SCREENSHOT': str(args.evidence.with_suffix('.png')), 'RM_BATCH_PLAYWRIGHT': str(((REPO if (REPO / 'node_modules/@playwright/test/index.mjs').is_file() else args.web_dir.resolve(strict=True).parents[1]) / 'node_modules/@playwright/test/index.mjs').resolve(strict=True))}, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, bufsize=1, start_new_session=True)
            report['browserControllerPid'] = controller.pid
            actions = ['authenticate', 'posting-rules-setup'] if args.original_posting_rules else ['authenticate', 'original-dimensions'] if args.original_dimensions else ['authenticate', 'original-commitment'] if args.original_commitment else ['authenticate', 'historical-vat'] if args.historical_vat else ['authenticate', 'original-vat-purchase'] if args.original_vat_purchase else ['authenticate', 'original-vat'] if args.original_vat else ['authenticate', 'original-budget'] if args.original_budget else ['authenticate', 'original-mileage'] if args.original_mileage else ['authenticate', 'intake', 'original-asset'] if args.original_asset else ['authenticate', 'intake', 'original-accrual'] if args.original_accrual else ['authenticate', 'intake', 'original-expense'] if args.original_expense else ['authenticate', 'intake', 'original-parties'] if args.original_document_parties else ['authenticate', 'intake', 'party-links', 'rules', 'workbench', 'approve-apply', 'replay', 'clients']
            setup_deadline = time.monotonic() + 15 * 60
            for action in actions:
                remaining = setup_deadline - time.monotonic()
                if (args.original_document_parties or args.original_expense or args.original_accrual or args.original_asset or args.original_mileage or args.original_budget or args.original_vat or args.original_vat_purchase or args.original_commitment or args.original_dimensions or args.original_posting_rules or args.historical_vat) and remaining <= 0:
                    raise RuntimeError('Original document-party 15 minute setup budget exhausted; preserve profile, no replay')
                phase(action, min(300, remaining) if (args.original_document_parties or args.original_expense or args.original_accrual or args.original_asset or args.original_mileage or args.original_budget or args.original_vat or args.original_vat_purchase or args.original_commitment or args.original_dimensions or args.original_posting_rules or args.historical_vat) else 300)
                if action == 'intake':
                    bank_original()
            if args.original_posting_rules:
                report['postingRulesSetupInventory'] = inspect_resolution_inventory(Path(config['root']) / 'synthetic/data/ledger.sqlite', setup=True)
                initial_control = budget_control_inventory()
                report['postingRulesSetupControlInventory'] = initial_control
                save()
                bank_original()
                remaining = setup_deadline - time.monotonic()
                if remaining <= 0:
                    raise RuntimeError('Posting rules 15 minute setup budget exhausted; preserve profile, no replay')
                phase('original-posting-rules', min(300, remaining))
            if args.original_expense:
                phase('expense-hold-begin')
                if not held_received.wait(15):
                    raise RuntimeError('Held original expense did not reach provider; preserve profile, no retry')
                phase('expense-revoke')
                held_release.set()
                phase('expense-hold-finish')
            if args.original_document_parties or args.original_expense or args.original_accrual or args.original_asset or args.original_mileage or args.original_budget or args.original_vat or args.original_vat_purchase or args.original_commitment or args.original_dimensions or args.original_posting_rules or args.historical_vat:
                initial_inventory = inspect_resolution_inventory(Path(config['root']) / 'synthetic/data/ledger.sqlite')
                report['initialResolutionInventory'] = initial_inventory
                if args.original_budget or args.original_vat or args.original_vat_purchase or args.original_commitment or args.original_dimensions or args.original_posting_rules or args.historical_vat:
                    initial_control = budget_control_inventory()
                    report['initialPostingRulesControlInventory' if args.original_posting_rules else 'initialDimensionsControlInventory' if args.original_dimensions else 'initialCommitmentControlInventory' if args.original_commitment else 'initialVatControlInventory' if (args.original_vat or args.original_vat_purchase or args.original_commitment or args.original_dimensions or args.original_posting_rules or args.historical_vat) else 'initialBudgetControlInventory'] = initial_control
            if args.original_posting_rules:
                phase('posting-rules-readonly')
                if budget_control_inventory() != initial_control or inspect_resolution_inventory(Path(config['root']) / 'synthetic/data/ledger.sqlite') != initial_inventory:
                    raise RuntimeError('Posting rule dry runs changed raw company/control inventory')
            if args.original_dimensions:
                phase('dimensions-readonly')
                if budget_control_inventory() != initial_control or inspect_resolution_inventory(Path(config['root']) / 'synthetic/data/ledger.sqlite') != initial_inventory:
                    raise RuntimeError('Dimension repeated reads changed company/control/audit inventory')
            if args.original_commitment:
                phase('commitment-readonly')
                if budget_control_inventory() != initial_control or inspect_resolution_inventory(Path(config['root']) / 'synthetic/data/ledger.sqlite') != initial_inventory:
                    raise RuntimeError('Commitment repeated client reads changed company/control/audit inventory')
            if args.original_budget:
                phase('budget-readonly')
                if budget_control_inventory() != initial_control:
                    raise RuntimeError('Forecast reads changed exact control business inventory')
                if inspect_resolution_inventory(Path(config['root']) / 'synthetic/data/ledger.sqlite') != initial_inventory:
                    raise RuntimeError('Forecast/client repeated reads changed exact company/audit inventory')
            stop()
            config.pop('firstOwner')
            write_config()
            start()
            phase('reopen')
            if (args.original_mileage or args.original_budget or args.original_vat or args.original_vat_purchase or args.original_commitment or args.original_dimensions or args.original_posting_rules or args.historical_vat) and inspect_resolution_inventory(Path(config['root']) / 'synthetic/data/ledger.sqlite') != initial_inventory:
                raise RuntimeError('Mileage entry/audit inventory changed across restart')
            bank_original()
            archive = phase('archive')
            report['signedSnapshot'] = archive
            if (args.original_document_parties or args.original_expense or args.original_accrual or args.original_asset or args.original_mileage or args.original_budget or args.original_vat or args.original_vat_purchase or args.original_commitment or args.original_dimensions or args.original_posting_rules or args.historical_vat) and not archive['supported']:
                raise RuntimeError('Nonempty resolution histories require signed backup support')
            if archive['supported']:
                progress('operator:independent-signed-snapshot-verification')
                path = work / 'batch.native-backup.json'
                if digest(path) != archive['sha256']:
                    raise RuntimeError('Signed artifact changed after download')
                verified = subprocess.run([str(pinned['server_binary']), '--verify-backup', str(path), '--trusted-backup-key', str(work / 'trusted.pub')], cwd=REPO, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=60)
                if verified.returncode != 0:
                    raise RuntimeError('Independent signature/inventory verification refused')
                if args.original_document_parties or args.original_expense or args.original_accrual or args.original_asset or args.original_mileage or args.original_budget or args.original_vat or args.original_vat_purchase or args.original_commitment or args.original_dimensions or args.original_posting_rules or args.historical_vat:
                    wire = json.loads(path.read_bytes())
                    entries = [e for e in wire['entries'] if e['path'] == 'synthetic/data/ledger.sqlite']
                    if len(entries) != 1:
                        raise RuntimeError('Exact signed resolution ledger required')
                    archive_db = work / 'readonly-resolution-archive.sqlite'
                    archive_db.write_bytes(base64.b64decode(entries[0]['bytesBase64'], validate=True))
                    if inspect_resolution_inventory(archive_db) != initial_inventory:
                        raise RuntimeError('Signed resolution inventory differs from source')
                    report['signedResolutionInventoryVerified'] = True
                report['trustedSourcePublicKeySha256'] = digest(work / 'trusted.pub')
                report['independentArchiveVerification'] = True
                stop()
                source = work / 'workspace'
                if not owned() or source.is_symlink() or not source.is_dir():
                    raise RuntimeError('Exact owned source required before source-free removal')
                shutil.rmtree(source)
                (work / 'signing.der').unlink()
                config.pop('backupSigningKeyFile')
                config.update({'root': str(work / 'restored'), 'cookieKey': key(), 'mfaKey': key(), 'restoreActor': 'system:synthetic-installer', 'firstOwner': {'actor': 'system:synthetic-installer', 'email': 'restored@example.invalid', 'name': 'Synthetic Restored Owner', 'password': 'synthetic-restored-password-789'}})
                write_config()
                start(restore=True)
                restored = phase('restored')
                bank_original()
                report['sourceFreeRestoreQualified'] = restored['sourceFreeRestoreQualified']
                report['sourceWorkspaceAndPrivateSignerRemoved'] = not source.exists() and not (work / 'signing.der').exists()
            if args.original_document_parties or args.original_expense or args.original_accrual or args.original_asset or args.original_mileage or args.original_budget or args.original_vat or args.original_vat_purchase or args.original_commitment or args.original_dimensions or args.original_posting_rules or args.historical_vat:
                restored_inventory = inspect_resolution_inventory(Path(config['root']) / 'synthetic/data/ledger.sqlite')
                with sqlite3.connect(archive_db.resolve(strict=True).as_uri() + '?mode=ro', uri=True) as archived:
                    receipt_high_water = archived.execute('SELECT COALESCE(MAX(id),0) FROM mutation_idempotency_tombstones').fetchone()[0]
                verify_restored_resolution_inventory(initial_inventory, restored_inventory, hashlib.sha256(base64.b64decode(wire['manifestBase64'], validate=True)).hexdigest(), receipt_high_water)
                report['restoredResolutionInventoryVerified'] = True
                if args.original_budget or args.original_vat or args.original_vat_purchase or args.original_commitment or args.original_dimensions or args.original_posting_rules or args.historical_vat:
                    if budget_control_inventory() != initial_control:
                        raise RuntimeError('Restored control business inventory differs')
                    report['restoredPostingRulesControlInventoryVerified' if args.original_posting_rules else 'restoredDimensionsControlInventoryVerified' if args.original_dimensions else 'restoredCommitmentControlInventoryVerified' if args.original_commitment else 'restoredVatControlInventoryVerified' if (args.original_vat or args.original_vat_purchase or args.original_commitment or args.original_dimensions or args.original_posting_rules or args.historical_vat) else 'restoredBudgetControlInventoryVerified'] = True
                report['providerRequests'] = list(vies_requests)
                report['scannerRequests'] = list(scanner_requests)
                save()
                if args.original_vat_purchase and [r['vatNumber'] for r in vies_requests] != ['168000001']:
                    raise RuntimeError('Original VAT purchase requires exactly one explicit existing preflight provider request')
                if (args.original_vat or args.original_commitment or args.original_dimensions or args.original_posting_rules or args.historical_vat) and vies_requests:
                    raise RuntimeError('Original VAT reports must make zero provider requests')
                if len(scanner_requests) != (1 if args.original_posting_rules else 1 if args.original_dimensions else 1 if args.original_commitment else 0 if args.historical_vat else 3 if args.original_vat_purchase else 1 if args.original_vat else 3 if args.original_budget else 0 if args.original_mileage else 2 if (args.original_accrual or args.original_asset) else 6 if args.original_expense else 3):
                    raise RuntimeError('Actual originals must traverse scanner exactly once')
                if args.original_expense and ([r['vatNumber'] for r in vies_requests] != ['168000001', '168000001', '987654321', '987654321', '168000003']):
                    raise RuntimeError('Original expense exact failure/recovery/held provider trace differs')
                report['originalPartyScannerRequests'] = scanner_requests
                verify_pins()
            if args.original_expense or args.original_accrual or args.original_asset or args.original_mileage or args.original_budget or args.original_vat or args.original_vat_purchase or args.original_commitment or args.original_dimensions or args.original_posting_rules or args.historical_vat:
                stop()
                config.pop('firstOwner')
                write_config()
                start()
                phase('restored-reopen')
                if (args.original_accrual or args.original_asset or args.original_mileage or args.original_budget or args.original_vat or args.original_vat_purchase or args.original_commitment or args.original_dimensions or args.original_posting_rules or args.historical_vat) and inspect_resolution_inventory(Path(config['root']) / 'synthetic/data/ledger.sqlite') != restored_inventory:
                    raise RuntimeError('Accrual restored reopen inventory changed')
                verify_pins()
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
            report.update({'status': 'passed', 'currentAction': 'complete', 'originalPostingRulesQualified': args.original_posting_rules, 'originalDimensionsQualified': args.original_dimensions, 'originalCommitmentQualified': args.original_commitment, 'historicalVatQualified': args.historical_vat, 'originalVatPurchaseQualified': args.original_vat_purchase, 'originalVatQualified': args.original_vat, 'originalBudgetQualified': args.original_budget, 'originalMileageQualified': args.original_mileage, 'originalAssetQualified': args.original_asset, 'originalAccrualQualified': args.original_accrual, 'originalExpenseQualified': args.original_expense, 'normalTwoMfaAtomicPurchaseBatchQualified': not (args.original_document_parties or args.original_expense or args.original_accrual or args.original_asset or args.original_mileage or args.original_budget or args.original_vat or args.original_vat_purchase or args.original_commitment or args.original_dimensions or args.original_posting_rules or args.historical_vat), 'originalDocumentPartiesQualified': args.original_document_parties, 'initialAndRestartQualified': True, 'snapshotCoverage': 'source-free-restored' if archive['supported'] else 'explicitly-blocked-by-nonempty-unsupported-inventory'})
            save()
            print(('PASS: original posting rules two versions/six events/six audits and signed recovery; ' if args.original_posting_rules else 'PASS: original dimensions 5 assignments, 6 budget events, exact actual variance and signed recovery; ' if args.original_dimensions else 'PASS: original supplier commitment exact source plan/approval/match/lifecycle and signed recovery; ' if args.original_commitment else 'PASS: attested live Dinero DKK VAT, reviewed close/filing preparation and signed recovery; ' if args.historical_vat else 'PASS: original VAT purchases no-bank EU CLI / linked bank representation MCP, exact signed recovery; ' if args.original_vat_purchase else 'PASS: original VAT one DK purchase, reviewed closed filing aliases and signed recovery; ' if args.original_vat else 'PASS: original budget two revisions, actuals and source cash planning, signed recovery; ' if args.original_budget else 'PASS: original mileage, three trips 15.85 km/71.63, two exact local exports, signed recovery; ' if args.original_mileage else 'PASS: original asset, 5 journals/10 lines, exact signed recovery; ' if args.original_asset else 'PASS: original accrual three types, 8 journals/16 lines, exact signed recovery; ' if args.original_accrual else 'PASS: original expense DK/EU, CLI/MCP, exact posting and signed source-free restore; ' if args.original_expense else 'PASS: isolated original document-party resolution, zero journals, exact restart and signed source-free restore; ' if args.original_document_parties else 'PASS: normal two-MFA reviewed atomic purchase batch, native clients, exact replay and restart; ') + str(args.evidence))
        except BaseException as error:
            report['providerRequests'] = list(vies_requests)
            report['scannerRequests'] = list(scanner_requests)
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
            if owned() and (args.original_document_parties or args.original_expense or args.original_accrual or args.original_asset or args.original_mileage or args.original_budget or args.original_vat or args.original_vat_purchase or args.original_commitment or args.original_dimensions or args.original_posting_rules or args.historical_vat) and report['status'] != 'passed':
                report['failedPrivateProfilePreserved'] = True
                report['ownedRuntimeCleaned'] = False
            elif owned():
                shutil.rmtree(work)
                report['ownedRuntimeCleaned'] = True
            else:
                report['ownedRuntimeCleaned'] = False
            save()


if __name__ == '__main__':
    main()
