#!/usr/bin/env python3
"""Ordinary-MFA EU B2B EUR invoice, VIES, realized FX and signed recovery qualification.
Supplied binaries/web are privately pinned; only a fresh synthetic macOS profile is used.
All business changes use ordinary authorized UI/APIs. Signed SQLite is inspected read-only.
Only a private verified-TLS synthetic VIES/mail gateway is used; no external calls, identity seeds, implicit retry, or product build.
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
PREFIX='Rentemester-Synthetic-EU-Sales-'
SIGNER="const c=require('node:crypto'),f=require('node:fs'),k=c.generateKeyPairSync('ed25519'),s=k.privateKey.export({format:'der',type:'pkcs8'}).subarray(-32),p=k.publicKey.export({format:'der',type:'spki'}).subarray(-32);f.writeFileSync(process.argv[1],Buffer.concat([Buffer.from('3051020101300506032b657004220420','hex'),s,Buffer.from('812100','hex'),p]),{mode:0o600});f.writeFileSync(process.argv[2],p,{mode:0o600});"
BROWSER = r'''








import {createHmac,createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {createInterface} from 'node:readline';
import {readFileSync,readdirSync,writeFileSync,unlinkSync,lstatSync,realpathSync} from 'node:fs';
import {join} from 'node:path';
import {isDeepStrictEqual} from 'node:util';
const {chromium}=await import(process.env.RM_EU_SALES_PLAYWRIGHT);
const base=process.env.RM_EU_SALES_BASE,work=process.env.RM_EU_SALES_WORK;
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
let today,issueDate,receiptDate,secondReceiptDate,firstRefundDate,finalDate,customer,stable,freshOwner,sourceCookie;
const issued={},originalsById={},credits={};
const money=v=>{need(typeof v==='number'||typeof v==='string','explicit monetary wire');const s=String(v);need(/^-?\d+(\.\d{1,2})?$/.test(s),'exact cent fixture amount');const negative=s.startsWith('-'),[whole,part='']=s.replace('-','').split('.');return (BigInt(whole)*100n+BigInt(part.padEnd(2,'0')))*(negative?-1n:1n);};
async function eu(to=today){return read(prefix+`eu-sales?from=${issueDate}&to=${to}`);}
async function snapshot(){return {invoices:await read(prefix+'invoices'),documents:await read(prefix+'documents'),journal:await read(prefix+'journal'),trialBalance:await read(prefix+'trial-balance'),incomeStatement:await read(prefix+'income-statement'),vat:await read(prefix+'vat'),bank:await read(prefix+'bank'),periods:await read(prefix+'periods'),euSales:await eu(),euBeforeCredits:await eu(secondReceiptDate),euAfterPartial:await eu(firstRefundDate)};}
async function saveOriginal(id,pdf=false){const r=await ownerPage.request.get(base+prefix+`documents/${id}/file`);need(r.status()===200,'canonical original readable');const bytes=await r.body();const proof={jsonBase64:bytes.toString('base64'),jsonSha256:hash(bytes)};if(pdf){const p=await ownerPage.request.get(base+prefix+`invoices/${id}/pdf`);need(p.status()===200,'canonical PDF readable');const raw=await p.body();need(raw.subarray(0,5).toString()==='%PDF-','issued real PDF');Object.assign(proof,{pdfBase64:raw.toString('base64'),pdfSha256:hash(raw)});}if(originalsById[id])need(isDeepStrictEqual(originalsById[id],proof),'immutable original JSON/PDF unchanged');else originalsById[id]=proof;return JSON.parse(bytes);}
async function originals(){for(const item of Object.values(issued))await saveOriginal(item.id,true);for(const list of Object.values(credits))for(const item of list)await saveOriginal(item.documentId);return originalsById;}
async function setup(){
 step='ordinary-EUR-company-chart-explicit-FX-roles-and-customer';today=(await read(prefix+'scheduled-recurring')).scheduledRecurring.asOfDate;need(/^\d{4}-\d{2}-\d{2}$/.test(today),'trusted server date');const ago=n=>{const d=new Date(today+'T12:00:00Z');d.setUTCDate(d.getUTCDate()-n);return d.toISOString().slice(0,10);};issueDate=ago(6);receiptDate=ago(5);secondReceiptDate=ago(4);firstRefundDate=ago(3);finalDate=ago(1);need(issueDate.slice(0,4)===today.slice(0,4)&&Math.floor((Number(issueDate.slice(5,7))-1)/3)===Math.floor((Number(today.slice(5,7))-1)/3),'fixture requires same current fiscal year and VAT quarter');
 const changed=await ownerPage.request.patch(base+prefix+'company',{headers:{origin:base},data:{cvr:'87654321',address:'Synthetic EU Street 77',postalCode:'1000',city:'Synthetic City',vatPeriodType:'quarter',paymentTermsDays:14}});need(changed.status()===200,'ordinary legal profile');
 for(const [accountNo,name,type,normalBalance] of [['1100','Synthetic receivables','asset','debit'],['1800','Synthetic EUR bank','asset','debit'],['2200','Synthetic payables','liability','credit'],['1000','Synthetic service revenue','income','credit'],['4200','Synthetic output VAT','vat','credit'],['4300','Synthetic input VAT','vat','debit'],['1010','Synthetic FX gain','income','credit'],['6010','Synthetic FX loss','expense','debit']])await post(prefix+'accounts',{accountNo,name,type,normalBalance});
 for(const [role,accountNo]of [['debtors','1100'],['bank','1800'],['creditors','2200'],['output_vat','4200'],['input_vat','4300'],['fx_gain','1010'],['fx_loss','6010']])await post(prefix+'account-roles',{role,accountNo,confirm:true});
 await post(prefix+'bank-accounts',{name:'Synthetic EU service EUR bank',slug:'synthetic-eu77-bank',currency:'EUR',ledgerAccountNo:'1800'});
 customer=(await post(prefix+'customers',{name:'Synthetic EU Service Buyer',address:'Synthetic Buyer Street 77',vatOrCvr:'DE123456789',paymentTermsDays:14,defaultCurrency:'EUR'})).customer;
 return {customerId:customer.id,today,issueDate,receiptDate,secondReceiptDate,firstRefundDate,finalDate};
}
function issueInput(){return {issueDate,customerId:customer.id,currency:'EUR',fxRateToDkk:7.40,vatTreatment:'foreign_reverse_charge',reverseChargeBasis:'EU_B2B_SERVICES',reverseChargeNote:'Reverse charge - EU B2B services',lines:[{description:'Synthetic EU B2B service',quantity:1,unitPriceExVat:100,taxClassification:'reverse_charge',vatRate:0,reverseChargeBasis:'EU_B2B_SERVICES'}]};}
async function viesCases(){
 step='verified-TLS-valid-invalid-wrong-inconclusive-and-revoked-held-VIES';
 const before=await snapshot();for(const vat of ['DK12345678','US123456789','not-a-VAT'])need((await fetchJson(ownerPage,prefix+'vies/validate',{vatOrCvr:vat})).status===422,'non-EU malformed lookup refuses');
 const invalid=await post(prefix+'vies/validate',{vatOrCvr:'DE000000000'});need(invalid.validation?.valid===false,'invalid provider result explicitly retained');
 for(const vat of ['DE111111111','DE333333333']){const r=await fetchJson(ownerPage,prefix+'vies/validate',{vatOrCvr:vat});need(r.status>=400&&r.body?.ok===false,'wrong VAT or unavailable provider never validity');}
 await post('/api/workspace/members/company',{userId:memberId,action:'grant',companySlug:'synthetic',role:'owner'});
 const held=fetchJson(memberPage,prefix+'vies/validate',{vatOrCvr:'DE222222222'});for(let i=0;i<100&&!readdirSync(work).includes('vies-held');i++)await new Promise(r=>setTimeout(r,50));need(readdirSync(work).includes('vies-held'),'exact held provider request began');
 await post('/api/workspace/members/company',{userId:memberId,action:'revoke',companySlug:'synthetic'});writeFileSync(join(work,'vies-release'),'released',{mode:0o600,flag:'wx'});const revoked=await held;need(revoked.status===403,'revoked held provider result cannot commit');
 await post('/api/workspace/members/company',{userId:memberId,action:'grant',companySlug:'synthetic',role:'reviewer'});need(isDeepStrictEqual(await snapshot(),before),'VIES and authority probes change no accounting');
 return {invalid:invalid.validation,wrongAndUnavailableRefused:true,heldLookupRevocationRefused:true};
}
async function issueAll(){
 const viesProof=await viesCases();step='existing-EUR-form-explicit-VIES-preview-and-issue';
 const missing=await fetchJson(ownerPage,prefix+'invoices/issue',issueInput());need(missing.status===422,'uncached foreign VAT invoice refuses');
 await ownerPage.goto(base+'/companies/synthetic/fakturaer');await ownerPage.getByRole('link',{name:'Udsted faktura',exact:true}).click();const form=ownerPage.getByRole('region',{name:'Udsted faktura',exact:true});
 for(const [label,value]of [['Fakturadato',issueDate],['Sælger','Synthetic EU Service Company'],['Sælger CVR/moms','DK87654321'],['Sælgeradresse','Synthetic EU Street 77']])await form.getByLabel(label,{exact:true}).fill(value);
 await form.getByLabel('Vælg kunde',{exact:true}).selectOption(String(customer.id));await form.getByLabel('Momsbehandling',{exact:true}).selectOption('foreign_reverse_charge');await form.getByLabel('Valuta',{exact:true}).fill('EUR');await form.getByLabel('Valutakurs i DKK',{exact:true}).fill('7,40');await form.getByLabel('Note om omvendt betalingspligt',{exact:true}).fill('Reverse charge - EU B2B services');
 await form.getByLabel('Linje 1 beskrivelse',{exact:true}).fill('Synthetic EU B2B service');await form.getByLabel('Linje 1 antal',{exact:true}).fill('1');await form.getByLabel('Linje 1 enhedspris',{exact:true}).fill('100');
 const lookup=ownerPage.waitForResponse(r=>new URL(r.url()).pathname===prefix+'vies/validate'&&r.request().method()==='POST');await form.getByRole('button',{name:'Verificér EU-momsnummer',exact:true}).click();const observed=await lookup;need(observed.status()===200,'explicit user VIES result');const validation=(await observed.json()).validation;need(validation.valid===true&&validation.normalized==='DE123456789','trusted matched provider VAT');await form.getByText(/Gyldigt EU-momsnummer/).waitFor();
 const before=await read(prefix+'invoices'),viewed=ownerPage.waitForResponse(r=>new URL(r.url()).pathname===prefix+'invoices/preview'&&r.request().method()==='POST');await form.getByRole('button',{name:'Forhåndsvis',exact:true}).click();const preview=await viewed;need(preview.status()===200,'EUR form preview');const link=form.getByRole('link',{name:'Åbn forhåndsvisning',exact:true});await link.waitFor();const shown=await link.evaluate(async link=>{const b=new Uint8Array(await(await fetch(link.href)).arrayBuffer());return{length:b.length,prefix:Array.from(b.subarray(0,5)).map(v=>String.fromCharCode(v)).join('')};});need(shown.length>100&&shown.prefix==='%PDF-','actual EUR PDF preview');need(isDeepStrictEqual(await read(prefix+'invoices'),before),'preview creates no invoice');
 const pending=ownerPage.waitForResponse(r=>new URL(r.url()).pathname===prefix+'invoices/issue'&&r.request().method()==='POST');await form.getByRole('button',{name:'Udsted faktura',exact:true}).click();const response=await pending;need(response.status()===200,'EUR UI invoice confirmed');const summary=(await response.json()).invoice;need(money(summary.grossAmount)===money(100)&&money(summary.grossAmountDkk)===money(740),'truthful original EUR and DKK issue totals');await form.getByText(/Bogføring i DKK/).waitFor();await form.getByRole('button',{name:'Luk',exact:true}).click();
 issued.eu={id:summary.documentId,number:summary.invoiceNumber};const payload=await saveOriginal(summary.documentId,true);need(payload.currency==='EUR'&&payload.fxValuationVersion===1&&payload.totals.fxRateToDkk===7.4&&money(payload.totals.grossAmountDkk)===money(740)&&payload.viesValidation.normalized==='DE123456789'&&payload.viesValidation.valid===true,'immutable original FX and validated customer provenance');
 const stable=await snapshot();for(const input of [{...issueInput(),fxRateToDkk:undefined},{...issueInput(),reverseChargeBasis:'EU_B2B_GOODS'},{...issueInput(),buyer:{name:'Synthetic domestic',vatOrCvr:'DK12345678'},customerId:undefined},{...issueInput(),buyer:{name:'Synthetic invalid',vatOrCvr:'DE000000000'},customerId:undefined}])need((await fetchJson(ownerPage,prefix+'invoices/issue',input)).status===422,'missing FX or invalid tax basis/buyer fail closed');need((await fetchJson(memberPage,prefix+'invoices/issue',issueInput())).status===403,'reviewer issue denied');need(isDeepStrictEqual(await snapshot(),stable),'denied foreign issues have no effects');return {issued,validation,viesProof,originals:originalsById};
}
function checkVat(value,base){const r=value.vat;need(money(r.outputVat)===0n&&money(r.payable)===0n&&money(r.rubrikker.rubrikB)===money(base),'exact zero output VAT and dated EU service rubrik B');need(r.vatReportErrors.length===0,'VAT native source proof clean');return value;}
async function reportUi(want){await ownerPage.goto(base+'/companies/synthetic/fakturaer');await ownerPage.getByRole('button',{name:'Moms og perioder',exact:true}).click();await ownerPage.getByRole('link',{name:'EU-salg uden moms',exact:true}).click();await ownerPage.getByLabel('Fra',{exact:true}).fill(issueDate);await ownerPage.getByLabel('Til',{exact:true}).fill(today);const pending=ownerPage.waitForResponse(r=>new URL(r.url()).pathname===prefix+'eu-sales'&&new URL(r.url()).searchParams.get('from')===issueDate);await ownerPage.getByRole('button',{name:'Vis periode',exact:true}).click();need((await pending).status()===200,'normal dated EU sales report');const table=ownerPage.getByRole('table',{name:'EU-salg pr. kunde',exact:true});await table.getByText('DE123456789',{exact:true}).waitFor();need((await table.innerText()).includes(String(want)+',00'),'EU UI DKK valuation');}
async function postAll(){
 step='existing-EUR-posting-ui-and-DKK-journal';await ownerPage.goto(base+'/companies/synthetic/fakturaer');const row=ownerPage.getByRole('row').filter({hasText:issued.eu.number});await row.getByRole('button',{name:'Bogfør faktura',exact:true}).click();const dialog=ownerPage.getByRole('dialog');await dialog.getByLabel(/^Omsætningskonto/).selectOption('1000');const pending=ownerPage.waitForResponse(r=>new URL(r.url()).pathname===prefix+'invoices/post'&&r.request().method()==='POST');await dialog.getByRole('button',{name:'Bogfør salget',exact:true}).click();need((await pending).status()===200,'EUR UI posting');await dialog.waitFor({state:'hidden'});
 const current=await read(prefix+'journal'),entries=current.journal.entries.filter(e=>e.documentId===issued.eu.id);need(entries.length===1,'one native EUR sale posting');issued.eu.entryId=entries[0].id;const lines=entries[0].lines;need(money(lines.find(l=>l.accountNo==='1100').debit)===money(740)&&money(lines.find(l=>l.accountNo==='1000').credit)===money(740),'exact DKK740 counters and EU tax classification');
 const sales=await eu();need(money(sales.totalValue)===money(740)&&sales.invoiceCount===1&&sales.creditNoteCount===0&&sales.customers[0].vatNumber==='DE123456789','posted EU basis740');checkVat(await read(prefix+'vat'),740);await reportUi(740);const before=await snapshot();need((await fetchJson(ownerPage,prefix+'invoices/post',{invoiceDocumentId:issued.eu.id,revenueAccountNo:'1000',confirm:true})).status===422,'duplicate sale refused');need(isDeepStrictEqual(await snapshot(),before),'duplicate posting has no effect');return {journal:current,euSales:sales,vat:before.vat};
}
async function credit(gross,date){const r=await post(prefix+'invoices/credit-note',{invoiceDocumentId:issued.eu.id,issueDate:date,reason:'Synthetic reviewed EU service credit',grossAmount:gross,confirm:true});const note=r.creditNote;(credits.eu??=[]).push(note);const source=await saveOriginal(note.documentId);need(source.currency==='EUR'&&money(source.grossAmount)===money(gross)&&money(source.grossAmountDkk)===money(gross===20?148:592),'credit preserves original EUR and fixed DKK carrying valuation');return note;}
async function cashUi(endpoint,reference,refund=false){await ownerPage.goto(base+'/companies/synthetic/fakturaer');const row=ownerPage.getByRole('row').filter({hasText:issued.eu.number});await row.getByRole('button',{name:refund?'Afstem tilbagebetaling':'Afstem',exact:true}).click();const dialog=ownerPage.getByRole('dialog',{name:refund?'Afstem tilbagebetaling':'Afstem faktura mod bankbetaling',exact:true});await dialog.getByLabel('Bankreference',{exact:true}).fill(reference);const pending=ownerPage.waitForResponse(r=>new URL(r.url()).pathname===prefix+endpoint&&r.request().method()==='POST');await dialog.getByRole('button',{name:refund?'Afstem tilbagebetaling':'Afstem faktura',exact:true}).click();const response=await pending;need(response.status()===200,'ordinary invoice cash UI confirmed');const result=await response.json();await dialog.waitFor({state:'hidden'});return result;}
async function creditsAndCash(){
 step='EUR-partial-final-receipts-realized-FX-credit-and-refund';const csv=`date;text;amount;currency;reference;amount_dkk;fx_rate_to_dkk\n${receiptDate};Synthetic receipt40;40;EUR;EU77-RECEIPT40;300;7.50\n${secondReceiptDate};Synthetic receipt60;60;EUR;EU77-RECEIPT60;441;7.35\n${firstRefundDate};Synthetic refund20;-20;EUR;EU77-REFUND20;-146;7.30\n${finalDate};Synthetic refund80;-80;EUR;EU77-REFUND80;-592;7.40\n`;
 await post(prefix+'bank/import',{account:'synthetic-eu77-bank',csvContent:csv,confirm:true});const receipts=[];for(const [ref,date,principal,bank]of [['EU77-RECEIPT40',receiptDate,296,300],['EU77-RECEIPT60',secondReceiptDate,444,441]]){const s=(ref==='EU77-RECEIPT40'?await cashUi('invoices/settle',ref):await post(prefix+'invoices/settle',{invoiceDocumentId:issued.eu.id,bankTransactionReference:ref,paymentDate:date,confirm:true})).settlement;need(money(s.principalAmountDkk)===money(principal)&&money(s.bankAmountDkk)===money(bank),'exact allocated DKK principal and actual bank');receipts.push(s);}
 need((await read(prefix+'invoices')).invoices.invoices[0].openBalance===0,'EUR receivable fully paid');await credit(20,firstRefundDate);const partial=await eu();need(money(partial.totalValue)===money(592)&&partial.creditNoteCount===1,'partial credit EUbasis592');checkVat(await read(prefix+'vat'),592);
 const refund20=(await cashUi('invoices/refund','EU77-REFUND20',true)).refund;need(money(refund20.remainingCreditBalance)===0n&&money(refund20.principalAmountDkk)===money(148)&&money(refund20.bankAmountDkk)===money(146),'partial EUR refund consumes exactly credit20 DKK148');
 await credit(80,finalDate);const final=await eu();need(money(final.totalValue)===0n&&final.creditNoteCount===2,'final EUbasiszero');checkVat(await read(prefix+'vat'),0);const refund80=(await post(prefix+'invoices/refund',{invoiceDocumentId:issued.eu.id,bankTransactionReference:'EU77-REFUND80',refundDate:finalDate,confirm:true})).refund;need(money(refund80.remainingCreditBalance)===0n&&money(refund80.principalAmountDkk)===money(592)&&money(refund80.bankAmountDkk)===money(592),'final EUR refund exact');
 const current=await snapshot();need(current.journal.journal.entries.length===7,'exact invoice+2receipt+2credit+2refund journals');need(current.trialBalance.trialBalance.balanced,'final balanced DKK ledger');const net=no=>current.journal.journal.entries.flatMap(e=>e.lines).filter(l=>l.accountNo===no).reduce((s,l)=>s+money(l.debit)-money(l.credit),0n);for(const [no,want]of [['1100',0],['1000',0],['1800',3],['1010',-6],['6010',3]])need(net(no)===money(want),'exact complete journey account '+no);
 for(const [path,body]of [['invoices/credit-note',{invoiceDocumentId:issued.eu.id,issueDate:today,reason:'Forbidden extra credit',grossAmount:1,confirm:true}],['invoices/refund',{invoiceDocumentId:issued.eu.id,bankTransactionReference:'EU77-REFUND20',confirm:true}],['invoices/settle',{invoiceDocumentId:issued.eu.id,bankTransactionReference:'EU77-RECEIPT40',confirm:true}]])need((await fetchJson(ownerPage,prefix+path,body)).status===422,'duplicate cash and excesscredit refuse');need(isDeepStrictEqual(await snapshot(),current),'no duplicate economic effects');await originals();return {credits,receipts,refund20,refund80,firstReceiptAndRefundUi:true,datedCreditsUseApiBecauseUiDateNotEditable:true,euAfterPartial:partial,euFinal:final,exactFxNetGainDkk:3,bankOriginalBase64:Buffer.from(csv).toString('base64'),bankOriginalSha256:hash(Buffer.from(csv))};
}
async function datedClose(){step='dated-EU-original-basis-and-signed-credit-adjustments';const before=await eu(secondReceiptDate),partial=await eu(firstRefundDate),final=await eu();need(money(before.totalValue)===money(740)&&before.creditNoteCount===0&&money(partial.totalValue)===money(592)&&partial.creditNoteCount===1&&money(final.totalValue)===0n&&final.creditNoteCount===2,'later credit does not rewrite earlier dated EU basis');return{before,partial,final,vat:checkVat(await read(prefix+'vat'),0)};}
async function freeze(){step='canonical-stable-ledger-and-originals';sourceCookie=(await ownerPage.context().cookies(base)).map(c=>`${c.name}=${c.value}`).join('; ');stable=await snapshot();await originals();await ownerPage.goto(base+'/companies/synthetic/fakturaer');await ownerPage.screenshot({path:process.env.RM_EU_SALES_SCREENSHOT,fullPage:true});return {canonical:stable,issued,credits,originals:originalsById};}
async function archive(name){step='signed-eu-sales-'+name;const r=await ownerPage.request.post(base+'/api/workspace/backup',{headers:{origin:base},data:{confirm:true}});const ct=r.headers()['content-type'];if(r.status()!==200||ct!=='application/vnd.rentemester.native-backup+json'){let body;try{body=await r.json();}catch{}detail={status:r.status(),contentType:ct,code:body?.code,errors:body?.errors};throw Error('signed VAT backup refused');}const bytes=await r.body();need(bytes.length&&bytes.length<=96*1024*1024,'bounded complete signed artifact');writeFileSync(join(work,`eu-sales-${name}.native-backup.json`),bytes,{mode:0o600,flag:'wx'});need(isDeepStrictEqual(await snapshot(),stable),'backup changes no business evidence');return{sha256:hash(bytes),bytes:bytes.length};}
async function reopen(){step='source-normal-mfa-restart';await ownerPage.context().clearCookies();await login(ownerPage,owner);need(isDeepStrictEqual(await snapshot(),stable),'exact source restart domain state');await originals();return{freshMfaRestart:true};}
async function restored(){step='source-free-normal-new-owner';ownerPage=await newPage();freshOwner={email:'restored@example.invalid',password:'synthetic-restored-password-789',codes:[]};await ownerPage.goto((await mail('verification',freshOwner.email)).actionUrl);need((await enroll(ownerPage,freshOwner)).user.id!==ownerId,'historical actor is not live restored grant');const denied=await ownerPage.request.get(base+prefix+'journal',{headers:{cookie:sourceCookie}});need([401,403].includes(denied.status()),'old source session has no authority');need(isDeepStrictEqual(await snapshot(),stable),'source-free exact invoice/credit/cash/VAT/close state');await originals();return {sourceFreeRestoreQualified:true,credentialsAbsent:true};}
async function restoredReopen(){step='target-normal-mfa-reopen';await ownerPage.context().clearCookies();await login(ownerPage,freshOwner);need(isDeepStrictEqual(await snapshot(),stable),'target reopen exact retained accounting');await originals();return {restoredTargetReopenQualified:true};}
let clientPhase=0;
async function clientReads(){step='current-session-cli-mcp-http-full-parity';const before=await snapshot(),cookiePath=join(realpathSync(work),`vat-client-${++clientPhase}.cookie`);writeFileSync(cookiePath,(await ownerPage.context().cookies(base)).map(c=>`${c.name}=${c.value}`).join('; '),{mode:0o600,flag:'wx'});const common=['--server',base,'--cookie-file',cookiePath,'--allow-loopback-http'];const cli=(command,body,selectors=[])=>{const run=spawnSync(join(work,'rentemester-rust'),['remote',command,...common,'--company','synthetic',...selectors,...(body?['--confirm']:[])],{input:body?JSON.stringify(body):undefined,encoding:'utf8',timeout:45000,maxBuffer:32*1024*1024});need(!run.error,'CLI invocation');return {code:run.status,data:JSON.parse(run.stdout)};};const rpc=(name,args)=>{const input=[{jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'synthetic-eu-sales',version:'1'}}},{jsonrpc:'2.0',method:'notifications/initialized'},{jsonrpc:'2.0',id:2,method:'tools/call',params:{name,arguments:args}}].map(JSON.stringify).join('\n')+'\n';const run=spawnSync(join(work,'rentemester-mcp'),common,{input,encoding:'utf8',timeout:45000,maxBuffer:32*1024*1024});need(!run.error&&run.status===0,'MCP process terminal');const response=run.stdout.trim().split('\n').map(JSON.parse).find(v=>v.id===2);need(!response.error,'MCP protocol');return response.result;};
 try{for(const [command,path]of [['invoices','invoices'],['journal','journal'],['vat','vat'],['periods','periods'],['eu-sales',`eu-sales?from=${issueDate}&to=${today}`]]){const canonical=await read(prefix+path),c=cli(command,undefined,command==='eu-sales'?['--from',issueDate,'--to',today]:[]),m=rpc(command==='eu-sales'?'eu_sales':command,{company:'synthetic',...(command==='eu-sales'?{from:issueDate,to:today}:{})});need(c.code===0&&c.data.ok===true&&isDeepStrictEqual(c.data.data,canonical),'full CLI canonical read');need(!m.isError&&isDeepStrictEqual(m.structuredContent.data,canonical),'full MCP canonical read');}
 const duplicatePost=cli('post-invoice',{invoiceDocumentId:issued.eu.id,revenueAccountNo:'1000',confirm:true});need(duplicatePost.code===1&&duplicatePost.data.code==='server_refused','CLI known duplicate refuses without new posting');const duplicateCredit=rpc('credit_invoice',{company:'synthetic',body:JSON.stringify({invoiceDocumentId:issued.eu.id,issueDate:today,reason:'Synthetic client duplicate',confirm:true}),confirm:true});need(duplicateCredit.isError===true&&[409,422].includes(duplicateCredit.structuredContent?.error?.status),'MCP known duplicate or closed-period refuses without new credit');need(isDeepStrictEqual(await snapshot(),before),'clients never create duplicate effects');return {httpCliMcpCanonicalEquality:true,explicitKnownDuplicateMutationsRefused:true};}finally{unlinkSync(cookiePath);}}
for await(const line of createInterface({input:process.stdin,crlfDelay:Infinity})){try{const action=JSON.parse(line);let value;if(action==='authenticate')value=await authenticate();else if(action==='setup')value=await setup();else if(action==='issue')value=await issueAll();else if(action==='post')value=await postAll();else if(action==='credits-and-cash')value=await creditsAndCash();else if(action==='dated-close')value=await datedClose();else if(action==='freeze')value=await freeze();else if(action==='reopen')value=await reopen();else if(action==='restored')value=await restored();else if(action==='restored-reopen')value=await restoredReopen();else if(action.startsWith('client-reads'))value=await clientReads();else if(action.startsWith('archive-'))value=await archive(action.slice(8));else if(action==='finish'){await browser?.close();emit({ok:true,value:{finished:true}});break;}else throw Error('unknown phase');emit({ok:true,value});}catch(error){if(ownerId&&diagnosticPage)await ownerPage.screenshot({path:process.env.RM_EU_SALES_SCREENSHOT,fullPage:true}).catch(()=>{});emit({ok:false,step,detail,errorType:error?.name,errorMessage:String(error?.message).slice(0,1200),trace});await browser?.close().catch(()=>{});process.exitCode=1;break;}}
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
    vies_requests = []
    gateway_lock = threading.Lock()
    bearer = secrets.token_urlsafe(32)
    backend_port, mail_port = free_port(), free_port()
    if backend_port == mail_port:
        raise RuntimeError('Distinct owned ports required')
    origin = f'http://127.0.0.1:{backend_port}'
    report = {'status':'running','sourceRevision':subprocess.check_output(['git','rev-parse','HEAD'],cwd=REPO,text=True).strip(),'runnerSha256':digest(Path(__file__)),'browserHelperSha256':hashlib.sha256(BROWSER.encode()).hexdigest(),'canonicalOrigin':origin,'noOfflineIdentitySeeds':True,'ordinaryMacOsUser':True,'ownedProfile':str(work),'externalMailDelivered':False,'workflow':'explicit_eu_sales_accounting','installedNativeQualified':False,'euSalesActualQualified':False,'sourceFreeRestoreQualified':False,'screenshot':str(args.evidence.with_suffix('.png')),'actions':[],'results':{}}
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
                match = re.fullmatch(r'/vies/ms/DE/vat/(123456789|000000000|111111111|222222222|333333333)', self.path)
                if not match:
                    self.send_error(404)
                    return
                vat = match.group(1)
                with gateway_lock:
                    vies_requests.append({'method':'GET','path':self.path,'vatNumber':vat})
                if vat == '333333333':
                    self.reply(503, {'userError':'MS_UNAVAILABLE'})
                    return
                if vat == '222222222':
                    (work / 'vies-held').write_text('held')
                    deadline = time.monotonic() + 8
                    while not (work / 'vies-release').exists() and time.monotonic() < deadline:
                        time.sleep(.02)
                    if not (work / 'vies-release').exists():
                        self.reply(503, {'userError':'MS_UNAVAILABLE'})
                        return
                self.reply(200, {'countryCode':'DE','vatNumber':'999999999' if vat == '111111111' else vat,'isValid':vat != '000000000','userError':'VALID','name':'Synthetic EU Service Buyer','address':'Synthetic Buyer Street 77'})

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
                argv += ['--restore-backup', str(work / 'eu-sales-source.native-backup.json'), '--trusted-backup-key', str(work / 'trusted.pub'), '--confirm-target', str(root), '--source-unavailable']
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
            progress('operator:independent-signed-eu-sales-' + name)
            path = work / ('eu-sales-' + name + '.native-backup.json')
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
            copy = work / ('readonly-eu-sales-' + name + '.sqlite')
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
            if len(result['issued_invoice_postings']) != 1 or len(result['credit_note_postings']) != 2 or len(result['invoice_payments']) != 2 or len(result['invoice_refunds']) != 2 or len(result['journal_entries']) != 7 or len(result['bank_transactions']) != 4:
                raise RuntimeError('Exact native VAT/post/credit/payment/refund effects missing')
            signed_originals={name:hashlib.sha256(value).hexdigest() for name,value in entries.items() if '/documents/' in name or '/bank/' in name}
            original_proofs=report['results']['freeze']['originals']
            for proof in original_proofs.values():
                for key in ['jsonBase64','pdfBase64']:
                    if key in proof:
                        wanted=base64.b64decode(proof[key],validate=True)
                        if not any(value==wanted for name,value in entries.items() if '/documents/' in name):
                            raise RuntimeError('Exact issued/credit original bytes missing from signed inventory')
            statement = base64.b64decode(report['results']['credits-and-cash']['bankOriginalBase64'], validate=True)
            if hashlib.sha256(statement).hexdigest() != report['results']['credits-and-cash']['bankOriginalSha256'] or not any(raw==statement for raw in entries.values()):
                raise RuntimeError('Exact supplied bank statement original missing from signed inventory')
            accounts = {row['id']:row['account_no'] for row in result['accounts']}
            frozen = report['results']['freeze']
            item=frozen['issued']['eu']
            original=[row for row in result['journal_lines'] if row['journal_entry_id']==item['entryId'] and accounts[row['account_id']]=='1000']
            if len(original)!=1 or original[0]['vat_code']!='REVERSE_CHARGE_EXEMPT' or Decimal(str(original[0]['credit_amount']))!=Decimal('740'):
                raise RuntimeError('Exact original EU service revenue/VAT journal provenance missing')
            net={account:sum((Decimal(str(row['debit_amount']))-Decimal(str(row['credit_amount'])) for row in result['journal_lines'] if accounts[row['account_id']]==account),Decimal(0)) for account in ['1100','1000','1800','1010','6010']}
            if net!={'1100':Decimal(0),'1000':Decimal(0),'1800':Decimal(3),'1010':Decimal(-6),'6010':Decimal(3)}:
                raise RuntimeError('Final exact DKK cash/revenue/receivable/FX effect differs')
            validations={row['vat_number']:row for row in result['vies_validations']}
            if set(validations)!={'123456789','000000000'} or validations['123456789']['valid']!=1 or validations['000000000']['valid']!=0:
                raise RuntimeError('Failed or revoked VIES observation committed, or trusted cache missing')
            documents = {row['id']:json.loads(row['payload_json']) for row in result['documents'] if row['payload_json']}
            lines = {row['id']:row for row in result['journal_lines']}
            for posting in result['credit_note_postings']:
                payload = documents[posting['credit_note_document_id']]
                allocations = payload['creditAllocations']
                actual = [row for row in result['journal_lines'] if row['journal_entry_id']==posting['journal_entry_id'] and row['account_id']!=posting['receivable_account_id']]
                expected = []
                for allocation in allocations:
                    original = lines[allocation['journalLineId']]
                    if accounts[original['account_id']] != allocation['accountNo'] or original['vat_code'] != allocation['vatCode'] or Decimal(str(original['credit_amount'])) != Decimal(allocation['originalAmount']):
                        raise RuntimeError('Credit allocation does not bind exact original counter')
                    amount = Decimal(allocation['allocatedAmount'])
                    if amount:
                        expected.append((original['account_id'], original['vat_code'], amount, Decimal(0)))
                posted = [(row['account_id'],row['vat_code'],Decimal(str(row['debit_amount'])),Decimal(str(row['credit_amount']))) for row in actual]
                if posted != expected:
                    raise RuntimeError('Actual credit journal differs from immutable original allocations')
            result['signedOriginalHashes']=signed_originals
            report.setdefault('snapshots', {})[name] = {'sha256':receipt['sha256'],'trustedPublicKeySha256':digest(trusted),'privateCredentialsExcluded':True,'tables':result}
            save()
            return result

        def preserved(original, newer):
            if set(original)!=set(newer):
                raise RuntimeError('Restored full company table inventory changed')
            for table in original:
                if table!='audit_log' and original[table]!=newer[table]:
                    raise RuntimeError('Exact restored EU sales evidence differs: '+table)
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
                ['openssl', 'req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', str(work / 'ca.key'), '-out', str(ca), '-days', '1', '-subj', '/CN=Synthetic EU Service Auth CA', '-addext', 'basicConstraints=critical,CA:TRUE', '-addext', 'keyUsage=critical,keyCertSign,cRLSign'],
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
            config = {'runtimeProfile': 'local-user-macos', 'root': str(work / 'workspace'), 'brokerUid': os.getuid(), 'workspace': 'synthetic-native-eu-sales', 'companySlug': 'synthetic', 'companyName': 'Synthetic EU Service Company', 'cookieKey': key(), 'mfaKey': key(), 'operatorActors': ['system:synthetic-installer'], 'accounts': [], 'firstOwner': {'actor': 'system:synthetic-installer', 'email': 'owner@example.invalid', 'name': 'Synthetic Owner', 'password': 'synthetic-owner-password-123'}, 'backupSigningKeyFile': str(work / 'signing.der'), 'vies': {'baseUrl':f'https://127.0.0.1:{mail_port}/vies','trustedCaPem':ca.read_text()}, 'authMail': {'endpoint': f'https://127.0.0.1:{mail_port}/mail', 'bearerToken': bearer, 'from': 'auth@example.invalid', 'idempotencySecret': secrets.token_urlsafe(32), 'trustedCaFile': str(ca)}}
            write_config()
            start(create=True)
            script = work / 'browser.mjs'
            script.write_text(BROWSER)
            controller = subprocess.Popen(['/opt/homebrew/bin/bun', str(script)], cwd=REPO, env={**os.environ, 'RM_EU_SALES_BASE': origin, 'RM_EU_SALES_WORK': str(work), 'RM_EU_SALES_SCREENSHOT': str(args.evidence.with_suffix('.png')), 'RM_EU_SALES_PLAYWRIGHT': str((REPO / 'node_modules/@playwright/test/index.mjs').resolve(strict=True))}, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, bufsize=1, start_new_session=True)
            report['browserControllerPid'] = controller.pid
            phase('authenticate')
            phase('setup')
            phase('issue')
            phase('post')
            phase('credits-and-cash')
            phase('dated-close')
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
                raise RuntimeError('Fresh target reopen changed retained full EU sales evidence')
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
                report['gateway']={'verifiedHttps':True,'acceptedMessages':len(gateway_rows),'requests':list(gateway_requests),'viesRequests':list(vies_requests)}
                if len(vies_requests)!=5 or len({row['vatNumber'] for row in vies_requests})!=5:
                    raise RuntimeError('VIES requests replayed, retried or missing')
            report.update({'status':'passed','currentAction':'complete','euSalesActualQualified':True,'httpCliMcpReadParityQualified':True,'snapshotCoverage':'four-independently-verified-eu-sales-full-ledger-original-archives-fresh-owner-restore-and-reopen'})
            save()
            print('PASS: ordinary MFA EUR EU services, trusted VIES, exact FX receipts/credits/refunds, dated EU/VAT reports, canonical clients and signed source-free recovery; '+str(args.evidence))
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
