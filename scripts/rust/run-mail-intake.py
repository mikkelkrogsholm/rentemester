#!/usr/bin/env python3
"""Owned ordinary-MFA Bilagsmail + real TLS IMAP + explicit review and signed recovery.
New private synthetic profile only; no identity/ledger SQL seeds, external mailbox,
automatic retries, guessed metadata, hidden posting, or restored credentials.
Normal auth/process/signing scaffold follows run-bank-profile.py; no shared helper is edited.
--self-test exercises ONLY the private synthetic protocol server, never the product.
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
import socketserver
import ssl
import subprocess
import tempfile
import threading
import time
import uuid
from pathlib import Path
REPO=Path(__file__).resolve().parents[2]
PREFIX='Rentemester-Synthetic-Mail-Intake-'
SIGNER="const c=require('node:crypto'),f=require('node:fs'),k=c.generateKeyPairSync('ed25519'),s=k.privateKey.export({format:'der',type:'pkcs8'}).subarray(-32),p=k.publicKey.export({format:'der',type:'spki'}).subarray(-32);f.writeFileSync(process.argv[1],Buffer.concat([Buffer.from('3051020101300506032b657004220420','hex'),s,Buffer.from('812100','hex'),p]),{mode:0o600});f.writeFileSync(process.argv[2],p,{mode:0o600});"
BROWSER = r'''







import {createHmac,createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {createInterface} from 'node:readline';
import {readFileSync,readdirSync,writeFileSync,unlinkSync} from 'node:fs';
import {join} from 'node:path';
import {isDeepStrictEqual} from 'node:util';
const {chromium}=await import(process.env.RM_MAIL_INTAKE_PLAYWRIGHT);
const base=process.env.RM_MAIL_INTAKE_BASE,work=process.env.RM_MAIL_INTAKE_WORK;
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
async function newPage(){const context=await browser.newContext();const page=await context.newPage();page.setDefaultTimeout(45000);diagnosticPage=page;page.on('request',r=>{if(new URL(r.url()).pathname==='/api/auth/request-password-reset')trace.push({kind:'request',path:'/api/auth/request-password-reset',method:r.method()});});page.on('requestfailed',r=>{if(new URL(r.url()).pathname==='/api/auth/request-password-reset')trace.push({kind:'requestfailed',path:'/api/auth/request-password-reset',code:r.failure()?.errorText?.match(/net::[A-Z_]+/)?.[0]});});page.on('response',r=>{const path=new URL(r.url()).pathname;if(path.startsWith('/api/auth/')){trace.push({path:path.startsWith('/api/auth/reset-password/')?'/api/auth/reset-password/:token':path,status:r.status()});if(trace.length>24)trace.shift();}});page.on('response',r=>{const u=new URL(r.url());if(/^\/api\/companies\/[a-z0-9-]+\/(bilagsmail|workspace-inbox|documents|journal|trial-balance)(\/|$)/.test(u.pathname)){trace.push({step,path:u.pathname,status:r.status(),method:r.request().method(),year:u.searchParams.get('year'),source:u.searchParams.get('source'),format:u.searchParams.get('format')});if(trace.length>32)trace.shift();}});return page;}
async function authenticate(){
 browser=await chromium.launch({headless:true});ownerPage=await newPage();memberPage=await newPage();
 step='owner-mail-verification';await ownerPage.goto((await mail('verification',owner.email)).actionUrl);const who=await enroll(ownerPage,owner);ownerId=who.user.id;
 step='ordinary-web-invitation';await ownerPage.goto(base+'/adgang');await ownerPage.getByRole('heading',{name:'Brugere og adgang',exact:true}).waitFor();await ownerPage.getByLabel('E-mail',{exact:true}).fill(member.email);await ownerPage.getByLabel(/^Rolle/).selectOption('reviewer');await ownerPage.getByRole('button',{name:'Send invitation',exact:true}).click();await ownerPage.getByText('Invitationen er sendt.',{exact:true}).waitFor();const invitation=await mail('workspace-invitation',member.email);const listed=await get(ownerPage,'/api/workspace/invitations');const item=listed.invitations.find(i=>i.email===member.email);need(item?.status==='delivery_confirmed','canonical invitation delivery');invitationId=item.invitationId;
 step='normal-invited-claim';await memberPage.goto(invitation.actionUrl);await memberPage.getByRole('heading',{name:'Acceptér invitation',exact:true}).waitFor();need(!new URL(memberPage.url()).hash,'invitation token removed from address');await memberPage.getByLabel('Navn',{exact:true}).fill('Synthetic Invited User');await memberPage.getByLabel('Adgangskode',{exact:true}).fill(member.password);const claim=memberPage.waitForResponse(r=>new URL(r.url()).pathname==='/api/invitations/claim'&&r.request().method()==='POST');await memberPage.getByRole('button',{name:'Acceptér invitation',exact:true}).click();const claimed=await claim;need(claimed.ok()&&!claimed.headers()['set-cookie'],'claim cannot mint session');await memberPage.getByText('Invitationen er accepteret.',{exact:true}).waitFor();need((await fetchJson(memberPage,'/api/companies/synthetic/trial-balance')).status===401,'claim not sufficient for business read');
 step='invited-mail-verification-existing-mfa';await memberPage.goto((await mail('verification',member.email)).actionUrl);const mwho=await enroll(memberPage,member);memberId=mwho.user.id;need(memberId!==ownerId,'independent ordinary users');baseline=await get(memberPage,'/api/companies/synthetic/trial-balance');
 return {ownerId,memberId,invitationId,ordinaryInvitationClaim:true,distinctNormalMfaUsers:true};
}
const prefix='/api/companies/synthetic/',hash=b=>createHash('sha256').update(b).digest('hex');
const fixtures=JSON.parse(readFileSync(join(work,'mail-fixtures.json'),'utf8'));
let stable,attachmentId,documentId,service,freshOwner,pendingPage,heldSources;
const read=p=>get(ownerPage,p),post=async(p,b,page=ownerPage)=>{const r=await fetchJson(page,p,b);need(r.status===200&&r.body?.ok===true,`confirmed ${p} HTTP${r.status}`);return r.body;};
async function sources(){const r=await read(prefix+'workspace-inbox?limit=100');need(r.nextCursor===null,'complete owned finite inbox');const full=[];for(const s of r.rows){const r=await read(prefix+'workspace-inbox/'+encodeURIComponent(s.sourceId));full.push(r.source);}return full.sort((a,b)=>a.sourceId.localeCompare(b.sourceId));}
async function snapshot(){return{sources:await sources(),documents:await read(prefix+'documents'),journal:await read(prefix+'journal'),trialBalance:await read(prefix+'trial-balance')};}
async function original(source){const r=await ownerPage.request.get(base+prefix+'workspace-inbox/'+encodeURIComponent(source.sourceId)+'/original');need(r.status()===200,'authorized source original');const bytes=await r.body();need(hash(bytes)===source.sha256&&r.headers()['x-content-sha256']===source.sha256,'exact source SHA response');need(r.headers()['content-type'].split(';')[0]===source.mimeType,'original MIME');return bytes;}
async function originals(){const rows=await sources();for(const source of rows){const observations=source.history.filter(e=>e.event_type==='mail_observed').map(e=>e.payload.observation);need(observations.length>0&&observations.every(o=>o.companySlug==='synthetic'&&o.mailbox==='INBOX'&&o.uidValidity===73&&typeof o.configEpoch==='string'&&o.configEpoch.length>0),'retained canonical company/mailbox/UIDVALIDITY/configepoch');const parentHash=source.mail.role==='attachment'?source.mail.emlSha256:source.sha256;const fixture=fixtures.messages.find(m=>m.sha256===parentHash);need(observations.every(o=>source.mail.role==='refusal'?o.uid===4:fixtures.messages.some(m=>m.sha256===parentHash&&m.uid===o.uid)),'exact fetchedUID bound to retained original or declaredrefusal');const bytes=await original(source);if(source.mail.role==='eml'){need(source.mimeType==='message/rfc822','raw EML never disguised as document');need(fixtures.messages.some(m=>m.sha256===source.sha256&&Buffer.from(m.bytesBase64,'base64').equals(bytes)),'exact privately supplied rawEML');}else if(source.mail.role==='attachment'){const expected=fixtures.attachments.find(a=>a.sha256===source.sha256);need(expected&&Buffer.from(expected.bytesBase64,'base64').equals(bytes),'exact MIME decoded PDF original');const eml=rows.find(s=>s.sourceId===source.mail.emlSourceId);need(eml?.sha256===source.mail.emlSha256,'attachment exact retained parent');}else{need(source.mail.role==='refusal','only explicit refused receipt');const receipt=JSON.parse(bytes);need(receipt.code==='MAIL_INTAKE_OVERSIZED'&&receipt.declaredSize===25*1024*1024+1,'declared oversized receipt not invented EML');}}
 if(documentId){const r=await ownerPage.request.get(base+prefix+`documents/${documentId}/file`);need(r.status()===200,'retained completed document original');need((await r.body()).equals(Buffer.from(fixtures.attachments[0].bytesBase64,'base64')),'exact transferred PDF');}}
async function configure(){step='existing-bilagsmail-human-config';await ownerPage.goto(base+'/companies/synthetic/bilagsmail');await ownerPage.getByRole('heading',{name:'Bilagsmail',exact:true}).waitFor();const form=ownerPage.locator('form').filter({has:ownerPage.getByLabel('Host',{exact:true})});for(const[label,value]of[['Host','127.0.0.1'],['Port',String(fixtures.imapPort)],['Username',fixtures.username],['Password (kun ved oprettelse/skift)',fixtures.password],['Mailbox','INBOX']])await form.getByLabel(label,{exact:true}).fill(value);await form.getByLabel('IMAPS (TLS)',{exact:true}).check();const result=ownerPage.waitForResponse(r=>new URL(r.url()).pathname===prefix+'bilagsmail/imap-config'&&r.request().method()==='POST');await form.getByRole('button',{name:/^(Gem|Opdatér)$/}).click();need((await result).status()===200,'ordinary source configuration persisted');need(await form.getByLabel('Password (kun ved oprettelse/skift)',{exact:true}).inputValue()==='','password cleared from form');const status=await read(prefix+'bilagsmail');need(status.bilagsmail.imapConfigured&&status.bilagsmail.imapStatus.secure===true,'safe configured canonical read');need(!JSON.stringify(status).includes(fixtures.password),'no secret in status');return{configured:true,status};}
async function setup(){step='alias-and-readonly-config';await configure();await ownerPage.getByLabel('Alias',{exact:true}).fill('synthetic-mail-73');await ownerPage.getByRole('button',{name:'Gem alias',exact:true}).click();need((await read(prefix+'bilagsmail')).bilagsmail.mailAlias==='synthetic-mail-73','canonical source alias');const before=await snapshot();const denied=await fetchJson(memberPage,prefix+'bilagsmail/poll',{confirm:true});need(denied.status===403,'Reviewer cannot poll');for(let i=0;i<3;i++)await read(prefix+'bilagsmail');need(isDeepStrictEqual(await snapshot(),before),'status and denied poll have no domain effects');return{statusReads:3,unauthorized:denied};}
async function pollUi(){step='explicit-bilagsmail-one-pass';await ownerPage.goto(base+'/companies/synthetic/bilagsmail');await ownerPage.getByRole('button',{name:'Gennemgå postkasse nu',exact:true}).click();const r=ownerPage.waitForResponse(r=>new URL(r.url()).pathname===prefix+'bilagsmail/poll'&&r.request().method()==='POST');await ownerPage.getByRole('dialog',{name:'Bekræft mailgennemgang',exact:true}).getByRole('button',{name:'Hent mail én gang',exact:true}).click();const result=await r;need(result.status()===200,'real configured manual IMAP poll');const body=await result.json();need(body.ok===true&&Array.isArray(body.poll.sourceIds),'canonical poll receipt');return body;}
async function initial(){await pollUi();const rows=await sources();need(rows.length===5,'one EML+attachment, missing-id EML, noattachment EML, oversized receipt');const attached=rows.filter(s=>s.mail.role==='attachment');need(attached.length===1,'only supported message gets usable attachment');attachmentId=attached[0].sourceId;need(rows.filter(s=>s.mail.role==='eml').length===3,'three exact EML containers');for(const code of ['MAIL_INTAKE_MISSING_MESSAGE_ID','MAIL_INTAKE_NO_ATTACHMENT','MAIL_INTAKE_OVERSIZED'])need(rows.some(s=>s.exception?.code===code),'retained refusal '+code);need((await read(prefix+'documents')).documents.documents.length===0,'mail reception never infers company document metadata');await originals();return{sources:rows};}
async function review(){step='explicit-existing-inbox-owner-review';await ownerPage.goto(base+'/companies/synthetic/workspace-inbox');await ownerPage.getByLabel('Inbox målvirksomhed',{exact:true}).selectOption('synthetic');const article=ownerPage.getByRole('article',{name:`Kilde ${attachmentId}`,exact:true});await article.getByRole('button',{name:'Gennemgå kildeoplysninger',exact:true}).click();const dialog=ownerPage.getByRole('dialog',{name:'Gennemgå mailbilag',exact:true});await dialog.getByLabel('Bilagstype',{exact:true}).selectOption('cash_register_receipt');await dialog.getByLabel('Valuta',{exact:true}).fill('DKK');await dialog.getByLabel('Jeg har gennemgået originalen, målvirksomheden og de indtastede kildeoplysninger.',{exact:true}).check();const saved=ownerPage.waitForResponse(r=>new URL(r.url()).pathname===prefix+`workspace-inbox/${encodeURIComponent(attachmentId)}/review`&&r.request().method()==='POST');await dialog.getByRole('button',{name:'Gem gennemgåede kildeoplysninger',exact:true}).click();need((await saved).status()===200,'review persisted exact selected source and current target');await dialog.getByRole('button',{name:'Luk',exact:true}).click();const reviewed=(await read(prefix+'workspace-inbox/'+encodeURIComponent(attachmentId))).source;need(reviewed.metadataReview?.companySlug==='synthetic'&&reviewed.metadataReview.sourceHash===reviewed.sha256&&reviewed.metadataReview.metadata.documentType==='cash_register_receipt','canonical target-bound review');need(reviewed.assignments.some(a=>a.companySlug==='synthetic'&&a.state==='approved'),'review atomically approved explicit target');await article.getByRole('button',{name:'Fuldfør handoff',exact:true}).click();const complete=ownerPage.waitForResponse(r=>new URL(r.url()).pathname===prefix+`workspace-inbox/${encodeURIComponent(attachmentId)}/complete`&&r.request().method()==='POST');await ownerPage.getByRole('dialog',{name:'Bekræft handoff',exact:true}).getByRole('button',{name:'Bekræft handoff',exact:true}).click();need((await complete).status()===200,'one explicit handoff');const current=(await read(prefix+'workspace-inbox/'+encodeURIComponent(attachmentId))).source;const assignment=current.assignments.find(a=>a.companySlug==='synthetic'&&a.state==='completed');need(assignment?.documentId,'canonical completion document identity');documentId=assignment.documentId;const docs=(await read(prefix+'documents')).documents.documents;need(docs.length===1&&docs[0].id===documentId,'one current unposted document');need((await read(prefix+'journal')).journal.entries.length===0,'no hidden bookkeeping');await originals();stable=await snapshot();return{documentId,canonical:stable};}
async function repeat(){step='explicit-repeat-and-no-double-handoff';const before=await sources();await pollUi();need(isDeepStrictEqual(await sources(),before),'same UID poll creates no new source/history');const result=await post(prefix+`workspace-inbox/${encodeURIComponent(attachmentId)}/complete`,{companySlug:'synthetic',confirm:true});need(result.source.assignments.some(a=>a.documentId===documentId),'explicit known completed handoff replay');need((await read(prefix+'documents')).documents.documents.length===1,'no duplicate documents');stable=await snapshot();return{deduplicated:true};}
async function dedup(){step='same-message-new-server-uid-content-dedup';const before=await sources();await pollUi();const after=await sources();need(isDeepStrictEqual(after.map(s=>s.sourceId),before.map(s=>s.sourceId)),'identical EML and MessageID/PDF under new UID create no new source');need(after.some(s=>s.history.some(e=>e.event_type==='mail_observed'&&e.payload.observation.uid===5)),'new UID observation is real retained evidence');need((await read(prefix+'documents')).documents.documents.length===1,'newUID duplicate never repeats handoff');await originals();stable=await snapshot();return{newUidDuplicateQualified:true};}
async function holdBegin(kind){step='held-'+kind;heldSources=await sources();pendingPage=kind==='revoke'?memberPage:ownerPage;if(kind==='revoke')await post('/api/workspace/members/company',{userId:memberId,action:'grant',companySlug:'synthetic',role:'owner'});await pendingPage.evaluate(path=>{window.mailPending=fetch(path,{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify({confirm:true})}).then(async r=>({status:r.status,body:await r.json()}));},prefix+'bilagsmail/poll');return{started:true};}
async function deleteConfig(){step='delete-config-while-provider-held';const r=await ownerPage.request.delete(base+prefix+'bilagsmail/imap-config',{headers:{origin:base}});need(r.status()===200,'ordinary delete completes during heldfetch');return{deleted:true};}
async function revoke(){step='revoke-while-provider-held';await post('/api/workspace/members/company',{userId:memberId,action:'revoke',companySlug:'synthetic'});return{revoked:true};}
async function holdFinish(){step='held-finalize-current-refusal';const result=await pendingPage.evaluate(()=>window.mailPending);need([403,422].includes(result.status)&&['access_denied','mail_intake_refused'].includes(result.body?.code),'known currentauthority/config refusal after I/O');const rows=await sources();need(isDeepStrictEqual(rows,heldSources),'failed finalize preserves all retained source/history');need((await read(prefix+'documents')).documents.documents.length===1,'failed finalize never creates document');need(!rows.some(s=>fixtures.messages.filter(m=>m.held).some(m=>m.sha256===s.sha256)),'no held mail/source persisted after currentrefusal');return{result};}
async function createService(){step='ordinary-web-real-service-lifecycle';await ownerPage.goto(base+'/adgang');const panel=ownerPage.getByRole('region',{name:'Servicekonti',exact:true});await panel.getByLabel('Navn på servicekonto',{exact:true}).fill('Synthetic Mail Executor');const receipt=ownerPage.waitForResponse(r=>new URL(r.url()).pathname==='/api/workspace/service-principals'&&r.request().method()==='POST');await panel.getByRole('button',{name:'Opret servicekonto',exact:true}).click();await ownerPage.getByRole('dialog',{name:'Bekræft servicehandling',exact:true}).getByRole('button',{name:'Gennemfør servicehandling',exact:true}).click();const r=await receipt;need(r.status()===201,'ordinary real service creation');service=await r.json();need(service.secret&&service.serviceAccountId&&service.credentialId,'one time real service receipt');await panel.getByRole('button',{name:'Skjul og glem nøglen',exact:true}).click();const principal=panel.getByRole('region',{name:'Synthetic Mail Executor',exact:true});await principal.getByRole('button',{name:'Aktivér workspace-adgang som medlem',exact:true}).click();await ownerPage.getByRole('dialog',{name:'Bekræft servicehandling',exact:true}).getByRole('button',{name:'Gennemfør servicehandling',exact:true}).click();await ownerPage.getByRole('dialog',{name:'Bekræft servicehandling',exact:true}).waitFor({state:'hidden'});await ownerPage.getByRole('combobox',{name:'Bruger',exact:true}).selectOption(service.serviceAccountId);await ownerPage.getByRole('combobox',{name:'Virksomhedsrolle',exact:true}).selectOption('bookkeeper');const granted=ownerPage.waitForResponse(r=>new URL(r.url()).pathname==='/api/workspace/members/company'&&r.request().method()==='POST');await ownerPage.getByRole('button',{name:'Gem virksomhedsrolle',exact:true}).click();await ownerPage.getByRole('dialog',{name:'Bekræft adgangsændring',exact:true}).getByRole('button',{name:'Gennemfør ændring',exact:true}).click();need((await granted).status()===200,'ordinary current service company grant');writeFileSync(join(work,'mail-worker.secret'),service.secret,{mode:0o600,flag:'wx'});return{serviceAccountId:service.serviceAccountId,credentialId:service.credentialId,oneTimeSecretPrivateFile:true};}
async function background(){step='fresh-mfa-service-mailbox-consent';await ownerPage.context().clearCookies();await login(ownerPage,owner);await configure();const configured=await read(prefix+'bilagsmail');need(configured.bilagsmail.polling?.enabled===true,'current operator service explicitly bound by human configuration');const deadline=Date.now()+75000;let rows;do{rows=await sources();if(rows.some(s=>s.sha256===fixtures.messages.find(m=>m.kind==='background').sha256))break;await new Promise(r=>setTimeout(r,500));}while(Date.now()<deadline);const eml=rows.find(s=>s.sha256===fixtures.messages.find(m=>m.kind==='background').sha256);need(eml,'background worker received actual next message without manual poll');need(eml.createdBy===`service_account:${service.serviceAccountId}`&&JSON.stringify(eml.history).includes(`service_account:${service.serviceAccountId}`),'retention attributed to real service independent from grantor');need((await read(prefix+'documents')).documents.documents.length===1,'background never infers metadata/handoff');await originals();stable=await snapshot();return{serviceAccountId:service.serviceAccountId,source:eml};}
async function reopen(){step='fresh-owner-mfa-source-restart';await ownerPage.context().clearCookies();await login(ownerPage,owner);need(isDeepStrictEqual(await snapshot(),stable),'source restart exact evidence');await pollUi();need(isDeepStrictEqual(await snapshot(),stable),'real repeated poll after restart creates no duplicate source/history/document');await originals();return{freshMfaRestart:true,explicitRestartPollDeduplicated:true};}
async function freeze(){step='stable-source-snapshot';stable=await snapshot();await originals();await ownerPage.screenshot({path:process.env.RM_MAIL_INTAKE_SCREENSHOT,fullPage:true});return{canonical:stable};}
async function archive(name){step='signed-mail-'+name;const r=await ownerPage.request.post(base+'/api/workspace/backup',{headers:{origin:base},data:{confirm:true}});const ct=r.headers()['content-type'];let failure;if(r.status()!==200||ct!=='application/vnd.rentemester.native-backup+json'){try{failure=await r.json();}catch{}detail={status:r.status(),contentType:ct,code:failure?.code,errors:failure?.errors};throw Error('signed mail backup refused');}const bytes=await r.body();need(bytes.length&&bytes.length<=96*1024*1024,'bounded signedartifact');writeFileSync(join(work,`mail-intake-${name}.native-backup.json`),bytes,{mode:0o600,flag:'wx'});need(isDeepStrictEqual(await snapshot(),stable),'backup does not mutate retained businessfacts');return{sha256:hash(bytes),bytes:bytes.length};}
async function restored(){step='fresh-owner-normal-mfa-source-free';ownerPage=await newPage();freshOwner={email:'restored@example.invalid',password:'synthetic-restored-password-789',codes:[]};await ownerPage.goto((await mail('verification',freshOwner.email)).actionUrl);need((await enroll(ownerPage,freshOwner)).user.id!==ownerId,'no historical user authority adoption');need(isDeepStrictEqual(await snapshot(),stable),'full sources/history/review/dedup/document/books survive freshrestore');const status=await read(prefix+'bilagsmail');need(status.bilagsmail.imapConfigured===false&&!status.bilagsmail.polling.enabled,'no restored credentials or background authority');await originals();return{sourceFreeRestoreQualified:true,credentialsAbsent:true};}
async function restoredReopen(){step='fresh-mfa-target-reopen';await ownerPage.context().clearCookies();await login(ownerPage,freshOwner);need(isDeepStrictEqual(await snapshot(),stable),'restored reopen exact source/businesshistory');await originals();return{restoredTargetReopenQualified:true};}
for await(const line of createInterface({input:process.stdin,crlfDelay:Infinity})){try{const a=JSON.parse(line);let value;if(a==='authenticate')value=await authenticate();else if(a==='setup')value=await setup();else if(a==='initial')value=await initial();else if(a==='review')value=await review();else if(a==='repeat')value=await repeat();else if(a==='dedup')value=await dedup();else if(a==='hold-delete')value=await holdBegin('delete');else if(a==='hold-revoke')value=await holdBegin('revoke');else if(a==='delete-config')value=await deleteConfig();else if(a==='revoke')value=await revoke();else if(a==='hold-finish')value=await holdFinish();else if(a==='configure')value=await configure();else if(a==='prime')value=await pollUi();else if(a==='service')value=await createService();else if(a==='background')value=await background();else if(a==='freeze')value=await freeze();else if(a==='reopen')value=await reopen();else if(a.startsWith('archive-'))value=await archive(a.slice(8));else if(a==='restored')value=await restored();else if(a==='restored-reopen')value=await restoredReopen();else if(a==='finish'){await browser?.close();emit({ok:true,value:{finished:true}});break;}else throw Error('unknown phase');emit({ok:true,value});}catch(error){emit({ok:false,step,detail,errorType:error?.name});await browser?.close().catch(()=>{});process.exitCode=1;break;}}

'''

class Mailbox(socketserver.ThreadingTCPServer):
    allow_reuse_address = False
    daemon_threads = True

    def __init__(self, cert, key, username, password, messages):
        self.tls = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        self.tls.load_cert_chain(cert, key)
        self.username, self.password = username, password
        self.messages = messages
        self.active = {1, 2, 3, 4}
        self.lock = threading.Lock()
        self.trace = []
        self.held, self.release = threading.Event(), threading.Event()
        self.hold_uid = None
        super().__init__(('127.0.0.1', 0), MailHandler)

    def change(self, uid, active=True, hold=False):
        with self.lock:
            if active:
                self.active.add(uid)
            else:
                self.active.discard(uid)
            self.hold_uid = uid if hold else None
            self.held.clear()
            self.release.clear()

    def count(self):
        with self.lock:
            return len(self.trace)

    def get_request(self):
        sock, address = super().get_request()
        sock.settimeout(35)
        try:
            return self.tls.wrap_socket(sock, server_side=True), address
        except BaseException:
            sock.close()
            raise

    def handle_error(self, *_args):
        pass  # No provider/body/credential diagnostics in logs.


class MailHandler(socketserver.StreamRequestHandler):
    def handle(self):
        self.wfile.write(b'* OK synthetic IMAP ready\r\n')
        authenticated = False
        try:
            while True:
                line = self.rfile.readline(16385)
                if not line:
                    return
                if len(line) > 16384 or not line.endswith(b'\r\n'):
                    raise ValueError('bounded IMAP command required')
                tag, cmd = line[:-2].decode('utf8').split(' ', 1)
                if not re.fullmatch('[A-Za-z0-9]+', tag):
                    raise ValueError('invalid command tag')
                with self.server.lock:
                    self.server.trace.append({'command': 'LOGIN' if cmd.startswith('LOGIN ') else cmd})
                if cmd == 'CAPABILITY':
                    self.wfile.write(b'* CAPABILITY IMAP4rev1\r\n')
                elif cmd.startswith('LOGIN '):
                    expected = f'LOGIN "{self.server.username}" "{self.server.password}"'
                    if cmd != expected:
                        self.wfile.write((tag + ' NO authentication refused\r\n').encode())
                        continue
                    authenticated = True
                elif not authenticated:
                    self.wfile.write((tag + ' NO authenticate first\r\n').encode())
                    continue
                elif cmd.startswith('SELECT '):
                    if cmd != 'SELECT "INBOX"' and cmd != 'SELECT INBOX':
                        raise ValueError('explicit fixture mailbox required')
                    self.wfile.write(b'* FLAGS (\\Seen)\r\n* OK [UIDVALIDITY 73] retained\r\n')
                elif cmd.startswith('UID SEARCH '):
                    match = re.fullmatch(r'UID SEARCH UID ([0-9]+):\*', cmd)
                    if not match:
                        raise ValueError('explicit UID range required')
                    with self.server.lock:
                        uids = sorted(u for u in self.server.active if u >= int(match.group(1)))
                    self.wfile.write(('* SEARCH ' + ' '.join(map(str, uids)) + '\r\n').encode())
                elif cmd.startswith('UID FETCH '):
                    match = re.fullmatch(r'UID FETCH ([0-9]+) \(UID BODY\.PEEK\[\]\)', cmd)
                    if not match:
                        raise ValueError('only exact nonmarking fetch supported')
                    uid = int(match.group(1))
                    with self.server.lock:
                        if uid not in self.server.active:
                            raise ValueError('UID not in current synthetic mailbox')
                        hold = uid == self.server.hold_uid
                    if hold:
                        self.server.held.set()
                        if not self.server.release.wait(25):
                            return
                    raw = self.server.messages.get(uid)
                    size = 25*1024*1024+1 if uid == 4 else len(raw)
                    self.wfile.write(f'* 1 FETCH (UID {uid} BODY[] {{{size}}}\r\n'.encode())
                    self.wfile.flush()
                    if uid == 4:
                        self.rfile.read(1)  # Runtime closes before allocating/downloading oversized EML.
                        return
                    self.wfile.write(raw[:19])
                    self.wfile.flush()
                    self.wfile.write(raw[19:])
                    self.wfile.write(b')\r\n')
                elif cmd == 'LOGOUT':
                    self.wfile.write(b'* BYE done\r\n' + (tag + ' OK logout\r\n').encode())
                    return
                else:
                    raise ValueError('mutating or unsupported IMAP command')
                self.wfile.write((tag + ' OK completed\r\n').encode())
                self.wfile.flush()
        except (OSError, ValueError, ssl.SSLError):
            return


def pdf(label):
    body = f'BT /F1 12 Tf 40 700 Td ({label}) Tj ET'.encode()
    objects = [b'<< /Type /Catalog /Pages 2 0 R >>', b'<< /Type /Pages /Kids [3 0 R] /Count 1 >>', b'<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>', b'<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>', b'<< /Length '+str(len(body)).encode()+b' >>\nstream\n'+body+b'\nendstream']
    out = b'%PDF-1.4\n'; offsets = []
    for index, obj in enumerate(objects, 1):
        offsets.append(len(out)); out += str(index).encode()+b' 0 obj\n'+obj+b'\nendobj\n'
    xref = len(out)
    out += b'xref\n0 6\n0000000000 65535 f \n'+b''.join(f'{n:010d} 00000 n \n'.encode() for n in offsets)
    return out+b'trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n'+str(xref).encode()+b'\n%%EOF\n'


def eml(message_id, attachment=None, label='Synthetic receipt'):
    from email.message import EmailMessage
    from email.policy import SMTP
    mail = EmailMessage(policy=SMTP)
    mail['From'], mail['To'] = 'synthetic-sender@example.invalid', 'synthetic-mail@example.invalid'
    mail['Subject'], mail['Date'] = label, 'Tue, 20 Oct 2026 12:00:00 +0200'
    if message_id:
        mail['Message-ID'] = message_id
    mail.set_content('Synthetic retained source only. No inferred accounting metadata.')
    if attachment:
        mail.add_attachment(attachment, maintype='application', subtype='pdf', filename='synthetic-mail.pdf')
        mail.set_boundary('synthetic-mail-73-boundary')
    return mail.as_bytes()


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
    mailbox = None
    worker_enabled = False
    service_secret = None
    mailbox_password = secrets.token_urlsafe(24)
    mailbox_username = 'synthetic-mail-user'
    attachments = [pdf('Synthetic mail receipt 12.34 DKK'),pdf('Synthetic background receipt 23.45 DKK')]
    raw_messages = {1:eml('<ordinary-73@example.invalid>',attachments[0]),2:eml(None,attachments[0]),3:eml('<no-attachment-73@example.invalid>'),8:eml('<background-73@example.invalid>',attachments[1]),6:eml('<held-delete-73@example.invalid>',attachments[0]),7:eml('<held-revoke-73@example.invalid>',attachments[0])}
    bearer = secrets.token_urlsafe(32)
    backend_port, mail_port = free_port(), free_port()
    if backend_port == mail_port:
        raise RuntimeError('Distinct owned ports required')
    origin = f'http://127.0.0.1:{backend_port}'
    report = {'status':'running','sourceRevision':subprocess.check_output(['git','rev-parse','HEAD'],cwd=REPO,text=True).strip(),'runnerSha256':digest(Path(__file__)),'browserHelperSha256':hashlib.sha256(BROWSER.encode()).hexdigest(),'canonicalOrigin':origin,'noOfflineIdentitySeeds':True,'ordinaryMacOsUser':True,'ownedProfile':str(work),'externalMailDelivered':False,'workflow':'configured_imap_mail_intake','installedNativeQualified':False,'mailIntakeActualQualified':False,'sourceFreeRestoreQualified':False,'screenshot':str(args.evidence.with_suffix('.png')),'actions':[],'results':{}}
    raw_messages[5] = raw_messages[1]
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
                argv += ['--restore-backup', str(work / 'mail-intake-source.native-backup.json'), '--trusted-backup-key', str(work / 'trusted.pub'), '--confirm-target', str(root), '--source-unavailable']
            if worker_enabled:
                if not service_secret:
                    raise RuntimeError('Real one-time service key required before background worker')
                argv += ['--mail-worker','--mail-service-key-stdin']
            log = (work / 'server.log').open('ab')
            child = subprocess.Popen(argv, cwd=REPO, stdin=subprocess.PIPE if worker_enabled else subprocess.DEVNULL, stdout=log, stderr=log)
            if worker_enabled:
                child.stdin.write(service_secret.encode()+b'\n');child.stdin.close()
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
            with gateway_lock:
                report['privateGatewayErrors'] = list(errors)
            report['actions'].append({'action': action, 'status': 'complete'})
            report['results'][action] = value['value']
            save()
            return value['value']

        def inspect_snapshot(name, trusted):
            progress('operator:independent-signed-mail-intake-' + name)
            path = work / ('mail-intake-' + name + '.native-backup.json')
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
                if any(secret.encode() in raw for secret in [bearer, mailbox_password, service_secret or '', config['authMail']['idempotencySecret'], config['cookieKey'], config['mfaKey']] if secret):
                    raise RuntimeError('Private credential present in signed inventory')
            if any(p.endswith('/config/imap.json') for p in entries):
                raise RuntimeError('Company IMAP credentials must not be backed up')
            raw = entries.get('synthetic/data/ledger.sqlite')
            if not raw or len(raw) > 32*1024*1024:
                raise RuntimeError('Bounded complete signed company database missing')
            copy = work / ('readonly-mail-' + name + '.sqlite')
            result = {}
            def cell(value):
                return {'bytesBase64': base64.b64encode(value).decode()} if isinstance(value, bytes) else value
            try:
                with copy.open('xb') as f:
                    f.write(raw)
                db = sqlite3.connect(copy.as_uri()+'?mode=ro', uri=True)
                try:
                    db.execute('PRAGMA query_only=ON')
                    for table in ['documents','mail_intake_messages','journal_entries','journal_lines','audit_log']:
                        rows = db.execute('SELECT * FROM '+table+' ORDER BY id')
                        result[table] = [{k:cell(v) for k,v in zip([c[0] for c in rows.description], row)} for row in rows.fetchall()]
                finally:
                    db.close()
            finally:
                copy.unlink(missing_ok=True)
            if len(result['documents']) != 1 or len(result['mail_intake_messages']) != 1 or result['journal_entries'] or result['journal_lines']:
                raise RuntimeError('Exactly one unposted reviewed document/mail receipt and zero accounting effects required')
            row = result['mail_intake_messages'][0]
            if row['document_id'] != result['documents'][0]['id'] or row['attachment_sha256'] != hashlib.sha256(attachments[0]).hexdigest():
                raise RuntimeError('Mail receipt differs from exact transferred source/document')
            inbox = entries.get('control/workspace-inbox.json')
            if not inbox or json.loads(inbox).get('format') != 'rentemester-native-workspace-inbox/2':
                raise RuntimeError('Complete current typed mail/inbox business section missing')
            result['inboxSection'] = json.loads(inbox)
            wanted = 'synthetic/documents/originals/'+hashlib.sha256(attachments[0]).hexdigest()+'.pdf'
            if entries.get(wanted) != attachments[0]:
                raise RuntimeError('Signed immutable company original differs from retained MIME attachment')
            report.setdefault('snapshots', {})[name] = {'sha256':receipt['sha256'],'trustedPublicKeySha256':digest(trusted),'privateCredentialsExcluded':True,'tables':result}
            save()
            return result

        def preserved(original, newer):
            for table in ['documents','mail_intake_messages','journal_entries','journal_lines','inboxSection']:
                if original[table] != newer[table]:
                    raise RuntimeError('Exact restored mail evidence differs: '+table)
            retained = {row['id']:row for row in newer['audit_log']}
            if any(retained.get(row['id']) != row for row in original['audit_log']):
                raise RuntimeError('Full original historical audit/provenance not retained')

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
                ['openssl', 'req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', str(work / 'ca.key'), '-out', str(ca), '-days', '1', '-subj', '/CN=Synthetic Mail Intake Auth CA', '-addext', 'basicConstraints=critical,CA:TRUE', '-addext', 'keyUsage=critical,keyCertSign,cRLSign'],
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
            mailbox = Mailbox(cert,private,mailbox_username,mailbox_password,raw_messages)
            threading.Thread(target=mailbox.serve_forever,daemon=True).start();servers.append(mailbox)
            fixture={'imapPort':mailbox.server_address[1],'username':mailbox_username,'password':mailbox_password,'messages':[{'uid':uid,'kind':'background' if uid==8 else 'ordinary','held':uid in [6,7],'sha256':hashlib.sha256(raw).hexdigest(),'bytesBase64':base64.b64encode(raw).decode()} for uid,raw in raw_messages.items()],'attachments':[{'sha256':hashlib.sha256(raw).hexdigest(),'bytesBase64':base64.b64encode(raw).decode()} for raw in attachments]}
            (work/'mail-fixtures.json').write_text(json.dumps(fixture))
            subprocess.run(['/opt/homebrew/bin/bun', '-e', "const c=require('node:crypto'),f=require('node:fs'),k=c.generateKeyPairSync('ed25519'),s=k.privateKey.export({format:'der',type:'pkcs8'}).subarray(-32),p=k.publicKey.export({format:'der',type:'spki'}).subarray(-32);f.writeFileSync(process.argv[1],Buffer.concat([Buffer.from('3051020101300506032b657004220420','hex'),s,Buffer.from('812100','hex'),p]),{mode:0o600});f.writeFileSync(process.argv[2],p,{mode:0o600});", str(work / 'signing.der'), str(work / 'trusted.pub')], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            key = lambda: base64.urlsafe_b64encode(secrets.token_bytes(32)).decode().rstrip('=')
            config = {'runtimeProfile': 'local-user-macos', 'root': str(work / 'workspace'), 'brokerUid': os.getuid(), 'workspace': 'synthetic-native-mail-intake', 'companySlug': 'synthetic', 'companyName': 'Synthetic Mail Intake Company', 'cookieKey': key(), 'mfaKey': key(), 'operatorActors': ['system:synthetic-installer'], 'accounts': [], 'firstOwner': {'actor': 'system:synthetic-installer', 'email': 'owner@example.invalid', 'name': 'Synthetic Owner', 'password': 'synthetic-owner-password-123'}, 'backupSigningKeyFile': str(work / 'signing.der'), 'authMail': {'endpoint': f'https://127.0.0.1:{mail_port}/mail', 'bearerToken': bearer, 'from': 'auth@example.invalid', 'idempotencySecret': secrets.token_urlsafe(32), 'trustedCaFile': str(ca)}}
            config['imapTransport']={'trustedCaPem':ca.read_text(),'timeoutMs':30000}
            write_config()
            start(create=True)
            script = work / 'browser.mjs'
            script.write_text(BROWSER)
            controller = subprocess.Popen(['/opt/homebrew/bin/bun', str(script)], cwd=REPO, env={**os.environ, 'RM_MAIL_INTAKE_BASE': origin, 'RM_MAIL_INTAKE_WORK': str(work), 'RM_MAIL_INTAKE_SCREENSHOT': str(args.evidence.with_suffix('.png')), 'RM_MAIL_INTAKE_PLAYWRIGHT': str((REPO / 'node_modules/@playwright/test/index.mjs').resolve(strict=True))}, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, bufsize=1, start_new_session=True)
            report['browserControllerPid'] = controller.pid
            phase('authenticate')
            calls = mailbox.count()
            phase('setup')
            if mailbox.count() != calls:
                raise RuntimeError('Config/status/denied poll unexpectedly contacted IMAP')
            private_cfg = Path(config['root'])/'synthetic/config/imap.json'
            info = private_cfg.lstat()
            if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid() or info.st_nlink != 1 or info.st_mode & 0o777 != 0o600:
                raise RuntimeError('Mailbox credentials not private regular0600')
            phase('initial')
            phase('review')
            phase('repeat')
            mailbox.change(5)
            phase('dedup')
            mailbox.change(6, hold=True)
            phase('hold-delete')
            if not mailbox.held.wait(5):
                raise RuntimeError('Real owned IMAP FETCH not held')
            phase('delete-config', 3)
            mailbox.release.set()
            phase('hold-finish')
            mailbox.change(6, active=False)
            phase('configure')
            # A new explicit configuration epoch has its own cursor. Consume the
            # already-retained initial batch/refusal before the held new UID.
            phase('prime')
            mailbox.change(7, hold=True)
            phase('hold-revoke')
            if not mailbox.held.wait(5):
                raise RuntimeError('Invited currentadmin real FETCH not held')
            phase('revoke', 3)
            mailbox.release.set()
            phase('hold-finish')
            mailbox.change(7, active=False)
            phase('service')
            secret_path = work/'mail-worker.secret'
            secret_info = secret_path.lstat()
            if not stat.S_ISREG(secret_info.st_mode) or secret_info.st_uid != os.getuid() or secret_info.st_nlink != 1 or secret_info.st_mode & 0o777 != 0o600:
                raise RuntimeError('One-time service credential private file changed')
            service_secret = secret_path.read_text()
            secret_path.unlink()
            if not re.fullmatch(r'rms_[A-Za-z0-9_-]{64}', service_secret):
                raise RuntimeError('One exact real service key receipt required')
            stop()
            config.pop('firstOwner')
            write_config()
            mailbox.change(8)
            worker_enabled = True
            start()
            phase('background', 110)
            worker_enabled = False
            stop()
            # Stop the worker before immutable comparison/backup; the retained
            # current service proof is historical, never forged by this helper.
            start()
            phase('freeze')
            phase('archive-source')
            original = inspect_snapshot('source', work/'trusted.pub')
            stop()
            write_config()
            calls = mailbox.count()
            start()
            phase('reopen')
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
            calls = mailbox.count()
            start(restore=True)
            phase('restored')
            if mailbox.count() != calls:
                raise RuntimeError('Fresh restore imported polling credentials/authority')
            report['sourceFreeRestoreQualified'] = True
            phase('archive-restored')
            preserved(original, inspect_snapshot('restored',work/'target-trusted.pub'))
            stop()
            config.pop('firstOwner');config.pop('restoreActor');write_config();start()
            phase('restored-reopen')
            if mailbox.count() != calls:
                raise RuntimeError('Restored target reopen performed unconsented IMAP I/O')
            phase('archive-reopened')
            if inspect_snapshot('reopened',work/'target-trusted.pub') != report['snapshots']['restored']['tables']:
                raise RuntimeError('Fresh target reopen changed retained full mail evidence')
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
            with mailbox.lock:
                report['imapProtocol']={'verifiedTls':True,'trustedCaSha256':digest(ca),'commands':list(mailbox.trace),'noMarkSeenDeleteRetry':True}
            with gateway_lock:
                if errors:
                    raise RuntimeError('Private auth gateway protocol error')
                report['gateway']={'verifiedHttps':True,'acceptedMessages':len(gateway_rows),'requests':list(gateway_requests)}
            report.update({'status':'passed','currentAction':'complete','mailIntakeActualQualified':True,'manualAndServicePollingQualified':True,'snapshotCoverage':'four-independently-verified-mail-source-history-review-dedup-document-archives-fresh-owner-restore-and-reopen'})
            save()
            print('PASS: normal MFA Bilagsmail config, realTLS manual/service receive, explicit metadata/handoff, refusals/dedup and signed source-free recovery; '+str(args.evidence))
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
            if mailbox is not None:
                mailbox.release.set()
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


def self_test():
    """Only our protocol fixture: no Rentemester process, credentials or external I/O."""
    os.umask(0o077)
    with tempfile.TemporaryDirectory(prefix='rm73-imap-protocol-') as directory:
        work=Path(directory);ca=work/'ca.pem';cert=work/'server.pem';key=work/'server.key'
        (work/'ext').write_text('basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\nsubjectAltName=IP:127.0.0.1\n')
        commands=[['openssl','req','-x509','-newkey','rsa:2048','-nodes','-keyout',str(work/'ca.key'),'-out',str(ca),'-days','1','-subj','/CN=Synthetic IMAP fixture CA','-addext','basicConstraints=critical,CA:TRUE','-addext','keyUsage=critical,keyCertSign,cRLSign'],['openssl','req','-new','-newkey','rsa:2048','-nodes','-keyout',str(key),'-out',str(work/'csr'),'-subj','/CN=localhost'],['openssl','x509','-req','-in',str(work/'csr'),'-CA',str(ca),'-CAkey',str(work/'ca.key'),'-CAcreateserial','-out',str(cert),'-days','1','-extfile',str(work/'ext')]]
        for argv in commands:
            subprocess.run(argv,check=True,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=30)
        attachment=pdf('Synthetic fixture protocol')
        raw=eml('<protocol@example.invalid>',attachment)
        mailbox=Mailbox(cert,key,'synthetic-user','synthetic-secret',{1:raw})
        with mailbox.lock:
            mailbox.active={1,4}
        threading.Thread(target=mailbox.serve_forever,daemon=True).start()
        try:
            context=ssl.create_default_context(cafile=str(ca))
            with socket.create_connection(mailbox.server_address,timeout=3) as tcp:
                with context.wrap_socket(tcp,server_hostname='127.0.0.1') as conn:
                    f=conn.makefile('rwb',buffering=0)
                    assert f.readline().startswith(b'* OK ')
                    f.write(b'A1 LOGIN "synthetic-user" "synthetic-secret"\r\n');assert f.readline()==b'A1 OK completed\r\n'
                    f.write(b'A2 SELECT "INBOX"\r\n');assert f.readline().startswith(b'* FLAGS');assert b'UIDVALIDITY 73' in f.readline();assert f.readline()==b'A2 OK completed\r\n'
                    f.write(b'A3 UID SEARCH UID 1:*\r\n');assert f.readline()==b'* SEARCH 1 4\r\n';assert f.readline()==b'A3 OK completed\r\n'
                    f.write(b'A4 UID FETCH 1 (UID BODY.PEEK[])\r\n');assert f.readline()==f'* 1 FETCH (UID 1 BODY[] {{{len(raw)}}}\r\n'.encode();received=bytearray()
                    while len(received)<len(raw):
                        received.extend(f.read(len(raw)-len(received)))
                    assert received==raw and f.readline()==b')\r\n' and f.readline()==b'A4 OK completed\r\n'
                    f.write(b'A5 UID FETCH 4 (UID BODY.PEEK[])\r\n');assert f.readline()==b'* 1 FETCH (UID 4 BODY[] {26214401}\r\n'
            with mailbox.lock:
                assert len(mailbox.trace)==5 and mailbox.trace[0]=={'command':'LOGIN'}
                assert all('synthetic-secret' not in row['command'] and not any(v in row['command'] for v in ['STORE','EXPUNGE','DELETE']) for row in mailbox.trace)
            print('PASS: private TLS IMAP fixture protocol, UIDVALIDITY, exact EML/PDF literal and explicit oversized header; no product run')
        finally:
            mailbox.release.set();mailbox.shutdown();mailbox.server_close()


if __name__ == '__main__':
    import sys
    if sys.argv[1:]==['--self-test']:
        self_test()
    else:
        main()
