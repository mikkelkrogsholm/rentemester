#!/usr/bin/env python3
"""Owned ordinary-MFA public invoicing, synthetic verified TLS provider and signed recovery.
New private synthetic profile only; no identity/ledger SQL seeds, external provider,
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
import urllib.parse
import xml.etree.ElementTree as ET
from pathlib import Path
REPO=Path(__file__).resolve().parents[2]
PREFIX='Rentemester-Synthetic-Public-Einvoice-'
SIGNER="const c=require('node:crypto'),f=require('node:fs'),k=c.generateKeyPairSync('ed25519'),s=k.privateKey.export({format:'der',type:'pkcs8'}).subarray(-32),p=k.publicKey.export({format:'der',type:'spki'}).subarray(-32);f.writeFileSync(process.argv[1],Buffer.concat([Buffer.from('3051020101300506032b657004220420','hex'),s,Buffer.from('812100','hex'),p]),{mode:0o600});f.writeFileSync(process.argv[2],p,{mode:0o600});"
BROWSER = r'''







import {createHmac,createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {createInterface} from 'node:readline';
import {readFileSync,readdirSync,writeFileSync,unlinkSync,lstatSync,realpathSync} from 'node:fs';
import {join} from 'node:path';
import {isDeepStrictEqual} from 'node:util';
const {chromium}=await import(process.env.RM_PUBLIC_EINVOICE_PLAYWRIGHT);
const base=process.env.RM_PUBLIC_EINVOICE_BASE,work=process.env.RM_PUBLIC_EINVOICE_WORK;
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
let stable,freshOwner,customer,issued={},originalsById={},heldBefore,sourceCookie;
const read=p=>get(ownerPage,p),post=async(p,b,page=ownerPage)=>{const r=await fetchJson(page,p,b);need(r.status===200&&r.body?.ok===true,`confirmed ${p} HTTP${r.status}`);return r.body;};
async function rows(){const r=await read(prefix+'invoices/public-submissions');need(r.ok===true&&Array.isArray(r.rows),'flat canonical submissions array');return r;}
async function snapshot(){return{submissions:await rows(),invoices:await read(prefix+'invoices'),documents:await read(prefix+'documents'),journal:await read(prefix+'journal'),trialBalance:await read(prefix+'trial-balance')};}
async function originals(){for(const [kind,item] of Object.entries(issued)){const doc=await ownerPage.request.get(base+prefix+`documents/${item.id}/file`),pdf=await ownerPage.request.get(base+prefix+`invoices/${item.id}/pdf`);need(doc.status()===200&&pdf.status()===200,'issued registered JSON/PDF readable');const json=await doc.body(),bytes=await pdf.body();need(bytes.subarray(0,5).toString()==='%PDF-','actual issued PDF');const proof={jsonBase64:json.toString('base64'),pdfBase64:bytes.toString('base64'),jsonSha256:hash(json),pdfSha256:hash(bytes)};if(originalsById[item.id])need(isDeepStrictEqual(originalsById[item.id],proof),'immutable exact issued JSON/PDF');else originalsById[item.id]=proof;const source=JSON.parse(json);need(JSON.stringify(source).includes('5790000000074'),'issued immutable recipient EAN');}return originalsById;}
async function setup(){step='legal-profile-and-explicit-public-customer';const changed=await ownerPage.request.patch(base+prefix+'company',{headers:{origin:base},data:{cvr:'87654321',address:'Synthetic Street 74',postalCode:'1000',city:'Synthetic City',vatPeriodType:'quarter',paymentTermsDays:14}});need(changed.status()===200,'explicit current legal identity');const c=await fetchJson(ownerPage,prefix+'customers',{name:'Synthetic Public Recipient',address:'Synthetic Buyer Street',vatOrCvr:'DK12345678',eanNumber:'5790000000074',paymentTermsDays:14,defaultCurrency:'DKK'});need([200,201].includes(c.status),'ordinary customer creation');customer=c.body.customer;need(customer?.id,'canonical customer ID');const cfg=await read(prefix+'invoices/public-config');need(cfg.publicEInvoice.configured===true&&!cfg.publicEInvoice.registered,'operator configuration distinct from binding');const denied=await fetchJson(memberPage,prefix+'invoices/public-register',{confirm:true});need(denied.status===403,'reviewer cannot register outbound identity');return{customerId:customer.id};}
async function register(){step='existing-web-explicit-outbound-registration';await ownerPage.goto(base+'/companies/synthetic/fakturaer');await ownerPage.getByRole('heading',{name:'E-faktura',exact:true}).waitFor();await ownerPage.getByRole('button',{name:'Aktivér e-faktura',exact:true}).click();const dialog=ownerPage.getByRole('dialog',{name:'Aktivér e-faktura',exact:true});const receipt=ownerPage.waitForResponse(r=>new URL(r.url()).pathname===prefix+'invoices/public-register'&&r.request().method()==='POST');await dialog.getByRole('button',{name:'Bekræft tilmelding',exact:true}).click();need((await receipt).status()===200,'explicit native registration');const cfg=await read(prefix+'invoices/public-config');need(cfg.publicEInvoice.registered&&cfg.publicEInvoice.outbound,'binding and outbound participant retained');need((await rows()).rows.length===0,'registration sends no invoice');return cfg;}
async function issueAll(){step='ordinary-real-invoice-preview-and-issue';const today=(await read(prefix+'scheduled-recurring')).scheduledRecurring.asOfDate;need(/^\d{4}-\d{2}-\d{2}$/.test(today),'trusted economic date');for(const kind of ['queued','terminal','unknown','invalid','held']){const input={issueDate:today,customerId:customer.id,seller:{vatOrCvr:'DK87654321'},buyer:{eanNumber:'5790000000074',publicRecipient:true,buyerReference:'Synthetic Buyer Reference',orderReference:'Synthetic Order 74'},vatRatePercent:25,lines:[{description:`Synthetic public ${kind} 74`,quantity:1,unitPriceExVat:100}]};const preview=await ownerPage.request.post(base+prefix+'invoices/preview',{headers:{origin:base},data:input});need(preview.status()===200&&(await preview.body()).subarray(0,5).toString()==='%PDF-','ordinary actual PDF preview');let result;if(kind==='queued'){await ownerPage.goto(base+'/companies/synthetic/fakturaer');await ownerPage.getByRole('button',{name:'Udsted faktura',exact:true}).click();const dialog=ownerPage.getByRole('dialog',{name:'Udsted faktura',exact:true});await dialog.getByLabel('Fakturadato',{exact:true}).fill(today);await dialog.getByLabel('Momssats (%)',{exact:true}).fill('25');await dialog.getByLabel('Sælger',{exact:true}).fill('Synthetic Public Invoice Company');await dialog.getByLabel('Sælger CVR/moms',{exact:true}).fill('DK87654321');await dialog.getByLabel('Sælgeradresse',{exact:true}).fill('Synthetic Street 74');await dialog.getByLabel('Vælg kunde',{exact:true}).selectOption(String(customer.id));await dialog.getByLabel('EAN-nummer',{exact:true}).fill('5790000000074');await dialog.getByLabel('Offentlig modtager',{exact:true}).check();await dialog.getByLabel('Købers reference',{exact:true}).fill('Synthetic Buyer Reference');await dialog.getByLabel('Ordrenummer',{exact:true}).fill('Synthetic Order 74');await dialog.getByLabel('Linje 1 beskrivelse',{exact:true}).fill(input.lines[0].description);await dialog.getByLabel('Linje 1 antal',{exact:true}).fill('1');await dialog.getByLabel('Linje 1 enhedspris',{exact:true}).fill('100');const viewed=ownerPage.waitForResponse(r=>new URL(r.url()).pathname===prefix+'invoices/preview'&&r.request().method()==='POST');await dialog.getByRole('button',{name:'Forhåndsvis',exact:true}).click();const rendered=await viewed;need(rendered.status()===200&&(await rendered.body()).subarray(0,5).toString()==='%PDF-','existing web real PDF review');const receipt=ownerPage.waitForResponse(r=>new URL(r.url()).pathname===prefix+'invoices/issue'&&r.request().method()==='POST');await dialog.getByRole('button',{name:'Udsted faktura',exact:true}).click();const response=await receipt;need(response.status()===200,'explicit web issuance');result=await response.json();await dialog.getByRole('button',{name:'Luk',exact:true}).click();}else result=await post(prefix+'invoices/issue',input);need(result.invoice?.documentId>0&&result.invoice.invoiceNumber,'canonical issue identity');issued[kind]={id:result.invoice.documentId,number:result.invoice.invoiceNumber};}await originals();writeFileSync(join(work,'issued-fixtures.json'),JSON.stringify(issued),{mode:0o600,flag:'wx'});return{issued,originals:originalsById};}
async function editContact(){step='mutable-contact-cannot-reroute-issued-invoice';const r=await ownerPage.request.patch(base+prefix+'customers/'+customer.id,{headers:{origin:base},data:{name:'Synthetic Changed Customer',eanNumber:'5790000000075'}});need(r.status()===200,'ordinary customer edit after issue');await originals();return{issuedRoutingUnchanged:true};}
async function send(kind,page=ownerPage){const result=await fetchJson(page,prefix+'invoices/send-public',{invoiceDocumentId:issued[kind].id,confirm:true});return result;}
async function unbound(){step='unbound-send-no-provider';const r=await send('queued');need(r.status===422,'send requires explicit retained binding/outbound');need((await rows()).rows.length===0,'missing binding creates no reservation');return{status:r.status,code:r.body?.code};}
async function primary(){step='existing-web-confirmed-send';await ownerPage.goto(base+'/companies/synthetic/fakturaer');const row=ownerPage.getByRole('row').filter({hasText:issued.queued.number});need(await row.count()===1,'exact issued invoice row');await row.getByRole('button',{name:'Send e-faktura',exact:true}).click();const dialog=ownerPage.getByRole('dialog',{name:'Send e-faktura',exact:true});need((await dialog.textContent()).includes('5790000000074'),'review uses immutable EAN');const response=ownerPage.waitForResponse(r=>new URL(r.url()).pathname===prefix+'invoices/send-public'&&r.request().method()==='POST');await dialog.getByRole('button',{name:'Send e-faktura',exact:true}).click();const r=await response;need(r.status()===200,'queued send canonical response');const result=await r.json();need(result.submission?.status==='prepared'&&result.submission.transmissionId&&result.submission.providerStatus==='queued-for-delivery','accepted queued is not delivered');const replay=await send('queued');need(replay.status===200&&isDeepStrictEqual(replay.body.submission,result.submission),'confirmed known send returns retained canonical row');return{queued:result.submission};}
async function status(){step='existing-web-explicit-status-only';await ownerPage.reload();const row=ownerPage.getByRole('row').filter({hasText:issued.queued.number});await row.getByRole('button',{name:'Opdatér leveringsstatus',exact:true}).click();const dialog=ownerPage.getByRole('dialog',{name:'Opdatér leveringsstatus',exact:true});const response=ownerPage.waitForResponse(r=>new URL(r.url()).pathname===prefix+'invoices/send-public/status'&&r.request().method()==='POST');await dialog.getByRole('button',{name:'Kontrollér status',exact:true}).click();const r=await response;need(r.status()===200,'confirmed status response');const result=await r.json();need(result.submission?.status==='acknowledged'&&result.submission.providerStatus==='delivered','actual provider delivered observation');return result;}
async function outcomes(){step='accepted-terminal-and-unknown-never-redeliver';const result={};for(const kind of ['terminal','unknown']){const sent=await send(kind);need(sent.status===200&&sent.body.ok===true,'canonical retained outcome');const row=sent.body.submission;need(row.redeliveryBlocked===true,'retained no-redelivery contract');if(kind==='terminal')need(row.status==='failed'&&row.transmissionId,'accepted terminal rejection retains provider ID');else need(row.status==='uncertain'&&!row.transmissionId,'ambiguous delivery remains uncertain without fabricated ID');const again=await send(kind);need(again.status===200&&isDeepStrictEqual(again.body.submission,row),'exact explicit duplicate read before provider');result[kind]=row;}const before=await rows(),invalid=await send('invalid');need(invalid.status===422,'validation rejection is definite refusal');need(isDeepStrictEqual(await rows(),before),'validation refusal creates no reservation/history');result.invalid={status:invalid.status,code:invalid.body?.code};return result;}
async function holdBegin(){step='current-grant-and-real-held-validation';await post('/api/workspace/members/company',{userId:memberId,action:'grant',companySlug:'synthetic',role:'owner'});heldBefore=await rows();await memberPage.evaluate(({path,id})=>{window.publicPending=fetch(path,{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify({invoiceDocumentId:id,confirm:true})}).then(async r=>({status:r.status,body:await r.json()}));},{path:prefix+'invoices/send-public',id:issued.held.id});return{started:true};}
async function revoke(){step='ordinary-revoke-during-validation';await post('/api/workspace/members/company',{userId:memberId,action:'revoke',companySlug:'synthetic'});return{revoked:true};}
async function holdFinish(){step='fresh-authority-denies-delivery-after-validation';const r=await memberPage.evaluate(()=>window.publicPending);need(r.status===403,'revocation after I/O returns403');need(isDeepStrictEqual(await rows(),heldBefore),'revoked validation never reserves/delivers');return{status:r.status,code:r.body?.code};}
async function freeze(){step='canonical-source-stable';sourceCookie=(await ownerPage.context().cookies(base)).map(c=>`${c.name}=${c.value}`).join('; ');stable=await snapshot();await originals();need(stable.submissions.rows.length===3,'three exact retained outcomes, validation and revoked absent');await ownerPage.screenshot({path:process.env.RM_PUBLIC_EINVOICE_SCREENSHOT,fullPage:true});return{canonical:stable,issued,originals:originalsById};}
async function archive(name){step='signed-public-invoice-'+name;const r=await ownerPage.request.post(base+'/api/workspace/backup',{headers:{origin:base},data:{confirm:true}});const ct=r.headers()['content-type'];if(r.status()!==200||ct!=='application/vnd.rentemester.native-backup+json'){let body;try{body=await r.json();}catch{}detail={status:r.status(),contentType:ct,code:body?.code,errors:body?.errors};throw Error('signed public invoice backup refused');}const bytes=await r.body();need(bytes.length&&bytes.length<=96*1024*1024,'bounded full signed archive');writeFileSync(join(work,`public-einvoice-${name}.native-backup.json`),bytes,{mode:0o600,flag:'wx'});need(isDeepStrictEqual(await snapshot(),stable),'backup leaves retained domainfacts unchanged');return{sha256:hash(bytes),bytes:bytes.length};}
async function reopen(){step='ordinary-fresh-mfa-source-restart';await ownerPage.context().clearCookies();await login(ownerPage,owner);need(isDeepStrictEqual(await snapshot(),stable),'source restart exact canonical invoice and submission history');await originals();return{freshMfaRestart:true};}
async function restored(){step='source-free-fresh-owner-mfa';ownerPage=await newPage();freshOwner={email:'restored@example.invalid',password:'synthetic-restored-password-789',codes:[]};await ownerPage.goto((await mail('verification',freshOwner.email)).actionUrl);need((await enroll(ownerPage,freshOwner)).user.id!==ownerId,'no historical authority adoption');const historical=await ownerPage.request.get(base+prefix+'invoices/public-submissions',{headers:{cookie:sourceCookie}});need([401,403].includes(historical.status()),'old source session gains no restored authority');need(isDeepStrictEqual(await snapshot(),stable),'full exact source-free restored invoice/submission/history');await originals();const config=await read(prefix+'invoices/public-config');need(config.publicEInvoice.configured===false,'provider credentials absent after restore');const before=await rows(),r=await send('queued');need(r.status===422&&r.body?.code==='public_einvoice_config_required','restored send refuses absent operator configuration');need(isDeepStrictEqual(await rows(),before),'unconfigured restored request changes no retained evidence');return{sourceFreeRestoreQualified:true,credentialsAbsent:true};}
async function restoredReopen(){step='fresh-mfa-target-reopen';await ownerPage.context().clearCookies();await login(ownerPage,freshOwner);need(isDeepStrictEqual(await snapshot(),stable),'target second normal MFA reopen exact canonical evidence');await originals();return{restoredTargetReopenQualified:true};}
let clientPhase=0;
async function clientReads(){step='current-mfa-http-cli-mcp-parity';const before=await snapshot(),cookiePath=join(realpathSync(work),`public-client-${++clientPhase}.cookie`);const cookies=await ownerPage.context().cookies(base);need(cookies.some(c=>c.httpOnly),'current actual MFA session');writeFileSync(cookiePath,cookies.map(c=>`${c.name}=${c.value}`).join('; '),{mode:0o600,flag:'wx'});const common=['--server',base,'--cookie-file',cookiePath,'--allow-loopback-http'];try{for(const [command,tool,path]of[['public-invoice-config','public_invoice_config','invoices/public-config'],['public-invoice-submissions','public_invoice_submissions','invoices/public-submissions']]){const canonical=await read(prefix+path);const cli=spawnSync(join(work,'rentemester-rust'),['remote',command,...common,'--company','synthetic'],{encoding:'utf8',timeout:45000,maxBuffer:32*1024*1024});need(!cli.error&&cli.status===0,'CLI read terminal');const result=JSON.parse(cli.stdout);need(result.ok===true&&isDeepStrictEqual(result.data,canonical),'full canonical CLI/HTTP equality');const input=[{jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'synthetic-public-einvoice',version:'1'}}},{jsonrpc:'2.0',method:'notifications/initialized'},{jsonrpc:'2.0',id:2,method:'tools/call',params:{name:tool,arguments:{company:'synthetic'}}}].map(JSON.stringify).join('\n')+'\n';const mcp=spawnSync(join(work,'rentemester-mcp'),common,{input,encoding:'utf8',timeout:45000,maxBuffer:32*1024*1024});need(!mcp.error&&mcp.status===0,'MCP read terminal');const call=mcp.stdout.trim().split('\n').map(JSON.parse).find(v=>v.id===2);need(!call?.error&&!call?.result?.isError&&call.result.structuredContent?.ok===true&&isDeepStrictEqual(call.result.structuredContent.data,canonical),'full canonical MCP/HTTP equality');}if((await read(prefix+'invoices/public-config')).publicEInvoice.configured){for(const [kind,transport] of [['queued','cli'],['unknown','mcp']]){const body=JSON.stringify({invoiceDocumentId:issued[kind].id,confirm:true});let data;if(transport==='cli'){const r=spawnSync(join(work,'rentemester-rust'),['remote','send-public-invoice',...common,'--company','synthetic','--confirm'],{input:body,encoding:'utf8',timeout:45000,maxBuffer:32*1024*1024});need(!r.error&&r.status===0,'explicit known duplicate CLI terminal');data=JSON.parse(r.stdout).data;}else{const input=[{jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'synthetic-known-public-outcome',version:'1'}}},{jsonrpc:'2.0',method:'notifications/initialized'},{jsonrpc:'2.0',id:2,method:'tools/call',params:{name:'send_public_invoice',arguments:{company:'synthetic',body,confirm:true}}}].map(JSON.stringify).join('\n')+'\n';const r=spawnSync(join(work,'rentemester-mcp'),common,{input,encoding:'utf8',timeout:45000,maxBuffer:32*1024*1024});need(!r.error&&r.status===0,'explicit known duplicate MCP terminal');const response=r.stdout.trim().split('\n').map(JSON.parse).find(v=>v.id===2);need(!response.error&&!response.result?.isError,'known retained outcome MCP result');data=response.result.structuredContent.data;}const current=before.submissions.rows.find(row=>row.invoiceDocumentId===issued[kind].id);need(data?.ok===true&&isDeepStrictEqual(data.submission,current),'transport returns exact retained outcome, never resends');}}need(isDeepStrictEqual(await snapshot(),before),'all reads and explicit retained duplicate leave canonical facts unchanged');return{httpCliMcpCanonicalEquality:true};}finally{unlinkSync(cookiePath);}}
for await(const line of createInterface({input:process.stdin,crlfDelay:Infinity})){try{const action=JSON.parse(line);let value;if(action==='authenticate')value=await authenticate();else if(action==='setup')value=await setup();else if(action==='register')value=await register();else if(action==='issue')value=await issueAll();else if(action==='edit-contact')value=await editContact();else if(action==='unbound')value=await unbound();else if(action==='primary')value=await primary();else if(action==='status')value=await status();else if(action==='outcomes')value=await outcomes();else if(action==='hold')value=await holdBegin();else if(action==='revoke')value=await revoke();else if(action==='hold-finish')value=await holdFinish();else if(action==='freeze')value=await freeze();else if(action==='reopen')value=await reopen();else if(action==='restored')value=await restored();else if(action==='restored-reopen')value=await restoredReopen();else if(action.startsWith('client-reads'))value=await clientReads();else if(action.startsWith('archive-'))value=await archive(action.slice(8));else if(action==='finish'){await browser?.close();emit({ok:true,value:{finished:true}});break;}else throw Error('unknown phase');emit({ok:true,value});}catch(error){emit({ok:false,step,detail,errorType:error?.name});await browser?.close().catch(()=>{});process.exitCode=1;break;}}
'''

class Provider:
    """Synthetic official DigiSense protocol; never contacts an external service."""
    def __init__(self, key):
        self.key=key
        self.trace=[]
        self.lock=threading.Lock()
        self.held,self.release=threading.Event(),threading.Event()
        self.ids={}

    def count(self, operation=None):
        with self.lock:
            return sum(1 for item in self.trace if operation is None or item['operation']==operation)

    def request(self, handler):
        parsed=urllib.parse.urlsplit(handler.path)
        operation=parsed.path.removeprefix('/ap/api/rest/')
        if not parsed.path.startswith('/ap/api/rest/') or handler.headers.get('authorization')!=self.key:
            return handler.reply(403,{'statusCode':403})
        length=int(handler.headers.get('content-length','0'))
        if not 0<=length<=10*1024*1024:
            return handler.reply(413,{'statusCode':413})
        raw=handler.rfile.read(length)
        query=urllib.parse.parse_qs(parsed.query,strict_parsing=True)
        record={'operation':operation,'method':handler.command,'sha256':hashlib.sha256(raw).hexdigest(),'bytes':len(raw)}
        if operation in ('validate-document','deliver-document'):
            if handler.command!='POST' or 'application/xml' not in handler.headers.get('content-type',''):
                raise ValueError('fixed XML POST required')
            root=ET.fromstring(raw)
            number=root.find('{urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2}ID')
            if number is None or not number.text:
                raise ValueError('actual source invoice number required')
            fixture=next((value for value in self.ids.values() if value['number']==number.text),None)
            if fixture is None:
                raise ValueError('unregistered synthetic issued identity')
            kind=fixture['kind']
            record.update({'kind':kind,'invoiceNumber':number.text,'xml':raw.decode('utf8')})
            if '5790000000074' not in raw.decode() or '5790000000075' in raw.decode():
                raise ValueError('immutable exact issued recipient required')
            if operation=='deliver-document' and query!={'companyKey':['synthetic-company-key-74']}:
                raise ValueError('exact retained company binding required')
        else:
            kind=None
        with self.lock:
            self.trace.append(record)
        if operation=='list-registered-companies':
            if handler.command!='GET' or query or raw:
                raise ValueError('readonly exact registered-company inventory required')
            companies=[]
            if self.count('register-company'):
                companies=[{'companyType':{'type':'DK:CVR','id':'DK87654321'},'companyName':'Synthetic Public Invoice Company','createdAt':'2026-10-20T10:00:00Z','companyKey':'synthetic-company-key-74'}]
            return handler.reply(200,{'apiLicenseKey':self.key,'companyKeyConstraint':None,'companies':companies})
        if operation=='list-participants-for-company':
            if handler.command!='GET' or query!={'companyKey':['synthetic-company-key-74']} or raw:
                raise ValueError('readonly exact participant inventory required')
            participants=[]
            if self.count('v2/register-participant/nemhandel'):
                participants=[{'networkType':'nemhandel','companyKey':'synthetic-company-key-74','direction':'outbound','participantType':'DK:CVR','participantId':'DK87654321','webhookUrl':None,'documentProfiles':'default-nemhandel','createdAt':'2026-10-20T10:00:00Z'}]
            return handler.reply(200,{'apiLicenseKey':self.key,'companyKey':'synthetic-company-key-74','company':{'companyType':{'type':'DK:CVR','id':'DK87654321'},'companyName':'Synthetic Public Invoice Company','createdAt':'2026-10-20T10:00:00Z'},'participants':participants})
        if operation=='register-company':
            body=json.loads(raw)
            if handler.command!='POST' or body.get('companyType')!={'type':'DK:CVR','id':'DK87654321'} or not body.get('companyName'):
                raise ValueError('explicit legal identity registration required')
            return handler.reply(200,{'companyKey':'synthetic-company-key-74','message':None})
        if operation=='v2/register-participant/nemhandel':
            body=json.loads(raw)
            if handler.command!='POST' or body.get('direction')!='outbound' or body.get('companyKey')!='synthetic-company-key-74' or body.get('participantType')!='DK:CVR' or body.get('participantId')!='DK87654321' or body.get('webhookUrl') is not None or body.get('documentProfiles')!='default-nemhandel':
                raise ValueError('explicit outbound participant required')
            return handler.reply(200,{'registeredOnNetwork':True,'webhookRegistered':False,'message':None})
        if operation=='validate-document':
            if kind=='held':
                self.held.set()
                if not self.release.wait(25):
                    raise ValueError('bounded held validation expired')
            if kind=='invalid':
                return handler.reply(400,{'statusCode':400,'success':False,'errors':[{'context':'invoice','pattern':'synthetic','description':'Synthetic rejection','xpath':None,'type':'error'}]})
            return handler.reply(200,{'statusCode':200,'success':True,'errors':[]})
        if operation=='deliver-document':
            with self.lock:
                calls=[row for row in self.trace if row['operation']=='deliver-document' and row['kind']==kind]
                validated=[row for row in self.trace if row['operation']=='validate-document' and row.get('kind')==kind]
            if len(calls)!=1 or len(validated)!=1 or validated[0]['sha256']!=record['sha256'] or kind in ('held','invalid'):
                raise ValueError('one identical validated delivery only')
            if kind=='unknown':
                return handler.reply(502,b'bounded intentionally ambiguous synthetic response')
            code,status=(400,'document-not-valid') if kind=='terminal' else (202,'queued-for-delivery')
            return handler.reply(code,{'statusCode':code,'documentId':'synthetic-'+kind+'-74','documentStatus':status,'message':None,'publicUrl':None,'ksef':None,'verificationUrl':None,'ksefDocumentNumber':None})
        if operation.startswith('document-status/'):
            if handler.command!='GET' or operation!='document-status/synthetic-queued-74' or query!={'companyKey':['synthetic-company-key-74']}:
                raise ValueError('exact recorded ID status-only request required')
            return handler.reply(200,{'statusCode':200,'documentId':'synthetic-queued-74','documentStatus':'delivered','message':None,'publicUrl':None,'ksef':None,'verificationUrl':None,'ksefDocumentNumber':None})
        raise ValueError('unsupported synthetic provider operation')

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
    provider = Provider(secrets.token_urlsafe(32))
    bearer = secrets.token_urlsafe(32)
    backend_port, mail_port = free_port(), free_port()
    if backend_port == mail_port:
        raise RuntimeError('Distinct owned ports required')
    origin = f'http://127.0.0.1:{backend_port}'
    report = {'status':'running','sourceRevision':subprocess.check_output(['git','rev-parse','HEAD'],cwd=REPO,text=True).strip(),'runnerSha256':digest(Path(__file__)),'browserHelperSha256':hashlib.sha256(BROWSER.encode()).hexdigest(),'canonicalOrigin':origin,'noOfflineIdentitySeeds':True,'ordinaryMacOsUser':True,'ownedProfile':str(work),'externalMailDelivered':False,'workflow':'explicit_public_invoice_delivery','installedNativeQualified':False,'publicEInvoiceActualQualified':False,'sourceFreeRestoreQualified':False,'screenshot':str(args.evidence.with_suffix('.png')),'actions':[],'results':{}}
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
                self.do_POST()

            def do_POST(self):
                try:
                    if self.path.startswith('/ap/api/rest/'):
                        return provider.request(self)

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
                argv += ['--restore-backup', str(work / 'public-einvoice-source.native-backup.json'), '--trusted-backup-key', str(work / 'trusted.pub'), '--confirm-target', str(root), '--source-unavailable']
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
            progress('operator:independent-signed-public-einvoice-' + name)
            path = work / ('public-einvoice-' + name + '.native-backup.json')
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
                if any(secret.encode() in raw for secret in [bearer, provider.key, config['authMail']['idempotencySecret'], config['cookieKey'], config['mfaKey']] if secret):
                    raise RuntimeError('Private credential present in signed inventory')
            raw = entries.get('synthetic/data/ledger.sqlite')
            if not raw or len(raw) > 32*1024*1024:
                raise RuntimeError('Bounded complete signed company database missing')
            copy = work / ('readonly-public-' + name + '.sqlite')
            result = {}
            def cell(value):
                return {'bytesBase64': base64.b64encode(value).decode()} if isinstance(value, bytes) else value
            try:
                with copy.open('xb') as f:
                    f.write(raw)
                db = sqlite3.connect(copy.as_uri()+'?mode=ro', uri=True)
                try:
                    db.execute('PRAGMA query_only=ON')
                    for table in ['documents','peppol_submissions','peppol_submission_events','digisense_companies','digisense_participants','journal_entries','journal_lines','audit_log']:
                        rows = db.execute('SELECT * FROM '+table+' ORDER BY id')
                        result[table] = [{k:cell(v) for k,v in zip([c[0] for c in rows.description], row)} for row in rows.fetchall()]
                finally:
                    db.close()
            finally:
                copy.unlink(missing_ok=True)
            if len(result['peppol_submissions']) != 3 or result['journal_entries'] or result['journal_lines']:
                raise RuntimeError('Exactly three retained public submissions and no accounting effects required')
            if len(result['digisense_companies']) != 1 or len(result['digisense_participants']) != 1:
                raise RuntimeError('Actual company/outbound business binding missing')
            signed_originals={name:hashlib.sha256(value).hexdigest() for name,value in entries.items() if '/documents/' in name}
            original_proofs=report['results']['freeze']['originals']
            for identifier,proof in original_proofs.items():
                for key in ['jsonBase64','pdfBase64']:
                    wanted=base64.b64decode(proof[key],validate=True)
                    if not any(value==wanted for name,value in entries.items() if '/documents/' in name):
                        raise RuntimeError('Exact issued original bytes missing from signed inventory')
            for row in result['peppol_submissions']:
                envelope=row['envelope_xml']
                match=re.fullmatch(r'<NativePeppolSubmission xmlns="urn:rentemester-native-peppol:1"><Evidence>([0-9a-f]+)</Evidence></NativePeppolSubmission>',envelope)
                if not match:
                    raise RuntimeError('Native complete typed submission evidence missing')
                evidence=json.loads(bytes.fromhex(match.group(1)))
                source=evidence['source']
                document_id=str(source['invoiceDocumentId'])
                if source['jsonSha256']!=original_proofs[document_id]['jsonSha256'] or source['receiverEndpointId']!='5790000000074':
                    raise RuntimeError('Signed submission differs from immutable issued source')
                delivered=[r for r in provider.trace if r['operation']=='deliver-document' and r['invoiceNumber']==source['invoiceNumber']]
                if len(delivered)!=1 or source['oioublXml']!=delivered[0]['xml'] or source['oioublSha256']!=delivered[0]['sha256']:
                    raise RuntimeError('Signed exact XML/source differs from actual provider handoff')
            result['signedOriginalHashes']=signed_originals
            report.setdefault('snapshots', {})[name] = {'sha256':receipt['sha256'],'trustedPublicKeySha256':digest(trusted),'privateCredentialsExcluded':True,'tables':result}
            save()
            return result

        def preserved(original, newer):
            for table in ['documents','peppol_submissions','peppol_submission_events','digisense_companies','digisense_participants','journal_entries','journal_lines','signedOriginalHashes']:
                if original[table] != newer[table]:
                    raise RuntimeError('Exact restored public invoice evidence differs: '+table)
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
                ['openssl', 'req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', str(work / 'ca.key'), '-out', str(ca), '-days', '1', '-subj', '/CN=Synthetic Public Invoice Auth CA', '-addext', 'basicConstraints=critical,CA:TRUE', '-addext', 'keyUsage=critical,keyCertSign,cRLSign'],
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
            config = {'runtimeProfile': 'local-user-macos', 'root': str(work / 'workspace'), 'brokerUid': os.getuid(), 'workspace': 'synthetic-native-public-einvoice', 'companySlug': 'synthetic', 'companyName': 'Synthetic Public Invoice Company', 'cookieKey': key(), 'mfaKey': key(), 'operatorActors': ['system:synthetic-installer'], 'accounts': [], 'firstOwner': {'actor': 'system:synthetic-installer', 'email': 'owner@example.invalid', 'name': 'Synthetic Owner', 'password': 'synthetic-owner-password-123'}, 'backupSigningKeyFile': str(work / 'signing.der'), 'authMail': {'endpoint': f'https://127.0.0.1:{mail_port}/mail', 'bearerToken': bearer, 'from': 'auth@example.invalid', 'idempotencySecret': secrets.token_urlsafe(32), 'trustedCaFile': str(ca)}}
            config['publicEInvoice']={'synthetic':{'apiLicenseKey':provider.key,'environment':'test','baseUrl':f'https://127.0.0.1:{mail_port}/ap/api/rest','trustedCaPem':ca.read_text(),'timeoutMs':30000}}
            write_config()
            start(create=True)
            script = work / 'browser.mjs'
            script.write_text(BROWSER)
            controller = subprocess.Popen(['/opt/homebrew/bin/bun', str(script)], cwd=REPO, env={**os.environ, 'RM_PUBLIC_EINVOICE_BASE': origin, 'RM_PUBLIC_EINVOICE_WORK': str(work), 'RM_PUBLIC_EINVOICE_SCREENSHOT': str(args.evidence.with_suffix('.png')), 'RM_PUBLIC_EINVOICE_PLAYWRIGHT': str((REPO / 'node_modules/@playwright/test/index.mjs').resolve(strict=True))}, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, bufsize=1, start_new_session=True)
            report['browserControllerPid'] = controller.pid
            phase('authenticate')
            phase('setup')
            if provider.count():
                raise RuntimeError('Status/denied setup unexpectedly called provider')
            phase('issue')
            fixture=json.loads((work/'issued-fixtures.json').read_bytes())
            with provider.lock:
                provider.ids={kind:{**value,'kind':kind} for kind,value in fixture.items()}
            phase('edit-contact')
            phase('unbound')
            if provider.count():
                raise RuntimeError('Unbound invoice must not call provider')
            phase('register')
            if provider.count('register-company')!=1 or provider.count('v2/register-participant/nemhandel')!=1 or provider.count('list-registered-companies')!=1 or provider.count('list-participants-for-company')!=1:
                raise RuntimeError('One explicit company and outbound registration required')
            phase('primary')
            if provider.count('deliver-document')!=1:
                raise RuntimeError('Primary plus repeated send must deliver exactly once')
            phase('status')
            if provider.count('deliver-document')!=1:
                raise RuntimeError('Status refresh performed forbidden delivery')
            phase('outcomes')
            if provider.count('deliver-document')!=3:
                raise RuntimeError('Only primary/terminal/unknown each deliver once')
            phase('hold')
            if not provider.held.wait(8):
                raise RuntimeError('Real validation request not held')
            phase('revoke',3)
            provider.release.set()
            phase('hold-finish')
            if provider.count('deliver-document')!=3:
                raise RuntimeError('Revoked validation delivered an invoice')
            phase('freeze')
            calls=provider.count()
            phase('client-reads-source')
            if provider.count()!=calls:
                raise RuntimeError('HTTP/CLI/MCP reads contacted provider')
            phase('archive-source')
            original = inspect_snapshot('source', work/'trusted.pub')
            stop()
            config.pop('firstOwner')
            write_config()
            calls = provider.count()
            start()
            phase('reopen')
            phase('client-reads-restarted')
            if provider.count()!=calls:
                raise RuntimeError('Source restart or reads unexpectedly contacted provider')
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
            config.pop('publicEInvoice')
            config.update({'root':str(work/'restored'),'backupSigningKeyFile':str(work/'target-signing.der'),'cookieKey':key(),'mfaKey':key(),'restoreActor':'system:synthetic-installer','firstOwner':{'actor':'system:synthetic-installer','email':'restored@example.invalid','name':'Synthetic Restored Owner','password':'synthetic-restored-password-789'}})
            write_config()
            calls = provider.count()
            start(restore=True)
            phase('restored')
            phase('client-reads-restored')
            if provider.count() != calls:
                raise RuntimeError('Fresh restore contacted provider')
            report['sourceFreeRestoreQualified'] = True
            phase('archive-restored')
            preserved(original, inspect_snapshot('restored',work/'target-trusted.pub'))
            stop()
            config.pop('firstOwner');config.pop('restoreActor');write_config();start()
            phase('restored-reopen')
            phase('client-reads-reopened')
            if provider.count() != calls:
                raise RuntimeError('Restored target reopen contacted provider')
            phase('archive-reopened')
            if inspect_snapshot('reopened',work/'target-trusted.pub') != report['snapshots']['restored']['tables']:
                raise RuntimeError('Fresh target reopen changed retained full public invoice evidence')
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
            report['provider']={'verifiedTls':True,'trustedCaSha256':digest(ca),'requests':provider.trace,'exactlyThreeDeliveries':provider.count('deliver-document')==3,'noRetries':True}
            with gateway_lock:
                if errors:
                    raise RuntimeError('Private auth gateway protocol error')
                report['gateway']={'verifiedHttps':True,'acceptedMessages':len(gateway_rows),'requests':list(gateway_requests)}
            report.update({'status':'passed','currentAction':'complete','publicEInvoiceActualQualified':True,'httpCliMcpReadParityQualified':True,'snapshotCoverage':'four-independently-verified-public-invoice-full-source-submission-event-archives-fresh-owner-restore-and-reopen'})
            save()
            print('PASS: ordinary MFA public registration, verifiedTLS validation/delivery/status, exact canonical clients and signed source-free recovery; '+str(args.evidence))
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
            provider.release.set()
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
    """Fixture protocol ONLY: no Rentemester, ledger, actor seed or external provider."""
    os.umask(0o077)
    with tempfile.TemporaryDirectory(prefix='rm74-public-provider-') as directory:
        work=Path(directory)
        ca,cert,key=work/'ca.pem',work/'cert.pem',work/'key.pem'
        (work/'ext').write_text('basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\nsubjectAltName=IP:127.0.0.1\n')
        commands=[['openssl','req','-x509','-newkey','rsa:2048','-nodes','-keyout',str(work/'ca.key'),'-out',str(ca),'-days','1','-subj','/CN=Synthetic Public Invoice CA','-addext','basicConstraints=critical,CA:TRUE','-addext','keyUsage=critical,keyCertSign,cRLSign'],['openssl','req','-new','-newkey','rsa:2048','-nodes','-keyout',str(key),'-out',str(work/'csr'),'-subj','/CN=localhost'],['openssl','x509','-req','-in',str(work/'csr'),'-CA',str(ca),'-CAkey',str(work/'ca.key'),'-CAcreateserial','-out',str(cert),'-days','1','-extfile',str(work/'ext')]]
        for argv in commands:
            subprocess.run(argv,check=True,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=30)
        provider=Provider('synthetic-private-protocol-key')
        provider.ids={kind:{'kind':kind,'number':'SYNTH-'+kind} for kind in ['queued','terminal','unknown','invalid','held']}
        failures=[]
        class Handler(http.server.BaseHTTPRequestHandler):
            def log_message(self,*_args): pass
            def reply(self,status,value):
                raw=value if isinstance(value,bytes) else json.dumps(value).encode()
                self.send_response(status);self.send_header('content-type','application/json');self.send_header('content-length',str(len(raw)));self.end_headers();self.wfile.write(raw)
            def do_GET(self): self.do_POST()
            def do_POST(self):
                try: provider.request(self)
                except Exception:
                    failures.append('protocol-refusal');self.reply(400,{'statusCode':400})
        server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler)
        server.daemon_threads=True
        tls=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER);tls.load_cert_chain(cert,key)
        server.socket=tls.wrap_socket(server.socket,server_side=True)
        threading.Thread(target=server.serve_forever,daemon=True).start()
        def request(path,body=None,method='POST'):
            connection=http.client.HTTPSConnection('127.0.0.1',server.server_port,timeout=5,context=ssl.create_default_context(cafile=str(ca)))
            try:
                raw=body if isinstance(body,bytes) else json.dumps(body).encode() if body is not None else None
                connection.request(method,'/ap/api/rest/'+path,raw,{'authorization':provider.key,'content-type':'application/xml' if isinstance(body,bytes) else 'application/json'})
                response=connection.getresponse()
                return response.status,response.read()
            finally: connection.close()
        def xml(kind):
            return ('<Invoice xmlns="urn:oasis:names:specification:ubl:schema:xsd:Invoice-2" xmlns:cbc="urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2"><cbc:ID>SYNTH-'+kind+'</cbc:ID><cbc:EndpointID>5790000000074</cbc:EndpointID></Invoice>').encode()
        try:
            code,raw=request('list-registered-companies',method='GET');assert code==200 and json.loads(raw)['companies']==[]
            code,_=request('register-company',{'companyType':{'type':'DK:CVR','id':'DK87654321'},'companyName':'Synthetic Public Invoice Company'});assert code==200
            code,raw=request('list-participants-for-company?companyKey=synthetic-company-key-74',method='GET');assert code==200 and json.loads(raw)['participants']==[]
            code,_=request('v2/register-participant/nemhandel',{'direction':'outbound','participantType':'DK:CVR','participantId':'DK87654321','companyKey':'synthetic-company-key-74','webhookUrl':None,'documentProfiles':'default-nemhandel'});assert code==200
            for kind in ['queued','terminal','unknown']:
                code,raw=request('validate-document',xml(kind));assert code==200 and json.loads(raw)['success'] is True
                code,raw=request('deliver-document?companyKey=synthetic-company-key-74',xml(kind))
                assert code=={'queued':202,'terminal':400,'unknown':502}[kind]
                if kind!='unknown':assert json.loads(raw)['documentId']=='synthetic-'+kind+'-74'
                else:assert raw==b'bounded intentionally ambiguous synthetic response'
            code,raw=request('document-status/synthetic-queued-74?companyKey=synthetic-company-key-74',method='GET');assert code==200 and json.loads(raw)['documentStatus']=='delivered'
            code,raw=request('validate-document',xml('invalid'));assert code==400 and json.loads(raw)['success'] is False
            held=[]
            thread=threading.Thread(target=lambda:held.append(request('validate-document',xml('held'))));thread.start()
            assert provider.held.wait(3);provider.release.set();thread.join(5)
            assert not thread.is_alive() and held[0][0]==200
            assert provider.count('deliver-document')==3 and not failures
            assert all('authorization' not in row and provider.key not in json.dumps(row) for row in provider.trace)
            print('PASS: verifiedTLS synthetic official registration/list/outbound/validate/deliver/status, terminal/ambiguous/refused/held protocols; no product run')
        finally:
            provider.release.set();server.shutdown();server.server_close()


if __name__ == '__main__':
    import sys
    if sys.argv[1:]==['--self-test']:
        self_test()
    else:
        main()
