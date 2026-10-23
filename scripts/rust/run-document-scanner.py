#!/usr/bin/env python3
"""Owned normal-MFA required document scanner qualification using supplied artifacts.

Verified private TLS, exact original hashes, current revocation during held scan,
existing web/CLI/MCP upload, signed source-free recovery without scanner credentials.
Only ordinary ingestion is scanned; existing inbox completion remains scanner-off.
No auth seeds, real mail/provider or automatic retries. Standalone original
period/exception proofs explicitly seed bounded synthetic operational facts only. The default
scanner proof makes no financial writes; --service-purchase explicitly qualifies
one synthetic payable recognition and its bank-linked accounting payment.
--backup-lock instead proves an enforced overdue-backup financial gate, actual
UI backup, current-file integrity, one accepted posting and normal MFA restart.
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
import sys
import threading
import time
import uuid
from urllib.parse import urlsplit, parse_qs
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
PREFIX = 'Rentemester-Synthetic-Document-Scanner-'
BROWSER = r'''






import {createHmac,createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {createInterface} from 'node:readline';
import {readFileSync,readdirSync,writeFileSync,unlinkSync} from 'node:fs';
import {join} from 'node:path';
import {isDeepStrictEqual} from 'node:util';
const {chromium}=await import(process.env.RM_SCANNER_PLAYWRIGHT);
const base=process.env.RM_SCANNER_BASE,work=process.env.RM_SCANNER_WORK;
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
async function newPage(){const context=await browser.newContext();const page=await context.newPage();page.setDefaultTimeout(45000);diagnosticPage=page;page.on('request',r=>{if(new URL(r.url()).pathname==='/api/auth/request-password-reset')trace.push({kind:'request',path:'/api/auth/request-password-reset',method:r.method()});});page.on('requestfailed',r=>{if(new URL(r.url()).pathname==='/api/auth/request-password-reset')trace.push({kind:'requestfailed',path:'/api/auth/request-password-reset',code:r.failure()?.errorText?.match(/net::[A-Z_]+/)?.[0]});});page.on('response',r=>{const path=new URL(r.url()).pathname;if(path.startsWith('/api/auth/')){trace.push({path:path.startsWith('/api/auth/reset-password/')?'/api/auth/reset-password/:token':path,status:r.status()});if(trace.length>24)trace.shift();}});page.on('response',r=>{const u=new URL(r.url());if(/^\/api\/companies\/[a-z0-9-]+\/(documents|journal|trial-balance|income-statement|balance|overview|archives|archive-import)(\/|$)/.test(u.pathname)){trace.push({step,path:u.pathname,status:r.status(),method:r.request().method(),year:u.searchParams.get('year'),source:u.searchParams.get('source'),format:u.searchParams.get('format')});if(trace.length>32)trace.shift();}});return page;}
async function authenticate(){
 browser=await chromium.launch({headless:true});ownerPage=await newPage();memberPage=await newPage();
 step='owner-mail-verification';await ownerPage.goto((await mail('verification',owner.email)).actionUrl);const who=await enroll(ownerPage,owner);ownerId=who.user.id;
 step='ordinary-web-invitation';await ownerPage.goto(base+'/adgang');await ownerPage.getByRole('heading',{name:'Brugere og adgang',exact:true}).waitFor();await ownerPage.getByLabel('E-mail',{exact:true}).fill(member.email);await ownerPage.getByLabel(/^Rolle/).selectOption('reviewer');await ownerPage.getByRole('button',{name:'Send invitation',exact:true}).click();await ownerPage.getByText('Invitationen er sendt.',{exact:true}).waitFor();const invitation=await mail('workspace-invitation',member.email);const listed=await get(ownerPage,'/api/workspace/invitations');const item=listed.invitations.find(i=>i.email===member.email);need(item?.status==='delivery_confirmed','canonical invitation delivery');invitationId=item.invitationId;
 step='normal-invited-claim';await memberPage.goto(invitation.actionUrl);await memberPage.getByRole('heading',{name:'Acceptér invitation',exact:true}).waitFor();need(!new URL(memberPage.url()).hash,'invitation token removed from address');await memberPage.getByLabel('Navn',{exact:true}).fill('Synthetic Invited User');await memberPage.getByLabel('Adgangskode',{exact:true}).fill(member.password);const claim=memberPage.waitForResponse(r=>new URL(r.url()).pathname==='/api/invitations/claim'&&r.request().method()==='POST');await memberPage.getByRole('button',{name:'Acceptér invitation',exact:true}).click();const claimed=await claim;need(claimed.ok()&&!claimed.headers()['set-cookie'],'claim cannot mint session');await memberPage.getByText('Invitationen er accepteret.',{exact:true}).waitFor();need((await fetchJson(memberPage,'/api/companies/synthetic/trial-balance')).status===401,'claim not sufficient for business read');
 step='invited-mail-verification-existing-mfa';await memberPage.goto((await mail('verification',member.email)).actionUrl);const mwho=await enroll(memberPage,member);memberId=mwho.user.id;need(memberId!==ownerId,'independent ordinary users');baseline=await get(memberPage,'/api/companies/synthetic/trial-balance');
 return {ownerId,memberId,invitationId,ordinaryInvitationClaim:true,distinctNormalMfaUsers:true};
}
const prefix='/api/companies/synthetic/',hash=b=>createHash('sha256').update(b).digest('hex');
const fixtures=Object.fromEntries(['historical','clean','reject','timeout','missing-id','invalid-field','malformed','unauthorized','held','cli','mcp'].map(kind=>{const bytes=Buffer.from(`Synthetic required scanner ${kind}\nOwned harmless original, no accounting instruction.\n`);return[kind,{fileName:`synthetic-scanner-${kind}.txt`,bytes,sha256:hash(bytes)}];}));
let stable,documents=[],sourceBooks,freshOwner;
const read=path=>get(ownerPage,path);
async function call(input,page=ownerPage){const r=await fetchJson(page,prefix+'documents/ingest',input);return{status:r.status,body:r.body};}
function body(kind){const f=fixtures[kind];return{fileName:f.fileName,fileBase64:f.bytes.toString('base64'),metadata:{source:'Synthetic scanner acceptance '+kind,documentType:'cash_register_receipt'},confirm:true};}
async function post(path,input){const r=await fetchJson(ownerPage,path,input);need(r.status===200&&r.body?.ok===true,'normal confirmed public mutation '+path+' HTTP'+r.status);return r.body;}
async function books(){return{journal:await read(prefix+'journal'),trialBalance:await read(prefix+'trial-balance'),balance:await read(prefix+'balance')};}
async function snapshot(){return{documents:await read(prefix+'documents'),books:await books()};}
async function unchanged(before){need(isDeepStrictEqual(await snapshot(),before),'refusal leaves all canonical documents/ledger/UUID/figures unchanged');}
async function originals(){for(const d of documents){const f=fixtures[d.kind],r=await ownerPage.request.get(base+prefix+`documents/${d.id}/file`);need(r.status()===200&&r.headers()['content-type'].split(';')[0]==='text/plain'&&hash(await r.body())===f.sha256,'authorized exact original '+d.kind);}}
async function uiUpload(kind){const f=fixtures[kind];diagnosticPage=ownerPage;step='ui-'+kind+'-documents-route';await ownerPage.goto(base+'/companies/synthetic/bilag');await ownerPage.getByRole('heading',{name:'Bilag',exact:true}).waitFor();await ownerPage.getByRole('button',{name:'Indlæs bilag',exact:true}).click();const dialog=ownerPage.getByRole('dialog',{name:'Indlæs bilag',exact:true});step='ui-'+kind+'-file-and-explicit-type';await dialog.getByLabel('Bilagsfil',{exact:true}).setInputFiles({name:f.fileName,mimeType:'text/plain',buffer:f.bytes});await dialog.getByLabel('Bilagstype',{exact:true}).selectOption('cash_register_receipt');await dialog.getByLabel('Kilde',{exact:true}).fill(body(kind).metadata.source);step='ui-'+kind+'-single-confirmed-ingest';const wait=ownerPage.waitForResponse(r=>new URL(r.url()).pathname===prefix+'documents/ingest'&&r.request().method()==='POST');await dialog.getByRole('button',{name:'Indlæs bilag',exact:true}).click();const response=await wait,result=await response.json();need(response.status()===200&&result.ok===true&&Number.isSafeInteger(result.document.id),'actual UI original ingestion accepted');const sent=response.request().postDataJSON();need(sent.fileName===f.fileName&&sent.fileBase64===f.bytes.toString('base64')&&sent.confirm===true,'actual UI preserves exact chosen original bytes');step='ui-'+kind+'-complete-receipt';await dialog.getByText('Bilaget er indlæst som '+result.document.documentNo+'.',{exact:true}).waitFor();await dialog.getByRole('button',{name:'Luk',exact:true}).click();documents.push({kind,...result.document});await originals();return{result,originalSha256:f.sha256};}
async function historical(){step='historical-off-policy-original-has-no-invented-scanner';sourceBooks=await books();const result=await uiUpload('historical');need(isDeepStrictEqual(await books(),sourceBooks),'document ingress never posts');return result;}
async function clean(){step='required-policy-after-normal-reopen';await ownerPage.context().clearCookies();await login(ownerPage,owner);const result=await uiUpload('clean');need(isDeepStrictEqual(await books(),sourceBooks),'clean scan does not book');return result;}
async function negatives(){const before=await snapshot(),outcomes={};for(const[kind,status,code]of[['reject',422,'document_scanner_rejected'],['timeout',503,'document_scanner_unavailable'],['missing-id',422,'document_scanner_rejected'],['invalid-field',503,'document_scanner_unavailable'],['malformed',422,'document_scanner_rejected']]){step='provider-'+kind+'-exact-curated-outcome';const r=await call(body(kind));need(r.status===status&&r.body?.ok===false&&r.body.code===code,'typed provider '+kind+' HTTP/code contract');outcomes[kind]=r;await unchanged(before);}step='source-compatible-duplicate-original-refusal';const duplicate=await call(body('clean'));need(duplicate.status===422&&duplicate.body?.ok===false&&duplicate.body.code==='document_refused'&&duplicate.body.errors?.some(s=>s.includes('duplicate document content')),'duplicate original never re-registers');await unchanged(before);return{outcomes,duplicate,noCanonicalPublication:true};}
async function unauthorized(){step='reviewer-without-upload-right-refused-before-provider';const before=await snapshot(),r=await call(body('unauthorized'),memberPage);need(r.status===403&&r.body?.ok===false&&r.body.code==='access_denied','current unauthorized upload is denied');await unchanged(before);await post('/api/workspace/members/company',{userId:memberId,action:'grant',companySlug:'synthetic',role:'bookkeeper'});return{refusal:r,currentBookkeeperGrantedForHeldCase:true};}
async function holdBegin(){step='invited-current-bookkeeper-starts-held-scan';await memberPage.evaluate(({path,input})=>{window.scannerPending=fetch(path,{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify(input)}).then(async r=>({status:r.status,body:await r.json()}));},{path:prefix+'documents/ingest',input:body('held')});return{requestStarted:true};}
async function revoke(){step='public-owner-revokes-uploader-while-provider-is-held';const result=await post('/api/workspace/members/company',{userId:memberId,action:'revoke',companySlug:'synthetic'});return{revocationCompletedBeforeProviderRelease:true,result};}
async function holdFinish(){step='successful-provider-answer-cannot-reuse-revoked-authority';const r=await memberPage.evaluate(()=>window.scannerPending);need(r.status===403&&r.body?.ok===false&&r.body.code==='access_denied','revoked final admission refuses successful scan');need(isDeepStrictEqual(await books(),sourceBooks),'revoked final scan never books');return{refusal:r,postScanCurrentRightsQualified:true};}
async function client(kind,mode,id){const file=join(work,'scanner-client.cookie');writeFileSync(file,(await ownerPage.context().cookies(base)).map(c=>`${c.name}=${c.value}`).join('; '),{mode:0o600,flag:'wx'});try{const common=['--server',base,'--cookie-file',file,'--allow-loopback-http'];let argv,input;if(kind==='cli'){argv=['remote',mode==='ingest'?'ingest-document':'document-original',...common,'--company','synthetic',...(mode==='ingest'?['--confirm']:['--id',String(id)])];if(mode==='ingest')input=JSON.stringify(body(kind));}else{argv=common;input=[{jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'synthetic-scanner-proof',version:'1'}}},{jsonrpc:'2.0',method:'notifications/initialized'},{jsonrpc:'2.0',id:2,method:'tools/call',params:{name:mode==='ingest'?'ingest_document':'document_original',arguments:{company:'synthetic',...(mode==='ingest'?{body:JSON.stringify(body(kind)),confirm:true}:{id})}}}].map(v=>JSON.stringify(v)).join('\n')+'\n';}const r=spawnSync(join(work,kind==='cli'?'rentemester-rust':'rentemester-mcp'),argv,{input,encoding:'utf8',timeout:45000,maxBuffer:16*1024*1024});need(!r.error&&r.status===0,kind+' existing native document adapter terminal '+mode);const value=kind==='cli'?JSON.parse(r.stdout):r.stdout.trim().split('\n').map(s=>JSON.parse(s)).find(v=>v.id===2)?.result?.structuredContent;need(value?.ok===true&&value.data?.ok===true,'canonical unchanged document client envelope');return value.data;}finally{unlinkSync(file);}}
async function clients(){step='existing-confirmed-cli-mcp-ingress-actual-originals';const results={};for(const kind of['cli','mcp']){const result=await client(kind,'ingest');need(Number.isSafeInteger(result.document.id),'client registers real scanned original');documents.push({kind,...result.document});const original=await client(kind,'original',result.document.id);need(isDeepStrictEqual(original,{ok:true,original:{contentType:'text/plain',bytes:[...fixtures[kind].bytes]}}),'authorized original full HTTP/native client parity');results[kind]=result;}await originals();need(isDeepStrictEqual(await books(),sourceBooks),'all scanned document client writes leave financial statements unchanged');stable=await snapshot();await ownerPage.goto(base+'/companies/synthetic/bilag');await ownerPage.getByRole('heading',{name:'Bilag',exact:true}).waitFor();await ownerPage.screenshot({path:process.env.RM_SCANNER_SCREENSHOT,fullPage:true});return{results,documents,canonical:stable};}
async function assertStable(){need(isDeepStrictEqual(await snapshot(),stable),'exact documents/provenance/books/UUID survive restart/restore');await originals();}
async function reopen(){step='fresh-mfa-source-restart-no-rescan';await ownerPage.context().clearCookies();await login(ownerPage,owner);await assertStable();return{freshMfaRestart:true};}
async function archive(name){step='explicit-signed-originals-and-scan-evidence-'+name;const r=await ownerPage.request.post(base+'/api/workspace/backup',{headers:{origin:base},data:{confirm:true}});need(r.status()===200&&r.headers()['content-type']==='application/vnd.rentemester.native-backup+json','real scanner evidence signed coverage');const bytes=await r.body();need(bytes.length>0&&bytes.length<=96*1024*1024,'bounded full signed snapshot');writeFileSync(join(work,'scanner-'+name+'.native-backup.json'),bytes,{mode:0o600,flag:'wx'});await assertStable();return{sha256:hash(bytes),bytes:bytes.length,documents};}
async function restored(){step='source-free-fresh-owner-normal-verification-mfa-no-scanner-config';ownerPage=await newPage();freshOwner={email:'restored@example.invalid',password:'synthetic-restored-password-789',codes:[]};await ownerPage.goto((await mail('verification',freshOwner.email)).actionUrl);need((await enroll(ownerPage,freshOwner)).user.id!==ownerId,'restored history is not imported current identity');await assertStable();return{sourceFreeRestoreQualified:true,historicalEvidenceWithoutCredentialsOrRescan:true};}
async function restoredReopen(){step='restored-owner-fresh-mfa-reopen-exact-proof';await ownerPage.context().clearCookies();await login(ownerPage,freshOwner);await assertStable();return{restoredTargetReopenQualified:true};}
let verificationProof;
async function verificationPhase(action){
 step=action;
 if(!verificationProof){const module=await import(process.env.RM_AUDIT_VERIFY_HELPER);verificationProof=module.createAuditVerifyProof(ownerPage,base,work,memberPage);}
 return action==='verification-reopen'?verificationProof.reopen():verificationProof.run();
}
let auditProof;
async function auditPhase(action){
 step=action;
 if(!auditProof){const module=await import(process.env.RM_AUDIT_HELPER);auditProof=module.createAuditLogProof(ownerPage,base,work,memberPage);}
 if(action==='audit-reopen'){await ownerPage.context().clearCookies();await login(ownerPage,owner);return auditProof.reopen();}
 return auditProof.run();
}
let servicePurchase,serviceRunResult,backupRecovery,compensationProof,interestProof,badDebtProof,dineroProof,syntheticProof,importedPortfolioProof;
async function compensationPhase(action){
 step=action;diagnosticPage=ownerPage;
 if(!compensationProof){const module=await import(process.env.RM_INVOICE_COMPENSATION_HELPER);need(typeof module.createInvoiceCompensationProof==='function','pinned compensation proof factory required');compensationProof=module.createInvoiceCompensationProof(ownerPage,base,work,memberPage,serviceRunResult);}
 if(action==='compensation-reopen'){await ownerPage.context().clearCookies();await login(ownerPage,owner);return compensationProof.reopen();}
 return compensationProof.run();
}
async function interestPhase(action){
 step=action;diagnosticPage=ownerPage;
 if(!interestProof){const module=await import(process.env.RM_INVOICE_INTEREST_HELPER);need(typeof module.createInvoiceInterestProof==='function','pinned interest proof factory required');interestProof=module.createInvoiceInterestProof(ownerPage,base,work,memberPage,serviceRunResult);}
 if(action==='interest-reopen'){await ownerPage.context().clearCookies();await login(ownerPage,owner);return interestProof.reopen();}
 return interestProof.run();
}
async function badDebtPhase(action){
 step=action;diagnosticPage=ownerPage;
 if(!badDebtProof){const module=await import(process.env.RM_INVOICE_BAD_DEBT_HELPER);need(typeof module.createInvoiceBadDebtProof==='function','pinned bad-debt proof factory required');badDebtProof=module.createInvoiceBadDebtProof(ownerPage,base,work,memberPage,serviceRunResult);}
 if(action==='bad-debt-reopen'){await ownerPage.context().clearCookies();await login(ownerPage,owner);return badDebtProof.reopen();}
 return badDebtProof.run();
}
async function syntheticPhase(action){
 step=action;diagnosticPage=ownerPage;
 if(!syntheticProof){const module=await import(process.env.RM_SYNTHETIC_IMPORT_HELPER);syntheticProof=module.createSyntheticImportProof(ownerPage,base,work,memberPage);}
 if(action==='synthetic-reopen'){await ownerPage.context().clearCookies();await login(ownerPage,owner);return syntheticProof.reopen();}
 return syntheticProof.run();
}
async function importedPortfolioPhase(action){
 step=action;diagnosticPage=ownerPage;
 if(!importedPortfolioProof){const module=await import(process.env.RM_IMPORTED_PORTFOLIO_HELPER);importedPortfolioProof=module.createImportedPortfolioProof(ownerPage,base,work,serviceRunResult);}
 if(action==='imported-portfolio-reopen'){await ownerPage.context().clearCookies();await login(ownerPage,owner);return importedPortfolioProof.reopen();}
 return importedPortfolioProof.run();
}
async function dineroPhase(action){
 step=action;diagnosticPage=ownerPage;
 if(!dineroProof){const module=await import(process.env.RM_DINERO_IMPORT_HELPER);need(typeof module.createDineroImportProof==='function','pinned Dinero proof factory required');dineroProof=module.createDineroImportProof(ownerPage,base,work,memberPage,serviceRunResult);}
 if(action==='dinero-reopen'){await ownerPage.context().clearCookies();await login(ownerPage,owner);return dineroProof.reopen();}
 return dineroProof.run();
}
async function backupRecoveryPhase(action){
 step=action;
 if(!backupRecovery){const module=await import(process.env.RM_BACKUP_RECOVERY_HELPER);backupRecovery=module.createBackupRecoveryProof(base,work);}
 if(action==='backup-recovery-capture'){const result=await backupRecovery.capture(ownerPage,ownerId,serviceRunResult.remoteBackup);const response=await ownerPage.request.post(base+'/api/workspace/backup',{headers:{origin:base},data:{confirm:true}});need(response.status()===200&&response.headers()['content-type']==='application/vnd.rentemester.native-backup+json','final recovery signed export');const bytes=await response.body();need(bytes.length>0&&bytes.length<=96*1024*1024,'bounded recovery archive');writeFileSync(join(work,'scanner-source.native-backup.json'),bytes,{mode:0o600,flag:'wx'});return{...result,archiveSha256:hash(bytes),archiveBytes:bytes.length};}
 if(action==='backup-recovery-detach'){await Promise.all(browser.contexts().map(context=>context.close()));servicePurchase=null;serviceRunResult=null;owner.codes=[];member.codes=[];return{sourceBrowserContextsClosed:true};}
 if(action==='backup-recovery-restored'){ownerPage=await newPage();freshOwner={email:'restored@example.invalid',password:'synthetic-restored-password-789',codes:[]};await ownerPage.goto((await mail('verification',freshOwner.email)).actionUrl);const me=await enroll(ownerPage,freshOwner);return backupRecovery.restored(ownerPage,me.user.id);}
 if(action==='backup-recovery-reopen'){await ownerPage.context().clearCookies();const me=await login(ownerPage,freshOwner);return backupRecovery.reopen(ownerPage,me.user.id);}
 throw Error('Unknown recovery phase');
}
async function servicePhase(action){
 if(!servicePurchase){const module=await import(process.env.RM_SERVICE_PURCHASE_HELPER);const factory=module[process.env.RM_SCANNER_PROOF_FACTORY||'createServicePurchaseProof'];need(typeof factory==='function','configured proof factory required');servicePurchase=await factory(ownerPage,base,work,memberPage,['createLocalRuntimeProof','createLocalRuntimeMcpProof','createKnowledgeScopeProof','createPrivacyComplianceProof'].includes(process.env.RM_SCANNER_PROOF_FACTORY)?{owner,member}:undefined);}
 if(action==='service-reopen'){await ownerPage.context().clearCookies();await login(ownerPage,owner);return servicePurchase.reopen();}
 const result=await servicePurchase[{'service-run':'run','service-hold-begin':'holdBegin','service-revoke':'revoke','service-hold-finish':'holdFinish'}[action]]();if(action==='service-run')serviceRunResult=result;return result;
}
for await(const line of createInterface({input:process.stdin,crlfDelay:Infinity})){try{const action=JSON.parse(line);let value;if(action.startsWith('imported-portfolio-')){try{value=await importedPortfolioPhase(action);}catch(error){detail=String(error.message).slice(0,1200);throw error;}}else if(action.startsWith('synthetic-')){try{value=await syntheticPhase(action);}catch(error){detail=String(error.message).slice(0,1200);throw error;}}else if(action.startsWith('dinero-')){try{value=await dineroPhase(action);}catch(error){detail=String(error.message).slice(0,1200);throw error;}}else if(action.startsWith('bad-debt-')){try{value=await badDebtPhase(action);}catch(error){detail=String(error.message).slice(0,1200);throw error;}}else if(action.startsWith('interest-')){try{value=await interestPhase(action);}catch(error){detail=String(error.message).slice(0,1200);throw error;}}else if(action.startsWith('compensation-')){try{value=await compensationPhase(action);}catch(error){detail=String(error.message).slice(0,1200);throw error;}}else if(action.startsWith('verification-')){try{value=await verificationPhase(action);}catch(error){detail=String(error.message).slice(0,1200);throw error;}}else if(action.startsWith('audit-')){try{value=await auditPhase(action);}catch(error){detail=String(error.message).slice(0,200);throw error;}}else if(action.startsWith('backup-recovery-')){try{value=await backupRecoveryPhase(action);}catch(error){detail=String(error.message).slice(0,200);throw error;}}else if(action.startsWith('service-')){try{value=await servicePhase(action);}catch(error){detail=String(error.message).slice(0,200);throw error;}}else if(action==='authenticate')value=await authenticate();else if(action==='historical')value=await historical();else if(action==='clean')value=await clean();else if(action==='negatives')value=await negatives();else if(action==='unauthorized')value=await unauthorized();else if(action==='hold-begin')value=await holdBegin();else if(action==='revoke')value=await revoke();else if(action==='hold-finish')value=await holdFinish();else if(action==='clients')value=await clients();else if(action==='reopen')value=await reopen();else if(['archive-source','archive-restarted','archive-restored'].includes(action))value=await archive(action.slice(8));else if(action==='restored')value=await restored();else if(action==='restored-reopen')value=await restoredReopen();else if(action==='finish'){await browser?.close();emit({ok:true,value:{finished:true}});break;}else throw Error('unknown phase');emit({ok:true,value});}catch(error){let pageState=null;try{pageState=await Promise.race([diagnosticPage?.evaluate(()=>({pathname:location.pathname,headings:[...document.querySelectorAll('h1,h2,h3,h4')].map(e=>e.textContent.slice(0,100)),buttons:[...document.querySelectorAll('button')].map(e=>e.textContent.slice(0,100)),alerts:[...document.querySelectorAll('[role=alert]')].map(e=>e.textContent.slice(0,300))})),new Promise(resolve=>setTimeout(()=>resolve(null),2000))]);}catch{}emit({ok:false,step,detail,errorType:error?.name,trace,pageState});await Promise.race([browser?.close().catch(()=>{}),new Promise(resolve=>setTimeout(resolve,2000))]);process.exitCode=1;break;}}

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
    parser.add_argument('--backup-destinations', action='store_true', help='Add normal web placement and CLI/MCP readback of a signed backup to the service-purchase proof')
    parser.add_argument('--service-purchase', action='store_true', help='Focused service scanned-original→payable→bank payment/revocation/restart proof')
    parser.add_argument('--backup-lock', action='store_true', help='Normal MFA financial backup lock → qualified artifact → unlock/single posting/restart proof')
    parser.add_argument('--remote-backup', action='store_true', help='Extend backup-lock with private TLS Drive metadata/media verification; fixture copy is not app upload')
    parser.add_argument('--backup-recovery', action='store_true', help='Continue remote-backup proof through source-free fresh-owner restore and restart')
    parser.add_argument('--audit-log', action='store_true', help='Continue backup-lock single-post proof with one reversal and audit web/HTTP/CLI/MCP inspection, current revocation and restart')
    parser.add_argument('--audit-verify', action='store_true', help='Read-only complete accounting verification, cross-client parity, originals tamper, current revocation and restart')
    parser.add_argument('--invoice-compensation', action='store_true', help='Continue backup-lock through actual DKK/EUR compensation calculate/register/post/principal/claim receipt, exact full audit and normal MFA restart')
    parser.add_argument('--invoice-interest', action='store_true', help='Actual DKK dated interest registration, posting, causal correction, receipts, full audit and fresh MFA restart; foreign interest remains unqualified')
    parser.add_argument('--invoice-bad-debt', action='store_true', help='Actual DKK/foreign established-loss UI/CLI/MCP writes, original principal and claims, VAT/full audit, fresh MFA restart; no independent legal-loss proof')
    parser.add_argument('--dinero-import', action='store_true', help='Reviewed latest-year Dinero import and imported receivables through web/CLI/MCP, exact original, full audit and fresh MFA restart')
    parser.add_argument('--synthetic-import', action='store_true', help='Original synthetic CSV through shared chart/opening engine, nonVAT shifted-year owner, clients/expense/report/backup/restart')
    parser.add_argument('--imported-portfolio', action='store_true', help='After Dinero import, issue/post a native invoice and verify explicit native-only portfolio through web/HTTP/CLI/MCP and restart')
    parser.add_argument('--journal-history', action='store_true', help='Standalone original journal CLI/MCP headers/date/status/page, ordinary MFA/read refusal/noeffects and restart')
    parser.add_argument('--dated-reports', action='store_true', help='Standalone original three dated CLI financial reports, current authority, originals, no effects and normal MFA restart')
    parser.add_argument('--dashboard-json', action='store_true', help='Standalone original company dashboard JSON, dated invoice/receipt/current metadata, current reader/admin backup rights and restart')
    parser.add_argument('--portfolio-overview', action='store_true', help='Standalone original MCP portfolio_overview name/data/envelope over connected Rustserver, visibility/adminbackup/noeffects/restart; localfilesystem launcher remains separate')
    parser.add_argument('--annual-exports', action='store_true', help='Standalone closed-year annual to tax and deterministic iXBRL, original clients and normal MFA restart')
    parser.add_argument('--authority-export', action='store_true', help='Standalone confirmed authority/accountant request-context package, original CLI/MCP/localfiles and normal MFA restart')
    parser.add_argument('--saft-artifact', action='store_true', help='Original SAF-T CLI local package and timestamp modes through shared Rust server, normal MFA and restart')
    parser.add_argument('--privacy-compliance', action='store_true', help='Whole708 current legal-hold/eligible-forget/signed audit and versioned compliance on existing normal-MFA local Rust owner')
    parser.add_argument('--knowledge-scope', action='store_true', help='Full641 scoped-service knowledge MCP/CLI/HTTP canonical slug/local-root parity, history/refusals and ordinary MFA restart')
    parser.add_argument('--local-runtime-join', action='store_true', help='Local CLI normal MFA and one source-backed posting through same private UDS Rust owner, current authority/restart/cleanup')
    parser.add_argument('--local-runtime-mcp', action='store_true', help='Existing MCP tools join the same private Rust UDS owner with normal credentials, posting/replay/revocation/restart')
    parser.add_argument('--bank-queries', action='store_true', help='Original bank query/report/account tools through shared Rust server, real import/link/reversal/correction and normalMFArestart')
    parser.add_argument('--period-original', action='store_true', help='Standalone original five-command period lifecycle, historical reviews, atomic reported receipts and normal MFA restart')
    parser.add_argument('--journal-original', action='store_true', help='Standalone original numeric journal dry-run/post/reversal with period locks and normal MFA restart')
    parser.add_argument('--journal-mcp-original', action='store_true', help='Standalone original numeric MCP keyed post/reversal, source envelopes, period lifecycle and normal MFA receipt restart')
    parser.add_argument('--period-mcp-original', action='store_true', help='Standalone original five named period tools, owner-issued service force/open obligations and exact MFA restart')
    parser.add_argument('--exceptions-original', action='store_true', help='Standalone original exception CLI/MCP review-only, current service grants and exact normal MFA restart')
    parser.add_argument('--native-cli-candidate', action='store_true', help='Standalone private normal-name candidate, same local owner and two legally distinct same-name vendors')
    args = parser.parse_args()
    if args.native_cli_candidate and any(value for key,value in vars(args).items() if isinstance(value,bool) and key != 'native_cli_candidate'):
        parser.error('--native-cli-candidate is standalone and excludes other extensions')
    if args.exceptions_original and any(value for key,value in vars(args).items() if isinstance(value,bool) and key != 'exceptions_original'):
        parser.error('--exceptions-original is standalone and excludes other extensions')
    if args.period_mcp_original and any(value for key,value in vars(args).items() if isinstance(value,bool) and key != 'period_mcp_original'):
        parser.error('--period-mcp-original is standalone and excludes other extensions')
    if args.journal_mcp_original and any(value for key,value in vars(args).items() if isinstance(value,bool) and key != 'journal_mcp_original'):
        parser.error('--journal-mcp-original is standalone and excludes other extensions')
    if args.journal_original and any(value for key,value in vars(args).items() if isinstance(value,bool) and key != 'journal_original'):
        parser.error('--journal-original is standalone and excludes other extensions')
    if args.period_original and any(value for key,value in vars(args).items() if isinstance(value,bool) and key != 'period_original'):
        parser.error('--period-original is standalone and excludes other extensions')
    if args.bank_queries and any(value for key,value in vars(args).items() if isinstance(value,bool) and key != 'bank_queries'):
        parser.error('--bank-queries is standalone and excludes other extensions')
    if args.local_runtime_join and args.local_runtime_mcp:
        parser.error('Choose exactly one local adapter')
    if (args.local_runtime_join or args.local_runtime_mcp) and any(value for key,value in vars(args).items() if isinstance(value,bool) and key not in {'local_runtime_join','local_runtime_mcp','native_cli_candidate'}):
        parser.error('--local-runtime-join is standalone and excludes other extensions')
    if args.saft_artifact and any(value for key,value in vars(args).items() if isinstance(value,bool) and key != 'saft_artifact'):
        parser.error('--saft-artifact is standalone and excludes other extensions')
    if args.authority_export and any(value for key,value in vars(args).items() if isinstance(value,bool) and key != 'authority_export'):
        parser.error('--authority-export is standalone and excludes other extensions')
    if args.annual_exports and any(value for key,value in vars(args).items() if isinstance(value,bool) and key != 'annual_exports'):
        parser.error('--annual-exports is a standalone proof and excludes other extensions')
    if args.portfolio_overview and any(value for key,value in vars(args).items() if isinstance(value,bool) and key != 'portfolio_overview'):
        parser.error('--portfolio-overview is a standalone proof and excludes other extensions')
    if args.dashboard_json and any(value for key, value in vars(args).items() if isinstance(value, bool) and key != 'dashboard_json'):
        parser.error('--dashboard-json is a standalone proof and excludes other extensions')
    if args.dated_reports and any(value for key, value in vars(args).items() if isinstance(value, bool) and key != 'dated_reports'):
        parser.error('--dated-reports is a standalone proof and excludes other extensions')
    if args.journal_history and any(value for key, value in vars(args).items() if isinstance(value, bool) and key != 'journal_history'):
        parser.error('--journal-history is a standalone proof and excludes other extensions')
    if args.imported_portfolio and (not args.backup_lock or args.dinero_import or args.synthetic_import or args.invoice_bad_debt or args.invoice_interest or args.invoice_compensation or args.audit_log or args.audit_verify or args.remote_backup or args.backup_recovery or args.backup_destinations or args.service_purchase):
        parser.error('--imported-portfolio requires --backup-lock and excludes other proof extensions')
    if args.synthetic_import and (not args.backup_lock or args.dinero_import or args.invoice_bad_debt or args.invoice_interest or args.invoice_compensation or args.audit_log or args.audit_verify or args.remote_backup or args.backup_recovery or args.backup_destinations or args.service_purchase):
        parser.error('--synthetic-import requires --backup-lock and excludes other proof extensions')
    if args.dinero_import and (not args.backup_lock or args.invoice_bad_debt or args.invoice_interest or args.invoice_compensation or args.audit_log or args.audit_verify or args.remote_backup or args.backup_recovery or args.backup_destinations or args.service_purchase):
        parser.error('--dinero-import requires --backup-lock and excludes other proof extensions')
    if args.invoice_bad_debt and (not args.backup_lock or args.invoice_interest or args.invoice_compensation or args.audit_log or args.audit_verify or args.remote_backup or args.backup_recovery or args.backup_destinations or args.service_purchase):
        parser.error('--invoice-bad-debt requires --backup-lock and excludes other proof extensions')
    if args.invoice_interest and (not args.backup_lock or args.invoice_compensation or args.audit_log or args.audit_verify or args.remote_backup or args.backup_recovery or args.backup_destinations or args.service_purchase):
        parser.error('--invoice-interest requires --backup-lock and excludes other proof extensions')
    if args.invoice_compensation and (not args.backup_lock or args.audit_log or args.remote_backup):
        parser.error('--invoice-compensation requires --backup-lock and excludes --audit-log/--remote-backup')
    if args.audit_verify and not args.audit_log:
        parser.error('--audit-verify requires --audit-log')
    if args.audit_log and (not args.backup_lock or args.remote_backup):
        parser.error('--audit-log requires --backup-lock and excludes --remote-backup')
    if args.backup_recovery and not args.remote_backup:
        parser.error('--backup-recovery requires --remote-backup')
    if args.remote_backup and not args.backup_lock:
        parser.error('--remote-backup requires --backup-lock')
    if args.backup_lock and (args.service_purchase or args.backup_destinations):
        parser.error('--backup-lock is mutually exclusive with --service-purchase/--backup-destinations')
    if args.backup_destinations and not args.service_purchase:
        parser.error('--backup-destinations requires --service-purchase')
    if args.privacy_compliance:
        if any(value for key,value in vars(args).items() if isinstance(value,bool) and key != 'privacy_compliance'):
            parser.error('--privacy-compliance is standalone and excludes other extensions')
        args.local_runtime_join = True
    if args.knowledge_scope:
        if any(value for key,value in vars(args).items() if isinstance(value,bool) and key != 'knowledge_scope'):
            parser.error('--knowledge-scope is standalone and excludes other extensions')
        args.local_runtime_join = True
    if args.native_cli_candidate:
        args.local_runtime_join = True
    if os.path.lexists(args.evidence) or os.path.lexists(args.evidence.with_suffix('.png')):
        parser.error('New evidence and screenshot paths required')
    if os.uname().sysname != 'Darwin' or os.getuid() == 0 or os.getuid() != os.geteuid():
        parser.error('Ordinary macOS fixture owner required')
    os.umask(0o077)
    parent = (Path.home() if (args.local_runtime_join or args.local_runtime_mcp) else Path.home() / 'Library/Application Support').resolve(strict=True)
    fixture_prefix = '.rm139-' if args.local_runtime_mcp else '.rm138-' if (args.local_runtime_join or args.local_runtime_mcp) else PREFIX
    work = parent / (fixture_prefix + uuid.uuid4().hex)
    identity = None
    child = controller = None
    log = None
    servers = []
    gateway_rows, gateway_requests, errors = {}, [], []
    provider_requests = []
    drive_requests = []
    drive_token = secrets.token_urlsafe(32)
    scanner_token = secrets.token_urlsafe(32)
    held_received, held_release = threading.Event(), threading.Event()
    gateway_lock = threading.Lock()
    bearer = secrets.token_urlsafe(32)
    backend_port, mail_port = free_port(), free_port()
    if backend_port == mail_port:
        raise RuntimeError('Distinct owned ports required')
    origin = f'http://127.0.0.1:{backend_port}'
    report = {'status': 'running', 'sourceRevision': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=REPO, text=True).strip(), 'runnerSha256': digest(Path(__file__)), 'browserHelperSha256': hashlib.sha256(BROWSER.encode()).hexdigest(), 'canonicalOrigin': origin, 'noOfflineIdentitySeeds': True, 'ordinaryMacOsUser': True, 'ownedProfile': str(work), 'externalMailDelivered': False, 'workflow': 'required_ordinary_document_scanning', 'ordinaryDocumentScanningQualified': False, 'inboxCompletionScannerOffAsSource': True, 'currentRevocationDuringScanQualified': False, 'installedNativeQualified': False, 'screenshot': str(args.evidence.with_suffix('.png')),  'actions': [], 'results': {}, 'sourceFreeRestoreQualified': False}
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
            return work.parent == parent and work.name.startswith(fixture_prefix) and (stat.st_dev, stat.st_ino, stat.st_uid) == (*identity, os.getuid())

        class QuietServer(http.server.ThreadingHTTPServer):
            daemon_threads = True

        class Gateway(http.server.BaseHTTPRequestHandler):
            def log_message(self, *_args):
                pass

            def do_GET(self):
                try:
                    parsed = urlsplit(self.path)
                    match = re.fullmatch(r'/drive/v3/files/(synthetic_[a-z_]+)', parsed.path)
                    if not args.remote_backup or not match or self.headers.get('authorization') != 'Bearer ' + drive_token:
                        raise ValueError('Private Drive request authority/path refused')
                    object_id = match[1]
                    if object_id not in ('synthetic_good', 'synthetic_wrong_parent', 'synthetic_wrong_hash', 'synthetic_oversized', 'synthetic_missing', 'synthetic_held'):
                        raise ValueError('Unexpected Drive object')
                    query = parse_qs(parsed.query)
                    media = query == {'alt': ['media'], 'supportsAllDrives': ['true']}
                    if not media and query != {'fields': ['id,name,parents,size,sha256Checksum,trashed'], 'supportsAllDrives': ['true']}:
                        raise ValueError('Exact Drive GET query required')
                    descriptor = json.loads((work / 'drive-object.json').read_text())
                    raw = (work / 'drive-remote-object.native-backup.json').read_bytes()
                    if len(raw) != descriptor['size'] or hashlib.sha256(raw).hexdigest() != descriptor['sha256']:
                        raise ValueError('Independent owned remote object changed')
                    with gateway_lock:
                        drive_requests.append({'objectId': object_id, 'kind': 'media' if media else 'metadata'})
                        pending = work / 'drive-trace.pending'
                        pending.write_text(json.dumps(drive_requests))
                        pending.replace(work / 'drive-trace.json')
                    if object_id == 'synthetic_held' and not media:
                        (work / 'drive-held-received').write_text('held\n')
                        deadline = time.monotonic() + 12
                        while not (work / 'drive-held-release').exists():
                            if time.monotonic() >= deadline:
                                raise ValueError('Explicit held Drive release deadline')
                            time.sleep(0.02)
                    if object_id == 'synthetic_missing':
                        self.send_response(404)
                        self.send_header('content-length', '0')
                        self.end_headers()
                        return
                    payload = {'id': object_id, 'name': descriptor['fileName'], 'parents': ['synthetic-wrong-folder' if object_id == 'synthetic_wrong_parent' else descriptor['folderId']], 'size': str(len(raw)), 'sha256Checksum': descriptor['sha256'], 'trashed': False}
                    media_bytes = bytes([raw[0] ^ 1]) + raw[1:] if object_id == 'synthetic_wrong_hash' else raw
                    data = (raw + b'x' if object_id == 'synthetic_oversized' else media_bytes) if media else json.dumps(payload).encode()
                    self.send_response(200)
                    self.send_header('content-type', 'application/octet-stream' if media else 'application/json')
                    self.send_header('content-length', str(len(data)))
                    self.end_headers()
                    self.wfile.write(data)
                except (BrokenPipeError, ConnectionResetError, ssl.SSLEOFError):
                    pass
                except Exception:
                    errors.append('private-drive-contract-refusal')
                    self.send_error(400)

            def do_POST(self):
                if self.path == '/scan':
                    return self.provider()
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

            def provider(self):
                try:
                    length = int(self.headers.get('content-length', '0'))
                    if not 0 < length <= 32768 or self.headers.get('authorization') != 'Bearer ' + scanner_token or self.headers.get('content-type') != 'application/json' or self.headers.get('accept') != 'application/json':
                        raise ValueError('Private scanner authority or content envelope')
                    request = json.loads(self.rfile.read(length))
                    if set(request) != {'sha256', 'mimeType', 'filename', 'bytesBase64'}:
                        raise ValueError('Exact scanner request fields')
                    match = re.fullmatch(r'synthetic-scanner-([a-z-]+)\.txt', request['filename'])
                    if not match or match[1] not in ('clean', 'reject', 'timeout', 'missing-id', 'invalid-field', 'malformed', 'held', 'cli', 'mcp'):
                        raise ValueError('Unexpected original or unauthorized I/O')
                    kind = match[1]
                    raw = base64.b64decode(request['bytesBase64'], validate=True)
                    expected = f'Synthetic required scanner {kind}\nOwned harmless original, no accounting instruction.\n'.encode()
                    if raw != expected or request['sha256'] != hashlib.sha256(raw).hexdigest() or request['mimeType'] != 'text/plain':
                        raise ValueError('Immutable scanner bytes/MIME/SHA mismatch')
                    with gateway_lock:
                        provider_requests.append({'kind': kind, 'sha256': request['sha256'], 'bytes': len(raw), 'mimeType': request['mimeType']})
                    if kind == 'held':
                        held_received.set()
                        if not held_release.wait(8):
                            raise ValueError('Owned explicit held scan release deadline')
                    if kind == 'timeout':
                        time.sleep(11)
                    payload = {'ok': True, 'scannerId': '  Synthetic Scanner  ', 'scannerVersion': '  65.1  ', 'evidenceRef': ' synthetic:' + request['sha256'] + ' '}
                    if kind == 'reject':
                        payload = {'ok': False}
                    elif kind == 'missing-id':
                        payload = {'ok': True, 'scannerVersion': '65.1'}
                    elif kind == 'invalid-field':
                        payload['scannerVersion'] = 'invalid\nversion'
                    data = b'not valid JSON' if kind == 'malformed' else json.dumps(payload).encode()
                    self.send_response(200)
                    self.send_header('content-type', 'application/json')
                    self.send_header('content-length', str(len(data)))
                    self.end_headers()
                    self.wfile.write(data)
                except (BrokenPipeError, ConnectionResetError, ssl.SSLEOFError):
                    pass
                except Exception:
                    errors.append('private-scanner-contract-refusal')
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
            if (args.local_runtime_join or args.local_runtime_mcp):
                argv += ['--local-runtime-dir', str(work)]
            if create:
                argv.append('--create')
            if restore:
                root = Path(config['root'])
                if os.path.lexists(root):
                    raise RuntimeError('Fresh restore only; refuse overwrite')
                argv += ['--restore-backup', str(work / 'scanner-source.native-backup.json'), '--trusted-backup-key', str(work / 'trusted.pub'), '--confirm-target', str(root), '--source-unavailable']
            log = (work / 'server.log').open('ab')
            product_env = None
            if args.native_cli_candidate:
                product_env = {key: value for key, value in os.environ.items()
                               if not re.search(r'BUN|NODE|FALLBACK|LEGACY|RENTEMESTER|^RM_', key)}
                product_env['PATH'] = str(work / 'empty-path')
            child = subprocess.Popen(argv, cwd=REPO, env=product_env, stdin=subprocess.DEVNULL, stdout=log, stderr=log)
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
                if args.backup_destinations and (work / 'backup-diagnostic.json').is_file():
                    report['backupDiagnostic'] = json.loads((work / 'backup-diagnostic.json').read_text())
                save()
                raise RuntimeError('Browser assertion failed at ' + value.get('step', action))
            with gateway_lock:
                report['providerRequestTrace'] = list(provider_requests)
                if args.remote_backup:
                    report['driveGetTrace'] = list(drive_requests)
                report['privateGatewayErrors'] = list(errors)
            report['actions'].append({'action': action, 'status': 'complete'})
            report['results'][action] = value['value']
            save()
            return value['value']

        def provider_count():
            with gateway_lock:
                return len(provider_requests)

        def original_inventory():
            directory = Path(config['root']) / 'synthetic/documents/originals'
            if not owned() or not directory.is_dir() or directory.is_symlink():
                raise RuntimeError('Actual owned original directory required')
            paths = list(directory.rglob('*'))
            if any(p.is_symlink() for p in paths):
                raise RuntimeError('Unexpected symlink in owned original evidence')
            return {str(p.relative_to(directory)): digest(p) for p in sorted(paths) if p.is_file()}

        def inspect_snapshot(name, trusted):
            progress('operator:independent-signed-scan-evidence-' + name)
            path = work / ('scanner-' + name + '.native-backup.json')
            receipt = report['results']['archive-' + name]
            if digest(path) != receipt['sha256']:
                raise RuntimeError('Signed original bytes changed')
            verified = subprocess.run([str(pinned['server_binary']), '--verify-backup', str(path), '--trusted-backup-key', str(trusted)], cwd=REPO, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=60)
            if verified.returncode != 0:
                raise RuntimeError('Independent signature/inventory verification refused')
            wire = json.loads(path.read_bytes())
            payloads = [base64.b64decode(entry['bytesBase64'], validate=True) for entry in wire['entries']]
            payloads.append(base64.b64decode(wire['manifestBase64'], validate=True))
            for raw in payloads:
                if any(secret.encode() in raw for secret in [scanner_token, bearer, config['authMail']['idempotencySecret'], config['cookieKey'], config['mfaKey']]):
                    raise RuntimeError('Private operator credential found in signed inventory')
            entries = [entry for entry in wire['entries'] if entry['path'] == 'synthetic/data/ledger.sqlite']
            if len(entries) != 1:
                raise RuntimeError('Exact signed company SQLite snapshot required')
            raw = base64.b64decode(entries[0]['bytesBase64'], validate=True)
            if not 0 < len(raw) <= 32 * 1024 * 1024:
                raise RuntimeError('Bounded owned signed ledger required')
            copy = work / ('readonly-scan-' + name + '.sqlite')
            try:
                with copy.open('xb') as output:
                    output.write(raw)
                db = sqlite3.connect(copy.as_uri() + '?mode=ro', uri=True)
                try:
                    db.execute('PRAGMA query_only=ON')
                    result = {}
                    for table in ['documents', 'document_scan_evidence', 'audit_log', 'journal_entries', 'journal_lines']:
                        cursor = db.execute('SELECT * FROM ' + table + ' ORDER BY id')
                        result[table] = [dict(zip([c[0] for c in cursor.description], row)) for row in cursor.fetchall()]
                finally:
                    db.close()
            finally:
                copy.unlink(missing_ok=True)
            if len(result['documents']) != 4 or len(result['document_scan_evidence']) != 3 or result['journal_entries'] or result['journal_lines']:
                raise RuntimeError('Exact four originals / three clean evidence / zero financial effects required')
            docs = {row['id']: row for row in result['documents']}
            for document in docs.values():
                matches = [entry for entry in wire['entries'] if entry['path'] == 'synthetic/' + document['stored_path']]
                if len(matches) != 1 or hashlib.sha256(base64.b64decode(matches[0]['bytesBase64'], validate=True)).hexdigest() != document['sha256_hash']:
                    raise RuntimeError('Retained signed original bytes do not match canonical document')
            if len([row for row in result['audit_log'] if row['event_type'] == 'document_ingest']) != 4:
                raise RuntimeError('Exactly one immutable ingestion audit per accepted original required')
            historical = [row for row in docs.values() if row['original_filename'] == 'synthetic-scanner-historical.txt']
            if len(historical) != 1 or any(row['document_id'] == historical[0]['id'] for row in result['document_scan_evidence']):
                raise RuntimeError('Historical scanner-off original must remain without fabricated evidence')
            for row in result['document_scan_evidence']:
                document = docs.get(row['document_id'])
                if not document or row['sha256_hash'] != document['sha256_hash'] or row['scanner_id'] != 'Synthetic Scanner' or row['scanner_version'] != '65.1' or row['result'] != 'clean' or row['evidence_ref'] != 'synthetic:' + row['sha256_hash'] or not row['scanned_at']:
                    raise RuntimeError('Exact canonical normalized clean scan evidence missing')
            report.setdefault('snapshots', {})[name] = {'sha256': receipt['sha256'], 'trustedPublicKeySha256': digest(trusted), 'operatorCredentialsExcluded': True, 'tables': result}
            save()
            return result

        def preserved(original, newer):
            for table in ['documents', 'document_scan_evidence', 'journal_entries', 'journal_lines']:
                if original[table] != newer[table]:
                    raise RuntimeError('Restored exact immutable evidence differs: ' + table)
            retained = {row['id']: row for row in newer['audit_log']}
            if any(retained.get(row['id']) != row for row in original['audit_log']):
                raise RuntimeError('Original company audit must remain exact historical evidence')

        def invalid_config_checks():
            for index, scanner in enumerate([{'policy': 'required'}, {'policy': 'off', 'provider': 'http-json-v1'}, {'policy': 'required', 'provider': 'http-json-v1', 'url': 'http://127.0.0.1:1', 'bearerToken': scanner_token}]):
                progress('operator:invalid-required-config-startup-refusal-' + str(index))
                bad = {**config, 'root': str(work / ('invalid-workspace-' + str(index))), 'documentScanner': scanner}
                path = work / ('invalid-config-' + str(index) + '.json')
                path.write_text(json.dumps(bad))
                with (work / ('invalid-config-' + str(index) + '.log')).open('wb') as output:
                    result = subprocess.run([str(pinned['server_binary']), '--config', str(path), '--web-dir', str(web), '--listen', f'127.0.0.1:{backend_port}', '--origin', origin, '--create'], cwd=REPO, stdin=subprocess.DEVNULL, stdout=output, stderr=output, timeout=15)
                if result.returncode == 0 or Path(bad['root']).exists():
                    raise RuntimeError('Invalid scanner config did not refuse before workspace creation')
            report['invalidScannerConfigurationStartupRefused'] = True
            save()

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
            if args.native_cli_candidate:
                revision = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=REPO, text=True).strip()
                candidate = work / 'candidate'
                packaging = REPO / 'scripts/release/assemble-native-cli.py'
                packaging_hash = digest(packaging)
                subprocess.run([sys.executable, str(packaging), '--cli-binary', str(pinned['cli_binary']), '--mcp-binary', str(pinned['mcp_binary']), '--server-binary', str(pinned['server_binary']), '--web-dir', str(web), '--output', str(candidate), '--revision', revision], check=True, timeout=60, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
                manifest_path = candidate / 'native-cli-candidate.json'
                manifest = json.loads(manifest_path.read_bytes())
                expected = {'bin/rentemester': pinned['cli_binary'], 'bin/rentemester-rust': pinned['cli_binary'], 'bin/rentemester-mcp': pinned['mcp_binary'], 'bin/rentemester-server': pinned['server_binary']}
                expected.update({'web/'+name: web/name for name in report['webFiles']})
                inventory = [{'path':name,'size':source.stat().st_size,'sha256':digest(source),'executable':bool(source.stat().st_mode & 0o111)} for name,source in sorted(expected.items())]
                if manifest != {'format':'rentemester-native-cli-candidate/1','sourceRevision':revision,'candidateOnly':True,'files':inventory}:
                    raise RuntimeError('Candidate exact manifest differs')
                def verify_candidate():
                    files = {str(path.relative_to(candidate)) for path in candidate.rglob('*') if path.is_file()}
                    if files != set(expected) | {'native-cli-candidate.json'} or digest(manifest_path) != manifest_hash or digest(packaging) != packaging_hash:
                        raise RuntimeError('Candidate inventory or packaging pin changed')
                    for name,source in expected.items():
                        target = candidate/name
                        if target.is_symlink() or not target.is_file() or digest(target) != digest(source) or target.stat().st_size != source.stat().st_size or bool(target.stat().st_mode & 0o111) != bool(source.stat().st_mode & 0o111) or (target.stat().st_dev,target.stat().st_ino) == (source.stat().st_dev,source.stat().st_ino):
                            raise RuntimeError('Candidate independent bytes/mode differ')
                manifest_hash = digest(manifest_path)
                verify_candidate()
                report['nativeCliCandidate'] = {'manifest':manifest,'manifestSha256':manifest_hash,'packagingScriptSha256':packaging_hash,'path':str(candidate),'sourceByteEquality':True,'privateCandidateOnly':True}
                pinned = {'cli_binary':candidate/'bin/rentemester','mcp_binary':candidate/'bin/rentemester-mcp','server_binary':candidate/'bin/rentemester-server'}
                web = candidate/'web'
                (work/'empty-path').mkdir(mode=0o700)
            progress('operator:private-auth-gateway-trust-and-backup-signer')
            ca, cert, private = work / 'ca.pem', work / 'tls.pem', work / 'tls.key'
            (work / 'tls.ext').write_text('basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\nsubjectAltName=DNS:localhost,IP:127.0.0.1\n')
            commands = [
                ['openssl', 'req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', str(work / 'ca.key'), '-out', str(ca), '-days', '1', '-subj', '/CN=Synthetic Document Scanner Auth CA', '-addext', 'basicConstraints=critical,CA:TRUE', '-addext', 'keyUsage=critical,keyCertSign,cRLSign'],
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
            config = {'runtimeProfile': 'local-user-macos', 'root': str(work / 'workspace'), 'brokerUid': os.getuid(), 'workspace': 'synthetic-native-document-scanner', 'companySlug': 'synthetic', 'companyName': 'Synthetic Document Scanner Company', 'cookieKey': key(), 'mfaKey': key(), 'operatorActors': ['system:synthetic-installer'], 'accounts': [], 'firstOwner': {'actor': 'system:synthetic-installer', 'email': 'owner@example.invalid', 'name': 'Synthetic Owner', 'password': 'synthetic-owner-password-123'}, 'backupSigningKeyFile': str(work / 'signing.der'), 'authMail': {'endpoint': f'https://127.0.0.1:{mail_port}/mail', 'bearerToken': bearer, 'from': 'auth@example.invalid', 'idempotencySecret': secrets.token_urlsafe(32), 'trustedCaFile': str(ca)}}
            if args.backup_destinations:
                (work / 'offsite').mkdir(mode=0o700)
                config['backupDestinations']=[{'id':'synthetic-destination','label':'Synthetic folder','kind':'local-folder','directory':str(work/'offsite'),'nonRelatedParty':False,'inEeaOrEu':False,'country':'DK','meetsRecognisedStandards':False,'attestedBy':'Synthetic operator','attestedAt':'2026-01-01T00:00:00Z'}]
            if args.remote_backup:
                (work / 'backup-destination').mkdir(mode=0o700)
                (work / 'drive-trace.json').write_text('[]')
                config['backupDestinations'] = [{'id':'synthetic-drive','label':'Synthetic Drive fixture','kind':'google-drive','directory':str(work/'backup-destination'),'nonRelatedParty':False,'inEeaOrEu':False,'country':'DK','meetsRecognisedStandards':False,'attestedBy':'Synthetic operator','attestedAt':'2026-01-01T00:00:00Z'}]
                config['backupRemoteVerification'] = {'provider':'google-drive','url':f'https://127.0.0.1:{mail_port}/drive/v3','bearerToken':drive_token,'trustedCaFile':str(ca),'destinations':[{'destinationId':'synthetic-drive','folderId':'synthetic-drive-folder'}]}
            if not args.service_purchase and not args.backup_lock and not args.journal_history and not args.dated_reports and not args.dashboard_json and not args.portfolio_overview and not args.annual_exports and not args.authority_export and not args.saft_artifact and not (args.local_runtime_join or args.local_runtime_mcp) and not args.bank_queries and not args.period_original and not args.journal_original and not args.journal_mcp_original and not args.period_mcp_original and not args.exceptions_original:
                invalid_config_checks()
            write_config()
            start(create=True)
            if args.exceptions_original:
                stop()
                ledger = work / 'workspace/synthetic/data/ledger.sqlite'
                if not owned() or ledger.is_symlink() or not ledger.is_file():
                    raise RuntimeError('Owned initialized synthetic exception ledger required')
                with sqlite3.connect(ledger) as fixture:
                    if fixture.execute('SELECT count(*) FROM exceptions').fetchone()[0]:
                        raise RuntimeError('Original exception synthetic namespace must be empty')
                    # Only two explicit undated operational facts. Never seed auth,
                    # sessions, grants, review/decision/audit or waiver/proof controls.
                    for exception_id in [1, 2]:
                        fixture.execute("INSERT INTO exceptions(id,type,severity,status,message,required_action,source_evidence,posting_preview,resolution_key) VALUES(?,'SYNTHETIC_ORIGINAL_REVIEW','low','open',?,?,?,?,?)", (exception_id, 'Synthetic original exception '+str(exception_id), 'Record review only; posting remains separate', json.dumps({'source':'synthetic-original-exceptions163','underlyingInvariantSatisfied':False,'fixtureId':exception_id}), json.dumps({'mustPostSeparately':True,'fixtureId':exception_id}), 'synthetic-original-exceptions163-'+str(exception_id)))
                    fixture.row_factory = sqlite3.Row
                    report['exceptionOriginalSeedFacts'] = [dict(r) for r in fixture.execute('SELECT * FROM exceptions ORDER BY id')]
                report['operationalFixture'] = {'synthetic':True,'exceptionIds':[1,2],'undated':True,'bankRowsSeeded':0,'exceptionCreationUiQualified':False,'beforePrivateLedgerRestart':True}
                start()
            if args.period_mcp_original:
                stop()
                ledger = work / 'workspace/synthetic/data/ledger.sqlite'
                if not owned() or ledger.is_symlink() or not ledger.is_file():
                    raise RuntimeError('Owned initialized synthetic ledger required')
                with sqlite3.connect(ledger) as fixture:
                    # Bounded operational fixture only: no identities, grants, reviews,
                    # packet/decision/waiver/audit controls are seeded or modified.
                    if fixture.execute('SELECT count(*) FROM bank_transactions').fetchone()[0] or fixture.execute('SELECT count(*) FROM exceptions').fetchone()[0]:
                        raise RuntimeError('Synthetic operational fixture namespace must be empty')
                    # Independent synthetic statement closing balance is an input fact,
                    # never inferred from the ledger. Missing balance must remain a blocker.
                    fixture.execute("INSERT INTO bank_transactions(id,transaction_date,text,amount,currency,status,balance_after) VALUES(162,'2001-02-01','Explicit synthetic period operational fixture',1.25,'DKK','imported',0)")
                    fixture.execute("INSERT INTO exceptions(id,type,severity,status,related_bank_transaction_id,message,required_action) VALUES(162,'synthetic-period-follow-up','medium','open',162,'Explicit synthetic period force proof only','Owner follow-up remains open')")
                report['operationalFixture'] = {'synthetic':True,'bankId':162,'exceptionId':162,'exceptionCreationUiQualified':False,'beforePrivateLedgerRestart':True,'independentSyntheticClosingBalance':0}
                start()
            script = work / 'browser.mjs'
            script.write_text(BROWSER)
            controller = subprocess.Popen(['/opt/homebrew/bin/bun', str(script)], cwd=REPO, env={**os.environ, 'RM_IMPORTED_PORTFOLIO_HELPER': str(REPO / 'scripts/rust/check-native-imported-portfolio.ts'), 'RM_SYNTHETIC_IMPORT_HELPER': str(REPO / 'scripts/rust/check-native-synthetic-import.ts'), 'RM_DINERO_IMPORT_HELPER': str(REPO / 'scripts/rust/check-native-dinero-import.ts'), 'RM_INVOICE_BAD_DEBT_HELPER': str(REPO / 'scripts/rust/check-native-invoice-bad-debt.ts'), 'RM_INVOICE_INTEREST_HELPER': str(REPO / 'scripts/rust/check-native-invoice-interest.ts'), 'RM_INVOICE_COMPENSATION_HELPER': str(REPO / 'scripts/rust/check-native-invoice-compensation.ts'), 'RM_NATIVE_BACKUP_DESTINATION_PROOF':'1' if args.backup_destinations else '0', 'RM_NATIVE_REMOTE_BACKUP_PROOF':'1' if args.remote_backup else '0', 'RM_AUDIT_VERIFY_HELPER': str(REPO / 'scripts/rust/check-native-audit-verify.ts'), 'RM_AUDIT_HELPER': str(REPO / 'scripts/rust/check-native-audit-log.ts'), 'RM_BACKUP_RECOVERY_HELPER': str(REPO / 'scripts/rust/check-native-backup-recovery.ts'), 'RM_SERVICE_PURCHASE_HELPER': str(REPO / ('scripts/rust/check-native-privacy708.ts' if args.privacy_compliance else 'scripts/rust/check-native-knowledge-scope705.ts' if args.knowledge_scope else 'scripts/rust/check-native-exceptions-original.ts' if args.exceptions_original else 'scripts/rust/check-native-period-mcp.ts' if args.period_mcp_original else 'scripts/rust/check-native-journal-mcp.ts' if args.journal_mcp_original else 'scripts/rust/check-native-period-original.ts' if (args.period_original or args.journal_original or args.journal_mcp_original) else 'scripts/rust/check-native-bank-queries.ts' if args.bank_queries else 'scripts/rust/check-native-local-runtime.ts' if (args.local_runtime_join or args.local_runtime_mcp) else 'scripts/rust/check-native-saft.ts' if args.saft_artifact else 'scripts/rust/check-native-accountant.ts' if args.authority_export else 'scripts/rust/check-native-annual-clients.ts' if args.annual_exports else 'scripts/rust/check-native-dashboard-json.ts' if args.dashboard_json or args.portfolio_overview else 'scripts/rust/check-native-dated-reports.ts' if args.dated_reports else 'scripts/rust/check-native-journal-history.ts' if args.journal_history else 'scripts/rust/check-native-backup-lock.ts' if args.backup_lock else 'scripts/rust/check-native-service-purchase.ts')), 'RM_SCANNER_PROOF_FACTORY': 'createPrivacyComplianceProof' if args.privacy_compliance else 'createKnowledgeScopeProof' if args.knowledge_scope else 'createOriginalExceptionsProof' if args.exceptions_original else 'createOriginalPeriodMcpProof' if args.period_mcp_original else 'createOriginalJournalMcpProof' if args.journal_mcp_original else 'createOriginalJournalProof' if args.journal_original else 'createOriginalPeriodProof' if args.period_original else 'createBankQueriesProof' if args.bank_queries else 'createLocalRuntimeMcpProof' if args.local_runtime_mcp else 'createLocalRuntimeProof' if (args.local_runtime_join or args.local_runtime_mcp) else 'createSaftArtifactProof' if args.saft_artifact else 'createAuthorityExportProof' if args.authority_export else 'createAnnualExportsProof' if args.annual_exports else 'createPortfolioOverviewProof' if args.portfolio_overview else 'createDashboardJsonProof' if args.dashboard_json else 'createDatedReportsProof' if args.dated_reports else 'createJournalHistoryProof' if args.journal_history else 'createBackupLockProof' if args.backup_lock else 'createServicePurchaseProof', 'RM_NATIVE_CLI_CANDIDATE': '1' if args.native_cli_candidate else '0', 'RM_SCANNER_BASE': origin, 'RM_SCANNER_WORK': str(work), 'RM_SCANNER_SCREENSHOT': str(args.evidence.with_suffix('.png')), 'RM_SCANNER_PLAYWRIGHT': str((REPO / 'node_modules/@playwright/test/index.mjs').resolve(strict=True))}, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, bufsize=1, start_new_session=True)
            report['browserControllerPid'] = controller.pid
            phase('authenticate')
            if args.exceptions_original:
                sources = [REPO / p for p in ['scripts/rust/check-native-exceptions-original.ts','src/cli/exceptions.ts','src/cli-meta/master-data-specs.ts','src/mcp/tools/exceptions.ts','src/core/exceptions.ts','src/mcp/security.ts','src/mcp/tool-runtime.ts','src/mcp/envelope.ts']]
                hashes = {str(p.relative_to(REPO)):digest(p) for p in sources}
                def exceptions_sql(actual):
                    ledger = work / 'workspace/synthetic/data/ledger.sqlite'
                    if not owned() or ledger.is_symlink() or not ledger.is_file():
                        raise RuntimeError('Owned original exception ledger missing')
                    with sqlite3.connect(ledger.as_uri()+'?mode=ro', uri=True) as db:
                        db.row_factory = sqlite3.Row
                        facts = {t:[dict(r) for r in db.execute('SELECT * FROM '+t+' ORDER BY id')] for t in ['exceptions','audit_log','journal_entries','journal_lines','bank_transactions','documents','period_close_open_items','period_close_decisions','period_close_reviews','accounting_periods']}
                    if any(facts[t] for t in ['journal_entries','journal_lines','bank_transactions','documents','period_close_open_items','period_close_decisions','period_close_reviews','accounting_periods']):
                        raise RuntimeError('Review-only original exceptions changed accounting/period controls')
                    rows = facts['exceptions']
                    if len(rows) != 2:
                        raise RuntimeError('Exactly two synthetic original exception facts required')
                    mutable = {'status','resolved_at','resolved_by','resolution_note'}
                    expected_actors = ['user:'+actual['ownerId'], 'service-account:'+actual['serviceSubjectId']]
                    audits = facts['audit_log']
                    if len(audits) != 2 or [a['event_type'] for a in audits] != ['exception_resolve','exception_resolve']:
                        raise RuntimeError('Exactly two immutable resolution audits required; no refusal/no-op audits')
                    for index, row in enumerate(rows):
                        seed = report['exceptionOriginalSeedFacts'][index]
                        if {k:v for k,v in row.items() if k not in mutable} != {k:v for k,v in seed.items() if k not in mutable}:
                            raise RuntimeError('Original immutable source facts changed')
                        if row['status'] != 'resolved' or not row['resolved_at'] or row['resolved_by'] != expected_actors[index] or row['resolution_note'] != (actual['note'] if index == 0 else None):
                            raise RuntimeError('Current human/service actor, first timestamp/note differs')
                        canonical = actual['stable']['exceptions']['rows'][1-index]
                        if (canonical['resolvedAt'],canonical['resolvedBy'],canonical['resolutionNote']) != (row['resolved_at'],row['resolved_by'],row['resolution_note']):
                            raise RuntimeError('Independent SQL first resolution differs from canonical result')
                        audit = audits[index]
                        detail = json.loads(audit['message'])
                        if audit['entity_type'] != 'exception' or audit['entity_id'] != str(row['id']) or audit['actor'] != expected_actors[index] or detail.get('exceptionId') != row['id'] or detail.get('resolvedBy') != expected_actors[index] or detail.get('note') != row['resolution_note'] or detail.get('effect') != 'review_only_no_posting':
                            raise RuntimeError('Independent exact resolution audit actor/note/effect mismatch')
                        if detail.get('type') != row['type'] or detail.get('message') != row['message'] or detail.get('requiredAction') != row['required_action']:
                            raise RuntimeError('Resolution audit original source facts mismatch')
                    return facts
                phase('service-run')
                actual = report['results']['service-run']
                if not actual.get('originalExceptionsQualified'):
                    raise RuntimeError('Finite original exceptions journey missing')
                stop()
                first = exceptions_sql(actual)
                report['exceptionsOriginalSqlBeforeRestart'] = first
                config.pop('firstOwner'); write_config(); start()
                phase('service-reopen')
                reopened = report['results']['service-reopen']
                stop()
                second = exceptions_sql(actual)
                if first != second or not reopened.get('exactNormalMfaRestartState') or not reopened.get('originalExceptionsRestartQualified') or actual['accountingFingerprint'] != reopened['accountingFingerprint']:
                    raise RuntimeError('Exact SQL and ordinary MFA original exceptions restart mismatch')
                if any(digest(p) != hashes[str(p.relative_to(REPO))] for p in sources):
                    raise RuntimeError('Original exception pinned source changed')
                if any(digest(path) != report['artifacts'][name] or digest(getattr(args,name)) != report['artifacts'][name] for name,path in pinned.items()):
                    raise RuntimeError('Original exception pinned binaries changed')
                if {str(path.relative_to(args.web_dir)):digest(path) for path in sorted(args.web_dir.rglob('*')) if path.is_file()} != report['webFiles']:
                    raise RuntimeError('Original exception pinned web artifacts changed')
                report['originalSourceHashes'] = hashes
                report['exceptionsOriginalSqlAfterRestart'] = second
                report['waiverCoverage'] = 'No actual waiver obligation fixture or flow exercised here; backend focused storage unit proof is separate.'
                phase('finish',20)
                controller.stdin.close(); controller.wait(timeout=20)
                if controller.returncode != 0:
                    raise RuntimeError('Original exceptions browser controller failed')
                report.update(status='passed',currentAction='complete',workflow='original_exceptions_cli_mcp_review_only_current_service_restart',exceptionsOriginalQualified=True,exceptionsOriginalRestartQualified=True,installedNativeQualified=False,sourceFreeRestoreQualified=False,externalFilingQualified=False,exceptionCreationUiQualified=False,waiverObligationJourneyQualified=False)
                save()
                return
            if args.period_mcp_original:
                helper = REPO / 'scripts/rust/check-native-period-mcp.ts'
                sources = [helper, REPO / 'scripts/rust/check-native-period-original.ts'] + [REPO / p for p in ['src/mcp/tools/period.ts','src/mcp/security.ts','src/mcp/tool-runtime.ts','src/mcp/envelope.ts','src/core/periods.ts','src/core/period-close-readiness.ts']]
                hashes = {str(p.relative_to(REPO)):digest(p) for p in sources}
                def period_sql(actual):
                    ledger = work / 'workspace/synthetic/data/ledger.sqlite'
                    if not owned() or ledger.is_symlink() or not ledger.is_file():
                        raise RuntimeError('Owned period MCP ledger missing')
                    with sqlite3.connect(ledger.as_uri()+'?mode=ro', uri=True) as db:
                        db.row_factory = sqlite3.Row
                        db.execute('PRAGMA query_only=ON')
                        facts = {t:[dict(r) for r in db.execute('SELECT * FROM '+t+' ORDER BY id')] for t in ['accounting_periods','period_close_readiness_packets','period_close_reviews','period_close_decisions','period_close_open_items','exceptions','bank_transactions','journal_entries','journal_lines','audit_log']}
                        counts = {t:db.execute('SELECT count(*) FROM '+t).fetchone()[0] for t in ['documents','bank_transactions','journal_entries','journal_lines']}
                        if counts != {'documents':0,'bank_transactions':1,'journal_entries':2,'journal_lines':4}:
                            raise RuntimeError('Period MCP unexpected financial effects')
                        if len(facts['exceptions']) != 1 or facts['exceptions'][0]['id'] != 162 or facts['exceptions'][0]['status'] != 'open' or facts['exceptions'][0]['resolved_at'] is not None:
                            raise RuntimeError('Original operational exception must remain open')
                        raw_list = [dict(r) for r in db.execute('SELECT id,period_start AS periodStart,period_end AS periodEnd,kind,status,reference,created_at AS createdAt FROM accounting_periods WHERE id != ? ORDER BY period_start DESC,id DESC',(actual['forced']['periodId'],))]
                        if actual['listed'] != {'periods':raw_list,'count':len(raw_list)}:
                            raise RuntimeError('Original raw stored list DTO/order/reference mismatch')
                        subject = actual['serviceSubjectId']
                        packets = {p['packet_hash']:json.loads(p['packet_json']) for p in facts['period_close_readiness_packets']}
                        for saved in actual['reviews']:
                            stored = next((r for r in facts['period_close_reviews'] if r['id'] == saved['id']), None)
                            packet = packets.get(saved['packet']['hash'])
                            if not stored or packet != saved['packet'] or json.loads(stored['reviewer_principal']) != {'kind':'service-account','subjectId':subject} or stored['reviewer_actor'] != saved['reviewerActor']:
                                raise RuntimeError('Exact service review/packet independent SQL mismatch')
                            body = {k:v for k,v in packet.items() if k != 'hash'}
                            canonical = lambda v: json.dumps(v,sort_keys=True,separators=(',',':'),ensure_ascii=False)
                            if packet['version'] != 4 or packet['format'] != 'rentemester.native-period-close.v1' or hashlib.sha256(canonical(body).encode()).hexdigest() != packet['hash'] or not all(isinstance(packet['sourceRevision'][k],list) for k in ['journal','journalCount','audit']):
                                raise RuntimeError('Exact real native4 packet/hash/revision mismatch')
                        reviewed = actual['reviews'][-1]
                        decisions = [d for d in facts['period_close_decisions'] if d['decision'] == 'forced_closed']
                        if len(decisions) != 1:
                            raise RuntimeError('Exactly one actual forced decision required')
                        decision = decisions[0]
                        if decision['period_id'] != actual['forced']['periodId'] or decision['packet_hash'] != reviewed['packet']['hash'] or decision['reason'] != actual['reason'] or decision['actor'] != reviewed['reviewerActor']:
                            raise RuntimeError('Forced decision exact packet/reason/actor mismatch')
                        blocked = [i for i in reviewed['packet']['items'] if i['status'] == 'blocked']
                        obligations = facts['period_close_open_items']
                        if not blocked or len(obligations) != len(blocked) or any(not i['waivable'] for i in blocked):
                            raise RuntimeError('Every and only waived blocked control requires an obligation')
                        for item in blocked:
                            matches = [o for o in obligations if o['code'] == item['code']]
                            if len(matches) != 1:
                                raise RuntimeError('Missing exact waived control')
                            o = matches[0]
                            expected = {'decision_id':decision['id'],'period_id':decision['period_id'],'packet_hash':decision['packet_hash'],'code':item['code'],'severity':'blocker','count':item['count'],'amount':item['amount'],'reason':actual['reason'],'actor':decision['actor']}
                            if any(o[k] != v for k,v in expected.items()) or o['evidence_json'] != canonical(item['evidence']):
                                raise RuntimeError('Exact durable obligation evidence/reason/actor mismatch')
                        if len([a for a in facts['audit_log'] if a['event_type'] == 'PERIOD_CLOSED_WITH_OPEN_ITEMS']) != 1:
                            raise RuntimeError('Forced open-items audit required exactly once')
                        return facts
                phase('service-run')
                actual = report['results']['service-run']
                if not actual.get('qualified') or not actual.get('originalPeriodMcpQualified'):
                    raise RuntimeError('Original period MCP whole journey missing')
                stop()
                first = period_sql(actual)
                report['periodMcpSqlBeforeRestart'] = first
                config.pop('firstOwner'); write_config(); start()
                phase('service-reopen')
                reopened = report['results']['service-reopen']
                stop()
                second = period_sql(actual)
                if first != second or not reopened.get('exactNormalMfaRestartState') or not reopened.get('originalPeriodMcpRestartQualified') or actual['accountingFingerprint'] != reopened['accountingFingerprint']:
                    raise RuntimeError('Exact historical SQL and ordinary MFA period restart mismatch')
                if any(digest(p) != hashes[str(p.relative_to(REPO))] for p in sources):
                    raise RuntimeError('Period MCP pinned source changed')
                if any(digest(path) != report['artifacts'][name] or digest(getattr(args,name)) != report['artifacts'][name] for name,path in pinned.items()):
                    raise RuntimeError('Period MCP pinned binaries changed')
                if {str(path.relative_to(args.web_dir)):digest(path) for path in sorted(args.web_dir.rglob('*')) if path.is_file()} != report['webFiles']:
                    raise RuntimeError('Period MCP pinned web artifacts changed')
                report['originalSourceHashes'] = hashes
                report['periodMcpSqlAfterRestart'] = second
                phase('finish',20)
                controller.stdin.close(); controller.wait(timeout=20)
                if controller.returncode != 0:
                    raise RuntimeError('Period MCP browser controller failed')
                report.update(status='passed',currentAction='complete',workflow='original_period_mcp_owner_service_forced_open_obligations_restart',periodMcpOriginalQualified=True,periodMcpOriginalRestartQualified=True,installedNativeQualified=False,sourceFreeRestoreQualified=False,externalFilingQualified=False,exceptionCreationUiQualified=False)
                save()
                return
            if args.journal_mcp_original or args.journal_original or args.period_original or args.bank_queries or args.journal_history or args.dated_reports or args.dashboard_json or args.portfolio_overview or args.annual_exports or args.authority_export or args.saft_artifact or (args.local_runtime_join or args.local_runtime_mcp):
                helper = REPO / ('scripts/rust/check-native-privacy708.ts' if args.privacy_compliance else 'scripts/rust/check-native-knowledge-scope705.ts' if args.knowledge_scope else 'scripts/rust/check-native-period-mcp.ts' if args.period_mcp_original else 'scripts/rust/check-native-journal-mcp.ts' if args.journal_mcp_original else 'scripts/rust/check-native-period-original.ts' if (args.period_original or args.journal_original or args.journal_mcp_original) else 'scripts/rust/check-native-bank-queries.ts' if args.bank_queries else 'scripts/rust/check-native-local-runtime.ts' if (args.local_runtime_join or args.local_runtime_mcp) else 'scripts/rust/check-native-saft.ts' if args.saft_artifact else 'scripts/rust/check-native-accountant.ts' if args.authority_export else 'scripts/rust/check-native-annual-clients.ts' if args.annual_exports else 'scripts/rust/check-native-dashboard-json.ts' if args.dashboard_json or args.portfolio_overview else 'scripts/rust/check-native-dated-reports.ts' if args.dated_reports else 'scripts/rust/check-native-journal-history.ts')
                helper_key = 'journalMcpOriginalHelperSha256' if args.journal_mcp_original else 'journalOriginalHelperSha256' if args.journal_original else 'periodOriginalHelperSha256' if args.period_original else 'bankQueriesHelperSha256' if args.bank_queries else 'localRuntimeMcpHelperSha256' if args.local_runtime_mcp else 'localRuntimeHelperSha256' if (args.local_runtime_join or args.local_runtime_mcp) else 'saftArtifactHelperSha256' if args.saft_artifact else 'authorityExportHelperSha256' if args.authority_export else 'annualExportsHelperSha256' if args.annual_exports else 'portfolioOverviewHelperSha256' if args.portfolio_overview else 'dashboardJsonHelperSha256' if args.dashboard_json else 'datedReportsHelperSha256' if args.dated_reports else 'journalHistoryHelperSha256'
                report[helper_key] = digest(helper)
                reference_files = [REPO / path for path in ['src/mcp/tools/journal.ts','src/mcp/tool-runtime.ts','src/mcp/envelope.ts','src/core/idempotency.ts','scripts/rust/check-native-period-original.ts']] if args.journal_mcp_original else [REPO / path for path in ['src/cli/journal.ts','src/cli-meta/journal-specs.ts','src/core/ledger.ts','src/cli/period.ts','src/core/period-close-readiness.ts']] if args.journal_original else [REPO / path for path in ['src/cli/period.ts','src/cli-meta/period-specs.ts','src/core/period-close-readiness.ts','src/core/periods.ts']] if args.period_original else [REPO / ('src/core/' + name + '.ts') for name in ['saft-export','tar']] if args.saft_artifact else [REPO / ('src/core/' + name + '.ts') for name in ['authority-export','tar']] if args.authority_export else [REPO / ('src/core/' + name + '.ts') for name in ['tax-return','ixbrl','annual-report']] if args.annual_exports else [REPO / ('src/core/' + name + '.ts') for name in ['financial-statements','account-classification','dates','money']] if args.dated_reports else []
                if args.native_cli_candidate:
                    reference_files += [REPO/path for path in ['src/cli/vendor.ts','src/core/master-data.ts','src/mcp/tools/vendor.ts','scripts/rust/check-native-master-data-original.ts']]
                if args.privacy_compliance:
                    reference_files += [REPO/'scripts/rust/check-native-local-runtime.ts', REPO/'src/core/gdpr.ts', REPO/'src/core/regulatory-coverage.ts']
                if args.knowledge_scope:
                    reference_files += [REPO / 'scripts/rust/check-native-local-runtime.ts', REPO / 'scripts/rust/check-native-company-knowledge.ts']
                reference_hashes = {str(path.relative_to(REPO)):digest(path) for path in reference_files}
                phase('service-run')
                if digest(helper) != report[helper_key]:
                    raise RuntimeError('Standalone report helper changed during execution')
                stop()
                if (args.local_runtime_join or args.local_runtime_mcp) and any(os.path.lexists(work/name) for name in ['r.json','r.sock']):
                    raise RuntimeError('Owned local runtime endpoints not retired on normal stop')
                config.pop('firstOwner'); write_config(); start()
                phase('service-reopen')
                actual, reopened = report['results']['service-run'], report['results']['service-reopen']
                if not actual.get('qualified') or not reopened.get('qualified') or not reopened.get('exactNormalMfaRestartState') or actual['accountingFingerprint'] != reopened['accountingFingerprint']:
                    raise RuntimeError('Standalone report exact ordinary-MFA restart not qualified')
                stop()
                ledger = work / 'workspace/synthetic/data/ledger.sqlite'
                if not owned() or ledger.is_symlink() or not ledger.is_file():
                    raise RuntimeError('Owned journal history ledger missing')
                db = sqlite3.connect(ledger.as_uri()+'?mode=ro', uri=True)
                try:
                    db.execute('PRAGMA query_only=ON')
                    counts = {table:db.execute('SELECT COUNT(*) FROM '+table).fetchone()[0] for table in ['documents','bank_transactions','journal_entries','journal_lines']}
                    expected_counts = {'documents':2,'bank_transactions':0,'journal_entries':1,'journal_lines':2} if args.privacy_compliance else {'documents':0,'bank_transactions':0,'journal_entries':2,'journal_lines':4} if (args.period_original or args.journal_original or args.journal_mcp_original) else {'documents':0,'bank_transactions':3,'journal_entries':3,'journal_lines':6} if args.bank_queries else {'documents':1,'bank_transactions':0,'journal_entries':1,'journal_lines':2} if args.authority_export or args.saft_artifact or (args.local_runtime_join or args.local_runtime_mcp) else {'documents':2,'bank_transactions':0,'journal_entries':2,'journal_lines':6} if args.annual_exports else {'documents':4,'bank_transactions':2,'journal_entries':3,'journal_lines':8} if args.dashboard_json or args.portfolio_overview else {'documents':2,'bank_transactions':0,'journal_entries':4,'journal_lines':8} if args.dated_reports else {'documents':0,'bank_transactions':0,'journal_entries':3,'journal_lines':6}
                    if counts != expected_counts:
                        raise RuntimeError('Standalone report independent terminal effects differ')
                    rows = db.execute('SELECT id,entry_no,transaction_date,status,reversal_of_entry_id FROM journal_entries ORDER BY id DESC').fetchall()
                    expected_status = ['reversed','posted','posted','posted'] if args.dated_reports else ['reversed','posted','posted']
                    correction_valid = (len(rows) == 2 and rows[0][3] == 'reversed' and rows[0][4] == rows[1][0] and rows[1][3] == 'posted' and rows[1][4] is None) if (args.journal_original or args.journal_mcp_original) else all(row[3] == 'posted' and row[4] is None for row in rows) if args.period_original or args.dashboard_json or args.portfolio_overview or args.annual_exports or args.authority_export or args.saft_artifact or (args.local_runtime_join or args.local_runtime_mcp) else [row[3] for row in rows] == expected_status and rows[0][4] == rows[2][0] and all(row[4] is None for row in rows[1:])
                    if not correction_valid:
                        raise RuntimeError('Standalone report stored correction status/link differs')
                    report['terminalSql'] = {'counts':counts,'headers':rows,'journalActors':db.execute('SELECT created_by,created_by_program FROM journal_entries ORDER BY id').fetchall()}
                    if args.journal_original or args.journal_mcp_original:
                        db.row_factory = sqlite3.Row
                        journal_evidence = actual['journalEvidence']
                        facts = {table:[dict(row) for row in db.execute('SELECT * FROM '+table+' ORDER BY '+('kind,scope' if table == 'sequences' else 'tombstone_id' if table == 'mutation_idempotency_outcomes' else 'id'))] for table in ['journal_entries','journal_lines','sequences','audit_log','exceptions','mutation_idempotency_tombstones','mutation_idempotency_outcomes']}
                        original, reversal = facts['journal_entries']
                        if original['currency'] != 'DKK' or reversal['currency'] != 'DKK' or original['created_by'] != 'user:'+report['results']['authenticate']['ownerId'] or reversal['created_by'] != original['created_by']:
                            raise RuntimeError('Original journal default currency/current principal attribution differs')
                        if (original['id'],original['entry_no'],original['entry_hash'],original['previous_hash']) != (journal_evidence['posted']['entryId'],journal_evidence['posted']['entryNo'],journal_evidence['posted']['entryHash'],journal_evidence['dryRun']['previousHash']):
                            raise RuntimeError('Original journal persisted provisional identity/hash differs')
                        if reversal['entry_hash'] != journal_evidence['reversal']['entryHash'] or reversal['previous_hash'] != original['entry_hash'] or reversal['reversal_of_entry_id'] != original['id']:
                            raise RuntimeError('Original journal reversal hash chain differs')
                        lines = facts['journal_lines']
                        if [(r['debit_amount'],r['credit_amount']) for r in lines] != [(3.25,0),(0,3.25),(0,3.25),(3.25,0)]:
                            raise RuntimeError('Original journal numeric account effects differ')
                        if [(r['account_id'],r['debit_amount'],r['credit_amount']) for r in lines[:2]] != [(r['account_id'],r['credit_amount'],r['debit_amount']) for r in lines[2:]]:
                            raise RuntimeError('Original journal reversal is not exact opposite lines')
                        if [r['value'] for r in facts['sequences'] if r['kind'] == 'journal_entry'] != [2] or facts['exceptions'] or (not args.journal_mcp_original and (facts['mutation_idempotency_tombstones'] or facts['mutation_idempotency_outcomes'])):
                            raise RuntimeError('Original unkeyed journal sequence/exception/receipt rollback differs')
                        if [r['event_type'] for r in facts['audit_log'] if r['event_type'] in ['journal_post','journal_reverse']] != ['journal_post','journal_reverse']:
                            raise RuntimeError('Original journal exact post/reversal audit differs')
                        packets = [dict(row) for row in db.execute('SELECT * FROM period_close_readiness_packets ORDER BY id')]
                        for observed in actual['reviews']:
                            packet = next(json.loads(row['packet_json']) for row in packets if row['packet_hash'] == observed['packet']['hash'])
                            if packet != observed['packet'] or packet['version'] != 4 or packet['format'] != 'rentemester.native-period-close.v1' or not all(isinstance(packet['sourceRevision'][key],list) for key in ['journal','journalCount','audit']):
                                raise RuntimeError('Original journal native4 persisted source arrays differ')
                        if args.journal_mcp_original:
                            receipts = facts['mutation_idempotency_tombstones']
                            outcomes = facts['mutation_idempotency_outcomes']
                            if len(receipts) != 2 or len(outcomes) != 2:
                                raise RuntimeError('Original MCP requires exactly two accepted receipts/outcomes; refusals created a receipt')
                            owner_id = report['results']['authenticate']['ownerId']
                            for stored, result_name, operation_name, explicit_key in zip(receipts, ['posted','reversal'], ['journal_post','journal_reverse'], [actual['explicitKeys']['post'],actual['explicitKeys']['reverse']]):
                                accepted = journal_evidence[result_name]
                                meta = accepted['idempotency']
                                stored_outcome = next(row for row in outcomes if row['tombstone_id'] == stored['id'])
                                decoded_outcome = json.loads(stored_outcome['outcome_json'])
                                expected_core = {key:value for key,value in accepted.items() if key != 'idempotency'}
                                if (stored['id'],stored['created_at'],stored['expires_at']) != (meta['receiptId'],meta['createdAt'],meta['expiresAt']) or decoded_outcome != expected_core or stored_outcome['created_at'] != meta['createdAt']:
                                    raise RuntimeError('Original MCP persisted exact receipt outcome/identity/timestamps differ')
                                if stored['principal_kind'] != 'user' or stored['principal_subject_id'] != owner_id or not stored['ledger_uuid'] or stored['operation'] != operation_name or stored['client_key_hash'] != hashlib.sha256(explicit_key.encode()).hexdigest() or len(stored['payload_hash']) != 64:
                                    raise RuntimeError('Original MCP current principal/ledger/operation/intent binding differs')
                            if receipts[0]['ledger_uuid'] != receipts[1]['ledger_uuid'] or receipts[0]['operation'] == receipts[1]['operation'] or receipts[0]['client_key_hash'] == receipts[1]['client_key_hash']:
                                raise RuntimeError('Original MCP distinct explicit key/operation identities missing')
                            if not reopened.get('receiptRestart',{}).get('sameReceiptOutcomes'):
                                raise RuntimeError('Original MCP both receipt replays after exact MFA restart missing')
                            report['terminalSql']['originalMcpReceipts'] = {'receipts':receipts,'outcomes':outcomes,'sameExactRestartOutcomes':True}
                        report['terminalSql'].update(journalFacts=facts, nativePackets=packets)
                        db.row_factory = None
                    if args.period_original or args.journal_mcp_original:
                        # Readonly post-stop evidence independent of the browser/client envelopes.
                        db.row_factory = sqlite3.Row
                        facts = {table:[dict(row) for row in db.execute('SELECT * FROM '+table+' ORDER BY id')] for table in ['accounting_periods','period_close_readiness_packets','period_close_reviews','period_close_decisions','period_close_open_items']}
                        if len(facts['accounting_periods']) != 3 or len(facts['period_close_reviews']) != 6 or len(facts['period_close_decisions']) != 6 or facts['period_close_open_items']:
                            raise RuntimeError('Original period independent review/decision/row counts differ; a refused write may have persisted')
                        if [row['decision'] for row in facts['period_close_decisions']] != ['closed','reopened','closed','closed','closed','closed']:
                            raise RuntimeError('Original period append-only decision sequence differs')
                        if len(facts['period_close_readiness_packets']) != len({row['packet_hash'] for row in facts['period_close_reviews']}):
                            raise RuntimeError('Original period unexpected persisted packet from rejected write')
                        for observed in actual['reviews']:
                            stored = next(row for row in facts['period_close_reviews'] if row['id'] == observed['id'])
                            packet = next(row for row in facts['period_close_readiness_packets'] if row['id'] == stored['packet_id'])
                            principal = json.loads(stored['reviewer_principal']) if stored['reviewer_principal'] is not None else None
                            reconstructed = {'id':stored['id'],'packet':json.loads(packet['packet_json']),'reviewerActor':stored['reviewer_actor'],'reviewerPrincipal':principal,'createdAt':stored['created_at']}
                            if reconstructed != observed or stored['packet_hash'] != observed['packet']['hash']:
                                raise RuntimeError('Original historical persisted review/principal/packet differs')
                        lifecycle = [dict(row) for row in db.execute("SELECT * FROM audit_log WHERE entity_type='accounting_period' ORDER BY id")]
                        if [row['event_type'] for row in lifecycle] != ['period_close','period_reopen','period_close','period_report','period_report','period_report']:
                            raise RuntimeError('Original period terminal atomic lifecycle audit differs')
                        report['terminalSql'].update(periodFacts=facts, lifecycleAudit=lifecycle)
                        db.row_factory = None
                    if args.bank_queries:
                        query_bank_rows = db.execute('SELECT id,transaction_date,text,amount,currency,amount_dkk,bank_account_id,source_file_hash FROM bank_transactions ORDER BY id').fetchall()
                        query_links = db.execute('SELECT bank_transaction_id,journal_entry_id FROM bank_journal_reconciliations ORDER BY bank_transaction_id').fetchall()
                        if query_links != [(actual['selected']['bankId'],actual['replacementId'])]:
                            raise RuntimeError('Original bank query correction current identity differs')
                        query_posts = db.execute("SELECT event_type,actor FROM audit_log WHERE event_type IN ('journal_post','journal_reverse') ORDER BY id").fetchall()
                        if [r[0] for r in query_posts] != ['journal_post','journal_post','journal_reverse']:
                            raise RuntimeError('Original bank query exact posting/reversal effects differ')
                        query_sources = []
                        for source in actual['sources']:
                            query_hash = source['sha256']
                            if len(query_hash) != 64 or any(c not in '0123456789abcdef' for c in query_hash):
                                raise RuntimeError('Bank original hash shape differs')
                            query_path = work/'workspace/synthetic/bank/processed'/(query_hash+'.txt')
                            if query_path.is_symlink() or not query_path.is_file() or digest(query_path) != query_hash or query_path.read_bytes() != base64.b64decode(source['bytesBase64'],validate=True):
                                raise RuntimeError('Retained bank query CSV original differs')
                            query_sources.append({'sha256':query_hash,'bytes':query_path.stat().st_size})
                        if len(query_sources) != 2 or len({r[7] for r in query_bank_rows}) != 2:
                            raise RuntimeError('Bank query expected two retained statement originals')
                        query_accounts = db.execute('SELECT id,slug,name,bank_name,registration_no,account_no,iban,bic,account_owner,customer_no,currency,ledger_account_no,active,created_at FROM bank_accounts ORDER BY id').fetchall()
                        account_audits = db.execute("SELECT event_type,entity_type,entity_id,actor FROM audit_log WHERE event_type IN ('bank_account_add','bank_account_update') ORDER BY id").fetchall()
                        if len(query_accounts) != 2 or [a[0] for a in account_audits] != ['bank_account_add']*2 + ['bank_account_update']*4:
                            raise RuntimeError('Original bank account lifecycle exact account/audit effects differ')
                        original_links = db.execute('SELECT id,bank_transaction_id,journal_entry_id,match_method,source_reference,note,created_by,created_by_program FROM bank_journal_reconciliation_links ORDER BY id').fetchall()
                        original_corrections = db.execute('SELECT id,bank_transaction_id,supersedes_reconciliation_id,superseded_journal_entry_id,replacement_journal_entry_id,principal_kind,principal_subject_id,plan_hash FROM bank_reconciliation_correction_events ORDER BY id').fetchall()
                        original_matching_audits = db.execute("SELECT event_type,message,actor FROM audit_log WHERE event_type IN ('bank_journal_reconciliation_link','bank_reconciliation_corrected') ORDER BY id").fetchall()
                        if len(original_links) != 1 or len(original_corrections) != 1 or [a[0] for a in original_matching_audits] != ['bank_journal_reconciliation_link','bank_reconciliation_corrected']:
                            raise RuntimeError('Original bank matching/replay exact source-only effects differ')
                        report['terminalSql'].update(originalLinks=original_links,originalCorrections=original_corrections,originalMatchingAudits=original_matching_audits)
                        report['terminalSql']['unmatchedExceptions'] = db.execute("SELECT id,related_bank_transaction_id,status FROM exceptions WHERE type='UNMATCHED_BANK_TRANSACTION' ORDER BY id").fetchall()
                        report['terminalSql']['exceptionResolutionAudits'] = db.execute("SELECT event_type,entity_id,actor FROM audit_log WHERE event_type='exception_resolve' ORDER BY id").fetchall()
                        report['terminalSql']['correctionReceiptAudits'] = db.execute("SELECT event_type,entity_id,actor FROM audit_log WHERE message LIKE '%bank_reconciliation_correction_apply' ORDER BY id").fetchall()
                        report['terminalSql'].update(bankRows=query_bank_rows,reconciliations=query_links,postingAudits=query_posts,originals=query_sources,bankAccounts=query_accounts,bankAccountAudits=account_audits)
                    if (args.local_runtime_join or args.local_runtime_mcp):
                        local_post_rows = db.execute("SELECT event_type,actor FROM audit_log WHERE event_type IN ('journal_post','journal_reverse') ORDER BY id").fetchall()
                        local_owner_id = report['results']['authenticate']['ownerId']
                        if len(local_post_rows) != 1 or local_post_rows[0][0] != 'journal_post' or not local_post_rows[0][1].startswith('user:'+local_owner_id+' via '):
                            raise RuntimeError('Local runtime exact authenticated posting audit differs')
                        if any(os.path.lexists(work/name) for name in ['r.json','r.sock']):
                            raise RuntimeError('Local runtime terminal endpoints not retired')
                        if args.native_cli_candidate:
                            vendor_rows = db.execute('SELECT id,name,vat_or_cvr,country_code,identifier_kind FROM vendors ORDER BY id').fetchall()
                            retained = actual['vendors']['rows']
                            expected_vendors = [(r['id'],r['name'],r['vatOrCvr'],r['countryCode'],r['identifierKind']) for r in retained]
                            vendor_audits = db.execute("SELECT event_type,actor FROM audit_log WHERE event_type='vendor_create' ORDER BY id").fetchall()
                            if vendor_rows != expected_vendors or vendor_audits != [('vendor_create','user:'+local_owner_id)]*2 or actual['vendors'] != reopened['vendors']:
                                raise RuntimeError('Candidate exact vendor identities/restart/owner audits differ')
                            fields = [('id','id'),('name','name'),('address','address'),('vat_or_cvr','vatOrCvr'),('country_code','countryCode'),('identifier_kind','identifierKind'),('identity_status','identityStatus'),('email','email'),('phone','phone'),('website','website'),('default_expense_account','defaultExpenseAccount'),('default_vat_treatment','defaultVatTreatment'),('notes','notes'),('archived','archived'),('created_at','createdAt')]
                            raw_vendors = db.execute('SELECT '+','.join(column for column,_ in fields)+' FROM vendors ORDER BY id').fetchall()
                            full_vendors = [{key:bool(value) if key == 'archived' else value for (_,key),value in zip(fields,row)} for row in raw_vendors]
                            if full_vendors != retained:
                                raise RuntimeError('Candidate entire raw vendor rows differ from retained source projection')
                            report['terminalSql'].update(vendors=vendor_rows,vendorCreateAudits=vendor_audits,vendorSourceProjection=full_vendors)
                        report['terminalSql']['postingAudits'] = local_post_rows
                        local_amount_rows = db.execute("SELECT a.account_no,l.debit_amount,l.credit_amount FROM journal_lines l JOIN accounts a ON a.id=l.account_id ORDER BY l.id").fetchall()
                        if local_amount_rows != [('1738',100.0,0.0),('3738',0.0,100.0)]:
                            raise RuntimeError('Local CLI exact source posting amounts differ')
                        local_original_rows = db.execute('SELECT id,stored_path,sha256_hash FROM documents ORDER BY id').fetchall()
                        local_original_evidence = []
                        for local_doc_id,local_relative,local_expected_hash in local_original_rows:
                            local_relative_path = Path(local_relative)
                            if local_relative_path.is_absolute() or '..' in local_relative_path.parts:
                                raise RuntimeError('Local original path escapes owned company')
                            local_original_path = work/'workspace/synthetic'/local_relative_path
                            if local_original_path.is_symlink() or not local_original_path.is_file() or digest(local_original_path) != local_expected_hash:
                                raise RuntimeError('Local retained original hash differs')
                            local_original_evidence.append({'id':local_doc_id,'sha256':local_expected_hash,'bytes':local_original_path.stat().st_size})
                        report['terminalSql']['postingLines'] = local_amount_rows
                        report['terminalSql']['originals'] = local_original_evidence
                        report['localRuntimeEndpointsRetired'] = True
                    if args.saft_artifact:
                        saft_rows = db.execute("SELECT event_type,actor FROM audit_log WHERE event_type='saft_export' ORDER BY id").fetchall()
                        saft_identities = report['results']['authenticate']
                        if len(saft_rows) != 15 or sum(row[1] == 'user:'+saft_identities['ownerId'] for row in saft_rows) != 14 or sum(row[1] == 'user:'+saft_identities['memberId'] for row in saft_rows) != 1:
                            raise RuntimeError('SAF-T exact accepted export audit effects differ')
                        report['terminalSql']['exportAudits'] = {'count':len(saft_rows),'rows':saft_rows}
                    if args.authority_export:
                        export_rows = db.execute("SELECT event_type,actor FROM audit_log WHERE event_type IN ('authority_export','accountant_handoff_export') ORDER BY id").fetchall()
                        export_counts = {kind:sum(row[0] == kind for row in export_rows) for kind in ['authority_export','accountant_handoff_export']}
                        handover_identities = report['results']['authenticate']
                        if export_counts != {'authority_export':7,'accountant_handoff_export':4} or sum(row[1] == 'user:'+handover_identities['ownerId'] for row in export_rows) != 10 or sum(row[1] == 'user:'+handover_identities['memberId'] for row in export_rows) != 1:
                            raise RuntimeError('Authority handover exact export audit effects differ')
                        report['terminalSql']['exportAudits'] = {'counts':export_counts,'rows':export_rows}

                finally:
                    db.close()
                if args.dated_reports:
                    # Original pure readers are an oracle on the stopped, owned synthetic
                    # ledger only. No TypeScript backend or second product engine runs.
                    reference_code = """import {Database} from 'bun:sqlite';
