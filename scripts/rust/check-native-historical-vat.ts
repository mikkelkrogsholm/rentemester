import type { Page } from '@playwright/test';
import { spawnSync } from 'node:child_process';
import { createHash, randomUUID } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { isDeepStrictEqual as equal } from 'node:util';

import { dineroFixture } from './check-native-dinero-import.ts';

/** Frozen from importer semantics: one two-line opening plus one three-line voucher.
 * Retained JSON source package only, no supporting invoices or scanner/provider calls.
 * DKK live Dinero only; archive/unattested/general imports and submission not qualified.
 */
export const historicalVatExpected = Object.freeze({ gross:125, purchaseBase25:100, inputVat:25, outputVat:0, netVatPayable:-25, journalEntryCount:2, linesConsidered:5, scannerRequests:0, providerRequests:0, documents:0, euTotalValue:0, ossConsumerSalesBase:0 });
export function historicalVatFixture(year:string) {
  if(!/^\d{4}$/.test(year))throw Error('Exact current year required');
  const entries=[dineroFixture('0,000000','none').entries[0],
    [year+'/Kontoplan.csv','Nummer;Navn;Type;Momstype;Total\n1500;Synthetic opening asset;Aktiv;;10,00\n2000;Synthetic supplies;Udgift;I25;100,00\n5510;Bankkonto;Aktiv;;0,00\n5520;Tilgodehavender fra salg;Aktiv;;0,00\n60010;Synthetic opening equity;Passiv;;-10,00\n61000;Synthetic financing;Passiv;;-125,00\n64000;Salgsmoms;Passiv;;0,00\n64060;Købsmoms;Passiv;;25,00\n6800;Leverandørgæld;Passiv;;0,00\n'],
    [year+'/Posteringer.csv',`Konto;Kontonavn;Dato;Bilag;Bilagstype;Tekst;Momstype;Beløb;Saldo
1500;Synthetic opening asset;${year}-01-01;0;;Primobeholdning;;10,000000;10,000000
60010;Synthetic opening equity;${year}-01-01;0;;Primobeholdning;;-10,000000;-10,000000
2000;Synthetic supplies;${year}-03-20;1;Finansbilag;Synthetic purchase;I25;100,000000;100,000000
64060;Købsmoms;${year}-03-20;1;Finansbilag;Synthetic purchase;;25,000000;25,000000
61000;Synthetic financing;${year}-03-20;1;Finansbilag;Synthetic purchase;;-125,000000;-125,000000
`],
    [year+'/SaldoBalance.csv','Konto;Kontonavn;Beløb\n5510;Bankkonto;0,000000\n5520;Tilgodehavender fra salg;0,000000\n6800;Leverandørgæld;0,000000\n']];
  const bytes=Buffer.from(JSON.stringify({format:'rentemester-dinero-source/1',currency:'DKK',files:entries.map(([path,content])=>({path,contentBase64:Buffer.from(content).toString('base64')}))}));
  return {bytes,body:{fileName:'synthetic-historical-vat175.json',contentBase64:bytes.toString('base64')}};
}
export function createHistoricalVatProof(base:string,work:string,onStep:(s:string)=>void) {
  const root='/api/companies/synthetic/', api=root+'original/vat/', cookie=join(work,'vat175.cookie'), key=join(work,'vat175.key');
  const observations:any[]=[];let owner:Page, reviewer:Page, service:any, stable:any, from:string,to:string,fixture:ReturnType<typeof historicalVatFixture>,receipt:any,ran=false;
  const need=(v:unknown,s:string)=>{if(!v)throw Error('historical_vat: '+s);};
  function record(v:any){observations.push(v);writeFileSync(join(work,'vat175-observations.json'),JSON.stringify(observations),{mode:0o600});}
  async function http(path:string,data?:any,status=200,page=owner,method='POST') {
    onStep((data===undefined?'GET':method)+':'+path);
    const r=data===undefined?await page.request.get(base+path):await page.request.fetch(base+path,{method,headers:{origin:base},data});
    const value=await r.json();record({path,status:r.status(),value:path==='/api/workspace/service-principals'?{...value,secret:'[redacted]'}:value});
    need(r.status()===status,path+' expected '+status+' got '+r.status());return value;
  }
  const admin=(path:string,data:any)=>http('/api/workspace/'+path,data,path==='service-principals'?201:200);
  async function cookies(){writeFileSync(cookie,(await owner.context().cookies(base)).map(c=>c.name+'='+c.value).join('; '),{mode:0o600});}
  const query=()=>new URLSearchParams({from,to}).toString();
  function cli(group:string,op:string,flags:any={},asService=false,exit=0,company='synthetic') {
    onStep('cli:'+group+':'+op);
    const file=asService?key:cookie,secret=readFileSync(file,'utf8').trim();
    const r=spawnSync(join(work,'rentemester-rust'),[group,op,'--server',base,'--allow-loopback-http',asService?'--service-key-file':'--cookie-file',file,'--company',company,'--json',...Object.entries(flags).flatMap(([k,v])=>['--'+k,String(v)])],{encoding:'utf8',timeout:45000,maxBuffer:16*1024*1024});
    need(!r.stdout.includes(secret)&&!r.stderr.includes(secret),'CLI credential privacy');record({group,op,flags,asService,company,exit:r.status,stdout:r.stdout,stderr:r.stderr,terminalError:r.error?.name});
    need(!r.error&&r.status===exit,'CLI exact exit; never retry unknown writes');if(exit===2)return;
    return JSON.parse(r.stdout);
  }
  function mcp(op:string,asService=false,ok=true,company='synthetic') {
    onStep('mcp:'+op);const file=asService?key:cookie,secret=readFileSync(file,'utf8').trim();
    const input=[{jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'original-vat175',version:'1'}}},{jsonrpc:'2.0',method:'notifications/initialized'},{jsonrpc:'2.0',id:2,method:'tools/call',params:{name:'vat_'+op.replaceAll('-','_'),arguments:{company,from,to}}}].map(v=>JSON.stringify(v)).join('\n')+'\n';
    const r=spawnSync(join(work,'rentemester-mcp'),['--server',base,'--allow-loopback-http',asService?'--service-key-file':'--cookie-file',file],{input,encoding:'utf8',timeout:45000,maxBuffer:16*1024*1024});
    need(!r.stdout.includes(secret)&&!r.stderr.includes(secret),'MCP credential privacy');record({kind:'mcp',op,asService,company,exit:r.status,stdout:r.stdout,stderr:r.stderr,terminalError:r.error?.name});need(!r.error&&r.status===0,'bounded MCP terminal');
    const reply=r.stdout.trim().split('\n').map(s=>JSON.parse(s)).find(v=>v.id===2),result=reply?.result,v=result?.structuredContent;
    need(!reply?.error&&result.content.length===1&&equal(JSON.parse(result.content[0].text),v),'MCP text/structured equality');need(v.ok===ok&&result.isError===!ok&&Array.isArray(v.errors),'source MCP outcome');
    if(!ok){need(!Object.hasOwn(v,'data')&&v.errors.length>0,'ALL source false omit data');return v;}
    need(!Object.hasOwn(v.data,'ok'),'source not native envelope');return {ok:true,...v.data,errors:v.errors,...(v.appliedRules?{appliedRules:v.appliedRules}:{})};
  }
  async function snapshot(){const v:any={};for(const p of ['journal','trial-balance','bank','documents','periods','imports/dinero'])v[p]=await http(root+p+(['journal','trial-balance','bank'].includes(p)?'?year='+from.slice(0,4)+(['journal','trial-balance'].includes(p)?'&source=live':''):''));v.native=await http(root+'vat?year='+from.slice(0,4));for(const op of ['report','eu-sales-list','oss-report','filing'])v[op]=await http(api+op+'?'+query());return v;}
  async function originals(){const r=await owner.request.get(base+root+`imports/dinero/${receipt.attemptId}/original`);need(r.status()===200&&(await r.body()).equals(fixture.bytes),'immutable retained Dinero source package');}
  async function parity(){for(const op of ['report','eu-sales-list','oss-report','filing','momsangivelse'])need(equal(cli('vat',op,{from,to}),stable[op==='momsangivelse'?'filing':op]),'flat CLI exact '+op);for(const op of ['report','eu-sales-list','oss-report'])need(equal(mcp(op),stable[op]),'source MCP exact '+op);}
  async function run(page:Page,member:Page){
    need(!ran,'one finite fixture, no write replay');ran=true;owner=page;reviewer=member;
    record({expected:historicalVatExpected,nonemptyActual:'one imported DKK purchase and balanced opening',newProviderFixtures:0});await cookies();
    const today=(await http(root+'scheduled-recurring')).scheduledRecurring.asOfDate,year=today.slice(0,4);from=year+'-01-01';to=year+'-03-31';
    await http(root+'company',{cvr:'87654321',address:'Synthetic VAT Street 1',postalCode:'1000',city:'Synthetic City',vatPeriodType:'quarter',paymentTermsDays:14},200,owner,'PATCH');
    fixture=historicalVatFixture(year);
    const plan=(await http(root+'imports/dinero/plan',fixture.body)).dineroImport;
    need(plan.status==='ready'&&equal(plan.counts,{accounts:9,openingLines:2,historicalVouchers:1,importedInvoices:0,importedEvents:0,archiveYears:0}),'frozen exact ordinary import plan counts');
    need(plan.controls.length===0,'zero real bank/debtor/creditor source controls');
    receipt=(await http(root+'imports/dinero/apply',{...fixture.body,planHash:plan.planHash,idempotencyKey:'synthetic-historical-vat175-once',confirm:true})).dineroImport;
    need(receipt.attemptId>0&&receipt.journalCount===1&&receipt.openingBalance,'ordinary accepted import with nonzero opening');
    const native=(await http(root+'vat?year='+year)).vat;
    need(native.periodStart===from&&native.periodEnd===to&&native.inputVat===25&&native.outputVat===0&&native.outputVatAdjustment===0&&native.payable===-25&&native.vatReportErrors.length===0,'independent native HTTP VAT');
    const report=await http(api+'report?'+query());
    for(const k of ['purchaseBase25','inputVat','outputVat','netVatPayable','journalEntryCount','linesConsidered'] as const)need(report[k]===historicalVatExpected[k],'independent report '+k);
    need(report.totalJournalEntryCount===2&&report.totalLinesConsidered===5&&report.reversedJournalEntryCount===0&&report.reversalJournalEntryCount===0&&report.reversedLinesConsidered===0&&report.reversalLinesConsidered===0,'disjoint source counts');
    need(report.rubrikker.kobsmoms===25&&report.rubrikker.salgsmoms===0&&report.rubrikker.momstilsvar===-25,'independent source rubrics');
    const open=await http(api+'filing?'+query(),undefined,422);need(open.ok===false&&open.periodStatus==='open'&&open.errors.length>0,'open filing refused');
    cli('vat','filing',{from,to},false,1);cli('vat','momsangivelse',{from,to},false,1);
    const packet=cli('period','readiness',{from,to}).packet;need(packet.blockers===0,'all ordinary source/accounting controls ready, no bypass');
    const review=cli('period','review',{from,to,'packet-hash':packet.hash,confirm:'yes'});need(equal(review.packet,packet),'exact authorized reviewed packet');
    const closed=cli('period','close',{from,to,'review-id':review.id,'packet-hash':packet.hash,kind:'vat_period',confirm:'yes'});need(closed.ok===true,'ordinary close');
    stable=await snapshot();need(stable.filing.ok===true&&stable.filing.periodStatus==='closed'&&equal(stable.filing.vatReport,stable.report)&&equal(stable.filing.rubrikker,stable.report.rubrikker),'closed filing exact full report');
    need(stable['eu-sales-list'].totalValue===0&&stable['eu-sales-list'].customers.length===0,'empty source EU; nonempty evidence reused');need(stable['oss-report'].submission===false&&stable['oss-report'].ossConsumerSalesBase===0&&stable['oss-report'].entryCount===0&&equal(stable['oss-report'].vatReport,stable.report),'empty OSS full report; no submission');
    service=await admin('service-principals',{displayName:'Synthetic VAT175',operationId:randomUUID(),confirm:true});writeFileSync(key,service.secret+'\n',{mode:0o600,flag:'wx'});
    await admin('members/access',{userId:service.serviceAccountId,action:'set-role',workspaceRole:'member'});await admin('members/company',{userId:service.serviceAccountId,companySlug:'synthetic',action:'grant',role:'bookkeeper'});
    await parity();for(const op of ['report','eu-sales-list','oss-report'])need(equal(mcp(op,true),stable[op]),'current service '+op);need(equal(cli('vat','filing',{from,to},true),stable.filing),'service filing');
    await admin('members/company',{userId:service.serviceAccountId,companySlug:'synthetic',action:'grant',role:'reader'});need(equal(mcp('report',true),stable.report),'Reader report allowed');mcp('eu-sales-list',true,false);
    const memberId=(await http('/api/me',undefined,200,reviewer)).user.id;await admin('members/company',{userId:memberId,companySlug:'synthetic',action:'grant',role:'reader'});await http(api+'report?'+query(),undefined,200,reviewer);await http(api+'eu-sales-list?'+query(),undefined,403,reviewer);
    await http('/api/companies',{name:'Synthetic Nonregistered VAT175',slug:'synthetic-vat175-none',fiscalYearStartMonth:'1',vatPeriodType:null},201);
    for(const op of ['report','eu-sales-list','oss-report','filing']){const v=await http('/api/companies/synthetic-vat175-none/original/vat/'+op+'?'+query(),undefined,422);need(equal(v,{ok:false,errors:['selskabet er ikke momsregistreret']}),'exact nonregistration gate');}
    const nonregistered=mcp('report',false,false,'synthetic-vat175-none');need(nonregistered.code==='NOT_VAT_REGISTERED','recognized nonregistration code');
    for(const op of ['report','eu-sales-list','oss-report','filing','momsangivelse'])cli('vat',op,{},false,2);
    await http(api+'report?'+query()+'&from='+from,undefined,400);await http(api+'report?'+query()+'&actor=forged',undefined,400);
    await admin('members/company',{userId:memberId,companySlug:'synthetic',action:'revoke'});await http(api+'report?actor=forged',undefined,403,reviewer);await admin('members/company',{userId:memberId,companySlug:'synthetic',action:'grant',role:'bookkeeper'});
    await admin('members/company',{userId:service.serviceAccountId,companySlug:'synthetic',action:'revoke'});mcp('report',true,false);await admin('service-principals/revoke',{serviceAccountId:service.serviceAccountId,credentialId:service.credentialId,operationId:randomUUID(),confirm:true});mcp('report',true,false);
    need(equal(await snapshot(),stable),'all reports/clients/refusals preserve books, periods, audit');await originals();await owner.goto(base+'/companies/synthetic/moms');await owner.screenshot({path:process.env.RM_BATCH_SCREENSHOT!,fullPage:true});
    return {expected:historicalVatExpected,nonemptyActual:'attested live Dinero DKK purchase',sourceSha256:createHash('sha256').update(fixture.bytes).digest('hex'),receipt,from,to,review,closed,stable,submission:false,historicalImportParityQualified:true,installedNativeQualified:false};
  }
  async function assertStable(page:Page){owner=page;need(equal(await snapshot(),stable),'exact normal-MFA restart / signed fresh-owner recovery / reopen');await originals();}
  async function reopen(page:Page){await assertStable(page);await cookies();await parity();mcp('report',true,false);}
  return {run,assertStable,reopen};
}
