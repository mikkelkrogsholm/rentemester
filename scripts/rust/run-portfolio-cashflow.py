#!/usr/bin/env python3
"""Ordinary-MFA two-company portfolio, historical bank cashflow and signed recovery.
Supplied artifacts are privately pinned; only a fresh synthetic macOS fixture is used.
No identity/business SQL seeds, provider effects, builds or implicit mutation retries.
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
REPO=Path(__file__).resolve().parents[2]
PREFIX='Rentemester-Synthetic-Portfolio-Cashflow-'
SIGNER="const c=require('node:crypto'),f=require('node:fs'),k=c.generateKeyPairSync('ed25519'),s=k.privateKey.export({format:'der',type:'pkcs8'}).subarray(-32),p=k.publicKey.export({format:'der',type:'spki'}).subarray(-32);f.writeFileSync(process.argv[1],Buffer.concat([Buffer.from('3051020101300506032b657004220420','hex'),s,Buffer.from('812100','hex'),p]),{mode:0o600});f.writeFileSync(process.argv[2],p,{mode:0o600});"
BROWSER = r'''








import {createHmac,createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {createInterface} from 'node:readline';
import {readFileSync,readdirSync,writeFileSync,unlinkSync,lstatSync,realpathSync} from 'node:fs';
import {join} from 'node:path';
import {isDeepStrictEqual} from 'node:util';
const {chromium}=await import(process.env.RM_PORTFOLIO76_PLAYWRIGHT);
const base=process.env.RM_PORTFOLIO76_BASE,work=process.env.RM_PORTFOLIO76_WORK;
if(!/^http:\/\/127\.0\.0\.1:\d+$/.test(base))throw Error('owned literal loopback origin required');
const owner={email:'owner@example.invalid',password:'synthetic-owner-password-123',codes:[]},member={email:'invited@example.invalid',password:'synthetic-invited-password-456',codes:[]};


let responseSequence=0;
function emit(value){const bytes=Buffer.from(JSON.stringify(value)),name=`browser-response-${String(++responseSequence).padStart(4,'0')}.json`;writeFileSync(join(work,name),bytes,{mode:0o600,flag:'wx'});console.log(JSON.stringify({sequence:responseSequence,bytes:bytes.length,sha256:createHash('sha256').update(bytes).digest('hex')}));}
let browser,ownerPage,memberPage,step='boot',detail=null,baseline,ownerId,memberId,invitationId,lastLogin=0;const trace=[];let diagnosticPage;
const need=(v,s)=>{if(!v){detail={assertion:s,evidence:detail};throw Error(s);}};
function totp(secret){let bits=0,count=0,bytes=[];for(const c of secret){const v='ABCDEFGHIJKLMNOPQRSTUVWXYZ234567'.indexOf(c);need(v>=0,'MFA setup key syntax');bits=(bits<<5)|v;count+=5;if(count>=8){count-=8;bytes.push((bits>>>count)&255);}}const counter=Buffer.alloc(8);counter.writeBigUInt64BE(BigInt(Math.floor(Date.now()/30000)));const d=createHmac('sha1',Buffer.from(bytes)).update(counter).digest(),o=d[19]&15;return String((d.readUInt32BE(o)&0x7fffffff)%1000000).padStart(6,'0');}
async function cooldown(){const wait=11000-(Date.now()-lastLogin);if(wait>0)await new Promise(r=>setTimeout(r,wait));lastLogin=Date.now();}
async function fetchJson(page,path,data){return page.evaluate(async({path,data})=>{const r=await fetch(path,data===undefined?{}:{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify(data)});let body;try{body=await r.json();}catch{body=null;}return{status:r.status,body};},{path,data});}
async function get(page,path){const r=await fetchJson(page,path);if(r.status!==200)detail={path,status:r.status,response:r.body};need(r.status===200,`canonical ${path} HTTP${r.status}`);return r.body;}
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
const slugs=['synthetic','synthetic-portfolio-shifted'],names={synthetic:'Synthetic Portfolio Primary','synthetic-portfolio-shifted':'Synthetic Portfolio Shifted'};
const display=v=>v===null?'Ikke tilgængelig':new Intl.NumberFormat('da-DK',{minimumFractionDigits:2,maximumFractionDigits:2}).format(v)+' kr.';
const prefix=slug=>`/api/companies/${slug}/`,hash=b=>createHash('sha256').update(b).digest('hex');
const read=p=>get(ownerPage,p);
const post=async(p,b,page=ownerPage)=>{const r=await fetchJson(page,p,b);if(![200,201].includes(r.status)||r.body?.ok!==true){detail={path:p,status:r.status,code:r.body?.code,errors:r.body?.errors};throw Error('confirmed domain mutation refused');}return r.body;};
const early='2025-09-01',late='2025-10-12',issueDate='2025-08-10',laterIssueDate='2025-10-10';
let stable,freshOwner,sourceCookie;const issued=[],originalsById={},statements=[],customers={};
function proof(v,label){if(!v){const evidence=detail;detail={assertion:label,evidence};throw Error(label);}}
async function portfolio(page=ownerPage,asOf=late){return get(page,`/api/portfolio?asOf=${asOf}`);}
async function flow(slug,selected=true){return read(prefix(slug)+'cashflow'+(selected?`?year=${encodeURIComponent(slug==='synthetic'?'2025':'2025/26')}`:''));}
async function snapshot(){const result={};for(const slug of slugs){const p=prefix(slug);result[slug]={journal:await read(p+'journal'),trialBalance:await read(p+'trial-balance'),invoices:await read(p+'invoices'),documents:await read(p+'documents'),bank:await read(p+'bank'),company:await read(p+'company')};}return result;}
async function saveOriginal(item){const p=prefix(item.slug),r=await ownerPage.request.get(base+p+`documents/${item.id}/file`);need(r.status()===200,'issued immutable JSON readable');const bytes=await r.body(),pdf=await ownerPage.request.get(base+p+`invoices/${item.id}/pdf`);need(pdf.status()===200,'issued immutable PDF readable');const pdfBytes=await pdf.body();need(pdfBytes.subarray(0,5).toString()==='%PDF-','actual immutable PDF bytes');const proof={jsonBase64:bytes.toString('base64'),jsonSha256:hash(bytes),pdfBase64:pdfBytes.toString('base64'),pdfSha256:hash(pdfBytes)};const key=item.slug+':'+item.id;if(originalsById[key])need(isDeepStrictEqual(originalsById[key],proof),'original JSON/PDF unchanged');else originalsById[key]=proof;}
async function originals(){for(const item of issued)await saveOriginal(item);return originalsById;}
async function setup(){
 step='ordinary-two-company-profiles-charts-explicit-owner-grants';
 const today=(await read(prefix('synthetic')+'scheduled-recurring')).scheduledRecurring.asOfDate;need(today>=late,'explicit fixture dates are nonfuture');
 await post('/api/companies',{name:names[slugs[1]],slug:slugs[1],fiscalYearStartMonth:'7',fiscalYearLabelStrategy:'span',vatPeriodType:'quarter'});
 for(const slug of slugs){await post('/api/workspace/members/company',{userId:ownerId,action:'grant',companySlug:slug,role:'owner'});const p=prefix(slug),r=await ownerPage.request.patch(base+p+'company',{headers:{origin:base},data:{cvr:slug==='synthetic'?'87654321':'12345678',address:'Synthetic Portfolio Street',postalCode:'1000',city:'Synthetic City',vatPeriodType:'quarter',paymentTermsDays:14}});need(r.status()===200,'ordinary exact company legal profile');for(const [accountNo,name,type,normalBalance]of [['1100','Synthetic receivables','asset','debit'],['1800','Synthetic bank','asset','debit'],['1000','Synthetic revenue','income','credit'],['4200','Synthetic VAT','vat','credit']])await post(p+'accounts',{accountNo,name,type,normalBalance});for(const [role,accountNo]of [['debtors','1100'],['bank','1800'],['output_vat','4200']])await post(p+'account-roles',{role,accountNo,confirm:true});await post(p+'bank-accounts',{name:'Synthetic Portfolio Bank',slug:'synthetic-portfolio-bank',currency:'DKK',ledgerAccountNo:'1800'});}
 const empty=await portfolio(ownerPage,early),flows=await Promise.all(slugs.map(slug=>flow(slug,false)));detail={portfolio:empty,cashflows:flows};proof(empty.portfolio.companyCount===2&&empty.portfolio.rollup.liquidity===null&&empty.portfolio.rollup.bankBalanceComplete===false,'empty bank is unknown, never explicit zero');for(const f of flows)proof(f.cashflow.totalIn===0&&f.cashflow.totalOut===0&&f.cashflow.openingBalance===null&&f.cashflow.closingBalance===null,'empty historical bank totals known zero while independent balances unknown');return{slugs,today,empty,flows};
}
async function importBank(slug,csv){const imported=await post(prefix(slug)+'bank/import',{account:'synthetic-portfolio-bank',csvContent:csv,confirm:true});statements.push({slug,bytesBase64:Buffer.from(csv).toString('base64'),sha256:hash(Buffer.from(csv)),receipt:imported});return imported;}
async function invoice(slug,net,date,label){const p=prefix(slug);const customer=customers[slug]??(customers[slug]=(await post(p+'customers',{name:'Synthetic Portfolio Buyer '+slug,address:'Synthetic Buyer Road',vatOrCvr:'DK11223344',paymentTermsDays:14,defaultCurrency:'DKK'})).customer);const result=await post(p+'invoices/issue',{issueDate:date,customerId:customer.id,vatTreatment:'standard',lines:[{description:'Synthetic Portfolio '+label,quantity:1,unitPriceExVat:net,taxClassification:'taxable',vatRate:.25}]});const item={slug,id:result.invoice?.documentId??result.documentId,number:result.invoice?.invoiceNumber??result.invoiceNumber,net,date};detail={slug,result};proof(Number.isSafeInteger(item.id),'actual issued invoice ID');const posted=await post(p+'invoices/post',{invoiceDocumentId:item.id,revenueAccountNo:'1000',confirm:true});item.posting=posted;issued.push(item);await saveOriginal(item);return item;}
async function sources(){
 step='ordinary-issued-posted-originals-and-independent-bank-source';await invoice(slugs[0],100,issueDate,'primary early');await invoice(slugs[1],200,issueDate,'shifted early');await invoice(slugs[0],300,laterIssueDate,'primary later');
 await importBank(slugs[0],'date;text;amount;currency;reference\n2025-08-11;Synthetic primary inflow;100;DKK;P76-PRIMARY-IN\n2025-08-12;Synthetic primary outflow;-30;DKK;P76-PRIMARY-OUT\n');
 await importBank(slugs[1],'date;text;amount;currency;reference;balance_after\n2025-08-11;Synthetic shifted inflow;40;DKK;P76-SHIFTED-IN;40\n2025-08-12;Synthetic shifted outflow;-40;DKK;P76-SHIFTED-OUT;0\n');
 return{issued,statements,ordinaryOriginals:true,journalCounts:{synthetic:2,'synthetic-portfolio-shifted':1}};
}
async function initialReports(){
 step='early-cutoff-and-unknown-versus-zero-bank';const before=await snapshot(),r=await portfolio(ownerPage,early),p=r.portfolio,primary=p.companies.find(c=>c.slug===slugs[0]),shifted=p.companies.find(c=>c.slug===slugs[1]);detail={portfolio:r};
 proof(p.companyCount===2&&p.consolidated===false&&p.currency==='DKK','visible two-company nonconsolidated native DKK scope');proof(primary?.omsaetning===100&&primary.resultat===100&&shifted?.omsaetning===200&&shifted.resultat===200&&p.rollup.omsaetning===300&&p.rollup.resultat===300,'asOf excludes actual later posted income');proof(primary.actualBankBalance===null&&shifted.actualBankBalance===0&&p.rollup.liquidity===null&&!p.rollup.bankBalanceComplete&&p.rollup.knownBankCompanyCount===1&&p.rollup.unknownBankCompanyCount===1,'unknown primary and genuine zero shifted bank never summed as zero');proof(primary.fiscalPeriodStart==='2025-01-01'&&shifted.fiscalPeriodStart==='2025-07-01'&&primary.periodEnd===early&&shifted.periodEnd===early,'per-company actual fiscal periods end at cutoff');proof(primary.openInvoiceTotal===125&&shifted.openInvoiceTotal===250,'later invoice excluded from earlier open balances');proof(primary.vat?.payable===25&&shifted.vat?.payable===50&&p.rollup.vatPayable===75,'VAT source window ends at exact earlier cutoff');await portfolioUi(early,r);need(isDeepStrictEqual(await snapshot(),before),'initial report/UI reads have no ledger effects');return{canonical:r,earlyCutoffQualified:true,unknownVersusZeroQualified:true};
}
async function portfolioUi(date,canonical,page=ownerPage){
 diagnosticPage=page;await page.goto(base+'/');await page.getByRole('heading',{name:'Portefølje',exact:true}).waitFor();await page.getByLabel('Pr. dato',{exact:true}).fill(date);const pending=page.waitForResponse(r=>{const u=new URL(r.url());return u.pathname==='/api/portfolio'&&u.searchParams.get('asOf')===date;});await page.getByRole('button',{name:'Hent portefølje igen',exact:true}).click();const response=await pending,body=await response.json();detail={canonical,renderedResponse:body,status:response.status()};proof(response.status()===200&&isDeepStrictEqual(body,canonical),'actual portfolio UI exact canonical dated response');for(const c of canonical.portfolio.companies){const card=page.getByRole('article',{name:c.name,exact:true});await card.getByRole('heading',{name:c.name,exact:true}).waitFor();for(const [label,want]of [['Omsætning',display(c.omsaetning)],['Resultat',display(c.resultat)],['Banksaldo',display(c.actualBankBalance)]]){const value=card.getByText(label,{exact:true}).locator('..').locator('dd');await value.filter({hasText:want}).waitFor();}await card.getByRole('link',{name:'Se pengestrøm',exact:true}).waitFor();}return{date,renderedCompanyCount:canonical.portfolio.companyCount};
}
async function completeReports(){
 step='later-known-bank-closing-and-complete-portfolio';await importBank(slugs[0],'date;text;amount;currency;reference;balance_after\n2025-10-11;Synthetic later primary inflow;50;DKK;P76-PRIMARY-LATER;120\n');const before=await snapshot(),r=await portfolio(ownerPage,late),p=r.portfolio;detail={portfolio:r};proof(p.rollup.resultat===600&&p.rollup.omsaetning===600&&p.rollup.liquidity===120&&p.rollup.bankBalanceComplete&&p.rollup.knownBankCompanyCount===2&&p.rollup.unknownBankCompanyCount===0,'later full visible income and independent complete bank aggregate');proof(p.companies.find(c=>c.slug===slugs[0]).vat?.payable===75&&p.companies.find(c=>c.slug===slugs[1]).vat?.payable===0&&p.rollup.vatPayable===75,'later VAT quarter uses only October source lines');proof(p.companies.find(c=>c.slug===slugs[0]).openInvoiceTotal===500&&p.companies.find(c=>c.slug===slugs[1]).openInvoiceTotal===250,'actual open invoice facts include later issued gross');await portfolioUi(late,r);
 const flows={};for(const slug of slugs){const value=await flow(slug),f=value.cashflow;flows[slug]=value;detail={slug,cashflow:value};const primary=slug===slugs[0];proof(f.selectedYear===(primary?'2025':'2025/26')&&f.periodStart===(primary?'2025-01-01':'2025-07-01')&&f.periodEnd===(primary?'2025-12-31':'2026-06-30'),'historical bank exact native fiscal descriptor');proof(f.months.length===12&&f.months[0].month===(primary?1:7)&&f.months[11].month===(primary?12:6),'twelve true chronological fiscal buckets');proof(f.totalIn===(primary?150:40)&&f.totalOut===(primary?30:40)&&f.closingBalance===(primary?120:0)&&f.openingBalance===null,'actual bank inflows outflows and independent closing with unknown opening');const august=f.months.find(m=>m.label==='2025-08');proof(august.indbetalinger===(primary?100:40)&&august.udbetalinger===(primary?30:40)&&august.netto===(primary?70:0),'exact source-backed August bucket');if(primary)proof(f.months.find(m=>m.label==='2025-10').indbetalinger===50,'later October bank movement in proper bucket');await cashflowUi(slug,value);}
 need(isDeepStrictEqual(await snapshot(),before),'complete overview/history/forecast reads have no effects');return{canonical:r,flows,completeBalanceAggregateQualified:true};
}
async function cashflowUi(slug,canonical){
 diagnosticPage=ownerPage;await ownerPage.goto(base+`/companies/${slug}/likviditet?source=live`);const region=ownerPage.getByRole('region',{name:'Historisk pengestrøm',exact:true});await region.waitFor();const select=region.getByLabel('Vælg regnskabsår',{exact:true});const value=await select.locator('option').evaluateAll((options,year)=>options.find(o=>o.value.replace(/^live:/,'')===year)?.value,canonical.cashflow.selectedYear);need(typeof value==='string','actual fiscal selector includes exact server descriptor');const pending=ownerPage.waitForResponse(r=>{const u=new URL(r.url());return u.pathname===prefix(slug)+'cashflow'&&u.searchParams.get('year')===canonical.cashflow.selectedYear;});await select.selectOption(value);const response=await pending,body=await response.json();detail={slug,status:response.status(),canonical,response:body};proof(response.status()===200&&isDeepStrictEqual(body,canonical),'fiscal selector actual response equals canonical history');await ownerPage.getByRole('table',{name:'Faktiske månedlige bankbevægelser',exact:true}).waitFor();for(const month of canonical.cashflow.months)await region.getByRole('rowheader',{name:month.label,exact:true}).waitFor();for(const [label,expected]of [['Primo',canonical.cashflow.openingBalance],['Indbetalinger',canonical.cashflow.totalIn],['Udbetalinger',canonical.cashflow.totalOut],['Ultimo',canonical.cashflow.closingBalance]]){const cell=region.getByText(label,{exact:true}).locator('..');await cell.getByText(display(expected),{exact:true}).waitFor();}await ownerPage.getByRole('heading',{name:'13 ugers likviditetsprognose',exact:true}).waitFor();const forecast=ownerPage.getByRole('table',{name:'13 ugers prognose',exact:true});await forecast.waitFor();need(await forecast.locator('tbody tr').count()===13,'existing native thirteen-week forecast retained alongside history');return{selectedYear:canonical.cashflow.selectedYear,historicalMonths:12,forecastWeeks:13};}

async function rights(){
 step='reviewer-current-single-company-only-then-revoke';const before=await snapshot(),r=await portfolio(memberPage,late);detail={restricted:r};proof(r.portfolio.companyCount===1&&r.portfolio.companies[0].slug===slugs[0]&&r.portfolio.rollup.omsaetning===400&&r.portfolio.rollup.liquidity===120&&!JSON.stringify(r).includes(slugs[1])&&!JSON.stringify(r).includes(names[slugs[1]]),'restricted current reviewer cards and totals contain only primary');await portfolioUi(late,r,memberPage);await post('/api/workspace/members/company',{userId:memberId,action:'revoke',companySlug:slugs[0]});const denied=await fetchJson(memberPage,`/api/portfolio?asOf=${late}`);detail={denied};proof(denied.status===403&&!JSON.stringify(denied.body).includes(names[slugs[1]])&&!JSON.stringify(denied.body).includes(names[slugs[0]]),'zero-current-company reviewer denied without names or totals');proof((await portfolio(ownerPage,late)).portfolio.companyCount===2,'owner current rights retained');await post('/api/workspace/members/company',{userId:memberId,action:'grant',companySlug:slugs[0],role:'reviewer'});need(isDeepStrictEqual(await snapshot(),before),'reviewer revoke/regrant changes no company books');return{restricted:r,revokedStatus:denied.status,hiddenNamesAbsent:true,reviewerRegrantedPrimaryOnly:true};
}
async function freeze(){step='canonical-stable-two-company-books-originals-and-reports';sourceCookie=(await ownerPage.context().cookies(base)).map(c=>`${c.name}=${c.value}`).join('; ');stable={books:await snapshot(),early:await portfolio(ownerPage,early),late:await portfolio(ownerPage,late),flows:{}};for(const slug of slugs)stable.flows[slug]=await flow(slug);await originals();await portfolioUi(late,stable.late);await ownerPage.screenshot({path:process.env.RM_PORTFOLIO76_SCREENSHOT,fullPage:true});return{canonical:stable,issued,originals:originalsById,statements};}
async function unchanged(){detail={expected:stable,actual:{books:await snapshot(),early:await portfolio(ownerPage,early),late:await portfolio(ownerPage,late),flows:{}}};for(const slug of slugs)detail.actual.flows[slug]=await flow(slug);proof(isDeepStrictEqual(detail.actual,stable),'exact retained books, UUIDs, dated portfolio and fiscal bank reports');await originals();}
async function archive(name){step='signed-portfolio-cashflow-'+name;const r=await ownerPage.request.post(base+'/api/workspace/backup',{headers:{origin:base},data:{confirm:true}});if(r.status()!==200||r.headers()['content-type']!=='application/vnd.rentemester.native-backup+json'){let body;try{body=await r.json();}catch{}detail={status:r.status(),code:body?.code,errors:body?.errors};throw Error('signed whole-workspace backup refused');}const bytes=await r.body();need(bytes.length>0&&bytes.length<=96*1024*1024,'complete bounded signed archive');writeFileSync(join(work,`portfolio-cashflow-${name}.native-backup.json`),bytes,{mode:0o600,flag:'wx'});await unchanged();return{sha256:hash(bytes),bytes:bytes.length};}
async function reopen(){step='source-normal-mfa-restart';await ownerPage.context().clearCookies();await login(ownerPage,owner);await unchanged();return{freshMfaRestart:true};}
async function restored(){step='source-free-normal-new-owner';ownerPage=await newPage();freshOwner={email:'restored@example.invalid',password:'synthetic-restored-password-789',codes:[]};await ownerPage.goto((await mail('verification',freshOwner.email)).actionUrl);const me=await enroll(ownerPage,freshOwner);need(me.user.id!==ownerId,'historical owner does not become restored live identity');await post('/api/workspace/members/company',{userId:me.user.id,action:'grant',companySlug:slugs[1],role:'owner'});const denied=await ownerPage.request.get(base+prefix(slugs[0])+'journal',{headers:{cookie:sourceCookie}});need([401,403].includes(denied.status()),'old source session has no restored authority');await unchanged();return{sourceFreeRestoreQualified:true,oldAuthorityRefused:true,explicitFreshOwnerShiftedCompanyGrant:true};}
async function restoredReopen(){step='target-normal-mfa-reopen';await ownerPage.context().clearCookies();await login(ownerPage,freshOwner);await unchanged();return{restoredTargetReopenQualified:true};}
let clientPhase=0;
async function clientReads(){step='canonical-http-cli-mcp-dated-reports';const before=await snapshot(),file=join(realpathSync(work),`portfolio-client-${++clientPhase}.cookie`);writeFileSync(file,(await ownerPage.context().cookies(base)).map(c=>`${c.name}=${c.value}`).join('; '),{mode:0o600,flag:'wx'});const common=['--server',base,'--cookie-file',file,'--allow-loopback-http'];try{for(const [command,args,flags,canonical]of [['portfolio',{asOf:early},['--as-of',early],await portfolio(ownerPage,early)],['portfolio',{asOf:late},['--as-of',late],await portfolio(ownerPage,late)],...await Promise.all(slugs.map(async slug=>['cashflow',{company:slug,year:slug===slugs[0]?'2025':'2025/26'},['--company',slug,'--year',slug===slugs[0]?'2025':'2025/26'],await flow(slug)]))]){const cli=spawnSync(join(work,'rentemester-rust'),['remote',command,...common,...flags],{encoding:'utf8',timeout:45000,maxBuffer:32*1024*1024});let c;try{c=JSON.parse(cli.stdout);}catch{}detail={command,args,client:'cli',status:cli.status,error:cli.error?.code,canonical,response:c};proof(!cli.error&&cli.status===0&&c?.ok===true&&isDeepStrictEqual(c.data,canonical),'full CLI canonical portfolio/cashflow equality');const input=[{jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'synthetic-portfolio-cashflow',version:'1'}}},{jsonrpc:'2.0',method:'notifications/initialized'},{jsonrpc:'2.0',id:2,method:'tools/call',params:{name:command,arguments:args}}].map(JSON.stringify).join('\n')+'\n';const rpc=spawnSync(join(work,'rentemester-mcp'),common,{input,encoding:'utf8',timeout:45000,maxBuffer:32*1024*1024});let m;try{m=rpc.stdout.trim().split('\n').map(JSON.parse).find(v=>v.id===2)?.result;}catch{}detail={command,args,client:'mcp',status:rpc.status,error:rpc.error?.code,canonical,response:m};proof(!rpc.error&&rpc.status===0&&!m?.isError&&m?.structuredContent?.ok===true&&isDeepStrictEqual(m.structuredContent.data,canonical),'full MCP canonical portfolio/cashflow equality');}need(isDeepStrictEqual(await snapshot(),before),'read clients no financial or source mutation');return{httpCliMcpCanonicalEquality:true,queries:4,readOnly:true};}finally{unlinkSync(file);}}
for await(const line of createInterface({input:process.stdin,crlfDelay:Infinity})){try{const action=JSON.parse(line);let value;if(action==='authenticate')value=await authenticate();else if(action==='setup')value=await setup();else if(action==='sources')value=await sources();else if(action==='initial-reports')value=await initialReports();else if(action==='complete-reports')value=await completeReports();else if(action==='rights')value=await rights();else if(action==='freeze')value=await freeze();else if(action==='reopen')value=await reopen();else if(action==='restored')value=await restored();else if(action==='restored-reopen')value=await restoredReopen();else if(action.startsWith('client-reads'))value=await clientReads();else if(action.startsWith('archive-'))value=await archive(action.slice(8));else if(action==='finish'){await browser?.close();emit({ok:true,value:{finished:true}});break;}else throw Error('unknown phase');emit({ok:true,value});}catch(error){let pageState;try{pageState=await Promise.race([diagnosticPage?.evaluate(()=>{const authentication=Boolean(document.querySelector('input[type=password]'))||/Recovery codes|Opsætningsnøgle/.test(document.body.innerText);return{path:location.pathname,authentication,headings:[...document.querySelectorAll('h1,h2,h3')].map(n=>n.textContent),body:authentication?'[authentication details omitted]':document.body.innerText.slice(0,12000)};}),new Promise(resolve=>setTimeout(()=>resolve({unavailable:true}),2000))]);}catch{}if(ownerId&&diagnosticPage&&pageState?.authentication===false)await diagnosticPage.screenshot({path:process.env.RM_PORTFOLIO76_SCREENSHOT,fullPage:true,timeout:2000}).catch(()=>{});emit({ok:false,step,detail,pageState,errorType:error?.name,errorMessage:String(error?.message).slice(0,1200),trace});await browser?.close().catch(()=>{});process.exitCode=1;break;}}
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
    report = {'status':'running','sourceRevision':subprocess.check_output(['git','rev-parse','HEAD'],cwd=REPO,text=True).strip(),'runnerSha256':digest(Path(__file__)),'browserHelperSha256':hashlib.sha256(BROWSER.encode()).hexdigest(),'canonicalOrigin':origin,'noOfflineIdentitySeeds':True,'ordinaryMacOsUser':True,'ownedProfile':str(work),'externalMailDelivered':False,'workflow':'explicit_portfolio_cashflow_overview','installedNativeQualified':False,'portfolioCashflowActualQualified':False,'sourceFreeRestoreQualified':False,'screenshot':str(args.evidence.with_suffix('.png')),'actions':[],'results':{}}
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
                self.do_POST()

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
                argv += ['--restore-backup', str(work / 'portfolio-cashflow-source.native-backup.json'), '--trusted-backup-key', str(work / 'trusted.pub'), '--confirm-target', str(root), '--source-unavailable']
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
            progress('operator:independent-signed-portfolio-cashflow-' + name)
            path = work / ('portfolio-cashflow-' + name + '.native-backup.json')
            receipt = report['results']['archive-' + name]
            if digest(path) != receipt['sha256']:
                raise RuntimeError('Exact signed archive bytes changed')
            verified = subprocess.run([str(pinned['server_binary']), '--verify-backup', str(path), '--trusted-backup-key', str(trusted)], cwd=REPO, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=60)
            if verified.returncode:
                raise RuntimeError('Independent trusted-key verification refused')
            verification = json.loads(verified.stdout)
            if verification != {'ok':True,'format':'rentemester-native-backup/1','companies':2,'scope':'signed_inventory_current_schema_integrity_journal_chain_and_originals'}:
                raise RuntimeError('Independent signature/inventory/Owner validation receipt differs')
            wire = json.loads(path.read_bytes())
            entries = {e['path']: base64.b64decode(e['bytesBase64'], validate=True) for e in wire['entries']}
            if len(entries) != len(wire['entries']):
                raise RuntimeError('Duplicate signed entry path')
            for raw in [*entries.values(), base64.b64decode(wire['manifestBase64'], validate=True)]:
                if any(secret.encode() in raw for secret in excluded_secrets if secret):
                    raise RuntimeError('Private credential present in signed inventory')
            result = {}
            def cell(value):
                return {'bytesBase64': base64.b64encode(value).decode()} if isinstance(value, bytes) else value
            for slug, journals, bank_rows, invoices in [('synthetic',2,3,2),('synthetic-portfolio-shifted',1,2,1)]:
                raw = entries.get(slug+'/data/ledger.sqlite')
                if not raw or len(raw)>32*1024*1024:
                    raise RuntimeError('Bounded signed company database missing: '+slug)
                copy=work/('readonly-portfolio-'+slug+'-'+name+'.sqlite')
                tables={}
                try:
                    with copy.open('xb') as f:
                        f.write(raw)
                    db=sqlite3.connect(copy.as_uri()+'?mode=ro',uri=True)
                    try:
                        db.execute('PRAGMA query_only=ON')
                        names=[row[0] for row in db.execute("SELECT name FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")]
                        for table in names:
                            query=db.execute('SELECT * FROM "'+table.replace('"','""')+'" ORDER BY rowid')
                            tables[table]=[{k:cell(v) for k,v in zip([c[0] for c in query.description],row)} for row in query.fetchall()]
                    finally:
                        db.close()
                finally:
                    copy.unlink(missing_ok=True)
                if len(tables['journal_entries'])!=journals or len(tables['issued_invoice_postings'])!=invoices or len(tables['bank_transactions'])!=bank_rows:
                    raise RuntimeError('Exact company posting/invoice/bank counts differ: '+slug)
                for forbidden in ['credit_note_postings','invoice_payments','invoice_refunds']:
                    if tables[forbidden]:
                        raise RuntimeError('Read-only overview created unsupported economic effect: '+forbidden)
                result[slug]=tables
            originals={key:hashlib.sha256(raw).hexdigest() for key,raw in entries.items() if '/documents/' in key or '/bank/' in key}
            for compound,proof in report['results']['freeze']['originals'].items():
                slug=compound.split(':',1)[0]
                for field in ['jsonBase64','pdfBase64']:
                    wanted=base64.b64decode(proof[field],validate=True)
                    if not any(raw==wanted for key,raw in entries.items() if key.startswith(slug+'/documents/')):
                        raise RuntimeError('Exact original invoice JSON/PDF absent from signed company inventory')
            for statement in report['results']['freeze']['statements']:
                raw=base64.b64decode(statement['bytesBase64'],validate=True)
                if hashlib.sha256(raw).hexdigest()!=statement['sha256'] or not any(raw==value for key,value in entries.items() if key.startswith(statement['slug']+'/')):
                    raise RuntimeError('Exact bank source original absent from signed company inventory')
            result['signedOriginalHashes']=originals
            report.setdefault('snapshots',{})[name]={'sha256':receipt['sha256'],'trustedPublicKeySha256':digest(trusted),'privateCredentialsExcluded':True,'verification':verification,'tables':result}
            save()
            return result

        def preserved(original, newer):
            if original['signedOriginalHashes']!=newer['signedOriginalHashes']:
                raise RuntimeError('Exact signed company originals differ')
            for slug in ['synthetic','synthetic-portfolio-shifted']:
                if original[slug].keys()!=newer[slug].keys():
                    raise RuntimeError('Company table inventory differs after restore')
                for table, rows in original[slug].items():
                    current=newer[slug][table]
                    if table!='audit_log':
                        if rows!=current:
                            raise RuntimeError('Exact restored company table differs: '+slug+'/'+table)
                        continue
                    historical={r['id']:r for r in rows}
                    retained={r['id']:r for r in current}
                    if any(retained.get(key)!=row for key,row in historical.items()):
                        raise RuntimeError('Original audit prefix/provenance changed')
                    appended=[row for row in current if row['id'] not in historical]
                    if any(row['event_type'] not in ['native_restore_receipt_boundary','native_restore_scheduled_boundary'] or row['actor']!='system:portable-restore' for row in appended):
                        raise RuntimeError('Unexpected company audit effect after read-only restore workflow')

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
                ['openssl', 'req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', str(work / 'ca.key'), '-out', str(ca), '-days', '1', '-subj', '/CN=Synthetic Portfolio Cashflow Auth CA', '-addext', 'basicConstraints=critical,CA:TRUE', '-addext', 'keyUsage=critical,keyCertSign,cRLSign'],
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
            config = {'runtimeProfile': 'local-user-macos', 'root': str(work / 'workspace'), 'brokerUid': os.getuid(), 'workspace': 'synthetic-native-portfolio-cashflow', 'companySlug': 'synthetic', 'companyName': 'Synthetic Portfolio Primary', 'cookieKey': key(), 'mfaKey': key(), 'operatorActors': ['system:synthetic-installer'], 'accounts': [], 'firstOwner': {'actor': 'system:synthetic-installer', 'email': 'owner@example.invalid', 'name': 'Synthetic Owner', 'password': 'synthetic-owner-password-123'}, 'backupSigningKeyFile': str(work / 'signing.der'), 'authMail': {'endpoint': f'https://127.0.0.1:{mail_port}/mail', 'bearerToken': bearer, 'from': 'auth@example.invalid', 'idempotencySecret': secrets.token_urlsafe(32), 'trustedCaFile': str(ca)}}
            excluded_secrets=[bearer,config['authMail']['idempotencySecret'],config['cookieKey'],config['mfaKey'],'synthetic-owner-password-123','synthetic-invited-password-456','synthetic-restored-password-789']
            write_config()
            start(create=True)
            script = work / 'browser.mjs'
            script.write_text(BROWSER)
            controller = subprocess.Popen(['/opt/homebrew/bin/bun', str(script)], cwd=REPO, env={**os.environ, 'RM_PORTFOLIO76_BASE': origin, 'RM_PORTFOLIO76_WORK': str(work), 'RM_PORTFOLIO76_SCREENSHOT': str(args.evidence.with_suffix('.png')), 'RM_PORTFOLIO76_PLAYWRIGHT': str((REPO / 'node_modules/@playwright/test/index.mjs').resolve(strict=True))}, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, bufsize=1, start_new_session=True)
            report['browserControllerPid'] = controller.pid
            phase('authenticate')
            phase('setup')
            phase('sources')
            phase('initial-reports')
            phase('complete-reports')
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
            excluded_secrets.extend([config['cookieKey'],config['mfaKey'],'synthetic-owner-password-123','synthetic-invited-password-456','synthetic-restored-password-789'])
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
                raise RuntimeError('Fresh target reopen changed retained full two-company evidence')
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
                report['gateway']={'verifiedHttps':True,'acceptedMessages':len(gateway_rows),'requests':list(gateway_requests)}
            report.update({'status':'passed','currentAction':'complete','portfolioCashflowActualQualified':True,'httpCliMcpReadParityQualified':True,'snapshotCoverage':'four-independently-verified-two-company-ledger-original-archives-fresh-owner-restore-and-reopen'})
            save()
            print('PASS: ordinary MFA dated two-company portfolio/history/forecast, current access, canonical clients and signed source-free recovery; '+str(args.evidence))
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
