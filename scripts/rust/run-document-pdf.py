#!/usr/bin/env python3
"""Owned ordinary-MFA explicit native PDF inspection, booking and signed recovery.
Uses supplied pinned native worker/server/clients/web, no system reader or OCR.
No SQL/auth/business seeds or inferred accounting values. Physical original tamper
is probed only while the owned source is stopped, then exact bytes are repaired.
Zero supervised child executions and SQL-evidence tamper remain unit-qualified;
actual proof retains exact signed evidence and cached attempts without fake counts.
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
PREFIX = 'Rentemester-Synthetic-Document-Pdf-'
BROWSER = r'''







import {createHmac,createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {createInterface} from 'node:readline';
import {readFileSync,readdirSync,writeFileSync,unlinkSync} from 'node:fs';
import {join} from 'node:path';
import {isDeepStrictEqual} from 'node:util';
const {chromium}=await import(process.env.RM_PDF_PLAYWRIGHT);
const base=process.env.RM_PDF_BASE,work=process.env.RM_PDF_WORK;
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
let originalProof;
async function originalPhase(action){
 step='original:import-or-action';
 if(!originalProof){const module=await import(process.env.RM_DOCUMENTS_ORIGINAL_HELPER);originalProof=module.createDocumentsOriginalProof(base,work,pdf,name=>{step='original:'+name;detail=null;});}
 if(action==='original-run')return originalProof.run(ownerPage);
 return originalProof.assertStable(ownerPage);
}
const fixtures={text:pdf('BT\n/F1 18 Tf\n72 720 Td\n(Synthetic parser evidence) Tj\n0 -28 Td\n(Invoice SYN-001) Tj\nET\n'),blank:pdf(''),malformed:Buffer.from('%PDF-1.4\nSynthetic invalid PDF with no catalog or xref\n')};
const documents={},parses={};let today,stable,sourceBooks,bank,freshOwner;
const read=p=>get(ownerPage,p);
async function call(path,data,page=ownerPage){const r=await fetchJson(page,path,data);return{status:r.status,body:r.body};}
async function post(path,data){const r=await call(path,data);need(r.status===200&&r.body?.ok===true,'confirmed public operation '+path+' HTTP'+r.status);return r.body;}
async function books(){return{journal:await read(prefix+'journal'),trialBalance:await read(prefix+'trial-balance'),income:await read(prefix+'income-statement'),balance:await read(prefix+'balance')};}
function path(kind,suffix){return prefix+`documents/${documents[kind].id}/${suffix}`;}
async function originals(){for(const[kind,doc]of Object.entries(documents)){const r=await ownerPage.request.get(base+prefix+`documents/${doc.id}/file`);need(r.status()===200&&r.headers()['content-type'].split(';')[0]==='application/pdf'&&hash(await r.body())===hash(fixtures[kind]),'exact retained source PDF '+kind);}}
async function setup(){step='explicit-chart-and-ordinary-server-date';today=(await read(prefix+'scheduled-recurring')).scheduledRecurring.asOfDate;need(/^\d{4}-\d{2}-\d{2}$/.test(today),'trusted Copenhagen fixture date');for(const[accountNo,type,normalBalance]of[['1866','asset','debit'],['4866','expense','debit']])await post(prefix+'accounts',{accountNo,name:'Synthetic PDF '+accountNo,type,normalBalance});await post(prefix+'account-roles',{role:'bank',accountNo:'1866',confirm:true});await post(prefix+'bank-accounts',{slug:'synthetic-pdf-bank',name:'Synthetic PDF bank',currency:'DKK',ledgerAccountNo:'1866'});sourceBooks=await books();need(sourceBooks.journal.journal.entries.length===0,'fresh genuine empty journal');return{today,sourceBooks};}
async function upload(){for(const[kind,bytes]of Object.entries(fixtures)){step='web-upload-'+kind+'-route';await ownerPage.goto(base+'/companies/synthetic/bilag');await ownerPage.getByRole('heading',{name:'Bilag',exact:true}).waitFor();await ownerPage.getByRole('button',{name:'Indlæs bilag',exact:true}).click();const dialog=ownerPage.getByRole('dialog',{name:'Indlæs bilag',exact:true});step='web-upload-'+kind+'-explicit-fields';await dialog.getByLabel('Bilagsfil',{exact:true}).setInputFiles({name:'synthetic-pdf-'+kind+'.pdf',mimeType:'application/pdf',buffer:bytes});await dialog.getByLabel('Bilagstype',{exact:true}).selectOption('cash_register_receipt');await dialog.getByLabel('Kilde',{exact:true}).fill('Synthetic explicit PDF '+kind);if(kind==='text'){await dialog.getByLabel('Bilagsdato (valgfri)',{exact:true}).fill(today);await dialog.getByLabel('Beløb inkl. moms (valgfri)',{exact:true}).fill('10,00');await dialog.getByLabel('Momsbeløb (valgfri)',{exact:true}).fill('0,00');}step='web-upload-'+kind+'-single-submit';const wait=ownerPage.waitForResponse(r=>new URL(r.url()).pathname===prefix+'documents/ingest'&&r.request().method()==='POST');await dialog.getByRole('button',{name:'Indlæs bilag',exact:true}).click();const response=await wait,value=await response.json();need(response.status()===200&&value.ok===true&&Number.isSafeInteger(value.document.id),'real registered '+kind+' PDF');documents[kind]=value.document;await dialog.getByText('Bilaget er indlæst som '+value.document.documentNo+'.',{exact:true}).waitFor();await dialog.getByRole('button',{name:'Luk',exact:true}).click();need((await read(path(kind,'parse-status'))).parse===null,'upload does not parse implicitly');}await originals();need(isDeepStrictEqual(await books(),sourceBooks),'PDF intake does not book or infer accounting amounts');return{documents,originals:Object.fromEntries(Object.entries(fixtures).map(([k,b])=>[k,{sha256:hash(b),bytes:b.length}]))};}
async function verified(kind){const status=await read(path(kind,'parse-status')),text=await read(path(kind,'parsed-text')+'?offset=0&limit=10');need(status.ok===true&&status.parse?.documentId===documents[kind].id&&status.parse.sourceSha256===hash(fixtures[kind])&&status.parse.parserId==='pdfium'&&status.parse.parserVersion==='151.0.7881.0+binding0.9.4.adapter1.1bc45b15466b34ce'&&/^[0-9a-f]{64}$/.test(status.parse.resultHash),'actual pinned native parser/source/evidence identity '+kind);need(isDeepStrictEqual(text.parse,status.parse)&&text.offset===0&&text.limit===10&&text.nextOffset===null&&text.pages.length===status.parse.pageCount,'exact bounded public parsed pages');if(kind==='text'){need(status.parse.status==='ok'&&status.parse.errorCode===null&&status.parse.pageCount===1&&status.parse.itemCount>0&&status.parse.textLength>0,'real text parse succeeds');const page=text.pages[0];need(page.pageNumber===1&&page.width===612&&page.height===792&&page.rotation===0&&page.text.includes('Synthetic parser evidence')&&page.text.includes('Invoice SYN-001')&&page.itemCount===status.parse.itemCount&&page.text.length===status.parse.textLength&&/^[0-9a-f]{64}$/.test(page.layoutHash),'real benchmark text, geometry, counts and layout hash');}else need(status.parse.status===(kind==='blank'?'no_text_layer':'malformed_pdf')&&status.parse.errorCode===status.parse.status,'declared retained cacheable non-success status '+kind);return{status,text};}
async function parse(){step='unauthorized-reviewer-parse-refuses-before-worker';const denied=await call(path('blank','parse'),{confirm:true},memberPage);need(denied.status===403&&denied.body?.ok===false,'current reviewer cannot request parse');need((await read(path('blank','parse-status'))).parse===null,'denied parse publishes no evidence');step='explicit-text-parse';const first=await post(path('text','parse'),{confirm:true});need(first.parse.cached===false&&first.parse.status==='ok','first parse is actual fresh native execution');parses.text=await verified('text');need(first.parse.resultHash===parses.text.status.parse.resultHash,'write receipt and read evidence same exact hash');step='explicit-text-cache-read';const cached=await post(path('text','parse'),{confirm:true});need(cached.parse.cached===true&&cached.parse.resultHash===first.parse.resultHash,'cache retains exact verified result');step='explicit-bounded-pending-two-source-pdfs';const pending=await post(prefix+'documents/parse-pending',{confirm:true,cursor:0,limit:100});need(pending.batch.requested===2&&pending.batch.parsed===2&&pending.batch.failed===0&&pending.batch.resume===null,'pending processes only two unparsed PDFs, including cacheable declared failures');for(const kind of['blank','malformed'])parses[kind]=await verified(kind);const empty=await post(prefix+'documents/parse-pending',{confirm:true,cursor:0,limit:100});need(empty.batch.requested===0&&empty.batch.parsed===0&&empty.batch.failed===0,'fully parsed/cacheable sources not pending again');step='read-pagination-and-invalid-upper-bound';const last=await read(path('text','parsed-text')+'?offset=1&limit=10');need(last.pages.length===0&&last.nextOffset===null&&isDeepStrictEqual(last.parse,parses.text.status.parse),'bounded final page selection');const invalid=await call(path('text','parsed-text')+'?offset=0&limit=11');need(invalid.status===400&&invalid.body?.ok===false,'eleven-page request refused');need(isDeepStrictEqual(await books(),sourceBooks),'all explicit parse/cache/read/refusal actions leave books unchanged');return{denied,first,cached,pending,empty,parses,actualZeroWorkerExecutionsQualified:false,zeroWorkerSupervisorUnitCoverageRequired:true};}
async function book(){step='real-bank-source-before-manual-expense';const csv=`dato;tekst;beløb;valuta;saldo\n${today};Synthetic PDF explicit purchase;-10,00;DKK;-10,00\n`;const imported=await post(prefix+'bank/import',{confirm:true,csvContent:csv,account:'synthetic-pdf-bank'});need(imported.import.imported===1,'real retained bank original imported');bank=(await read(prefix+'bank')).bank.transactions.find(r=>r.text==='Synthetic PDF explicit purchase');need(bank&&bank.amount===-10,'exact independent outgoing bank source');need(isDeepStrictEqual(await books(),sourceBooks),'bank intake still does not post');step='manual-expense-opens-exact-document';await ownerPage.goto(base+'/companies/synthetic/bilag');await ownerPage.getByRole('row').filter({hasText:documents.text.documentNo}).getByRole('link',{name:'Bogfør bilag',exact:true}).click();await ownerPage.getByRole('heading',{name:'Bogfør bilag',exact:true,level:1}).waitFor();step='manual-expense-explicit-account-bank-and-vat';await ownerPage.getByLabel(/^Udgiftskonto/).selectOption('4866');await ownerPage.getByLabel(/^Banktransaktion \(uafstemt, udgående\)/).selectOption(String(bank.id));await ownerPage.getByLabel(/^Moms-behandling/).selectOption('exempt');step='manual-expense-single-explicit-post';const wait=ownerPage.waitForResponse(r=>new URL(r.url()).pathname===prefix+'documents/book-expense'&&r.request().method()==='POST');await ownerPage.getByRole('button',{name:'Bogfør',exact:true}).click();const response=await wait;need(response.status()===200,'reviewed explicit expense accepted');await ownerPage.getByText(/Bilaget blev bogført som journalpost/).waitFor();const after=await books(),entries=after.journal.journal.entries;need(entries.length===1&&entries[0].documentId===documents.text.id&&entries[0].total===10&&entries[0].lines.length===2&&after.income.incomeStatement.totalExpense===10&&after.income.incomeStatement.result===-10,'exact one balanced expense from caller chosen10, no inferred PDF amount');for(const kind of Object.keys(documents))need(isDeepStrictEqual(await verified(kind),parses[kind]),'manual booking preserves parser evidence '+kind);const payment=(await read(prefix+'bank')).bank;need(payment.matchedCount===1&&payment.unmatchedCount===0&&payment.bookedBalance===-10&&payment.actualBalance===-10,'canonical bank relation and independent closing reconcile');stable=await snapshot();await ownerPage.screenshot({path:process.env.RM_PDF_SCREENSHOT,fullPage:true});return{receipt:await response.json(),stable,manualAmountNotExtracted:true};}
async function snapshot(){const evidence={};for(const kind of Object.keys(documents))evidence[kind]=await verified(kind);return{documents:await read(prefix+'documents'),books:await books(),bank:await read(prefix+'bank'),evidence};}
async function assertStable(){if(process.env.RM_DOCUMENTS_ORIGINAL_HELPER)return originalPhase('original-stable');need(isDeepStrictEqual(await snapshot(),stable),'exact source originals, parser evidence, one expense and ledger UUID survive restart/restore');await originals();}
async function client(kind,op,id){const cookie=join(work,'pdf-client.cookie');writeFileSync(cookie,(await ownerPage.context().cookies(base)).map(c=>`${c.name}=${c.value}`).join('; '),{mode:0o600,flag:'wx'});try{const common=['--server',base,'--cookie-file',cookie,'--allow-loopback-http'];const table={'status':['document-parse-status','document_parse_status'],'text':['document-parsed-text','document_parsed_text'],'parse':['parse-document','parse_document'],'pending':['parse-pending-documents','parse_pending_documents']};let argv,input;if(kind==='cli'){argv=['remote',table[op][0],...common,'--company','synthetic',...(op==='pending'?['--cursor','0','--limit','100']:['--id',String(id)]),...(op==='text'?['--cursor','0','--limit','10']:[]),...(['parse','pending'].includes(op)?['--confirm']:[])];}else{argv=common;input=[{jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'synthetic-pdf-proof',version:'1'}}},{jsonrpc:'2.0',method:'notifications/initialized'},{jsonrpc:'2.0',id:2,method:'tools/call',params:{name:table[op][1],arguments:{company:'synthetic',...(op==='pending'?{cursor:0,limit:100}:{id}),...(op==='text'?{offset:0,limit:10}:{}),...(['parse','pending'].includes(op)?{confirm:true}:{})}}}].map(r=>JSON.stringify(r)).join('\n')+'\n';}const r=spawnSync(join(work,kind==='cli'?'rentemester-rust':'rentemester-mcp'),argv,{input,encoding:'utf8',timeout:45000,maxBuffer:16*1024*1024});need(!r.error&&r.status===0,'native PDF client '+kind+' '+op);const value=kind==='cli'?JSON.parse(r.stdout):r.stdout.trim().split('\n').map(s=>JSON.parse(s)).find(v=>v.id===2)?.result?.structuredContent;need(value?.ok===true&&value.data?.ok===true,'canonical PDF client envelope');return value.data;}finally{unlinkSync(cookie);}}
async function clients(mutate=false){const proof={};for(const kind of['cli','mcp']){proof[kind]={};for(const item of Object.keys(documents)){need(isDeepStrictEqual(await client(kind,'status',documents[item].id),parses[item].status),'full HTTP/client status parity');need(isDeepStrictEqual(await client(kind,'text',documents[item].id),parses[item].text),'full HTTP/client text parity');}if(mutate){const expected=await post(path('text','parse'),{confirm:true}),actual=await client(kind,'parse',documents.text.id);need(actual.parse.cached===true&&isDeepStrictEqual(actual,expected),'explicit native cached parse canonical parity');const empty=await post(prefix+'documents/parse-pending',{confirm:true,cursor:0,limit:100});need(isDeepStrictEqual(await client(kind,'pending'),empty),'explicit fully resolved native pending parity');proof[kind].cached=actual;}}await assertStable();return{proof,httpCliMcpParity:true};}
async function tamper(){step='stopped-owned-physical-source-tamper-clear-refusal';const results=[];for(const suffix of['parse-status','parsed-text?offset=0&limit=10']){const r=await call(path('text',suffix));need(r.status===409&&r.body?.ok===false&&r.body.code==='PDF_EVIDENCE_TAMPERED'&&!r.body.parse&&!r.body.pages,'tampered source read refuses verified content with exact integrity code');results.push(r);}const failed=await call(path('text','parse'),{confirm:true});need(failed.status===200&&failed.body?.ok===false&&isDeepStrictEqual(failed.body.errors,['PDF_PARSE_FAILED'])&&failed.body.errorCode==='source_unavailable'&&!failed.body.parse&&!failed.body.pages,'confirmed corrupt-source parse commits only exact source-unavailable failed attempt, never a result');results.push(failed);return{physicalTamperRefused:true,committedSourceFailurePreserved:true,results,sqlEvidenceTamperActualQualified:false};}
async function reopen(){step='normal-owner-fresh-mfa-reopen-no-parse-post';await ownerPage.context().clearCookies();await login(ownerPage,owner);if(!process.env.RM_DOCUMENTS_ORIGINAL_HELPER||originalProof)await assertStable();return{freshMfaRestart:true};}
async function archive(name){const r=await ownerPage.request.post(base+'/api/workspace/backup',{headers:{origin:base},data:{confirm:true}});need(r.status()===200&&r.headers()['content-type']==='application/vnd.rentemester.native-backup+json','PDF evidence real signed archive');const bytes=await r.body();need(bytes.length>0&&bytes.length<=96*1024*1024,'bounded full PDF inventory');writeFileSync(join(work,'pdf-'+name+'.native-backup.json'),bytes,{mode:0o600,flag:'wx'});await assertStable();return{sha256:hash(bytes),bytes:bytes.length,documents,parses};}
async function restored(){step='fresh-restored-owner-normal-verification-mfa';ownerPage=await newPage();freshOwner={email:'restored@example.invalid',password:'synthetic-restored-password-789',codes:[]};await ownerPage.goto((await mail('verification',freshOwner.email)).actionUrl);need((await enroll(ownerPage,freshOwner)).user.id!==ownerId,'historical parser actor not imported current authority');await assertStable();return{sourceFreeRestoreQualified:true,parserNotConfigured:true,noParseWriteRequested:true};}
async function restoredReopen(){await ownerPage.context().clearCookies();await login(ownerPage,freshOwner);await assertStable();return{restoredTargetReopenQualified:true};}
for await(const line of createInterface({input:process.stdin,crlfDelay:Infinity})){try{const action=JSON.parse(line);let value;if(action==='authenticate')value=await authenticate();else if(action==='original-run')value=await originalPhase(action);else if(action==='original-stable')value=await originalPhase(action);else if(action==='setup')value=await setup();else if(action==='upload')value=await upload();else if(action==='parse')value=await parse();else if(action==='book')value=await book();else if(action==='clients')value=await clients(true);else if(action==='client-reads')value=await clients(false);else if(action==='tamper')value=await tamper();else if(action==='reopen')value=await reopen();else if(['archive-source','archive-restarted','archive-restored','archive-reopened'].includes(action))value=await archive(action.slice(8));else if(action==='restored')value=await restored();else if(action==='restored-reopen')value=await restoredReopen();else if(action==='finish'){await browser?.close();emit({ok:true,value:{finished:true}});break;}else throw Error('unknown phase');emit({ok:true,value});}catch(error){let pageState=null;try{pageState=await Promise.race([diagnosticPage?.evaluate(()=>({pathname:location.pathname,headings:[...document.querySelectorAll('h1,h2,h3,h4')].map(e=>e.textContent.slice(0,100)),buttons:[...document.querySelectorAll('button')].map(e=>e.textContent.slice(0,100)),alerts:[...document.querySelectorAll('[role=alert]')].map(e=>e.textContent.slice(0,300))})),new Promise(resolve=>setTimeout(()=>resolve(null),2000))]);}catch{}emit({ok:false,step,detail:process.env.RM_DOCUMENTS_ORIGINAL_HELPER?(error?.originalAssertion??error?.code??detail):detail,errorType:error?.name,trace,pageState});await Promise.race([browser?.close().catch(()=>{}),new Promise(resolve=>setTimeout(resolve,2000))]);process.exitCode=1;break;}}

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
    parser.add_argument('--pdf-worker-bundle', required=True, type=Path)
    parser.add_argument('--original-documents', action='store_true', help='Run the original eight CLI/MCP document journey instead of the default PDF flow')
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
    scanner_token = secrets.token_urlsafe(32)
    scanner_requests = []
    backend_port, mail_port = free_port(), free_port()
    if backend_port == mail_port:
        raise RuntimeError('Distinct owned ports required')
    origin = f'http://127.0.0.1:{backend_port}'
    report = {'status': 'running', 'sourceRevision': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=REPO, text=True).strip(), 'runnerSha256': digest(Path(__file__)), 'browserHelperSha256': hashlib.sha256(BROWSER.encode()).hexdigest(), 'canonicalOrigin': origin, 'noOfflineIdentitySeeds': True, 'ordinaryMacOsUser': True, 'ownedProfile': str(work), 'externalMailDelivered': False, 'workflow': 'explicit_native_pdf_inspection_and_manual_expense', 'pdfInspectionActualQualified': False, 'actualZeroWorkerExecutionsQualified': False, 'sqlEvidenceTamperActualQualified': False, 'installedNativeQualified': False, 'screenshot': str(args.evidence.with_suffix('.png')),  'actions': [], 'results': {}, 'sourceFreeRestoreQualified': False}
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
                    if args.original_documents and self.path == '/scan':
                        if not 0 < length <= 35 * 1024 * 1024 or self.headers.get('authorization') != 'Bearer ' + scanner_token:
                            raise ValueError('Private scanner authority/size')
                        request = json.loads(self.rfile.read(length))
                        if set(request) != {'sha256', 'mimeType', 'filename', 'bytesBase64'} or not re.fullmatch(r'synthetic-original165-(enrich|context|blank|malformed|force)\.pdf', request['filename']):
                            raise ValueError('Private original scanner envelope')
                        raw = base64.b64decode(request['bytesBase64'], validate=True)
                        if not raw.startswith(b'%PDF-1.4') or request['mimeType'] != 'application/pdf' or request['sha256'] != hashlib.sha256(raw).hexdigest():
                            raise ValueError('Private scanner exact immutable bytes')
                        with gateway_lock:
                            scanner_requests.append({'filename': request['filename'], 'sha256': request['sha256']})
                        payload = json.dumps({'ok': True, 'scannerId': 'Synthetic Scanner', 'scannerVersion': '165.1', 'evidenceRef': 'synthetic:' + request['sha256']}).encode()
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
                argv += ['--restore-backup', str(work / 'pdf-source.native-backup.json'), '--trusted-backup-key', str(work / 'trusted.pub'), '--confirm-target', str(root), '--source-unavailable']
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
            with gateway_lock:
                report['privateGatewayErrors'] = list(errors)
            report['actions'].append({'action': action, 'status': 'complete'})
            report['results'][action] = value['value']
            save()
            return value['value']

        def inspect_snapshot(name, trusted):
            progress('operator:verify-signed-pdf-evidence-' + name)
            path = work / ('pdf-' + name + '.native-backup.json')
            receipt = report['results']['archive-' + name]
            if digest(path) != receipt['sha256']:
                raise RuntimeError('Signed PDF snapshot bytes changed')
            verified = subprocess.run([str(pinned['server_binary']), '--verify-backup', str(path), '--trusted-backup-key', str(trusted)], cwd=REPO, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=60)
            if verified.returncode != 0:
                raise RuntimeError('Independent PDF inventory signature verification refused')
            wire = json.loads(path.read_bytes())
            secrets_excluded = [bearer, config['authMail']['idempotencySecret'], config['cookieKey'], config['mfaKey'], scanner_token]
            for raw in [base64.b64decode(e['bytesBase64'], validate=True) for e in wire['entries']] + [base64.b64decode(wire['manifestBase64'], validate=True)]:
                if any(secret.encode() in raw for secret in secrets_excluded):
                    raise RuntimeError('Operator secret imported in signed business payload')
            entries = [e for e in wire['entries'] if e['path'] == 'synthetic/data/ledger.sqlite']
            if len(entries) != 1:
                raise RuntimeError('Exact owned signed ledger required')
            raw = base64.b64decode(entries[0]['bytesBase64'], validate=True)
            if not 0 < len(raw) <= 32 * 1024 * 1024:
                raise RuntimeError('Bounded signed ledger required')
            copy = work / ('readonly-pdf-' + name + '.sqlite')
            try:
                with copy.open('xb') as output:
                    output.write(raw)
                db = sqlite3.connect(copy.as_uri() + '?mode=ro', uri=True)
                try:
                    db.execute('PRAGMA query_only=ON')
                    result = {}
                    for table in ['documents', 'document_pdf_parse_results', 'document_pdf_parse_pages', 'document_pdf_parse_attempts', 'audit_log', 'journal_entries', 'journal_lines'] + (['document_metadata_enrichments', 'document_company_contexts', 'payables'] if args.original_documents else []):
                        cursor = db.execute('SELECT * FROM ' + table + (' ORDER BY document_id' if table in ('document_metadata_enrichments', 'document_company_contexts') else ' ORDER BY id'))
                        result[table] = [dict(zip([c[0] for c in cursor.description], row)) for row in cursor.fetchall()]
                finally:
                    db.close()
            finally:
                copy.unlink(missing_ok=True)
            if args.original_documents:
                return inspect_original_snapshot(name, receipt, wire, result, trusted)
            if len(result['documents']) != 3 or len(result['document_pdf_parse_results']) != 3 or len(result['journal_entries']) != 1 or len(result['journal_lines']) != 2:
                raise RuntimeError('Three source PDFs/results and only one explicit balanced expense required')
            docs = {r['id']: r for r in result['documents']}
            public = report['results']['parse']['parses']
            for kind, expected in public.items():
                dto = expected['status']['parse']
                rows = [r for r in result['document_pdf_parse_results'] if r['document_id'] == dto['documentId']]
                if len(rows) != 1:
                    raise RuntimeError('Unique original parser result required')
                row = rows[0]
                if row['source_sha256_hash'] != dto['sourceSha256'] or row['result_sha256_hash'] != dto['resultHash'] or hashlib.sha256(row['result_json'].encode()).hexdigest() != dto['resultHash'] or row['status'] != dto['status']:
                    raise RuntimeError('Exact signed canonical result hash/source binding differs')
                for page in expected['text']['pages']:
                    pages = [p for p in result['document_pdf_parse_pages'] if p['result_id'] == row['id'] and p['page_number'] == page['pageNumber']]
                    if len(pages) != 1:
                        raise RuntimeError('Exact retained PDF page missing')
                    stored = pages[0]
                    if stored['text'] != page['text'] or stored['width'] != page['width'] or stored['height'] != page['height'] or stored['rotation'] != page['rotation'] or stored['item_count'] != page['itemCount'] or hashlib.sha256(stored['layout_json'].encode()).hexdigest() != page['layoutHash']:
                        raise RuntimeError('Public PDF text/layout hash differs from signed page evidence')
                doc = docs[dto['documentId']]
                originals = [e for e in wire['entries'] if e['path'] == 'synthetic/' + doc['stored_path']]
                if len(originals) != 1 or hashlib.sha256(base64.b64decode(originals[0]['bytesBase64'], validate=True)).hexdigest() != dto['sourceSha256']:
                    raise RuntimeError('Exact signed PDF original missing')
            attempts = result['document_pdf_parse_attempts']
            if len([r for r in attempts if r['outcome'] == 'parsed']) != 3 or not any(r['outcome'] == 'cached' for r in attempts):
                raise RuntimeError('Real fresh/cache attempt provenance missing')
            failed = [row for row in attempts if row['outcome'] == 'failed']
            text_source = public['text']['status']['parse']
            if len(failed) != 1 or failed[0]['document_id'] != text_source['documentId'] or failed[0]['source_sha256_hash'] != text_source['sourceSha256'] or failed[0]['error_code'] != 'source_unavailable' or failed[0]['result_id'] is not None:
                raise RuntimeError('Exactly one retained source-unavailable failed tamper attempt with no result required')
            if any(report['results']['authenticate']['memberId'] in r['actor'] for r in attempts):
                raise RuntimeError('Denied reviewer created a parser attempt')
            report.setdefault('signedPdfProofs', {})[name] = {'sha256': receipt['sha256'], 'trustedKeySha256': digest(trusted), 'tables': result}
            save()
            return result

        def inspect_original_snapshot(name, receipt, wire, result, trusted):
            proof = report['results']['original-run']
            if (len(result['documents']), len(result['document_pdf_parse_results']), len(result['document_metadata_enrichments']), len(result['document_company_contexts']), len(result['payables']), len(result['journal_entries'])) != (5, 5, 1, 1, 1, 1):
                raise RuntimeError('Original documents requires five originals/parses, both nonempty histories and exactly one payable/posting')
            docs = {row['id']: row for row in result['documents']}
            for kind, expected in proof['documents'].items():
                doc = docs[expected['documentId']]
                entries = [entry for entry in wire['entries'] if entry['path'] == 'synthetic/' + doc['stored_path']]
                if len(entries) != 1 or hashlib.sha256(base64.b64decode(entries[0]['bytesBase64'], validate=True)).hexdigest() != expected['sha256'] or doc['sha256_hash'] != expected['sha256']:
                    raise RuntimeError('Original document source bytes/hash differ')
                dto = proof['evidence'][kind]['status']['parse']
                rows = [row for row in result['document_pdf_parse_results'] if row['document_id'] == doc['id']]
                if len(rows) != 1 or rows[0]['source_sha256_hash'] != doc['sha256_hash'] or rows[0]['result_sha256_hash'] != dto['resultHash'] or hashlib.sha256(rows[0]['result_json'].encode()).hexdigest() != dto['resultHash']:
                    raise RuntimeError('Original document parser source/result hash differs')
                for page in proof['evidence'][kind]['text']['pages']:
                    pages = [row for row in result['document_pdf_parse_pages'] if row['result_id'] == rows[0]['id'] and row['page_number'] == page['pageNumber']]
                    if len(pages) != 1 or pages[0]['text'] != page['text'] or hashlib.sha256(pages[0]['layout_json'].encode()).hexdigest() != page['layoutHash']:
                        raise RuntimeError('Original document exact page/layout differs')
            enrichment = result['document_metadata_enrichments'][0]
            context = result['document_company_contexts'][0]
            for column, sha in [('original_payload_json', 'original_payload_sha256'), ('enriched_metadata_json', 'enriched_metadata_sha256')]:
                if hashlib.sha256(enrichment[column].encode()).hexdigest() != enrichment[sha]:
                    raise RuntimeError('Enrichment raw/canonical hash differs')
            if enrichment['document_id'] != proof['documents']['enrich']['documentId'] or docs[enrichment['document_id']]['payload_json'] != enrichment['enriched_metadata_json']:
                raise RuntimeError('Enrichment current payload/original history linkage differs')
            if context['document_id'] != proof['documents']['context']['documentId'] or context['document_sha256'] != docs[context['document_id']]['sha256_hash'] or context['payload_sha256'] != hashlib.sha256(docs[context['document_id']]['payload_json'].encode()).hexdigest() or context['company_snapshot_sha256'] != hashlib.sha256(context['company_snapshot_json'].encode()).hexdigest():
                raise RuntimeError('Company context source/payload/snapshot linkage differs')
            canonical = lambda value: json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=False)
            bound = {'documentHash': context['document_sha256'], 'payloadHash': context['payload_sha256'], 'snapshot': json.loads(context['company_snapshot_json']), 'sourceReference': context['source_reference'], 'businessUseReason': context['business_use_reason']}
            if context['context_sha256'] != hashlib.sha256(canonical(bound).encode()).hexdigest():
                raise RuntimeError('Company context canonical binding differs')
            if not all(row['actor'] and row['program'] for table in ['document_metadata_enrichments', 'document_company_contexts'] for row in result[table]):
                raise RuntimeError('Immutable history attribution missing')
            report.setdefault('signedOriginalDocumentsProofs', {})[name] = {'sha256': receipt['sha256'], 'trustedKeySha256': digest(trusted), 'tables': result}
            save()
            return result

        def preserved(original, newer):
            for table in ['documents', 'document_pdf_parse_results', 'document_pdf_parse_pages', 'document_pdf_parse_attempts', 'journal_entries', 'journal_lines'] + (['document_metadata_enrichments', 'document_company_contexts', 'payables'] if args.original_documents else []):
                if original[table] != newer[table]:
                    raise RuntimeError('Retained PDF/original/attempt/book evidence changed: ' + table)
            retained = {row['id']: row for row in newer['audit_log']}
            if any(retained.get(row['id']) != row for row in original['audit_log']):
                raise RuntimeError('Historical PDF audit/provenance changed')

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
            bundle_source = args.pdf_worker_bundle.resolve(strict=True)
            if args.pdf_worker_bundle.is_symlink() or not bundle_source.is_dir() or any(p.is_symlink() for p in bundle_source.rglob('*')):
                raise RuntimeError('Real native worker bundle without symlinks required')
            bundle = work / 'pdf-worker'
            shutil.copytree(bundle_source, bundle)
            if not (bundle / 'rentemester-pdf-worker').is_file() or not (bundle / 'pdfium/lib/libpdfium.dylib').is_file():
                raise RuntimeError('Native worker and sibling pinned macOS PDFium required')
            report['pdfWorkerFiles'] = {str(p.relative_to(bundle)): digest(p) for p in sorted(bundle.rglob('*')) if p.is_file()}
            if args.original_documents:
                helper = REPO / 'scripts/rust/check-native-documents-original.ts'
                helper_copy = work / helper.name
                shutil.copyfile(helper, helper_copy)
                report['originalHelperSha256'] = digest(helper_copy)
                report['sourceFiles'] = {str(path.relative_to(REPO)): digest(path) for folder in ['crates', 'src', 'app/src', 'scripts/rust'] for path in sorted((REPO / folder).rglob('*')) if path.is_file() and path.suffix in ('.rs', '.ts', '.tsx', '.py', '.json', '.toml', '.lock')}
                report['workflow'] = 'original_documents_eight_cli_eight_mcp_nonempty_histories_one_payable'
            progress('operator:private-auth-gateway-trust-and-backup-signer')
            ca, cert, private = work / 'ca.pem', work / 'tls.pem', work / 'tls.key'
            (work / 'tls.ext').write_text('basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\nsubjectAltName=DNS:localhost,IP:127.0.0.1\n')
            commands = [
                ['openssl', 'req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', str(work / 'ca.key'), '-out', str(ca), '-days', '1', '-subj', '/CN=Synthetic Document PDF Auth CA', '-addext', 'basicConstraints=critical,CA:TRUE', '-addext', 'keyUsage=critical,keyCertSign,cRLSign'],
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
            config = {'runtimeProfile': 'local-user-macos', 'root': str(work / 'workspace'), 'brokerUid': os.getuid(), 'workspace': 'synthetic-native-document-pdf', 'companySlug': 'synthetic', 'companyName': 'Synthetic Document PDF Company', 'cookieKey': key(), 'mfaKey': key(), 'operatorActors': ['system:synthetic-installer'], 'accounts': [], 'firstOwner': {'actor': 'system:synthetic-installer', 'email': 'owner@example.invalid', 'name': 'Synthetic Owner', 'password': 'synthetic-owner-password-123'}, 'backupSigningKeyFile': str(work / 'signing.der'), 'authMail': {'endpoint': f'https://127.0.0.1:{mail_port}/mail', 'bearerToken': bearer, 'from': 'auth@example.invalid', 'idempotencySecret': secrets.token_urlsafe(32), 'trustedCaFile': str(ca)}}
            config['documentPdfWorker'] = {'bundleDir': str(bundle)}
            if args.original_documents:
                config['documentScanner'] = {'policy': 'required', 'provider': 'http-json-v1', 'url': f'https://127.0.0.1:{mail_port}/scan', 'bearerToken': scanner_token, 'timeoutMs': 10000, 'trustedCaFile': str(ca)}
            write_config()
            start(create=True)
            script = work / 'browser.mjs'
            script.write_text(BROWSER)
            controller = subprocess.Popen(['/opt/homebrew/bin/bun', str(script)], cwd=REPO, env={**os.environ, 'RM_PDF_BASE': origin, **({'RM_DOCUMENTS_ORIGINAL_HELPER': str(helper_copy)} if args.original_documents else {}), 'RM_PDF_WORK': str(work), 'RM_PDF_SCREENSHOT': str(args.evidence.with_suffix('.png')), 'RM_PDF_PLAYWRIGHT': str((REPO / 'node_modules/@playwright/test/index.mjs').resolve(strict=True))}, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, bufsize=1, start_new_session=True)
            report['browserControllerPid'] = controller.pid
            if args.original_documents:
                phase('authenticate')
                # Exact normal MFA restart before any document or business effects.
                stop()
                config.pop('firstOwner')
                write_config()
                start()
                phase('reopen')
                phase('original-run')
            else:
                for action in ['authenticate', 'setup', 'upload', 'parse', 'book', 'clients']:
                    phase(action)
                # Tamper only an owned physical source while the server is stopped;
                # never inject business records or alter live SQLite.
                stop()
                sha = report['results']['upload']['originals']['text']['sha256']
                original_path = work / 'workspace/synthetic/documents/originals' / (sha + '.pdf')
                if not owned() or original_path.is_symlink() or digest(original_path) != sha:
                    raise RuntimeError('Exact owned PDF required for bounded physical tamper')
                original_bytes = original_path.read_bytes()
                changed = bytearray(original_bytes)
                changed[20] ^= 1
                original_path.write_bytes(changed)
                config.pop('firstOwner')
                write_config()
                try:
                    start()
                    phase('tamper')
                finally:
                    stop()
                    if not owned() or original_path.is_symlink():
                        raise RuntimeError('Owned physical source changed identity')
                    original_path.write_bytes(original_bytes)
                if digest(original_path) != sha:
                    raise RuntimeError('Exact PDF repair failed')
                start()
                phase('reopen')
                phase('client-reads')
            phase('archive-source')
            original = inspect_snapshot('source', work / 'trusted.pub')
            stop()
            write_config()
            start()
            phase('reopen')
            phase('archive-restarted')
            if inspect_snapshot('restarted', work / 'trusted.pub') != original:
                raise RuntimeError('Readonly source restart changed evidence/attempts/audit/books')
            stop()
            source = work / 'workspace'
            if not owned() or source.is_symlink() or not source.is_dir():
                raise RuntimeError('Exact owned source required before source-free removal')
            shutil.rmtree(source)
            (work / 'signing.der').unlink()
            verified = subprocess.run([str(pinned['server_binary']), '--verify-backup', str(work / 'pdf-source.native-backup.json'), '--trusted-backup-key', str(work / 'trusted.pub')], cwd=REPO, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=60)
            if verified.returncode != 0:
                raise RuntimeError('Independent source-free signature verification refused')
            subprocess.run(['/opt/homebrew/bin/bun', '-e', "const c=require('node:crypto'),f=require('node:fs'),k=c.generateKeyPairSync('ed25519'),s=k.privateKey.export({format:'der',type:'pkcs8'}).subarray(-32),p=k.publicKey.export({format:'der',type:'spki'}).subarray(-32);f.writeFileSync(process.argv[1],Buffer.concat([Buffer.from('3051020101300506032b657004220420','hex'),s,Buffer.from('812100','hex'),p]),{mode:0o600});f.writeFileSync(process.argv[2],p,{mode:0o600});", str(work / 'target-signing.der'), str(work / 'target-trusted.pub')], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            config.pop('documentPdfWorker')
            if args.original_documents:
                config.pop('documentScanner')
                report['originalScannerRequests'] = list(scanner_requests)
                if len(scanner_requests) < 5:
                    raise RuntimeError('Original documents did not traverse required scanner')
            # No parser is available to restore/reopen: all historical evidence
            # must verify from the signed source without invoking a child.
            shutil.rmtree(bundle)
            config.update({'root': str(work / 'restored'), 'backupSigningKeyFile': str(work / 'target-signing.der'), 'cookieKey': key(), 'mfaKey': key(), 'restoreActor': 'system:synthetic-installer', 'firstOwner': {'actor': 'system:synthetic-installer', 'email': 'restored@example.invalid', 'name': 'Synthetic Restored Owner', 'password': 'synthetic-restored-password-789'}})
            write_config()
            start(restore=True)
            restored = phase('restored')
            phase('original-stable' if args.original_documents else 'client-reads')
            report['sourceFreeRestoreQualified'] = restored['sourceFreeRestoreQualified']
            report['sourceWorkspaceAndPrivateSignerRemoved'] = not source.exists() and not (work / 'signing.der').exists()
            report['workerBundleRemovedBeforeRestore'] = not bundle.exists()
            phase('archive-restored')
            restored_rows = inspect_snapshot('restored', work / 'target-trusted.pub')
            preserved(original, restored_rows)
            stop()
            config.pop('firstOwner')
            config.pop('restoreActor')
            write_config()
            start()
            phase('restored-reopen')
            phase('archive-reopened')
            if inspect_snapshot('reopened', work / 'target-trusted.pub') != restored_rows:
                raise RuntimeError('Readonly restored reopen changed exact parser/book evidence')
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
            if args.original_documents:
                if digest(helper_copy) != report['originalHelperSha256'] or digest(REPO / 'scripts/rust/check-native-documents-original.ts') != report['originalHelperSha256']:
                    raise RuntimeError('Pinned original helper/source changed')
                if any(digest(REPO / path) != sha for path, sha in report['sourceFiles'].items()):
                    raise RuntimeError('Original document source equivalence changed')
                if any(digest(pinned[name]) != sha for name, sha in report['artifacts'].items()) or {str(p.relative_to(web)): digest(p) for p in sorted(web.rglob('*')) if p.is_file()} != report['webFiles']:
                    raise RuntimeError('Pinned binary/web artifacts changed')
                if {str(p.relative_to(bundle_source)): digest(p) for p in sorted(bundle_source.rglob('*')) if p.is_file()} != report['pdfWorkerFiles']:
                    raise RuntimeError('Supplied PDF bundle changed')
                if scanner_requests != report['originalScannerRequests']:
                    raise RuntimeError('Historical restore attempted scanner IO')
                report['originalDocumentsActualQualified'] = True
                report['finalSourceEquivalenceQualified'] = True
            report.update({'status': 'passed', 'currentAction': 'complete', 'pdfInspectionActualQualified': True, 'existingPdfClientsQualified': True, 'initialAndRestartQualified': True, 'snapshotCoverage': 'signed-source-free-originals-pdf-evidence-restored-and-reopened'})
            save()
            print(('PASS: original documents eight CLI/eight MCP, nonempty histories, one payable and signed source-free restore; ' if args.original_documents else 'PASS: explicit native PDF parse/cache/manual expense, physical tamper refusal and signed source-free restore; ') + str(args.evidence))
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
            if owned() and args.original_documents and report['status'] != 'passed':
                report['failedProfilePreserved'] = str(work)
                report['ownedRuntimeCleaned'] = False
            elif owned():
                shutil.rmtree(work)
                report['ownedRuntimeCleaned'] = True
            else:
                report['ownedRuntimeCleaned'] = False
            save()


if __name__ == '__main__':
    main()
