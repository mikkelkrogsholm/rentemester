#!/usr/bin/env python3
"""Ordinary-MFA foreign-buyer full and delivery-aware reduced simplified invoice lifecycle and signed recovery qualification.
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
PREFIX='Rentemester-Synthetic-Invoice-Metadata-80-'
SIGNER="const c=require('node:crypto'),f=require('node:fs'),k=c.generateKeyPairSync('ed25519'),s=k.privateKey.export({format:'der',type:'pkcs8'}).subarray(-32),p=k.publicKey.export({format:'der',type:'spki'}).subarray(-32);f.writeFileSync(process.argv[1],Buffer.concat([Buffer.from('3051020101300506032b657004220420','hex'),s,Buffer.from('812100','hex'),p]),{mode:0o600});f.writeFileSync(process.argv[2],p,{mode:0o600});"
BROWSER = r'''








import {createHmac,createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {createInterface} from 'node:readline';
import {readFileSync,readdirSync,writeFileSync,unlinkSync,lstatSync,realpathSync} from 'node:fs';
import {join} from 'node:path';
import {isDeepStrictEqual} from 'node:util';
const {chromium}=await import(process.env.RM_INVOICE_METADATA_PLAYWRIGHT);
const base=process.env.RM_INVOICE_METADATA_BASE,work=process.env.RM_INVOICE_METADATA_WORK;
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
let today,issueDate,receiptDate,secondReceiptDate,firstRefundDate,finalDate,stable,freshOwner,sourceCookie;
const issued={},originalsById={},credits={};
const money=v=>{need(typeof v==='number'||typeof v==='string','explicit monetary wire');const s=String(v);need(/^-?\d+(\.\d{1,2})?$/.test(s),'exact cent fixture amount');const negative=s.startsWith('-'),[whole,part='']=s.replace('-','').split('.');return(BigInt(whole)*100n+BigInt(part.padEnd(2,'0')))*(negative?-1n:1n);};
async function snapshot(){return{invoices:await read(prefix+'invoices'),documents:await read(prefix+'documents'),journal:await read(prefix+'journal'),trialBalance:await read(prefix+'trial-balance'),incomeStatement:await read(prefix+'income-statement'),vat:await read(prefix+'vat'),bank:await read(prefix+'bank'),periods:await read(prefix+'periods')};}
async function saveOriginal(id,pdf=false){const r=await ownerPage.request.get(base+prefix+`documents/${id}/file`);need(r.status()===200,'canonical original readable');const bytes=await r.body(),proof={jsonBase64:bytes.toString('base64'),jsonSha256:hash(bytes)};if(pdf){const p=await ownerPage.request.get(base+prefix+`invoices/${id}/pdf`);need(p.status()===200,'canonical PDF readable');const raw=await p.body();need(raw.subarray(0,5).toString()==='%PDF-','issued real PDF');Object.assign(proof,{pdfBase64:raw.toString('base64'),pdfSha256:hash(raw)});}if(originalsById[id])need(isDeepStrictEqual(originalsById[id],proof),'immutable original JSON/PDF unchanged');else originalsById[id]=proof;return JSON.parse(bytes);}
function checkMetadata(original,kind){if(kind==='full'){need(original.deliveryDate===undefined&&original.deliveryPeriodStart===issueDate&&original.deliveryPeriodEnd===receiptDate&&original.unitCode==='HUR'&&original.lines[0].unitCode==='H87'&&original.buyer.vatOrCvr==='DE123456789','full original exact foreign buyer, delivery range and explicit document/line units');}else{need(original.deliveryDate===issueDate&&original.deliveryPeriodStart===undefined&&original.deliveryPeriodEnd===undefined&&original.unitCode===undefined&&original.lines[0].unitCode===undefined,'reduced original exact delivery date and no invented units');}}
async function originals(){for(const[kind,item]of Object.entries(issued))checkMetadata(await saveOriginal(item.id,true),kind);for(const list of Object.values(credits))for(const item of list)await saveOriginal(item.documentId);return originalsById;}
async function setup(){
 step='ordinary-DKK-company-chart-and-bank';today=(await read(prefix+'scheduled-recurring')).scheduledRecurring.asOfDate;need(/^\d{4}-\d{2}-\d{2}$/.test(today),'trusted server date');const ago=n=>{const d=new Date(today+'T12:00:00Z');d.setUTCDate(d.getUTCDate()-n);return d.toISOString().slice(0,10);};issueDate=ago(6);receiptDate=ago(5);secondReceiptDate=ago(4);firstRefundDate=ago(3);finalDate=ago(1);need(issueDate.slice(0,4)===today.slice(0,4)&&Math.floor((Number(issueDate.slice(5,7))-1)/3)===Math.floor((Number(today.slice(5,7))-1)/3),'fixture dates require same current year and VAT quarter');
 const changed=await ownerPage.request.patch(base+prefix+'company',{headers:{origin:base},data:{cvr:'87654321',address:'Synthetic Metadata Street 80',postalCode:'1000',city:'Synthetic City',vatPeriodType:'quarter',paymentTermsDays:14}});need(changed.status()===200,'ordinary legal profile');
 for(const[accountNo,name,type,normalBalance]of[['1100','Synthetic receivables','asset','debit'],['1800','Synthetic DKK bank','asset','debit'],['2200','Synthetic payables','liability','credit'],['1000','Synthetic service revenue','income','credit'],['4200','Synthetic output VAT','vat','credit'],['4300','Synthetic input VAT','vat','debit']])await post(prefix+'accounts',{accountNo,name,type,normalBalance});
 for(const[role,accountNo]of[['debtors','1100'],['bank','1800'],['creditors','2200'],['output_vat','4200'],['input_vat','4300']])await post(prefix+'account-roles',{role,accountNo,confirm:true});
 await post(prefix+'bank-accounts',{name:'Synthetic private sales DKK bank',slug:'synthetic-metadata80-bank',currency:'DKK',ledgerAccountNo:'1800'});return{today,issueDate,receiptDate,secondReceiptDate,firstRefundDate,finalDate};
}
function issueInput(kind='full',gross=100.07){return{issueDate,invoiceType:kind,currency:'DKK',seller:{name:'Synthetic Invoice Metadata Company',address:'Synthetic Metadata Street 80',vatOrCvr:'DK87654321'},...(kind==='full'?{buyer:{name:'Synthetic Foreign Customer',address:'Synthetic Metadata Buyer Street 80',vatOrCvr:'DE123456789'},deliveryPeriodStart:issueDate,deliveryPeriodEnd:receiptDate,unitCode:'HUR',vatRatePercent:25,lines:[{description:'Synthetic metadata full sale',quantity:1,unitPriceExVat:200,unitCode:'H87'}]}:{vatTreatment:'standard',deliveryDate:issueDate,lines:[{description:'Synthetic reduced simplified sale'}],totals:{grossAmount:gross,vatRate:0.25,vatComputationBasis:'VAT_20_OF_GROSS'}})};}
async function preview(form){const before=await snapshot(),pending=ownerPage.waitForResponse(r=>new URL(r.url()).pathname===prefix+'invoices/preview'&&r.request().method()==='POST');await form.getByRole('button',{name:'Forhåndsvis',exact:true}).click();const response=await pending;need(response.status()===200,'ordinary form PDF preview');const input=response.request().postDataJSON();const link=form.getByRole('link',{name:'Åbn forhåndsvisning',exact:true});await link.waitFor();const p=await link.evaluate(async link=>{const b=new Uint8Array(await(await fetch(link.href)).arrayBuffer());return{length:b.length,prefix:Array.from(b.subarray(0,5)).map(v=>String.fromCharCode(v)).join('')};});need(p.length>100&&p.prefix==='%PDF-','actual PDF preview bytes');need(isDeepStrictEqual(await snapshot(),before),'preview has no business effects');return input;}
async function issueUi(kind){
 await ownerPage.goto(base+'/companies/synthetic/fakturaer');await ownerPage.getByRole('link',{name:'Udsted faktura',exact:true}).click();const form=ownerPage.getByRole('region',{name:'Udsted faktura',exact:true});await form.getByLabel('Fakturatype',{exact:true}).selectOption(kind);
 for(const[label,value]of[['Fakturadato',issueDate],['Sælger','Synthetic Invoice Metadata Company'],['Sælger CVR/moms','DK87654321'],['Sælgeradresse','Synthetic Metadata Street 80']])await form.getByLabel(label,{exact:true}).fill(value);
 if(kind==='full'){await form.getByLabel('Kunde',{exact:true}).fill('Synthetic Foreign Customer');await form.getByLabel('Kundeadresse',{exact:true}).fill('Synthetic Metadata Buyer Street 80');await form.getByLabel('Kunde CVR/moms (valgfrit)',{exact:true}).fill('DE123456789');await form.getByLabel('Levering',{exact:true}).selectOption('period');await form.getByLabel('Leveringsperiode fra',{exact:true}).fill(issueDate);await form.getByLabel('Leveringsperiode til',{exact:true}).fill(receiptDate);await form.getByLabel('Enhedskode (valgfri)',{exact:true}).fill('HUR');await form.getByLabel('Linje 1 enhedskode (valgfri)',{exact:true}).fill('H87');await form.getByLabel('Linje 1 beskrivelse',{exact:true}).fill('Synthetic metadata full sale');await form.getByLabel('Linje 1 antal',{exact:true}).fill('1');await form.getByLabel('Linje 1 enhedspris',{exact:true}).fill('200');}
 else{await form.getByLabel('Levering',{exact:true}).selectOption('date');await form.getByLabel('Leveringsdato',{exact:true}).fill(issueDate);await form.getByLabel('Beskrivelse',{exact:true}).fill('Synthetic reduced simplified sale');await form.getByLabel('Beløb inkl. moms',{exact:true}).fill('100,07');for(const label of['Kunde (valgfrit)','Kundeadresse (valgfri)','Kunde CVR/moms (valgfrit)'])need(await form.getByLabel(label,{exact:true}).inputValue()==='','no fabricated simplified buyer');}
 const previewInput=await preview(form);checkMetadata(previewInput,kind);const pending=ownerPage.waitForResponse(r=>new URL(r.url()).pathname===prefix+'invoices/issue'&&r.request().method()==='POST');await form.getByRole('button',{name:'Udsted faktura',exact:true}).click();const r=await pending;const issueWire={...r.request().postDataJSON()};delete issueWire.confirm;const previewWire={...previewInput};delete previewWire.confirm;need(isDeepStrictEqual(issueWire,previewWire),'web preview and issue identical factual input');need(r.status()===200,'ordinary metadata/simplified UI issue');const summary=(await r.json()).invoice;need(summary.invoiceType===kind&&money(summary.grossAmount)===money(kind==='full'?250:100.07),'truthful issued type/gross');await form.getByRole('button',{name:'Luk',exact:true}).click();issued[kind]={id:summary.documentId,number:summary.invoiceNumber};
 const original=await saveOriginal(summary.documentId,true);need(original.invoiceType===kind&&original.currency==='DKK','immutable original invoice type and currency');checkMetadata(original,kind);if(kind==='full')need(original.buyer.name==='Synthetic Foreign Customer'&&original.buyer.address==='Synthetic Metadata Buyer Street 80'&&original.buyer.vatOrCvr==='DE123456789','ordinary foreign buyer identifier preserved without tax inference');else{need((!original.buyer||Object.keys(original.buyer).length===0)&&original.lines.length===1&&original.lines[0].description==='Synthetic reduced simplified sale'&&original.lines[0].quantity===undefined&&original.lines[0].unitPriceExVat===undefined,'no manufactured reduced line or buyer');need(money(original.totals.netAmount)===money(80.06)&&money(original.totals.vatAmount)===money(20.01)&&money(original.totals.grossAmount)===money(100.07),'server derived VAT20percent gross cent allocation');}return summary;
}
async function issueAll(){
 step='foreign-buyer-delivery-units-and-reduced-simplified-web-issue';const before=await snapshot(),full=issueInput(),simple=issueInput('simplified');const refused=[{...full,buyer:undefined},{...simple,invoiceType:'unknown'},{...simple,buyer:{publicRecipient:true,eanNumber:'5790000000000'}},{...simple,totals:{...simple.totals,grossAmount:3000.01}},{...simple,totals:{...simple.totals,vatAmount:1}},{...simple,totals:{...simple.totals,grossAmount:'not-money'}},{...simple,lines:[{description:'Synthetic mixed shape',quantity:1}]},{...simple,vatTreatment:'foreign_reverse_charge'},{...full,deliveryDate:issueDate},{...full,deliveryPeriodEnd:undefined},{...full,deliveryPeriodStart:receiptDate,deliveryPeriodEnd:issueDate},{...simple,deliveryDate:'2026-02-30'},{...full,unitCode:''},{...full,lines:[{...full.lines[0],unitCode:''}]}];for(const[index,input]of refused.entries()){const observed=await fetchJson(ownerPage,prefix+'invoices/issue',input);need(observed.status===(index===5?400:422),`invalid full/simplified input ${index} HTTP${observed.status} fail closed`);}need((await fetchJson(memberPage,prefix+'invoices/issue',simple)).status===403,'current reviewer cannot issue');const limit=await fetchJson(ownerPage,prefix+'invoices/preview',issueInput('simplified',3000));need(limit.status===200,'exact DKK3000 inclusive preview accepted');need(isDeepStrictEqual(await snapshot(),before),'refusals and exact-limit preview preserve source state');
 const fullSummary=await issueUi('full'),simplifiedSummary=await issueUi('simplified');return{issued,fullSummary,simplifiedSummary,exactLimitPreviewAccepted:true,invalidAndReviewerIssuesRefused:true,originals:originalsById};
}
function checkVat(value,want){const r=value.vat;need(money(r.outputVat)===money(want)&&money(r.payable)===money(want),'exact output VAT and payable');need(r.vatReportErrors.length===0,'native VAT source proof clean');return value;}
async function postAll(){
 step='ordinary-metadata-and-simplified-posting-ui';for(const[kind,item]of Object.entries(issued)){await ownerPage.goto(base+'/companies/synthetic/fakturaer');const row=ownerPage.getByRole('row').filter({hasText:item.number});await row.getByRole('button',{name:'Bogfør faktura',exact:true}).click();const dialog=ownerPage.getByRole('dialog');await dialog.getByLabel(/^Omsætningskonto/).selectOption('1000');const pending=ownerPage.waitForResponse(r=>new URL(r.url()).pathname===prefix+'invoices/post'&&r.request().method()==='POST');await dialog.getByRole('button',{name:'Bogfør salget',exact:true}).click();need((await pending).status()===200,'ordinary posting UI');await dialog.waitFor({state:'hidden'});const rows=(await read(prefix+'journal')).journal.entries.filter(e=>e.documentId===item.id);need(rows.length===1,'one original journal');item.entryId=rows[0].id;const net=rows[0].lines.find(l=>l.accountNo==='1000'),vat=rows[0].lines.find(l=>l.accountNo==='4200');need(money(net.credit)===money(kind==='full'?200:80.06)&&money(vat.credit)===money(kind==='full'?50:20.01),'exact private/simplified original net and VAT');}
 const before=await snapshot();checkVat(before.vat,70.01);need((await fetchJson(ownerPage,prefix+'invoices/post',{invoiceDocumentId:issued.full.id,revenueAccountNo:'1000',confirm:true})).status===422,'duplicate sale refused');need((await fetchJson(memberPage,prefix+'invoices/post',{invoiceDocumentId:issued.simplified.id,revenueAccountNo:'1000',confirm:true})).status===403,'reviewer cannot post');need(isDeepStrictEqual(await snapshot(),before),'denied postings preserve business state');return{vatBeforeCredits:before.vat,journal:before.journal};
}
async function credit(kind,gross,date){const r=await post(prefix+'invoices/credit-note',{invoiceDocumentId:issued[kind].id,issueDate:date,reason:'Synthetic reviewed private sales credit',grossAmount:gross,confirm:true});const note=r.creditNote;(credits[kind]??=[]).push(note);const source=await saveOriginal(note.documentId);need(source.currency==='DKK'&&money(source.grossAmount)===money(gross),'credit retains exact original gross');return note;}
async function cashUi(kind,endpoint,reference,refund=false){await ownerPage.goto(base+'/companies/synthetic/fakturaer');const row=ownerPage.getByRole('row').filter({hasText:issued[kind].number});await row.getByRole('button',{name:refund?'Afstem tilbagebetaling':'Afstem',exact:true}).click();const dialog=ownerPage.getByRole('dialog',{name:refund?'Afstem tilbagebetaling':'Afstem faktura mod bankbetaling',exact:true});await dialog.getByLabel('Bankreference',{exact:true}).fill(reference);const pending=ownerPage.waitForResponse(r=>new URL(r.url()).pathname===prefix+endpoint&&r.request().method()==='POST');await dialog.getByRole('button',{name:refund?'Afstem tilbagebetaling':'Afstem faktura',exact:true}).click();const r=await pending;need(r.status()===200,'ordinary cash UI');const result=await r.json();await dialog.waitFor({state:'hidden'});return result;}
async function creditsAndCash(){
 step='full-and-simplified-exact-partial-final-cash-credit-refund';const csv=`date;text;amount;currency;reference\n${receiptDate};Synthetic full receipt;250;DKK;M80-FULL-IN\n${firstRefundDate};Synthetic full refund;-250;DKK;M80-FULL-OUT\n${receiptDate};Synthetic simplified receipt40.03;40.03;DKK;M80-SIMPLE-IN40\n${secondReceiptDate};Synthetic simplified receipt60.04;60.04;DKK;M80-SIMPLE-IN60\n${firstRefundDate};Synthetic simplified refund40.03;-40.03;DKK;M80-SIMPLE-OUT40\n${finalDate};Synthetic simplified refund60.04;-60.04;DKK;M80-SIMPLE-OUT60\n`;await post(prefix+'bank/import',{account:'synthetic-metadata80-bank',csvContent:csv,confirm:true});const receipts=[],refunds=[];
 receipts.push((await cashUi('full','invoices/settle','M80-FULL-IN')).settlement);receipts.push((await cashUi('simplified','invoices/settle','M80-SIMPLE-IN40')).settlement);receipts.push((await post(prefix+'invoices/settle',{invoiceDocumentId:issued.simplified.id,bankTransactionReference:'M80-SIMPLE-IN60',paymentDate:secondReceiptDate,confirm:true})).settlement);checkVat(await read(prefix+'vat'),70.01);
 await credit('full',250,firstRefundDate);refunds.push((await cashUi('full','invoices/refund','M80-FULL-OUT',true)).refund);checkVat(await read(prefix+'vat'),20.01);
 await credit('simplified',40.03,firstRefundDate);const afterPartial=checkVat(await read(prefix+'vat'),12.01);refunds.push((await cashUi('simplified','invoices/refund','M80-SIMPLE-OUT40',true)).refund);
 await credit('simplified',60.04,finalDate);refunds.push((await post(prefix+'invoices/refund',{invoiceDocumentId:issued.simplified.id,bankTransactionReference:'M80-SIMPLE-OUT60',refundDate:finalDate,confirm:true})).refund);checkVat(await read(prefix+'vat'),0);
 const current=await snapshot();need(current.journal.journal.entries.length===11&&current.trialBalance.trialBalance.balanced,'eleven exact balanced journals');const net=no=>current.journal.journal.entries.flatMap(e=>e.lines).filter(l=>l.accountNo===no).reduce((s,l)=>s+money(l.debit)-money(l.credit),0n);for(const no of['1100','1000','1800','4200'])need(net(no)===0n,'complete exact closed economic cycle account '+no);
 for(const[path,body]of[['invoices/credit-note',{invoiceDocumentId:issued.simplified.id,issueDate:today,reason:'Forbidden extra credit',grossAmount:.01,confirm:true}],['invoices/refund',{invoiceDocumentId:issued.simplified.id,bankTransactionReference:'M80-SIMPLE-OUT40',confirm:true}],['invoices/settle',{invoiceDocumentId:issued.full.id,bankTransactionReference:'M80-FULL-IN',confirm:true}]])need((await fetchJson(ownerPage,prefix+path,body)).status===422,'duplicate cash or excess credit refused');need((await fetchJson(memberPage,prefix+'invoices/credit-note',{invoiceDocumentId:issued.full.id,issueDate:today,reason:'Forbidden reviewer credit',confirm:true})).status===403,'reviewer cannot credit');need(isDeepStrictEqual(await snapshot(),current),'duplicate or unauthorized effects absent');await originals();return{credits,receipts,refunds,firstReceiptAndRefundUi:true,datedCreditsUseApiBecauseUiDateNotEditable:true,vatAfterPartial:afterPartial,bankOriginalBase64:Buffer.from(csv).toString('base64'),bankOriginalSha256:hash(Buffer.from(csv))};
}
async function datedClose(){step='retained-dated-metadata-originals-and-exact-VAT-reversal';const journal=await read(prefix+'journal');for(const[kind,list]of Object.entries(credits))for(const item of list){const source=await saveOriginal(item.documentId);need([firstRefundDate,finalDate].includes(source.issueDate),'credit retains chosen legal issue date');}return{vat:checkVat(await read(prefix+'vat'),0),journal,periodCloseNotForced:true};}
async function freeze(){step='canonical-stable-ledger-and-originals';sourceCookie=(await ownerPage.context().cookies(base)).map(c=>`${c.name}=${c.value}`).join('; ');stable=await snapshot();await originals();await ownerPage.goto(base+'/companies/synthetic/fakturaer');await ownerPage.screenshot({path:process.env.RM_INVOICE_METADATA_SCREENSHOT,fullPage:true});return {canonical:stable,issued,credits,originals:originalsById,metadata:{full:issueInput(),simplified:issueInput('simplified')}};}
async function archive(name){step='signed-invoice-metadata80-'+name;const r=await ownerPage.request.post(base+'/api/workspace/backup',{headers:{origin:base},data:{confirm:true}});const ct=r.headers()['content-type'];if(r.status()!==200||ct!=='application/vnd.rentemester.native-backup+json'){let body;try{body=await r.json();}catch{}detail={status:r.status(),contentType:ct,code:body?.code,errors:body?.errors};throw Error('signed VAT backup refused');}const bytes=await r.body();need(bytes.length&&bytes.length<=96*1024*1024,'bounded complete signed artifact');writeFileSync(join(work,`invoice-metadata80-${name}.native-backup.json`),bytes,{mode:0o600,flag:'wx'});need(isDeepStrictEqual(await snapshot(),stable),'backup changes no business evidence');return{sha256:hash(bytes),bytes:bytes.length};}
async function reopen(){step='source-normal-mfa-restart';await ownerPage.context().clearCookies();await login(ownerPage,owner);need(isDeepStrictEqual(await snapshot(),stable),'exact source restart domain state');await originals();return{freshMfaRestart:true};}
async function restored(){step='source-free-normal-new-owner';ownerPage=await newPage();freshOwner={email:'restored@example.invalid',password:'synthetic-restored-password-789',codes:[]};await ownerPage.goto((await mail('verification',freshOwner.email)).actionUrl);need((await enroll(ownerPage,freshOwner)).user.id!==ownerId,'historical actor is not live restored grant');const denied=await ownerPage.request.get(base+prefix+'journal',{headers:{cookie:sourceCookie}});need([401,403].includes(denied.status()),'old source session has no authority');need(isDeepStrictEqual(await snapshot(),stable),'source-free exact private/simplified invoice credit cash and VAT state');await originals();return {sourceFreeRestoreQualified:true,credentialsAbsent:true};}
async function restoredReopen(){step='target-normal-mfa-reopen';await ownerPage.context().clearCookies();await login(ownerPage,freshOwner);need(isDeepStrictEqual(await snapshot(),stable),'target reopen exact retained accounting');await originals();return {restoredTargetReopenQualified:true};}
let clientPhase=0;
async function clientReads(){step='current-session-cli-mcp-http-full-parity';const before=await snapshot(),cookiePath=join(realpathSync(work),`vat-client-${++clientPhase}.cookie`);writeFileSync(cookiePath,(await ownerPage.context().cookies(base)).map(c=>`${c.name}=${c.value}`).join('; '),{mode:0o600,flag:'wx'});const common=['--server',base,'--cookie-file',cookiePath,'--allow-loopback-http'];const cli=(command,body,selectors=[])=>{const run=spawnSync(join(work,'rentemester-rust'),['remote',command,...common,'--company','synthetic',...selectors,...(body&&command!=='preview-invoice'?['--confirm']:[])],{input:body?JSON.stringify(body):undefined,encoding:'utf8',timeout:45000,maxBuffer:32*1024*1024});need(!run.error,'CLI invocation');return {code:run.status,data:JSON.parse(run.stdout)};};const rpc=(name,args)=>{const input=[{jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'synthetic-invoice-metadata80',version:'1'}}},{jsonrpc:'2.0',method:'notifications/initialized'},{jsonrpc:'2.0',id:2,method:'tools/call',params:{name,arguments:args}}].map(JSON.stringify).join('\n')+'\n';const run=spawnSync(join(work,'rentemester-mcp'),common,{input,encoding:'utf8',timeout:45000,maxBuffer:32*1024*1024});need(!run.error&&run.status===0,'MCP process terminal');const response=run.stdout.trim().split('\n').map(JSON.parse).find(v=>v.id===2);need(!response.error,'MCP protocol');return response.result;};
 try{for(const [command,path]of [['invoices','invoices'],['journal','journal'],['trial-balance','trial-balance'],['income-statement','income-statement'],['vat','vat'],['bank','bank'],['periods','periods']]){const canonical=await read(prefix+path),c=cli(command,undefined,[]),m=rpc(command.replaceAll('-','_'),{company:'synthetic',});need(c.code===0&&c.data.ok===true&&isDeepStrictEqual(c.data.data,canonical),'full CLI canonical read');need(!m.isError&&isDeepStrictEqual(m.structuredContent.data,canonical),'full MCP canonical read');}
 for(const kind of ['full','simplified']){const body=issueInput(kind),http=await ownerPage.request.post(base+prefix+'invoices/preview',{headers:{origin:base},data:body});need(http.status()===200,'canonical full/private simplified preview');const bytes=await http.body(),c=cli('preview-invoice',body),m=rpc('preview_invoice',{company:'synthetic',body:JSON.stringify(body)});need(c.code===0&&c.data.ok===true&&isDeepStrictEqual(Buffer.from(c.data.data.preview.bytes),bytes),'CLI new invoice input and exact PDF parity');need(!m.isError&&isDeepStrictEqual(Buffer.from(m.structuredContent.data.preview.bytes),bytes),'MCP new invoice input and exact PDF parity');}
 const duplicatePost=cli('post-invoice',{invoiceDocumentId:issued.full.id,revenueAccountNo:'1000',confirm:true});need(duplicatePost.code===1&&duplicatePost.data.code==='server_refused','CLI known duplicate refuses without new posting');const duplicateCredit=rpc('credit_invoice',{company:'synthetic',body:JSON.stringify({invoiceDocumentId:issued.full.id,issueDate:today,reason:'Synthetic client duplicate',confirm:true}),confirm:true});need(duplicateCredit.isError===true&&[409,422].includes(duplicateCredit.structuredContent?.error?.status),'MCP known duplicate or closed-period refuses without new credit');need(isDeepStrictEqual(await snapshot(),before),'clients never create duplicate effects');return {httpCliMcpCanonicalEquality:true,fullAndSimplifiedPreviewInputAndPdfEquality:true,foreignBuyerDeliveryAndUnitInputParity:true,explicitKnownDuplicateMutationsRefused:true};}finally{unlinkSync(cookiePath);}}
for await(const line of createInterface({input:process.stdin,crlfDelay:Infinity})){try{const action=JSON.parse(line);let value;if(action==='authenticate')value=await authenticate();else if(action==='setup')value=await setup();else if(action==='issue')value=await issueAll();else if(action==='post')value=await postAll();else if(action==='credits-and-cash')value=await creditsAndCash();else if(action==='dated-close')value=await datedClose();else if(action==='freeze')value=await freeze();else if(action==='reopen')value=await reopen();else if(action==='restored')value=await restored();else if(action==='restored-reopen')value=await restoredReopen();else if(action.startsWith('client-reads'))value=await clientReads();else if(action.startsWith('archive-'))value=await archive(action.slice(8));else if(action==='finish'){await browser?.close();emit({ok:true,value:{finished:true}});break;}else throw Error('unknown phase');emit({ok:true,value});}catch(error){if(ownerId&&diagnosticPage)await ownerPage.screenshot({path:process.env.RM_INVOICE_METADATA_SCREENSHOT,fullPage:true}).catch(()=>{});emit({ok:false,step,detail,errorType:error?.name,errorMessage:String(error?.message).slice(0,1200),trace});await browser?.close().catch(()=>{});process.exitCode=1;break;}}
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
    report = {'status':'running','sourceRevision':subprocess.check_output(['git','rev-parse','HEAD'],cwd=REPO,text=True).strip(),'runnerSha256':digest(Path(__file__)),'browserHelperSha256':hashlib.sha256(BROWSER.encode()).hexdigest(),'canonicalOrigin':origin,'noOfflineIdentitySeeds':True,'ordinaryMacOsUser':True,'ownedProfile':str(work),'externalMailDelivered':False,'workflow':'explicit_invoice_metadata_sales_accounting','installedNativeQualified':False,'invoiceMetadataActualQualified':False,'sourceFreeRestoreQualified':False,'screenshot':str(args.evidence.with_suffix('.png')),'actions':[],'results':{}}
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
                argv += ['--restore-backup', str(work / 'invoice-metadata80-source.native-backup.json'), '--trusted-backup-key', str(work / 'trusted.pub'), '--confirm-target', str(root), '--source-unavailable']
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
            progress('operator:independent-signed-invoice-metadata80-' + name)
            path = work / ('invoice-metadata80-' + name + '.native-backup.json')
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
            copy = work / ('readonly-invoice-metadata80-' + name + '.sqlite')
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
            if len(result['issued_invoice_postings']) != 2 or len(result['credit_note_postings']) != 3 or len(result['invoice_payments']) != 3 or len(result['invoice_refunds']) != 3 or len(result['journal_entries']) != 11 or len(result['bank_transactions']) != 6:
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
            for kind,item in report['results']['freeze']['issued'].items():
                proof=original_proofs[str(item['id'])]
                pdf=base64.b64decode(proof['pdfBase64'],validate=True)
                rendered=subprocess.run(['pdftotext','-','-'],input=pdf,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=15)
                if rendered.returncode:
                    raise RuntimeError('Actual issued invoice PDF extraction failed')
                text = ' '.join(rendered.stdout.decode('utf-8').split())
                source = json.loads(base64.b64decode(proof['jsonBase64'], validate=True))
                expected = report['results']['freeze']['metadata'][kind]
                for field in ['deliveryDate','deliveryPeriodStart','deliveryPeriodEnd','unitCode']:
                    if source.get(field) != expected.get(field):
                        raise RuntimeError('Signed original exact delivery/unit metadata differs: '+field)
                if kind=='full':
                    if source['buyer']['vatOrCvr']!='DE123456789' or source['lines'][0].get('unitCode')!='H87':
                        raise RuntimeError('Signed foreign buyer or explicit line unit differs')
                    wanted = ['DE123456789', 'Leveringsperiode', expected['deliveryPeriodStart'], expected['deliveryPeriodEnd'], 'H87']
                    if any(value not in text for value in wanted):
                        raise RuntimeError('Issued full PDF omits supplied foreign buyer/delivery/unit metadata')
                else:
                    if any(field in source['lines'][0] for field in ['quantity','unitPriceExVat','unitCode']) or source.get('buyer'):
                        raise RuntimeError('Signed reduced invoice invents buyer, quantity, price or unit')
                    if any(value not in text for value in ['Forenklet faktura','Leveringsdato',expected['deliveryDate']]):
                        raise RuntimeError('Issued reduced PDF omits legal type or supplied delivery date')
            accounts = {row['id']:row['account_no'] for row in result['accounts']}
            frozen = report['results']['freeze']
            for kind, net_amount, vat_amount in [('full',Decimal('200'),Decimal('50')),('simplified',Decimal('80.06'),Decimal('20.01'))]:
                item=frozen['issued'][kind]
                original=[row for row in result['journal_lines'] if row['journal_entry_id']==item['entryId']]
                revenue=[row for row in original if accounts[row['account_id']]=='1000']
                vat=[row for row in original if accounts[row['account_id']]=='4200']
                if len(revenue)!=1 or len(vat)!=1 or revenue[0]['vat_code']!='DK_SALE_25' or Decimal(str(revenue[0]['credit_amount']))!=net_amount or Decimal(str(vat[0]['credit_amount']))!=vat_amount:
                    raise RuntimeError('Exact original private/simplified net and VAT provenance missing')
            net={account:sum((Decimal(str(row['debit_amount']))-Decimal(str(row['credit_amount'])) for row in result['journal_lines'] if accounts[row['account_id']]==account),Decimal(0)) for account in ['1100','1000','1800','4200']}
            if any(value!=0 for value in net.values()):
                raise RuntimeError('Final exact DKK cash/revenue/receivable/VAT effect differs')
            if result.get('vies_validations'):
                raise RuntimeError('Ordinary foreign buyer metadata must not infer VIES validation')
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
                    raise RuntimeError('Exact restored private sales evidence differs: '+table)
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
                ['openssl', 'req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', str(work / 'ca.key'), '-out', str(ca), '-days', '1', '-subj', '/CN=Synthetic Private Sales Auth CA', '-addext', 'basicConstraints=critical,CA:TRUE', '-addext', 'keyUsage=critical,keyCertSign,cRLSign'],
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
            config = {'runtimeProfile': 'local-user-macos', 'root': str(work / 'workspace'), 'brokerUid': os.getuid(), 'workspace': 'synthetic-native-invoice-metadata80', 'companySlug': 'synthetic', 'companyName': 'Synthetic Invoice Metadata Company', 'cookieKey': key(), 'mfaKey': key(), 'operatorActors': ['system:synthetic-installer'], 'accounts': [], 'firstOwner': {'actor': 'system:synthetic-installer', 'email': 'owner@example.invalid', 'name': 'Synthetic Owner', 'password': 'synthetic-owner-password-123'}, 'backupSigningKeyFile': str(work / 'signing.der'), 'authMail': {'endpoint': f'https://127.0.0.1:{mail_port}/mail', 'bearerToken': bearer, 'from': 'auth@example.invalid', 'idempotencySecret': secrets.token_urlsafe(32), 'trustedCaFile': str(ca)}}
            write_config()
            start(create=True)
            script = work / 'browser.mjs'
            script.write_text(BROWSER)
            controller = subprocess.Popen(['/opt/homebrew/bin/bun', str(script)], cwd=REPO, env={**os.environ, 'RM_INVOICE_METADATA_BASE': origin, 'RM_INVOICE_METADATA_WORK': str(work), 'RM_INVOICE_METADATA_SCREENSHOT': str(args.evidence.with_suffix('.png')), 'RM_INVOICE_METADATA_PLAYWRIGHT': str((REPO / 'node_modules/@playwright/test/index.mjs').resolve(strict=True))}, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, bufsize=1, start_new_session=True)
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
                raise RuntimeError('Fresh target reopen changed retained full private sales evidence')
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
            report.update({'status':'passed','currentAction':'complete','invoiceMetadataActualQualified':True,'httpCliMcpReadParityQualified':True,'snapshotCoverage':'four-independently-verified-invoice-metadata80-full-ledger-original-archives-fresh-owner-restore-and-reopen'})
            save()
            print('PASS: ordinary MFA foreign buyer, delivery range/date and explicit unit metadata on full and reduced simplified invoices, exact net/VAT and partial/final cash-credit-refund cycles, canonical clients and signed source-free recovery; '+str(args.evidence))
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
