#!/usr/bin/env python3
"""Owned macOS hosted-generation proof. Ordinary verification/MFA, no SQL/auth seeds.

Explicit artifacts only. Does not build, deliver mail, provision a service credential
outside the normal owner API, or qualify Linux isolation / restored worker reconsent.
One signed V3 archive is verified source-free and restored to a fresh MFA owner.
Service keys move only over private process pipes; never argv, config or evidence.
"""
import argparse
import base64
import hashlib
import json
import os
import re
import secrets
import selectors
import shutil
import signal
import socket
import subprocess
import time
import urllib.request
import uuid
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
PREFIX = 'Rentemester-Synthetic-Hosted-'
# Enrollment/login and explicit source-template shape follow the existing native
# portable/scheduled helpers. The controller stays alive to retain MFA recovery
# material and service secrets in memory across browser/server restarts.
BROWSER = r'''
import {createHmac,createHash,randomUUID} from 'node:crypto';
import {createConnection} from 'node:net';
import {createInterface} from 'node:readline';
import {readFileSync,readdirSync,writeFileSync,unlinkSync} from 'node:fs';
import {join} from 'node:path';
import {spawnSync} from 'node:child_process';
import {isDeepStrictEqual} from 'node:util';
const {chromium,request}=await import(process.env.RM_HOSTED_PLAYWRIGHT);
const base=process.env.RM_HOSTED_BASE,work=process.env.RM_HOSTED_WORK;
if(!/^http:\/\/127\.0\.0\.1:\d+$/.test(base))throw Error('literal loopback fixture required');
const headers={origin:base,'content-type':'application/json'},prefix='/api/companies/synthetic/';
const password='synthetic-hosted-owner-password-123';let email='owner@example.invalid';
const hash=b=>createHash('sha256').update(b).digest('hex');
let browser=null,page=null,http=null,cookie='',codes=[],lastLogin=0,step='boot',main=null,templateId=null,baseline=null,accepted=null,closedAt=null,ownerId=null,sourceDate=null;
function need(value,label){if(!value)throw Error(label);}
async function json(path,data,expected=200){const r=data===undefined?await http.get(base+path):await http.post(base+path,{headers,data});need(r.status()===expected,`HTTP ${path} ${r.status()} expected${expected}`);return r.json();}
const get=path=>json(path),post=(path,data,status=200)=>json(path,data,status);
const current=async()=> (await get(prefix+'scheduled-recurring')).scheduledRecurring;
const row=async()=>{const rows=(await current()).rows.filter(r=>r.templateId===templateId);need(rows.length===1,'exact scheduled row');return rows[0];};
const recurring=()=>get(prefix+'recurring-invoices');
async function open(){browser=await chromium.launch({headless:true});page=await browser.newPage();page.setDefaultTimeout(45000);http=page.request;}
async function closeBrowser(){cookie=(await page.context().cookies(base)).map(c=>`${c.name}=${c.value}`).join('; ');await browser.close();browser=null;page=null;http=await request.newContext({extraHTTPHeaders:{cookie,origin:base}});closedAt=Date.now();}
function totp(secret){let bits=0,count=0,bytes=[];for(const c of secret){const v='ABCDEFGHIJKLMNOPQRSTUVWXYZ234567'.indexOf(c);need(v>=0,'synthetic MFA key syntax');bits=(bits<<5)|v;count+=5;if(count>=8){count-=8;bytes.push((bits>>>count)&255);}}const counter=Buffer.alloc(8);counter.writeBigUInt64BE(BigInt(Math.floor(Date.now()/30000)));const d=createHmac('sha1',Buffer.from(bytes)).update(counter).digest(),o=d[19]&15;return String((d.readUInt32BE(o)&0x7fffffff)%1000000).padStart(6,'0');}
async function login(){const wait=11000-(Date.now()-lastLogin);if(wait>0)await new Promise(r=>setTimeout(r,wait));lastLogin=Date.now();await page.goto(base);await page.getByLabel('E-mail',{exact:true}).fill(email);await page.getByLabel('Adgangskode',{exact:true}).fill(password);await page.getByRole('button',{name:'Log ind',exact:true}).click();await page.getByRole('button',{name:'Brug recovery code',exact:true}).click();need(codes.length,'unused ordinary recovery code');await page.getByLabel('Recovery code',{exact:true}).fill(codes.shift());await page.getByRole('button',{name:'Bekræft',exact:true}).click();await page.getByRole('link',{name:'Åbn bogføring',exact:true}).waitFor();}
async function enroll(){await page.goto(base);await page.getByLabel('E-mail',{exact:true}).fill(email);await page.getByLabel('Adgangskode',{exact:true}).fill(password);await page.getByRole('button',{name:'Log ind',exact:true}).click();await page.getByRole('heading',{name:'Opsæt totrinsbekræftelse'}).waitFor();await page.getByLabel('Adgangskode',{exact:true}).fill(password);await page.getByRole('button',{name:'Start opsætning',exact:true}).click();const key=await page.getByLabel('Opsætningsnøgle').inputValue();await page.getByLabel('Engangskode',{exact:true}).fill(totp(key));await page.getByRole('button',{name:'Bekræft opsætning',exact:true}).click();await page.getByRole('region',{name:'Recovery codes'}).locator('li').first().waitFor();codes=await page.getByRole('region',{name:'Recovery codes'}).locator('li').allTextContents();need(codes.length>=4,'ordinary MFA recovery inventory');await page.getByRole('button',{name:'Jeg har gemt koderne',exact:true}).click();await page.getByRole('link',{name:'Åbn bogføring',exact:true}).waitFor();await page.context().clearCookies();lastLogin=Date.now();await login();}
async function client(kind,command,body,company=false){
 const path=join(work,'human-client.cookie');writeFileSync(path,(await page.context().cookies(base)).map(c=>`${c.name}=${c.value}`).join('; '),{mode:0o600,flag:'wx'});
 try{const common=['--server',base,'--allow-loopback-http','--cookie-file',path];let args,input;
 if(kind==='cli'){args=['remote',command,...common,...(company?['--company','synthetic']:[]),...(body===undefined?[]:['--confirm'])];input=body===undefined?'':JSON.stringify(body);}
 else{args=common;const messages=[{jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'synthetic-hosted-proof',version:'1'}}},{jsonrpc:'2.0',method:'notifications/initialized'},{jsonrpc:'2.0',id:2,method:'tools/call',params:{name:command,arguments:body===undefined?{}:{body:JSON.stringify(body),confirm:true}}}];input=messages.map(m=>JSON.stringify(m)).join('\n')+'\n';}
 const out=spawnSync(join(work,kind==='cli'?'rentemester-rust':'rentemester-mcp'),args,{input,encoding:'utf8',timeout:30000,maxBuffer:4*1024*1024});need(!out.error&&out.status===0,`pinned ${kind}/${command} exit=${out.status} error=${out.error?.code??'none'} output=${out.stdout?.slice(0,1000)??''} stderr=${out.stderr?.slice(0,200)??''}`);
 if(kind==='cli')return JSON.parse(out.stdout).data;
 const result=out.stdout.trim().split('\n').map(l=>JSON.parse(l)).find(r=>r.id===2);need(!result?.error&&!result?.result?.isError,'MCP lifecycle receipt');return result.result.structuredContent.data;
 }finally{unlinkSync(path);}}
async function serviceContext(key,expected=200){const ctx=await request.newContext({extraHTTPHeaders:{'x-rentemester-service-key':key,origin:base}});try{const r=await ctx.get(base+'/api/me');need(r.status()===expected,`service context HTTP${r.status()}`);return expected===200?r.json():null;}finally{await ctx.dispose();}}
async function original(documentId){const a=await http.get(base+prefix+`documents/${documentId}/file`),b=await http.get(base+prefix+`invoices/${documentId}/pdf`);need(a.ok()&&b.ok(),'immutable generated originals');const jsonBytes=await a.body(),pdfBytes=await b.body(),payload=JSON.parse(jsonBytes.toString());need(pdfBytes.subarray(0,5).equals(Buffer.from('%PDF-')),'actual PDF signature');return{documentId,jsonBase64:jsonBytes.toString('base64'),jsonSha256:hash(jsonBytes),pdfBase64:pdfBytes.toString('base64'),pdfSha256:hash(pdfBytes),payload};}
async function observe(expected,minimumMs){const start=Date.now();let latest;
 while(Date.now()-start<90000){latest=await recurring();const generated=latest.recurringInvoices.templates.find(t=>t.id===templateId)?.generations;need(Array.isArray(generated)&&generated.length<=expected,'duplicate/unexpected generation');if(generated.length===expected&&Date.now()-start>=minimumMs)return latest;await new Promise(r=>setTimeout(r,1000));}throw Error('bounded worker observation expired');}
async function setup(){
 step='ordinary-owner-verification-mfa';await open();const spool=join(work,'workspace','.rentemester'),files=readdirSync(spool).filter(n=>n.startsWith('verification-')&&n.endsWith('.txt'));need(files.length===1,'one normal verification');await page.goto(readFileSync(join(spool,files[0]),'utf8').trim());await enroll();const me=await get('/api/me');need(me.user?.twoFactorEnabled&&me.companies.some(c=>c.slug==='synthetic'&&c.role==='owner'),'ordinary owner current MFA');
 step='ordinary-web-service-create-activate-grant';ownerId=me.user.id;
 step='web-service-navigate-access';await page.goto(base+'/adgang');
 const services=page.getByRole('region',{name:'Servicekonti',exact:true});
 step='web-service-fill-name';await services.getByLabel('Navn på servicekonto',{exact:true}).fill('Synthetic Hosted Executor');
 const issued=page.waitForResponse(r=>new URL(r.url()).pathname==='/api/workspace/service-principals'&&r.request().method()==='POST');
 step='web-service-open-create-confirmation';await services.getByRole('button',{name:'Opret servicekonto',exact:true}).click();
 step='web-service-confirm-create';await page.getByRole('dialog',{name:'Bekræft servicehandling',exact:true}).getByRole('button',{name:'Gennemfør servicehandling',exact:true}).click();
 step='web-service-create-response';const issuance=await issued;need(issuance.status()===201,'ordinary web service creation');main=await issuance.json();
 need(main.secret&&main.serviceAccountId&&main.credentialId,'one-time lifecycle receipt');
 step='web-service-one-time-secret';need(await services.getByLabel('Engangsnøgle',{exact:true}).inputValue()===main.secret,'web one-time credential receipt');
 const persisted=await page.evaluate(()=>JSON.stringify({session:{...sessionStorage},local:{...localStorage}}));
 need(!persisted.includes(main.secret),'service plaintext persisted by web UI');
 await services.getByRole('button',{name:'Skjul og glem nøglen',exact:true}).click();
 step='web-service-one-time-secret';need(await services.getByLabel('Engangsnøgle',{exact:true}).count()===0,'explicit one-time secret dismissal');
 const principal=services.getByRole('region',{name:'Synthetic Hosted Executor',exact:true});
 const activated=page.waitForResponse(r=>new URL(r.url()).pathname==='/api/workspace/members/access'&&r.request().method()==='POST');
 step='web-service-activate-workspace';await principal.getByRole('button',{name:'Aktivér workspace-adgang som medlem',exact:true}).click();
 await page.getByRole('dialog',{name:'Bekræft servicehandling',exact:true}).getByRole('button',{name:'Gennemfør servicehandling',exact:true}).click();
 need((await activated).ok(),'ordinary explicit service workspace activation');
 step='web-service-select-activated-member';await page.getByRole('combobox',{name:'Bruger',exact:true}).selectOption(main.serviceAccountId);
 await page.getByRole('combobox',{name:'Virksomhedsrolle',exact:true}).selectOption('bookkeeper');
 const granted=page.waitForResponse(r=>new URL(r.url()).pathname==='/api/workspace/members/company'&&r.request().method()==='POST');
 step='web-service-company-grant';await page.getByRole('button',{name:'Gem virksomhedsrolle',exact:true}).click();
 await page.getByRole('dialog',{name:'Bekræft adgangsændring',exact:true}).getByRole('button',{name:'Gennemfør ændring',exact:true}).click();
 need((await granted).ok(),'ordinary explicit service bookkeeper grant');
 const service=await serviceContext(main.secret);need(service.user.id===main.serviceAccountId&&service.workspaceRole==='member'&&service.companies.some(c=>c.slug==='synthetic'&&c.role==='bookkeeper'),'real admitted service context');
 const before=await get('/api/workspace/service-principals'),listed=await client('mcp','service_principals');need(isDeepStrictEqual(before,listed),'HTTP/MCP exact lifecycle inventory');
 const rotated=await client('cli','rotate-service-principal',{serviceAccountId:main.serviceAccountId,credentialId:main.credentialId,operationId:randomUUID(),confirm:true});need(rotated.secret&&rotated.credentialId!==main.credentialId,'rotation distinct receipt');await serviceContext(main.secret,401);main=rotated;await serviceContext(main.secret);
 // Native lifecycle inventory and mutations share the existing10/minute cooldown.
 // Separate the auxiliary recovery exercise without retrying or relaxing admission.
 step='existing-lifecycle-cooldown';await new Promise(r=>setTimeout(r,61000));
 step='lost-issuance-explicit-recovery';const operationId=randomUUID(),body=JSON.stringify({displayName:'Synthetic Lost Response Executor',operationId,confirm:true}),session=(await page.context().cookies(base)).map(c=>`${c.name}=${c.value}`).join('; ');
 await new Promise((resolve,reject)=>{const url=new URL(base),socket=createConnection({host:url.hostname,port:Number(url.port)},()=>socket.write(`POST /api/workspace/service-principals HTTP/1.1\r\nHost: ${url.host}\r\nOrigin: ${base}\r\nContent-Type: application/json\r\nCookie: ${session}\r\nContent-Length: ${Buffer.byteLength(body)}\r\nConnection: close\r\n\r\n${body}`));socket.setTimeout(30000,()=>{socket.destroy();reject(Error('lost response fixture timeout'));});socket.on('error',reject);socket.on('data',()=>{});socket.on('end',()=>{socket.destroy();resolve();});});
 const recovered=await client('mcp','recover_service_principal',{operationId,confirm:true});need(recovered.operationId===operationId&&recovered.status==='completed'&&!('secret'in recovered)&&recovered.serviceAccountId&&recovered.credentialId,'explicit recovery without redisclosure');await client('cli','revoke-service-principal',{serviceAccountId:recovered.serviceAccountId,credentialId:recovered.credentialId,operationId:randomUUID(),confirm:true});const inventory=await get('/api/workspace/service-principals');need(inventory.principals.find(p=>p.serviceAccountId===recovered.serviceAccountId)?.activeCredentialCount===0,'recovered credential explicit revoke');
 step='explicit-unposted-today-template';const profile=await http.patch(base+prefix+'company',{headers,data:{cvr:'87654321',address:'Synthetic Hosted Seller Street 1',postalCode:'1000',city:'Synthetic City',vatPeriodType:'quarter',paymentTermsDays:14}});need(profile.ok(),'explicit seller setup');const buyer=(await post(prefix+'customers',{name:'Synthetic Hosted Buyer',address:'Synthetic Hosted Buyer Street 2',vatOrCvr:'DK12345678',defaultCurrency:'DKK',paymentTermsDays:14})).customer;
 const today=(await current()).asOfDate;sourceDate=today;need(/^\d{4}-\d{2}-\d{2}$/.test(today),'server Copenhagen date');const created=await post(prefix+'recurring-invoices',{name:'Synthetic Explicit Hosted Weekly',interval:'weekly',intervalCount:1,deliveryChannel:'manual',firstIssueDate:today,paymentTermsDays:14,deliveryPeriodMode:'interval_window',currency:'DKK',customerId:buyer.id,vatRatePercent:25,lines:[{description:'Synthetic hosted exact source service',quantity:1,unitPriceExVat:100}],confirm:true});templateId=created.template.templateId;need(Number.isSafeInteger(templateId),'exact template ID');const plan=(await get(prefix+`scheduled-recurring/${templateId}/plan`)).plan;
 await client('cli','enable-scheduled-recurring',{templateId,templateHash:plan.templateHash,confirm:true},true);baseline={journal:await get(prefix+'journal'),recurring:await recurring()};need(baseline.journal.journal.entries.length===0&&baseline.recurring.recurringInvoices.templates.find(t=>t.id===templateId).generations.length===0,'fresh no-worker baseline');await page.goto(base+'/companies/synthetic/faktura-skabeloner');await page.getByRole('region',{name:'Planlagte fakturaer',exact:true}).waitFor();need(isDeepStrictEqual(await recurring(),baseline.recurring),'readonly app view issued without driver');await observe(0,35000);need(isDeepStrictEqual(await recurring(),baseline.recurring),'no-worker read window issued unexpectedly');
 await closeBrowser();return{ownerUserId:me.user.id,serviceAccountId:main.serviceAccountId,credentialId:main.credentialId,templateId,today,readonlyNoWorkerObservedMs:35000,lifecycle:{webCreateActivateAndCompanyGrant:true,oneTimeWebSecretNotPersisted:true,cliRotateRevoke:true,mcpInventoryAndRecovery:true,oldCredential401:true,lostResponseOperationId:operationId,recoveryWithoutRedisclosure:true},ledgerUuid:baseline.journal.journal.ledgerUuid};
}
async function consent(){
 step='fresh-browser-mfa-hosted-review';await http.dispose();await open();await login();need(isDeepStrictEqual(await recurring(),baseline.recurring),'worker issued before explicit hosted consent');await page.goto(base+'/companies/synthetic/faktura-skabeloner');const panel=page.getByRole('region',{name:'Planlagte fakturaer',exact:true});await panel.getByLabel('Planlagt fakturaskabelon',{exact:true}).selectOption(String(templateId));const hosted=panel.getByRole('region',{name:'Planlægning på serveren',exact:true});const requested=page.waitForResponse(r=>new URL(r.url()).pathname===prefix+`scheduled-recurring/${templateId}/hosted-plan`&&r.request().method()==='GET');await hosted.getByRole('button',{name:'Gennemgå serverplan og servicekonto',exact:true}).click();const r=await requested;need(r.ok(),'current hosted plan');const p=(await r.json()).hostedPlan;need(p.executor.serviceAccountId===main.serviceAccountId&&p.executor.credentialId===main.credentialId&&p.policy==='normal_due_only'&&p.schedule.missedCount===0&&p.schedule.queued.length===0&&!p.schedule.truncated,'exact configured executor and clean reviewed source');await hosted.getByRole('heading',{name:p.template.name,exact:true}).waitFor();await hosted.getByText('Synthetic hosted exact source service',{exact:true}).waitFor();const button=hosted.getByRole('button',{name:'Godkend automatisk udstedelse på serveren',exact:true});need(await button.isDisabled(),'consent lacked explicit reason/confirmation gate');await hosted.getByLabel('Begrundelse for serversamtykke',{exact:true}).fill('Ordinary verified owner explicitly delegates only today normal due generation to this selected service');await hosted.getByLabel('Godkend den viste serverhandling og servicekonto.',{exact:true}).check();const submitted=page.waitForResponse(r=>new URL(r.url()).pathname===prefix+'scheduled-recurring/hosted-consent'&&r.request().method()==='POST');await button.click();const result=await submitted;need(result.ok(),'explicit hosted UI consent');need(isDeepStrictEqual(result.request().postDataJSON(),{templateId,serviceAccountId:p.executor.serviceAccountId,credentialId:p.executor.credentialId,planHash:p.planHash,reason:'Ordinary verified owner explicitly delegates only today normal due generation to this selected service',confirm:true}),'UI submitted exact reviewed source and selected executor only');const next=(await result.json()).scheduledRecurring;need(next.executionMode==='hosted'&&next.hostedConsent?.active===true&&next.hostedConsent.serviceAccountId===main.serviceAccountId&&next.hostedConsent.credentialId===main.credentialId,'current canonical hosted grant');need(isDeepStrictEqual(await row(),next),'hosted receipt exact current readback');need(isDeepStrictEqual(await recurring(),baseline.recurring),'consent must not issue before browser closes');await closeBrowser();return{browserClosed:true,reviewedPlanHash:p.planHash,authorityHash:p.authorityHash,hostedConsent:next.hostedConsent};
}
async function qualify(reopened=false){
 step=reopened?'restart-no-replay-observation':'closed-browser-worker-observation';need(!browser&&closedAt,'browser genuinely closed');const observed=await observe(1,35000),generations=observed.recurringInvoices.templates.find(t=>t.id===templateId).generations;const generation=generations[0];need(generation.periodIndex===0&&generation.documentId,'one exact TODAY generation');const proof=await original(generation.documentId);need(proof.payload.recurringTemplateId===templateId&&proof.payload.recurringPeriodIndex===0&&proof.payload.totals.grossAmount===125,'immutable source binds exact intended occurrence');const invoices=(await get(prefix+'invoices')).invoices.invoices,invoice=invoices.find(i=>i.documentId===generation.documentId);need(invoice&&invoice.posted===false&&invoice.lastEmailedAt===null,'generation is unposted/unsent');const journal=await get(prefix+'journal');need(isDeepStrictEqual(journal,baseline.journal),'hosted generation mutated books');const canonical={recurring:observed,row:await row(),original:proof};need(canonical.row.lastRun?.status==='succeeded'&&canonical.row.hostedConsent?.serviceAccountId===main.serviceAccountId,'durable hosted result');const executions=canonical.row.history.filter(h=>h.version===3&&h.hosted?.kind==='execution');need(executions.length===2&&executions.filter(h=>h.kind==='attempted').length===1&&executions.filter(h=>h.kind==='succeeded').length===1,'one durable V3 service attempt/result');for(const event of executions){need(event.actor===main.serviceAccountId&&event.hosted.principalKind==='service_account'&&event.hosted.binding.serviceAccountId===main.serviceAccountId&&event.hosted.binding.credentialId===main.credentialId&&event.hosted.binding.grantorUserId===ownerId&&event.hosted.binding.policy==='normal_due_only'&&event.hosted.consentHash===canonical.row.hostedConsent.consentHash,'actual service principal distinct from human grantor');if(event.kind==='attempted')need(event.data.occurrences.length===1&&event.data.occurrences[0].periodIndex===0&&event.data.occurrences[0].issueDate===sourceDate,'service attempted exact normal TODAY occurrence');else need(event.data.generation.documentId===generation.documentId&&event.data.generation.templateId===templateId&&event.data.generation.periodIndex===0,'service success bound to actual generated original');}if(reopened)need(isDeepStrictEqual(canonical,accepted),'restart changed source/history/identity or replayed');else accepted=canonical;return{exactlyOne:true,observedBrowserClosedForAtLeastMs:35000,unpostedUnsent:true,unchangedJournal:true,ledgerUuid:journal.journal.ledgerUuid,canonical};
}
async function reconnect(){step='restart-ordinary-password-mfa';await http.dispose();await open();await login();need(isDeepStrictEqual(await recurring(),accepted.recurring),'restart issued before read');await closeBrowser();return{freshPasswordAndMfa:true,browserClosed:true};}

async function exportArchive(){step='actual-signed-v3-export';need(isDeepStrictEqual(await recurring(),accepted.recurring),'source changed before signed export');const response=await http.post(base+'/api/workspace/backup',{headers,data:{confirm:true}});need(response.ok()&&response.headers()['content-type']==='application/vnd.rentemester.native-backup+json','actual signed backup endpoint');const bytes=await response.body();need(bytes.length>0,'nonempty signed V3 archive');writeFileSync(join(work,'hosted.native-backup.json'),bytes,{flag:'wx',mode:0o600});return{bytes:bytes.length,sha256:hash(bytes)};}
async function restored(){step='source-free-fresh-owner-verification-mfa';await http.dispose();await open();email='restored-owner@example.invalid';const spool=join(work,'restored','.rentemester'),files=readdirSync(spool).filter(n=>n.startsWith('verification-')&&n.endsWith('.txt'));need(files.length===1,'one new-owner verification after signed restore');await page.goto(readFileSync(join(spool,files[0]),'utf8').trim());await enroll();const me=await get('/api/me');need(me.user.id!==ownerId&&me.user.id!==main.serviceAccountId&&me.user.twoFactorEnabled&&me.companies.some(c=>c.slug==='synthetic'&&c.role==='owner'),'genuinely new restored MFA owner');const inventory=await get('/api/workspace/service-principals');need(inventory.principals.length===0,'service credential authority was imported');await serviceContext(main.secret,401);const imported=await row();need(isDeepStrictEqual(imported.history,accepted.row.history)&&imported.templateHash===accepted.row.templateHash,'signed V3 raw history/actors changed');need(imported.state==='paused'&&imported.ownerUserId===null&&imported.queued.length===0&&imported.restoreConsent?.required===true&&imported.hostedConsent?.active===false&&imported.hostedConsent.serviceAccountId===main.serviceAccountId&&imported.hostedConsent.credentialId===main.credentialId,'restored ordinary/hosted authority not suspended with provenance retained');const plan=(await get(prefix+`scheduled-recurring/${templateId}/plan`)).plan;need(plan.normalDue===null&&plan.queued.length===0&&plan.restoreConsent?.required===true,'restored readonly plan made work executable');await page.goto(base+'/companies/synthetic/faktura-skabeloner');const panel=page.getByRole('region',{name:'Planlagte fakturaer',exact:true});await panel.getByLabel('Planlagt fakturaskabelon',{exact:true}).selectOption(String(templateId));await panel.getByRole('region',{name:'Planlægning efter gendannelse',exact:true}).waitFor();await closeBrowser();const observed=await observe(1,35000),again=await row(),proof=await original(accepted.original.documentId);need(isDeepStrictEqual(observed,accepted.recurring)&&isDeepStrictEqual(again,imported)&&isDeepStrictEqual(proof,accepted.original)&&isDeepStrictEqual(await get(prefix+'journal'),baseline.journal),'restore/read/browser closure changed original/history/books or dispatched');return{normalNewOwnerMfa:true,freshOwnerDiffers:true,sourceHistoryExact:true,sourceOriginalsExact:true,ledgerUuid:baseline.journal.journal.ledgerUuid,ordinaryReconsentRequired:true,hostedAuthorityInactive:true,sourceServiceKey401:true,noImportedServicePrincipals:true,readonlyNoGenerationObservedMs:35000,restoredReopenQualified:false,row:imported};}

const input=createInterface({input:process.stdin,crlfDelay:Infinity});
for await(const line of input){let cmd;try{cmd=JSON.parse(line);let data;if(cmd.action==='setup')data=await setup();else if(cmd.action==='worker-key')data={secret:main.secret};else if(cmd.action==='consent')data=await consent();else if(cmd.action==='qualify')data=await qualify(false);else if(cmd.action==='reconnect')data=await reconnect();else if(cmd.action==='reopened')data=await qualify(true);else if(cmd.action==='export')data=await exportArchive();else if(cmd.action==='restored')data=await restored();else if(cmd.action==='finish'){if(browser)await browser.close();if(http)await http.dispose();process.stdout.write(JSON.stringify({ok:true,data:{finished:true}})+'\n');break;}else throw Error('unknown controller command');process.stdout.write(JSON.stringify({ok:true,data})+'\n');}catch(error){process.stdout.write(JSON.stringify({ok:false,step,failureType:error?.name??'Error',failureLabel:String(error?.message??'').replace(/rms_[A-Za-z0-9_-]{64}/g,'[redacted-service-key]').replaceAll(password,'[synthetic-password]').slice(0,1600)})+'\n');if(browser)await browser.close().catch(()=>{});if(http)await http.dispose().catch(()=>{});process.exitCode=1;break;}}
'''


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for flag in ['server-binary', 'web-dir', 'cli-binary', 'mcp-binary']:
        parser.add_argument('--' + flag, required=True, type=Path)
    parser.add_argument('--evidence', type=Path, help='NEW JSON path, never overwrite')
    args = parser.parse_args()
    if os.uname().sysname != 'Darwin' or os.getuid() == 0 or os.getuid() != os.geteuid():
        raise RuntimeError('Ordinary macOS user required')
    os.umask(0o077)
    output = (args.evidence or Path('/tmp/rentemester-rust-reports') / ('native-hosted-scheduled-' + uuid.uuid4().hex + '.json')).absolute()
    output.parent.mkdir(parents=True, exist_ok=True)
    parent = (Path.home() / 'Library/Application Support').resolve(strict=True)
    work = parent / (PREFIX + uuid.uuid4().hex)
    child = controller = log = None
    identity = None
    evidence = {'status': 'running', 'profile': 'local-user-macos', 'noOfflineIdentitySeed': True,
                'linuxIsolationQualified': False, 'externalDeliveryQualified': False,
                'signedV3RestoreQualified': False, 'globalArchiveSingleUseClaim': False, 'sleepDstCatchupQualified': False, 'newDueRevocationOrdering': 'core unit coverage; not this actual fixture',
                'sourceRevision': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=REPO, text=True).strip(),
                'runnerSha256': sha(Path(__file__)), 'embeddedHelperSha256': hashlib.sha256(BROWSER.encode()).hexdigest(), 'phases': [], 'serverPids': []}
    with output.open('x', encoding='utf8') as report:
        def save():
            report.seek(0); report.truncate(); json.dump(evidence, report, indent=2); report.flush(); os.fsync(report.fileno())

        def owned():
            if identity is None or not work.exists(): return False
            st = work.lstat()
            return not work.is_symlink() and work.parent == parent and work.name.startswith(PREFIX) and st.st_uid == os.getuid() and (st.st_dev, st.st_ino) == identity

        def stop():
            nonlocal child, log
            if child is not None:
                if child.poll() is None: child.send_signal(signal.SIGTERM)
                try: child.wait(timeout=10)
                except subprocess.TimeoutExpired: child.kill(); child.wait(timeout=5)
                child = None
            if log: log.close(); log = None

        def command(action, timeout=120):
            if controller.poll() is not None: raise RuntimeError('Owned browser controller exited')
            controller.stdin.write(json.dumps({'action': action}) + '\n'); controller.stdin.flush()
            with selectors.DefaultSelector() as selector:
                selector.register(controller.stdout, selectors.EVENT_READ)
                if not selector.select(timeout): raise RuntimeError('Bounded controller phase timeout: ' + action)
            wire = controller.stdout.readline()
            try: result = json.loads(wire)
            except Exception: raise RuntimeError('Invalid private controller response') from None
            if result.get('ok') is not True:
                evidence['failedControllerStep'] = result.get('step'); evidence['controllerFailureType'] = result.get('failureType'); evidence['controllerFailureLabel'] = result.get('failureLabel'); save()
                raise RuntimeError('Owned browser controller check failed: ' + action)
            return result['data']

        try:
            save(); work.mkdir(mode=0o700); st = work.stat(); identity = (st.st_dev, st.st_ino)
            pinned = {}
            for attr, name in [('server_binary', 'rentemester-server'), ('cli_binary', 'rentemester-rust'), ('mcp_binary', 'rentemester-mcp')]:
                source = getattr(args, attr).resolve(strict=True)
                if not source.is_file(): raise RuntimeError('Explicit artifact must be a regular binary')
                target = work / name; shutil.copyfile(source, target); target.chmod(0o700); pinned[attr] = target
            web = work / 'web'; shutil.copytree(args.web_dir.resolve(strict=True), web)
            if not (web / 'index.html').is_file(): raise RuntimeError('Explicit built web directory required')
            evidence['artifacts'] = {name: sha(path) for name, path in pinned.items()}
            evidence['webFilesSha256'] = {str(path.relative_to(web)): sha(path) for path in sorted(web.rglob('*')) if path.is_file()}
            evidence['ownedFixture'] = str(work); save()
            def scan_service_plaintext():
                # Native NewServiceKey and storage require exactly64 ASCII letters.
                # A wider alphabet falsely joins SQLite start/reference/prefix/hash cells.
                pattern = re.compile(rb'rms_[A-Za-z]{64}')
                for path in work.rglob('*'):
                    if not path.is_file() or path.is_symlink() or path in pinned.values() or web in path.parents: continue
                    with path.open('rb') as stream:
                        carry = b''
                        while chunk := stream.read(1024*1024):
                            data = carry+chunk
                            if pattern.search(data):
                                evidence['servicePlaintextDetectedIn'] = str(path.relative_to(work)); save()
                                raise RuntimeError('Service plaintext persisted in owned fixture: '+str(path.relative_to(work)))
                            carry = data[-80:]
                if pattern.search(json.dumps(evidence).encode()): raise RuntimeError('Service plaintext entered evidence')
            with socket.socket() as sock: sock.bind(('127.0.0.1', 0)); port = sock.getsockname()[1]
            base = f'http://127.0.0.1:{port}'
            key = lambda: base64.urlsafe_b64encode(secrets.token_bytes(32)).decode().rstrip('=')
            # Same explicit synthetic operator-key provisioning as the portable J05 runner.
            # Private signer stays outside the source workspace and is removed before verify.
            subprocess.run(['/opt/homebrew/bin/bun','-e',"const c=require('node:crypto'),f=require('node:fs');const k=c.generateKeyPairSync('ed25519');const seed=k.privateKey.export({format:'der',type:'pkcs8'}).subarray(-32),pub=k.publicKey.export({format:'der',type:'spki'}).subarray(-32);f.writeFileSync(process.argv[1],Buffer.concat([Buffer.from('3051020101300506032b657004220420','hex'),seed,Buffer.from('812100','hex'),pub]),{mode:0o600});f.writeFileSync(process.argv[2],pub,{mode:0o600});",str(work/'signing.der'),str(work/'trusted.pub')],check=True,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
            config_path = work / 'server.json'
            config = {'runtimeProfile': 'local-user-macos', 'root': str(work / 'workspace'), 'brokerUid': os.getuid(),
                      'workspace': 'synthetic-native-hosted', 'companySlug': 'synthetic', 'companyName': 'Synthetic Hosted Company',
                      'cookieKey': key(), 'mfaKey': key(), 'backupSigningKeyFile': str(work/'signing.der'), 'operatorActors': ['system:synthetic-installer'],
                      'firstOwner': {'actor': 'system:synthetic-installer', 'email': 'owner@example.invalid', 'name': 'Synthetic Hosted Owner', 'password': 'synthetic-hosted-owner-password-123'}, 'accounts': []}
            def write():
                if not owned(): raise RuntimeError('Owned fixture identity changed')
                config_path.write_text(json.dumps(config)); config_path.chmod(0o600)
            def start(create=False, worker=False, extra=None):
                nonlocal child, log
                if child is not None or not owned(): raise RuntimeError('Exact prior child must stop before restart')
                argv = [str(pinned['server_binary']), '--config', str(config_path), '--web-dir', str(web), '--listen', f'127.0.0.1:{port}']
                if create: argv.append('--create')
                if extra: argv.extend(extra)
                if worker: argv.extend(['--scheduled-worker', '--scheduled-service-key-stdin'])
                log = open(work / 'server.log', 'ab')
                child = subprocess.Popen(argv, cwd=REPO, stdin=subprocess.PIPE if worker else subprocess.DEVNULL, stdout=log, stderr=log)
                evidence['serverPids'].append(child.pid); save()
                if worker:
                    material = command('worker-key')
                    secret = bytearray(material.pop('secret').encode())
                    try:
                        if len(secret) > 4096 or b'\n' in secret: raise RuntimeError('Bounded single service-key line required')
                        child.stdin.write(secret); child.stdin.write(b'\n'); child.stdin.close()
                    finally:
                        secret[:] = b'\0' * len(secret)
                for _ in range(300):
                    if child.poll() is not None: raise RuntimeError('Owned server exited before readiness')
                    try:
                        with urllib.request.urlopen(base + '/api/health', timeout=.2) as response:
                            if json.load(response).get('workspaceReady'): return
                    except Exception: pass
                    time.sleep(.1)
                raise RuntimeError('Owned server readiness timed out')
            write(); start(create=True)
            script = work / 'browser.mjs'; script.write_text(BROWSER); script.chmod(0o600)
            playwright = (REPO / 'node_modules/@playwright/test/index.mjs').resolve(strict=True)
            controller = subprocess.Popen(['/opt/homebrew/bin/bun', 'run', str(script)], cwd=REPO,
                                          env={**os.environ, 'RM_HOSTED_BASE': base, 'RM_HOSTED_WORK': str(work), 'RM_HOSTED_PLAYWRIGHT': str(playwright)},
                                          stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, bufsize=1, start_new_session=True)
            evidence['setup'] = command('setup',timeout=180); evidence['phases'].append('normal-owner-lifecycle-readonly-no-worker'); save()
            stop(); config.pop('firstOwner'); write(); start(worker=True)
            evidence['consent'] = command('consent'); evidence['phases'].append('explicit-hosted-browser-consent-browser-closed'); save()
            evidence['issued'] = command('qualify'); evidence['phases'].append('closed-browser-at-least-35s-exactly-one'); save()
            stop(); start(worker=True)
            evidence['reconnect'] = command('reconnect')
            evidence['reopened'] = command('reopened'); evidence['phases'].append('restart-fresh-mfa-at-least-35s-no-replay'); save()
            evidence['signedArchive'] = command('export'); stop(); scan_service_plaintext()
            source = work/'workspace'
            if not owned() or source.is_symlink() or not source.is_dir(): raise RuntimeError('Exact owned source removal refused')
            shutil.rmtree(source); (work/'signing.der').unlink()
            trusted = work/'trusted.pub'; archive = work/'hosted.native-backup.json'
            if sha(archive) != evidence['signedArchive']['sha256']: raise RuntimeError('Exported signed artifact changed')
            evidence['signedArchive'].update({'sourceAndSignerUnavailable':True,'trustedPublicKeySha256':sha(trusted)})
            verified = subprocess.run([str(pinned['server_binary']),'--verify-backup',str(archive),'--trusted-backup-key',str(trusted)],cwd=REPO,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=60)
            if verified.returncode != 0: raise RuntimeError('Independent source-free V3 verification refused')
            evidence['signedArchive']['independentlyVerifiedAfterSourceRemoval']=True; save()
            config.update({'root':str(work/'restored'),'cookieKey':key(),'mfaKey':key(),'restoreActor':'system:synthetic-installer','firstOwner':{'actor':'system:synthetic-installer','email':'restored-owner@example.invalid','name':'Synthetic Restored Hosted Owner','password':'synthetic-hosted-owner-password-123'}})
            config.pop('backupSigningKeyFile'); write()
            if os.path.lexists(work/'restored'): raise RuntimeError('Fresh restore target required')
            start(extra=['--restore-backup',str(archive),'--trusted-backup-key',str(trusted),'--confirm-target',config['root'],'--source-unavailable'])
            evidence['restored'] = command('restored'); evidence['phases'].append('source-free-verify-fresh-owner-mfa-suspended-v3-no-generation'); evidence['signedV3RestoreQualified']=True; save()
            command('finish'); controller.stdin.close(); controller.wait(timeout=10); controller = None; stop()
            # Recheck restored mutable material/evidence too; source was checked
            # before its removal, including the unknown lost-response credential.
            scan_service_plaintext()
            evidence.update({'status': 'passed', 'normalVerificationAndMfa': True, 'browserClosedHostedIssuance': True,
                             'restartNoReplay': True, 'oneIssuedUnpostedUnsent': True, 'servicePlaintextAbsentFromMutableFilesAndEvidence': True})
            save()
        except BaseException as error:
            evidence.update({'status': 'failed', 'failureType': type(error).__name__}); save(); raise
        finally:
            stop()
            if controller is not None:
                if controller.poll() is None: os.killpg(controller.pid, signal.SIGTERM)
                try: controller.wait(timeout=10)
                except subprocess.TimeoutExpired: os.killpg(controller.pid, signal.SIGKILL); controller.wait(timeout=5)
            if identity is not None:
                if not owned(): raise RuntimeError('Owned cleanup identity changed; deletion refused')
                shutil.rmtree(work)
            evidence['ownedCleanupCompleted'] = True; save()
    print('PASS: ordinary MFA/service lifecycle, explicit hosted consent, closed-browser issuance and restart; ' + str(output))


if __name__ == '__main__':
    main()