const {buildTrialBalance,buildProfitAndLoss,buildBalanceSheet}=await import(process.argv[2]);
const db=new Database(process.argv[1],{readonly:true});db.exec('PRAGMA query_only=ON');
const cases=JSON.parse(await Bun.stdin.text());
const out=cases.map(({kind,query})=>({kind,query,report:kind==='balance'?buildBalanceSheet(db,query.asOf):kind==='profit-loss'?buildProfitAndLoss(db,query.from,query.to):buildTrialBalance(db,query.from,query.to)}));
db.close();process.stdout.write(JSON.stringify(out));"""
                    oracle = subprocess.run(['/opt/homebrew/bin/bun','-e',reference_code,str(ledger),str(reference_files[0])], input=json.dumps(actual['projections']), text=True, capture_output=True, timeout=30, check=True)
                    if json.loads(oracle.stdout) != actual['projections'] or any(digest(path) != reference_hashes[str(path.relative_to(REPO))] for path in reference_files):
                        raise RuntimeError('Original dated numeric report reference differs')
                    report['originalReadOnlyReference'] = {'qualified':True,'cases':len(actual['projections']),'sources':reference_hashes,'stoppedOwnedSyntheticLedgerOnly':True}
                if args.journal_mcp_original or args.journal_original or args.period_original or args.annual_exports or args.authority_export or args.saft_artifact or (args.local_runtime_join or args.local_runtime_mcp):
                    if any(digest(path) != reference_hashes[str(path.relative_to(REPO))] for path in reference_files):
                        raise RuntimeError('Original annual export source changed during proof')
                    report['originalSourceHashes'] = reference_hashes
                if digest(helper) != report[helper_key] or any(digest(path) != report['artifacts'][name] or digest(getattr(args,name)) != report['artifacts'][name] for name,path in pinned.items()):
                    raise RuntimeError('Standalone report pinned helper or binaries changed')
                if args.native_cli_candidate:
                    verify_candidate()
                    report['nativeCliCandidateQualified'] = True
                source_web = {str(path.relative_to(args.web_dir)):digest(path) for path in sorted(args.web_dir.rglob('*')) if path.is_file()}
                if source_web != report['webFiles']:
                    raise RuntimeError('Standalone report web artifacts changed')
                phase('finish',20)
                controller.stdin.close(); controller.wait(timeout=20)
                if controller.returncode != 0:
                    raise RuntimeError('Standalone report browser controller failed')
                report.update(status='passed',currentAction='complete',installedNativeQualified=False,sourceFreeRestoreQualified=False)
                if args.journal_mcp_original:
                    report.update(workflow='original_journal_mcp_keyed_period_restart',journalMcpOriginalQualified=True,journalMcpOriginalRestartQualified=True,externalFilingQualified=False)
                elif args.journal_original:
                    report.update(workflow='original_journal_dry_run_post_period_reverse',journalOriginalQualified=True,journalOriginalRestartQualified=True,externalFilingQualified=False)
                elif args.period_original:
                    report.update(workflow='original_period_five_command_lifecycle',periodOriginalQualified=True,periodOriginalRestartQualified=True,externalFilingQualified=False)
                elif args.bank_queries:
                    report.update(workflow='original_bank_matching_account_import_queries_reconciliation',bankQueriesQualified=True,bankQueriesRestartQualified=True,bankOriginalImportQualified=report['results']['service-run'].get('originalBankImportQualified', False),bankOriginalAccountQualified=report['results']['service-run'].get('originalBankAccountQualified', False),bankOriginalMatchingQualified=report['results']['service-run'].get('originalBankMatchingQualified', False),bankOriginalMatchingRestartQualified=report['results']['service-reopen'].get('originalMatchingRestartQualified', False))
                elif args.privacy_compliance:
                    if not actual.get('privacy708',{}).get('qualified') or not reopened.get('privacy708',{}).get('exactNormalMfaRestartState'):
                        raise RuntimeError('Whole708 actual privacy/compliance and restart not qualified')
                    report.update(workflow='privacy708_legal_hold_eligible_forget_signed_audit_compliance',privacy708Qualified=True,privacy708RestartQualified=True)
                elif args.knowledge_scope:
                    if not actual.get('knowledge705',{}).get('qualified') or not reopened.get('knowledge705',{}).get('exactNormalMfaRestartState'):
                        raise RuntimeError('Full641 scoped knowledge and restart not qualified')
                    report.update(workflow='canonical_company_knowledge641_scoped_service_local_root',knowledgeScope641Qualified=True,knowledgeScope641RestartQualified=True)
                elif (args.local_runtime_join or args.local_runtime_mcp):
                    report.update(workflow='local_mcp_same_rust_owner_unix_socket' if args.local_runtime_mcp else 'local_cli_same_rust_owner_unix_socket',localRuntimeQualified=True,localRuntimeRestartQualified=True,localRuntimeMcpQualified=args.local_runtime_mcp)
                elif args.saft_artifact:
                    report.update(workflow='original_saft_cli_local_package_timing',saftArtifactQualified=True,saftArtifactRestartQualified=True)
                elif args.authority_export:
                    report.update(workflow='authority_handover_request_context',authorityExportQualified=True,authorityExportRestartQualified=True)
                elif args.annual_exports:
                    report.update(workflow='annual_tax_ixbrl',annualExportsQualified=True,annualExportsRestartQualified=True)
                elif args.portfolio_overview:
                    report.update(workflow='original_mcp_portfolio_overview',portfolioOverviewQualified=True,portfolioOverviewRestartQualified=True)
                elif args.dashboard_json:
                    report.update(workflow='original_company_dashboard_json',dashboardJsonQualified=True,dashboardJsonRestartQualified=True)
                elif args.dated_reports:
                    report.update(workflow='original_dated_financial_reports',datedReportsQualified=True,datedReportsRestartQualified=True)
                else:
                    report.update(workflow='original_journal_history_read_parity',journalHistoryQualified=True,journalHistoryRestartQualified=True)
                save()
                return
            if args.backup_lock:
                report['workflow'] = 'financial_backup_lock_qualified_artifact_unlock'
                report['backupLockHelperSha256'] = digest(REPO / 'scripts/rust/check-native-backup-lock.ts')
                proof_helpers = [REPO / 'scripts/rust/check-native-backup-lock.ts', REPO / 'scripts/rust/check-native-backup-mail-inbox.ts']
                report['backupLockMailInboxHelperSha256'] = digest(proof_helpers[1])
                # Static imports execute even when the optional extension is disabled.
                proof_helpers.append(REPO / 'scripts/rust/check-native-backup-remote.ts')
                report['backupRemoteHelperSha256'] = digest(proof_helpers[2])
                proof_helpers.append(REPO / 'scripts/rust/check-native-backup-governance.ts')
                report['backupGovernanceHelperSha256'] = digest(proof_helpers[3])
                proof_helpers.append(REPO / 'scripts/rust/check-native-backup-registration.ts')
                report['backupRegistrationHelperSha256'] = digest(proof_helpers[4])
                if args.backup_recovery:
                    proof_helpers.append(REPO / 'scripts/rust/check-native-backup-recovery.ts')
                    report['backupRecoveryHelperSha256'] = digest(proof_helpers[-1])
                if args.audit_log:
                    proof_helpers.append(REPO / 'scripts/rust/check-native-audit-log.ts')
                    report['auditLogHelperSha256'] = digest(proof_helpers[-1])
                if args.audit_verify:
                    proof_helpers.append(REPO / 'scripts/rust/check-native-audit-verify.ts')
                    report['auditVerifyHelperSha256'] = digest(proof_helpers[-1])
                if args.invoice_compensation:
                    proof_helpers.append(REPO / 'scripts/rust/check-native-invoice-compensation.ts')
                    report['invoiceCompensationHelperSha256'] = digest(proof_helpers[-1])
                if args.invoice_interest:
                    proof_helpers.append(REPO / 'scripts/rust/check-native-invoice-interest.ts')
                    report['invoiceInterestHelperSha256'] = digest(proof_helpers[-1])
                if args.invoice_bad_debt:
                    proof_helpers.append(REPO / 'scripts/rust/check-native-invoice-bad-debt.ts')
                    report['invoiceBadDebtHelperSha256'] = digest(proof_helpers[-1])
                    proof_helpers.append(REPO / 'scripts/rust/check-native-invoice-original.ts')
                    report['invoiceOriginalHelperSha256'] = digest(proof_helpers[-1])
                if args.dinero_import:
                    proof_helpers.append(REPO / 'scripts/rust/check-native-dinero-import.ts')
                    report['dineroImportHelperSha256'] = digest(proof_helpers[-1])
                if args.synthetic_import:
                    proof_helpers.append(REPO / 'scripts/rust/check-native-synthetic-import.ts')
                    report['syntheticImportHelperSha256'] = digest(proof_helpers[-1])
                if args.imported_portfolio:
                    proof_helpers.append(REPO / 'scripts/rust/check-native-imported-portfolio.ts')
                    report['importedPortfolioHelperSha256'] = digest(proof_helpers[-1])
                helper_hashes = {str(path): digest(path) for path in proof_helpers}
                phase('service-run')
                if any(digest(path) != helper_hashes[str(path)] for path in proof_helpers):
                    raise RuntimeError('Backup proof imported helper changed during execution')
                stop(); config.pop('firstOwner')
                write_config(); start()
                phase('service-reopen')
                if any(digest(path) != helper_hashes[str(path)] for path in proof_helpers):
                    raise RuntimeError('Backup proof imported helper changed during restart verification')
                if args.invoice_compensation:
                    phase('compensation-run', 600)
                    if any(digest(path) != helper_hashes[str(path)] for path in proof_helpers):
                        raise RuntimeError('Compensation proof helper changed during actual execution')
                    stop(); write_config(); start()
                    phase('compensation-reopen', 300)
                    if any(digest(path) != helper_hashes[str(path)] for path in proof_helpers):
                        raise RuntimeError('Compensation proof helper changed during restart')
                if args.invoice_interest:
                    phase('interest-run', 600)
                    if any(digest(path) != helper_hashes[str(path)] for path in proof_helpers):
                        raise RuntimeError('Interest proof helper changed during actual execution')
                    stop(); write_config(); start()
                    phase('interest-reopen', 300)
                    if any(digest(path) != helper_hashes[str(path)] for path in proof_helpers):
                        raise RuntimeError('Interest proof helper changed during restart')
                if args.invoice_bad_debt:
                    phase('bad-debt-run', 600)
                    if any(digest(path) != helper_hashes[str(path)] for path in proof_helpers):
                        raise RuntimeError('Bad-debt proof helper changed during actual execution')
                    stop(); write_config(); start()
                    phase('bad-debt-reopen', 300)
                    if any(digest(path) != helper_hashes[str(path)] for path in proof_helpers):
                        raise RuntimeError('Bad-debt proof helper changed during restart')
                if args.synthetic_import:
                    phase('synthetic-run', 600)
                    if any(digest(path) != helper_hashes[str(path)] for path in proof_helpers):
                        raise RuntimeError('Synthetic proof helper changed during execution')
                    stop(); write_config(); start()
                    phase('synthetic-reopen', 300)
                    if any(digest(path) != helper_hashes[str(path)] for path in proof_helpers):
                        raise RuntimeError('Synthetic proof helper changed during restart')
                if args.dinero_import:
                    phase('dinero-run', 600)
                    if any(digest(path) != helper_hashes[str(path)] for path in proof_helpers):
                        raise RuntimeError('Dinero proof helper changed during execution')
                    stop(); write_config(); start()
                    phase('dinero-reopen', 300)
                    if any(digest(path) != helper_hashes[str(path)] for path in proof_helpers):
                        raise RuntimeError('Dinero proof helper changed during restart')
                if args.imported_portfolio:
                    phase('imported-portfolio-run', 300)
                    if any(digest(path) != helper_hashes[str(path)] for path in proof_helpers):
                        raise RuntimeError('Imported portfolio proof helper changed during execution')
                    stop(); write_config(); start()
                    phase('imported-portfolio-reopen', 300)
                    if any(digest(path) != helper_hashes[str(path)] for path in proof_helpers):
                        raise RuntimeError('Imported portfolio proof helper changed during restart')
                if args.audit_log:
                    phase('audit-run')
                    if args.audit_verify:
                        phase('verification-run')
                    stop(); write_config(); start()
                    phase('audit-reopen')
                    if args.audit_verify:
                        phase('verification-reopen')
                    if any(digest(path) != helper_hashes[str(path)] for path in proof_helpers):
                        raise RuntimeError('Audit proof helper changed during execution or restart')
                if args.backup_recovery:
                    phase('backup-recovery-capture')
                    phase('backup-recovery-detach')
                stop()
                if args.invoice_compensation:
                    progress('operator:independent-signed-compensation-backup-verification')
                    archive = work / 'compensation-source.native-backup.json'
                    receipt = report['results']['compensation-run']['signedBackup']
                    if not owned() or archive.is_symlink() or not archive.is_file() or digest(archive) != receipt['sha256'] or archive.stat().st_size != receipt['bytes']:
                        raise RuntimeError('Exact owned signed compensation backup changed')
                    verified = subprocess.run([str(pinned['server_binary']), '--verify-backup', str(archive), '--trusted-backup-key', str(work / 'trusted.pub')], cwd=REPO, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=60)
                    if verified.returncode != 0:
                        raise RuntimeError('Independent signed compensation backup/schema verification refused')
                    report['invoiceCompensationSignedBackup'] = {'sha256':receipt['sha256'],'bytes':receipt['bytes'],'verified':True,'sourceFreeRestoreQualified':False}
                    save()
                if args.invoice_interest:
                    progress('operator:independent-signed-interest-backup-verification')
                    archive = work / 'interest-source.native-backup.json'
                    receipt = report['results']['interest-run']['signedBackup']
                    if not owned() or archive.is_symlink() or not archive.is_file() or digest(archive) != receipt['sha256'] or archive.stat().st_size != receipt['bytes']:
                        raise RuntimeError('Exact owned signed interest backup changed')
                    verified = subprocess.run([str(pinned['server_binary']), '--verify-backup', str(archive), '--trusted-backup-key', str(work / 'trusted.pub')], cwd=REPO, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=60)
                    if verified.returncode != 0:
                        raise RuntimeError('Independent signed interest backup/schema verification refused')
                    report['invoiceInterestSignedBackup'] = {'sha256':receipt['sha256'],'bytes':receipt['bytes'],'verified':True,'sourceFreeRestoreQualified':False}
                    save()
                if args.invoice_bad_debt:
                    progress('operator:independent-signed-bad-debt-backup-verification')
                    archive = work / 'bad-debt-source.native-backup.json'
                    receipt = report['results']['bad-debt-run']['signedBackup']
                    if not owned() or archive.is_symlink() or not archive.is_file() or digest(archive) != receipt['sha256'] or archive.stat().st_size != receipt['bytes']:
                        raise RuntimeError('Exact owned signed bad-debt backup changed')
                    verified = subprocess.run([str(pinned['server_binary']), '--verify-backup', str(archive), '--trusted-backup-key', str(work / 'trusted.pub')], cwd=REPO, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=60)
                    if verified.returncode != 0:
                        raise RuntimeError('Independent signed bad-debt backup/schema verification refused')
                    report['invoiceBadDebtSignedBackup'] = {'sha256':receipt['sha256'],'bytes':receipt['bytes'],'verified':True,'sourceFreeRestoreQualified':False}
                    save()
                if args.synthetic_import:
                    progress('operator:independent-signed-synthetic-backup-verification')
                    archive = work / 'synthetic-source.native-backup.json'
                    receipt = report['results']['synthetic-run']['signedBackup']
                    if not owned() or archive.is_symlink() or not archive.is_file() or digest(archive) != receipt['sha256'] or archive.stat().st_size != receipt['bytes']:
                        raise RuntimeError('Exact owned signed synthetic backup changed')
                    verified = subprocess.run([str(pinned['server_binary']), '--verify-backup', str(archive), '--trusted-backup-key', str(work / 'trusted.pub')], cwd=REPO, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=60)
                    if verified.returncode != 0:
                        raise RuntimeError('Independent signed synthetic backup/schema verification refused')
                    report['syntheticImportSignedBackup'] = {'sha256':receipt['sha256'],'bytes':receipt['bytes'],'verified':True,'sourceFreeRestoreQualified':False}
                    save()
                if args.dinero_import:
                    progress('operator:independent-signed-dinero-backup-verification')
                    archive = work / 'dinero-source.native-backup.json'
                    receipt = report['results']['dinero-run']['signedBackup']
                    if not owned() or archive.is_symlink() or not archive.is_file() or digest(archive) != receipt['sha256'] or archive.stat().st_size != receipt['bytes']:
                        raise RuntimeError('Exact owned signed Dinero backup changed')
                    verified = subprocess.run([str(pinned['server_binary']), '--verify-backup', str(archive), '--trusted-backup-key', str(work / 'trusted.pub')], cwd=REPO, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=60)
                    if verified.returncode != 0:
                        raise RuntimeError('Independent signed Dinero backup/schema verification refused')
                    report['dineroImportSignedBackup'] = {'sha256':receipt['sha256'],'bytes':receipt['bytes'],'verified':True,'sourceFreeRestoreQualified':False}
                    save()
                db = sqlite3.connect((work/'workspace/synthetic/data/ledger.sqlite').as_uri()+'?mode=ro', uri=True)
                try:
                    counts = {table: db.execute('SELECT count(*) FROM '+table).fetchone()[0] for table in ['documents','bank_transactions','journal_entries']}
                    expected_counts = {'documents':0,'bank_transactions':3,'journal_entries':3,'journal_lines':6} if args.bank_queries else {'documents':1,'bank_transactions':0,'journal_entries':1,'journal_lines':2} if args.authority_export else {'documents':2,'bank_transactions':0,'journal_entries':2,'journal_lines':6} if args.annual_exports else {'documents':4,'bank_transactions':4,'journal_entries':10} if args.invoice_bad_debt else {'documents':2,'bank_transactions':4,'journal_entries':7} if args.invoice_interest else {'documents':4,'bank_transactions':5,'journal_entries':9} if args.invoice_compensation else {'documents':0,'bank_transactions':1,'journal_entries':2 if args.audit_log else 1}
                    if counts != expected_counts:
                        raise RuntimeError('Backup lock terminal financial effects differ: '+json.dumps(counts,sort_keys=True))
                    report['terminalSql'] = {'counts':counts,'journalActors':db.execute('SELECT created_by,created_by_program FROM journal_entries ORDER BY id').fetchall()}
                    if args.invoice_bad_debt:
                        db.row_factory = sqlite3.Row
                        for table in ['documents','issued_invoice_postings','invoice_payments','invoice_bad_debt_writeoffs','invoice_compensation_claims','invoice_claim_payments','journal_entries','journal_lines']:
                            report['terminalSql'][table] = [dict(row) for row in db.execute('SELECT * FROM '+table+' ORDER BY '+('invoice_document_id' if table == 'issued_invoice_postings' else 'id'))]
                finally: db.close()
                if args.synthetic_import:
                    imported_db = sqlite3.connect((work/'workspace/synthetic-csv-j02/data/ledger.sqlite').as_uri()+'?mode=ro', uri=True)
                    try:
                        imported_counts = {table: imported_db.execute('SELECT count(*) FROM '+table).fetchone()[0] for table in ['documents','bank_transactions','journal_entries']}
                        if imported_counts != {'documents':1,'bank_transactions':1,'journal_entries':2}:
                            raise RuntimeError('Synthetic imported-owner financial effects differ')
                        proofs = imported_db.execute("SELECT message FROM audit_log WHERE event_type='native_opening_balance'").fetchall()
                        if len(proofs) != 1 or imported_db.execute('SELECT count(*) FROM opening_balances').fetchone()[0] != 1:
                            raise RuntimeError('Synthetic source opening duplicated or missing')
                        proof = json.loads(proofs[0][0])
                        raw = proof['binding']['syntheticSource']['content'].encode('utf8')
                        if hashlib.sha256(raw).hexdigest() != report['results']['synthetic-run']['sourceSha256'] or proof['record']['sourceBasis']['kind'] != 'synthetic_csv':
                            raise RuntimeError('Exact retained CSV source provenance differs')
                        if imported_db.execute('SELECT count(*) FROM journal_lines WHERE vat_code IS NOT NULL').fetchone()[0] != 0:
                            raise RuntimeError('Non-VAT imported owner gained VAT postings')
                        if imported_db.execute("SELECT count(*) FROM accounting_periods WHERE status='closed'").fetchone()[0] != 0:
                            raise RuntimeError('Historical evidence blocker was bypassed')
                        report['syntheticImportTerminalSql'] = {'company':'synthetic-csv-j02','counts':imported_counts,'exactSourceSha256':hashlib.sha256(raw).hexdigest(),'openingCount':1,'vatLineCount':0,'closedPeriodCount':0}
                    finally: imported_db.close()
                target_db = sqlite3.connect((work/'workspace/synthetic-backup-inbox-target/data/ledger.sqlite').as_uri()+'?mode=ro', uri=True)
                try:
                    target_counts = {table: target_db.execute('SELECT count(*) FROM '+table).fetchone()[0] for table in ['documents','bank_transactions','journal_entries']}
                    if target_counts != ({'documents':6,'bank_transactions':1,'journal_entries':5} if args.imported_portfolio else {'documents':4,'bank_transactions':1,'journal_entries':4} if args.dinero_import else {'documents':1,'bank_transactions':1,'journal_entries':0}):
                        raise RuntimeError('Backup inbox target terminal effects differ')
                    report['targetTerminalSql'] = {'company':'synthetic-backup-inbox-target','counts':target_counts}
                finally: target_db.close()
                if args.backup_recovery:
                    source = work / 'workspace'
                    if not owned() or source.is_symlink() or not source.is_dir():
                        raise RuntimeError('Exact owned source required before source-free removal')
                    # These are this harness's synthetic source copies, never user data.
                    shutil.rmtree(source)
                    (work / 'signing.der').unlink()
                    destination = work / 'backup-destination'
                    if destination.is_symlink() or not destination.is_dir():
                        raise RuntimeError('Exact owned synthetic destination required')
                    shutil.rmtree(destination)
                    for name in ['drive-remote-object.native-backup.json', 'drive-object.json']:
                        (work / name).unlink()
                    (work / 'backup-lock-service.key').unlink()
                    for old_archive in work.glob('*.native-backup.json'):
                        if old_archive.name != 'scanner-source.native-backup.json':
                            if old_archive.is_symlink() or not old_archive.is_file():
                                raise RuntimeError('Unexpected synthetic archive file')
                            old_archive.unlink()
                    for credential in work.glob('*.cookie'):
                        if credential.is_symlink() or not credential.is_file():
                            raise RuntimeError('Unexpected source credential file')
                        credential.unlink()
                    archive = work / 'scanner-source.native-backup.json'
                    captured = report['results']['backup-recovery-capture']
                    if digest(archive) != captured['archiveSha256'] or archive.stat().st_size != captured['archiveBytes']:
                        raise RuntimeError('Captured final recovery archive changed')
                    wire = json.loads(archive.read_bytes())
                    payloads = [base64.b64decode(entry['bytesBase64'], validate=True) for entry in wire['entries']]
                    payloads.append(base64.b64decode(wire['manifestBase64'], validate=True))
                    source_secrets = [scanner_token, drive_token, bearer, config['authMail']['idempotencySecret'], config['cookieKey'], config['mfaKey']]
                    if any(secret.encode() in raw for raw in payloads for secret in source_secrets):
                        raise RuntimeError('Private operator credential found in signed recovery archive')
                    report['sourceOperatorCredentialsAbsentFromArchive'] = True
                    progress('operator:source-free-independent-verification')
                    verified = subprocess.run([str(pinned['server_binary']), '--verify-backup', str(archive), '--trusted-backup-key', str(work / 'trusted.pub')], cwd=REPO, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=60)
                    if verified.returncode != 0:
                        raise RuntimeError('Source-free independent trusted-key verification refused')
                    # New runtime credentials and signer, separate from historical evidence.
                    subprocess.run(['/opt/homebrew/bin/bun', '-e', "const c=require('node:crypto'),f=require('node:fs'),k=c.generateKeyPairSync('ed25519'),s=k.privateKey.export({format:'der',type:'pkcs8'}).subarray(-32),p=k.publicKey.export({format:'der',type:'spki'}).subarray(-32);f.writeFileSync(process.argv[1],Buffer.concat([Buffer.from('3051020101300506032b657004220420','hex'),s,Buffer.from('812100','hex'),p]),{mode:0o600});f.writeFileSync(process.argv[2],p,{mode:0o600});", str(work / 'target-signing.der'), str(work / 'target-trusted.pub')], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                    for name in ['documentScanner', 'backupDestinations', 'backupRemoteVerification']:
                        config.pop(name, None)
                    config.update({'root': str(work / 'restored'), 'backupSigningKeyFile': str(work / 'target-signing.der'), 'cookieKey': key(), 'mfaKey': key(), 'restoreActor': 'system:synthetic-installer', 'firstOwner': {'actor': 'system:synthetic-installer', 'email': 'restored@example.invalid', 'name': 'Synthetic Restored Owner', 'password': 'synthetic-restored-password-789'}})
                    write_config(); start(restore=True)
                    phase('backup-recovery-restored')
                    stop(); config.pop('firstOwner'); config.pop('restoreActor')
                    write_config(); start()
                    phase('backup-recovery-reopen')
                    if any(digest(path) != helper_hashes[str(path)] for path in proof_helpers):
                        raise RuntimeError('Backup proof helper changed during source-free recovery')
                    if source.exists() or (work / 'signing.der').exists() or destination.exists() or (work / 'drive-remote-object.native-backup.json').exists():
                        raise RuntimeError('Source authority or backup copies unexpectedly recreated')
                    report.update(sourceFreeRestoreQualified=True,sourceWorkspaceAndPrivateSignerRemoved=True,sourceManagedAndProviderCopiesRemoved=True,restoredNewOwnerAndFreshMfaRestartQualified=True)
                phase('finish', 20)
                controller.stdin.close(); controller.wait(timeout=10)
                if controller.returncode != 0:
                    raise RuntimeError('Backup lock controller nonzero terminal')
                controller = None; stop()
                with gateway_lock:
                    if errors or provider_requests:
                        raise RuntimeError('Backup lock proof must not call scanner/provider')
                report.update(status='passed',currentAction='complete',financialBackupLockQualified=True,normalMfaRestartQualified=True,currentFileMissingAndTamperQualified=True,directControlMetadataGateQualified=True,directControlMetadataScope=['company-knowledge','corporate-record-ingest'],metadataRestartQualified=True,twoCompanyInboxTargetGateQualified=True,humanMailAliasGateQualified=True,multiCompanyMetadataGateQualified=False,partiesGateQualified=False,mailIntakeConfigurationGateQualified=False,smtpDispatchGateQualified=False,keyRotationQualified=False)
                if args.invoice_compensation:
                    actual = report['results']['compensation-run']
                    reopened = report['results']['compensation-reopen']
                    if not actual.get('qualified') or not reopened.get('qualified') or not reopened.get('exactNormalMfaRestartState') or not reopened.get('fullAudit119GreenAfterRestart') or actual['accountingFingerprint'] != reopened['accountingFingerprint']:
                        raise RuntimeError('Compensation whole-flow or exact restart not qualified')
                    report.update(workflow='invoice_compensation_dkk_eur_full_flow',invoiceCompensationQualified=True,invoiceCompensationRestartQualified=True,invoiceCompensationFullAudit119Qualified=True,invoiceCompensationSourceFreeRestoreQualified=False,invoiceCompensationPrivateBuyerActualQualified=False)
                if args.invoice_interest:
                    actual = report['results']['interest-run']
                    reopened = report['results']['interest-reopen']
                    if not actual.get('qualified') or not reopened.get('qualified') or not reopened.get('exactNormalMfaRestartState') or not reopened.get('fullAudit119GreenAfterRestart') or actual['accountingFingerprint'] != reopened['accountingFingerprint']:
                        raise RuntimeError('Interest whole-flow or exact restart not qualified')
                    report.update(workflow='invoice_interest_dkk_dated_full_flow',invoiceInterestQualified=True,invoiceInterestRestartQualified=True,invoiceInterestFullAudit119Qualified=True,invoiceInterestSourceFreeRestoreQualified=False,invoiceInterestForeignActualQualified=False,installedNativeQualified=False)
                if args.invoice_bad_debt:
                    actual = report['results']['bad-debt-run']
                    reopened = report['results']['bad-debt-reopen']
                    if not actual.get('qualified') or not reopened.get('qualified') or not reopened.get('exactNormalMfaRestartState') or not reopened.get('fullAudit119GreenAfterRestart') or actual['accountingFingerprint'] != reopened['accountingFingerprint']:
                        raise RuntimeError('Bad-debt whole-flow or exact restart not qualified')
                    if not actual.get('originalInvoiceQueriesQualified') or not reopened.get('originalInvoiceQueriesQualified'):
                        raise RuntimeError('Original invoice query family or restart not qualified')
                    report['originalInvoiceQueriesQualified'] = True
                    report['originalInvoiceQueriesRestartQualified'] = True
                    report.update(workflow='invoice_bad_debt_dkk_foreign_full_flow',invoiceBadDebtQualified=True,invoiceBadDebtRestartQualified=True,invoiceBadDebtFullAudit119Qualified=True,invoiceBadDebtSourceFreeRestoreQualified=False,invoiceBadDebtForeignActualQualified=True,independentLossEstablishmentQualified=False,recoveryFeatureQualified=False,foreignInterestQualified=False,installedNativeQualified=False)
                if args.synthetic_import:
                    actual = report['results']['synthetic-run']
                    reopened = report['results']['synthetic-reopen']
                    if not actual.get('qualified') or not reopened.get('qualified') or not reopened.get('exactNormalMfaRestartState') or actual['accountingFingerprint'] != reopened['accountingFingerprint']:
                        raise RuntimeError('Synthetic whole-flow or exact restart not qualified')
                    report.update(workflow='synthetic_csv_shifted_nonvat_import_owner',syntheticImportQualified=True,syntheticImportRestartQualified=True,syntheticImportSourceFreeRestoreQualified=False,installedNativeQualified=False)
                if args.dinero_import:
                    actual = report['results']['dinero-run']
                    reopened = report['results']['dinero-reopen']
                    if not actual.get('qualified') or not reopened.get('qualified') or not reopened.get('exactNormalMfaRestartState') or not reopened.get('fullAudit119GreenAfterRestart') or actual['accountingFingerprint'] != reopened['accountingFingerprint']:
                        raise RuntimeError('Dinero whole-flow or exact restart not qualified')
                    report.update(workflow='dinero_import_imported_receivables_full_flow',dineroImportQualified=True,dineroImportRestartQualified=True,dineroImportFullAudit119Qualified=True,dineroImportSourceFreeRestoreQualified=False,installedNativeQualified=False)
                if args.imported_portfolio:
                    actual = report['results']['imported-portfolio-run']
                    reopened = report['results']['imported-portfolio-reopen']
                    if not actual.get('qualified') or not reopened.get('exactNormalMfaRestartState') or actual['accountingFingerprint'] != reopened['accountingFingerprint']:
                        raise RuntimeError('Imported portfolio flow or restart not qualified')
                    progress('operator:independent-signed-imported-portfolio-backup-verification')
                    archive = work / 'imported-portfolio.native-backup.json'
                    receipt = actual['signedBackup']
                    if not owned() or archive.is_symlink() or not archive.is_file() or digest(archive) != receipt['sha256'] or archive.stat().st_size != receipt['bytes']:
                        raise RuntimeError('Exact owned imported portfolio backup changed')
                    verified = subprocess.run([str(pinned['server_binary']), '--verify-backup', str(archive), '--trusted-backup-key', str(work / 'trusted.pub')], cwd=REPO, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=60)
                    if verified.returncode != 0:
                        raise RuntimeError('Independent imported portfolio backup verification refused')
                    report.update(workflow='imported_company_native_invoice_portfolio',importedPortfolioQualified=True,importedPortfolioRestartQualified=True,importedPortfolioSignedBackupVerified=True,importedPortfolioSourceFreeRestoreQualified=False,installedNativeQualified=False)
                if args.audit_log:
                    report.update(auditLogQualified=True,auditLogRestartQualified=True,auditLogFullVerificationQualified=False)
                if args.audit_verify:
                    report.update(auditVerifyQualified=True,auditVerifyRestartQualified=True)
                if args.remote_backup:
                    expected_drive = [{'objectId':obj,'kind':kind} for obj,kinds in [('synthetic_good',['metadata','media']),('synthetic_wrong_parent',['metadata']),('synthetic_wrong_hash',['metadata','media']),('synthetic_oversized',['metadata','media']),('synthetic_missing',['metadata']),('synthetic_held',['metadata','media']),('synthetic_good',['metadata','media']),('synthetic_good',['metadata','media'])] for kind in kinds]
                    if drive_requests != expected_drive:
                        raise RuntimeError('Drive GET trace differs: skipped work or unexpected retries')
                    governance = report['results']['service-run']['remoteBackup']['governance']
                    reopened_governance = report['results']['service-reopen']['remoteBackupReopen']['governance']
                    if not governance.get('qualified') or not reopened_governance.get('sameNewLatestAfterNormalMfaRestart') or not reopened_governance.get('historicalEvidenceNotBorrowed'):
                        raise RuntimeError('Current exact-archive governance or restart parity not qualified')
                    registration = report['results']['service-run']['remoteBackup']['registrations']
                    reopened_registration = report['results']['service-reopen']['remoteBackupReopen']['registrations']
                    if not registration.get('qualified') or not reopened_registration.get('sameHistoryAfterNormalMfaRestart'):
                        raise RuntimeError('Destination registration/history or restart parity not qualified')
                    report.update(backupRegistrationQualified=True,backupGovernanceQualified=True,remoteBackupVerificationQualified=True,driveGetTrace=list(drive_requests),fixtureCopyByHarness=True,actualGoogleAccountQualified=False,offsiteQualified=False,installedNativeQualified=False)
                save()
                if args.invoice_compensation:
                    print('PASS: normal MFA DKK/EUR compensation calculation, separate registration/posting, principal and DKK claim receipts, exact adapters/full audit119 and restart; '+str(args.evidence))
                    return
                if args.invoice_interest:
                    print('PASS: normal MFA DKK interest calculation, separate claim/post, dated partial principal, causal correction, actual receipts, full audit119 and exact restart; '+str(args.evidence))
                    return
                if args.invoice_bad_debt:
                    print('PASS: normal MFA DKK/foreign bad-debt UI/CLI/MCP writes, distinct partial-loss intents, blocked lost reply, principal/claim receipts, exact VAT/full audit119 and restart; '+str(args.evidence))
                    return
                if args.imported_portfolio:
                    print('PASS: real Dinero import then native invoice, explicit separate portfolio totals through HTTP/web/CLI/MCP, full audit, signed backup and exact normal MFA restart; '+str(args.evidence))
                    return
                if args.dinero_import:
                    print('PASS: normal MFA reviewed Dinero import, exact-key replay, as-of imported receivables, exact originals, full audit and restart; '+str(args.evidence))
                    return
                print('PASS: normal MFA financial backup lock, human/service HTTP/CLI/MCP refusal, metadata refusal/recovery, two-company inbox/alias gate and exact handoff, UI verified artifact, missing/tamper lock, single posting and restart; '+str(args.evidence))
                return
            if args.service_purchase:
                report['workflow'] = 'original_payable_service_scanned_bill_partial_full_receipts'
                report['servicePurchaseHelperSha256'] = digest(REPO / 'scripts/rust/check-native-service-purchase.ts')
                stop(); config.pop('firstOwner')
                config['documentScanner'] = {'policy': 'required', 'provider': 'http-json-v1', 'url': f'https://127.0.0.1:{mail_port}/scan', 'bearerToken': scanner_token, 'timeoutMs': 10000, 'trustedCaFile': str(ca)}
                write_config(); start()
                phase('service-run')
                stop(); write_config(); start()
                phase('service-reopen')
                originals_before = original_inventory()
                if len(originals_before) != 1:
                    raise RuntimeError('Exactly one service purchase original required')
                phase('service-hold-begin')
                if not held_received.wait(5):
                    raise RuntimeError('Service upload did not reach held verified TLS scanner')
                try:
                    phase('service-revoke', 4)
                finally:
                    held_release.set()
                phase('service-hold-finish')
                if original_inventory() != originals_before:
                    raise RuntimeError('Revoked service publication changed original inventory')
                requests_before_restart = provider_count()
                stop(); write_config(); start()
                phase('service-reopen')
                if provider_count() != requests_before_restart:
                    raise RuntimeError('Service restart/read rescanned original')
                if digest(REPO / 'scripts/rust/check-native-service-purchase.ts') != report['servicePurchaseHelperSha256']:
                    raise RuntimeError('Original payable helper changed during execution')
                phase('finish', 20)
                controller.stdin.close(); controller.wait(timeout=10)
                if controller.returncode != 0:
                    raise RuntimeError('Service purchase controller nonzero terminal')
                controller = None; stop()
                db = sqlite3.connect((work/'workspace/synthetic/data/ledger.sqlite').as_uri()+'?mode=ro', uri=True)
                try:
                    counts = {table: db.execute('SELECT count(*) FROM '+table).fetchone()[0] for table in ['documents','document_scan_evidence','payables','payable_payments','bank_transactions','journal_entries']}
                    if counts != {'documents':1,'document_scan_evidence':1,'payables':1,'payable_payments':2,'bank_transactions':2,'journal_entries':3}:
                        raise RuntimeError('Service purchase terminal ledger effect counts differ')
                    report['terminalSql'] = {'counts':counts,'operationalActors':db.execute("SELECT event_type,actor FROM audit_log WHERE event_type IN ('document_ingest','payable_register','payable_payment_apply') ORDER BY id").fetchall(), 'journalActors':db.execute('SELECT created_by,created_by_program FROM journal_entries ORDER BY id').fetchall()}
                    actors = report['terminalSql']['operationalActors']
                    if len(actors) != 4 or not actors[0][1].startswith('service_account:') or any(actor != actors[0][1] for _, actor in actors):
                        raise RuntimeError('Service operational attribution differs')
                    subject = actors[0][1].removeprefix('service_account:')
                    if report['terminalSql']['journalActors'] != [('user:'+subject,'service-principal')]*3:
                        raise RuntimeError('Service journal attribution differs')
                    db.row_factory = sqlite3.Row
                    for table in ['payables','payable_payments','bank_transactions','journal_entries','journal_lines','mutation_idempotency_tombstones','mutation_idempotency_outcomes']:
                        report['terminalSql'][table] = [dict(row) for row in db.execute('SELECT * FROM '+table+' ORDER BY '+('tombstone_id' if table == 'mutation_idempotency_outcomes' else 'id'))]
                    report['terminalSql']['payableReceiptAudits'] = [dict(row) for row in db.execute("SELECT id,event_type,entity_type,entity_id,message,actor FROM audit_log WHERE message LIKE '%payable_register%' OR message LIKE '%payable_pay%' ORDER BY id")]
                    report['originalPayableQualified'] = report['results']['service-run'].get('originalPayableQualified', False)
                    report['originalPayableReceiptRestartQualified'] = report['results']['service-reopen'].get('originalReceiptRestartQualified', False)
                finally: db.close()
                if args.backup_destinations:
                    # Read only after every owned server has stopped. Never open a
                    # second SQLite handle in the live owner process.
                    control = sqlite3.connect((work/'workspace/.rentemester/workspace-control.sqlite').as_uri()+'?mode=ro', uri=True)
                    try:
                        snapshots = [json.loads(row[0]) for row in control.execute("SELECT entity_id FROM workspace_audit WHERE event_type='native_backup_created' AND entity_type='native_retained_snapshot' ORDER BY id")]
                    finally: control.close()
                    if len(snapshots) != 2:
                        raise RuntimeError('Expected retained snapshots for explicit placement and duplicate placement attempt')
                    retained_hashes = []
                    for snapshot in snapshots:
                        archive_digest = snapshot['archiveSha256']
                        retained = work/'workspace/native-backups'/f'{archive_digest}.native-backup.json'
                        meta = retained.lstat()
                        if not stat.S_ISREG(meta.st_mode) or stat.S_IMODE(meta.st_mode) != 0o600 or meta.st_nlink != 1 or meta.st_uid != os.getuid():
                            raise RuntimeError('Retained signed backup has unsafe identity/mode')
                        raw = retained.read_bytes()
                        if len(raw) != snapshot['archiveSizeBytes'] or hashlib.sha256(raw).hexdigest() != archive_digest:
                            raise RuntimeError('Retained snapshot differs from persistent inventory')
                        verified = subprocess.run([str(work/'rentemester-server'),'--verify-backup',str(retained),'--trusted-backup-key',str(work/'trusted.pub')], stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=60)
                        if verified.returncode != 0:
                            raise RuntimeError('Independent retained artifact/SQLite verification failed')
                        if snapshot['version'] != 1 or len(snapshot['companies']) != 1 or snapshot['companies'][0]['slug'] != 'synthetic':
                            raise RuntimeError('Retained inventory coverage differs')
                        retained_hashes.append(archive_digest)
                    report['retainedBackups'] = {'count':len(snapshots),'archiveSha256':retained_hashes,'privateImmutableFiles':True,'independentSignatureAndSqliteVerification':True,'inventoryPersistedAcrossRestart':True,'notOffsiteClaim':True}
                with gateway_lock:
                    if errors or len(provider_requests) != 2 or sorted(r['kind'] for r in provider_requests) != ['cli','held']:
                        raise RuntimeError('Service purchase scanner calls differ or private provider failed')
                    report['provider'] = {'verifiedTls':True,'requests':list(provider_requests),'restartNoRescan':True}
                report.update(status='passed',currentAction='complete',servicePurchaseQualified=True,currentRevocationDuringScanQualified=True,backupDestinationsQualified=args.backup_destinations)
                save()
                print('PASS: service scanned original→payable→bank payment, revoked in-flight upload, exact restart/terminal counts; '+str(args.evidence))
                return
            phase('historical')
            if provider_count() != 0:
                raise RuntimeError('Scanner-off historical upload called provider')
            stop()
            config.pop('firstOwner')
            config['documentScanner'] = {'policy': 'required', 'provider': 'http-json-v1', 'url': f'https://127.0.0.1:{mail_port}/scan', 'bearerToken': scanner_token, 'timeoutMs': 10000, 'trustedCaFile': str(ca)}
            write_config()
            start()
            phase('clean')
            originals_before = original_inventory()
            if len(originals_before) != 2:
                raise RuntimeError('Historical and clean originals must both physically exist')
            phase('negatives')
            if original_inventory() != originals_before:
                raise RuntimeError('Rejected/timed-out/malformed/duplicate scan published an original')
            count = provider_count()
            phase('unauthorized')
            if provider_count() != count:
                raise RuntimeError('Unauthorized preflight contacted provider')
            phase('hold-begin')
            if not held_received.wait(5):
                raise RuntimeError('Invited uploader did not reach owned held scanner')
            try:
                phase('revoke', 3)
            finally:
                held_release.set()
            phase('hold-finish')
            if original_inventory() != originals_before:
                raise RuntimeError('Revoked final admission published an original')
            report['currentRevocationDuringScanQualified'] = True
            phase('clients')
            if len(original_inventory()) != 4:
                raise RuntimeError('Exactly four accepted physical originals required')
            with gateway_lock:
                for kind in ['reject', 'timeout', 'missing-id', 'invalid-field', 'malformed', 'held', 'cli', 'mcp']:
                    if len([r for r in provider_requests if r['kind'] == kind]) != 1:
                        raise RuntimeError('Expected one provider request without retry for ' + kind)
                report['duplicateScannerRequestCount'] = len([r for r in provider_requests if r['kind'] == 'clean']) - 1
            phase('archive-source')
            original = inspect_snapshot('source', work / 'trusted.pub')
            requests_after_source = provider_count()
            stop()
            write_config()
            start()
            phase('reopen')
            phase('archive-restarted')
            if inspect_snapshot('restarted', work / 'trusted.pub') != original or provider_count() != requests_after_source:
                raise RuntimeError('Restart/read changed scan evidence/audit or rescanned original')
            stop()
            source = work / 'workspace'
            if not owned() or source.is_symlink() or not source.is_dir():
                raise RuntimeError('Exact owned source required before source-free removal')
            shutil.rmtree(source)
            (work / 'signing.der').unlink()
            progress('operator:source-free-independent-verification')
            verified = subprocess.run([str(pinned['server_binary']), '--verify-backup', str(work / 'scanner-source.native-backup.json'), '--trusted-backup-key', str(work / 'trusted.pub')], cwd=REPO, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=60)
            if verified.returncode != 0:
                raise RuntimeError('Source-free independent trusted-key verification refused')
            subprocess.run(['/opt/homebrew/bin/bun', '-e', "const c=require('node:crypto'),f=require('node:fs'),k=c.generateKeyPairSync('ed25519'),s=k.privateKey.export({format:'der',type:'pkcs8'}).subarray(-32),p=k.publicKey.export({format:'der',type:'spki'}).subarray(-32);f.writeFileSync(process.argv[1],Buffer.concat([Buffer.from('3051020101300506032b657004220420','hex'),s,Buffer.from('812100','hex'),p]),{mode:0o600});f.writeFileSync(process.argv[2],p,{mode:0o600});", str(work / 'target-signing.der'), str(work / 'target-trusted.pub')], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            config.pop('documentScanner')
            config.update({'root': str(work / 'restored'), 'backupSigningKeyFile': str(work / 'target-signing.der'), 'cookieKey': key(), 'mfaKey': key(), 'restoreActor': 'system:synthetic-installer', 'firstOwner': {'actor': 'system:synthetic-installer', 'email': 'restored@example.invalid', 'name': 'Synthetic Restored Owner', 'password': 'synthetic-restored-password-789'}})
            write_config()
            start(restore=True)
            restored = phase('restored')
            report['sourceFreeRestoreQualified'] = restored['sourceFreeRestoreQualified']
            report['sourceWorkspaceAndPrivateSignerRemoved'] = not source.exists() and not (work / 'signing.der').exists()
            phase('archive-restored')
            preserved(original, inspect_snapshot('restored', work / 'target-trusted.pub'))
            stop()
            config.pop('firstOwner')
            config.pop('restoreActor')
            write_config()
            start()
            phase('restored-reopen')
            if provider_count() != requests_after_source:
                raise RuntimeError('Restored historical evidence attempted scanner I/O')
            phase('finish', 20)
            controller.stdin.close()
            controller.wait(timeout=10)
            if controller.returncode != 0:
                raise RuntimeError('Controller nonzero terminal')
            controller = None
            with gateway_lock:
                if errors:
                    raise RuntimeError('Private gateway protocol error')
                report['provider'] = {'verifiedTls': True, 'requests': list(provider_requests), 'sourceFreeRestoreNoRescan': True, 'configuredOnlyThroughPrivateOperatorJson': True}
                report['gateway'] = {'https': True, 'trustedCaSha256': digest(ca), 'acceptedMessages': len(gateway_rows), 'requests': list(gateway_requests)}
            report.update({'status': 'passed', 'currentAction': 'complete', 'ordinaryDocumentScanningQualified': True, 'existingCliMcpDocumentIngressQualified': True, 'initialAndRestartQualified': True, 'snapshotCoverage': 'signed-source-free-originals-scanevidence-audit-restored-and-reopened'})
            save()
            print('PASS: ordinary MFA required scanner, exact originals/evidence, provider refusals, current revocation, clients and source-free restore; ' + str(args.evidence))
        except BaseException as error:
            if args.invoice_interest and owned():
                try:
                    diagnostic_db = sqlite3.connect((work/'workspace/synthetic/data/ledger.sqlite').as_uri()+'?mode=ro', uri=True)
                    try:
                        report['interestFailureSql'] = {table: diagnostic_db.execute('SELECT count(*) FROM '+table).fetchone()[0] for table in ['documents','bank_transactions','journal_entries','invoice_interest_claims','invoice_interest_postings','invoice_interest_corrections','invoice_claim_payments']}
                    finally: diagnostic_db.close()
                except (OSError, sqlite3.Error):
                    report['interestFailureSqlUnavailable'] = True
            if args.invoice_bad_debt and owned():
                try:
                    diagnostic_db = sqlite3.connect((work/'workspace/synthetic/data/ledger.sqlite').as_uri()+'?mode=ro', uri=True)
                    try:
                        report['badDebtFailureSql'] = {table: diagnostic_db.execute('SELECT count(*) FROM '+table).fetchone()[0] for table in ['documents','bank_transactions','journal_entries','invoice_bad_debt_writeoffs','invoice_claim_payments']}
                    finally: diagnostic_db.close()
                except (OSError, sqlite3.Error):
                    report['badDebtFailureSqlUnavailable'] = True
            with gateway_lock:
                report['providerRequestTrace'] = list(provider_requests)
                if args.remote_backup:
                    report['driveGetTrace'] = list(drive_requests)
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
            held_release.set()
            stop()
            for server in servers:
                server.shutdown()
                server.server_close()
            report['ownedProcessesStopped'] = child is None and (controller is None or controller.poll() is not None)
            if owned() and (args.exceptions_original or args.period_mcp_original or args.journal_mcp_original or args.journal_original or args.period_original or args.bank_queries or args.local_runtime_join or args.local_runtime_mcp or args.service_purchase or args.invoice_bad_debt) and report['status'] == 'failed':
                # Retain the stopped, private synthetic fixture for targeted diagnosis.
                # This prevents repeating MFA merely to recover an assertion's source.
                report['ownedRuntimeCleaned'] = False
                report['ownedRuntimeRetainedForDiagnosis'] = True
            elif owned():
                shutil.rmtree(work)
                report['ownedRuntimeCleaned'] = True
            else:
                report['ownedRuntimeCleaned'] = False
            save()


if __name__ == '__main__':
    main()
