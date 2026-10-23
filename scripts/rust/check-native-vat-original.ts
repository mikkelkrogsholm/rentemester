import type { Page } from '@playwright/test';
import { spawnSync } from 'node:child_process';
import { createHash, randomUUID } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { isDeepStrictEqual as equal } from 'node:util';

/** Independent oracle, published BEFORE any setup. No provider fixture duplicated.
 * Nonempty EU/dated EUR FX/partial+final credits: unchanged run-eu-sales.py
 * (740 -> 592 -> 0 DKK, net FX gain 3). Nonempty OSS and dated reversal:
 * unchanged run-oss.py. Those accepted receipts are reused, NOT requalified here.
 * This actual is nonempty ordinary DK purchase only; EU and OSS projections empty.
 * Historical-import parity, actual authority submission and installed desktop remain unqualified.
 */
export const originalVatExpected = Object.freeze({ gross:125, purchaseBase25:100, inputVat:25, outputVat:0, netVatPayable:-25, journalEntryCount:1, linesConsidered:3, scannerRequests:1, providerRequests:0, resolvedExceptions:1, recordedReference:'SYNTHETIC-J04-EXTERNAL-RECEIPT-NOT-SUBMITTED', euTotalValue:0, ossConsumerSalesBase:0 });
export function createOriginalVatProof(base:string,work:string,onStep:(s:string)=>void) {
  const root='/api/companies/synthetic/', api=root+'original/vat/', cookie=join(work,'vat173.cookie'), key=join(work,'vat173.key');
  const observations:any[]=[];let owner:Page, reviewer:Page, service:any, stable:any, from:string,to:string,document:any,ran=false;
  const need=(v:unknown,s:string)=>{if(!v)throw Error('original_vat: '+s);};
  function record(v:any){observations.push(v);writeFileSync(join(work,'vat173-observations.json'),JSON.stringify(observations),{mode:0o600});}
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
    const input=[{jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'original-vat173',version:'1'}}},{jsonrpc:'2.0',method:'notifications/initialized'},{jsonrpc:'2.0',id:2,method:'tools/call',params:{name:'vat_'+op.replaceAll('-','_'),arguments:{company,from,to}}}].map(v=>JSON.stringify(v)).join('\n')+'\n';
    const r=spawnSync(join(work,'rentemester-mcp'),['--server',base,'--allow-loopback-http',asService?'--service-key-file':'--cookie-file',file],{input,encoding:'utf8',timeout:45000,maxBuffer:16*1024*1024});
    need(!r.stdout.includes(secret)&&!r.stderr.includes(secret),'MCP credential privacy');record({kind:'mcp',op,asService,company,exit:r.status,stdout:r.stdout,stderr:r.stderr,terminalError:r.error?.name});need(!r.error&&r.status===0,'bounded MCP terminal');
    const reply=r.stdout.trim().split('\n').map(s=>JSON.parse(s)).find(v=>v.id===2),result=reply?.result,v=result?.structuredContent;
    need(!reply?.error&&result.content.length===1&&equal(JSON.parse(result.content[0].text),v),'MCP text/structured equality');need(v.ok===ok&&result.isError===!ok&&Array.isArray(v.errors),'source MCP outcome');
    if(!ok){need(!Object.hasOwn(v,'data')&&v.errors.length>0,'ALL source false omit data');return v;}
    need(!Object.hasOwn(v.data,'ok'),'source not native envelope');return {ok:true,...v.data,errors:v.errors,...(v.appliedRules?{appliedRules:v.appliedRules}:{})};
  }
  async function snapshot(){const v:any={};for(const p of ['journal','trial-balance','bank','documents','periods'])v[p]=await http(root+p+(['journal','trial-balance','bank'].includes(p)?'?year='+from.slice(0,4):''));for(const op of ['report','eu-sales-list','oss-report','filing'])v[op]=await http(api+op+'?'+query());return v;}
  async function originals(){const r=await owner.request.get(base+root+`documents/${document.id}/file`);need(r.status()===200&&createHash('sha256').update(await r.body()).digest('hex')===document.sha256,'immutable scanned original');}
  async function parity(){for(const op of ['report','eu-sales-list','oss-report','filing','momsangivelse'])need(equal(cli('vat',op,{from,to}),stable[op==='momsangivelse'?'filing':op]),'flat CLI exact '+op);for(const op of ['report','eu-sales-list','oss-report'])need(equal(mcp(op),stable[op]),'source MCP exact '+op);}
  const vatUrl=(year:string)=>base+'/companies/synthetic/moms?year='+year;
  async function panel(state:string,year=from.slice(0,4)) {
    await owner.goto(vatUrl(year));
    const region=owner.getByRole('region',{name:'Momsopgave',exact:true});
    await owner.locator(`[role="region"][aria-label="Momsopgave"][data-state="${state}"], section[aria-label="Momsopgave"][data-state="${state}"]`).waitFor();
    need((await region.innerText()).includes('Rentemester sender ikke til SKAT'),'explicit no automatic submission');
    return region;
  }
  async function recordedPanel(){const region=await panel('recorded');need((await region.innerText()).includes(originalVatExpected.recordedReference),'exact recorded synthetic external receipt visible');}
  async function run(page:Page,member:Page){
    need(!ran,'one finite fixture, no write replay');ran=true;owner=page;reviewer=member;
    record({expected:originalVatExpected,reusedEvidence:['scripts/rust/run-eu-sales.py: nonempty EUR740, dated credits592/0, FX gain3','scripts/rust/run-oss.py: nonempty manual OSS and dated reversal'],nonemptyActual:'one DK standard purchase',newProviderFixtures:0});await cookies();
    const today=(await http(root+'scheduled-recurring')).scheduledRecurring.asOfDate,year=String(Number(today.slice(0,4))-2);from=year+'-01-01';to=year+'-03-31';
    await http(root+'company',{cvr:'87654321',address:'Synthetic VAT Street 1',postalCode:'1000',city:'Synthetic City',vatPeriodType:'quarter',paymentTermsDays:14},200,owner,'PATCH');
    for(const [accountNo,type,normalBalance]of [['1800','asset','debit'],['3000','expense','debit'],['4700','vat','debit'],['1730','asset','debit'],['6730','liability','credit']])await http(root+'accounts',{accountNo,name:'Synthetic VAT '+accountNo,type,normalBalance});
    for(const [role,accountNo]of [['bank','1800'],['input_vat','4700'],['debtors','1730'],['creditors','6730']])await http(root+'account-roles',{role,accountNo,confirm:true});
    await http(root+'bank-accounts',{slug:'synthetic-vat173',name:'Synthetic VAT bank',currency:'DKK',ledgerAccountNo:'1800'});
    const bytes=Buffer.from('Synthetic VAT173 original DK supplier, net100 VAT25 gross125 DKK\n');
    const ingested=await http(root+'documents/ingest',{confirm:true,fileName:'synthetic-vat173-purchase.txt',fileBase64:bytes.toString('base64'),metadata:{source:'Synthetic VAT ordinary intake',documentType:'purchase_sale',issueDate:to,invoiceNo:'SYN-VAT173',deliveryDescription:'Synthetic supplies',amountIncVat:125,vatAmount:25,currency:'DKK',sender:{name:'Synthetic supplier',address:'Synthetic supplier 1',vatOrCvr:'DK12345678'},recipient:{name:'Synthetic Batch Company',address:'Synthetic VAT Street 1',vatOrCvr:'DK87654321'}}});
    document={id:ingested.document.id,sha256:createHash('sha256').update(bytes).digest('hex')};
    const csv=`dato;tekst;beløb;valuta;saldo\n${to};SYN-VAT173;-125,00;DKK;-125,00\n`;
    await http(root+'bank/import',{confirm:true,csvContent:csv,account:'synthetic-vat173'});
    const bank=(await http(root+'bank?year='+year)).bank;need(bank.transactions.length===1,'one independent statement row');
    // The UI selects its canonical obligation; a year URL is not a period selector.
    const native=(await http(root+'vat?year='+year)).vat;
    need(native.vatRegistered&&native.selectedYear===year&&!native.archived&&native.periodStatus==='open','canonical live selected year');
    from=native.periodStart;to=native.periodEnd;
    need(from<=year+'-03-31'&&to>=year+'-03-31','canonical UI period contains actual imported source');
    const blocked=(await http(root+'periods/close-readiness?'+query())).packet;
    need(blocked.blockers>0&&blocked.items.some((i:any)=>i.code==='BANK_UNRECONCILED'&&i.status==='blocked'&&i.count===1),'actual unresolved bank source blocker');
    const region=await panel('blocked',year);
    const source=region.locator('a').filter({hasText:/bank|bilag|dokument/i}).first();
    const href=await source.getAttribute('href');
    need(href&&/^\/companies\/synthetic\//.test(href)&&/bank|bilag|dokument/.test(href),'existing same-company source link');
    await source.click();await owner.waitForURL(url=>url.pathname!== '/companies/synthetic/moms');
    await owner.getByText(/SYN-VAT173|synthetic-vat173-purchase/).first().waitFor();
    record({sourceNavigation:{href,observed:await owner.getByText(/SYN-VAT173|synthetic-vat173-purchase/).first().innerText()},blocked});
    await http(root+'expense/original/book',{documentId:document.id,bankTransactionId:bank.transactions[0].id,expenseAccount:'3000',vatTreatment:'standard',confirm:true});
    const draft=(await http(root+'vat?year='+year)).vat;
    need(draft.periodStart===from&&draft.periodEnd===to&&draft.inputVat===25&&draft.payable===-25&&!draft.momsangivelseReady,'same canonical draft exact totals');
    await panel('draft');
    const report=await http(api+'report?'+query());
    for(const k of ['purchaseBase25','inputVat','outputVat','netVatPayable','journalEntryCount','linesConsidered'] as const)need(report[k]===originalVatExpected[k],'independent report '+k);
    need(report.totalJournalEntryCount===1&&report.totalLinesConsidered===3&&report.reversedJournalEntryCount===0&&report.reversalJournalEntryCount===0&&report.reversedLinesConsidered===0&&report.reversalLinesConsidered===0,'disjoint source counts');
    need(report.rubrikker.kobsmoms===25&&report.rubrikker.salgsmoms===0&&report.rubrikker.momstilsvar===-25,'independent source rubrics');
    const open=await http(api+'filing?'+query(),undefined,422);need(open.ok===false&&open.periodStatus==='open'&&open.errors.length>0,'open filing refused');
    cli('vat','filing',{from,to},false,1);cli('vat','momsangivelse',{from,to},false,1);
    const packet=cli('period','readiness',{from,to}).packet;need(packet.blockers===0,'all ordinary source/accounting controls ready, no bypass');
    // Exercise the shared UI's existing review/hash/lock gate, not a CLI substitute.
    await owner.getByRole('button',{name:'Luk momsperiode',exact:true}).click();
    const dialog=owner.getByRole('dialog',{name:'Luk momsperiode',exact:true});
    const reviewResponse=owner.waitForResponse(r=>r.url()===base+root+'periods/close-review'&&r.request().method()==='POST');
    const closeResponse=owner.waitForResponse(r=>r.url()===base+root+'periods/close'&&r.request().method()==='POST');
    await dialog.getByRole('button',{name:'Luk perioden',exact:true}).click();
    const rr=await reviewResponse,cr=await closeResponse;
    need(rr.ok()&&cr.ok(),'ordinary shared UI review and close');
    const review=(await rr.json()).review,closed=await cr.json();
    need(equal(review.packet,packet),'UI review exact current canonical packet');
    const closeInput=cr.request().postDataJSON();
    need(closeInput.reviewId===review.id&&closeInput.packetHash===packet.hash&&!closeInput.force&&closeInput.confirm===true,'UI authorized review/hash lock without waiver');
    await panel('reviewed');
    stable=await snapshot();need(stable.filing.ok===true&&stable.filing.periodStatus==='closed'&&equal(stable.filing.vatReport,stable.report)&&equal(stable.filing.rubrikker,stable.report.rubrikker),'closed filing exact full report');
    need(stable['eu-sales-list'].totalValue===0&&stable['eu-sales-list'].customers.length===0,'empty source EU; nonempty evidence reused');need(stable['oss-report'].submission===false&&stable['oss-report'].ossConsumerSalesBase===0&&stable['oss-report'].entryCount===0&&equal(stable['oss-report'].vatReport,stable.report),'empty OSS full report; no submission');
    await parity(); // Filing-ready working report is NOT a recorded filing.
    await owner.goto(base+'/companies/synthetic/periodelas');
    const row=owner.getByRole('row').filter({hasText:from}).filter({hasText:to});
    need(await row.count()===1,'exact canonical period row in existing PeriodsView');
    await row.getByRole('button',{name:'Registrér indberetning',exact:true}).click();
    const receiptDialog=owner.getByRole('dialog',{name:'Registrér indberetning',exact:true});
    need((await receiptDialog.innerText()).includes('allerede gennemførte'),'record prior external filing, not submit');
    await receiptDialog.getByLabel('Kvitteringsreference').fill(originalVatExpected.recordedReference);
    const receiptResponse=owner.waitForResponse(r=>r.url()===base+root+'periods/report'&&r.request().method()==='POST');
    await receiptDialog.getByRole('button',{name:'Registrér indberetning',exact:true}).click();
    const receipt=await receiptResponse;need(receipt.ok(),'explicit synthetic external receipt recorded once');
    const recorded=await snapshot();
    need(recorded.filing.periodStatus==='reported'&&recorded.filing.periodReference===originalVatExpected.recordedReference&&equal(recorded.report,stable.report),'recording preserves exact source totals and canonical receipt');
    const period=recorded.periods.periods.periods.find((p:any)=>p.periodStart===from&&p.periodEnd===to&&p.kind==='vat_period');
    need(period?.effectiveStatus==='reported'&&period.reference===originalVatExpected.recordedReference&&period.closePacketHash===packet.hash,'canonical exact period row receipt/hash');
    stable=recorded;await recordedPanel();
    const denied=await http(root+'journal/original/post',{confirm:true,input:{transactionDate:to,text:'Synthetic closed-period refusal',documentId:document.id,currency:'DKK',lines:[{accountNo:'3000',debitAmount:1,creditAmount:0},{accountNo:'1800',debitAmount:0,creditAmount:1}]}},422);
    need(denied.ok===false&&JSON.stringify(denied).match(/period|PERIOD|lukket|closed/i),'actual post-closed financial write refused');
    service=await admin('service-principals',{displayName:'Synthetic VAT173',operationId:randomUUID(),confirm:true});writeFileSync(key,service.secret+'\n',{mode:0o600,flag:'wx'});
    await admin('members/access',{userId:service.serviceAccountId,action:'set-role',workspaceRole:'member'});await admin('members/company',{userId:service.serviceAccountId,companySlug:'synthetic',action:'grant',role:'bookkeeper'});
    await parity();for(const op of ['report','eu-sales-list','oss-report'])need(equal(mcp(op,true),stable[op]),'current service '+op);need(equal(cli('vat','filing',{from,to},true),stable.filing),'service filing');
    await admin('members/company',{userId:service.serviceAccountId,companySlug:'synthetic',action:'grant',role:'reader'});need(equal(mcp('report',true),stable.report),'Reader report allowed');mcp('eu-sales-list',true,false);
    const memberId=(await http('/api/me',undefined,200,reviewer)).user.id;await admin('members/company',{userId:memberId,companySlug:'synthetic',action:'grant',role:'reader'});await http(api+'report?'+query(),undefined,200,reviewer);await http(api+'eu-sales-list?'+query(),undefined,403,reviewer);
    await http('/api/companies',{name:'Synthetic Nonregistered VAT173',slug:'synthetic-vat173-none',fiscalYearStartMonth:'1',vatPeriodType:null},201);
    for(const op of ['report','eu-sales-list','oss-report','filing']){const v=await http('/api/companies/synthetic-vat173-none/original/vat/'+op+'?'+query(),undefined,422);need(equal(v,{ok:false,errors:['selskabet er ikke momsregistreret']}),'exact nonregistration gate');}
    const nonregistered=mcp('report',false,false,'synthetic-vat173-none');need(nonregistered.code==='NOT_VAT_REGISTERED','recognized nonregistration code');
    for(const op of ['report','eu-sales-list','oss-report','filing','momsangivelse'])cli('vat',op,{},false,2);
    await http(api+'report?'+query()+'&from='+from,undefined,400);await http(api+'report?'+query()+'&actor=forged',undefined,400);
    await admin('members/company',{userId:memberId,companySlug:'synthetic',action:'revoke'});await http(api+'report?actor=forged',undefined,403,reviewer);await admin('members/company',{userId:memberId,companySlug:'synthetic',action:'grant',role:'bookkeeper'});
    await admin('members/company',{userId:service.serviceAccountId,companySlug:'synthetic',action:'revoke'});mcp('report',true,false);await admin('service-principals/revoke',{serviceAccountId:service.serviceAccountId,credentialId:service.credentialId,operationId:randomUUID(),confirm:true});mcp('report',true,false);
    need(equal(await snapshot(),stable),'all reports/clients/refusals preserve books, periods, audit');await originals();await recordedPanel();await owner.screenshot({path:process.env.RM_BATCH_SCREENSHOT!,fullPage:true});
    return {expected:originalVatExpected,nonemptyActual:'DK purchase',reusedNonemptyEvidence:['run-eu-sales.py','run-oss.py'],document,csvSha256:createHash('sha256').update(csv).digest('hex'),from,to,review,closed,stable,recordedReference:originalVatExpected.recordedReference,sharedUiJourneyQualified:true,syntheticExternalReceiptOnly:true,submission:false,historicalImportParityQualified:false,installedNativeQualified:false};
  }
  async function assertStable(page:Page){owner=page;need(equal(await snapshot(),stable),'exact normal-MFA restart / signed fresh-owner recovery / reopen');await originals();await recordedPanel();}
  async function reopen(page:Page){await assertStable(page);await cookies();await parity();mcp('report',true,false);}
  return {run,assertStable,reopen};
}
