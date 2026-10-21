#!/usr/bin/env python3
"""Owned normal-MFA required document scanner qualification using supplied artifacts.

Verified private TLS, exact original hashes, current revocation during held scan,
existing web/CLI/MCP upload, signed source-free recovery without scanner credentials.
Only ordinary ingestion is scanned; existing inbox completion remains scanner-off.
No SQL/auth/business seeds, real mail/provider or automatic retries. The default
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
let servicePurchase;
async function servicePhase(action){
 if(!servicePurchase){const module=await import(process.env.RM_SERVICE_PURCHASE_HELPER);const factory=module[process.env.RM_SCANNER_PROOF_FACTORY||'createServicePurchaseProof'];need(typeof factory==='function','configured proof factory required');servicePurchase=await factory(ownerPage,base,work,memberPage);}
 if(action==='service-reopen'){await ownerPage.context().clearCookies();await login(ownerPage,owner);return servicePurchase.reopen();}
 return servicePurchase[{'service-run':'run','service-hold-begin':'holdBegin','service-revoke':'revoke','service-hold-finish':'holdFinish'}[action]]();
}
for await(const line of createInterface({input:process.stdin,crlfDelay:Infinity})){try{const action=JSON.parse(line);let value;if(action.startsWith('service-'))value=await servicePhase(action);else if(action==='authenticate')value=await authenticate();else if(action==='historical')value=await historical();else if(action==='clean')value=await clean();else if(action==='negatives')value=await negatives();else if(action==='unauthorized')value=await unauthorized();else if(action==='hold-begin')value=await holdBegin();else if(action==='revoke')value=await revoke();else if(action==='hold-finish')value=await holdFinish();else if(action==='clients')value=await clients();else if(action==='reopen')value=await reopen();else if(['archive-source','archive-restarted','archive-restored'].includes(action))value=await archive(action.slice(8));else if(action==='restored')value=await restored();else if(action==='restored-reopen')value=await restoredReopen();else if(action==='finish'){await browser?.close();emit({ok:true,value:{finished:true}});break;}else throw Error('unknown phase');emit({ok:true,value});}catch(error){let pageState=null;try{pageState=await Promise.race([diagnosticPage?.evaluate(()=>({pathname:location.pathname,headings:[...document.querySelectorAll('h1,h2,h3,h4')].map(e=>e.textContent.slice(0,100)),buttons:[...document.querySelectorAll('button')].map(e=>e.textContent.slice(0,100)),alerts:[...document.querySelectorAll('[role=alert]')].map(e=>e.textContent.slice(0,300))})),new Promise(resolve=>setTimeout(()=>resolve(null),2000))]);}catch{}emit({ok:false,step,detail,errorType:error?.name,trace,pageState});await Promise.race([browser?.close().catch(()=>{}),new Promise(resolve=>setTimeout(resolve,2000))]);process.exitCode=1;break;}}

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
    args = parser.parse_args()
    if args.remote_backup and not args.backup_lock:
        parser.error('--remote-backup requires --backup-lock')
    if args.backup_lock and (args.service_purchase or args.backup_destinations):
        parser.error('--backup-lock is mutually exclusive with --service-purchase/--backup-destinations')
    if args.backup_destinations and not args.service_purchase:
        parser.error('--backup-destinations requires --service-purchase')
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
            return work.parent == parent and work.name.startswith(PREFIX) and (stat.st_dev, stat.st_ino, stat.st_uid) == (*identity, os.getuid())

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
            if create:
                argv.append('--create')
            if restore:
                root = Path(config['root'])
                if os.path.lexists(root):
                    raise RuntimeError('Fresh restore only; refuse overwrite')
                argv += ['--restore-backup', str(work / 'scanner-source.native-backup.json'), '--trusted-backup-key', str(work / 'trusted.pub'), '--confirm-target', str(root), '--source-unavailable']
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
            if not args.service_purchase and not args.backup_lock:
                invalid_config_checks()
            write_config()
            start(create=True)
            script = work / 'browser.mjs'
            script.write_text(BROWSER)
            controller = subprocess.Popen(['/opt/homebrew/bin/bun', str(script)], cwd=REPO, env={**os.environ, 'RM_NATIVE_BACKUP_DESTINATION_PROOF':'1' if args.backup_destinations else '0', 'RM_NATIVE_REMOTE_BACKUP_PROOF':'1' if args.remote_backup else '0', 'RM_SERVICE_PURCHASE_HELPER': str(REPO / ('scripts/rust/check-native-backup-lock.ts' if args.backup_lock else 'scripts/rust/check-native-service-purchase.ts')), 'RM_SCANNER_PROOF_FACTORY': 'createBackupLockProof' if args.backup_lock else 'createServicePurchaseProof', 'RM_SCANNER_BASE': origin, 'RM_SCANNER_WORK': str(work), 'RM_SCANNER_SCREENSHOT': str(args.evidence.with_suffix('.png')), 'RM_SCANNER_PLAYWRIGHT': str((REPO / 'node_modules/@playwright/test/index.mjs').resolve(strict=True))}, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, bufsize=1, start_new_session=True)
            report['browserControllerPid'] = controller.pid
            phase('authenticate')
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
                helper_hashes = {str(path): digest(path) for path in proof_helpers}
                phase('service-run')
                if any(digest(path) != helper_hashes[str(path)] for path in proof_helpers):
                    raise RuntimeError('Backup proof imported helper changed during execution')
                stop(); config.pop('firstOwner')
                write_config(); start()
                phase('service-reopen')
                if any(digest(path) != helper_hashes[str(path)] for path in proof_helpers):
                    raise RuntimeError('Backup proof imported helper changed during restart verification')
                phase('finish', 20)
                controller.stdin.close(); controller.wait(timeout=10)
                if controller.returncode != 0:
                    raise RuntimeError('Backup lock controller nonzero terminal')
                controller = None; stop()
                db = sqlite3.connect((work/'workspace/synthetic/data/ledger.sqlite').as_uri()+'?mode=ro', uri=True)
                try:
                    counts = {table: db.execute('SELECT count(*) FROM '+table).fetchone()[0] for table in ['documents','bank_transactions','journal_entries']}
                    if counts != {'documents':0,'bank_transactions':1,'journal_entries':1}:
                        raise RuntimeError('Backup lock terminal financial effects differ')
                    report['terminalSql'] = {'counts':counts,'journalActors':db.execute('SELECT created_by,created_by_program FROM journal_entries ORDER BY id').fetchall()}
                finally: db.close()
                target_db = sqlite3.connect((work/'workspace/synthetic-backup-inbox-target/data/ledger.sqlite').as_uri()+'?mode=ro', uri=True)
                try:
                    target_counts = {table: target_db.execute('SELECT count(*) FROM '+table).fetchone()[0] for table in ['documents','bank_transactions','journal_entries']}
                    if target_counts != {'documents':1,'bank_transactions':1,'journal_entries':0}:
                        raise RuntimeError('Backup inbox target terminal effects differ')
                    report['targetTerminalSql'] = {'company':'synthetic-backup-inbox-target','counts':target_counts}
                finally: target_db.close()
                with gateway_lock:
                    if errors or provider_requests:
                        raise RuntimeError('Backup lock proof must not call scanner/provider')
                report.update(status='passed',currentAction='complete',financialBackupLockQualified=True,normalMfaRestartQualified=True,currentFileMissingAndTamperQualified=True,directControlMetadataGateQualified=True,directControlMetadataScope=['company-knowledge','corporate-record-ingest'],metadataRestartQualified=True,twoCompanyInboxTargetGateQualified=True,humanMailAliasGateQualified=True,multiCompanyMetadataGateQualified=False,partiesGateQualified=False,mailIntakeConfigurationGateQualified=False,smtpDispatchGateQualified=False,keyRotationQualified=False)
                if args.remote_backup:
                    expected_drive = [{'objectId':obj,'kind':kind} for obj,kinds in [('synthetic_good',['metadata','media']),('synthetic_wrong_parent',['metadata']),('synthetic_wrong_hash',['metadata','media']),('synthetic_oversized',['metadata','media']),('synthetic_missing',['metadata']),('synthetic_held',['metadata','media']),('synthetic_good',['metadata','media']),('synthetic_good',['metadata','media'])] for kind in kinds]
                    if drive_requests != expected_drive:
                        raise RuntimeError('Drive GET trace differs: skipped work or unexpected retries')
                    governance = report['results']['service-run']['remoteBackup']['governance']
                    reopened_governance = report['results']['service-reopen']['remoteBackupReopen']['governance']
                    if not governance.get('qualified') or not reopened_governance.get('sameNewLatestAfterNormalMfaRestart') or not reopened_governance.get('historicalEvidenceNotBorrowed'):
                        raise RuntimeError('Current exact-archive governance or restart parity not qualified')
                    report.update(backupGovernanceQualified=True,remoteBackupVerificationQualified=True,driveGetTrace=list(drive_requests),fixtureCopyByHarness=True,actualGoogleAccountQualified=False,offsiteQualified=False,installedNativeQualified=False)
                save()
                print('PASS: normal MFA financial backup lock, human/service HTTP/CLI/MCP refusal, metadata refusal/recovery, two-company inbox/alias gate and exact handoff, UI verified artifact, missing/tamper lock, single posting and restart; '+str(args.evidence))
                return
            if args.service_purchase:
                report['workflow'] = 'service_scanned_document_payable_bank_payment'
                stop(); config.pop('firstOwner')
                config['documentScanner'] = {'policy': 'required', 'provider': 'http-json-v1', 'url': f'https://127.0.0.1:{mail_port}/scan', 'bearerToken': scanner_token, 'timeoutMs': 10000, 'trustedCaFile': str(ca)}
                write_config(); start()
                phase('service-run')
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
                phase('finish', 20)
                controller.stdin.close(); controller.wait(timeout=10)
                if controller.returncode != 0:
                    raise RuntimeError('Service purchase controller nonzero terminal')
                controller = None; stop()
                db = sqlite3.connect((work/'workspace/synthetic/data/ledger.sqlite').as_uri()+'?mode=ro', uri=True)
                try:
                    counts = {table: db.execute('SELECT count(*) FROM '+table).fetchone()[0] for table in ['documents','document_scan_evidence','payables','payable_payments','bank_transactions','journal_entries']}
                    if counts != {'documents':1,'document_scan_evidence':1,'payables':1,'payable_payments':1,'bank_transactions':1,'journal_entries':2}:
                        raise RuntimeError('Service purchase terminal ledger effect counts differ')
                    report['terminalSql'] = {'counts':counts,'operationalActors':db.execute("SELECT event_type,actor FROM audit_log WHERE event_type IN ('document_ingest','payable_register','payable_payment_apply') ORDER BY id").fetchall(), 'journalActors':db.execute('SELECT created_by,created_by_program FROM journal_entries ORDER BY id').fetchall()}
                    actors = report['terminalSql']['operationalActors']
                    if len(actors) != 3 or not actors[0][1].startswith('service_account:') or any(actor != actors[0][1] for _, actor in actors):
                        raise RuntimeError('Service operational attribution differs')
                    subject = actors[0][1].removeprefix('service_account:')
                    if report['terminalSql']['journalActors'] != [('user:'+subject,'service-principal')]*2:
                        raise RuntimeError('Service journal attribution differs')
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
            if owned():
                shutil.rmtree(work)
                report['ownedRuntimeCleaned'] = True
            else:
                report['ownedRuntimeCleaned'] = False
            save()


if __name__ == '__main__':
    main()
