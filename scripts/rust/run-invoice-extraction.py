#!/usr/bin/env python3
"""Owned normal-MFA explicit invoice extraction, manual expense and signed recovery.
Uses only supplied pinned native artifacts, a private verified-TLS synthetic provider,
public product APIs and existing web forms. No identity/business SQL seeds, provider
retries, inferred metadata or accounting values. Signed SQLite copies are query-only.
Automatic extraction on ingest and installed native GUI are not qualified here.
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
PREFIX = 'Rentemester-Synthetic-Invoice-Extraction-'
BROWSER = r'''







import {createHmac,createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {createInterface} from 'node:readline';
import {readFileSync,readdirSync,writeFileSync,unlinkSync} from 'node:fs';
import {join} from 'node:path';
import {isDeepStrictEqual} from 'node:util';
const {chromium}=await import(process.env.RM_EXTRACTION_PLAYWRIGHT);
const base=process.env.RM_EXTRACTION_BASE,work=process.env.RM_EXTRACTION_WORK;
const originalMode=process.env.RM_EXTRACTION_ORIGINAL==='1';
let original;
const originalFactory=originalMode?(await import(process.env.RM_EXTRACTION_ORIGINAL_HELPER)).originalExtractionProof:null;
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
function pdf(stream){const objects=['<< /Type /Catalog /Pages 2 0 R >>','<< /Type /Pages /Kids [3 0 R] /Count 1 >>','<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>',`<< /Length ${Buffer.byteLength(stream,'ascii')} >>\nstream\n${stream}endstream`,'<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>'];let value='%PDF-1.4\n%\xE2\xE3\xCF\xD3\n';const offsets=[];for(let i=0;i<objects.length;i++){offsets.push(Buffer.byteLength(value,'binary'));value+=`${i+1} 0 obj\n${objects[i]}\nendobj\n`;}const xref=Buffer.byteLength(value,'binary');value+=`xref\n0 ${objects.length+1}\n0000000000 65535 f \n`+offsets.map(n=>String(n).padStart(10,'0')+' 00000 n \n').join('')+`trailer\n<< /Size ${objects.length+1} /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`;return Buffer.from(value,'binary');}

const kinds=['good','conflict','malformed','unavailable','held'];
const fixtures={};
function prepareFixtures(){for(const kind of kinds){const amount=kind==='conflict'?12:10;const values={invoiceNumber:'SYN70-'+kind,supplierName:'Synthetic Extraction Supplier',supplierCountry:'DK',supplierLegalId:'12345678',supplierLegalIdKind:'dk_cvr',buyerName:buyer.name,buyerCountry:buyer.country,buyerLegalId:buyer.cvr,buyerLegalIdKind:'dk_cvr',invoiceDate:today,currency:'DKK',netAmount:amount,vatAmount:0,grossAmount:amount,reverseChargeWording:false};const lines=['SYN70-'+kind,...Object.entries(values).map(([k,v])=>k+': '+String(v))];const stream='BT\n/F1 10 Tf\n72 720 Td\n'+lines.map((line,i)=>(i?'0 -18 Td\n':'')+'('+line+') Tj\n').join('')+'ET\n';fixtures[kind]=pdf(stream);}}
const documents={},extractions={};let today,buyer,sourceBooks,uploaded,stable,bank,freshOwner;
const read=p=>get(ownerPage,p);
const path=kind=>prefix+`documents/${documents[kind].id}/invoice-extraction`;
const extractPath=kind=>prefix+`documents/${documents[kind].id}/extract-invoice`;
async function call(path,data,page=ownerPage){return fetchJson(page,path,data);}
async function post(path,data){const r=await call(path,data);need(r.status===200&&r.body?.ok===true,'ordinary explicit operation '+path+' HTTP'+r.status);return r.body;}
async function books(){return{journal:await read(prefix+'journal'),trialBalance:await read(prefix+'trial-balance'),income:await read(prefix+'income-statement'),balance:await read(prefix+'balance')};}
async function snapshot(){const evidence={};for(const kind of kinds)evidence[kind]=await read(path(kind));return{documents:await read(prefix+'documents'),books:await books(),bank:await read(prefix+'bank'),extractions:evidence};}
async function originals(){for(const kind of kinds){const r=await ownerPage.request.get(base+prefix+`documents/${documents[kind].id}/file`);need(r.status()===200&&r.headers()['content-type'].split(';')[0]==='application/pdf'&&hash(await r.body())===hash(fixtures[kind]),'exact authorized PDF original '+kind);}}
async function setup(){if(originalMode)original=originalFactory({base,work,need,ownerPage,documents,read,prefix,post,path,kinds});step='explicit-company-identity-and-chart';today=(await read(prefix+'scheduled-recurring')).scheduledRecurring.asOfDate;need(/^\d{4}-\d{2}-\d{2}$/.test(today),'trusted Copenhagen date');const changed=await ownerPage.evaluate(async path=>{const r=await fetch(path,{method:'PATCH',headers:{'content-type':'application/json'},body:JSON.stringify({cvr:'87654321'})});return{status:r.status,body:await r.json()};},prefix+'company');need(changed.status===200&&changed.body.ok===true,'ordinary explicit buyer CVR');const profile=await read(prefix+'company');buyer=profile.company;need(buyer?.name&&buyer.country==='DK'&&buyer.cvr==='DK87654321','canonical buyer validation identity');if(!originalMode){for(const[accountNo,type,normalBalance]of[['1870','asset','debit'],['4870','expense','debit']])await post(prefix+'accounts',{accountNo,name:'Synthetic Extraction '+accountNo,type,normalBalance});await post(prefix+'account-roles',{role:'bank',accountNo:'1870',confirm:true});await post(prefix+'bank-accounts',{slug:'synthetic-extraction-bank',name:'Synthetic extraction bank',currency:'DKK',ledgerAccountNo:'1870'});}sourceBooks=await books();need(sourceBooks.journal.journal.entries.length===0,'empty legitimate starting journal');return{today,buyer,sourceBooks};}
async function upload(){prepareFixtures();for(const kind of kinds){step='web-upload-'+kind+'-open';await ownerPage.goto(base+'/companies/synthetic/bilag');await ownerPage.getByRole('heading',{name:'Bilag',exact:true}).waitFor();await ownerPage.getByRole('button',{name:'Indlæs bilag',exact:true}).click();const dialog=ownerPage.getByRole('dialog',{name:'Indlæs bilag',exact:true});step='web-upload-'+kind+'-explicit-fields';await dialog.getByLabel('Bilagsfil',{exact:true}).setInputFiles({name:'synthetic-extraction-'+kind+'.pdf',mimeType:'application/pdf',buffer:fixtures[kind]});await dialog.getByLabel('Bilagstype',{exact:true}).selectOption('cash_register_receipt');await dialog.getByLabel('Kilde',{exact:true}).fill('Synthetic explicit extraction '+kind);await dialog.getByLabel('Bilagsdato (valgfri)',{exact:true}).fill(today);await dialog.getByLabel('Beløb inkl. moms (valgfri)',{exact:true}).fill('10,00');await dialog.getByLabel('Momsbeløb (valgfri)',{exact:true}).fill('0,00');step='web-upload-'+kind+'-submit-once';const wait=ownerPage.waitForResponse(r=>new URL(r.url()).pathname===prefix+'documents/ingest'&&r.request().method()==='POST');await dialog.getByRole('button',{name:'Indlæs bilag',exact:true}).click();const response=await wait,value=await response.json();need(response.status()===200&&value.ok===true&&Number.isSafeInteger(value.document.id),'real registered '+kind+' PDF');const sent=response.request().postDataJSON();need(sent.fileBase64===fixtures[kind].toString('base64')&&sent.confirm===true,'web retains chosen exact PDF bytes');documents[kind]=value.document;await dialog.getByText('Bilaget er indlæst som '+value.document.documentNo+'.',{exact:true}).waitFor();await dialog.getByRole('button',{name:'Luk',exact:true}).click();need((await read(path(kind))).extraction===null,'upload does not implicitly extract '+kind);}uploaded=await read(prefix+'documents');need(isDeepStrictEqual(await books(),sourceBooks),'ingest books nothing');await originals();return{documents,originals:Object.fromEntries(kinds.map(k=>[k,{sha256:hash(fixtures[k]),bytes:fixtures[k].length}]))};}
async function disabled(){step='explicit-disabled-provider-refusal-no-attempt';const r=await call(extractPath('good'),{confirm:true});need(r.status===503&&r.body?.ok===false,'disabled provider refuses explicit extraction');need(JSON.stringify(r.body).includes('EXTRACTION_PROVIDER_UNAVAILABLE'),'curated absent provider code');need((await read(path('good'))).extraction===null,'disabled provider creates no attempt');return{refusal:r};}
async function unauthorized(){step='reviewer-without-master-data-refused-before-provider';const r=await call(extractPath('held'),{confirm:true},memberPage);need(r.status===403&&r.body?.ok===false,'Reviewer lacks CompanyMasterData');need((await read(path('held'))).extraction===null,'denied extraction creates no attempt');await post('/api/workspace/members/company',{userId:memberId,action:'grant',companySlug:'synthetic',role:'bookkeeper'});return{refusal:r,currentMasterDataBookkeeperGranted:true};}
async function providerReady(){step='source-reopen-configured-provider-fresh-mfa';await ownerPage.context().clearCookies();await login(ownerPage,owner);return{configuredReopen:true};}
function evidence(value,kind,status){need(value?.ok===true&&value.extraction?.documentId===documents[kind].id&&value.extraction.status===status&&value.extraction.originalHash===hash(fixtures[kind])&&Number.isSafeInteger(value.extraction.attemptId),'canonical '+kind+' evidence identity/status/source');need(Array.isArray(value.extraction.fields)&&Array.isArray(value.extraction.resolutions)&&Array.isArray(value.extraction.conflicts),'source-shaped field/resolution/conflict surface');return value.extraction;}
async function completed(){step='explicit-extraction-cited-completion';const first=await post(extractPath('good'),{confirm:true}),e=evidence(first,'good','completed');need(e.fields.length===15&&e.conflicts.length===0&&e.exceptionState===null,'all fifteen cited fields complete without exception');const expected={invoiceNumber:'SYN70-good',supplierName:'Synthetic Extraction Supplier',supplierCountry:'DK',supplierLegalId:'12345678',supplierLegalIdKind:'dk_cvr',buyerName:buyer.name,buyerCountry:buyer.country,buyerLegalId:buyer.cvr,buyerLegalIdKind:'dk_cvr',invoiceDate:today,currency:'DKK',netAmount:10,vatAmount:0,grossAmount:10,reverseChargeWording:false};for(const f of e.fields){need(Object.hasOwn(expected,f.key)&&isDeepStrictEqual(f.value,expected[f.key])&&f.confidence===0.99&&f.page===1&&typeof f.sourceText==='string'&&f.sourceText.length>0&&isDeepStrictEqual(f.box,{x:72,y:100,width:250,height:14}),'exact source field/citation '+f.key);if(f.key.endsWith('LegalId'))need(f.sourceText.includes(String(f.value)),'exact identifier cited');}need(isDeepStrictEqual(await read(path('good')),first),'POST and GET canonical evidence equal');step='explicit-current-context-cache';const duplicate=await post(extractPath('good'),{confirm:true});need(isDeepStrictEqual(duplicate,first),'execution cache returns exact original attempt/history');need(isDeepStrictEqual(await read(prefix+'documents'),uploaded)&&isDeepStrictEqual(await books(),sourceBooks),'completed extraction never mutates document metadata or books');extractions.good=first;return{first,cached:duplicate};}
async function negatives(){const result={};for(const kind of['conflict','malformed','unavailable']){step='typed-provider-'+kind+'-retained-needs-resolution';const r=await post(extractPath(kind),{confirm:true}),e=evidence(r,kind,'needs_resolution');need(e.conflicts.length>0&&e.exceptionState?.status==='open','curated exception and unresolved status '+kind);if(kind!=='conflict')need(e.conflicts.includes('EXTRACTION_PROVIDER_UNAVAILABLE'),'runtime failure exposes only curated unavailable code');if(kind==='conflict')need(e.fields.find(f=>f.key==='grossAmount')?.value===12,'conflicting evidence retained without adopting amount');need(isDeepStrictEqual(await read(path(kind)),r),'exact failed write/read evidence');extractions[kind]=r;result[kind]=r;need(isDeepStrictEqual(await read(prefix+'documents'),uploaded)&&isDeepStrictEqual(await books(),sourceBooks),'failure/conflict neither edits metadata nor posts');}return result;}
async function holdBegin(){if(originalMode)return original.holdBegin();step='authorized-bookkeeper-starts-held-extraction';await memberPage.evaluate(({path})=>{window.extractionPending=fetch(path,{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify({confirm:true})}).then(async r=>({status:r.status,body:await r.json()}));},{path:extractPath('held')});return{requestStarted:true};}
async function revoke(){if(originalMode)return original.revoke();step='public-owner-revokes-master-data-while-provider-held';const result=await post('/api/workspace/members/company',{userId:memberId,action:'revoke',companySlug:'synthetic'});return{revokedBeforeProviderRelease:true,result};}
async function holdFinish(){if(originalMode){const result=await original.holdFinish();extractions.held=result.evidence;return result;}step='held-provider-cannot-finalize-revoked-principal';const r=await memberPage.evaluate(()=>window.extractionPending);need(r.status===403&&r.body?.ok===false,'current authority rechecked after held provider');const current=await read(path('held')),e=evidence(current,'held','started');need(e.fields.length===0&&e.resolutions.length===0&&e.conflicts.length===0,'revoked attempt remains historically started without manufactured terminal cleanup');extractions.held=current;need(isDeepStrictEqual(await read(prefix+'documents'),uploaded)&&isDeepStrictEqual(await books(),sourceBooks),'revoked result cannot modify metadata/books');return{refusal:r,evidence:current};}
async function book(){if(originalMode){stable=await snapshot();await originals();await ownerPage.screenshot({path:process.env.RM_EXTRACTION_SCREENSHOT,fullPage:true});return{canonical:stable,noFinancialReplay:true};}step='independent-bank-original-for-explicit-manual-expense';const imported=await post(prefix+'bank/import',{confirm:true,csvContent:`dato;tekst;beløb;valuta;saldo\n${today};Synthetic extraction explicit purchase;-10,00;DKK;-10,00\n`,account:'synthetic-extraction-bank'});need(imported.import.imported===1,'actual retained outgoing bank original');bank=(await read(prefix+'bank')).bank.transactions.find(r=>r.text==='Synthetic extraction explicit purchase');need(bank?.amount===-10,'explicit bank amount');step='manual-expense-exact-source-document';await ownerPage.goto(base+'/companies/synthetic/bilag');await ownerPage.getByRole('row').filter({hasText:documents.good.documentNo}).getByRole('link',{name:'Bogfør bilag',exact:true}).click();await ownerPage.getByRole('heading',{name:'Bogfør bilag',exact:true,level:1}).waitFor();step='manual-expense-explicit-account-bank-vat';await ownerPage.getByLabel(/^Udgiftskonto/).selectOption('4870');await ownerPage.getByLabel(/^Banktransaktion \(uafstemt, udgående\)/).selectOption(String(bank.id));await ownerPage.getByLabel(/^Moms-behandling/).selectOption('exempt');const wait=ownerPage.waitForResponse(r=>new URL(r.url()).pathname===prefix+'documents/book-expense'&&r.request().method()==='POST');step='one-explicit-manual-booking';await ownerPage.getByRole('button',{name:'Bogfør',exact:true}).click();const response=await wait,receipt=await response.json();need(response.status()===200&&receipt.ok===true,'explicit expense receipt');await ownerPage.getByText(/Bilaget blev bogført som journalpost/).waitFor();const b=await books();need(b.journal.journal.entries.length===1,'only one explicit journal after all extraction actions');const j=b.journal.journal.entries[0];need(j.documentId===documents.good.id&&j.lines.some(l=>l.accountNo==='4870'&&l.debit===10)&&j.lines.some(l=>l.accountNo==='1870'&&l.credit===10),'manual explicit account/amounts, not provider inference');const payment=(await read(prefix+'bank')).bank;need(payment.matchedCount===1&&payment.unmatchedCount===0&&payment.bookedBalance===-10&&payment.actualBalance===-10,'one canonical bank relation and closing reconcile');stable=await snapshot();await originals();await ownerPage.screenshot({path:process.env.RM_EXTRACTION_SCREENSHOT,fullPage:true});return{receipt,canonical:stable,manualAmountNotExtracted:true};}
async function client(kind,write,id){const file=join(work,'extraction-client.cookie');writeFileSync(file,(await ownerPage.context().cookies(base)).map(c=>`${c.name}=${c.value}`).join('; '),{mode:0o600,flag:'wx'});try{const common=['--server',base,'--cookie-file',file,'--allow-loopback-http'];let argv,input;if(kind==='cli')argv=[originalMode?'documents':'remote',write?'extract-invoice':'invoice-extraction',...common,'--company','synthetic','--document-id',String(id),...(write&&!originalMode?['--confirm']:[])];else{argv=common;input=[{jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'synthetic-extraction-proof',version:'1'}}},{jsonrpc:'2.0',method:'notifications/initialized'},{jsonrpc:'2.0',id:2,method:'tools/call',params:{name:write?'documents_extract_invoice':'documents_invoice_extraction',arguments:{company:'synthetic',documentId:id,...(write?{confirm:true}:{})}}}].map(v=>JSON.stringify(v)).join('\n')+'\n';}const r=spawnSync(join(work,kind==='cli'?'rentemester-rust':'rentemester-mcp'),argv,{input,encoding:'utf8',timeout:45000,maxBuffer:16*1024*1024});need(!r.error&&r.status===0,kind+' source-name adapter terminal');const value=kind==='cli'?JSON.parse(r.stdout):r.stdout.trim().split('\n').map(s=>JSON.parse(s)).find(v=>v.id===2)?.result?.structuredContent;if(kind==='cli'&&originalMode){need(value?.ok===true&&Object.keys(value).sort().join()==='extraction,ok','flat original CLI');return value;}if(kind==='mcp'){need(value?.ok===true&&Object.keys(value.data??{}).join()==='extraction','source MCP envelope');return{ok:true,extraction:value.data.extraction};}need(value?.ok===true&&value.data?.ok===true,'full canonical CLI envelope');return value.data;}finally{unlinkSync(file);}}
async function clients(write=false){if(originalMode){if(!write)return{cookieReadsOnly:await originalReads()};return original.clients();}step='actual-source-name-http-cli-mcp-full-parity';const results={};for(const kind of['cli','mcp']){results[kind]={};for(const name of kinds){const expected=await read(path(name)),actual=await client(kind,false,documents[name].id);need(isDeepStrictEqual(actual,expected),'full exact HTTP/'+kind+' canonical '+name);results[kind][name]=actual;}if(write){const actual=await client(kind,true,documents.good.id);need(isDeepStrictEqual(actual,extractions.good),'confirmed client cache does not create another attempt');}}need(isDeepStrictEqual(await snapshot(),stable),'all read/client cache actions preserve exact canonical metadata/books/history');return results;}
async function originalReads(){const results={};for(const kind of ['cli','mcp'])for(const name of kinds){const actual=await client(kind,false,documents[name].id);need(isDeepStrictEqual(actual,await read(path(name))),'retained source projection after restart');results[kind+'-'+name]=actual;}return results;}
async function assertStable(){need(isDeepStrictEqual(await snapshot(),stable),'exact source history/metadata/books/UUID survive restart or source-free restore');await originals();}
async function reopen(){step='ordinary-owner-fresh-mfa-source-restart';await ownerPage.context().clearCookies();await login(ownerPage,owner);await assertStable();return{freshMfaRestart:true};}
async function archive(name){step='explicit-signed-extraction-and-originals-'+name;const r=await ownerPage.request.post(base+'/api/workspace/backup',{headers:{origin:base},data:{confirm:true}});need(r.status()===200&&r.headers()['content-type']==='application/vnd.rentemester.native-backup+json','actual signed extraction backup');const bytes=await r.body();need(bytes.length>0&&bytes.length<=96*1024*1024,'bounded complete signed artifact');writeFileSync(join(work,'extraction-'+name+'.native-backup.json'),bytes,{mode:0o600,flag:'wx'});await assertStable();return{sha256:hash(bytes),bytes:bytes.length,documents};}
async function restored(){step='source-free-fresh-owner-normal-verification-mfa-provider-absent';ownerPage=await newPage();freshOwner={email:'restored@example.invalid',password:'synthetic-restored-password-789',codes:[]};await ownerPage.goto((await mail('verification',freshOwner.email)).actionUrl);need((await enroll(ownerPage,freshOwner)).user.id!==ownerId,'fresh restored owner differs from retained actors');await assertStable();return{sourceFreeRestoreQualified:true,historicalEvidenceWithoutProviderCredentialsOrCalls:true};}
async function restoredReopen(){step='fresh-mfa-restored-target-reopen';await ownerPage.context().clearCookies();await login(ownerPage,freshOwner);await assertStable();return{restoredTargetReopenQualified:true};}
for await(const line of createInterface({input:process.stdin,crlfDelay:Infinity})){try{const action=JSON.parse(line);let value;if(action==='authenticate')value=await authenticate();else if(action==='setup')value=await setup();else if(action==='upload')value=await upload();else if(action==='disabled')value=await disabled();else if(action==='unauthorized')value=await unauthorized();else if(action==='provider-ready')value=await providerReady();else if(action==='original-prepare')value=await original.prepare();else if(action==='completed')value=await completed();else if(action==='negatives')value=await negatives();else if(action==='hold-begin')value=await holdBegin();else if(action==='revoke')value=await revoke();else if(action==='hold-finish')value=await holdFinish();else if(action==='book')value=await book();else if(action==='clients')value=await clients(true);else if(action==='client-reads')value=await clients(false);else if(action==='reopen')value=await reopen();else if(['archive-source','archive-restarted','archive-restored','archive-reopened'].includes(action))value=await archive(action.slice(8));else if(action==='restored')value=await restored();else if(action==='restored-reopen')value=await restoredReopen();else if(action==='finish'){await browser?.close();emit({ok:true,value:{finished:true}});break;}else throw Error('unknown phase');emit({ok:true,value});}catch(error){let pageState=null;try{pageState=await Promise.race([diagnosticPage?.evaluate(()=>({pathname:location.pathname,headings:[...document.querySelectorAll('h1,h2,h3,h4')].map(e=>e.textContent.slice(0,100)),buttons:[...document.querySelectorAll('button')].map(e=>e.textContent.slice(0,100)),alerts:[...document.querySelectorAll('[role=alert]')].map(e=>e.textContent.slice(0,300))})),new Promise(resolve=>setTimeout(()=>resolve(null),2000))]);}catch{}emit({ok:false,step,detail,errorType:error?.name,trace,pageState});await Promise.race([browser?.close().catch(()=>{}),new Promise(resolve=>setTimeout(resolve,2000))]);process.exitCode=1;break;}}
'''

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def free_port():
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0))
        return sock.getsockname()[1]


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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--original-interfaces', action='store_true', help='Bounded original 2CLI/2MCP proof; no manual financial replay')
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
    provider_requests = []
    provider_token = secrets.token_urlsafe(32)
    held_received, held_release = threading.Event(), threading.Event()
    gateway_lock = threading.Lock()
    bearer = secrets.token_urlsafe(32)
    backend_port, mail_port = free_port(), free_port()
    if backend_port == mail_port:
        raise RuntimeError('Distinct owned ports required')
    origin = f'http://127.0.0.1:{backend_port}'
    report = {'status': 'running', 'sourceRevision': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=REPO, text=True).strip(), 'runnerSha256': digest(Path(__file__)), 'browserHelperSha256': hashlib.sha256(BROWSER.encode()).hexdigest(), 'canonicalOrigin': origin, 'noOfflineIdentitySeeds': True, 'ordinaryMacOsUser': True, 'ownedProfile': str(work), 'externalMailDelivered': False, 'workflow': 'explicit_invoice_extraction', 'invoiceExtractionActualQualified': False, 'autoOnIngestActualQualified': False, 'currentRevocationDuringExtractionQualified': False, 'installedNativeQualified': False, 'screenshot': str(args.evidence.with_suffix('.png')),  'originalInterfaces': args.original_interfaces, 'actions': [], 'results': {}, 'sourceFreeRestoreQualified': False}
    args.evidence.parent.mkdir(parents=True, exist_ok=True)
    with args.evidence.open('x') as evidence:
        def save():
            report['providerRequestTrace'] = list(provider_requests)
            report['providerRequestCount'] = len(provider_requests)
            report['scanner'] = {'privateGatewayErrors': list(errors), 'acceptedMailCount': len(gateway_rows)}
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
                if self.path == '/extract':
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
                    if not 0 < length <= 14 * 1024 * 1024 or self.headers.get('authorization') != 'Bearer ' + provider_token or self.headers.get('content-type') != 'application/json' or self.headers.get('accept') != 'application/json':
                        raise ValueError('Private extraction authority or envelope')
                    request = json.loads(self.rfile.read(length))
                    if set(request) != {'sha256', 'pdfBytesBase64'}:
                        raise ValueError('Exact extraction request fields')
                    raw = base64.b64decode(request['pdfBytesBase64'], validate=True)
                    match = re.search(rb'\(SYN70-(good|conflict|malformed|unavailable|held)\)', raw)
                    if not match or not raw.startswith(b'%PDF-1.4') or request['sha256'] != hashlib.sha256(raw).hexdigest():
                        raise ValueError('Exact PDF and SHA required')
                    kind = match[1].decode()
                    expected = report['results']['upload']['originals'][kind]
                    if expected != {'sha256': request['sha256'], 'bytes': len(raw)}:
                        raise ValueError('Provider PDF differs from actual uploaded original')
                    with gateway_lock:
                        provider_requests.append({'kind': kind, 'sha256': request['sha256'], 'bytes': len(raw)})
                        with (work / 'provider-request-trace.json').open('w') as trace_file:
                            json.dump(provider_requests, trace_file)
                            trace_file.flush()
                            os.fsync(trace_file.fileno())
                    if kind == 'held':
                        held_received.set()
                        if not held_release.wait(10):
                            raise ValueError('Explicit held provider release deadline')
                    buyer = report['results']['setup']['buyer']
                    amount = 12 if kind == 'conflict' else 10
                    values = {'invoiceNumber': 'SYN70-' + kind, 'supplierName': 'Synthetic Extraction Supplier', 'supplierCountry': 'DK', 'supplierLegalId': '12345678', 'supplierLegalIdKind': 'dk_cvr', 'buyerName': buyer['name'], 'buyerCountry': buyer['country'], 'buyerLegalId': buyer['cvr'], 'buyerLegalIdKind': 'dk_cvr', 'invoiceDate': report['results']['setup']['today'], 'currency': 'DKK', 'netAmount': amount, 'vatAmount': 0, 'grossAmount': amount, 'reverseChargeWording': False}
                    fields = [{'key': key, 'value': value, 'confidence': .99, 'page': 1, 'sourceText': key + ': ' + ('false' if value is False else str(value)), 'box': {'x': 72, 'y': 100, 'width': 250, 'height': 14}} for key, value in values.items()]
                    data = b'not valid JSON' if kind == 'malformed' else json.dumps({'fields': fields}).encode()
                    self.send_response(503 if kind == 'unavailable' else 200)
                    self.send_header('content-type', 'application/json')
                    self.send_header('content-length', str(len(data)))
                    self.end_headers()
                    self.wfile.write(data)
                except (BrokenPipeError, ConnectionResetError, ssl.SSLEOFError):
                    pass
                except Exception:
                    errors.append('private-extraction-provider-contract-refusal')
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
                argv += ['--restore-backup', str(work / 'extraction-source.native-backup.json'), '--trusted-backup-key', str(work / 'trusted.pub'), '--confirm-target', str(root), '--source-unavailable']
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
            report['lastBrowserResponse'] = value
            save()
            if not value.get('ok'):
                report['browserFailure'] = value
                save()
                raise RuntimeError('Browser assertion failed at ' + value.get('step', action))
            with gateway_lock:
                report['providerRequestTrace'] = list(provider_requests)
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
            progress('operator:independent-signed-extraction-evidence-' + name)
            path = work / ('extraction-' + name + '.native-backup.json')
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
                if any(secret.encode() in raw for secret in [provider_token, bearer, config['authMail']['idempotencySecret'], config['cookieKey'], config['mfaKey']]):
                    raise RuntimeError('Private operator credential found in signed inventory')
            entries = [entry for entry in wire['entries'] if entry['path'] == 'synthetic/data/ledger.sqlite']
            if len(entries) != 1:
                raise RuntimeError('Exact signed company SQLite required')
            raw = base64.b64decode(entries[0]['bytesBase64'], validate=True)
            if not 0 < len(raw) <= 32 * 1024 * 1024:
                raise RuntimeError('Bounded signed SQLite copy required')
            copy = work / ('readonly-extraction-' + name + '.sqlite')
            tables = ['documents', 'invoice_extraction_documents', 'invoice_extraction_attempts', 'invoice_extraction_results', 'invoice_extraction_fields', 'invoice_extraction_resolutions', 'exceptions', 'audit_log', 'journal_entries', 'journal_lines']
            try:
                with copy.open('xb') as output:
                    output.write(raw)
                db = sqlite3.connect(copy.as_uri() + '?mode=ro', uri=True)
                try:
                    db.execute('PRAGMA query_only=ON')
                    result = {}
                    receipt_high_water = db.execute('SELECT COALESCE(MAX(id),0) FROM mutation_idempotency_tombstones').fetchone()[0]
                    for table in tables:
                        cursor = db.execute('SELECT * FROM ' + table + ' ORDER BY id')
                        rows = [dict(zip([c[0] for c in cursor.description], row)) for row in cursor.fetchall()]
                        for row in rows:
                            for key, value in list(row.items()):
                                if isinstance(value, bytes):
                                    row[key] = {'bytesBase64': base64.b64encode(value).decode(), 'sha256': hashlib.sha256(value).hexdigest()}
                        result[table] = rows
                finally:
                    db.close()
            finally:
                copy.unlink(missing_ok=True)
            if len(result['documents']) != 5 or len(result['invoice_extraction_documents']) != 5 or len(result['invoice_extraction_attempts']) != 5 or len(result['invoice_extraction_results']) != 2 or len(result['invoice_extraction_fields']) != 30 or len(result['journal_entries']) != (0 if args.original_interfaces else 1) or len(result['journal_lines']) != (0 if args.original_interfaces else 2):
                raise RuntimeError('Exact five sources/attempts, two cited results, thirty fields and mode-specific journal count required')
            docs = {row['id']: row for row in result['documents']}
            attempts = {row['id']: row for row in result['invoice_extraction_attempts']}
            public = report['results']['book']['canonical']['extractions']
            for kind, current in public.items():
                surface = current['extraction']
                attempt = attempts.get(surface['attemptId'])
                if not attempt or attempt['status'] != 'started' or attempt['extractor_id'] != 'http-json-v1' or attempt['extractor_version'] != '1':
                    raise RuntimeError('Exact immutable started attempt/provider provenance required')
                source = next(row for row in result['invoice_extraction_documents'] if row['id'] == attempt['extraction_document_id'])
                if source['document_id'] != surface['documentId'] or source['sha256_hash'] != surface['originalHash'] or source['pdf_bytes']['sha256'] != surface['originalHash']:
                    raise RuntimeError('Exact extraction BLOB/source SHA link differs')
                blob = base64.b64decode(source['pdf_bytes']['bytesBase64'], validate=True)
                if source['byte_length'] != len(blob):
                    raise RuntimeError('Exact retained PDF BLOB length differs')
                doc = docs[surface['documentId']]
                originals = [entry for entry in wire['entries'] if entry['path'] == 'synthetic/' + doc['stored_path']]
                if len(originals) != 1 or base64.b64decode(originals[0]['bytesBase64'], validate=True) != blob:
                    raise RuntimeError('Signed physical original and retained extraction BLOB must be byte equal')
                results = [row for row in result['invoice_extraction_results'] if row['attempt_id'] == attempt['id']]
                resolutions = [row for row in result['invoice_extraction_resolutions'] if row['attempt_id'] == attempt['id']]
                if kind == 'held':
                    if attempt['initiated_by'] != ('service_account:' + report['results']['original-prepare']['serviceAccountId'] if args.original_interfaces else 'user:' + report['results']['authenticate']['memberId']) or results or resolutions or surface['status'] != 'started':
                        raise RuntimeError('Revoked current authority must retain only its historical started attempt')
                elif kind in ('good', 'conflict'):
                    if len(results) != 1:
                        raise RuntimeError('Cited native result missing')
                    payload = json.loads(results[0]['result_json'])
                    if payload.get('version') != 'rentemester-native-invoice-extraction/1' or payload.get('extractor') != {'id': 'http-json-v1', 'version': '1'} or payload.get('sourceSha256') != surface['originalHash'] or not all(re.fullmatch('[0-9a-f]{64}', payload.get(field, '')) for field in ('contextHash', 'fieldsHash')):
                        raise RuntimeError('Native context-bound signed result codec/hash identity differs')
                    fields = [row for row in result['invoice_extraction_fields'] if row['result_id'] == results[0]['id']]
                    if len(fields) != 15 or any(row['page_number'] != 1 or row['confidence'] != .99 for row in fields):
                        raise RuntimeError('All fifteen original cited fields must survive')
                elif results:
                    raise RuntimeError('Provider malformed/unavailable failure must not fabricate a result')
            if len([row for row in result['audit_log'] if row['event_type'] == 'document_ingest']) != 5:
                raise RuntimeError('Exact five immutable document intake audits required')
            report.setdefault('snapshots', {})[name] = {'sha256': receipt['sha256'], 'trustedPublicKeySha256': digest(trusted), 'operatorCredentialsExcluded': True, 'manifestSha256': hashlib.sha256(base64.b64decode(wire['manifestBase64'], validate=True)).hexdigest(), 'receiptHighWater': receipt_high_water, 'tables': result}
            save()
            return result

        def preserved(original, newer):
            for table in ['documents', 'invoice_extraction_documents', 'invoice_extraction_attempts', 'invoice_extraction_results', 'invoice_extraction_fields', 'invoice_extraction_resolutions', 'exceptions', 'journal_entries', 'journal_lines']:
                if original[table] != newer[table]:
                    raise RuntimeError('Restored exact immutable evidence differs: ' + table)
            source_snapshot = report['snapshots']['source']
            verify_restored_resolution_inventory(original, newer, source_snapshot['manifestSha256'], source_snapshot['receiptHighWater'])
            report.setdefault('restoreAuditBoundaries', []).append({'originalRows': len(original['audit_log']), 'restoredRows': len(newer['audit_log']), 'additionalRows': newer['audit_log'][len(original['audit_log']):]})
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
            source_paths = [REPO / name for name in subprocess.check_output(['git', 'ls-files', 'src', 'crates', 'rust', 'app/src', 'scripts/rust'], cwd=REPO, text=True).splitlines()]
            source_paths = [p for p in source_paths if p.is_file()]
            report['sourceFiles'] = {str(p.relative_to(REPO)): digest(p) for p in source_paths}
            report['sourceTree'] = subprocess.check_output(['git', 'status', '--porcelain'], cwd=REPO, text=True)
            report['originalHelperSha256'] = digest(REPO / 'scripts/rust/check-native-invoice-extraction-original.ts')
            report['artifacts'] = {name: digest(path) for name, path in pinned.items()}
            report['webFiles'] = {str(p.relative_to(web)): digest(p) for p in sorted(web.rglob('*')) if p.is_file()}
            progress('operator:private-auth-gateway-trust-and-backup-signer')
            ca, cert, private = work / 'ca.pem', work / 'tls.pem', work / 'tls.key'
            (work / 'tls.ext').write_text('basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\nsubjectAltName=DNS:localhost,IP:127.0.0.1\n')
            commands = [
                ['openssl', 'req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', str(work / 'ca.key'), '-out', str(ca), '-days', '1', '-subj', '/CN=Synthetic Invoice Extraction Auth CA', '-addext', 'basicConstraints=critical,CA:TRUE', '-addext', 'keyUsage=critical,keyCertSign,cRLSign'],
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
            config = {'runtimeProfile': 'local-user-macos', 'root': str(work / 'workspace'), 'brokerUid': os.getuid(), 'workspace': 'synthetic-native-invoice-extraction', 'companySlug': 'synthetic', 'companyName': 'Synthetic Invoice Extraction Company', 'cookieKey': key(), 'mfaKey': key(), 'operatorActors': ['system:synthetic-installer'], 'accounts': [], 'firstOwner': {'actor': 'system:synthetic-installer', 'email': 'owner@example.invalid', 'name': 'Synthetic Owner', 'password': 'synthetic-owner-password-123'}, 'backupSigningKeyFile': str(work / 'signing.der'), 'authMail': {'endpoint': f'https://127.0.0.1:{mail_port}/mail', 'bearerToken': bearer, 'from': 'auth@example.invalid', 'idempotencySecret': secrets.token_urlsafe(32), 'trustedCaFile': str(ca)}}
            write_config()
            start(create=True)
            helper_source = REPO / 'scripts/rust/check-native-invoice-extraction-original.ts'
            helper = work / helper_source.name
            shutil.copyfile(helper_source, helper)
            report['originalHelperSha256'] = digest(helper)
            playwright_candidates = [REPO / 'node_modules/@playwright/test/index.mjs']
            playwright_candidates += [parent / 'node_modules/@playwright/test/index.mjs' for parent in [args.web_dir.resolve(strict=True), *args.web_dir.resolve(strict=True).parents]]
            playwright = next((p.resolve(strict=True) for p in playwright_candidates if p.is_file()), None)
            if playwright is None:
                raise RuntimeError('Existing Playwright missing in worktree and supplied web checkout ancestors')
            report['playwrightModule'] = str(playwright)
            save()
            script = work / 'browser.mjs'
            script.write_text(BROWSER)
            controller = subprocess.Popen(['/opt/homebrew/bin/bun', str(script)], cwd=REPO, env={**os.environ, 'RM_EXTRACTION_BASE': origin, 'RM_EXTRACTION_WORK': str(work), 'RM_EXTRACTION_SCREENSHOT': str(args.evidence.with_suffix('.png')), 'RM_EXTRACTION_PLAYWRIGHT': str(playwright), 'RM_EXTRACTION_ORIGINAL': '1' if args.original_interfaces else '0', 'RM_EXTRACTION_ORIGINAL_HELPER': str(helper)}, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, bufsize=1, start_new_session=True)
            report['browserControllerPid'] = controller.pid
            for action in ['authenticate', 'setup', 'upload', 'disabled']:
                phase(action)
            if provider_count() != 0:
                raise RuntimeError('Default-off/upload/read actions contacted provider')
            stop()
            config.pop('firstOwner')
            config['invoiceExtractor'] = {'provider': 'http-json-v1', 'url': f'https://127.0.0.1:{mail_port}/extract', 'bearerToken': provider_token, 'timeoutMs': 15000, 'trustedCaFile': str(ca)}
            write_config()
            start()
            phase('provider-ready')
            before = provider_count()
            phase('unauthorized')
            if provider_count() != before:
                raise RuntimeError('Unauthorized CompanyMasterData preflight contacted provider')
            if args.original_interfaces:
                phase('original-prepare')
            phase('completed')
            if provider_count() != (2 if args.original_interfaces else 1):
                raise RuntimeError('Completed/cache requests must execute provider exactly once')
            phase('negatives')
            if args.original_interfaces:
                phase('clients')
            phase('hold-begin')
            if not held_received.wait(5):
                raise RuntimeError('Authorized extraction did not reach owned held provider')
            try:
                phase('revoke', 3)
            finally:
                held_release.set()
            phase('hold-finish')
            report['currentRevocationDuringExtractionQualified'] = True
            phase('book')
            if not args.original_interfaces:
                phase('clients')
            with gateway_lock:
                if sorted(row['kind'] for row in provider_requests) != ['conflict', 'good', 'held', 'malformed', 'unavailable']:
                    raise RuntimeError('Exactly one request per explicit source, no client/cache/provider retry')
            phase('archive-source')
            original = inspect_snapshot('source', work / 'trusted.pub')
            requests_after_source = provider_count()
            stop()
            write_config()
            start()
            phase('reopen')
            phase('client-reads')
            phase('archive-restarted')
            if inspect_snapshot('restarted', work / 'trusted.pub') != original or provider_count() != requests_after_source:
                raise RuntimeError('Restart/read changed extraction evidence/audit or re-extracted original')
            stop()
            source = work / 'workspace'
            if not owned() or source.is_symlink() or not source.is_dir():
                raise RuntimeError('Exact owned source required before source-free removal')
            shutil.rmtree(source)
            (work / 'signing.der').unlink()
            progress('operator:source-free-independent-verification')
            verified = subprocess.run([str(pinned['server_binary']), '--verify-backup', str(work / 'extraction-source.native-backup.json'), '--trusted-backup-key', str(work / 'trusted.pub')], cwd=REPO, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=60)
            if verified.returncode != 0:
                raise RuntimeError('Source-free independent trusted-key verification refused')
            subprocess.run(['/opt/homebrew/bin/bun', '-e', "const c=require('node:crypto'),f=require('node:fs'),k=c.generateKeyPairSync('ed25519'),s=k.privateKey.export({format:'der',type:'pkcs8'}).subarray(-32),p=k.publicKey.export({format:'der',type:'spki'}).subarray(-32);f.writeFileSync(process.argv[1],Buffer.concat([Buffer.from('3051020101300506032b657004220420','hex'),s,Buffer.from('812100','hex'),p]),{mode:0o600});f.writeFileSync(process.argv[2],p,{mode:0o600});", str(work / 'target-signing.der'), str(work / 'target-trusted.pub')], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            config.pop('invoiceExtractor')
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
            phase('client-reads')
            phase('archive-reopened')
            if inspect_snapshot('reopened', work / 'target-trusted.pub') != report['snapshots']['restored']['tables']:
                raise RuntimeError('Restored reopen changed retained source/evidence/audit/books')
            if provider_count() != requests_after_source:
                raise RuntimeError('Restored historical evidence attempted provider I/O')
            phase('finish', 20)
            controller.stdin.close()
            controller.wait(timeout=10)
            if controller.returncode != 0:
                raise RuntimeError('Controller nonzero terminal')
            controller = None
            with gateway_lock:
                if errors:
                    raise RuntimeError('Private gateway protocol error')
                report['provider'] = {'verifiedTls': True, 'requests': list(provider_requests), 'sourceFreeRestoreNoProviderCalls': True, 'configuredOnlyThroughPrivateOperatorJson': True}
                report['gateway'] = {'https': True, 'trustedCaSha256': digest(ca), 'acceptedMessages': len(gateway_rows), 'requests': list(gateway_requests)}
            report['artifactsAfter'] = {name: digest(path) for name, path in pinned.items()}
            report['webFilesAfter'] = {str(p.relative_to(web)): digest(p) for p in sorted(web.rglob('*')) if p.is_file()}
            report['sourceFilesAfter'] = {str(p.relative_to(REPO)): digest(p) for p in source_paths}
            report['runnerSha256After'] = digest(Path(__file__))
            report['originalHelperSha256After'] = digest(helper)
            save()
            if report['sourceFilesAfter'] != report['sourceFiles'] or report['artifactsAfter'] != report['artifacts'] or report['webFilesAfter'] != report['webFiles'] or report['runnerSha256After'] != report['runnerSha256'] or report['originalHelperSha256After'] != report['originalHelperSha256'] or digest(helper_source) != report['originalHelperSha256']:
                raise RuntimeError('Pinned source/helper/native/web changed during proof')
            report.update({'status': 'passed', 'currentAction': 'complete', 'invoiceExtractionActualQualified': True, 'existingSourceNamedCliMcpQualified': True, 'initialAndRestartQualified': True, 'snapshotCoverage': 'signed-source-free-five-extraction-tables-originals-audit-restored-and-reopened'})
            save()
            print('PASS: ordinary MFA explicit invoice extraction/cache, ' + ('original cookie/service interfaces without financial replay' if args.original_interfaces else 'native interfaces and manual expense') + ', current revocation and signed source-free recovery; ' + str(args.evidence))
        except BaseException as error:
            with gateway_lock:
                report['providerRequestTrace'] = list(provider_requests)
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
            if owned() and report['status'] == 'passed':
                shutil.rmtree(work)
                report['ownedRuntimeCleaned'] = True
            else:
                report['ownedRuntimeCleaned'] = False
            save()


if __name__ == '__main__':
    main()
