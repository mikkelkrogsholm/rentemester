#!/usr/bin/env python3
"""Owned normal-MFA Contacts import with a real synthetic verified-TLS CVR gateway.

Supplied server/web artifacts only. No SQL identity/business seeds, real provider
credentials, external mail, automatic replay or financial postings. Requires exact
repeat/cache/audit checks plus signed source-free fresh-owner restore and reopen.
Stale-cache expiration and reduced-role revocation remain unit-qualified only.
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
PREFIX = 'Rentemester-Synthetic-Contact-Import-'
BROWSER = r'''


import {createHmac,createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {createInterface} from 'node:readline';
import {readFileSync,readdirSync,writeFileSync,unlinkSync} from 'node:fs';
import {join} from 'node:path';
import {isDeepStrictEqual} from 'node:util';
const {chromium}=await import(process.env.RM_CONTACT_IMPORT_PLAYWRIGHT);
const base=process.env.RM_CONTACT_IMPORT_BASE,work=process.env.RM_CONTACT_IMPORT_WORK;
if(!/^http:\/\/127\.0\.0\.1:\d+$/.test(base))throw Error('owned literal loopback origin required');
const owner={email:'owner@example.invalid',password:'synthetic-owner-password-123',codes:[]};


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
async function authenticate(){browser=await chromium.launch({headless:true});ownerPage=await newPage();step='ordinary-owner-email-verification';await ownerPage.goto((await mail('verification',owner.email)).actionUrl);ownerId=(await enroll(ownerPage,owner)).user.id;return{ownerId,normalVerifiedMfa:true};}const prefix='/api/companies/synthetic/',hash=b=>createHash('sha256').update(b).digest('hex'),enrichLabel='Berig danske virksomheder med adresse m.m. fra CVR-registeret';
let sourceBooks,initialContacts,stable,imported,initialImport,cacheImport,missingImport,repeatImport;
const headers=['Kontaktnavn','Adresse','Postnummer','By','Landekode','CVR-nummer','EAN-nummer','Telefon','E-mail','Att. person','Hjemmeside','Betalingsfrist i dage','Total salg','Total køb','Kontakttype','Ekstra'];
const names={customer:'Synthetic CSV customer',eu:'Synthetic EU; "vendor"',both:'Synthetic both roles',null:'Synthetic null identifier',failed:'Synthetic failed provider',cached:'Synthetic cached new customer',missing:'Synthetic missing operator config'};
const cell=v=>'"'+String(v??'').replaceAll('"','""')+'"';
function csv(rows){return '\ufeff'+[headers,...rows.map(row=>headers.map(h=>row[h]??''))].map(row=>row.map(cell).join(';')).join('\r\n')+'\r\n';}
const row=(name,extra={})=>({'Kontaktnavn':name,'Total salg':'0','Total køb':'0','Kontakttype':'Source label deliberately unused',...extra});
const rows=[row(names.customer,{'Landekode':'dk','CVR-nummer':'12 34 56 78','EAN-nummer':'5790000000000','Betalingsfrist i dage':'730','Total salg':'1','Att. person':'CSV attention','Ekstra':'quoted unknown column\nsecond physical line'}),row(names.eu,{'Adresse':'EU Street; 9,  ','Postnummer':'10115','By':'Berlin','Landekode':'DE','CVR-nummer':'de 123 456 789','E-mail':'csv-eu@example.invalid','Telefon':'22222222','Hjemmeside':'https://csv-eu.example.invalid','Att. person':'Person "quoted"','Total køb':'1'}),row(names.both,{'Adresse':'CSV wins 5','Postnummer':'2000','By':'CSV City','Landekode':'DK','CVR-nummer':'DK87654321','E-mail':'csv-both@example.invalid','Telefon':'33333333','Hjemmeside':'https://csv-both.example.invalid','Total salg':'1','Total køb':'1'}),row(names.null),row(names.failed,{'Landekode':'DK','CVR-nummer':'11223344','Adresse':'Original CSV fallback 8','Total køb':'1'})];
const content=csv(rows),cachedContent=csv([row(names.cached,{'Landekode':'DK','CVR-nummer':'12345678','Total salg':'1'})]),missingContent=csv([row(names.missing,{'Landekode':'DK','CVR-nummer':'55667788','Adresse':'Missing config CSV 1','Total køb':'1'})]);
async function read(path){return get(ownerPage,path);}
async function contacts(){return read(prefix+'contacts');}
async function books(){return{journal:await read(prefix+'journal'),trialBalance:await read(prefix+'trial-balance'),balance:await read(prefix+'balance')};}
async function unchangedBooks(){need(isDeepStrictEqual(await books(),sourceBooks),'contact import and provider/cache reads never change journal/UUID/trial balance/balance');}
async function importUi(data,label){diagnosticPage=ownerPage;step=label+'-contacts-import-modal';await ownerPage.goto(base+'/companies/synthetic/kontakter');await ownerPage.getByRole('heading',{name:'Kontakter',exact:true}).waitFor();await ownerPage.getByRole('button',{name:'Importér',exact:true}).click();const d=ownerPage.getByRole('dialog',{name:'Importér fil',exact:true});need(await d.getByLabel(enrichLabel,{exact:true}).isChecked(),'optional source enrichment remains visibly default-on');await d.getByLabel('Fil',{exact:true}).setInputFiles({name:'Synthetic-Contacts.csv',mimeType:'text/csv',buffer:Buffer.from(data)});step=label+'-single-explicit-import';const response=ownerPage.waitForResponse(r=>new URL(r.url()).pathname===prefix+'import'&&r.request().method()==='POST');await d.getByRole('button',{name:'Importér',exact:true}).click();const r=await response;need(r.status()===200,'ordinary confirmed import HTTP200');const full=await r.json();need(full.ok===true&&full.import?.detected?.id==='dinero-contacts','actual source recognition preserved');await d.getByText('Genkendt som Dinero — Kontakter (kunder og leverandører).',{exact:true}).waitFor();return{d,result:full.import};}
async function closeImport(d){await d.getByRole('button',{name:'Luk',exact:true}).click();await d.waitFor({state:'hidden'});await unchangedBooks();}
function find(list,name){const value=list.find(r=>r.name===name);need(value,'canonical contact '+name);return value;}
function expectedFields(contact,expected){for(const[key,value]of Object.entries(expected))need(isDeepStrictEqual(contact[key],value),'exact retained source field '+contact.name+' '+key);}
async function verifyInitial(){const c=(await contacts()).contacts;need(c.customers.length===2&&c.vendors.length===4,'all source roles including both and no-activity vendor');expectedFields(find(c.customers,names.customer),{vatOrCvr:'DK12345678',address:'Registry Road 7, 1000 Synthetic City',email:'registry@example.invalid',phone:'11111111',website:'https://registry.example.invalid',eanNumber:'5790000000000',paymentTermsDays:730,notes:'Att.: CSV attention'});expectedFields(find(c.vendors,names.eu),{vatOrCvr:'DE123456789',address:'EU Street; 9, 10115 Berlin',email:'csv-eu@example.invalid',phone:'22222222',website:'https://csv-eu.example.invalid',notes:'Att.: Person "quoted"'});for(const list of[c.customers,c.vendors])expectedFields(find(list,names.both),{vatOrCvr:'DK87654321',address:'CSV wins 5, 2000 CSV City',email:'csv-both@example.invalid',phone:'33333333',website:'https://csv-both.example.invalid'});expectedFields(find(c.vendors,names.null),{vatOrCvr:null,address:null,email:null,phone:null,website:null});expectedFields(find(c.vendors,names.failed),{vatOrCvr:'DK11223344',address:'Original CSV fallback 8',email:null});await unchangedBooks();return c;}
async function intake(){step='source-empty-canonical-contacts-and-books';sourceBooks=await books();need(sourceBooks.journal.journal.entries.length===0&&/^[0-9a-f]{32}$/.test(sourceBooks.journal.journal.ledgerUuid),'empty real owned ledger and stable UUID');initialContacts=await contacts();need(initialContacts.contacts.customers.length===0&&initialContacts.contacts.vendors.length===0,'fixture starts with no contact seeds');const {d,result}=await importUi(content,'primary-file');initialImport=result;need(isDeepStrictEqual(result.summary,{parsed:5,customersCreated:2,vendorsCreated:4,skipped:0,enriched:2,enrichmentFailures:1}),'source role/enrichment/failure counters');need(result.errors.length>=1&&result.errors.some(e=>e.includes('CVR-berigelse')),'provider503 continues CSV import with truthful warning');await d.getByText('2 beriget med CVR-data',{exact:false}).waitFor();await d.getByText('CVR-berigelse mislykkedes i 1 forsøg for kunde-/leverandørroller. Samme kontakt kan tælle to gange.',{exact:true}).waitFor();await closeImport(d);imported=await verifyInitial();return{summary:result.summary,errors:result.errors,contacts:imported,sourceBooks,quotedEmbeddedNewlineInUnknownColumn:true};}
async function repeat(){step='exact-repeat-natural-key-skip-no-updates';const before=await contacts();const{d,result}=await importUi(content,'exact-repeat');repeatImport=result;need(isDeepStrictEqual(result.summary,{parsed:5,customersCreated:0,vendorsCreated:0,skipped:6,enriched:0,enrichmentFailures:0})&&result.errors.length===0,'repeat exact source counters without enrichment attempts');await closeImport(d);need(isDeepStrictEqual(await contacts(),before),'case-sensitive null-aware natural keys retain exact existing rows');return{summary:result.summary,noNewContacts:true};}
async function client(kind){const cookieFile=join(work,'client.cookie');writeFileSync(cookieFile,(await ownerPage.context().cookies(base)).map(c=>`${c.name}=${c.value}`).join('; '),{mode:0o600,flag:'wx'});try{const common=['--server',base,'--cookie-file',cookieFile,'--allow-loopback-http'],body=JSON.stringify({fileName:'Synthetic-Contacts.csv',content,enrichCvr:true,confirm:true});let argv,input;if(kind==='cli'){argv=['remote','import-contacts',...common,'--company','synthetic','--confirm'];input=body;}else{argv=common;input=[{jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'synthetic-contact-import-proof',version:'1'}}},{jsonrpc:'2.0',method:'notifications/initialized'},{jsonrpc:'2.0',id:2,method:'tools/call',params:{name:'import_contacts',arguments:{company:'synthetic',body,confirm:true}}}].map(v=>JSON.stringify(v)).join('\n')+'\n';}const result=spawnSync(join(work,kind==='cli'?'rentemester-rust':'rentemester-mcp'),argv,{input,encoding:'utf8',timeout:45000,maxBuffer:16*1024*1024});need(!result.error&&result.status===0,kind+' explicit canonical import terminal');const full=kind==='cli'?JSON.parse(result.stdout):result.stdout.trim().split('\n').map(line=>JSON.parse(line)).find(v=>v.id===2)?.result?.structuredContent;need(full?.ok===true&&isDeepStrictEqual(full.data,{ok:true,import:repeatImport}),'exact HTTP CLI MCP canonical repeated-import parity');return full.data;}finally{unlinkSync(cookieFile);}}
async function clients(){step='explicit-cli-and-mcp-confirmed-known-repeat-no-automatic-retry';const before=await contacts();const responses={};for(const kind of['cli','mcp']){responses[kind]=await client(kind);need(isDeepStrictEqual(await contacts(),before),'each explicit native client repeat preserves exact contact rows');await unchangedBooks();}return{responses,httpCliMcpCanonicalParity:true,noNewContacts:true};}
async function freshCache(){const {d,result}=await importUi(cachedContent,'fresh-cache-new-name');cacheImport=result;need(isDeepStrictEqual(result.summary,{parsed:1,customersCreated:1,vendorsCreated:0,skipped:0,enriched:1,enrichmentFailures:0})&&result.errors.length===0,'new distinct contact uses retained fresh CVR success');await closeImport(d);expectedFields(find((await contacts()).contacts.customers,names.cached),{vatOrCvr:'DK12345678',address:'Registry Road 7, 1000 Synthetic City',email:'registry@example.invalid',phone:'11111111',website:'https://registry.example.invalid'});return{summary:result.summary,freshCacheSuccess:true};}
async function invalid(){step='truthful-partial-row-refusal-and-malformed-no-writes';const before=await contacts();const invalidContent=csv([row('Synthetic invalid activity',{'Total salg':'1,25'})]);const {d,result}=await importUi(invalidContent,'invalid-nonblank-activity');need(result.summary.customersCreated===0&&result.summary.vendorsCreated===0&&result.errors.length>0,'invalid nonblank total row is reported and never silently vendor');await closeImport(d);for(const invalid of['"Kontaktnavn";"CVR-nummer";"Kontakttype"\n"unterminated','Kontaktnavn;CVR-nummer;Kontakttype;Kontaktnavn\nA;;B;C']){const r=await ownerPage.request.post(base+prefix+'import',{headers:{origin:base},data:{fileName:'invalid.csv',content:invalid,enrichCvr:true,confirm:true}});need(r.status()===400&&(await r.json()).ok===false,'malformed or duplicate recognized headers refuse before writes');}need(isDeepStrictEqual(await contacts(),before),'all row/parser refusals leave canonical contact rows unchanged');await unchangedBooks();return{partialRowErrors:result.errors,malformedAndDuplicateHeadersRefused:true};}
async function reopen(){step='operator-without-cvr-config-normal-password-mfa-restart';await ownerPage.context().clearCookies();need((await login(ownerPage,owner)).user.id===ownerId,'same verified MFA owner');await unchangedBooks();await verifyRetained();return{freshMfaRestart:true,originalFieldsRetained:true};}
async function verifyRetained(){const c=(await contacts()).contacts;need(c.customers.length===3&&c.vendors.length===4,'original and cached-source contact counts persist');for(const item of imported.customers)need(isDeepStrictEqual(find(c.customers,item.name),item),'original customer unchanged');for(const item of imported.vendors)need(isDeepStrictEqual(find(c.vendors,item.name),item),'original vendor unchanged');return c;}
async function missingConfig(){const{d,result}=await importUi(missingContent,'missing-operator-cvr-config');missingImport=result;need(isDeepStrictEqual(result.summary,{parsed:1,customersCreated:0,vendorsCreated:1,skipped:0,enriched:0,enrichmentFailures:1})&&result.errors.some(e=>e.includes('CVR_USERNAME')),'actual missing configuration preserves CSV and truthful setup guidance');await d.getByText('Adgang til CVR-registeret er ikke konfigureret for de mislykkede opslag.',{exact:false}).waitFor();await closeImport(d);expectedFields(find((await contacts()).contacts.vendors,names.missing),{vatOrCvr:'DK55667788',address:'Missing config CSV 1',email:null});stable={contacts:await contacts(),books:await books()};need(stable.contacts.contacts.customers.length===3&&stable.contacts.contacts.vendors.length===5,'all seven imported contacts retain eight explicit role rows');await ownerPage.screenshot({path:process.env.RM_CONTACT_IMPORT_SCREENSHOT,fullPage:true});return{summary:result.summary,errors:result.errors,stable,missingConfigActualQualified:true};}
async function assertStable(){need(isDeepStrictEqual(await contacts(),stable.contacts)&&isDeepStrictEqual(await books(),stable.books),'complete canonical contacts/financial fields/UUID survive source-free restore and reopen');}
async function archive(name){step='explicit-signed-'+name+'-snapshot';const r=await ownerPage.request.post(base+'/api/workspace/backup',{headers:{origin:base},data:{confirm:true}});need(r.status()===200&&r.headers()['content-type']==='application/vnd.rentemester.native-backup+json','signed imported contacts/cache coverage required');const bytes=await r.body();need(bytes.length>0&&bytes.length<=96*1024*1024,'bounded exact signed bytes');writeFileSync(join(work,'contact-import-'+name+'.native-backup.json'),bytes,{mode:0o600,flag:'wx'});await unchangedBooks();return{supported:true,sha256:hash(bytes),bytes:bytes.length,name};}
let freshOwner;
async function restored(){step='source-free-fresh-owner-normal-email-verification-mfa';ownerPage=await newPage();freshOwner={email:'restored@example.invalid',password:'synthetic-restored-password-789',codes:[]};await ownerPage.goto((await mail('verification',freshOwner.email)).actionUrl);const me=await enroll(ownerPage,freshOwner);need(me.user.id!==ownerId,'new restored owner current authority, historical import actor unchanged');await assertStable();return{sourceFreeRestoreQualified:true,freshOwnerMfa:true,noImportedCredentials:true};}
async function restoredRepeat(){const before=await contacts();const{d,result}=await importUi(content,'restored-exact-repeat-without-provider-config');need(result.summary.customersCreated===0&&result.summary.vendorsCreated===0&&result.summary.skipped===6&&result.summary.enriched===0&&result.summary.enrichmentFailures===0&&result.errors.length===0,'restored natural keys remain skips without provider');await closeImport(d);need(isDeepStrictEqual(await contacts(),before),'restored repeat preserves all rows');await assertStable();return{summary:result.summary,noNewContacts:true};}
async function restoredReopen(){step='restored-target-reopen-normal-password-mfa';await ownerPage.context().clearCookies();await login(ownerPage,freshOwner);await assertStable();return{restoredTargetReopenQualified:true,exactContactsAndUuidsRetained:true};}
for await(const line of createInterface({input:process.stdin,crlfDelay:Infinity})){try{const action=JSON.parse(line);let value;if(action==='authenticate')value=await authenticate();else if(action==='intake')value=await intake();else if(action==='repeat')value=await repeat();else if(action==='clients')value=await clients();else if(action==='cache')value=await freshCache();else if(action==='invalid')value=await invalid();else if(action==='reopen')value=await reopen();else if(action==='missing-config')value=await missingConfig();else if(['archive-initial','archive-repeat','archive-clients','archive-final','archive-restored','archive-restored-repeat'].includes(action))value=await archive(action.slice(8));else if(action==='restored')value=await restored();else if(action==='restored-repeat')value=await restoredRepeat();else if(action==='restored-reopen')value=await restoredReopen();else if(action==='finish'){await browser?.close();emit({ok:true,value:{finished:true}});break;}else throw Error('unknown phase');emit({ok:true,value});}catch(error){let pageState=null;try{pageState=await Promise.race([diagnosticPage?.evaluate(()=>({pathname:location.pathname,headings:[...document.querySelectorAll('h1,h2')].map(e=>e.textContent.slice(0,100)),buttons:[...document.querySelectorAll('button')].map(e=>e.textContent.slice(0,100)),alerts:document.querySelectorAll('[role=alert]').length})),new Promise(resolve=>setTimeout(()=>resolve(null),2000))]);}catch{}emit({ok:false,step,detail,errorType:error?.name,trace,pageState});await Promise.race([browser?.close().catch(()=>{}),new Promise(resolve=>setTimeout(resolve,2000))]);process.exitCode=1;break;}}

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
    provider_requests = []
    cvr_username, cvr_password = secrets.token_urlsafe(24), secrets.token_urlsafe(32)
    cvr_basic = base64.b64encode((cvr_username + ':' + cvr_password).encode()).decode()
    cvr_configured = True
    gateway_lock = threading.Lock()
    bearer = secrets.token_urlsafe(32)
    backend_port, mail_port = free_port(), free_port()
    if backend_port == mail_port:
        raise RuntimeError('Distinct owned ports required')
    origin = f'http://127.0.0.1:{backend_port}'
    report = {'status': 'running', 'sourceRevision': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=REPO, text=True).strip(), 'runnerSha256': digest(Path(__file__)), 'browserHelperSha256': hashlib.sha256(BROWSER.encode()).hexdigest(), 'canonicalOrigin': origin, 'noOfflineIdentitySeeds': True, 'ordinaryMacOsUser': True, 'ownedProfile': str(work), 'externalMailDelivered': False, 'workflow': 'contact_csv_import_with_verified_tls_cvr', 'staleFallbackActualQualified': False, 'roleRevocationAndErasureActualQualified': False, 'cliMcpImportActualQualified': False, 'installedNativeQualified': False, 'screenshot': str(args.evidence.with_suffix('.png')),  'actions': [], 'results': {}, 'sourceFreeRestoreQualified': False}
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
                if self.path == '/virksomhed/_search':
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
                    if not 0 < length <= 32768 or self.headers.get('authorization') != 'Basic ' + cvr_basic:
                        raise ValueError('Private provider authority')
                    query = json.loads(self.rfile.read(length))
                    if set(query) != {'from', 'size', '_source', 'query'} or query['from'] != 0 or query['size'] != 1 or not isinstance(query['_source'], list):
                        raise ValueError('Fixed provider query shape')
                    cvr = str(query['query']['term']['Vrvirksomhed.cvrNummer'])
                    if cvr not in ('12345678', '87654321', '11223344'):
                        raise ValueError('Unexpected synthetic provider identity')
                    status = 503 if cvr == '11223344' else 200
                    with gateway_lock:
                        provider_requests.append({'cvr': cvr, 'status': status})
                    temporal = {'gyldigFra': '2000-01-01', 'gyldigTil': None}
                    contact = lambda value: [{'kontaktoplysning': 'hidden synthetic value', 'hemmelig': True, 'periode': temporal}, {'kontaktoplysning': value, 'hemmelig': False, 'periode': temporal}]
                    entity = {'cvrNummer': int(cvr), 'navne': [{'navn': 'Former registry name', 'periode': {'gyldigTil': '1999-12-31'}}, {'navn': 'Provider must not overwrite CSV name', 'periode': temporal}], 'beliggenhedsadresse': [{'vejnavn': 'Former Road', 'husnummerFra': 1, 'periode': {'gyldigTil': '1999-12-31'}}, {'vejnavn': 'Registry Road', 'husnummerFra': 7, 'postnummer': 1000, 'postdistrikt': 'Synthetic City', 'periode': temporal}], 'telefonNummer': contact('11111111'), 'elektroniskPost': contact('registry@example.invalid'), 'hjemmeside': contact('https://registry.example.invalid'), 'reklamebeskyttet': False}
                    payload = {'hits': {'hits': [{'_source': {'Vrvirksomhed': entity}}]}} if status == 200 else {'error': 'Synthetic known provider unavailability'}
                    data = json.dumps(payload).encode()
                    self.send_response(status)
                    self.send_header('content-type', 'application/json')
                    self.send_header('content-length', str(len(data)))
                    self.end_headers()
                    self.wfile.write(data)
                except (BrokenPipeError, ConnectionResetError):
                    pass
                except Exception:
                    errors.append('private-cvr-gateway-contract-refusal')
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
                argv += ['--restore-backup', str(work / 'contact-import-final.native-backup.json'), '--trusted-backup-key', str(work / 'trusted.pub'), '--confirm-target', str(root), '--source-unavailable']
            log = (work / 'server.log').open('ab')
            provider_names = {'CVR_USERNAME', 'CVR_PASSWORD', 'RENTEMESTER_CVR_ENDPOINT', 'RENTEMESTER_CVR_TRUSTED_CA_FILE'}
            environment = {name: value for name, value in os.environ.items() if name not in provider_names}
            if cvr_configured:
                environment.update({'CVR_USERNAME': cvr_username, 'CVR_PASSWORD': cvr_password, 'RENTEMESTER_CVR_ENDPOINT': f'https://127.0.0.1:{mail_port}', 'RENTEMESTER_CVR_TRUSTED_CA_FILE': str(work / 'ca.pem')})
            child = subprocess.Popen(argv, cwd=REPO, env=environment, stdin=subprocess.DEVNULL, stdout=log, stderr=log)
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
                report['providerRequestTrace'] = list(provider_requests)
                report['privateGatewayErrors'] = list(errors)
            report['actions'].append({'action': action, 'status': 'complete'})
            report['results'][action] = value['value']
            save()
            return value['value']

        def provider_count():
            with gateway_lock:
                return len(provider_requests)

        def inspect_snapshot(name, trusted):
            progress('operator:verify-and-read-own-signed-snapshot-' + name)
            path = work / ('contact-import-' + name + '.native-backup.json')
            receipt = report['results']['archive-' + name]
            if digest(path) != receipt['sha256']:
                raise RuntimeError('Signed bytes changed after download')
            verified = subprocess.run([str(pinned['server_binary']), '--verify-backup', str(path), '--trusted-backup-key', str(trusted)], cwd=REPO, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=60)
            if verified.returncode != 0:
                raise RuntimeError('Independent signature/inventory verification refused')
            wire = json.loads(path.read_bytes())
            entries = wire['entries']
            secrets_to_exclude = [cvr_username, cvr_password, 'Basic ' + cvr_basic, bearer, config['authMail']['idempotencySecret'], config['cookieKey'], config['mfaKey']]
            payloads = [base64.b64decode(entry['bytesBase64'], validate=True) for entry in entries]
            payloads.append(base64.b64decode(wire['manifestBase64'], validate=True))
            for decoded in payloads:
                if any(secret.encode() in decoded for secret in secrets_to_exclude):
                    raise RuntimeError('Private operator credential found in signed payload')
            ledger = [entry for entry in entries if entry['path'] == 'synthetic/data/ledger.sqlite']
            if len(ledger) != 1:
                raise RuntimeError('Exact signed owned ledger entry required')
            decoded = base64.b64decode(ledger[0]['bytesBase64'], validate=True)
            if not 0 < len(decoded) <= 32 * 1024 * 1024:
                raise RuntimeError('Signed company SQLite snapshot bound')
            copy = work / ('readonly-snapshot-' + name + '.sqlite')
            try:
                with copy.open('xb') as output:
                    output.write(decoded)
                # This is the downloaded, verified synthetic snapshot. Never a
                # live database, fixture seed or authority/configuration source.
                db = sqlite3.connect(copy.as_uri() + '?mode=ro', uri=True)
                try:
                    db.execute('PRAGMA query_only=ON')
                    result = {}
                    for table, order in [('customers', 'id'), ('vendors', 'id'), ('cvr_lookups', 'cvr'), ('audit_log', 'id')]:
                        cursor = db.execute(f'SELECT * FROM {table} ORDER BY {order}')
                        columns = [column[0] for column in cursor.description]
                        result[table] = [dict(zip(columns, row)) for row in cursor.fetchall()]
                finally:
                    db.close()
            finally:
                copy.unlink(missing_ok=True)
            if [row['cvr'] for row in result['cvr_lookups']] != ['12345678', '87654321'] or any(not row['raw_response'] or not row['fetched_at'] or not row['expires_at'] for row in result['cvr_lookups']):
                raise RuntimeError('Exact successful provider cache/provenance missing or invented failure cache')
            report.setdefault('snapshots', {})[name] = {'sha256': receipt['sha256'], 'bytes': receipt['bytes'], 'trustedPublicKeySha256': digest(trusted), 'operatorCredentialsExcluded': True, 'tables': result}
            save()
            return result

        def preserved(original, newer):
            for table in ['customers', 'vendors', 'cvr_lookups']:
                if newer[table] != original[table]:
                    raise RuntimeError('Restored exact source contacts/cache changed: ' + table)
            # A real restore boundary or new-owner admission may append audit;
            # every original import row/actor/payload must remain byte-for-byte.
            retained = {row['id']: row for row in newer['audit_log']}
            if any(retained.get(row['id']) != row for row in original['audit_log']):
                raise RuntimeError('Original import audit/history changed after restore')

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
                ['openssl', 'req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', str(work / 'ca.key'), '-out', str(ca), '-days', '1', '-subj', '/CN=Synthetic Contact Import Auth CA', '-addext', 'basicConstraints=critical,CA:TRUE', '-addext', 'keyUsage=critical,keyCertSign,cRLSign'],
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
            config = {'runtimeProfile': 'local-user-macos', 'root': str(work / 'workspace'), 'brokerUid': os.getuid(), 'workspace': 'synthetic-native-contact-import', 'companySlug': 'synthetic', 'companyName': 'Synthetic Contact Import Company', 'cookieKey': key(), 'mfaKey': key(), 'operatorActors': ['system:synthetic-installer'], 'accounts': [], 'firstOwner': {'actor': 'system:synthetic-installer', 'email': 'owner@example.invalid', 'name': 'Synthetic Owner', 'password': 'synthetic-owner-password-123'}, 'backupSigningKeyFile': str(work / 'signing.der'), 'authMail': {'endpoint': f'https://127.0.0.1:{mail_port}/mail', 'bearerToken': bearer, 'from': 'auth@example.invalid', 'idempotencySecret': secrets.token_urlsafe(32), 'trustedCaFile': str(ca)}}
            write_config()
            start(create=True)
            script = work / 'browser.mjs'
            script.write_text(BROWSER)
            controller = subprocess.Popen(['/opt/homebrew/bin/bun', str(script)], cwd=REPO, env={**{name: value for name, value in os.environ.items() if name not in {'CVR_USERNAME', 'CVR_PASSWORD', 'RENTEMESTER_CVR_ENDPOINT', 'RENTEMESTER_CVR_TRUSTED_CA_FILE'}}, 'RM_CONTACT_IMPORT_BASE': origin, 'RM_CONTACT_IMPORT_WORK': str(work), 'RM_CONTACT_IMPORT_SCREENSHOT': str(args.evidence.with_suffix('.png')), 'RM_CONTACT_IMPORT_PLAYWRIGHT': str((REPO / 'node_modules/@playwright/test/index.mjs').resolve(strict=True))}, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, bufsize=1, start_new_session=True)
            report['browserControllerPid'] = controller.pid
            phase('authenticate')
            phase('intake')
            with gateway_lock:
                if sorted(provider_requests, key=lambda row: row['cvr']) != [{'cvr': '11223344', 'status': 503}, {'cvr': '12345678', 'status': 200}, {'cvr': '87654321', 'status': 200}]:
                    raise RuntimeError('Exact real unique provider successes/failure requests required')
            phase('archive-initial')
            original = inspect_snapshot('initial', work / 'trusted.pub')
            requests_after_initial = provider_count()
            phase('repeat')
            phase('archive-repeat')
            repeated = inspect_snapshot('repeat', work / 'trusted.pub')
            if repeated != original or provider_count() != requests_after_initial:
                raise RuntimeError('Exact repeat changed contacts/cache/audit or called provider')
            phase('clients')
            phase('archive-clients')
            if inspect_snapshot('clients', work / 'trusted.pub') != original or provider_count() != requests_after_initial:
                raise RuntimeError('Explicit client repeat changed contacts/cache/audit or called provider')
            phase('cache')
            phase('invalid')
            if provider_count() != requests_after_initial:
                raise RuntimeError('Fresh cache or refused rows called provider')
            stop()
            cvr_configured = False
            config.pop('firstOwner')
            write_config()
            start()
            phase('reopen')
            phase('missing-config')
            if provider_count() != requests_after_initial:
                raise RuntimeError('Missing config unexpectedly contacted provider')
            phase('archive-final')
            final = inspect_snapshot('final', work / 'trusted.pub')
            if final['cvr_lookups'] != original['cvr_lookups']:
                raise RuntimeError('Fresh-cache/failure/missing-config changed original cache facts')
            stop()
            source = work / 'workspace'
            if not owned() or source.is_symlink() or not source.is_dir():
                raise RuntimeError('Exact owned source required before source-free removal')
            shutil.rmtree(source)
            (work / 'signing.der').unlink()
            progress('operator:source-free-independent-final-verification')
            verified = subprocess.run([str(pinned['server_binary']), '--verify-backup', str(work / 'contact-import-final.native-backup.json'), '--trusted-backup-key', str(work / 'trusted.pub')], cwd=REPO, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=60)
            if verified.returncode != 0:
                raise RuntimeError('Source-free separate trusted-key verification refused')
            # Fresh operator provisioning: this target key is outside the restored
            # workspace and is never imported from the original archive.
            subprocess.run(['/opt/homebrew/bin/bun', '-e', "const c=require('node:crypto'),f=require('node:fs'),k=c.generateKeyPairSync('ed25519'),s=k.privateKey.export({format:'der',type:'pkcs8'}).subarray(-32),p=k.publicKey.export({format:'der',type:'spki'}).subarray(-32);f.writeFileSync(process.argv[1],Buffer.concat([Buffer.from('3051020101300506032b657004220420','hex'),s,Buffer.from('812100','hex'),p]),{mode:0o600});f.writeFileSync(process.argv[2],p,{mode:0o600});", str(work / 'target-signing.der'), str(work / 'target-trusted.pub')], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            if digest(work / 'target-trusted.pub') == digest(work / 'trusted.pub'):
                raise RuntimeError('Independent target signer required')
            config.update({'root': str(work / 'restored'), 'backupSigningKeyFile': str(work / 'target-signing.der'), 'cookieKey': key(), 'mfaKey': key(), 'restoreActor': 'system:synthetic-installer', 'firstOwner': {'actor': 'system:synthetic-installer', 'email': 'restored@example.invalid', 'name': 'Synthetic Restored Owner', 'password': 'synthetic-restored-password-789'}})
            write_config()
            start(restore=True)
            restored = phase('restored')
            report['sourceFreeRestoreQualified'] = restored['sourceFreeRestoreQualified']
            report['sourceWorkspaceAndPrivateSignerRemoved'] = not source.exists() and not (work / 'signing.der').exists()
            phase('archive-restored')
            restored_rows = inspect_snapshot('restored', work / 'target-trusted.pub')
            preserved(final, restored_rows)
            phase('restored-repeat')
            phase('archive-restored-repeat')
            if inspect_snapshot('restored-repeat', work / 'target-trusted.pub') != restored_rows or provider_count() != requests_after_initial:
                raise RuntimeError('Restored exact repeat changed audit/contacts/cache or called provider')
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
                report['provider'] = {'verifiedTls': True, 'requests': list(provider_requests), 'freshCacheAndRepeatNoExtraRequests': True, 'configuredOnlyThroughPrivateOperatorEnvironment': True}
                report['gateway'] = {'https': True, 'trustedCaSha256': digest(ca), 'acceptedMessages': len(gateway_rows), 'requests': list(gateway_requests)}
            report.update({'status': 'passed', 'currentAction': 'complete', 'normalMfaContactImportQualified': True, 'cliMcpImportActualQualified': True, 'initialAndRestartQualified': True, 'snapshotCoverage': 'signed-source-free-cache-contacts-audit-restored-and-reopened'})
            save()
            print('PASS: ordinary MFA contact CSV import, real synthetic trusted TLS provider/cache/failure, repeat and source-free restore; ' + str(args.evidence))
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
