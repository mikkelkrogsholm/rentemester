#!/usr/bin/env python3
"""Owned direct-bank purchase to payable correction proof; no builds.

Normal owner email/password/MFA, real original and archived bank CSV, reviewed
Payables correction, canonical as-of/financial receipts, native clients and restart.
Only a successful signed export can enter the source-free fresh-owner branch.
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
import socket
import ssl
import subprocess
import threading
import time
import uuid
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
PREFIX = 'Rentemester-Synthetic-Direct-Bank-'
BROWSER = r'''


import {createHmac,createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {createInterface} from 'node:readline';
import {readFileSync,readdirSync,writeFileSync,unlinkSync} from 'node:fs';
import {join} from 'node:path';
import {isDeepStrictEqual} from 'node:util';
const {chromium}=await import(process.env.RM_DIRECT_PLAYWRIGHT);
const base=process.env.RM_DIRECT_BASE,work=process.env.RM_DIRECT_WORK;
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
const prefix='/api/companies/synthetic/',correctionBase=prefix+'payables/direct-bank-correction',hash=b=>createHash('sha256').update(b).digest('hex');
let latest,billDate,bankDate,year,document,bankTransaction,input,original,reviewed,confirmed,applyBody,stable;
async function call(page,path,body,method='POST'){const r=await page.request.fetch(base+path,{method,headers:{origin:base,'content-type':'application/json'},data:body});let value;try{value=await r.json();}catch{value=null;}return{status:r.status(),value};}
async function post(path,body,method='POST'){const r=await call(ownerPage,path,body,method);need(r.status===200&&r.value?.ok===true,`canonical ${path} HTTP${r.status} ${r.value?.code??''}`);return r.value;}
async function read(path){const r=await ownerPage.request.get(base+path);need(r.status()===200,`canonical ${path} HTTP${r.status()}`);return r.json();}
async function authenticate(){browser=await chromium.launch({headless:true});ownerPage=await newPage();step='ordinary-owner-email-verification';await ownerPage.goto((await mail('verification',owner.email)).actionUrl);ownerId=(await enroll(ownerPage,owner)).user.id;return{ownerId,normalVerifiedMfa:true};}
async function originals(){const r=await ownerPage.request.get(base+prefix+`documents/${document.id}/file`);need(r.ok(),'authorized immutable invoice readable');need(hash(await r.body())===original.sha256,'exact original bytes unchanged');const d=(await read(prefix+'documents')).documents.documents.find(d=>d.id===document.id);const options=(await read(prefix+`documents/${document.id}/booking-options`)).options.document;need(d?.invoiceDate===billDate&&d.amountIncVat===125&&options.vatAmount===25,'invoice date and source VAT unchanged');return{documentId:document.id,sha256:original.sha256,invoiceDate:d.invoiceDate,amountIncVat:d.amountIncVat,vatAmount:options.vatAmount};}
async function reports(){const q='?year='+encodeURIComponent(year);return{journal:await read(prefix+'journal'+q),trialBalance:await read(prefix+'trial-balance'+q),income:await read(prefix+'income-statement'+q),vat:await read(prefix+'vat'+q),bank:await read(prefix+'bank'+q)};}
async function intake(){diagnosticPage=ownerPage;step='ordinary-profile-explicit-chart-roles';await post(prefix+'company',{cvr:'87654321',address:'Synthetic Correction Street 1',postalCode:'1000',city:'Synthetic City',vatPeriodType:'quarter',paymentTermsDays:14},'PATCH');for(const[accountNo,type,normalBalance]of[['1800','asset','debit'],['3000','expense','debit'],['4700','vat','debit'],['7000','liability','credit']])await post(prefix+'accounts',{accountNo,name:'Synthetic Correction '+accountNo,type,normalBalance});for(const[role,accountNo]of[['bank','1800'],['input_vat','4700'],['creditors','7000']])await post(prefix+'account-roles',{role,accountNo,confirm:true});await post(prefix+'bank-accounts',{slug:'synthetic-correction-bank',name:'Synthetic Correction Bank',currency:'DKK',ledgerAccountNo:'1800'});latest=(await read(prefix+'scheduled-recurring')).scheduledRecurring.asOfDate;const[y,m]=latest.split('-').map(Number);billDate=new Date(Date.UTC(y,m-2,10)).toISOString().slice(0,10);bankDate=new Date(Date.UTC(y,m-2,20)).toISOString().slice(0,10);year=billDate.slice(0,4);need(billDate<bankDate&&bankDate<=latest,'chosen independent past invoice and bank dates');step='ordinary-immutable-invoice-intake';const bytes=Buffer.from(`Synthetic direct purchase correction\nInvoice SYN-DIRECT-58\nDate ${billDate}\nDKK125 VAT25\n`),filename='synthetic-direct-bank-invoice.txt';await post(prefix+'documents/ingest',{confirm:true,fileName:filename,fileBase64:bytes.toString('base64'),metadata:{source:'Synthetic correction ordinary intake',documentType:'purchase_sale',issueDate:billDate,invoiceNo:'SYN-DIRECT-58',deliveryDescription:'Synthetic supplies',amountIncVat:125,vatAmount:25,currency:'DKK',sender:{name:'Synthetic correction supplier',address:'Synthetic Supplier Street 1',vatOrCvr:'DK12345678'},recipient:{name:'Synthetic Correction Company',address:'Synthetic Correction Street 1',vatOrCvr:'DK87654321'}}});const docs=(await read(prefix+'documents')).documents.documents.filter(d=>d.filename===filename);need(docs.length===1,'one real canonical original');document=docs[0];original={filename,sha256:hash(bytes)};await originals();step='ordinary-archived-later-outgoing-bank-intake';const csv=`dato;tekst;beløb;valuta;saldo\n${bankDate};SYN-DIRECT-58;-125,00;DKK;-125,00\n`;const imported=await post(prefix+'bank/import',{confirm:true,csvContent:csv,account:'synthetic-correction-bank'});need(imported.import.imported===1&&imported.import.skippedDuplicates===0,'one actual archived CSV source');const bank=(await read(prefix+'bank?year='+year)).bank;need(bank.transactions.length===1&&bank.unmatchedCount===1&&bank.actualBalance===-125&&bank.bookedBalance===0,'independent bank closing balance before posting');bankTransaction=bank.transactions[0];need(bankTransaction.date===bankDate&&bankTransaction.amount===-125,'later bank source exact');return{latest,billDate,bankDate,year,document,original,bankTransaction,csvSha256:hash(Buffer.from(csv))};}
async function direct(){step='ordinary-direct-bank-purchase';const posted=await post(prefix+'documents/book-expense',{confirm:true,documentId:document.id,bankTransactionId:bankTransaction.id,expenseAccountNo:'3000',vatTreatment:'standard'});baseline=await reports();need(baseline.journal.journal.entries.length===1&&baseline.journal.journal.entries[0].documentId===document.id&&baseline.journal.journal.entries[0].date===bankDate,'real direct purchase uses later bank date');need(baseline.income.incomeStatement.totalExpense===100&&baseline.income.incomeStatement.result===-100&&baseline.vat.vat.inputVat===25&&baseline.vat.vat.payable===-25&&baseline.bank.bank.bookedBalance===-125&&baseline.bank.bank.actualBalance===-125&&baseline.bank.bank.matchedCount===1,'original expense VAT and bank reference figures');await originals();return{posted,baseline};}
async function asOf(date){return read(prefix+'payables?'+new URLSearchParams({status:'all',asOf:date}));}
function correctionReceipt(value){const r=value.correction;need(value.ok===true&&r?.ok===true&&Array.isArray(r.errors)&&r.errors.length===0,'canonical source correction receipt');for(const key of['id','payableId','paymentId','settlementJournalEntryId'])need(Number.isSafeInteger(r[key])&&r[key]>0,'real persisted '+key);return r;}
async function unchangedDirect(){need(isDeepStrictEqual(await reports(),baseline),'refusal changed original direct journal or reports');await originals();}
async function refusals(){diagnosticPage=ownerPage;step='source-backed-readonly-plan-and-invalid-input-refusals';input={documentId:document.id,bankTransactionId:bankTransaction.id,billDate,dueDate:bankDate,expenseAccountNo:'3000',vatTreatment:'standard'};const planResponse=await post(correctionBase+'/plan',input);reviewed=planResponse.plan;need(reviewed.schemaVersion==='rentemester-direct-bank-purchase-payable-correction-v1'&&reviewed.documentId===document.id&&reviewed.documentHash===original.sha256&&reviewed.billDate===billDate&&reviewed.bankDate===bankDate&&reviewed.bankAmount===125&&reviewed.originalJournalEntryId===baseline.journal.journal.entries[0].id&&reviewed.originalJournalDate===bankDate&&/^[a-f0-9]{64}$/.test(reviewed.originalJournalHash)&&/^[a-f0-9]{64}$/.test(reviewed.planHash),'real immutable sourcev1 plan');need(isDeepStrictEqual(await post(correctionBase+'/plan',input),planResponse),'repeat readonly plan deterministic');await unchangedDirect();for(const[path,body,status]of[[correctionBase+'/plan',{...input,actor:'user:forged'},400],[correctionBase+'/plan',{...input,billDate:bankDate},422],[correctionBase+'/apply',{...input,planHash:'0'.repeat(64),reason:'Synthetic stale reviewed hash must refuse without effects',confirm:true,idempotencyKey:'synthetic-direct-stale-hash'},422]]){const refused=await call(ownerPage,path,body);need(refused.status===status&&refused.value?.ok===false,'definite invalid or stale input refusal');await unchangedDirect();}
step='ordinary-closed-period-admission';const bounds={periodStart:bankDate,periodEnd:bankDate,confirm:true},packet=(await read(prefix+'periods/close-readiness?'+new URLSearchParams({from:bankDate,to:bankDate}))).packet;let closedPeriodRefusal={qualified:false,reason:'canonical readiness blocks an unrelated period-close prerequisite',packet};if(packet.blockers===0){const review=(await post(prefix+'periods/close-review',bounds)).review;const closed=(await post(prefix+'periods/close',{...bounds,reviewId:review.id,packetHash:review.packet.hash})).period;need(closed.effectiveStatus==='closed','normal reviewed close establishes real period gate');const locked=await call(ownerPage,correctionBase+'/apply',{...input,planHash:reviewed.planHash,reason:'Synthetic correction must not write into closed source period',confirm:true,idempotencyKey:'synthetic-direct-closed-period'});need(locked.status===422&&locked.value?.ok===false,'current period fence refuses correction');need((await reports()).journal.journal.entries.length===1,'closed-period refusal created no reversal or payable');const reopened=(await post(prefix+'periods/reopen',{...bounds,reason:'Synthetic owner explicitly reopened after validating correction evidence'})).period;need(reopened.effectiveStatus==='open','normal justified reopen before intentional correction');closedPeriodRefusal={qualified:true,periodId:closed.id,refusal:locked.value};}return{initialPlan:reviewed,staleHashAndCallerActorRefused:true,invoiceDateChangeRefused:true,closedPeriodRefusal};}
async function correction(){diagnosticPage=ownerPage;step='existing-payables-native-candidate';const before=await asOf(bankDate);const candidates=before.payables.directBankCorrectionCandidates;need(Array.isArray(candidates)&&candidates.length===1&&candidates[0].documentId===document.id&&candidates[0].bankTransactionId===bankTransaction.id&&candidates[0].invoiceDate===billDate&&candidates[0].bankDate===bankDate,'real direct-linked correction candidate');need(!before.payables.unregisteredDocuments.some(d=>d.id===document.id),'direct purchase not misrepresented as ordinary register candidate');await ownerPage.goto(base+'/companies/synthetic/leverandoerfaktura');await ownerPage.getByRole('heading',{name:'Leverandørfakturaer',exact:true}).waitFor();await ownerPage.getByRole('button',{name:'Ret direkte bankkøb',exact:true}).click();const d=ownerPage.getByRole('dialog',{name:'Ret direkte bankkøb',exact:true});await d.waitFor();await d.getByLabel('Bilag',{exact:true}).selectOption(`${document.id}:${bankTransaction.id}`);need(await d.getByLabel('Fakturadato',{exact:true}).inputValue()===billDate&&await d.getByLabel('Fakturadato',{exact:true}).getAttribute('readonly')!==null,'immutable invoice date shown readonly');need(await d.getByLabel('Banktransaktions-id',{exact:true}).inputValue()===String(bankTransaction.id),'actual associated bank, no invented caller link');await d.getByLabel('Forfaldsdato',{exact:true}).fill(bankDate);await d.getByLabel('Udgiftskonto',{exact:true}).selectOption('3000');await d.getByLabel('Momsbehandling',{exact:true}).selectOption('standard');await d.getByLabel('Begrundelse',{exact:true}).fill('Synthetic owner reviewed original purchase and shifted invoice versus settlement dates');const planning=ownerPage.waitForResponse(r=>new URL(r.url()).pathname===correctionBase+'/plan'&&r.request().method()==='POST');planning.catch(()=>{});await d.getByRole('button',{name:'Opret plan',exact:true}).click();const pr=await planning;need(pr.status()===200&&isDeepStrictEqual(pr.request().postDataJSON(),input),'exact reviewed native plan request');reviewed=(await pr.json()).plan;const summary=d.getByRole('region',{name:'Gennemgå korrektionen',exact:true});await summary.waitFor();need(await d.getByLabel('Plan-hash',{exact:true}).inputValue()===reviewed.planHash,'displayed full canonical plan hash');need((await summary.innerText()).includes(billDate)&&(await summary.innerText()).includes(bankDate),'human readable separate invoice and bank dates');need(await d.getByRole('button',{name:'Bekræft korrektion',exact:true}).isDisabled(),'correction requires explicit source review');const popup=ownerPage.waitForEvent('popup');await d.getByRole('link',{name:'Åbn originalbilag',exact:true}).click();await(await popup).close();await originals();need(await d.getByRole('link',{name:'Åbn oprindelig postering',exact:true}).getAttribute('href')===`/companies/synthetic/posteringer?journalEntryId=${reviewed.originalJournalEntryId}`,'review link binds exact original journal');const journalPopup=ownerPage.waitForEvent('popup');await d.getByRole('link',{name:'Åbn oprindelig postering',exact:true}).click();const shown=await journalPopup;await shown.getByRole('heading',{name:'Posteringer',exact:true}).waitFor();await shown.close();await d.getByLabel('Jeg har gennemgået bilaget, den oprindelige postering og korrektionen',{exact:true}).check();const response=ownerPage.waitForResponse(r=>new URL(r.url()).pathname===correctionBase+'/apply'&&r.request().method()==='POST');response.catch(()=>{});step='explicit-reviewed-correction-one-apply';await d.getByRole('button',{name:'Bekræft korrektion',exact:true}).click();const r=await response;need(r.status()===200,'one confirmed native correction');confirmed=await r.json();applyBody=r.request().postDataJSON();need(applyBody.planHash===reviewed.planHash&&applyBody.confirm===true&&typeof applyBody.idempotencyKey==='string'&&applyBody.idempotencyKey.length>0&&r.request().headers()['idempotency-key']===applyBody.idempotencyKey,'one caller-selected transport key binds reviewed plan');const result=correctionReceipt(confirmed);need(result.idempotent===false&&confirmed.idempotency?.replayed===false,'original committed logical result and caller receipt distinct');await d.waitFor({state:'hidden'});await verifyEffects();stable={reports:await reports(),billDate:await asOf(billDate),bankDate:await asOf(bankDate),original:await originals()};await ownerPage.getByRole('button',{name:'Betalte',exact:true}).click();await ownerPage.getByRole('row').filter({hasText:'SYN-DIRECT-58'}).getByText('Betalt',{exact:true}).waitFor();await ownerPage.screenshot({path:process.env.RM_DIRECT_SCREENSHOT,fullPage:true});return{reviewed,confirmed,stable};}
async function verifyEffects(){const result=correctionReceipt(confirmed),after=await reports(),entries=after.journal.journal.entries;need(entries.length===4,'one original plus exactly reversal bill settlement');const old=entries.find(e=>e.id===reviewed.originalJournalEntryId),before=baseline.journal.journal.entries[0];for(const key of['id','entryNo','date','text','documentId','documentNo','total','lines','reversalOfEntryId'])need(isDeepStrictEqual(old?.[key],before[key]),'immutable original journal '+key);const mirror=entries.filter(e=>e.reversalOfEntryId===old.id);need(mirror.length===1&&mirror[0].date===bankDate&&old.reversedByEntryId===mirror[0].id,'one real mirror at original journal date');const payment=entries.find(e=>e.id===result.settlementJournalEntryId);need(payment?.date===bankDate&&payment.lines.length===2,'one settlement on actual bank date');const bills=entries.filter(e=>e.id!==old.id&&e.id!==mirror[0].id&&e.id!==payment.id);need(bills.length===1&&bills[0].date===billDate&&bills[0].documentId===document.id&&bills[0].lines.length===3,'bill recognized on preserved invoice date');const open=(await asOf(billDate)).payables.rows.filter(p=>p.payableId===result.payableId),paid=(await asOf(bankDate)).payables.rows.filter(p=>p.payableId===result.payableId);need(open.length===1&&open[0].billDate===billDate&&open[0].status==='open'&&open[0].openBalance===125&&open[0].paidAmount===0,'genuine open liability before later payment');need(paid.length===1&&paid[0].status==='paid'&&paid[0].openBalance===0&&paid[0].paidAmount===125,'genuine paid liability on bank date');need(after.income.incomeStatement.totalExpense===100&&after.income.incomeStatement.result===-100&&after.vat.vat.inputVat===25&&after.vat.vat.payable===-25&&after.vat.vat.vatReportErrors.length===0,'final expense and VAT unchanged');const bank=after.bank.bank;need(bank.transactions.length===1&&bank.matchedCount===1&&bank.unmatchedCount===0&&bank.bookedBalance===-125&&bank.actualBalance===-125&&bank.difference===0,'one canonical bank relation and unchanged bank totals');need(bank.transactions[0].journalEntryNo===payment.entryNo&&bank.transactions[0].reconciliationId?.startsWith('correction:')&&bank.transactions[0].reconciliationHistory.length===2&&bank.transactions[0].reconciliationHistory.filter(h=>h.kind==='direct'&&h.id===old.id&&h.journalHash===reviewed.originalJournalHash).length===1&&bank.transactions[0].reconciliationHistory.filter(h=>h.kind==='correction'&&h.oldJournalEntryId===old.id&&h.replacementJournalEntryId===payment.id&&h.principalSubjectId===ownerId).length===1,'exact superseded direct relation now points only to settlement');need(after.trialBalance.trialBalance.balanced===true&&after.trialBalance.trialBalance.rows.find(r=>r.accountNo==='7000')?.balance===0,'real balanced books with creditors settled');await originals();return after;}
async function assertStable(){await verifyEffects();need(isDeepStrictEqual(await reports(),stable.reports)&&isDeepStrictEqual(await asOf(billDate),stable.billDate)&&isDeepStrictEqual(await asOf(bankDate),stable.bankDate),'exact canonical reports/UUID/as-of histories persist');await originals();}
async function replay(){step='explicit-same-key-known-result-replay';const repeated=await post(correctionBase+'/apply',applyBody);need(isDeepStrictEqual(repeated.correction,confirmed.correction)&&repeated.idempotency?.replayed===true&&repeated.idempotency.receiptId===confirmed.idempotency.receiptId,'original exact durable caller receipt replay');await assertStable();const changed=await call(ownerPage,correctionBase+'/apply',{...applyBody,reason:applyBody.reason+' changed business payload'});need(changed.status===409&&changed.value?.ok===false,`same key changed body conflicts HTTP${changed.status} ${changed.value?.code??''}`);const wrong=await call(ownerPage,correctionBase+'/apply',{...applyBody,planHash:'0'.repeat(64),idempotencyKey:'synthetic-direct-logical-conflict'});need(wrong.status===422&&wrong.value?.ok===false,'different reviewed plan cannot logically replay');await assertStable();return{callerReceiptReplay:true,changedKeyedPayloadRefused:true,changedLogicalPlanRefused:true};}
async function client(kind,name,body,confirmed=false){const file=join(work,'client.cookie');writeFileSync(file,(await ownerPage.context().cookies(base)).map(c=>`${c.name}=${c.value}`).join('; '),{mode:0o600,flag:'wx'});try{const common=['--server',base,'--cookie-file',file,'--allow-loopback-http'];let argv,input;if(kind==='cli'){argv=['remote',name,...common,'--company','synthetic',...(confirmed?['--confirm']:[])];input=JSON.stringify(body);}else{argv=common;input=[{jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'synthetic-direct-bank-proof',version:'1'}}},{jsonrpc:'2.0',method:'notifications/initialized'},{jsonrpc:'2.0',id:2,method:'tools/call',params:{name,arguments:{company:'synthetic',body:JSON.stringify(body),...(confirmed?{confirm:true}:{})}}}].map(v=>JSON.stringify(v)).join('\n')+'\n';}const out=spawnSync(join(work,kind==='cli'?'rentemester-rust':'rentemester-mcp'),argv,{input,encoding:'utf8',timeout:45000,maxBuffer:16*1024*1024});need(!out.error&&out.status===0,kind+' canonical current correction terminal');const value=kind==='cli'?JSON.parse(out.stdout):out.stdout.trim().split('\n').map(l=>JSON.parse(l)).find(v=>v.id===2)?.result?.structuredContent;need(value?.ok===true&&value.data,'full canonical native client envelope');return value.data;}finally{unlinkSync(file);}}
async function planClients(){step='passive-current-plan-cli-mcp-parity';const canonical=await post(correctionBase+'/plan',input);need(isDeepStrictEqual(await client('cli','direct-bank-payable-correction-plan',input),canonical)&&isDeepStrictEqual(await client('mcp','direct_bank_payable_correction_plan',input),canonical),'full canonical plan HTTP CLI MCP parity');need((await reports()).journal.journal.entries.length===1,'readonly client plan never posts');return{httpCliMcpPlanParity:true,plan:canonical.plan};}
async function clients(){step='explicit-native-client-exact-known-outcome-replay';for(const[kind,name]of[['cli','direct-bank-payable-correction'],['mcp','direct_bank_payable_correction']]){const repeated=await client(kind,name,applyBody,true);need(isDeepStrictEqual(repeated.correction,confirmed.correction)&&repeated.idempotency?.replayed===true&&repeated.idempotency.receiptId===confirmed.idempotency.receiptId,'exact original native caller receipt parity');}await assertStable();return{httpCliMcpReceiptParity:true,noNewEffects:true};}
async function reopen(){diagnosticPage=ownerPage;step='restart-normal-password-existing-mfa';await ownerPage.context().clearCookies();need((await login(ownerPage,owner)).user.id===ownerId,'same ordinary MFA owner after restart');await assertStable();await replay();await clients();return{freshMfaAfterRestart:true,canonicalOriginalAndAsOfAndRelationRetained:true,noNewEffects:true};}
async function archive(){step='explicit-signed-correction-snapshot';const r=await ownerPage.request.post(base+'/api/workspace/backup',{headers:{origin:base,'content-type':'application/json'},data:{confirm:true}});if(r.status()===422){const refusal=await r.json();need(refusal.ok===false&&Array.isArray(refusal.errors)&&refusal.errors.some(e=>/nonempty unsupported|unsupported.*(?:correction|schema|inventory)|(?:correction|schema|inventory).*unsupported/i.test(e)),'snapshot must state concrete unsupported nonempty inventory');await assertStable();return{supported:false,status:r.status(),code:refusal.code,errors:refusal.errors,sourceFreeRestoreQualified:false};}need(r.status()===200&&r.headers()['content-type']==='application/vnd.rentemester.native-backup+json','configured signed snapshot response');const bytes=await r.body();need(bytes.length>0&&bytes.length<=96*1024*1024,'bounded exact signed bytes');writeFileSync(join(work,'direct-bank.native-backup.json'),bytes,{mode:0o600,flag:'wx'});await assertStable();return{supported:true,sha256:hash(bytes),bytes:bytes.length,sourceFreeRestoreQualified:false};}
async function restored(){step='fresh-restored-owner-normal-verification-mfa';ownerPage=await newPage();const fresh={email:'restored@example.invalid',password:'synthetic-restored-password-789',codes:[]};await ownerPage.goto((await mail('verification',fresh.email)).actionUrl);const me=await enroll(ownerPage,fresh);need(me.user.id!==ownerId,'fresh restored owner authority, historical actor not imported');await assertStable();return{sourceFreeRestoreQualified:true,freshOwnerMfa:true,sourceCorrectionHistoricalActorPreserved:true,readOnlyNoImportedReceiptReplay:true};}
for await(const line of createInterface({input:process.stdin,crlfDelay:Infinity})){try{const action=JSON.parse(line);let value;if(action==='authenticate')value=await authenticate();else if(action==='intake')value=await intake();else if(action==='direct')value=await direct();else if(action==='refusals')value=await refusals();else if(action==='plan-clients')value=await planClients();else if(action==='correction')value=await correction();else if(action==='replay')value=await replay();else if(action==='clients')value=await clients();else if(action==='reopen')value=await reopen();else if(action==='archive')value=await archive();else if(action==='restored')value=await restored();else if(action==='finish'){await browser?.close();emit({ok:true,value:{finished:true}});break;}else throw Error('unknown phase');emit({ok:true,value});}catch(error){let pageState=null;try{pageState=await Promise.race([diagnosticPage?.evaluate(()=>({pathname:location.pathname,headings:[...document.querySelectorAll('h1,h2')].map(e=>e.textContent.slice(0,100)),buttons:[...document.querySelectorAll('button')].map(e=>e.textContent.slice(0,100)),alerts:document.querySelectorAll('[role=alert]').length})),new Promise(resolve=>setTimeout(()=>resolve(null),2000))]);}catch{}emit({ok:false,step,detail,errorType:error?.name,trace,pageState});await Promise.race([browser?.close().catch(()=>{}),new Promise(resolve=>setTimeout(resolve,2000))]);process.exitCode=1;break;}}
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
    report = {'status': 'running', 'sourceRevision': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=REPO, text=True).strip(), 'runnerSha256': digest(Path(__file__)), 'browserHelperSha256': hashlib.sha256(BROWSER.encode()).hexdigest(), 'canonicalOrigin': origin, 'noOfflineIdentitySeeds': True, 'ordinaryMacOsUser': True, 'ownedProfile': str(work), 'externalMailDelivered': False, 'workflow': 'direct_bank_purchase_payable_correction', 'mixedVatAndCrossVatPeriodActualQualified': False,  'providerTaxParityQualified': False, 'installedNativeQualified': False, 'faultInjectedInActualRunner': False, 'screenshot': str(args.evidence.with_suffix('.png')),  'actions': [], 'results': {}, 'sourceFreeRestoreQualified': False}
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
                argv += ['--restore-backup', str(work / 'direct-bank.native-backup.json'), '--trusted-backup-key', str(work / 'trusted.pub'), '--confirm-target', str(root), '--source-unavailable']
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
            report['actions'].append({'action': action, 'status': 'complete'})
            report['results'][action] = value['value']
            save()
            return value['value']

        def bank_original():
            expected = report['results']['intake']['csvSha256']
            root = Path(config['root'])
            path = root / 'synthetic/bank/processed' / (expected + '.txt')
            if not owned() or root.parent != work or any(p.is_symlink() for p in [root, root / 'synthetic', root / 'synthetic/bank', path.parent, path]):
                raise RuntimeError('Exact owned bank namespace required')
            info = path.stat()
            if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid() or digest(path) != expected:
                raise RuntimeError('Actual archived bank bytes differ from ordinary CSV intake')
            report['archivedBankOriginal'] = {'sha256': expected, 'bytes': info.st_size, 'verified': True}
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
                ['openssl', 'req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', str(work / 'ca.key'), '-out', str(ca), '-days', '1', '-subj', '/CN=Synthetic Direct Correction Auth CA', '-addext', 'basicConstraints=critical,CA:TRUE', '-addext', 'keyUsage=critical,keyCertSign,cRLSign'],
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
            config = {'runtimeProfile': 'local-user-macos', 'root': str(work / 'workspace'), 'brokerUid': os.getuid(), 'workspace': 'synthetic-native-direct-correction', 'companySlug': 'synthetic', 'companyName': 'Synthetic Correction Company', 'cookieKey': key(), 'mfaKey': key(), 'operatorActors': ['system:synthetic-installer'], 'accounts': [], 'firstOwner': {'actor': 'system:synthetic-installer', 'email': 'owner@example.invalid', 'name': 'Synthetic Owner', 'password': 'synthetic-owner-password-123'}, 'backupSigningKeyFile': str(work / 'signing.der'), 'authMail': {'endpoint': f'https://127.0.0.1:{mail_port}/mail', 'bearerToken': bearer, 'from': 'auth@example.invalid', 'idempotencySecret': secrets.token_urlsafe(32), 'trustedCaFile': str(ca)}}
            write_config()
            start(create=True)
            script = work / 'browser.mjs'
            script.write_text(BROWSER)
            controller = subprocess.Popen(['/opt/homebrew/bin/bun', str(script)], cwd=REPO, env={**os.environ, 'RM_DIRECT_BASE': origin, 'RM_DIRECT_WORK': str(work), 'RM_DIRECT_SCREENSHOT': str(args.evidence.with_suffix('.png')), 'RM_DIRECT_PLAYWRIGHT': str((REPO / 'node_modules/@playwright/test/index.mjs').resolve(strict=True))}, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, bufsize=1, start_new_session=True)
            report['browserControllerPid'] = controller.pid
            for action in ['authenticate', 'intake', 'direct', 'refusals', 'plan-clients', 'correction', 'replay', 'clients']:
                phase(action)
                if action == 'intake':
                    bank_original()
            stop()
            config.pop('firstOwner')
            write_config()
            start()
            phase('reopen')
            bank_original()
            archive = phase('archive')
            report['signedSnapshot'] = archive
            if archive['supported']:
                progress('operator:independent-signed-snapshot-verification')
                path = work / 'direct-bank.native-backup.json'
                if digest(path) != archive['sha256']:
                    raise RuntimeError('Signed artifact changed after download')
                verified = subprocess.run([str(pinned['server_binary']), '--verify-backup', str(path), '--trusted-backup-key', str(work / 'trusted.pub')], cwd=REPO, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=60)
                if verified.returncode != 0:
                    raise RuntimeError('Independent signature/inventory verification refused')
                report['trustedSourcePublicKeySha256'] = digest(work / 'trusted.pub')
                report['independentArchiveVerification'] = True
                stop()
                source = work / 'workspace'
                if not owned() or source.is_symlink() or not source.is_dir():
                    raise RuntimeError('Exact owned source required before source-free removal')
                shutil.rmtree(source)
                (work / 'signing.der').unlink()
                config.pop('backupSigningKeyFile')
                config.update({'root': str(work / 'restored'), 'cookieKey': key(), 'mfaKey': key(), 'restoreActor': 'system:synthetic-installer', 'firstOwner': {'actor': 'system:synthetic-installer', 'email': 'restored@example.invalid', 'name': 'Synthetic Restored Owner', 'password': 'synthetic-restored-password-789'}})
                write_config()
                start(restore=True)
                restored = phase('restored')
                bank_original()
                report['sourceFreeRestoreQualified'] = restored['sourceFreeRestoreQualified']
                report['sourceWorkspaceAndPrivateSignerRemoved'] = not source.exists() and not (work / 'signing.der').exists()
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
            report.update({'status': 'passed', 'currentAction': 'complete', 'normalMfaShiftedDatePurchaseCorrectionQualified': True, 'initialAndRestartQualified': True, 'snapshotCoverage': 'source-free-restored' if archive['supported'] else 'explicitly-blocked-by-nonempty-unsupported-inventory'})
            save()
            print('PASS: ordinary MFA direct purchase to dated payable correction, native clients, replay and restart; ' + str(args.evidence))
        except BaseException as error:
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
