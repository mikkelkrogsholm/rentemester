#!/usr/bin/env python3
"""Ordinary-MFA original UTF8/Windows1252 bank bytes, reviewed expense posting and signed recovery qualification.
Supplied binaries/web are privately pinned; only a fresh synthetic macOS profile is used.
All business changes use ordinary authorized UI/APIs. Signed SQLite is inspected read-only.
Only a private verified-TLS synthetic mail gateway is used; no external calls, identity seeds, implicit retry, or product build.
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
from decimal import Decimal
from pathlib import Path
REPO=Path(__file__).resolve().parents[2]
PREFIX='Rentemester-Synthetic-Bank-Original-Bytes83-'
SIGNER="const c=require('node:crypto'),f=require('node:fs'),k=c.generateKeyPairSync('ed25519'),s=k.privateKey.export({format:'der',type:'pkcs8'}).subarray(-32),p=k.publicKey.export({format:'der',type:'spki'}).subarray(-32);f.writeFileSync(process.argv[1],Buffer.concat([Buffer.from('3051020101300506032b657004220420','hex'),s,Buffer.from('812100','hex'),p]),{mode:0o600});f.writeFileSync(process.argv[2],p,{mode:0o600});"
BROWSER = r'''








import {createHmac,createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {createInterface} from 'node:readline';
import {readFileSync,readdirSync,writeFileSync,unlinkSync,lstatSync,realpathSync} from 'node:fs';
import {join} from 'node:path';
import {isDeepStrictEqual} from 'node:util';
const {chromium}=await import(process.env.RM_BANK_ORIGINAL_BYTES_PLAYWRIGHT);
const base=process.env.RM_BANK_ORIGINAL_BYTES_BASE,work=process.env.RM_BANK_ORIGINAL_BYTES_WORK;
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
const read=p=>get(ownerPage,p);
const post=async(p,b,page=ownerPage)=>{const r=await fetchJson(page,p,b);if(r.status!==200||r.body?.ok!==true){detail={path:p,status:r.status,code:r.body?.code,errors:r.body?.errors};throw Error('confirmed domain mutation refused');}return r.body;};
let today,issueDate,stable,freshOwner,sourceCookie,outgoingId,incomingId,incomingTaskId,bookingBody;
const originalsById={},receipts={};
const money=v=>{need(typeof v==='number'||typeof v==='string','explicit monetary wire');const s=String(v);need(/^-?\d+(\.\d{1,2})?$/.test(s),'exact cent fixture amount');const negative=s.startsWith('-'),[whole,part='']=s.replace('-','').split('.');return(BigInt(whole)*100n+BigInt(part.padEnd(2,'0')))*(negative?-1n:1n);};
const tasks=(status='all',archived)=>prefix+'exceptions?'+new URLSearchParams({status,...(archived===undefined?{}:{includeArchived:String(archived)})});
const rows=value=>value.exceptions.rows;
async function financial(){return{invoices:await read(prefix+'invoices'),documents:await read(prefix+'documents'),journal:await read(prefix+'journal'),trialBalance:await read(prefix+'trial-balance'),incomeStatement:await read(prefix+'income-statement'),vat:await read(prefix+'vat'),bank:await read(prefix+'bank'),periods:await read(prefix+'periods')};}
async function snapshot(){return{...await financial(),exceptions:await read(tasks())};}
async function originals(){for(const[id,proof]of Object.entries(originalsById)){const r=await ownerPage.request.get(base+prefix+`documents/${id}/file`);need(r.status()===200,'ordinary original readable');const raw=await r.body();need(raw.toString('base64')===proof.bytesBase64&&hash(raw)===proof.sha256,'exact retained supporting original');}return originalsById;}
function documentId(kind){return Number(Object.entries(originalsById).find(([id,proof])=>proof.kind===kind)[0]);}
const rawOriginals={};
function csv(extra=false){const [year,month,day]=issueDate.split('-'),date=`${day}.${month}.${year}`;return `Dato;Tekst;Beløb;Saldo;ref\r\n${date};Synthetic outgoing83 æøå €;-125,00;-125,00;BANK83-OUT\r\n${date};Synthetic incoming83 æøå €;200,00;75,00;BANK83-IN\r\n${date};Synthetic fee83 æøå €;-50,00;25,00;BANK83-FEE\r\n${date};Synthetic fee83 æøå €;-50,00;-25,00;BANK83-FEE\r\n${extra?`${date};Synthetic fee83 æøå €;-50,00;-75,00;BANK83-FEE\r\n`:''}`;}
function originalBytes(kind){if(kind==='web')return Buffer.concat([Buffer.from([239,187,191]),Buffer.from(csv())]);return Buffer.from([...csv(kind==='mcp')].map(c=>{const n=c.codePointAt(0);need(n<=255||n===0x20ac,'explicit synthetic Windows1252 fixture alphabet');return n===0x20ac?0x80:n;}));}
function importBody(kind='web'){return{account:'synthetic-bank83',profile:'danske-bank',csvBase64:originalBytes(kind).toString('base64'),confirm:true};}
function keepRaw(kind){const bytes=originalBytes(kind);rawOriginals[kind]={bytesBase64:bytes.toString('base64'),sha256:hash(bytes),encoding:kind==='web'?'utf8-bom':'windows1252'};}

async function setup(){step='ordinary-VAT-profile-bank-and-accounts';today=(await read(prefix+'scheduled-recurring')).scheduledRecurring.asOfDate;const d=new Date(today+'T12:00:00Z');d.setUTCDate(d.getUTCDate()-1);issueDate=d.toISOString().slice(0,10);need(issueDate.slice(0,4)===today.slice(0,4)&&Math.floor((Number(issueDate.slice(5,7))-1)/3)===Math.floor((Number(today.slice(5,7))-1)/3),'fixture same current fiscalyear/VATquarter');const profile=await ownerPage.request.patch(base+prefix+'company',{headers:{origin:base},data:{cvr:'87654321',address:'Synthetic Bank Street 83',postalCode:'1000',city:'Synthetic City',vatPeriodType:'quarter'}});need(profile.status()===200,'ordinary legal profile');for(const[accountNo,name,type,normalBalance]of[['1800','Synthetic DKK bank','asset','debit'],['6000','Synthetic purchase expense','expense','debit'],['4200','Synthetic output VAT','vat','credit'],['4300','Synthetic input VAT','vat','debit']])await post(prefix+'accounts',{accountNo,name,type,normalBalance});for(const[role,accountNo]of[['bank','1800'],['output_vat','4200'],['input_vat','4300']])await post(prefix+'account-roles',{role,accountNo,confirm:true});await post(prefix+'bank-accounts',{name:'Synthetic task bank83',slug:'synthetic-bank83',currency:'DKK',ledgerAccountNo:'1800'});return{today,issueDate};}
async function taskUi(expected){await ownerPage.goto(base+'/companies/synthetic/undtagelser');const counts=new Map();for(const task of expected)counts.set(task.message,(counts.get(task.message)||0)+1);for(const[message,count]of counts){const found=ownerPage.getByText(message,{exact:true});await found.first().waitFor();need(await found.count()===count,'visible task duplicate inventory');}return true;}
function checkOpen(value,want){const open=rows(value).filter(r=>r.status==='open');need(open.length===want&&open.every(r=>r.type==='UNMATCHED_BANK_TRANSACTION'),'exact open bank task inventory');for(const item of open){need(item.sourceEvidence.currency==='DKK'&&item.sourceEvidence.bankTransactionId===item.relatedBankTransactionId,'exact task bank identity/currency');need(item.requiredAction?.includes('kontrollér')||item.requiredAction?.includes('Kontrollér'),'Danish explicit review guidance');}return open;}
async function importUi(){step='ordinary-web-exact-UTF8-BOM-CRLF-bank-source';await ownerPage.goto(base+'/companies/synthetic/bank');await ownerPage.getByRole('button',{name:'Importér kontoudtog',exact:true}).click();const form=ownerPage.getByRole('dialog',{name:'Importér kontoudtog',exact:true});await form.getByLabel(/^CSV-fil/).setInputFiles({name:'synthetic-bank83-utf8.csv',mimeType:'text/csv',buffer:originalBytes('web')});await form.getByLabel(/^Bankkonto \(valgfri\)/).fill('synthetic-bank83');await form.getByLabel(/^Bankformat \(valgfri\)/).fill('danske-bank');const pending=ownerPage.waitForResponse(r=>new URL(r.url()).pathname===prefix+'bank/import'&&r.request().method()==='POST');await form.getByRole('button',{name:'Importér',exact:true}).click();const response=await pending;need(response.status()===200,'ordinary named raw bank import');const wire=response.request().postDataJSON();need(wire.csvBase64===originalBytes('web').toString('base64')&&wire.csvContent===undefined&&wire.profile==='danske-bank','web transports exact source bytes without text decoding');receipts.web={input:wire,result:await response.json()};keepRaw('web');await form.getByRole('button',{name:'Luk',exact:true}).click();const bank=(await read(prefix+'bank')).bank;need(bank.transactions.length===4&&bank.matchedCount===0&&bank.unmatchedCount===4,'four first-source rows, including two distinct fee occurrences');outgoingId=bank.transactions.find(r=>r.reference==='BANK83-OUT').id;incomingId=bank.transactions.find(r=>r.reference==='BANK83-IN').id;const open=checkOpen(await read(tasks()),4);need(bank.transactions.filter(r=>r.reference==='BANK83-FEE').length===2&&bank.transactions.every(r=>r.text.includes('æøå €')),'truthful decoded Danish and euro text and duplicate ordinal inventory');await taskUi(open);return{outgoingId,incomingId,rawOriginals,receipt:receipts.web};}
async function encodedImports(){step='CLI-Windows1252-equivalence-and-MCP-third-duplicate-occurrence';const session=await clientSession();try{const before=await snapshot(),cli=session.cli('import-bank',importBody('cli'));need(cli.code===0&&cli.data.ok===true,'explicit CLI raw Windows1252 import');keepRaw('cli');receipts.cli={input:importBody('cli'),result:cli.data};need(isDeepStrictEqual(await snapshot(),before),'equivalent second encoding adds no row/task/journal');const mcp=session.rpc('import_bank',{company:'synthetic',body:JSON.stringify(importBody('mcp')),confirm:true});need(!mcp.isError&&mcp.structuredContent.ok===true,'explicit MCP raw Windows1252 import');keepRaw('mcp');receipts.mcp={input:importBody('mcp'),result:mcp.structuredContent};const after=await snapshot();need(after.bank.bank.transactions.length===5&&after.bank.bank.transactions.filter(r=>r.reference==='BANK83-FEE').length===3&&after.journal.journal.entries.length===0,'MCP imports exactly third identical fee occurrence without accounting');checkOpen(after.exceptions,5);return{rawOriginals,receipts};}finally{unlinkSync(session.cookiePath);}}
async function uploadUi(kind,currency){await ownerPage.goto(base+'/companies/synthetic/bilag');await ownerPage.getByRole('button',{name:'Indlæs bilag',exact:true}).click();const form=ownerPage.getByRole('dialog',{name:'Indlæs bilag',exact:true});const bytes=Buffer.from(`Synthetic purchase ${kind}82. ${issueDate}. ${currency}125 incl VAT25. Supplier Synthetic Supplier83, DK12345678, Synthetic Supplier Street83. Buyer Synthetic Bank Company, DK87654321, Synthetic Bank Street83. Invoice BANK83-${kind}. Distinct original.\n`);await form.getByLabel('Bilagsfil',{exact:true}).setInputFiles({name:`synthetic-${kind}82.txt`,mimeType:'text/plain',buffer:bytes});await form.getByLabel('Bilagstype',{exact:true}).selectOption('purchase_sale');for(const[label,value]of[['Bilagsdato',issueDate],['Fakturanr. (valgfri)',`BANK83-${kind}`],['Beløb inkl. moms','125'],['Momsbeløb','25'],['Valuta',currency],['Beskrivelse af leverance','Synthetic reviewed purchase83'],['Afsender','Synthetic Supplier83'],['Afsender CVR/moms','DK12345678'],['Leverandørland (ISO)','DK'],['Afsenderadresse','Synthetic Supplier Street83'],['Modtager','Synthetic Bank Company'],['Modtager CVR/moms','DK87654321'],['Modtageradresse','Synthetic Bank Street83']])await form.getByLabel(label,{exact:true}).fill(value);await form.getByLabel(/^Identitetstype/).selectOption('dk_cvr');const pending=ownerPage.waitForResponse(r=>new URL(r.url()).pathname===prefix+'documents/ingest'&&r.request().method()==='POST');await form.getByRole('button',{name:'Indlæs bilag',exact:true}).click();const response=await pending;need(response.status()===200,'ordinary web document ingestion');const uploaded=await response.json();originalsById[uploaded.document.id]={kind,bytesBase64:bytes.toString('base64'),sha256:hash(bytes)};await form.getByRole('button',{name:'Luk',exact:true}).click();return uploaded.document;}
async function documentJourney(){step='one-genuine-DKK-purchase-original-and-unproven-hint';const before=await read(prefix+'journal');await uploadUi('first','DKK');const current=await read(tasks()),unique=checkOpen(current,5).find(r=>r.relatedBankTransactionId===outgoingId);need(unique.relatedDocumentId===documentId('first')&&unique.sourceEvidence.candidateBilagId===documentId('first')&&unique.message.includes('Beløbet alene beviser ikke'),'only amount/currency hint, requiring explicit review');need(rows(current).length===6&&rows(current).filter(r=>r.status==='resolved').length===1,'one superseded outgoing guidance task retained');need(isDeepStrictEqual(await read(prefix+'journal'),before),'guidance never posts');await taskUi([unique]);await originals();return{unique,originals:originalsById};}
function clientSession(){const cookiePath=join(realpathSync(work),`bank-client-${Date.now()}-${Math.random().toString(16).slice(2)}.cookie`);return ownerPage.context().cookies(base).then(cookies=>{writeFileSync(cookiePath,cookies.map(c=>`${c.name}=${c.value}`).join('; '),{mode:0o600,flag:'wx'});const common=['--server',base,'--cookie-file',cookiePath,'--allow-loopback-http'];return{cookiePath,cli(command,body,selectors=[]){const run=spawnSync(join(work,'rentemester-rust'),['remote',command,...common,'--company','synthetic',...selectors,...(body?['--confirm']:[])],{input:body?JSON.stringify(body):undefined,encoding:'utf8',timeout:45000,maxBuffer:32*1024*1024});need(!run.error,'CLI terminal');return{code:run.status,data:JSON.parse(run.stdout)};},rpc(name,args){const input=[{jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'synthetic-bank-original-bytes83',version:'1'}}},{jsonrpc:'2.0',method:'notifications/initialized'},{jsonrpc:'2.0',id:2,method:'tools/call',params:{name,arguments:args}}].map(JSON.stringify).join('\n')+'\n';const run=spawnSync(join(work,'rentemester-mcp'),common,{input,encoding:'utf8',timeout:45000,maxBuffer:32*1024*1024});need(!run.error&&run.status===0,'MCP terminal');const response=run.stdout.trim().split('\n').map(JSON.parse).find(v=>v.id===2);need(!response.error,'MCP protocol');return response.result;}};});}
function checkFinal(value){need(value.journal.journal.entries.length===1&&value.bank.bank.transactions.length===5&&value.bank.bank.matchedCount===1&&value.bank.bank.unmatchedCount===4,'one posting/reconciliation and retained unmatched incoming');need(money(value.bank.bank.bookedBalance)===money(-125)&&money(value.bank.bank.actualBalance)===money(-75)&&money(value.bank.bank.difference)===money(-50),'truthful independent bank statement difference -50');need(money(value.vat.vat.inputVat)===money(25)&&money(value.vat.vat.outputVat)===0n,'exact DK purchase inputVAT25/outputzero');const all=rows(value.exceptions);need(all.length===6&&all.filter(r=>r.status==='resolved').length===2&&all.filter(r=>r.status==='open').length===4,'six retained tasks: two resolved and four unmatched open');}
async function bookAndReview(){step='bank-row-explicit-one-purchase-review-from-retained-raw-source';await ownerPage.goto(base+'/companies/synthetic/bank');const row=ownerPage.getByRole('row').filter({hasText:'Synthetic outgoing83 æøå €'});await row.getByRole('button',{name:'Bogfør',exact:true}).click();const picker=ownerPage.getByRole('dialog',{name:'Vælg bilag til bankpost',exact:true});need(await picker.getByLabel('Bilag',{exact:true}).inputValue()==='','purchase never implicitly selected');await picker.getByLabel('Bilag',{exact:true}).selectOption(String(documentId('first')));await picker.getByRole('button',{name:'Gennemgå bilag',exact:true}).click();const form=ownerPage.getByRole('dialog',{name:'Bogfør bilag',exact:true});await form.getByLabel(/^Udgiftskonto/).selectOption('6000');need(await form.getByLabel(/^Banktransaktion \(uafstemt, udgående\)/).inputValue()===String(outgoingId),'review exact bank source');await form.getByLabel(/^Moms-behandling \(valgfri — udledes ellers af kontoen\)/).selectOption('standard');const pending=ownerPage.waitForResponse(r=>new URL(r.url()).pathname===prefix+'documents/book-expense'&&r.request().method()==='POST');await form.getByRole('button',{name:'Bogfør',exact:true}).click();const response=await pending;bookingBody=response.request().postDataJSON();need(bookingBody.documentId===documentId('first')&&bookingBody.bankTransactionId===outgoingId&&bookingBody.expenseAccountNo==='6000','explicit reviewed original, bank and expense account submitted');need(response.status()===200,'reviewed outgoing purchase posted once');receipts.booking=await response.json();await form.getByRole('button',{name:'Luk',exact:true}).click();const afterPosting=await snapshot();need(rows(afterPosting.exceptions).filter(r=>r.status==='open').length===4&&afterPosting.journal.journal.entries.length===1,'purchase resolves only outgoing task');const final=await snapshot();checkFinal(final);await originals();return{receipts,rawOriginals,final:final.bank,oneExplicitPurchasePosting:true};}

async function rights(){step='current-reviewer-write-denials-and-revoked-read';const before=await snapshot();need((await get(memberPage,tasks())).exceptions.count===6,'reviewer current task read');for(const[path,body]of[['bank/import',importBody()],['documents/ingest',{confirm:true,fileName:'forbidden83.txt',fileBase64:Buffer.from('forbidden').toString('base64'),metadata:{source:'synthetic83',documentType:'cash_register_receipt'}}],['documents/book-expense',bookingBody]])need((await fetchJson(memberPage,prefix+path,body)).status===403,'reviewer cannot import/upload/post');const valid=importBody();for(const input of [{...valid,csvContent:'date;text;amount\n2026-10-20;x;1\n'},{confirm:true,account:'synthetic-bank83'},{...valid,csvBase64:null},{...valid,csvBase64:'AB=='},{...valid,profile:'unknown'},{...valid,csvBase64:Buffer.from([255]).toString('base64'),profile:undefined}]){const denied=await fetchJson(ownerPage,prefix+'bank/import',input);need([400,422].includes(denied.status)&&denied.body?.ok===false,'malformed raw source/union/profile refuses without effects');}need(isDeepStrictEqual(await snapshot(),before),'readonly/denied requests preserve businessstate');await post('/api/workspace/members/company',{userId:memberId,action:'revoke',companySlug:'synthetic'});need((await fetchJson(memberPage,tasks())).status===403,'revoked taskread refused');return{reviewerReadQualified:true,reviewerImportUploadPostDenied:true,reviewerResolveNotDeniedByAssumption:true,revokedReadDenied:true};}
async function freeze(){step='canonical-bank-tasks-and-retained-originals';sourceCookie=(await ownerPage.context().cookies(base)).map(c=>`${c.name}=${c.value}`).join('; ');stable=await snapshot();checkFinal(stable);await originals();await taskUi(rows(stable.exceptions).filter(r=>r.status==='open'));await ownerPage.screenshot({path:process.env.RM_BANK_ORIGINAL_BYTES_SCREENSHOT,fullPage:true});return{canonical:stable,receipts,rawOriginals,originals:originalsById};}

async function archive(name){step='signed-bank-original-bytes83-'+name;const r=await ownerPage.request.post(base+'/api/workspace/backup',{headers:{origin:base},data:{confirm:true}});const ct=r.headers()['content-type'];if(r.status()!==200||ct!=='application/vnd.rentemester.native-backup+json'){let body;try{body=await r.json();}catch{}detail={status:r.status(),contentType:ct,code:body?.code,errors:body?.errors};throw Error('signed bank-task backup refused');}const bytes=await r.body();need(bytes.length&&bytes.length<=96*1024*1024,'bounded complete signed artifact');writeFileSync(join(work,`bank-original-bytes83-${name}.native-backup.json`),bytes,{mode:0o600,flag:'wx'});need(isDeepStrictEqual(await snapshot(),stable),'backup changes no business evidence');return{sha256:hash(bytes),bytes:bytes.length};}
async function reopen(){step='source-normal-mfa-restart';await ownerPage.context().clearCookies();await login(ownerPage,owner);need(isDeepStrictEqual(await snapshot(),stable),'exact source restart domain state');await originals();return{freshMfaRestart:true};}
async function restored(){step='source-free-normal-new-owner';ownerPage=await newPage();freshOwner={email:'restored@example.invalid',password:'synthetic-restored-password-789',codes:[]};await ownerPage.goto((await mail('verification',freshOwner.email)).actionUrl);need((await enroll(ownerPage,freshOwner)).user.id!==ownerId,'historical actor is not live restored grant');const denied=await ownerPage.request.get(base+prefix+'journal',{headers:{cookie:sourceCookie}});need([401,403].includes(denied.status()),'old source session has no authority');need(isDeepStrictEqual(await snapshot(),stable),'source-free exact unmatched bank task and financial state');await originals();return {sourceFreeRestoreQualified:true,credentialsAbsent:true};}
async function restoredReopen(){step='target-normal-mfa-reopen';await ownerPage.context().clearCookies();await login(ownerPage,freshOwner);need(isDeepStrictEqual(await snapshot(),stable),'target reopen exact retained accounting');await originals();return {restoredTargetReopenQualified:true};}
async function clientReads(){step='current-session-four-phase-client-task-and-accounting-parity';const before=await snapshot(),session=await clientSession();try{for(const command of ['journal','trial-balance','income-statement','vat','bank','invoices']){const canonical=await read(prefix+command),c=session.cli(command),m=session.rpc(command.replaceAll('-','_'),{company:'synthetic'});need(c.code===0&&c.data.ok===true&&isDeepStrictEqual(c.data.data,canonical),'full CLI accounting parity');need(!m.isError&&isDeepStrictEqual(m.structuredContent.data,canonical),'full MCP accounting parity');}for(const status of ['open','resolved','all'])for(const includeArchived of [false,true]){const canonical=await read(tasks(status,includeArchived)),c=session.cli('list-exceptions',undefined,['--status',status,'--include-archived',String(includeArchived)]),m=session.rpc('exceptions_list',{company:'synthetic',status,includeArchived});need(c.code===0&&c.data.ok===true&&isDeepStrictEqual(c.data.data,canonical),'exact CLI task selectors/current archived equality');need(!m.isError&&isDeepStrictEqual(m.structuredContent.data,canonical),'exact MCP task selectors/current archived equality');}need(isDeepStrictEqual(await snapshot(),before),'readonly clients preserve tasks and financialstate');return{httpCliMcpCanonicalEquality:true,allTaskStatusesAndArchiveSelectorsQualified:true,readonlyPreservesState:true};}finally{unlinkSync(session.cookiePath);}}

for await(const line of createInterface({input:process.stdin,crlfDelay:Infinity})){try{const action=JSON.parse(line);let value;if(action==='authenticate')value=await authenticate();else if(action==='setup')value=await setup();else if(action==='import')value=await importUi();else if(action==='encoded-imports')value=await encodedImports();else if(action==='documents')value=await documentJourney();else if(action==='book-and-review')value=await bookAndReview();else if(action==='rights')value=await rights();else if(action==='freeze')value=await freeze();else if(action==='reopen')value=await reopen();else if(action==='restored')value=await restored();else if(action==='restored-reopen')value=await restoredReopen();else if(action.startsWith('client-reads'))value=await clientReads();else if(action.startsWith('archive-'))value=await archive(action.slice(8));else if(action==='finish'){await browser?.close();emit({ok:true,value:{finished:true}});break;}else throw Error('unknown phase');emit({ok:true,value});}catch(error){if(ownerId&&diagnosticPage)await ownerPage.screenshot({path:process.env.RM_BANK_ORIGINAL_BYTES_SCREENSHOT,fullPage:true}).catch(()=>{});emit({ok:false,step,detail,errorType:error?.name,errorMessage:String(error?.message).slice(0,1200),trace});await browser?.close().catch(()=>{});process.exitCode=1;break;}}
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
    report = {'status':'running','sourceRevision':subprocess.check_output(['git','rev-parse','HEAD'],cwd=REPO,text=True).strip(),'runnerSha256':digest(Path(__file__)),'browserHelperSha256':hashlib.sha256(BROWSER.encode()).hexdigest(),'canonicalOrigin':origin,'noOfflineIdentitySeeds':True,'ordinaryMacOsUser':True,'ownedProfile':str(work),'externalMailDelivered':False,'workflow':'explicit_original_bank_bytes_and_reviewed_expense','installedNativeQualified':False,'bankOriginalBytesActualQualified':False,'sourceFreeRestoreQualified':False,'screenshot':str(args.evidence.with_suffix('.png')),'actions':[],'results':{}}
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
                self.send_error(404)

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
                argv += ['--restore-backup', str(work / 'bank-original-bytes83-source.native-backup.json'), '--trusted-backup-key', str(work / 'trusted.pub'), '--confirm-target', str(root), '--source-unavailable']
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
            progress('operator:independent-signed-bank-original-bytes83-' + name)
            path = work / ('bank-original-bytes83-' + name + '.native-backup.json')
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
                if any(secret.encode() in raw for secret in [bearer, config['authMail']['idempotencySecret'], config['cookieKey'], config['mfaKey']] if secret):
                    raise RuntimeError('Private credential present in signed inventory')
            raw = entries.get('synthetic/data/ledger.sqlite')
            if not raw or len(raw) > 32*1024*1024:
                raise RuntimeError('Bounded complete signed company database missing')
            copy = work / ('readonly-bank83-' + name + '.sqlite')
            result = {}
            def cell(value):
                return {'bytesBase64': base64.b64encode(value).decode()} if isinstance(value, bytes) else value
            try:
                with copy.open('xb') as f:
                    f.write(raw)
                db = sqlite3.connect(copy.as_uri()+'?mode=ro', uri=True)
                try:
                    db.execute('PRAGMA query_only=ON')
                    tables = [row[0] for row in db.execute("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")]
                    for table in tables:
                        if not re.fullmatch('[A-Za-z_][A-Za-z0-9_]*',table):
                            raise RuntimeError('Unexpected signed table name')
                        names = [row[1] for row in db.execute('PRAGMA table_info('+table+')')]
                        rows = db.execute('SELECT * FROM '+table+' ORDER BY '+','.join('"'+name+'"' for name in names))
                        result[table] = [{k:cell(v) for k,v in zip([c[0] for c in rows.description], row)} for row in rows.fetchall()]
                finally:
                    db.close()
            finally:
                copy.unlink(missing_ok=True)
            if len(result['journal_entries'])!=1 or len(result['documents'])!=1 or len(result['bank_transactions'])!=5 or sum(row['source_bank_transaction_id'] is not None for row in result['journal_entries'])!=1:
                raise RuntimeError('Exact one purchase journal/reconciliation, one purchase original and five bankrows required')
            for table in ['issued_invoice_postings','credit_note_postings','invoice_payments','invoice_refunds']:
                if result[table]:
                    raise RuntimeError('Unexpected invoice/payment/credit/refund effect: '+table)
            tasks=result['exceptions']
            if len(tasks)!=6 or sum(row['status']=='resolved' for row in tasks)!=2 or sum(row['status']=='open' for row in tasks)!=4 or any(row['type']!='UNMATCHED_BANK_TRANSACTION' for row in tasks):
                raise RuntimeError('Expected six retained unmatched tasks: two resolved and four open')
            signed_originals={name:hashlib.sha256(value).hexdigest() for name,value in entries.items() if '/documents/' in name or '/bank/' in name}
            for proof in report['results']['freeze']['originals'].values():
                wanted=base64.b64decode(proof['bytesBase64'],validate=True)
                if hashlib.sha256(wanted).hexdigest()!=proof['sha256'] or not any(value==wanted for name,value in entries.items() if '/documents/' in name):
                    raise RuntimeError('Exact supporting original missing from signed inventory')
            raw_proofs=report['results']['freeze']['rawOriginals']
            if set(raw_proofs)!={'web','cli','mcp'} or len({proof['sha256'] for proof in raw_proofs.values()})!=3:
                raise RuntimeError('Three distinct exact bank sources required')
            for kind,proof in raw_proofs.items():
                raw=base64.b64decode(proof['bytesBase64'],validate=True)
                if hashlib.sha256(raw).hexdigest()!=proof['sha256'] or not any(value==raw for value in entries.values()):
                    raise RuntimeError('Exact raw bank source variant missing: '+kind)
                if kind=='web' and (not raw.startswith(bytes([239,187,191])) or b'\r\n' not in raw):
                    raise RuntimeError('Original UTF8 BOM/CRLF lost')
                if kind!='web' and (bytes([128]) not in raw or bytes([230,248,229]) not in raw):
                    raise RuntimeError('Original Windows1252 Danish/euro bytes lost')
            fees=sorted((row for row in result['bank_transactions'] if row['reference']=='BANK83-FEE'),key=lambda row:row['id'])
            if len(fees)!=3 or any(Decimal(str(row['amount']))!=Decimal(-50) or row['text']!='Synthetic fee83 æøå €' for row in fees):
                raise RuntimeError('Three identical decoded fee occurrences required')
            for ordinal,row in enumerate(fees):
                source={'bank_account_id':row['bank_account_id'],'transaction_date':row['transaction_date'],'booking_date':row['booking_date'],'text':row['text'],'amount':-50,'currency':row['currency'],'reference':row['reference'],'amount_dkk':None,'fx_rate_to_dkk':None,'occurrence':ordinal}
                wanted=hashlib.sha256(json.dumps(source,ensure_ascii=False,separators=(',',':')).encode()).hexdigest()
                if row['transaction_hash']!=wanted or row['source_file_hash']!=raw_proofs['web' if ordinal<2 else 'mcp']['sha256']:
                    raise RuntimeError('Exact duplicate occurrence ordinal or original-source lineage differs')
            for row in result['bank_transactions']:
                profiles=[json.loads(a['message']) for a in result['audit_log'] if a['event_type']=='bank_import_profile' and a['entity_id']==row['import_batch_id']]
                if len(profiles)!=1 or profiles[0]['profile']!='danske-bank' or profiles[0]['sourceFileHash']!=row['source_file_hash']:
                    raise RuntimeError('Frozen named source-profile provenance missing')
            selected=next(int(id) for id,proof in report['results']['freeze']['originals'].items() if proof['kind']=='first')
            journal=result['journal_entries'][0]
            if journal['document_id']!=selected or journal['source_bank_transaction_id']!=report['results']['import']['outgoingId']:
                raise RuntimeError('Purchase journal does not bind the explicitly reviewed DKK original and outgoing bankrow')
            accounts={row['id']:row['account_no'] for row in result['accounts']}
            net={account:sum((Decimal(str(row['debit_amount']))-Decimal(str(row['credit_amount'])) for row in result['journal_lines'] if accounts[row['account_id']]==account),Decimal(0)) for account in ['1800','6000','4300','4200']}
            if net!={'1800':Decimal(-125),'6000':Decimal(100),'4300':Decimal(25),'4200':Decimal(0)}:
                raise RuntimeError('Exact bank/expense/inputVAT purchase effect differs')
            if result.get('vies_validations'):
                raise RuntimeError('Bank task flow must not create provider observations')
            result['signedOriginalHashes']=signed_originals
            report.setdefault('snapshots', {})[name] = {'sha256':receipt['sha256'],'trustedPublicKeySha256':digest(trusted),'privateCredentialsExcluded':True,'tables':result}
            save()
            return result

        def preserved(original, newer):
            if set(original)!=set(newer):
                raise RuntimeError('Restored full company table inventory changed')
            for table in original:
                if table!='audit_log' and original[table]!=newer[table]:
                    raise RuntimeError('Exact restored bank-task evidence differs: '+table)
            retained = {row['id']:row for row in newer['audit_log']}
            if any(retained.get(row['id']) != row for row in original['audit_log']):
                raise RuntimeError('Full original historical audit/provenance not retained')
            prior={row['id'] for row in original['audit_log']}
            if any(row['event_type'] not in ['native_restore_receipt_boundary','native_restore_scheduled_boundary'] or row['actor']!='system:portable-restore' for row in newer['audit_log'] if row['id'] not in prior):
                raise RuntimeError('Unexpected restore-boundary audit mutation')

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
                ['openssl', 'req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', str(work / 'ca.key'), '-out', str(ca), '-days', '1', '-subj', '/CN=Synthetic Bank83 Auth CA', '-addext', 'basicConstraints=critical,CA:TRUE', '-addext', 'keyUsage=critical,keyCertSign,cRLSign'],
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
            config = {'runtimeProfile': 'local-user-macos', 'root': str(work / 'workspace'), 'brokerUid': os.getuid(), 'workspace': 'synthetic-native-bank-original-bytes83', 'companySlug': 'synthetic', 'companyName': 'Synthetic Bank Company', 'cookieKey': key(), 'mfaKey': key(), 'operatorActors': ['system:synthetic-installer'], 'accounts': [], 'firstOwner': {'actor': 'system:synthetic-installer', 'email': 'owner@example.invalid', 'name': 'Synthetic Owner', 'password': 'synthetic-owner-password-123'}, 'backupSigningKeyFile': str(work / 'signing.der'), 'authMail': {'endpoint': f'https://127.0.0.1:{mail_port}/mail', 'bearerToken': bearer, 'from': 'auth@example.invalid', 'idempotencySecret': secrets.token_urlsafe(32), 'trustedCaFile': str(ca)}}
            write_config()
            start(create=True)
            script = work / 'browser.mjs'
            script.write_text(BROWSER)
            controller = subprocess.Popen(['/opt/homebrew/bin/bun', str(script)], cwd=REPO, env={**os.environ, 'RM_BANK_ORIGINAL_BYTES_BASE': origin, 'RM_BANK_ORIGINAL_BYTES_WORK': str(work), 'RM_BANK_ORIGINAL_BYTES_SCREENSHOT': str(args.evidence.with_suffix('.png')), 'RM_BANK_ORIGINAL_BYTES_PLAYWRIGHT': str((REPO / 'node_modules/@playwright/test/index.mjs').resolve(strict=True))}, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, bufsize=1, start_new_session=True)
            report['browserControllerPid'] = controller.pid
            phase('authenticate')
            phase('setup')
            phase('import')
            phase('encoded-imports')
            phase('documents')
            phase('book-and-review')
            phase('rights')
            phase('freeze')
            phase('client-reads-source')
            phase('archive-source')
            original = inspect_snapshot('source', work/'trusted.pub')
            stop()
            config.pop('firstOwner')
            write_config()
            start()
            phase('reopen')
            phase('client-reads-restarted')
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
            start(restore=True)
            phase('restored')
            phase('client-reads-restored')
            report['sourceFreeRestoreQualified'] = True
            phase('archive-restored')
            preserved(original, inspect_snapshot('restored',work/'target-trusted.pub'))
            stop()
            config.pop('firstOwner');config.pop('restoreActor');write_config();start()
            phase('restored-reopen')
            phase('client-reads-reopened')
            phase('archive-reopened')
            if inspect_snapshot('reopened',work/'target-trusted.pub') != report['snapshots']['restored']['tables']:
                raise RuntimeError('Fresh target reopen changed retained full bank-task evidence')
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
            with gateway_lock:
                if errors:
                    raise RuntimeError('Private auth gateway protocol error')
                report['gateway']={'verifiedHttps':True,'acceptedMessages':len(gateway_rows),'requests':list(gateway_requests),'externalRequests':0}
            report.update({'status':'passed','currentAction':'complete','bankOriginalBytesActualQualified':True,'httpCliMcpReadParityQualified':True,'snapshotCoverage':'four-independently-verified-bank-original-bytes83-full-ledger-original-archives-fresh-owner-restore-and-reopen'})
            save()
            print('PASS: ordinary MFA exact UTF8-BOM/Windows1252 raw bank sources, duplicate occurrences and reviewed purchase posting, canonical clients and signed source-free recovery; '+str(args.evidence))
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
            if owned():
                shutil.rmtree(work)
                report['ownedRuntimeCleaned'] = True
            else:
                report['ownedRuntimeCleaned'] = False
            save()



if __name__ == '__main__':
    main()
