#!/usr/bin/env python3
"""Private real TLS SMTP + ordinary MFA/browser invoice delivery qualification.

No real forwarding, builds, offline identity seeds or production profiles. Supplied
binaries/web are pinned. Optional one signed archive uses source-free fresh restore.
"""
import argparse
import base64
import email.parser
import email.policy
import hashlib
import json
import os
import secrets
import selectors
import shutil
import signal
import socket
import socketserver
import sqlite3
import ssl
import subprocess
import threading
import time
import urllib.request
import uuid
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
PREFIX = 'Rentemester-Synthetic-InvoiceMail-'
BROWSER = r'''
import {createHmac,createHash} from 'node:crypto';
import {createInterface} from 'node:readline';
import {readFileSync,readdirSync,writeFileSync,unlinkSync} from 'node:fs';
import {join} from 'node:path';
import {spawnSync} from 'node:child_process';
import {isDeepStrictEqual} from 'node:util';
const {chromium}=await import(process.env.RM_MAIL_PLAYWRIGHT);
const base=process.env.RM_MAIL_BASE,work=process.env.RM_MAIL_WORK,prefix='/api/companies/synthetic/';
if(!/^http:\/\/127\.0\.0\.1:\d+$/.test(base))throw Error('literal owned loopback required');
const password='synthetic-invoice-mail-password-123',headers={origin:base,'content-type':'application/json'};
let serviceMail;
let email='owner@example.invalid',codes=[],lastLogin=0,step='boot',detail=null,browser,page,http,invoice,baseline,sourceProof,ownerId,finalRows,pending=null,trace=[];
const hash=b=>createHash('sha256').update(b).digest('hex');
function need(v,label){if(!v){detail=label;throw Error(label);}}
async function json(path,data,status=200,method='POST'){const r=data===undefined?await http.get(base+path):await http.fetch(base+path,{method,headers,data});need(r.status()===status,`canonical ${path} HTTP${r.status()} expected${status}`);return r.json();}
const get=p=>json(p),post=(p,data,status=200)=>json(p,data,status);
const current=()=>get(prefix+'invoices/email-deliveries');
async function selected(recipient){const value=await current();const rows=value.invoiceEmail.rows.filter(r=>r.invoiceDocumentId===invoice.documentId&&r.recipient===recipient&&r.kind==='invoice');need(rows.length===1,'exact current delivery identity');return rows[0];}
function totp(secret){let bits=0,count=0,bytes=[];for(const c of secret){const v='ABCDEFGHIJKLMNOPQRSTUVWXYZ234567'.indexOf(c);need(v>=0,'ordinary MFA setup key');bits=(bits<<5)|v;count+=5;if(count>=8){count-=8;bytes.push((bits>>>count)&255);}}const counter=Buffer.alloc(8);counter.writeBigUInt64BE(BigInt(Math.floor(Date.now()/30000)));const d=createHmac('sha1',Buffer.from(bytes)).update(counter).digest(),o=d[19]&15;return String((d.readUInt32BE(o)&0x7fffffff)%1000000).padStart(6,'0');}
async function cooldown(){const wait=11000-(Date.now()-lastLogin);if(wait>0)await new Promise(r=>setTimeout(r,wait));lastLogin=Date.now();}
async function open(){browser=await chromium.launch({headless:true});page=await browser.newPage();http=page.request;page.setDefaultTimeout(45000);page.on('response',r=>{const p=new URL(r.url()).pathname;if(p.startsWith('/api/')){trace.push({path:p.startsWith('/api/auth/reset-password/')?'/api/auth/reset-password/:token':p,status:r.status()});if(trace.length>32)trace.shift();}});}
async function login(){step='normal-password-login';await cooldown();await page.goto(base);await page.getByLabel('E-mail',{exact:true}).fill(email);await page.getByLabel('Adgangskode',{exact:true}).fill(password);await page.getByRole('button',{name:'Log ind',exact:true}).click();step='normal-existing-mfa';await page.getByRole('button',{name:'Brug recovery code',exact:true}).click();need(codes.length,'unused ordinary MFA recovery');await page.getByLabel('Recovery code',{exact:true}).fill(codes.shift());await page.getByRole('button',{name:'Bekræft',exact:true}).click();await page.locator('a[href="/companies/synthetic/posteringer"]').waitFor();const me=await get('/api/me');need(me.user?.email===email&&me.user.twoFactorEnabled,'normal verified human MFA identity');return me;}
async function enroll(root='workspace'){step='private-owner-verification';const spool=join(work,root,'.rentemester'),files=readdirSync(spool).filter(n=>n.startsWith('verification-')&&n.endsWith('.txt'));need(files.length===1,'exact ordinary verification delivery');await page.goto(readFileSync(join(spool,files[0]),'utf8').trim());step='normal-enrollment-login';await cooldown();await page.goto(base);await page.getByLabel('E-mail',{exact:true}).fill(email);await page.getByLabel('Adgangskode',{exact:true}).fill(password);await page.getByRole('button',{name:'Log ind',exact:true}).click();step='normal-enrollment-heading';await page.getByRole('heading',{name:'Opsæt totrinsbekræftelse',exact:true}).waitFor();await page.getByLabel('Adgangskode',{exact:true}).fill(password);await page.getByRole('button',{name:'Start opsætning',exact:true}).click();step='normal-enrollment-confirm';const key=await page.getByLabel('Opsætningsnøgle',{exact:true}).inputValue();await page.getByLabel('Engangskode',{exact:true}).fill(totp(key));await page.getByRole('button',{name:'Bekræft opsætning',exact:true}).click();const items=page.getByRole('region',{name:'Recovery codes'}).locator('li');await items.first().waitFor();codes=await items.allTextContents();need(codes.length>=4,'ordinary MFA recovery inventory');await page.getByRole('button',{name:'Jeg har gemt koderne',exact:true}).click();await page.locator('a[href="/companies/synthetic/posteringer"]').waitFor();await page.context().clearCookies();return login();}
async function originals(){const a=await http.get(base+prefix+`documents/${invoice.documentId}/file`),b=await http.get(base+prefix+`invoices/${invoice.documentId}/pdf`);need(a.ok()&&b.ok(),'authorized immutable issued originals');const j=await a.body(),pdf=await b.body();need(pdf.subarray(0,5).equals(Buffer.from('%PDF-')),'stored actual PDF');return{documentId:invoice.documentId,jsonSha256:hash(j),pdfSha256:hash(pdf),jsonBase64:j.toString('base64'),pdfBase64:pdf.toString('base64')};}
async function invariant(){need(isDeepStrictEqual(await originals(),sourceProof),'issued JSON/PDF changed');need(isDeepStrictEqual(await get(prefix+'journal'),baseline.journal),'mail mutated journal/UUID');need(isDeepStrictEqual(await get(prefix+'trial-balance'),baseline.balance),'mail mutated full trial balance');const row=(await get(prefix+'invoices')).invoices.invoices.find(r=>r.documentId===invoice.documentId);need(row&&row.posted===false,'mail must not post invoice');}
async function navigate(){step='navigate-unique-mail-heading';await page.goto(base+'/companies/synthetic/fakturaer');await page.getByRole('heading',{name:'Maillevering',exact:true}).waitFor();const panel=page.getByRole('region',{name:'Maillevering',exact:true});await panel.getByRole('button',{name:'Genlæs leveringsstatus',exact:true}).waitFor();return panel;}
async function pdfReview(scope,label){step='review-authorized-stored-pdf';const link=scope.getByRole('link',{name:label,exact:true});need((await link.getAttribute('href')).endsWith(`/invoices/${invoice.documentId}/pdf`),'review exact issued PDF URL');const popup=page.waitForEvent('popup');await link.click();const tab=await popup;const bytes=await http.get(base+prefix+`invoices/${invoice.documentId}/pdf`);need(bytes.ok()&&hash(await bytes.body())===sourceProof.pdfSha256,'user review exact stored PDF');await tab.close();}
async function prepare(recipient){const panel=await navigate();step='select-explicit-source-and-recipient';await panel.getByLabel('Faktura til mail',{exact:true}).selectOption(String(invoice.documentId));await panel.getByLabel('Modtagerens e-mail',{exact:true}).fill(recipient);await panel.getByLabel('Mailtype',{exact:true}).selectOption('invoice');await pdfReview(panel,'Åbn fakturaens gemte PDF');await panel.getByLabel('Jeg har gennemgået modtageren og den gemte PDF',{exact:true}).check();step='review-explicit-mail-dialog';await panel.getByRole('button',{name:'Gennemgå mailafsendelse',exact:true}).click();const dialog=page.getByRole('dialog',{name:'Bekræft mailafsendelse',exact:true});await dialog.waitFor();need(await dialog.getByRole('button',{name:'Send gennemgået mail én gang',exact:true}).isDisabled(),'separate confirmation before dispatch');need((await dialog.textContent()).includes(recipient),'dialog binds explicit recipient');await dialog.getByLabel('Jeg bekræfter den gennemgåede handling',{exact:true}).check();return dialog;}
async function send(recipient,status){const dialog=await prepare(recipient);step='confirmed-web-send';const response=page.waitForResponse(r=>new URL(r.url()).pathname===prefix+'invoices/send-email'&&r.request().method()==='POST');response.catch(()=>{});await dialog.getByRole('button',{name:'Send gennemgået mail én gang',exact:true}).click();const r=await response;need(r.status()===status,'canonical explicit send status');need(isDeepStrictEqual(r.request().postDataJSON(),{invoiceDocumentId:invoice.documentId,to:recipient,kind:'invoice',confirm:true}),'exact reviewed send body');const receipt=await r.json();await dialog.waitFor({state:'hidden'});const row=await selected(recipient);need(row.state===(status===200?'accepted':'refused')&&row.pdfSha256===sourceProof.pdfSha256,'canonical stored PDF outcome');need(receipt.delivery.messageId===row.messageId,'receipt/current identity');await invariant();return row;}
async function client(kind,name,body,accepted=true){const file=join(work,'client.cookie');writeFileSync(file,(await page.context().cookies(base)).map(c=>`${c.name}=${c.value}`).join('; '),{mode:0o600,flag:'wx'});try{const common=['--server',base,'--allow-loopback-http','--cookie-file',file];let args,input;if(kind==='cli'){args=['remote',name,...common,'--company','synthetic',...(body===undefined?[]:['--confirm'])];input=body===undefined?'':JSON.stringify(body);}else{args=common;input=[{jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'synthetic-invoice-email',version:'1'}}},{jsonrpc:'2.0',method:'notifications/initialized'},{jsonrpc:'2.0',id:2,method:'tools/call',params:{name,arguments:body===undefined?{company:'synthetic'}:{company:'synthetic',body:JSON.stringify(body),confirm:true}}}].map(v=>JSON.stringify(v)).join('\n')+'\n';}const out=spawnSync(join(work,kind==='cli'?'rentemester-rust':'rentemester-mcp'),args,{input,encoding:'utf8',timeout:30000,maxBuffer:16*1024*1024});need(!out.error&&out.status===(kind==='cli'&&!accepted?1:0),'pinned client exit matches canonical outcome');need(!out.stdout.includes(password),'client must not echo private authentication input');const result=kind==='cli'?JSON.parse(out.stdout):out.stdout.trim().split('\n').map(l=>JSON.parse(l)).find(v=>v.id===2)?.result?.structuredContent;need(result?.ok===accepted&&result.data,'full client canonical result');return result.data;}finally{unlinkSync(file);}}
async function parity(){const value=await current();need(isDeepStrictEqual(await client('cli','invoice-email-status'),value),'HTTP/CLI full canonical delivery parity');need(isDeepStrictEqual(await client('mcp','invoice_email_status'),value),'HTTP/MCP full canonical delivery parity');return value;}
async function setup(){await open();const me=await enroll();ownerId=me.user.id;step='explicit-source-seller-customer';await json(prefix+'company',{cvr:'87654321',address:'Synthetic Invoice Seller Street 1',postalCode:'1000',city:'Synthetic City',vatPeriodType:'quarter',paymentTermsDays:14},200,'PATCH');const buyer=(await post(prefix+'customers',{name:'Synthetic Invoice Mail Buyer',address:'Synthetic Buyer Street 2',vatOrCvr:'DK12345678',paymentTermsDays:14,defaultCurrency:'DKK'})).customer;need(buyer?.id,'ordinary customer receipt');const today=(await get(prefix+'scheduled-recurring')).scheduledRecurring.asOfDate;need(/^\d{4}-\d{2}-\d{2}$/.test(today),'server calendar date');const body={issueDate:today,customerId:buyer.id,seller:{vatOrCvr:'DK87654321'},vatRatePercent:25,lines:[{description:'Synthetic immutable mail service',quantity:1,unitPriceExVat:100}]};step='ordinary-review-preview-then-issue';const preview=await http.post(base+prefix+'invoices/preview',{headers,data:body});need(preview.ok()&&(await preview.body()).subarray(0,5).equals(Buffer.from('%PDF-')),'ordinary real PDF preview');invoice=(await post(prefix+'invoices/issue',body)).invoice;need(invoice.documentId&&invoice.invoiceNumber,'ordinary issued source identity');sourceProof=await originals();baseline={journal:await get(prefix+'journal'),balance:await get(prefix+'trial-balance')};need(baseline.journal.journal.entries.length===0,'unposted invoice source books empty');const empty=await current();need(empty.invoiceEmail.configured===true&&empty.invoiceEmail.rows.length===0,'explicit configured native SMTP and empty delivery history');return{normalVerificationAndMfa:true,ownerId,invoice,sourceProof,ledgerUuid:baseline.journal.journal.ledgerUuid};}
async function accepted(){step='accepted-web-send';return{row:await send('accepted@example.invalid',200)};}
async function acceptedDuplicate(){const row=await selected('accepted@example.invalid');step='accepted-cli-duplicate-no-transport';const duplicate=await client('cli','invoice-email-send',{invoiceDocumentId:invoice.documentId,to:'accepted@example.invalid',kind:'invoice',confirm:true});need(duplicate.delivery.duplicate===true&&duplicate.delivery.messageId===row.messageId&&duplicate.delivery.state==='accepted','accepted duplicate receipt');return{row,canonical:await parity()};}
async function refusalRetry(){step='definite-refusal-web-send';const refused=await send('retry@example.invalid',422);const panel=await navigate(),article=panel.getByRole('article',{name:`Mail ${invoice.invoiceNumber} til retry@example.invalid`,exact:true});step='explicit-review-refused-row';await article.getByRole('button',{name:'Gennemgå nyt forsøg',exact:true}).click();const group=panel.getByRole('group',{name:'Nyt mailforsøg',exact:true});const reason='Synthetic SMTP 550 refusal reviewed against fixture log';await group.getByLabel('Begrundelse',{exact:true}).fill(reason);await pdfReview(group,'Åbn fakturaens gemte PDF før nyt forsøg');await group.getByLabel('Jeg har gennemgået den faste modtager og PDF til nyt forsøg',{exact:true}).check();await group.getByRole('button',{name:'Gennemgå det nye forsøg',exact:true}).click();const dialog=page.getByRole('dialog',{name:'Bekræft nyt mailforsøg',exact:true});await dialog.waitFor();need(await dialog.getByRole('button',{name:'Send gennemgået mail én gang',exact:true}).isDisabled(),'retry separate confirmation');await dialog.getByLabel('Jeg bekræfter den gennemgåede handling',{exact:true}).check();step='confirmed-web-review-retry';const response=page.waitForResponse(r=>new URL(r.url()).pathname===prefix+'invoices/email-deliveries/retry'&&r.request().method()==='POST');response.catch(()=>{});await dialog.getByRole('button',{name:'Send gennemgået mail én gang',exact:true}).click();const r=await response;need(r.status()===200,'explicit reviewed retry accepted');need(isDeepStrictEqual(r.request().postDataJSON(),{messageId:refused.messageId,expectedStateHash:refused.stateHash,reason,confirm:true}),'retry exact reviewed state/reason');await dialog.waitFor({state:'hidden'});const row=await selected('retry@example.invalid');need(row.state==='accepted'&&row.messageId===refused.messageId&&row.stateHash!==refused.stateHash&&row.history.length>refused.history.length,'immutable refusal plus new accepted attempt');await invariant();return{refused,row,canonical:await parity()};}
async function startUnknown(){const dialog=await prepare('unknown@example.invalid');step='unknown-send-request-in-flight';const request=page.waitForRequest(r=>new URL(r.url()).pathname===prefix+'invoices/send-email'&&r.method()==='POST');request.catch(()=>{});pending=page.waitForResponse(r=>new URL(r.url()).pathname===prefix+'invoices/send-email'&&r.request().method()==='POST').then(r=>({status:r.status()})).catch(()=>({networkUnknown:true}));await dialog.getByRole('button',{name:'Send gennemgået mail én gang',exact:true}).click();await request;return{requestStarted:true};}
async function killed(){step='killed-response-unknown-marker';await pending;const dialog=page.getByRole('dialog',{name:'Bekræft mailafsendelse',exact:true});await dialog.getByRole('button',{name:'Resultatet er afklaret',exact:true}).waitFor();need(await dialog.getByRole('button',{name:'Send gennemgået mail én gang',exact:true}).isDisabled(),'ambiguous send blocked against replay');return{browserUnknownMarker:true};}
async function reopened(command){await page.context().clearCookies();const me=await login();need(me.user.id===ownerId,'same ordinary owner after real crash');step='restart-folded-unknown-no-send';const before=await selected('unknown@example.invalid');need(before.state==='unknown','durable reservation folds unknown');await invariant();step='mcp-unknown-duplicate-no-send';const duplicate=await client('mcp','invoice_email_send',{invoiceDocumentId:invoice.documentId,to:'unknown@example.invalid',kind:'invoice',confirm:true},false);need(duplicate.code==='delivery_unknown'&&duplicate.delivery.state==='unknown'&&duplicate.delivery.messageId===before.messageId,'canonical unknown duplicate preserved');const panel=await navigate(),article=panel.getByRole('article',{name:`Mail ${invoice.invoiceNumber} til unknown@example.invalid`,exact:true});step='human-review-unknown-queue-evidence';await article.getByRole('button',{name:'Gennemgå afklaring',exact:true}).click();const group=panel.getByRole('group',{name:'Afklar mailforsøg',exact:true});const reason='Synthetic queue accepted DATA before lost final acknowledgment';await group.getByLabel('Begrundelse',{exact:true}).fill(reason);await group.getByLabel('Kontrolleret udfald',{exact:true}).selectOption('accepted');await group.getByLabel('Dokumentation fra SMTP-log',{exact:true}).fill(command.evidence);await group.getByRole('button',{name:'Gennemgå afklaringen',exact:true}).click();const dialog=page.getByRole('dialog',{name:'Bekræft afklaring uden afsendelse',exact:true});await dialog.waitFor();need(await dialog.getByRole('button',{name:'Registrér afklaring',exact:true}).isDisabled(),'reconcile separate confirmation');await dialog.getByLabel('Jeg bekræfter den gennemgåede handling',{exact:true}).check();step='confirmed-human-reconcile-without-send';const response=page.waitForResponse(r=>new URL(r.url()).pathname===prefix+'invoices/email-deliveries/reconcile'&&r.request().method()==='POST');response.catch(()=>{});await dialog.getByRole('button',{name:'Registrér afklaring',exact:true}).click();const r=await response;need(r.status()===200,'human accepted reconciliation receipt');need(isDeepStrictEqual(r.request().postDataJSON(),{messageId:before.messageId,expectedStateHash:before.stateHash,reason,resolution:'accepted',evidence:command.evidence,confirm:true}),'exact human attestation evidence/state');await dialog.waitFor({state:'hidden'});const row=await selected('unknown@example.invalid');need(row.state==='accepted'&&row.humanResolution===true&&row.history.some(h=>h.resolution==='accepted'&&h.evidence===command.evidence&&h.reason===reason),'history distinguishes human accepted attestation');const rendered=await navigate();await rendered.getByRole('article',{name:`Mail ${invoice.invoiceNumber} til unknown@example.invalid`,exact:true}).getByText('Accepteret efter ejerens afklaring',{exact:false}).waitFor();await invariant();const canonical=await parity();need(canonical.invoiceEmail.rows.length===3&&canonical.invoiceEmail.rows.every(r=>r.state==='accepted'),'exact three accepted canonical recipient deliveries');finalRows=canonical.invoiceEmail.rows;return{before,row,canonical,unchangedBooksAndOriginals:true};}
async function exportArchive(){step='explicit-signed-workspace-export';const r=await http.post(base+'/api/workspace/backup',{headers,data:{confirm:true}});need(r.ok()&&r.headers()['content-type']==='application/vnd.rentemester.native-backup+json','actual native signed archive');const bytes=await r.body();need(bytes.length>0&&bytes.length<=96*1024*1024,'bounded signed artifact');writeFileSync(join(work,'invoice-mail.native-backup.json'),bytes,{mode:0o600,flag:'wx'});return{sha256:hash(bytes),bytes:bytes.length};}
async function restored(){await browser.close();await open();email='restored-owner@example.invalid';codes=[];const who=await enroll('restored');need(who.user.id!==ownerId,'fresh restored human identity');step='restored-history-no-operator-smtp-config';const value=await parity();need(value.invoiceEmail.configured===false&&isDeepStrictEqual(value.invoiceEmail.rows,finalRows),'exact delivery history without SMTP config');await invariant();const failed=await http.post(base+prefix+'invoices/send-email',{headers,data:{invoiceDocumentId:invoice.documentId,to:'no-config@example.invalid',kind:'invoice',confirm:true}});need(failed.status()===422,'no-config restored send refuses');need(isDeepStrictEqual((await current()).invoiceEmail.rows,finalRows),'no-config refusal adds no delivery attempt');await navigate();await page.getByText('Mailafsendelse er ikke konfigureret for virksomheden.',{exact:false}).waitFor();return{newOwnerId:who.user.id,exactHistory:true,originalsAndUuid:true,configured:false,noConfigSendRefused:true};}
for await(const line of createInterface({input:process.stdin,crlfDelay:Infinity})){try{const c=JSON.parse(line);let data;if(c.action==='setup'){data=await setup();if(process.env.RM_MAIL_SERVICE==='1'){const factory=await import(process.env.RM_MAIL_SERVICE_HELPER);serviceMail=await factory.createServiceMailProof(page,base,work,invoice.documentId);data.service=await serviceMail.setup();}}else if(c.action==='accepted'){data=serviceMail?await serviceMail.accepted():await accepted();if(serviceMail&&c.action!=='start-unknown'&&c.action!=='killed')await invariant();}else if(c.action==='accepted-duplicate'){data=serviceMail?await serviceMail.acceptedDuplicate():await acceptedDuplicate();if(serviceMail&&c.action!=='start-unknown'&&c.action!=='killed')await invariant();}else if(c.action==='refused-retry'){data=serviceMail?await serviceMail.refusalRetry():await refusalRetry();if(serviceMail&&c.action!=='start-unknown'&&c.action!=='killed')await invariant();}else if(c.action==='start-unknown'){data=serviceMail?await serviceMail.startUnknown():await startUnknown();if(serviceMail&&c.action!=='start-unknown'&&c.action!=='killed')await invariant();}else if(c.action==='killed'){data=serviceMail?await serviceMail.killed():await killed();if(serviceMail&&c.action!=='start-unknown'&&c.action!=='killed')await invariant();}else if(c.action==='service-rotate')data=await serviceMail.rotate();else if(c.action==='reopened'){if(serviceMail){await page.context().clearCookies();await login();data=await serviceMail.reopened(c.evidence);finalRows=data.canonical.invoiceEmail.rows;await invariant();}else data=await reopened(c);}else if(c.action==='export')data=await exportArchive();else if(c.action==='restored')data=await restored();else if(c.action==='finish'){await serviceMail?.dispose();await browser?.close();console.log(JSON.stringify({ok:true,data:{finished:true}}));break;}else throw Error('unknown controller action');console.log(JSON.stringify({ok:true,data}));}catch(error){let ui=null;try{ui=await page?.evaluate(()=>({pathname:location.pathname,headings:[...document.querySelectorAll('h1,h2')].map(e=>e.textContent.slice(0,100)),buttons:[...document.querySelectorAll('button')].map(e=>e.textContent.slice(0,100)),alerts:document.querySelectorAll('[role=alert]').length}));}catch{}console.log(JSON.stringify({ok:false,step,detail:error?.name==='ServiceMailAssertion'?error.message:detail,errorType:error?.name,trace,ui}));await browser?.close().catch(()=>{});process.exitCode=1;break;}}
'''


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def port():
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0))
        return sock.getsockname()[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for flag in ['server-binary', 'web-dir', 'cli-binary', 'mcp-binary', 'evidence']:
        parser.add_argument('--' + flag, required=True, type=Path)
    parser.add_argument('--service-delivery', action='store_true', help='Qualify explicit service status/send/retry/reconcile over the existing real TLS SMTP fixture')
    parser.add_argument('--with-backup', action='store_true', help='One signed source-free verify/fresh-owner restore without SMTP configuration')
    args = parser.parse_args()
    if os.uname().sysname != 'Darwin' or os.getuid() == 0 or os.getuid() != os.geteuid():
        parser.error('Ordinary macOS user required')
    if args.evidence.exists():
        parser.error('New evidence path required')
    os.umask(0o077)
    parent = (Path.home() / 'Library/Application Support').resolve(strict=True)
    work = parent / (PREFIX + uuid.uuid4().hex)
    child = controller = log = smtp = None
    identity = None
    lock = threading.Lock()
    queued = threading.Event()
    release = threading.Event()
    smtp_secret = secrets.token_urlsafe(32)
    stats = {'connections': 0, 'rcptAttempts': {}, 'messages': [], 'errors': []}
    report = {'status': 'running', 'sourceRevision': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=REPO, text=True).strip(), 'runnerSha256': sha(Path(__file__)), 'browserHelperSha256': hashlib.sha256(BROWSER.encode()).hexdigest(), 'noOfflineIdentitySeeds': True, 'externalMailForwarded': False, 'linuxIsolationQualified': False, 'globalArchiveSingleUseClaim': False, 'signedRestoreQualified': False, 'serverPids': [], 'phases': []}
    if args.service_delivery:
        report['serviceHelperSha256'] = sha(REPO / 'scripts/rust/check-native-service-mail.ts')
        report['installedNativeQualified'] = False
        report['preHandoffRevocationEvidence'] = 'Focused domain test; actual service fixture rotates before the unknown SMTP attempt, not during it'
    args.evidence.parent.mkdir(parents=True, exist_ok=True)
    with args.evidence.open('x') as output:
        def save():
            output.seek(0)
            output.truncate()
            json.dump(report, output, indent=2)
            output.flush()
            os.fsync(output.fileno())

        def owned():
            if identity is None or not work.exists() or work.is_symlink():
                return False
            st = work.lstat()
            return work.parent == parent and work.name.startswith(PREFIX) and (st.st_dev, st.st_ino, st.st_uid) == (*identity, os.getuid())

        def stop(crash=False):
            nonlocal child, log
            if child is not None:
                if child.poll() is None:
                    child.kill() if crash else child.terminate()
                try:
                    child.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait(timeout=5)
                if crash:
                    report['crashExitCode'] = child.returncode
                child = None
            if log:
                log.close()
                log = None

        def command(action, timeout=180, **fields):
            controller.stdin.write(json.dumps({'action': action, **fields}) + '\n')
            controller.stdin.flush()
            with selectors.DefaultSelector() as ready:
                ready.register(controller.stdout, selectors.EVENT_READ)
                if not ready.select(timeout):
                    raise RuntimeError('Bounded browser phase expired: ' + action)
                line = controller.stdout.readline()
            try:
                result = json.loads(line)
            except ValueError:
                raise RuntimeError('Controller ended without sanitized response') from None
            if not result.get('ok'):
                report['browserFailure'] = result
                raise RuntimeError('Browser assertion failed at ' + result.get('step', action))
            return result['data']

        def snapshot():
            with lock:
                return json.loads(json.dumps(stats))

        def check_queue(expected, original, states):
            value = snapshot()
            if value['errors'] or value['rcptAttempts'] != states or len(value['messages']) != expected:
                raise RuntimeError('Exact SMTP attempt/queue invariants refused')
            for message in value['messages']:
                if message['pdfSha256'] != original['pdfSha256'] or message['pdfCount'] != 1:
                    raise RuntimeError('SMTP attachment differs from authorized immutable PDF')
            return value

        def success_rows(root):
            database = root / 'synthetic/data/ledger.sqlite'
            if not owned() or database.is_symlink() or not database.is_file():
                raise RuntimeError('Only exact stopped owned synthetic ledger may be observed')
            # Read-only observability, never seeds identity or changes accounting state.
            connection = sqlite3.connect(database.as_uri() + '?mode=ro', uri=True)
            try:
                return connection.execute('SELECT invoice_document_id,recipient,message_id,body_sha256 FROM email_send_log ORDER BY id').fetchall()
            finally:
                connection.close()

        class Server(socketserver.ThreadingTCPServer):
            daemon_threads = True

        class Handler(socketserver.BaseRequestHandler):
            def handle(self):
                connection = None
                try:
                    self.request.settimeout(15)
                    connection = tls.wrap_socket(self.request, server_side=True)
                    connection.settimeout(15)
                    stream = connection.makefile('rb')
                    with lock:
                        stats['connections'] += 1
                    authenticated = False
                    recipient = None
                    sender = None
                    def reply(value):
                        connection.sendall(value.encode('ascii') + b'\r\n')
                    def line():
                        value = stream.readline(16385)
                        if not value:
                            raise EOFError('SMTP peer closed its connection')
                        if len(value) > 16384 or not value.endswith(b'\r\n'):
                            raise ValueError('bounded SMTP line required')
                        return value[:-2]
                    def auth(raw):
                        nonlocal authenticated
                        value = base64.b64decode(raw, validate=True)
                        if value != b'\0fixture-smtp-user\0' + smtp_secret.encode():
                            reply('535 Authentication refused')
                            return
                        authenticated = True
                        reply('235 Authentication successful')
                    reply('220 localhost synthetic private SMTP')
                    for _ in range(100):
                        raw = line()
                        upper = raw.upper()
                        if upper.startswith(b'EHLO '):
                            reply('250-localhost')
                            reply('250-AUTH PLAIN')
                            reply('250-8BITMIME')
                            reply('250 SIZE 20971520')
                        elif upper.startswith(b'AUTH PLAIN'):
                            parts = raw.split(b' ', 2)
                            if len(parts) == 3:
                                auth(parts[2])
                            else:
                                reply('334 ')
                                auth(line())
                        elif upper.startswith(b'MAIL FROM:') and authenticated:
                            sender = raw.split(b':', 1)[1].split(b'>', 1)[0].strip().strip(b'<').decode('ascii')
                            if sender != 'sales@example.invalid':
                                raise ValueError('unexpected synthetic SMTP sender')
                            reply('250 Sender accepted')
                        elif upper.startswith(b'RCPT TO:') and authenticated and sender:
                            recipient = raw.split(b':', 1)[1].split(b'>', 1)[0].strip().strip(b'<').decode('ascii')
                            if recipient not in ('accepted@example.invalid', 'retry@example.invalid', 'unknown@example.invalid'):
                                raise ValueError('non-synthetic SMTP recipient refused')
                            with lock:
                                count = stats['rcptAttempts'].get(recipient, 0) + 1
                                stats['rcptAttempts'][recipient] = count
                            if recipient == 'retry@example.invalid' and count == 1:
                                recipient = None
                                reply('550 Synthetic definite RCPT refusal')
                            else:
                                reply('250 Recipient accepted')
                        elif upper == b'DATA' and authenticated and recipient:
                            reply('354 End with dot')
                            chunks = []
                            length = 0
                            for _ in range(200000):
                                part = line()
                                if part == b'.':
                                    break
                                if part.startswith(b'..'):
                                    part = part[1:]
                                length += len(part) + 2
                                if length > 20 * 1024 * 1024:
                                    raise ValueError('bounded SMTP MIME exceeded')
                                chunks.append(part + b'\r\n')
                            else:
                                raise ValueError('bounded SMTP DATA lines exceeded')
                            raw_message = b''.join(chunks)
                            parsed = email.parser.BytesParser(policy=email.policy.default).parsebytes(raw_message)
                            pdfs = [p.get_payload(decode=True) for p in parsed.walk() if p.get_content_type() == 'application/pdf']
                            if len(pdfs) != 1 or not pdfs[0].startswith(b'%PDF-'):
                                raise ValueError('one real PDF attachment required')
                            with lock:
                                number = len(stats['messages']) + 1
                                record = {'queueId': 'synthetic-queue-' + str(number), 'recipient': recipient, 'sender': sender, 'messageId': str(parsed['Message-ID']), 'bodySha256': hashlib.sha256(raw_message).hexdigest(), 'pdfSha256': hashlib.sha256(pdfs[0]).hexdigest(), 'pdfCount': len(pdfs), 'queuedMonotonic': time.monotonic(), 'finalAck': recipient != 'unknown@example.invalid'}
                                with (work / ('queue-' + str(number) + '.mime')).open('xb') as archived:
                                    archived.write(raw_message)
                                    archived.flush()
                                    os.fsync(archived.fileno())
                                stats['messages'].append(record)
                            if recipient == 'unknown@example.invalid':
                                queued.set()
                                # Source is killed while real queue evidence exists and
                                # before the native eight-second SMTP deadline/terminal.
                                release.wait(12)
                            reply('250 Synthetic queue accepted')
                            recipient = sender = None
                        elif upper == b'RSET':
                            recipient = sender = None
                            reply('250 Reset')
                        elif upper == b'QUIT':
                            reply('221 Bye')
                            break
                        elif upper == b'NOOP':
                            reply('250 OK')
                        else:
                            reply('503 Bad sequence')
                except (EOFError, OSError, ssl.SSLError):
                    pass  # Lost final acknowledgement after the intentional crash is expected.
                except Exception:
                    with lock:
                        stats['errors'].append('smtp-protocol-or-source-invariant')
                finally:
                    if connection:
                        connection.close()

        try:
            work.mkdir(mode=0o700)
            identity = (work.stat().st_dev, work.stat().st_ino)
            report['ownedProfile'] = str(work)
            pinned = {}
            for field, name in [('server_binary', 'rentemester-server'), ('cli_binary', 'rentemester-rust'), ('mcp_binary', 'rentemester-mcp')]:
                path = work / name
                shutil.copyfile(getattr(args, field).resolve(strict=True), path)
                path.chmod(0o700)
                pinned[field] = path
            web = work / 'web'
            shutil.copytree(args.web_dir.resolve(strict=True), web)
            report['artifacts'] = {field: sha(path) for field, path in pinned.items()}
            report['webFiles'] = {str(p.relative_to(web)): sha(p) for p in sorted(web.rglob('*')) if p.is_file()}
            ca, cert, private = work / 'ca.pem', work / 'smtp.pem', work / 'smtp.key'
            (work / 'smtp.ext').write_text('basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\nsubjectAltName=DNS:localhost,IP:127.0.0.1\n')
            commands = [
                ['openssl', 'req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', str(work / 'ca.key'), '-out', str(ca), '-days', '1', '-subj', '/CN=Synthetic Invoice SMTP CA', '-addext', 'basicConstraints=critical,CA:TRUE', '-addext', 'keyUsage=critical,keyCertSign,cRLSign'],
                ['openssl', 'req', '-new', '-newkey', 'rsa:2048', '-nodes', '-keyout', str(private), '-out', str(work / 'smtp.csr'), '-subj', '/CN=localhost'],
                ['openssl', 'x509', '-req', '-in', str(work / 'smtp.csr'), '-CA', str(ca), '-CAkey', str(work / 'ca.key'), '-CAcreateserial', '-out', str(cert), '-days', '1', '-extfile', str(work / 'smtp.ext')],
            ]
            with (work / 'openssl.log').open('wb') as tls_log:
                for argv in commands:
                    subprocess.run(argv, check=True, stdout=tls_log, stderr=tls_log, timeout=30)
            tls = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
            tls.load_cert_chain(cert, private)
            smtp = Server(('127.0.0.1', 0), Handler)
            threading.Thread(target=smtp.serve_forever, daemon=True).start()
            web_port = port()
            base = f'http://127.0.0.1:{web_port}'
            key = lambda: base64.urlsafe_b64encode(secrets.token_bytes(32)).decode().rstrip('=')
            config = {'runtimeProfile': 'local-user-macos', 'root': str(work / 'workspace'), 'brokerUid': os.getuid(), 'workspace': 'synthetic-native-invoice-mail', 'companySlug': 'synthetic', 'companyName': 'Synthetic Invoice Mail Company', 'cookieKey': key(), 'mfaKey': key(), 'operatorActors': ['system:synthetic-installer'], 'firstOwner': {'actor': 'system:synthetic-installer', 'email': 'owner@example.invalid', 'name': 'Synthetic Mail Owner', 'password': 'synthetic-invoice-mail-password-123'}, 'accounts': [], 'invoiceMail': {'synthetic': {'host': 'localhost', 'port': smtp.server_address[1], 'fromAddress': 'sales@example.invalid', 'fromName': 'Synthetic Seller', 'username': 'fixture-smtp-user', 'password': smtp_secret, 'tls': 'implicit', 'trustedCaFile': str(ca)}}}
            if args.with_backup:
                subprocess.run(['/opt/homebrew/bin/bun', '-e', "const c=require('node:crypto'),f=require('node:fs'),k=c.generateKeyPairSync('ed25519'),s=k.privateKey.export({format:'der',type:'pkcs8'}).subarray(-32),p=k.publicKey.export({format:'der',type:'spki'}).subarray(-32);f.writeFileSync(process.argv[1],Buffer.concat([Buffer.from('3051020101300506032b657004220420','hex'),s,Buffer.from('812100','hex'),p]),{mode:0o600});f.writeFileSync(process.argv[2],p,{mode:0o600});", str(work / 'signing.der'), str(work / 'trusted.pub')], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                config['backupSigningKeyFile'] = str(work / 'signing.der')
            def write():
                if not owned():
                    raise RuntimeError('Owned fixture identity changed')
                (work / 'server.json').write_text(json.dumps(config))
            def start(create=False, extra=None):
                nonlocal child, log
                if child is not None or not owned():
                    raise RuntimeError('Exact prior server must stop before restart')
                log = (work / 'server.log').open('ab')
                child = subprocess.Popen([str(pinned['server_binary']), '--config', str(work / 'server.json'), '--web-dir', str(web), '--listen', f'127.0.0.1:{web_port}'] + (['--create'] if create else []) + (extra or []), cwd=REPO, stdin=subprocess.DEVNULL, stdout=log, stderr=log)
                report['serverPids'].append(child.pid)
                save()
                for _ in range(300):
                    if child.poll() is not None:
                        raise RuntimeError('Owned server exited before readiness')
                    try:
                        with urllib.request.urlopen(base + '/api/health', timeout=.2) as r:
                            if json.load(r).get('workspaceReady'):
                                return
                    except (OSError, ValueError):
                        pass
                    time.sleep(.1)
                raise RuntimeError('Owned server readiness deadline')
            write()
            start(True)
            script = work / 'browser.mjs'
            script.write_text(BROWSER)
            controller = subprocess.Popen(['/opt/homebrew/bin/bun', str(script)], cwd=REPO, env={**os.environ, 'RM_MAIL_BASE': base, 'RM_MAIL_WORK': str(work), 'RM_MAIL_SERVICE': '1' if args.service_delivery else '0', 'RM_MAIL_SERVICE_HELPER': str(REPO / 'scripts/rust/check-native-service-mail.ts'), 'RM_MAIL_PLAYWRIGHT': str((REPO / 'node_modules/@playwright/test/index.mjs').resolve(strict=True))}, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, bufsize=1, start_new_session=True)
            report['browserControllerPid'] = controller.pid
            report['setup'] = command('setup')
            if snapshot()['connections'] != 0:
                raise RuntimeError('Setup/status must not open SMTP transport')
            original = report['setup']['sourceProof']
            report['accepted'] = command('accepted')
            first = check_queue(1, original, {'accepted@example.invalid': 1})
            report['acceptedDuplicate'] = command('accepted-duplicate')
            if snapshot() != first:
                raise RuntimeError('Accepted duplicate/status opened SMTP or changed queue')
            report['phases'].append('service-cli-accepted-mcp-duplicate-no-transport' if args.service_delivery else 'web-accepted-cli-duplicate-no-transport')
            report['refusedRetry'] = command('refused-retry')
            check_queue(2, original, {'accepted@example.invalid': 1, 'retry@example.invalid': 2})
            report['phases'].append('service-cli-refusal-explicit-mcp-retry' if args.service_delivery else '550-definite-refusal-reviewed-web-retry')
            if args.service_delivery:
                stable_smtp = snapshot()
                report['serviceRotation'] = command('service-rotate')
                if snapshot() != stable_smtp:
                    raise RuntimeError('Service rotation or revoked credential opened SMTP')
            report['inflight'] = command('start-unknown')
            if not queued.wait(7):
                raise RuntimeError('No real queued DATA before native deadline; refusing fake crash proof')
            killed_at = time.monotonic()
            stop(crash=True)
            elapsed = killed_at - snapshot()['messages'][-1]['queuedMonotonic']
            if elapsed >= 7 or report['crashExitCode'] != -signal.SIGKILL:
                raise RuntimeError('Source was not killed before bounded unknown handoff deadline')
            report['crashAfterQueueSeconds'] = elapsed
            report['browserUnknown'] = command('killed', timeout=60)
            before_restart = check_queue(3, original, {'accepted@example.invalid': 1, 'retry@example.invalid': 2, 'unknown@example.invalid': 1})
            partial = success_rows(work / 'workspace')
            if len(partial) != 2:
                raise RuntimeError('Crash reservation incorrectly manufactured terminal success')
            report['preReconcileSuccessRows'] = partial
            config.pop('firstOwner')
            write()
            start()
            evidence = 'Synthetic SMTP durable queue ' + before_restart['messages'][-1]['queueId'] + ' Message-ID ' + before_restart['messages'][-1]['messageId'] + ' accepted DATA SHA256 ' + before_restart['messages'][-1]['bodySha256']
            report['reopened'] = command('reopened', evidence=evidence)
            after = check_queue(3, original, before_restart['rcptAttempts'])
            if after['connections'] != before_restart['connections']:
                raise RuntimeError('Restart/status/duplicate/reconciliation opened SMTP transport')
            rows = report['reopened']['canonical']['invoiceEmail']['rows']
            for message in after['messages']:
                row = next(r for r in rows if r['recipient'] == message['recipient'])
                if row['messageId'] != message['messageId'] or row['bodySha256'] != message['bodySha256']:
                    raise RuntimeError('Canonical identity/MIME hash differs from actual SMTP DATA')
            report['phases'].append('crash-unknown-restart-mcp-no-resend-service-cli-external-evidence' if args.service_delivery else 'crash-unknown-restart-mcp-no-resend-human-accepted-attestation')
            if args.with_backup:
                report['signedArchive'] = command('export')
            stop()
            full = success_rows(work / 'workspace')
            if len(full) != 3 or len({r[2] for r in full}) != 3:
                raise RuntimeError('Exact three success log records required')
            report['successRows'] = full
            # Operator SMTP secret must never enter business evidence, including SQLite/WAL.
            secret_bytes = smtp_secret.encode()
            for path in (work / 'workspace').rglob('*'):
                if path.is_file() and not path.is_symlink():
                    carry = b''
                    with path.open('rb') as stream:
                        while chunk := stream.read(1024 * 1024):
                            data = carry + chunk
                            if secret_bytes in data:
                                raise RuntimeError('SMTP credential persisted in business workspace')
                            carry = data[-len(secret_bytes):]
            report['smtpSecretAbsentFromBusinessWorkspace'] = True
            if args.with_backup:
                source = work / 'workspace'
                if not owned() or source.is_symlink():
                    raise RuntimeError('Exact source removal refused')
                shutil.rmtree(source)
                (work / 'signing.der').unlink()
                archive, trusted = work / 'invoice-mail.native-backup.json', work / 'trusted.pub'
                if sha(archive) != report['signedArchive']['sha256'] or secret_bytes in archive.read_bytes():
                    raise RuntimeError('Signed archive changed or contains operator SMTP credential')
                verify = subprocess.run([str(pinned['server_binary']), '--verify-backup', str(archive), '--trusted-backup-key', str(trusted)], cwd=REPO, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=60)
                if verify.returncode != 0:
                    raise RuntimeError('Independent source-free signed invoice-email verification refused')
                config.update({'root': str(work / 'restored'), 'cookieKey': key(), 'mfaKey': key(), 'restoreActor': 'system:synthetic-installer', 'firstOwner': {'actor': 'system:synthetic-installer', 'email': 'restored-owner@example.invalid', 'name': 'Synthetic Restored Mail Owner', 'password': 'synthetic-invoice-mail-password-123'}})
                config.pop('invoiceMail')
                config.pop('backupSigningKeyFile')
                write()
                if os.path.lexists(work / 'restored'):
                    raise RuntimeError('Fresh restore target required')
                start(extra=['--restore-backup', str(archive), '--trusted-backup-key', str(trusted), '--confirm-target', config['root'], '--source-unavailable'])
                report['restored'] = command('restored')
                stop()
                if success_rows(work / 'restored') != full:
                    raise RuntimeError('Signed restore changed success history')
                if snapshot()['connections'] != after['connections']:
                    raise RuntimeError('Restore/read/no-config refusal opened SMTP transport')
                report['signedRestoreQualified'] = True
                report['signedArchive'].update({'sourceAndPrivateSignerUnavailable': True, 'independentlyVerified': True, 'trustedPublicKeySha256': sha(trusted)})
                report['phases'].append('signed-source-free-new-owner-mfa-restore-no-smtp-config')
            command('finish', timeout=20)
            controller.stdin.close()  # Bun controller must receive EOF before terminal wait.
            controller.wait(timeout=10)
            if controller.returncode != 0:
                raise RuntimeError('Browser controller nonzero terminal')
            controller = None
            report['smtp'] = snapshot()
            if smtp_secret in json.dumps(report):
                raise RuntimeError('Operator credential entered evidence')
            report['serviceDeliveryQualified'] = args.service_delivery
            report['status'] = 'passed'
            save()
            print('PASS: real verified TLS SMTP, ' + ('service CLI/MCP delivery' if args.service_delivery else 'reviewed web sends') + ', crash/restart unknown and client parity; ' + str(args.evidence))
        except BaseException as error:
            report['status'] = 'failed'
            report['failureType'] = type(error).__name__
            if isinstance(error, RuntimeError):
                report['failureLabel'] = str(error)[:240]
            report['smtp'] = snapshot()
            save()
            raise
        finally:
            release.set()
            if controller is not None and controller.poll() is None:
                os.killpg(controller.pid, signal.SIGTERM)
                try:
                    controller.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    os.killpg(controller.pid, signal.SIGKILL)
                    controller.wait(timeout=5)
            stop()
            if smtp:
                smtp.shutdown()
                smtp.server_close()
            if owned():
                shutil.rmtree(work)
                report['ownedRuntimeCleaned'] = True
            else:
                report['ownedRuntimeCleaned'] = False
            save()


if __name__ == '__main__':
    main()
