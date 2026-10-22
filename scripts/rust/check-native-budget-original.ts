import type { Page } from '@playwright/test';
import { spawnSync } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { isDeepStrictEqual as equal } from 'node:util';

/** Synthetic public-API facts, fixed independently of implementation responses.
 * Payable expense is deliberately a separate account: comparison total actual is
 * 60 + 40 = 100, NOT 60. Neither unpaid expense changes booked opening bank.
 * Recurring is ungenerated and never enters weekly receivables.
 */
export const originalBudgetFacts = Object.freeze({ opening:1000, revisions:[80,90], actual:60, payable:40, invoice:125, recurring:50, commitment:30, intercompany:75 });
export const originalBudgetExpected = Object.freeze({ monthly:1000+125+50-90, weeklyBase:1000+125-40-30, weeklyScenario:1000+125-40-30+75, expenseVariance:90-60, totalActual:60+40, totalVariance:60+40-90 });

export function createOriginalBudgetProof(base: string, work: string, onStep: (name:string)=>void) {
  const root='/api/companies/synthetic/', api=root+'original/budget/', other='synthetic-budget-counterparty';
  const cookie=join(work,'budget172.cookie'), key=join(work,'budget172.key');
  const observations:any[]=[];
  let owner:Page, service:any, stable:any, period:string, start:string, year:string, firstId:number, lastId:number, approved:any, ran=false;
  const need=(v:unknown,s:string)=>{if(!v)throw Error('original_budget: '+s);};
  function record(value:any){observations.push(value);writeFileSync(join(work,'budget172-observations.json'),JSON.stringify(observations),{mode:0o600});}
  async function http(path:string,data?:any,status=200,page=owner,method='POST') {
    onStep((data===undefined?'GET:':method+':')+path);
    const r=data===undefined?await page.request.get(base+path):await page.request.fetch(base+path,{method,headers:{origin:base,'content-type':'application/json'},data});
    need(r.status()===status,path+' expected HTTP'+status+' got '+r.status());
    const value=await r.json();record({path,status:r.status(),value:path==='/api/workspace/service-principals'?{...value,secret:'[redacted]'}:value});return value;
  }
  const post=(path:string,data:any,status=200,page=owner)=>http(path,data,status,page);
  const admin=(path:string,data:any)=>post('/api/workspace/'+path,data,path==='service-principals'?201:200);
  async function cookies(){writeFileSync(cookie,(await owner.context().cookies(base)).map(c=>c.name+'='+c.value).join('; '),{mode:0o600});}
  function invoke(kind:'cli'|'mcp',op:string,args:any={},asService=false,mode:'ok'|'refused'|'invalid'='ok') {
    onStep(kind+':'+op+':'+(asService?'service':'human'));
    const common=['--server',base,'--allow-loopback-http',asService?'--service-key-file':'--cookie-file',asService?key:cookie];
    let argv:string[],input:string|undefined;
    if(kind==='cli'){
      const names:Record<string,string>={accountNo:'account',startDate:'start'};
      argv=['budget',op,...common,'--company','synthetic','--json',...Object.entries(args).filter(([k])=>k!=='confirm').flatMap(([k,v])=>['--'+(names[k]??k),String(v)])];
    }else{
      argv=common;const name=op==='forecast-13-week'?'liquidity_forecast_13_week':'budget_'+op.replaceAll('-','_');
      input=[{jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'original-budget172',version:'1'}}},{jsonrpc:'2.0',method:'notifications/initialized'},{jsonrpc:'2.0',id:2,method:'tools/call',params:{name,arguments:{company:'synthetic',...args}}}].map(v=>JSON.stringify(v)).join('\n')+'\n';
    }
    const r=spawnSync(join(work,kind==='cli'?'rentemester-rust':'rentemester-mcp'),argv,{input,encoding:'utf8',timeout:45000,maxBuffer:16*1024*1024});
    for(const file of [cookie,key]){const secret=readFileSync(file,'utf8').trim();need(!r.stdout.includes(secret)&&!r.stderr.includes(secret),'credential privacy');}
    record({kind,op,args,asService,exit:r.status,stdout:r.stdout,stderr:r.stderr,terminalError:r.error?.name});
    need(!r.error&&r.status===(kind==='cli'&&mode!=='ok'?(mode==='invalid'?2:1):0),'bounded client terminal; never retry unknown writes');
    if(kind==='cli'){if(mode==='invalid')return;const v=JSON.parse(r.stdout);need(v.ok===(mode==='ok'),'CLI flat outcome');return v;}
    const response=r.stdout.trim().split('\n').map(s=>JSON.parse(s)).find(v=>v.id===2);
    if(mode==='invalid'){need(response?.error?.code===-32602,'strict RPC selectors');return;}
    const result=response?.result,v=result?.structuredContent;
    need(!response?.error&&result.content.length===1&&equal(JSON.parse(result.content[0].text),v),'MCP text/structured equality');
    need(v?.ok===(mode==='ok')&&result.isError===(mode!=='ok')&&Array.isArray(v.errors),'MCP source envelope');
    if(op==='vs-actual'&&Object.hasOwn(args,'year')&&mode==='ok')return v.data;
    // Domain false reports legitimately retain their report fields in data.
    return {ok:v.ok,...v.data,errors:v.errors,...(v.appliedRules?{appliedRules:v.appliedRules}:{})};
  }
  async function snapshot(){
    const state:any={};
    for(const path of ['journal','vat','trial-balance','documents','bank','invoices','recurring-invoices','payables?status=all',`supplier-commitments?startDate=${start}`])state[path]=await http(root+path);
    state.otherJournal=await http(`/api/companies/${other}/journal`);
    state.disposition=await http('/api/group-dispositions/synthetic-budget172-inflow?asOf='+start);
    state.corporate=await http(root+'corporate-records/synthetic-budget172-agreement/download');
    state.list=await http(api+'list');
    state.compare=await http(api+'vs-actual?'+new URLSearchParams({from:period,to:period}));
    state.monthly=await http(api+'forecast?'+new URLSearchParams({startDate:start,months:'1'}));
    state.weekly=await http(api+'forecast-13-week?'+new URLSearchParams({startDate:start,weeks:'1'}));
    return state;
  }
  async function parity(){for(const kind of ['cli','mcp'] as const){
    for(const [op,args,field]of [['list',{},'list'],['vs-actual',{from:period,to:period},'compare'],['forecast',{startDate:start,months:1},'monthly'],['forecast-13-week',{startDate:start,weeks:1},'weekly']] as const)need(equal(invoke(kind,op,args),stable[field]),kind+' exact '+op+' source parity');
  }}
  async function journal(date:string,text:string,lines:any[],documentId?:number){const j=(await http(root+'journal')).journal;return post(root+'manual-journal',{confirm:true,input:{format:'rentemester-manual-journal-post/1',expectedRuleVersion:j.nativePosting.ruleVersion,idempotencyKey:randomUUID(),request:{...(documentId===undefined?{}:{documentId:String(documentId)}),transactionDate:date,text,currency:'DKK',lines}}});}
  const line=(accountNo:string,debitAmount:string,creditAmount:string)=>({accountNo,debitAmount,creditAmount});
  async function run(page:Page,reviewer:Page){
    need(!ran,'one run, no budget resend');ran=true;owner=page;await cookies();
    // Publish facts before setup: even failed profiles retain the independent oracle.
    record({facts:originalBudgetFacts,expected:originalBudgetExpected});
    const who=await http('/api/me'),checker=await http('/api/me',undefined,200,reviewer);
    const today=(await http(root+'scheduled-recurring')).scheduledRecurring.asOfDate;
    period=today.slice(0,7);year=today.slice(0,4);start=period+'-01';
    const prior=new Date(start+'T00:00:00Z');prior.setUTCDate(0);const openingDate=prior.toISOString().slice(0,10);
    await http(root+'company',{cvr:'87654321',address:'Synthetic Budget Street 1',postalCode:'1000',city:'Synthetic City',vatPeriodType:'quarter',paymentTermsDays:0},200,owner,'PATCH');
    for(const [accountNo,type,normalBalance]of [['1800','asset','debit'],['3720','expense','debit'],['3721','expense','debit'],['6800','liability','credit'],['8872','equity','credit']])await post(root+'accounts',{accountNo,name:'Synthetic budget '+accountNo,type,normalBalance});
    // No name-based bank fallback: source-shaped refusal before explicit mapping.
    const missing=await http(api+'forecast?startDate='+start+'&months=1',undefined,422);need(missing.ok===false,'unmapped opening honestly refused');
    await post(root+'account-roles',{role:'bank',accountNo:'1800',confirm:true});
    await post(root+'account-roles',{role:'creditors',accountNo:'6800',confirm:true});
    await post(root+'bank-accounts',{slug:'synthetic-budget172',name:'Synthetic confirmed budget bank',currency:'DKK',ledgerAccountNo:'1800'});
    await journal(openingDate,'Synthetic budget opening contributed capital',[line('1800','1000.00','0.00'),line('8872','0.00','1000.00')]);
    const vendor=(await post(root+'vendors',{name:'Synthetic Budget Supplier',vatOrCvr:'DK12345678'})).vendor.id;
    async function document(name:string,amount:number,type='cash_register_receipt'){
      return (await post(root+'documents/ingest',{confirm:true,fileName:name+'.txt',fileBase64:Buffer.from(`Synthetic budget172 ${name} DKK ${amount}, VAT zero.\n`).toString('base64'),metadata:{source:'Synthetic source reviewed budget fixture',documentType:type,issueDate:start,invoiceNo:name,currency:'DKK',amountIncVat:amount,vatAmount:0,sender:{name:'Synthetic Budget Supplier',address:'Synthetic Supplier 2',vatOrCvr:'DK12345678'},recipient:{name:'Synthetic Batch Company',address:'Synthetic Budget Street 1',vatOrCvr:'DK87654321'}}})).document.id;
    }
    const actualDoc=await document('synthetic-budget172-actual',60);
    await journal(start,'Synthetic budget known actual owner funded',[line('3720','60.00','0.00'),line('8872','0.00','60.00')],actualDoc);
    const payableDoc=await document('synthetic-budget172-payable',40);
    await post(root+'payables/register',{documentId:payableDoc,billDate:start,dueDate:start,expenseAccountNo:'3721',vatTreatment:'exempt',vendorId:vendor,confirm:true});
    const source=await document('synthetic-budget172-commitment',30);
    const commitment={commitmentId:'synthetic-budget172-commitment',vendorPartyId:'vendor:'+vendor,type:'subscription',description:'Synthetic unmatched planning source',businessPurpose:'Synthetic planning only',amount:30,currency:'DKK',frequency:'monthly',nextDate:start,endDate:start,evidenceRefs:['document:'+source]};
    const plan=(await post(root+'supplier-commitments/plan',{commitment})).plan;
    await post(root+'supplier-commitments',{commitment,payloadHash:plan.payloadHash,confirm:true});
    const customer=(await post(root+'customers',{name:'Synthetic Budget Buyer',address:'Synthetic Buyer 3',vatOrCvr:'DK12345678'})).customer.id;
    await post(root+'invoices/issue',{issueDate:start,dueDate:start,customerId:customer,currency:'DKK',vatRatePercent:25,lines:[{description:'Synthetic open invoice',quantity:1,unitPriceExVat:100}]});
    await post(root+'recurring-invoices',{name:'Synthetic ungenerated budget schedule',interval:'monthly',intervalCount:1,deliveryChannel:'manual',firstIssueDate:start,paymentTermsDays:0,deliveryPeriodMode:'issue_month',vatRatePercent:25,currency:'DKK',customerId:customer,lines:[{description:'Synthetic ungenerated recurring',quantity:1,unitPriceExVat:40}],confirm:true});
    // Reuse normal related-company/corporate evidence and distinct MFA approval.
    await admin('members/access',{userId:checker.user.id,action:'set-role',workspaceRole:'workspace_owner'});
    await post('/api/companies',{name:'Synthetic Budget Counterparty',slug:other,fiscalYearStartMonth:'1',vatPeriodType:null},201);
    for(const slug of ['synthetic',other])for(const userId of [who.user.id,checker.user.id])await admin('members/company',{userId,companySlug:slug,action:'grant',role:'owner'});
    const party=(await post(root+'workspace-parties',{kind:'organization',name:'Synthetic budget related company',source:'Synthetic agreement',observedAt:new Date().toISOString(),reviewAssertion:'Explicit synthetic review',role:'related_company',confirm:true})).party.partyId;
    await post(`/api/companies/${other}/workspace-parties/${party}/role`,{role:'related_company',confirm:true});
    await post(root+'corporate-records',{recordId:'synthetic-budget172-agreement',type:'intercompany_agreement',bytesBase64:Buffer.from('Synthetic DKK75 intercompany inflow scenario, not settled cash.\n').toString('base64'),filename:'synthetic-budget172-agreement.txt',source:'Synthetic agreement',receivedAt:new Date().toISOString(),uploader:'Synthetic owner',sensitivity:'normal',links:[{type:'company',id:'synthetic'},{type:'company',id:other},{type:'party',id:party}],confirm:true});
    const disposition={dispositionId:'synthetic-budget172-inflow',type:'loan',economicDate:start,settlementDueDate:start,amount:75,currency:'DKK',partyIds:[party],evidenceRecordIds:['synthetic-budget172-agreement'],left:{companySlug:'synthetic',role:'lender',expectedSide:'receivable'},right:{companySlug:other,role:'borrower',expectedSide:'payable'}};
    const planned=await post(root+'group-dispositions/plan',{disposition});
    await post(root+'group-dispositions/propose',{disposition:planned.disposition,confirm:true});
    approved=await post(root+'group-dispositions/approve',{dispositionId:disposition.dispositionId,payloadHash:planned.payloadHash,confirm:true},200,reviewer);
    service=await admin('service-principals',{displayName:'Synthetic budget172',operationId:randomUUID(),confirm:true});writeFileSync(key,service.secret+'\n',{mode:0o600,flag:'wx'});
    await admin('members/access',{userId:service.serviceAccountId,action:'set-role',workspaceRole:'member'});
    await admin('members/company',{userId:service.serviceAccountId,companySlug:'synthetic',action:'grant',role:'bookkeeper'});
    const args={accountNo:'3720',period,amount:80,notes:' Synthetic budget revision '};
    for(const confirm of [undefined,false])invoke('mcp','set',{...args,...(confirm===undefined?{}:{confirm})},false,'refused');
    invoke('cli','set',{...args,amount:-1},false,'refused');
    invoke('cli','forecast',{startDate:start,months:0},false,'refused');
    invoke('cli','forecast-13-week',{startDate:start,weeks:14},false,'refused');
    invoke('mcp','vs-actual',{year,from:period,to:period},false,'invalid');
    invoke('mcp','vs-actual',{from:period},false,'invalid');
    const first=invoke('cli','set',args),last=invoke('mcp','set',{...args,amount:90,notes:' ',confirm:true},true);
    firstId=first.budgetLineId;lastId=last.budgetLineId;need(Number.isSafeInteger(firstId)&&lastId>firstId,'two append receipts exactly once');
    stable=await snapshot();
    need(stable.list.count===1&&stable.list.rows.length===1&&stable.list.rows[0].id===lastId&&stable.list.rows[0].amount===90&&stable.list.rows[0].notes===null&&!Object.hasOwn(stable.list,'appliedRules'),'latest revision, blank null, source list');
    const actual=stable.compare.lines.find((l:any)=>l.accountNo==='3720');
    need(actual?.budget===90&&actual.actual===60&&actual.variance===30,'expense natural sign');
    need(stable.compare.lines.length===2&&stable.compare.totalBudget===90&&stable.compare.totalActual===100&&stable.compare.totalVariance===10,'actual/budget union, no grid, independent payable arithmetic');
    const m=stable.monthly,w=stable.weekly,p=w.periods[0];
    need(m.openingBalance===1000&&m.closingBalance===1085&&m.periods.length===1,'monthly independent rollforward');
    need(equal(m.periods[0],{period,openingBalance:1000,invoiceInflow:125,recurringInflow:50,budgetedCostOutflow:90,netChange:85,closingBalance:1085}),'exact monthly source buckets');
    need(w.openingCash===1000&&w.lowestPoint===1000&&w.periods.length===1&&p.weekStart===start&&p.receivables===125&&p.payables===40&&p.commitments===30&&p.undatedBudgetAssumptions===-90&&p.budgets===0&&p.obligations===0&&p.scenarios===0&&p.intercompany===75&&p.closingCash===1055&&p.scenarioClosingCash===1130,'weekly base/scenario independent arithmetic');
    need(p.sources.some((s:any)=>s.source==='approved_intercompany_disposition'&&s.amount===75&&s.reference===`intercompany-disposition:${disposition.dispositionId}:${planned.payloadHash}`)&&p.sources.some((s:any)=>s.source==='approved_commitment'&&s.amount===-30&&s.reference===commitment.commitmentId),'current approved source references');
    need(equal(invoke('cli','list',{period,accountNo:'3720'}),stable.list),'exact list filters');
    need(invoke('cli','list',{accountNo:'missing'}).count===0,'unmatched source filter empty');
    need(invoke('cli','forecast',{startDate:start,months:18}).periods.length===18,'18-month upper bound');
    need(invoke('cli','forecast-13-week',{startDate:start}).periods.length===13,'13-week default');
    need((await http(api+'forecast?'+new URLSearchParams({startDate:period+'-15',months:'1'}))).closingBalance===1085,'midmonth starts containing month');
    const native=await http(root+'budget-vs-actual?year='+encodeURIComponent(year));
    need(equal(invoke('mcp','vs-actual',{year}),native),'native exact year overload preserved');
    await parity();await parity();
    await admin('members/company',{userId:service.serviceAccountId,companySlug:'synthetic',action:'grant',role:'reader'});
    invoke('mcp','set',{...args,confirm:true},true,'refused');
    need(equal(invoke('mcp','list',{},true),stable.list),'reader retains source read');
    await admin('members/company',{userId:checker.user.id,companySlug:'synthetic',action:'grant',role:'reader'});
    await http(api+'set',{...args,confirm:true},403,reviewer);
    await admin('members/company',{userId:service.serviceAccountId,companySlug:'synthetic',action:'revoke'});
    invoke('mcp','forecast-13-week',{startDate:start,weeks:1},true,'refused');
    await admin('service-principals/revoke',{serviceAccountId:service.serviceAccountId,credentialId:service.credentialId,operationId:randomUUID(),confirm:true});
    invoke('mcp','list',{},true,'refused');
    need(equal(await snapshot(),stable),'repeat forecasts and refusals no journal/invoice/schedule/budget effects');
    return {facts:originalBudgetFacts,expected:originalBudgetExpected,firstId,lastId,humanActor:'user:'+who.user.id,serviceActor:'service_account:'+service.serviceAccountId,approved,stable,noWriteReplay:true};
  }
  async function assertStable(page:Page){owner=page;need(equal(await snapshot(),stable),'exact source projections through restart/signed recovery/reopen');}
  async function reopen(page:Page){await assertStable(page);await cookies();await parity();invoke('mcp','list',{},true,'refused');}
  async function readOnly(){await parity();await parity();await assertStable(owner);return {repeatedReadOnly:true};}
  return {run,assertStable,reopen,readOnly};
}
