#!/usr/bin/env python3
"""Owned J06 source→staged destination transfer proof. No builds or real delivery.

Supplied binaries/web only. Normal verification/MFA, real accepted lost response,
private verified TLS unknown queue, reviewed web transfer, exact receipt replay.
This does not qualify hostile clones, older software, rollback or Linux isolation.
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
PREFIX = 'Rentemester-Synthetic-Transfer-'
BROWSER = r'''
import {createHmac,createHash} from 'node:crypto';
import {createConnection} from 'node:net';
import {createInterface} from 'node:readline';
import {readFileSync,readdirSync,writeFileSync,unlinkSync} from 'node:fs';
import {join} from 'node:path';
import {spawnSync} from 'node:child_process';
import {isDeepStrictEqual} from 'node:util';
const {chromium,request}=await import(process.env.RM_TRANSFER_PLAYWRIGHT);
const work=process.env.RM_TRANSFER_WORK,prefix='/api/companies/synthetic/',password='synthetic-transfer-owner-password-123';
const source={base:process.env.RM_TRANSFER_SOURCE,email:'owner@example.invalid',codes:[],lastLogin:0,name:'source'},target={base:process.env.RM_TRANSFER_TARGET,email:'target-owner@example.invalid',codes:[],lastLogin:0,name:'target'};
for(const s of[source,target])if(!/^http:\/\/127\.0\.0\.1:\d+$/.test(s.base))throw Error('owned literal loopback required');
let browser,scope,base,page,http,headers,email,codes,lastLogin,step='boot',detail=null,trace=[],invoice,sourceProof,baseline,original,frozen,archiveHash,challenge,signedRelease,mappedReceipt,sourceOwnerId,targetOwnerId,unknown,finalTargetJournal,finalTargetBalance,pending;
const hash=b=>createHash('sha256').update(b).digest('hex');
function need(v,label){if(!v){detail=label;throw Error(label);}}
function use(s){if(scope){scope.codes=codes;scope.lastLogin=lastLogin;}scope=s;base=s.base;page=s.page;http=page.request;headers={origin:base,'content-type':'application/json'};email=s.email;codes=s.codes;lastLogin=s.lastLogin;}
async function open(s){s.context=await browser.newContext();s.page=await s.context.newPage();s.page.setDefaultTimeout(45000);s.page.on('response',r=>{const p=new URL(r.url()).pathname;if(p.startsWith('/api/')){trace.push({server:s.name,path:p.startsWith('/api/auth/reset-password/')?'/api/auth/reset-password/:token':p,status:r.status()});if(trace.length>48)trace.shift();}});use(s);}
async function json(path,data,status=200,method='POST'){const r=data===undefined?await http.get(base+path):await http.fetch(base+path,{method,headers,data});need(r.status()===status,`${scope.name} canonical ${path} HTTP${r.status()} expected${status}`);return r.json();}
const get=p=>json(p),post=(p,data,status=200)=>json(p,data,status),transfer=async()=>(await get('/api/workspace/transfer')).transfer;
function totp(secret){let bits=0,count=0,bytes=[];for(const c of secret){const v='ABCDEFGHIJKLMNOPQRSTUVWXYZ234567'.indexOf(c);need(v>=0,'normal MFA setup key');bits=(bits<<5)|v;count+=5;if(count>=8){count-=8;bytes.push((bits>>>count)&255);}}const counter=Buffer.alloc(8);counter.writeBigUInt64BE(BigInt(Math.floor(Date.now()/30000)));const d=createHmac('sha1',Buffer.from(bytes)).update(counter).digest(),o=d[19]&15;return String((d.readUInt32BE(o)&0x7fffffff)%1000000).padStart(6,'0');}
async function cooldown(){const wait=11000-(Date.now()-lastLogin);if(wait>0)await new Promise(r=>setTimeout(r,wait));lastLogin=Date.now();}
async function login(){step=scope.name+'-normal-password';await cooldown();await page.goto(base);await page.getByLabel('E-mail',{exact:true}).fill(email);await page.getByLabel('Adgangskode',{exact:true}).fill(password);await page.getByRole('button',{name:'Log ind',exact:true}).click();step=scope.name+'-existing-mfa';await page.getByRole('button',{name:'Brug recovery code',exact:true}).click();need(codes.length,'unused ordinary MFA recovery code');await page.getByLabel('Recovery code',{exact:true}).fill(codes.shift());await page.getByRole('button',{name:'Bekræft',exact:true}).click();await page.locator('a[href="/companies/synthetic/posteringer"]').waitFor();const me=await get('/api/me');need(me.user?.email===email&&me.user.twoFactorEnabled,'normal verified distinct human MFA');return me;}
async function enroll(){step=scope.name+'-ordinary-owner-verification';const spool=join(work,scope.name,'workspace','.rentemester'),files=readdirSync(spool).filter(n=>n.startsWith('verification-')&&n.endsWith('.txt'));need(files.length===1,'one ordinary private verification delivery');await page.goto(readFileSync(join(spool,files[0]),'utf8').trim());await cooldown();await page.goto(base);step=scope.name+'-enroll-password';await page.getByLabel('E-mail',{exact:true}).fill(email);await page.getByLabel('Adgangskode',{exact:true}).fill(password);await page.getByRole('button',{name:'Log ind',exact:true}).click();step=scope.name+'-enroll-heading';await page.getByRole('heading',{name:'Opsæt totrinsbekræftelse',exact:true}).waitFor();await page.getByLabel('Adgangskode',{exact:true}).fill(password);await page.getByRole('button',{name:'Start opsætning',exact:true}).click();step=scope.name+'-enroll-confirm';const key=await page.getByLabel('Opsætningsnøgle',{exact:true}).inputValue();await page.getByLabel('Engangskode',{exact:true}).fill(totp(key));await page.getByRole('button',{name:'Bekræft opsætning',exact:true}).click();const items=page.getByRole('region',{name:'Recovery codes'}).locator('li');await items.first().waitFor();codes=await items.allTextContents();need(codes.length>=4,'normal recovery inventory');await page.getByRole('button',{name:'Jeg har gemt koderne',exact:true}).click();await page.locator('a[href="/companies/synthetic/posteringer"]').waitFor();await page.context().clearCookies();return login();}
async function originals(){const a=await http.get(base+prefix+`documents/${invoice.documentId}/file`),b=await http.get(base+prefix+`invoices/${invoice.documentId}/pdf`);need(a.ok()&&b.ok(),'authorized retained originals readable');const j=await a.body(),pdf=await b.body();need(pdf.subarray(0,5).equals(Buffer.from('%PDF-')),'real stored PDF');return{documentId:invoice.documentId,jsonSha256:hash(j),pdfSha256:hash(pdf),jsonBase64:j.toString('base64'),pdfBase64:pdf.toString('base64')};}
async function invariant(journal=baseline.journal,balance=baseline.balance){need(isDeepStrictEqual(await originals(),sourceProof),'transfer changed immutable originals');need(isDeepStrictEqual(await get(prefix+'journal'),journal),'unexpected canonical journal/UUID change');need(isDeepStrictEqual(await get(prefix+'trial-balance'),balance),'unexpected canonical balance change');const r=(await get(prefix+'invoices')).invoices.invoices.find(i=>i.documentId===invoice.documentId);need(r?.posted===false,'issued invoice must stay unposted');}
async function mail(){const value=await get(prefix+'invoices/email-deliveries');need(value.invoiceEmail.rows.length===1,'one exact pending mail row');const row=value.invoiceEmail.rows[0];need(row.recipient==='unknown@example.invalid'&&row.state==='unknown'&&row.pdfSha256===sourceProof.pdfSha256&&row.humanResolution!==true,'pending unknown not silently resolved');return row;}
async function unchangedUnknown(){need(isDeepStrictEqual(await mail(),unknown),'unknown SMTP history/stateHash changed across handoff');}
async function fenced(){step=scope.name+'-ordinary-writes-fenced';for(const[path,data]of[[prefix+'manual-journal',original],[prefix+'manual-journal',newJournal()],[prefix+'invoices/send-email',{invoiceDocumentId:invoice.documentId,to:'unknown@example.invalid',kind:'invoice',confirm:true}]]){const r=await http.post(base+path,{headers,data});const v=await r.json();need(r.status()>=400&&r.status()<500&&(v.code==='workspace_transfer_fenced'||scope===target&&path.endsWith('/send-email')&&r.status()===422&&v.code==='smtp_config_required'),'business write must fail transfer fence or explicit absent target SMTP config');}}
function newJournal(){const v=structuredClone(original);v.input.idempotencyKey='synthetic-transfer-new-destination-key';v.input.request.text='Synthetic accepted new destination activity';return v;}
async function navigate(){step=scope.name+'-unique-handoff-heading';await page.goto(base+'/flytning');await page.getByRole('heading',{name:'Flyt workspace',exact:true}).waitFor();}
async function review(title,button){const dialog=page.getByRole('dialog',{name:title,exact:true});await dialog.waitFor();need(await dialog.getByRole('button',{name:button,exact:true}).isDisabled(),'separate explicit transfer confirmation required');await dialog.getByLabel('Jeg bekræfter den gennemgåede handling',{exact:true}).check();return dialog;}
async function confirm(dialog,button,path,expected){const response=page.waitForResponse(r=>new URL(r.url()).pathname===path&&r.request().method()==='POST');response.catch(()=>{});await dialog.getByRole('button',{name:button,exact:true}).click();const r=await response;need(r.status()===200,'confirmed handoff receipt HTTP200');need(isDeepStrictEqual(r.request().postDataJSON(),expected),'exact reviewed handoff body');const value=await r.json();await dialog.waitFor({state:'hidden'});return value;}
async function client(kind,name,company=false){const file=join(work,scope.name+'-client.cookie');writeFileSync(file,(await page.context().cookies(base)).map(c=>`${c.name}=${c.value}`).join('; '),{mode:0o600,flag:'wx'});try{const common=['--server',base,'--allow-loopback-http','--cookie-file',file];let args,input;if(kind==='cli'){args=['remote',name,...common,...(company?['--company','synthetic']:[])];input='';}else{args=common;input=[{jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'synthetic-j06',version:'1'}}},{jsonrpc:'2.0',method:'notifications/initialized'},{jsonrpc:'2.0',id:2,method:'tools/call',params:{name,arguments:company?{company:'synthetic'}:{}}}].map(v=>JSON.stringify(v)).join('\n')+'\n';}const out=spawnSync(join(work,kind==='cli'?'rentemester-rust':'rentemester-mcp'),args,{input,encoding:'utf8',timeout:30000,maxBuffer:16*1024*1024});need(!out.error&&out.status===0,'pinned passive transfer client terminal');const v=kind==='cli'?JSON.parse(out.stdout):out.stdout.trim().split('\n').map(l=>JSON.parse(l)).find(v=>v.id===2)?.result?.structuredContent;need(v?.ok===true&&v.data,'canonical passive client response');return v.data;}finally{unlinkSync(file);}}
async function parity(){const value=await get('/api/workspace/transfer');need(isDeepStrictEqual(await client('cli','workspace-transfer-status'),value),'HTTP/CLI full transfer parity');need(isDeepStrictEqual(await client('mcp','workspace_transfer_status'),value),'HTTP/MCP full transfer parity');return value.transfer;}
async function setup(){browser=await chromium.launch({headless:true});await open(source);const who=await enroll();sourceOwnerId=who.user.id;step='ordinary-synthetic-seller-customer';await json(prefix+'company',{cvr:'87654321',address:'Synthetic Transfer Seller Street 1',postalCode:'1000',city:'Synthetic City',vatPeriodType:'quarter',paymentTermsDays:14},200,'PATCH');const buyer=(await post(prefix+'customers',{name:'Synthetic Transfer Buyer',address:'Synthetic Buyer Street 2',vatOrCvr:'DK12345678',defaultCurrency:'DKK',paymentTermsDays:14})).customer;need(buyer?.id,'normal customer receipt');const today=(await get(prefix+'scheduled-recurring')).scheduledRecurring.asOfDate;const input={issueDate:today,customerId:buyer.id,seller:{vatOrCvr:'DK87654321'},vatRatePercent:25,lines:[{description:'Synthetic transfer immutable invoice',quantity:1,unitPriceExVat:100}]};step='ordinary-preview-issue';const pdf=await http.post(base+prefix+'invoices/preview',{headers,data:input});need(pdf.ok()&&(await pdf.body()).subarray(0,5).equals(Buffer.from('%PDF-')),'normal real invoice preview');invoice=(await post(prefix+'invoices/issue',input)).invoice;sourceProof=await originals();for(const[accountNo,type,normalBalance]of[['1759','asset','debit'],['8759','equity','credit']])await post(prefix+'accounts',{accountNo,name:'Synthetic transfer '+accountNo,type,normalBalance});const j=(await get(prefix+'journal')).journal;need(j.entries.length===0,'clean source before chosen lost response');original={confirm:true,input:{format:'rentemester-manual-journal-post/1',expectedRuleVersion:j.nativePosting.ruleVersion,idempotencyKey:'synthetic-transfer-lost-source-response',request:{transactionDate:today,text:'Synthetic source accepted lost response',currency:'DKK',lines:[{accountNo:'1759',debitAmount:'12.34',creditAmount:'0.00'},{accountNo:'8759',debitAmount:'0.00',creditAmount:'12.34'}]}}};step='genuine-accepted-lost-response';const bytes=JSON.stringify(original),cookie=(await page.context().cookies(base)).map(c=>`${c.name}=${c.value}`).join('; ');await new Promise((ok,no)=>{const u=new URL(base),s=createConnection({host:u.hostname,port:Number(u.port)},()=>s.write(`POST ${prefix}manual-journal HTTP/1.1\r\nHost: ${u.host}\r\nOrigin: ${base}\r\nContent-Type: application/json\r\nCookie: ${cookie}\r\nContent-Length: ${Buffer.byteLength(bytes)}\r\nConnection: close\r\n\r\n${bytes}`));s.setTimeout(30000,()=>{s.destroy();no(Error('bounded original socket deadline'));});s.on('error',no);s.on('data',()=>{});s.on('end',()=>{s.destroy();ok();});});baseline={journal:await get(prefix+'journal'),balance:await get(prefix+'trial-balance')};need(baseline.journal.journal.entries.length===1&&baseline.journal.journal.entries[0].text===original.input.request.text,'canonical ledger proves accepted once without using reply');return{normalOwnerMfa:true,sourceOwnerId,invoice,sourceProof,original,baseline,discardedSuccessfulResponse:true};}
async function startUnknown(){use(source);step='source-mail-unique-heading';await page.goto(base+'/companies/synthetic/fakturaer');await page.getByRole('heading',{name:'Maillevering',exact:true}).waitFor();const panel=page.getByRole('region',{name:'Maillevering',exact:true});await panel.getByLabel('Faktura til mail',{exact:true}).selectOption(String(invoice.documentId));await panel.getByLabel('Modtagerens e-mail',{exact:true}).fill('unknown@example.invalid');await panel.getByLabel('Mailtype',{exact:true}).selectOption('invoice');step='source-mail-stored-pdf-review';const popup=page.waitForEvent('popup');await panel.getByRole('link',{name:'Åbn fakturaens gemte PDF',exact:true}).click();await(await popup).close();need((await originals()).pdfSha256===sourceProof.pdfSha256,'review immutable PDF');await panel.getByLabel('Jeg har gennemgået modtageren og den gemte PDF',{exact:true}).check();await panel.getByRole('button',{name:'Gennemgå mailafsendelse',exact:true}).click();const d=await review('Bekræft mailafsendelse','Send gennemgået mail én gang');step='source-unknown-send-inflight';const sent=page.waitForRequest(r=>new URL(r.url()).pathname===prefix+'invoices/send-email'&&r.method()==='POST');sent.catch(()=>{});pending=page.waitForResponse(r=>new URL(r.url()).pathname===prefix+'invoices/send-email').then(r=>r.status()).catch(()=>null);await d.getByRole('button',{name:'Send gennemgået mail én gang',exact:true}).click();await sent;return{requestStarted:true};}
async function killed(){use(source);step='source-kill-unknown-marker';await pending;const d=page.getByRole('dialog',{name:'Bekræft mailafsendelse',exact:true});await d.getByRole('button',{name:'Resultatet er afklaret',exact:true}).waitFor();need(await d.getByRole('button',{name:'Send gennemgået mail én gang',exact:true}).isDisabled(),'unknown browser write stays blocked');return{markerBlocked:true};}
async function freeze(){use(source);await page.context().clearCookies();need((await login()).user.id===sourceOwnerId,'same source owner after real crash');unknown=await mail();await invariant();const before=await transfer();need(before.mode==='active','source active before freeze');await navigate();step='review-source-freeze';const reason='Synthetic J06 source freeze after checking accepted journal and unknown SMTP';await page.getByLabel('Destination',{exact:true}).fill(target.base);await page.getByLabel('Begrundelse',{exact:true}).fill(reason);await page.getByRole('button',{name:'Gennemgå frysning',exact:true}).click();const d=await review('Bekræft frysning','Frys workspace');await confirm(d,'Frys workspace','/api/workspace/transfer/freeze',{expectedStateHash:before.stateHash,destination:target.base,reason,confirm:true});frozen=await parity();need(frozen.mode==='frozen'&&frozen.destination===target.base&&frozen.pendingEffects.some(e=>e.messageId===unknown.messageId&&e.state==='unknown'),'canonical frozen pending-effects identity');await fenced();await invariant();await unchangedUnknown();return{transfer:frozen,unknown};}
async function exportArchive(){use(source);await navigate();const before=await transfer();need(before.mode==='frozen','export only frozen');step='review-explicit-frozen-export';await page.getByRole('button',{name:'Gennemgå flyttearkiv',exact:true}).click();const d=await review('Bekræft flyttearkiv','Dan og hent flyttearkiv');const download=page.waitForEvent('download');const response=page.waitForResponse(r=>new URL(r.url()).pathname==='/api/workspace/transfer/export'&&r.request().method()==='POST');response.catch(()=>{});await d.getByRole('button',{name:'Dan og hent flyttearkiv',exact:true}).click();const r=await response;need(r.status()===200&&r.headers()['content-type']==='application/vnd.rentemester.native-backup+json','actual signed transfer response');need(isDeepStrictEqual(r.request().postDataJSON(),{expectedStateHash:before.stateHash,confirm:true}),'review exact export state');const file=await download;await file.saveAs(join(work,'transfer.native-backup.json'));const wire=readFileSync(join(work,'transfer.native-backup.json'));need(wire.length>0&&wire.length<=96*1024*1024&&hash(await r.body())===hash(wire),'actual user download exact response bytes');archiveHash=hash(wire);await d.waitFor({state:'hidden'});const after=await transfer();need(after.mode==='frozen'&&after.offeredArchiveHashes.includes(archiveHash),'offered full-wire archive hash persisted before receipt');step='cancel-after-offer-refusal';const cancel=await http.post(base+'/api/workspace/transfer/cancel',{headers,data:{expectedStateHash:after.stateHash,reason:'Synthetic cancel attempt after offered archive must refuse',confirm:true}});need(cancel.status()>=400&&cancel.status()<500,'offered archive cannot be cancelled');const still=await transfer();need(still.mode==='frozen'&&still.stateHash===after.stateHash,'cancel refusal cannot thaw/change source');await fenced();await invariant();return{sha256:archiveHash,bytes:wire.length,offered:after.offeredArchiveHashes,cancelAfterOfferRefused:true};}
async function stage(){await open(target);const who=await enroll();targetOwnerId=who.user.id;need(targetOwnerId!==sourceOwnerId,'fresh destination identity differs');const cookie=(await source.context.cookies(source.base)).map(c=>`${c.name}=${c.value}`).join('; ');const stale=await request.newContext({extraHTTPHeaders:{cookie,origin:target.base}});try{need((await stale.get(target.base+'/api/me')).status()===401,'old source cookie cannot authorize target');}finally{await stale.dispose();}step='target-staged-exact-challenge';const t=await parity();challenge=t.challenge;const sourceFact=frozen.history.find(e=>e.fact?.kind==='frozen'&&e.fact.transfer_id===frozen.transferId)?.fact;need(sourceFact?.source_instance_id,'source frozen historical instance identity');need(t.mode==='staged'&&challenge?.version===1&&challenge.transferId===frozen.transferId&&challenge.sourceInstanceId===sourceFact.source_instance_id&&challenge.archiveSha256===archiveHash&&challenge.destination===target.base&&challenge.workspaceId==='synthetic-native-transfer','exact source/archive/destination challenge binding');need(/^[0-9a-f]{32}$/.test(challenge.destinationInstanceId)&&challenge.destinationInstanceId!==challenge.sourceInstanceId,'fresh destination nonce');need(isDeepStrictEqual(challenge.companies,[{slug:'synthetic',ledgerUuid:baseline.journal.journal.ledgerUuid}]),'exact stable company ledger inventory');await fenced();await invariant();await unchangedUnknown();await navigate();need(isDeepStrictEqual(JSON.parse(await page.getByLabel('Målets udfordring',{exact:true}).inputValue()),challenge),'rendered target challenge matches canonical');return{targetOwnerId,transfer:t,oldSourceCookieRefused:true};}
async function mapReceipt(){use(target);const value=await get(prefix+'restore/receipts');need(isDeepStrictEqual(await client('cli','restored-receipts',true),value)&&isDeepStrictEqual(await client('mcp','restored_receipts',true),value),'full authorized receipt inventory client parity');const rows=value.restoredReceipts.rows;need(rows.length===1,'one imported original journal receipt');const r=rows[0];need(r.operation==='journal_post'&&r.sourcePrincipal.kind==='user'&&r.sourcePrincipal.subject===sourceOwnerId&&r.ledgerUuid===baseline.journal.journal.ledgerUuid&&r.keyHash===hash(Buffer.from(original.input.idempotencyKey))&&r.resultAvailable&&r.result?.entryId===baseline.journal.journal.entries[0].id&&r.result.entryNo===baseline.journal.journal.entries[0].entryNo&&!r.mapping,'exact original source accepted receipt, not a new principal alias');need(!JSON.stringify(value).includes(original.input.idempotencyKey),'inventory exposes only original key hash');await navigate();const section=page.getByRole('region',{name:'Importerede kvitteringer',exact:true});await section.getByLabel('Virksomhed til kvitteringer',{exact:true}).selectOption('synthetic');await section.getByRole('article',{name:`Kvittering ${r.receiptId}`,exact:true}).waitFor();const reason='Synthetic J06 owner reviewed original accepted lost-response journal receipt';await section.getByLabel('Begrundelse for tilknytning',{exact:true}).fill(reason);await section.getByRole('button',{name:`Gennemgå tilknytning af kvittering ${r.receiptId}`,exact:true}).click();step='review-exact-imported-receipt';const d=await review('Bekræft tilknytning af kvittering','Tilknyt kvittering til min bruger');const result=await confirm(d,'Tilknyt kvittering til min bruger',prefix+'restore/receipts/reconcile',{receiptId:r.receiptId,expectedStateHash:r.stateHash,reason,confirm:true});mappedReceipt=result.restoredReceipts.rows.find(x=>x.receiptId===r.receiptId);need(mappedReceipt?.mapping?.targetUserId===targetOwnerId&&mappedReceipt.mapping.reason===reason&&mappedReceipt.mapping.archiveSha256===r.archiveSha256&&mappedReceipt.mapping.restoreBoundaryId===r.restoreBoundaryId,'mapping only current new human with exact retained boundary');for(const key of['sourcePrincipal','keyHash','payloadHash','ledgerUuid','result','receiptId','archiveSha256','restoreBoundaryId'])need(isDeepStrictEqual(mappedReceipt[key],r[key]),'mapping must preserve original '+key);need((await transfer()).mode==='staged','mapping does not activate');await fenced();await invariant();return{before:r,after:mappedReceipt};}
async function releaseSource(){use(source);await navigate();const t=await transfer();need(t.mode==='frozen'&&t.offeredArchiveHashes.includes(archiveHash),'source still frozen before target release');const reason='Synthetic independent source owner checked destination nonce and archive';step='source-independent-challenge-review';await page.getByLabel('Begrundelse',{exact:true}).fill(reason);await page.getByLabel('Målets udfordring (JSON)',{exact:true}).fill(JSON.stringify(challenge));await page.getByLabel('Jeg har kontrolleret mål og udfordring via en uafhængig kanal',{exact:true}).check();await page.getByRole('button',{name:'Gennemgå frigivelse',exact:true}).click();const d=await review('Bekræft frigivelse af kilden','Frigiv til dette mål');await confirm(d,'Frigiv til dette mål','/api/workspace/transfer/release',{expectedStateHash:t.stateHash,challenge,reason,confirm:true});const moved=await parity();signedRelease=moved.release;need(moved.mode==='moved'&&typeof signedRelease==='string'&&signedRelease.length>0,'source moved with retained signed release');need(await page.getByLabel('Signeret frigivelse',{exact:true}).inputValue()===signedRelease,'rendered retained release exact');await fenced();await invariant();await unchangedUnknown();use(target);need((await transfer()).mode==='staged','source release is not target activation');return{source:moved,targetStillStaged:true};}
async function activateTarget(){use(target);await navigate();const t=await transfer();need(t.mode==='staged','target remains staged before activation');const reason='Synthetic new owner reviewed separately trusted source release and target';step='target-source-signature-review';await page.getByLabel('Begrundelse',{exact:true}).fill(reason);await page.getByLabel('Signeret frigivelse fra kilden',{exact:true}).fill(signedRelease);await page.getByLabel('Kildens offentlige nøgle er konfigureret og kontrolleret separat',{exact:true}).check();await page.getByRole('button',{name:'Gennemgå aktivering',exact:true}).click();const d=await review('Bekræft aktivering af målet','Aktivér dette mål');await confirm(d,'Aktivér dette mål','/api/workspace/transfer/activate',{expectedStateHash:t.stateHash,release:signedRelease,reason,confirm:true});const active=await parity();need(active.mode==='active','only target canonical status proves active');await invariant();await unchangedUnknown();return{target:active};}
async function replay(){step='target-exact-mapped-original-replay';const result=await post(prefix+'manual-journal',original);need(result.ok===true&&result.entryId===mappedReceipt.result.entryId&&result.entryNo===mappedReceipt.result.entryNo&&result.entryHash===mappedReceipt.result.entryHash&&result.receipt?.replayed===true&&result.receipt.receiptId===mappedReceipt.receiptId,'real original result/receipt replay without apply');const rec=result.receipt.reconciliation;need(rec?.archiveSha256===mappedReceipt.archiveSha256&&rec.restoreBoundaryId===mappedReceipt.restoreBoundaryId&&isDeepStrictEqual(rec.sourcePrincipal,mappedReceipt.sourcePrincipal)&&rec.mappedUserId===targetOwnerId,'explicit mapped receipt provenance, not historical authority');return result;}
async function activeWrites(){use(target);await replay();await invariant();step='target-conflicting-old-key-refusal';const changed=structuredClone(original);changed.input.request.text+=' changed payload';const conflict=await http.post(base+prefix+'manual-journal',{headers,data:changed});const cv=await conflict.json();need(cv.ok===false,'changed source payload must refuse');need(conflict.status()>=400&&conflict.status()<500||cv.code==='IDEMPOTENCY_CONFLICT','known conflict, no ambiguous good result');await invariant();step='target-explicit-new-intent';const added=await post(prefix+'manual-journal',newJournal());need(added.ok===true&&added.receipt?.replayed===false,'new key posts one fresh explicit destination intent');finalTargetJournal=await get(prefix+'journal');finalTargetBalance=await get(prefix+'trial-balance');need(finalTargetJournal.journal.entries.length===2&&finalTargetJournal.journal.entries.some(e=>e.text===original.input.request.text)&&finalTargetJournal.journal.entries.some(e=>e.text===newJournal().input.request.text),'exactly source and new destination entries');await replay();await invariant(finalTargetJournal,finalTargetBalance);await unchangedUnknown();const current=await parity();use(source);need((await transfer()).mode==='moved','source stays moved while target writes');await fenced();await invariant();await unchangedUnknown();return{target:current,journal:finalTargetJournal,balance:finalTargetBalance,sourceStillMoved:true};}
async function restarts(){use(source);await page.context().clearCookies();need((await login()).user.id===sourceOwnerId,'same source MFA identity after reopen');const s=await parity();need(s.mode==='moved'&&s.release===signedRelease,'source fence/release durable across reopen');await fenced();await invariant();await unchangedUnknown();use(target);await page.context().clearCookies();need((await login()).user.id===targetOwnerId,'same fresh target MFA identity after reopen');const t=await parity();need(t.mode==='active','target activation durable');await replay();await invariant(finalTargetJournal,finalTargetBalance);await unchangedUnknown();const rows=(await get(prefix+'restore/receipts')).restoredReceipts.rows;need(rows.some(r=>r.receiptId===mappedReceipt.receiptId&&r.mapping?.targetUserId===targetOwnerId),'exact receipt mapping durable');return{source:s,target:t,targetJournal:finalTargetJournal,unknownStillUnresolved:true};}
for await(const line of createInterface({input:process.stdin,crlfDelay:Infinity})){try{const c=JSON.parse(line);let data;if(c.action==='setup')data=await setup();else if(c.action==='start-unknown')data=await startUnknown();else if(c.action==='killed')data=await killed();else if(c.action==='freeze')data=await freeze();else if(c.action==='export')data=await exportArchive();else if(c.action==='stage')data=await stage();else if(c.action==='map')data=await mapReceipt();else if(c.action==='release')data=await releaseSource();else if(c.action==='activate')data=await activateTarget();else if(c.action==='active-writes')data=await activeWrites();else if(c.action==='restarts')data=await restarts();else if(c.action==='finish'){await browser?.close();console.log(JSON.stringify({ok:true,data:{finished:true}}));break;}else throw Error('unknown action');console.log(JSON.stringify({ok:true,data}));}catch(error){let ui=null;try{ui=await page?.evaluate(()=>({pathname:location.pathname,headings:[...document.querySelectorAll('h1,h2')].map(e=>e.textContent.slice(0,100)),buttons:[...document.querySelectorAll('button')].map(e=>e.textContent.slice(0,100)),alerts:document.querySelectorAll('[role=alert]').length}));}catch{}console.log(JSON.stringify({ok:false,step,detail,errorType:error?.name,trace,ui}));await browser?.close().catch(()=>{});process.exitCode=1;break;}}
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
    args = parser.parse_args()
    if os.uname().sysname != 'Darwin' or os.getuid() == 0 or os.getuid() != os.geteuid():
        parser.error('Ordinary macOS fixture owner required')
    if args.evidence.exists():
        parser.error('New evidence path required')
    os.umask(0o077)
    parent = (Path.home() / 'Library/Application Support').resolve(strict=True)
    work = parent / (PREFIX + uuid.uuid4().hex)
    identity = None
    profile_ids = {}
    children = {'source': None, 'target': None}
    logs = {'source': None, 'target': None}
    controller = smtp = None
    lock = threading.Lock()
    queued = threading.Event()
    release = threading.Event()
    smtp_secret = secrets.token_urlsafe(32)
    stats = {'connections': 0, 'rcptAttempts': {}, 'messages': [], 'errors': []}
    report = {'status': 'running', 'sourceRevision': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=REPO, text=True).strip(), 'runnerSha256': sha(Path(__file__)), 'browserHelperSha256': hashlib.sha256(BROWSER.encode()).hexdigest(), 'noOfflineIdentitySeeds': True, 'externalMailForwarded': False, 'linuxIsolationQualified': False, 'hostileOrPreFreezeCloneProtectionQualified': False, 'postCutoverRollbackQualified': False, 'otherKeyedFamiliesQualified': False, 'installedNativeGuiQualified': False, 'nonemptySchedulerAuthorityInThisFixture': False, 'serverPids': {'source': [], 'target': []}, 'actions': [], 'results': {}}
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

        def profile_owned(name):
            path = work / name
            if not owned() or path.is_symlink() or not path.is_dir():
                return False
            st = path.stat()
            return (st.st_dev, st.st_ino, st.st_uid) == (*profile_ids[name], os.getuid())

        def progress(action):
            report['currentAction'] = action
            save()

        def stop(name, crash=False):
            child = children[name]
            if child is not None:
                if child.poll() is None:
                    child.kill() if crash else child.terminate()
                try:
                    child.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait(timeout=5)
                report.setdefault('serverExits', []).append({'name': name, 'pid': child.pid, 'code': child.returncode, 'intentionalCrash': crash})
                if crash:
                    report['crashExitCode'] = child.returncode
                children[name] = None
            if logs[name]:
                logs[name].close()
                logs[name] = None
            save()

        def command(action, timeout=240):
            progress('browser:' + action)
            controller.stdin.write(json.dumps({'action': action}) + '\n')
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
                save()
                raise RuntimeError('Browser assertion failed at ' + result.get('step', action))
            report['actions'].append({'action': action, 'status': 'complete'})
            report['results'][action] = result['data']
            save()
            return result['data']

        def snapshot():
            with lock:
                return json.loads(json.dumps(stats))

        def unchanged_smtp(expected):
            current = snapshot()
            if current != expected:
                raise RuntimeError('Transfer/read/replay/restart caused SMTP activity or changed queue')

        def sql_snapshot(name):
            if children[name] is not None or not profile_owned(name):
                raise RuntimeError('Read-only SQL observer requires exact stopped owned profile')
            path = work / name / 'workspace/synthetic/data/ledger.sqlite'
            if path.is_symlink() or not path.is_file():
                raise RuntimeError('Exact owned synthetic ledger required')
            db = sqlite3.connect(path.as_uri() + '?mode=ro', uri=True)
            try:
                return {'tombstones': db.execute('SELECT * FROM mutation_idempotency_tombstones ORDER BY id').fetchall(), 'outcomes': db.execute('SELECT * FROM mutation_idempotency_outcomes ORDER BY tombstone_id').fetchall(), 'emailSuccessCount': db.execute('SELECT COUNT(*) FROM email_send_log').fetchone()[0], 'journalCount': db.execute('SELECT COUNT(*) FROM journal_entries').fetchone()[0]}
            finally:
                db.close()

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
            for name in children:
                path = work / name
                path.mkdir(mode=0o700)
                profile_ids[name] = (path.stat().st_dev, path.stat().st_ino)
            report['ownedProfiles'] = {name: str(work / name) for name in children}
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
            progress('operator:private-tls-and-signing-material')
            ca, cert, private = work / 'ca.pem', work / 'smtp.pem', work / 'smtp.key'
            (work / 'smtp.ext').write_text('basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\nsubjectAltName=DNS:localhost,IP:127.0.0.1\n')
            commands = [
                ['openssl', 'req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', str(work / 'ca.key'), '-out', str(ca), '-days', '1', '-subj', '/CN=Synthetic Transfer SMTP CA', '-addext', 'basicConstraints=critical,CA:TRUE', '-addext', 'keyUsage=critical,keyCertSign,cRLSign'],
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
            ports = {'source': port(), 'target': port()}
            if len(set(ports.values())) != 2:
                raise RuntimeError('Distinct source/target owned ports required')
            bases = {name: f'http://127.0.0.1:{number}' for name, number in ports.items()}
            report['origins'] = bases
            key = lambda: base64.urlsafe_b64encode(secrets.token_bytes(32)).decode().rstrip('=')
            subprocess.run(['/opt/homebrew/bin/bun', '-e', "const c=require('node:crypto'),f=require('node:fs'),k=c.generateKeyPairSync('ed25519'),s=k.privateKey.export({format:'der',type:'pkcs8'}).subarray(-32),p=k.publicKey.export({format:'der',type:'spki'}).subarray(-32);f.writeFileSync(process.argv[1],Buffer.concat([Buffer.from('3051020101300506032b657004220420','hex'),s,Buffer.from('812100','hex'),p]),{mode:0o600});f.writeFileSync(process.argv[2],p,{mode:0o600});", str(work / 'source-signing.der'), str(work / 'source-trusted.pub')], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            configs = {}
            for name in children:
                configs[name] = {'runtimeProfile': 'local-user-macos', 'root': str(work / name / 'workspace'), 'brokerUid': os.getuid(), 'workspace': 'synthetic-native-transfer', 'companySlug': 'synthetic', 'companyName': 'Synthetic Transfer Company', 'cookieKey': key(), 'mfaKey': key(), 'operatorActors': ['system:synthetic-installer'], 'firstOwner': {'actor': 'system:synthetic-installer', 'email': 'owner@example.invalid' if name == 'source' else 'target-owner@example.invalid', 'name': 'Synthetic ' + name + ' owner', 'password': 'synthetic-transfer-owner-password-123'}, 'accounts': []}
            configs['source']['backupSigningKeyFile'] = str(work / 'source-signing.der')
            configs['source']['invoiceMail'] = {'synthetic': {'host': 'localhost', 'port': smtp.server_address[1], 'fromAddress': 'sales@example.invalid', 'fromName': 'Synthetic Seller', 'username': 'fixture-smtp-user', 'password': smtp_secret, 'tls': 'implicit', 'trustedCaFile': str(ca)}}
            configs['target'].update({'restoreActor': 'system:synthetic-installer', 'transferTrustedKeyFile': str(work / 'source-trusted.pub')})

            def write(name):
                if not profile_owned(name):
                    raise RuntimeError('Owned profile identity changed')
                (work / name / 'server.json').write_text(json.dumps(configs[name]))

            def start(name, create=False, stage=False):
                if children[name] is not None or not profile_owned(name):
                    raise RuntimeError('Exact prior child must stop before owned reopen')
                progress('operator:start-' + name + ('-stage' if stage else ''))
                argv = [str(pinned['server_binary']), '--config', str(work / name / 'server.json'), '--web-dir', str(web), '--listen', f'127.0.0.1:{ports[name]}']
                if create:
                    argv.append('--create')
                if stage:
                    if os.path.lexists(work / name / 'workspace'):
                        raise RuntimeError('Fresh staged target required; refuse overwrite')
                    argv += ['--restore-backup', str(work / 'transfer.native-backup.json'), '--trusted-backup-key', str(work / 'source-trusted.pub'), '--confirm-target', configs[name]['root'], '--restore-transfer']
                logs[name] = (work / name / 'server.log').open('ab')
                children[name] = subprocess.Popen(argv, cwd=REPO, stdin=subprocess.DEVNULL, stdout=logs[name], stderr=logs[name])
                report['serverPids'][name].append(children[name].pid)
                save()
                for _ in range(300):
                    if children[name].poll() is not None:
                        raise RuntimeError('Owned ' + name + ' server exited before readiness')
                    try:
                        with urllib.request.urlopen(bases[name] + '/api/health', timeout=.2) as r:
                            if json.load(r).get('workspaceReady'):
                                return
                    except (OSError, ValueError):
                        pass
                    time.sleep(.1)
                raise RuntimeError('Owned ' + name + ' readiness deadline')

            write('source')
            write('target')
            start('source', create=True)
            script = work / 'browser.mjs'
            script.write_text(BROWSER)
            controller = subprocess.Popen(['/opt/homebrew/bin/bun', str(script)], cwd=REPO, env={**os.environ, 'RM_TRANSFER_SOURCE': bases['source'], 'RM_TRANSFER_TARGET': bases['target'], 'RM_TRANSFER_WORK': str(work), 'RM_TRANSFER_PLAYWRIGHT': str((REPO / 'node_modules/@playwright/test/index.mjs').resolve(strict=True))}, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, bufsize=1, start_new_session=True)
            report['browserControllerPid'] = controller.pid
            setup = command('setup')
            if snapshot()['connections'] != 0:
                raise RuntimeError('Source setup/read unexpectedly opened SMTP')
            command('start-unknown')
            if not queued.wait(7):
                raise RuntimeError('No durable queued DATA before transport deadline')
            queued_stats = snapshot()
            killed_at = time.monotonic()
            stop('source', crash=True)
            elapsed = killed_at - queued_stats['messages'][-1]['queuedMonotonic']
            if elapsed >= 7 or report['crashExitCode'] != -signal.SIGKILL:
                raise RuntimeError('Source not killed before native SMTP terminal deadline')
            report['crashAfterQueueSeconds'] = elapsed
            command('killed', timeout=60)
            before = sql_snapshot('source')
            if before['emailSuccessCount'] != 0 or before['journalCount'] != 1 or len(before['tombstones']) != 1 or len(before['outcomes']) != 1:
                raise RuntimeError('Exact accepted lost reply and unresolved mail source required')
            report['sourceReceiptsBeforeTransfer'] = before
            configs['source'].pop('firstOwner')
            write('source')
            start('source')
            freeze = command('freeze')
            smtp_baseline = snapshot()
            if smtp_baseline['errors'] or smtp_baseline['connections'] != 1 or smtp_baseline['rcptAttempts'] != {'unknown@example.invalid': 1} or len(smtp_baseline['messages']) != 1:
                raise RuntimeError('Exactly one real unknown SMTP attempt required')
            message = smtp_baseline['messages'][0]
            if message['pdfSha256'] != setup['sourceProof']['pdfSha256'] or message['pdfCount'] != 1 or message['finalAck'] or message['messageId'] != freeze['unknown']['messageId'] or message['bodySha256'] != freeze['unknown']['bodySha256']:
                raise RuntimeError('Queued immutable MIME/PDF/source unknown evidence mismatch')
            archive = command('export')
            unchanged_smtp(smtp_baseline)
            progress('operator:independent-signed-transfer-verification')
            path = work / 'transfer.native-backup.json'
            if sha(path) != archive['sha256']:
                raise RuntimeError('Downloaded wire archive changed')
            verified = subprocess.run([str(pinned['server_binary']), '--verify-backup', str(path), '--trusted-backup-key', str(work / 'source-trusted.pub')], cwd=REPO, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=60)
            if verified.returncode != 0:
                raise RuntimeError('Independent transfer signature/inventory verification refused')
            report['trustedSourcePublicKeySha256'] = sha(work / 'source-trusted.pub')
            report['independentArchiveVerification'] = True
            save()
            start('target', stage=True)
            command('stage')
            unchanged_smtp(smtp_baseline)
            command('map')
            unchanged_smtp(smtp_baseline)
            command('release')
            unchanged_smtp(smtp_baseline)
            command('activate')
            unchanged_smtp(smtp_baseline)
            command('active-writes')
            unchanged_smtp(smtp_baseline)
            stop('source')
            stop('target')
            for name in children:
                configs[name].pop('firstOwner', None)
                write(name)
                start(name)
            command('restarts')
            unchanged_smtp(smtp_baseline)
            command('finish', timeout=20)
            controller.stdin.close()
            controller.wait(timeout=10)
            if controller.returncode != 0:
                raise RuntimeError('Controller nonzero terminal')
            controller = None
            stop('source')
            stop('target')
            final_source = sql_snapshot('source')
            final_target = sql_snapshot('target')
            if final_source != before:
                raise RuntimeError('Moved source receipt tables/business effects changed')
            if final_target['journalCount'] != 2 or final_target['emailSuccessCount'] != 0 or final_target['tombstones'][:1] != before['tombstones'] or final_target['outcomes'][:1] != before['outcomes'] or len(final_target['tombstones']) != 2:
                raise RuntimeError('Destination replaced original receipts or duplicated accounting/mail effects')
            report['finalSql'] = {'source': final_source, 'target': final_target}
            secret = smtp_secret.encode()
            def scan(path):
                carry = b''
                with path.open('rb') as stream:
                    while chunk := stream.read(1024 * 1024):
                        data = carry + chunk
                        if secret in data:
                            raise RuntimeError('SMTP credential entered business/archive evidence')
                        carry = data[-len(secret):]
            for name in children:
                for path in (work / name / 'workspace').rglob('*'):
                    if path.is_file() and not path.is_symlink():
                        scan(path)
            scan(work / 'transfer.native-backup.json')
            if smtp_secret in json.dumps(report):
                raise RuntimeError('SMTP credential entered report')
            report.update({'status': 'passed', 'managedTransferScenarioQualified': True, 'sourceMovedTargetActiveAfterRestart': True, 'acceptedLostResponseMappedWithoutNewBooking': True, 'unknownSmtpPreservedNoResend': True, 'smtpSecretAbsentFromBusinessWorkspacesAndArchive': True, 'smtp': smtp_baseline, 'currentAction': 'complete'})
            save()
            print('PASS: ordinary MFA managed source→staged target handoff, exact lost-response receipt replay and no SMTP replay; ' + str(args.evidence))
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
            for name in children:
                stop(name)
            if smtp:
                smtp.shutdown()
                smtp.server_close()
            report['ownedProcessesStopped'] = all(child is None for child in children.values())
            if owned() and all(profile_owned(name) for name in children):
                shutil.rmtree(work)
                report['ownedProfilesCleaned'] = {'source': True, 'target': True}
            else:
                report['ownedProfilesCleaned'] = {name: False for name in children}
            save()


if __name__ == '__main__':
    main()
