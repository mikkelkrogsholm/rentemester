#!/usr/bin/env python3
"""Owned normal-MFA Danske UTF-8 bank profile UI, refusal and signed recovery.
Only supplied pinned artifacts and public product controls; no SQL identity/business
seeds, journal posting, real banks or mail. Read-only signed SQLite validates source
columns/history. Browser file.text strips BOM: archived original is exact posted UTF-8
text, not a promise of raw BOM or Windows-1252 file preservation.
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
PREFIX = 'Rentemester-Synthetic-Bank-Profile-'
BROWSER = r'''







import {createHmac,createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {createInterface} from 'node:readline';
import {readFileSync,readdirSync,writeFileSync,unlinkSync} from 'node:fs';
import {join} from 'node:path';
import {isDeepStrictEqual} from 'node:util';
const {chromium}=await import(process.env.RM_BANK_PROFILE_PLAYWRIGHT);
const base=process.env.RM_BANK_PROFILE_BASE,work=process.env.RM_BANK_PROFILE_WORK;
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
async function newPage(){const context=await browser.newContext();const page=await context.newPage();page.setDefaultTimeout(45000);diagnosticPage=page;page.on('request',r=>{if(new URL(r.url()).pathname==='/api/auth/request-password-reset')trace.push({kind:'request',path:'/api/auth/request-password-reset',method:r.method()});});page.on('requestfailed',r=>{if(new URL(r.url()).pathname==='/api/auth/request-password-reset')trace.push({kind:'requestfailed',path:'/api/auth/request-password-reset',code:r.failure()?.errorText?.match(/net::[A-Z_]+/)?.[0]});});page.on('response',r=>{const path=new URL(r.url()).pathname;if(path.startsWith('/api/auth/')){trace.push({path:path.startsWith('/api/auth/reset-password/')?'/api/auth/reset-password/:token':path,status:r.status()});if(trace.length>24)trace.shift();}});page.on('response',r=>{const u=new URL(r.url());if(/^\/api\/companies\/[a-z0-9-]+\/(bank|bank-accounts|journal|trial-balance|income-statement|balance)(\/|$)/.test(u.pathname)){trace.push({step,path:u.pathname,status:r.status(),method:r.request().method(),year:u.searchParams.get('year'),source:u.searchParams.get('source'),format:u.searchParams.get('format')});if(trace.length>32)trace.shift();}});return page;}
async function authenticate(){
 browser=await chromium.launch({headless:true});ownerPage=await newPage();memberPage=await newPage();
 step='owner-mail-verification';await ownerPage.goto((await mail('verification',owner.email)).actionUrl);const who=await enroll(ownerPage,owner);ownerId=who.user.id;
 step='ordinary-web-invitation';await ownerPage.goto(base+'/adgang');await ownerPage.getByRole('heading',{name:'Brugere og adgang',exact:true}).waitFor();await ownerPage.getByLabel('E-mail',{exact:true}).fill(member.email);await ownerPage.getByLabel(/^Rolle/).selectOption('reviewer');await ownerPage.getByRole('button',{name:'Send invitation',exact:true}).click();await ownerPage.getByText('Invitationen er sendt.',{exact:true}).waitFor();const invitation=await mail('workspace-invitation',member.email);const listed=await get(ownerPage,'/api/workspace/invitations');const item=listed.invitations.find(i=>i.email===member.email);need(item?.status==='delivery_confirmed','canonical invitation delivery');invitationId=item.invitationId;
 step='normal-invited-claim';await memberPage.goto(invitation.actionUrl);await memberPage.getByRole('heading',{name:'Acceptér invitation',exact:true}).waitFor();need(!new URL(memberPage.url()).hash,'invitation token removed from address');await memberPage.getByLabel('Navn',{exact:true}).fill('Synthetic Invited User');await memberPage.getByLabel('Adgangskode',{exact:true}).fill(member.password);const claim=memberPage.waitForResponse(r=>new URL(r.url()).pathname==='/api/invitations/claim'&&r.request().method()==='POST');await memberPage.getByRole('button',{name:'Acceptér invitation',exact:true}).click();const claimed=await claim;need(claimed.ok()&&!claimed.headers()['set-cookie'],'claim cannot mint session');await memberPage.getByText('Invitationen er accepteret.',{exact:true}).waitFor();need((await fetchJson(memberPage,'/api/companies/synthetic/trial-balance')).status===401,'claim not sufficient for business read');
 step='invited-mail-verification-existing-mfa';await memberPage.goto((await mail('verification',member.email)).actionUrl);const mwho=await enroll(memberPage,member);memberId=mwho.user.id;need(memberId!==ownerId,'independent ordinary users');baseline=await get(memberPage,'/api/companies/synthetic/trial-balance');
 return {ownerId,memberId,invitationId,ordinaryInvitationClaim:true,distinctNormalMfaUsers:true};
}

const prefix='/api/companies/synthetic/',hash=b=>createHash('sha256').update(b).digest('hex'),account='synthetic-profile-bank',year='2026';
const headers=['Dato','Rentedato','Tekst','Beløb','Valuta','Saldo','Afsender','Oprindelig_afsender','Modtagerkonto','Bésked','Arkivreference','Kundereference','Egen kolonne'];
const initialRows=[['04.04.2026','05.04.2026','Synthetic profile; "newest"','-20,00','DKK','1.130,00','','Synthetic fallback N','DK00-N','Message N','A-N','C-N','opaque; "N"'],['04.04.2026','04.04.2026','Synthetic profile same day prior','50,00','DKK','1.150,00','Synthetic primary P','Unused fallback P','DK00-P','Message P','A-P','C-P','opaque P'],['03.04.2026','03.04.2026','Synthetic profile oldest','100,00','DKK','1.100,00','  ','Synthetic fallback O','DK00-O','Message O','A-O','C-O','opaque O']];
const gapRows=[['07.04.2026','07.04.2026','Synthetic profile gap latest','-10,00','DKK','900,00','Synthetic gap G','','DK00-G','Gap message G','A-G','C-G','opaque G'],['06.04.2026','06.04.2026','Synthetic profile gap prior','-10,00','DKK','1.120,00','Synthetic gap B','','DK00-B','Gap message B','A-B','C-B','opaque B']];
const quote=v=>'"'+v.replaceAll('"','""')+'"';
const csv=rows=>headers.map(quote).join(';')+'\r\n'+rows.map(r=>r.map(quote).join(';')).join('\r\n')+'\r\n';
const csvs={initial:csv(initialRows),gap:csv(gapRows)};
const sourceBytes=kind=>Buffer.concat([Buffer.from([0xef,0xbb,0xbf]),Buffer.from(csvs[kind],'utf8')]);
const rawExpected=rows=>rows.map(row=>Object.fromEntries(headers.map((header,i)=>[header,row[i].trim()])));
const expected=[...initialRows,...gapRows].map(row=>({text:row[2],date:'2026-04-'+row[0].slice(0,2),bookingDate:'2026-04-'+row[1].slice(0,2),amount:Number(row[3].replace('.','').replace(',','.')),balanceAfter:Number(row[5].replaceAll('.','').replace(',','.')),counterpartyName:row[6].trim()||row[7].trim(),counterpartyAccount:row[8],message:row[9],archiveReference:row[10],customerReference:row[11],raw:Object.fromEntries(headers.map((h,i)=>[h,row[i].trim()]))}));
let sourceBooks,stable,freshOwner,selectedAccountId,lastBankResponse=null;const receipts={};
const read=p=>get(ownerPage,p);
async function call(path,data,page=ownerPage){return fetchJson(page,path,data);}
async function post(path,data){const r=await call(path,data);need(r.status===200&&r.body?.ok===true,'ordinary public operation '+path+' HTTP'+r.status);return r.body;}
async function books(){return{journal:await read(prefix+'journal'),trialBalance:await read(prefix+'trial-balance'),income:await read(prefix+'income-statement'),balance:await read(prefix+'balance')};}
async function snapshot(){return{bank:await read(prefix+'bank?year='+year),accounts:await read(prefix+'bank-accounts'),books:await books()};}
async function setup(){step='explicit-bank-account-and-source-registry';await post(prefix+'accounts',{accountNo:'1872',name:'Synthetic Bank Profile Asset',type:'asset',normalBalance:'debit'});await post(prefix+'account-roles',{role:'bank',accountNo:'1872',confirm:true});await post(prefix+'bank-accounts',{slug:account,name:'Synthetic profile bank',currency:'DKK',ledgerAccountNo:'1872'});const accounts=await read(prefix+'bank-accounts'),profiles=accounts.bankAccounts.profiles;const selected=accounts.bankAccounts.accounts.find(a=>a.slug===account);need(Number.isSafeInteger(selected?.id)&&selected.active===true&&selected.ledgerAccountNo==='1872','explicit canonical bank account');selectedAccountId=selected.id;need(profiles.length===1&&profiles[0].name==='danske-bank'&&profiles[0].label==='Danske Bank account export'&&profiles[0].delimiter===';'&&profiles[0].encoding==='utf8'&&profiles[0].dateOrder==='dmy','full actual sole source profile discovery');sourceBooks=await books();need(sourceBooks.journal.journal.entries.length===0,'legitimate empty journal');return{accounts,sourceBooks,fixtures:{initial:{uploadedBomSha256:hash(sourceBytes('initial')),postedSha256:hash(Buffer.from(csvs.initial)),raw:rawExpected(initialRows)},gap:{uploadedBomSha256:hash(sourceBytes('gap')),postedSha256:hash(Buffer.from(csvs.gap)),raw:rawExpected(gapRows)}}};}
function input(kind){return{csvContent:csvs[kind],account,profile:'danske-bank',confirm:true};}
function receiptText(r){return `${r.imported} ${r.imported===1?'transaktion':'transaktioner'} importeret${r.skippedDuplicates>0?' · '+r.skippedDuplicates+' dublet'+(r.skippedDuplicates===1?'':'ter')+' sprunget over':''}${r.exceptionsCreated>0?' · '+r.exceptionsCreated+' ny'+(r.exceptionsCreated===1?'':'e')+' opgave'+(r.exceptionsCreated===1?'':'r'):''}.`;}
async function uiImport(kind,repeat=false){step='web-profile-'+kind+(repeat?'-repeat':'')+'-open';await ownerPage.goto(base+'/companies/synthetic/bank?year='+year);await ownerPage.getByRole('heading',{name:'Bank',exact:true}).waitFor();await ownerPage.getByRole('button',{name:'Importér kontoudtog',exact:true}).click();const dialog=ownerPage.getByRole('dialog',{name:'Importér kontoudtog',exact:true});step='web-profile-'+kind+'-explicit-file-account-profile';await dialog.getByLabel('CSV-fil',{exact:true}).setInputFiles({name:'synthetic-danske-'+kind+'.csv',mimeType:'text/csv',buffer:sourceBytes(kind)});await dialog.getByLabel(/^Bankkonto \(valgfri\)/).fill(account);await dialog.getByLabel(/^Bankformat \(valgfri\)/).fill('danske-bank');const wait=ownerPage.waitForResponse(r=>new URL(r.url()).pathname===prefix+'bank/import'&&r.request().method()==='POST');step='web-profile-'+kind+'-single-import';await dialog.getByRole('button',{name:'Importér',exact:true}).click();const response=await wait,body=await response.json(),sent=response.request().postDataJSON();lastBankResponse={step,status:response.status(),body};need(response.status()===200&&body.ok===true&&sent.account===account&&sent.profile==='danske-bank'&&sent.confirm===true,'explicit selected account/profile real UI receipt');need(sent.csvContent===new TextDecoder().decode(sourceBytes(kind))&&sent.csvContent===csvs[kind],'archive exact posted UTF8 text; browser file.text strips BOM');const r=body.import,count=kind==='initial'?3:2;need(r.sourceFileHash===hash(Buffer.from(sent.csvContent))&&r.bankAccountId===selectedAccountId,'receipt exact posted source hash and selected account identity');need(r.profile==='danske-bank'&&r.bankAccountSlug===account&&r.imported===(repeat?0:count)&&r.skippedDuplicates===(repeat?count:0),'honest imported/skipped receipt');if(!repeat)need(r.firstBalance===(kind==='initial'?1100:1120)&&r.lastBalance===(kind==='initial'?1130:900),'source-reconstructed first/last balances');need(r.balanceWarnings.length===(!repeat&&kind==='gap'?1:0)&&r.exceptionsCreated===(!repeat&&kind==='gap'?1:0),'newly imported batch gap warning/exception counts only');step='web-profile-'+kind+'-exact-full-receipt';await dialog.getByText(receiptText(r),{exact:true}).waitFor();for(const warning of r.balanceWarnings)await dialog.getByText(warning,{exact:true}).waitFor();await dialog.getByRole('button',{name:'Luk',exact:true}).click();need(isDeepStrictEqual(await books(),sourceBooks),'bank profile import is not journal posting');receipts[kind+(repeat?'-repeat':'')]=body;return{body,postedUtf8Sha256:hash(Buffer.from(sent.csvContent)),uploadedBomFixtureSha256:hash(sourceBytes(kind)),rawWindows1252Qualified:false};}
async function initial(){const result=await uiImport('initial');const bank=(await read(prefix+'bank?year='+year)).bank;need(bank.transactions.length===3&&bank.actualBalance===1130&&bank.bookedBalance===0,'declared DMY/same-day saldo reconstruct independent1130 closing');await verifyRows(bank.transactions,expected.slice(0,3));return result;}
async function verifyRows(rows,source){for(const e of source){const matches=rows.filter(r=>r.text===e.text);need(matches.length===1,'exact unique source transaction '+e.text);const r=matches[0];for(const key of['date','bookingDate','amount','balanceAfter','counterpartyName','counterpartyAccount','message','archiveReference','customerReference','raw'])need(isDeepStrictEqual(r[key],e[key]),'exact canonical '+key+' '+e.text);const sourceKind=initialRows.some(row=>row[2]===e.text)?'initial':'gap';need(r.sourceFileHash===hash(Buffer.from(csvs[sourceKind]))&&r.importBatchId===receipts[sourceKind].import.importBatchId,'canonical row bound to original import identity');need(r.reference===null,'no invented reference/FX');}}
async function repeat(kind){const before=await snapshot(),result=await uiImport(kind,true);need(isDeepStrictEqual(await snapshot(),before),'duplicates add no transaction/exception/journal effects; source import-attempt audit remains legitimate');return result;}
async function gap(){const result=await uiImport('gap'),bank=(await read(prefix+'bank?year='+year)).bank;need(bank.transactions.length===5&&bank.actualBalance===900&&bank.bookedBalance===0&&bank.matchedCount===0&&bank.unmatchedCount===5,'gap retained, allfive rows remain unposted/unmatched');await verifyRows(bank.transactions,expected);stable=await snapshot();await ownerPage.screenshot({path:process.env.RM_BANK_PROFILE_SCREENSHOT,fullPage:true});return{...result,canonical:stable};}
function negativeInput(kind){const rows=initialRows.map(r=>[...r]);rows[0][2]='Synthetic negative '+kind;const value={csvContent:csv(rows),account,profile:'danske-bank',confirm:true};if(kind==='profile')value.profile='synthetic-unknown-profile';else if(kind==='date')value.csvContent=csv(rows.map((r,i)=>i===1?['33.04.2026',...r.slice(1)]:r));else if(kind==='fx')value.csvContent=csv(rows.map(r=>[...r.slice(0,4),'EUR',...r.slice(5)]));else if(kind==='account')value.account='synthetic-nonexistent-bank';else if(kind==='generic')delete value.profile;else throw Error('unknown negative fixture');return value;}
async function negative(kind){step='atomic-refusal-'+kind;const before=await snapshot(),r=await call(prefix+'bank/import',negativeInput(kind));lastBankResponse={step,...r};need(r.status===422&&r.body?.ok===false&&r.body.code===(kind==='account'?'bank_refused':'invalid_bank_csv'),'typed exact import refusal '+kind);need(isDeepStrictEqual(await snapshot(),before),'invalid batch does not partially mutate bank/books '+kind);return{refusal:r};}
async function unauthorized(){step='ordinary-reviewer-current-import-denial';const before=await snapshot(),r=await call(prefix+'bank/import',negativeInput('account'),memberPage);lastBankResponse={step,...r};need(r.status===403&&r.body?.ok===false&&r.body.code==='access_denied','current reviewer cannot import');need(isDeepStrictEqual(await snapshot(),before),'unauthorized import has no effects');return{refusal:r};}
async function client(kind,mode){const file=join(work,'bank-profile-client.cookie');writeFileSync(file,(await ownerPage.context().cookies(base)).map(c=>`${c.name}=${c.value}`).join('; '),{mode:0o600,flag:'wx'});try{const common=['--server',base,'--cookie-file',file,'--allow-loopback-http'];let argv,input;if(kind==='cli')argv=['remote',mode==='bank'?'bank':'bank-accounts',...common,'--company','synthetic',...(mode==='bank'?['--year',year]:[])];else{argv=common;input=[{jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'synthetic-bank-profile-proof',version:'1'}}},{jsonrpc:'2.0',method:'notifications/initialized'},{jsonrpc:'2.0',id:2,method:'tools/call',params:{name:mode==='bank'?'bank':'bank_accounts',arguments:{company:'synthetic',...(mode==='bank'?{year}:{})}}}].map(v=>JSON.stringify(v)).join('\n')+'\n';}const r=spawnSync(join(work,kind==='cli'?'rentemester-rust':'rentemester-mcp'),argv,{input,encoding:'utf8',timeout:45000,maxBuffer:16*1024*1024});need(!r.error&&r.status===0,kind+' canonicalbank reader terminal');const value=kind==='cli'?JSON.parse(r.stdout):r.stdout.trim().split('\n').map(s=>JSON.parse(s)).find(v=>v.id===2)?.result?.structuredContent;need(value?.ok===true&&value.data?.ok===true,'canonical '+kind+' envelope');return value.data;}finally{unlinkSync(file);}}
async function clients(){step='http-cli-mcp-whole-bank-profile-read-parity';const results={};for(const kind of['cli','mcp']){results[kind]={};for(const mode of['bank','accounts']){const expected=await read(prefix+(mode==='bank'?'bank?year='+year:'bank-accounts')),actual=await client(kind,mode);need(isDeepStrictEqual(actual,expected),'full native '+kind+' '+mode+' canonical parity');results[kind][mode]=actual;}}await assertStable();return results;}
async function assertStable(){need(isDeepStrictEqual(await snapshot(),stable),'exact imported source/account/raw/balances/books/UUID survive read/restart/restore');}
async function reopen(){step='fresh-mfa-source-restart';await ownerPage.context().clearCookies();await login(ownerPage,owner);await assertStable();return{freshMfaRestart:true};}
async function archive(name){step='explicit-signed-bank-profile-'+name;const r=await ownerPage.request.post(base+'/api/workspace/backup',{headers:{origin:base},data:{confirm:true}});need(r.status()===200&&r.headers()['content-type']==='application/vnd.rentemester.native-backup+json','actual signed bank history snapshot');const bytes=await r.body();need(bytes.length>0&&bytes.length<=96*1024*1024,'bounded full signed archive');writeFileSync(join(work,'bank-profile-'+name+'.native-backup.json'),bytes,{mode:0o600,flag:'wx'});await assertStable();return{sha256:hash(bytes),bytes:bytes.length};}
async function restored(){step='independent-source-free-fresh-owner-normal-mfa';ownerPage=await newPage();freshOwner={email:'restored@example.invalid',password:'synthetic-restored-password-789',codes:[]};await ownerPage.goto((await mail('verification',freshOwner.email)).actionUrl);need((await enroll(ownerPage,freshOwner)).user.id!==ownerId,'fresh owner is not imported historical authority');await assertStable();return{sourceFreeRestoreQualified:true};}
async function restoredReopen(){step='fresh-mfa-restored-target-reopen';await ownerPage.context().clearCookies();await login(ownerPage,freshOwner);await assertStable();return{restoredTargetReopenQualified:true};}
for await(const line of createInterface({input:process.stdin,crlfDelay:Infinity})){try{const action=JSON.parse(line);let value;if(action==='authenticate')value=await authenticate();else if(action==='setup')value=await setup();else if(action==='initial')value=await initial();else if(action==='repeat-initial')value=await repeat('initial');else if(action==='gap')value=await gap();else if(action==='repeat-gap')value=await repeat('gap');else if(action.startsWith('negative-'))value=await negative(action.slice(9));else if(action==='unauthorized')value=await unauthorized();else if(action==='clients'||action==='client-reads')value=await clients();else if(action==='reopen')value=await reopen();else if(['archive-source','archive-restarted','archive-restored','archive-reopened'].includes(action))value=await archive(action.slice(8));else if(action==='restored')value=await restored();else if(action==='restored-reopen')value=await restoredReopen();else if(action==='finish'){await browser?.close();emit({ok:true,value:{finished:true}});break;}else throw Error('unknown phase');emit({ok:true,value});}catch(error){let pageState=null;try{pageState=await Promise.race([diagnosticPage?.evaluate(()=>({pathname:location.pathname,headings:[...document.querySelectorAll('h1,h2,h3,h4')].map(e=>e.textContent.slice(0,100)),buttons:[...document.querySelectorAll('button')].map(e=>e.textContent.slice(0,100)),alerts:[...document.querySelectorAll('[role=alert]')].map(e=>e.textContent.slice(0,300))})),new Promise(resolve=>setTimeout(()=>resolve(null),2000))]);}catch{}emit({ok:false,step,detail,errorType:error?.name,trace,pageState,lastBankResponse});await Promise.race([browser?.close().catch(()=>{}),new Promise(resolve=>setTimeout(resolve,2000))]);process.exitCode=1;break;}}
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
    report = {'status': 'running', 'sourceRevision': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=REPO, text=True).strip(), 'runnerSha256': digest(Path(__file__)), 'browserHelperSha256': hashlib.sha256(BROWSER.encode()).hexdigest(), 'canonicalOrigin': origin, 'noOfflineIdentitySeeds': True, 'ordinaryMacOsUser': True, 'ownedProfile': str(work), 'externalMailDelivered': False, 'workflow': 'named_danske_bank_profile', 'bankProfileActualQualified': False, 'rawWindows1252Qualified': False, 'rawBrowserFileBomPreserved': False, 'sourceOriginalBasis': 'exact_browser_decoded_posted_utf8_text', 'rawJsonBasis': 'source_csv_parser_trims_cells_preserves_original_header_keys', 'installedNativeQualified': False, 'screenshot': str(args.evidence.with_suffix('.png')),  'actions': [], 'results': {}, 'sourceFreeRestoreQualified': False}
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
                argv += ['--restore-backup', str(work / 'bank-profile-source.native-backup.json'), '--trusted-backup-key', str(work / 'trusted.pub'), '--confirm-target', str(root), '--source-unavailable']
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

        def original_inventory():
            directory = Path(config['root']) / 'synthetic/bank/processed'
            if not owned() or directory.is_symlink():
                raise RuntimeError('Exact owned bank source directory required')
            if not directory.exists():
                return {}
            paths = list(directory.rglob('*'))
            if any(p.is_symlink() for p in paths):
                raise RuntimeError('Unexpected original symlink')
            return {str(p.relative_to(directory)): digest(p) for p in sorted(paths) if p.is_file()}

        def inspect_snapshot(name, trusted):
            progress('operator:independent-signed-bank-profile-evidence-' + name)
            path = work / ('bank-profile-' + name + '.native-backup.json')
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
                if any(secret.encode() in raw for secret in [bearer, config['authMail']['idempotencySecret'], config['cookieKey'], config['mfaKey']]):
                    raise RuntimeError('Private operator credential found in signed inventory')
            entries = [entry for entry in wire['entries'] if entry['path'] == 'synthetic/data/ledger.sqlite']
            if len(entries) != 1:
                raise RuntimeError('Exact signed company SQLite required')
            raw = base64.b64decode(entries[0]['bytesBase64'], validate=True)
            if not 0 < len(raw) <= 32 * 1024 * 1024:
                raise RuntimeError('Bounded signed SQLite copy required')
            copy = work / ('readonly-bank-profile-' + name + '.sqlite')
            tables = ['bank_accounts', 'bank_transactions', 'accounts', 'companies', 'exceptions', 'audit_log', 'journal_entries', 'journal_lines']
            try:
                with copy.open('xb') as output:
                    output.write(raw)
                db = sqlite3.connect(copy.as_uri() + '?mode=ro', uri=True)
                try:
                    db.execute('PRAGMA query_only=ON')
                    result = {}
                    for table in tables:
                        cursor = db.execute('SELECT * FROM ' + table + ' ORDER BY id')
                        result[table] = [dict(zip([c[0] for c in cursor.description], row)) for row in cursor.fetchall()]
                finally:
                    db.close()
            finally:
                copy.unlink(missing_ok=True)
            if len(result['bank_transactions']) != 5 or result['journal_entries'] or result['journal_lines']:
                raise RuntimeError('Exactly five retained bank rows and zero journal effects required')
            account_rows = [row for row in result['bank_accounts'] if row['slug'] == 'synthetic-profile-bank']
            if len(account_rows) != 1:
                raise RuntimeError('Caller-selected canonical bank account missing')
            expected_by_text = {}
            for kind in ('initial', 'gap'):
                fixture = report['results']['setup']['fixtures'][kind]
                for original in fixture['raw']:
                    expected_by_text[original['Tekst']] = (original, fixture['postedSha256'])
                matches = [entry for entry in wire['entries'] if entry['path'] == 'synthetic/bank/processed/' + fixture['postedSha256'] + '.txt']
                if len(matches) != 1:
                    raise RuntimeError('Exact posted UTF-8 source original missing from signed inventory')
                source = base64.b64decode(matches[0]['bytesBase64'], validate=True)
                if hashlib.sha256(source).hexdigest() != fixture['postedSha256'] or source.startswith(b'\xef\xbb\xbf'):
                    raise RuntimeError('Signed source differs from exact browser-decoded posted UTF-8 text')
            original_entries = [entry for entry in wire['entries'] if entry['path'].startswith('synthetic/bank/processed/')]
            if len(original_entries) != 2:
                raise RuntimeError('Only two accepted originals; failed imports must leave no orphan source')
            for row in result['bank_transactions']:
                expected, sha = expected_by_text[row['text']]
                if row['bank_account_id'] != account_rows[0]['id'] or row['source_file_hash'] != sha or json.loads(row['raw_json']) != expected:
                    raise RuntimeError('Account-scoped source hash and every raw original header/value must survive')
                sender = expected['Afsender'].strip() or expected['Oprindelig_afsender'].strip()
                if row['counterparty_name'] != sender or row['counterparty_account'] != expected['Modtagerkonto'] or row['archive_reference'] != expected['Arkivreference'] or row['customer_reference'] != expected['Kundereference']:
                    raise RuntimeError('Canonical sender fallback or references changed')
            profile_audits = [row for row in result['audit_log'] if row['event_type'] == 'bank_import_profile']
            if len(profile_audits) != 2:
                raise RuntimeError('Two native named-profile source attestations, no duplicate profile event')
            hashes = set()
            for row in profile_audits:
                proof = json.loads(row['message'])
                if proof['version'] != 'rentemester-native-bank-profile/1' or proof['profile'] != 'danske-bank' or proof['bankAccountId'] != account_rows[0]['id'] or row['actor'] != 'user:' + report['results']['authenticate']['ownerId']:
                    raise RuntimeError('Current actor/account/profile import attestation differs')
                hashes.add(proof['sourceFileHash'])
            if hashes != {fixture['postedSha256'] for fixture in report['results']['setup']['fixtures'].values()}:
                raise RuntimeError('Both profile attestations must bind the exact signed source hashes')
            gaps = [row for row in result['exceptions'] if row['type'] == 'BANK_BALANCE_GAP']
            if len(gaps) != 1 or gaps[0]['status'] != 'open':
                raise RuntimeError('One exact retained gap exception, no duplicate exception')
            gap_source = json.loads(gaps[0]['source_evidence'])
            if gap_source['previousBalance'] != 1120 or gap_source['amount'] != -10 or gap_source['expectedBalance'] != 1110 or gap_source['statementBalance'] != 900 or gap_source['drift'] != -210:
                raise RuntimeError('Exact real statement discontinuity evidence differs')
            report.setdefault('snapshots', {})[name] = {'sha256': receipt['sha256'], 'trustedPublicKeySha256': digest(trusted), 'operatorCredentialsExcluded': True, 'tables': result}
            save()
            return result

        def preserved(original, newer):
            for table in ['bank_accounts', 'bank_transactions', 'accounts', 'companies', 'exceptions', 'journal_entries', 'journal_lines']:
                if original[table] != newer[table]:
                    raise RuntimeError('Restored exact immutable evidence differs: ' + table)
            retained = {row['id']: row for row in newer['audit_log']}
            if any(retained.get(row['id']) != row for row in original['audit_log']):
                raise RuntimeError('Original company audit must remain exact historical evidence')

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
                ['openssl', 'req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', str(work / 'ca.key'), '-out', str(ca), '-days', '1', '-subj', '/CN=Synthetic Bank Profile Auth CA', '-addext', 'basicConstraints=critical,CA:TRUE', '-addext', 'keyUsage=critical,keyCertSign,cRLSign'],
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
            config = {'runtimeProfile': 'local-user-macos', 'root': str(work / 'workspace'), 'brokerUid': os.getuid(), 'workspace': 'synthetic-native-bank-profile', 'companySlug': 'synthetic', 'companyName': 'Synthetic Bank Profile Company', 'cookieKey': key(), 'mfaKey': key(), 'operatorActors': ['system:synthetic-installer'], 'accounts': [], 'firstOwner': {'actor': 'system:synthetic-installer', 'email': 'owner@example.invalid', 'name': 'Synthetic Owner', 'password': 'synthetic-owner-password-123'}, 'backupSigningKeyFile': str(work / 'signing.der'), 'authMail': {'endpoint': f'https://127.0.0.1:{mail_port}/mail', 'bearerToken': bearer, 'from': 'auth@example.invalid', 'idempotencySecret': secrets.token_urlsafe(32), 'trustedCaFile': str(ca)}}
            write_config()
            start(create=True)
            script = work / 'browser.mjs'
            script.write_text(BROWSER)
            controller = subprocess.Popen(['/opt/homebrew/bin/bun', str(script)], cwd=REPO, env={**os.environ, 'RM_BANK_PROFILE_BASE': origin, 'RM_BANK_PROFILE_WORK': str(work), 'RM_BANK_PROFILE_SCREENSHOT': str(args.evidence.with_suffix('.png')), 'RM_BANK_PROFILE_PLAYWRIGHT': str((REPO / 'node_modules/@playwright/test/index.mjs').resolve(strict=True))}, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, bufsize=1, start_new_session=True)
            report['browserControllerPid'] = controller.pid
            phase('authenticate')
            phase('setup')
            phase('initial')
            initial_originals = original_inventory()
            if len(initial_originals) != 1:
                raise RuntimeError('One exact initial original required')
            phase('repeat-initial')
            if original_inventory() != initial_originals:
                raise RuntimeError('Repeat created an extra original')
            for kind in ['profile', 'date', 'fx', 'account', 'generic']:
                before = original_inventory()
                phase('negative-' + kind)
                if original_inventory() != before:
                    raise RuntimeError('Rejected import published orphan original: ' + kind)
            phase('unauthorized')
            if original_inventory() != initial_originals:
                raise RuntimeError('Unauthorized import published an original')
            phase('gap')
            gap_originals = original_inventory()
            if len(gap_originals) != 2:
                raise RuntimeError('Two accepted source originals required')
            phase('repeat-gap')
            if original_inventory() != gap_originals:
                raise RuntimeError('Repeated gap published another original')
            phase('clients')
            phase('archive-source')
            original = inspect_snapshot('source', work / 'trusted.pub')
            stop()
            config.pop('firstOwner')
            write_config()
            start()
            phase('reopen')
            phase('client-reads')
            phase('archive-restarted')
            if inspect_snapshot('restarted', work / 'trusted.pub') != original:
                raise RuntimeError('Restart/read changed bank evidence/audit')
            stop()
            source = work / 'workspace'
            if not owned() or source.is_symlink() or not source.is_dir():
                raise RuntimeError('Exact owned source required before source-free removal')
            shutil.rmtree(source)
            (work / 'signing.der').unlink()
            progress('operator:source-free-independent-verification')
            verified = subprocess.run([str(pinned['server_binary']), '--verify-backup', str(work / 'bank-profile-source.native-backup.json'), '--trusted-backup-key', str(work / 'trusted.pub')], cwd=REPO, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=60)
            if verified.returncode != 0:
                raise RuntimeError('Source-free independent trusted-key verification refused')
            subprocess.run(['/opt/homebrew/bin/bun', '-e', "const c=require('node:crypto'),f=require('node:fs'),k=c.generateKeyPairSync('ed25519'),s=k.privateKey.export({format:'der',type:'pkcs8'}).subarray(-32),p=k.publicKey.export({format:'der',type:'spki'}).subarray(-32);f.writeFileSync(process.argv[1],Buffer.concat([Buffer.from('3051020101300506032b657004220420','hex'),s,Buffer.from('812100','hex'),p]),{mode:0o600});f.writeFileSync(process.argv[2],p,{mode:0o600});", str(work / 'target-signing.der'), str(work / 'target-trusted.pub')], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
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
            report.update({'status': 'passed', 'currentAction': 'complete', 'bankProfileActualQualified': True, 'existingSourceNamedCliMcpQualified': True, 'initialAndRestartQualified': True, 'snapshotCoverage': 'signed-source-free-bank-profile-raw-originals-history-audit-restored-and-reopened'})
            save()
            print('PASS: ordinary MFA Danske profile UI, raw source fields, duplicate/refusal/gap and signed source-free recovery; ' + str(args.evidence))
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
