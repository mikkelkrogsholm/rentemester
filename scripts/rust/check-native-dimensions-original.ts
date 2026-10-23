import type { Page } from '@playwright/test';
import { spawnSync } from 'node:child_process';
import { createHash, randomUUID } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { isDeepStrictEqual as equal } from 'node:util';

// Frozen independent oracle; setup is finite and public API only.
export const originalDimensionsFacts = Object.freeze({ amount:12.50, accountBudget:20, documents:1, definitions:2, members:4, assignments:5, budgetEvents:6, currentBudgets:2, actual:[5,7.50], budgets:[13,7], variance:[8,-0.50], scannerRequests:1, providerRequests:0, journals:1, lines:2 });
export function createOriginalDimensionsProof(base:string,work:string,onStep:(name:string)=>void){
  const root='/api/companies/synthetic/',api=root+'original/dimensions',id='synthetic-original-dimensions177',account='3729';
  const toolsByOperation:Record<string,string>={define:'dimension_definition_create',member:'dimension_member_create','definition-lifecycle':'dimension_definition_lifecycle','member-lifecycle':'dimension_member_lifecycle',definitions:'dimension_definition_list',members:'dimension_member_list',plan:'dimension_assignment_plan',apply:'dimension_assignment_apply',replace:'dimension_assignment_replace',supersede:'dimension_assignment_supersede',list:'dimension_assignment_list','budget-plan':'dimension_budget_plan','budget-apply':'dimension_budget_apply',budgets:'dimension_budget_list'};
  const cookie=join(work,'dimensions177.cookie'),key=join(work,'dimensions177.key'),observations:any[]=[];
  const original='Synthetic dimension budget expense: DKK12.50, no VAT, paid by owner.\n';
  let owner:Page,service:any,stable:any,baseline:any,date:string,period:string,year:string,line:number,doc:number,ran=false;
  const need=(v:unknown,s:string)=>{if(!v)throw Error('original_dimensions: '+s);};
  function record(value:any){observations.push(value);writeFileSync(join(work,'dimensions177-observations.json'),JSON.stringify(observations),{mode:0o600});}
  async function http(path:string,data?:any,status=200,page=owner,method='POST') {
    onStep((data===undefined?'GET:':method+':')+path);
    const r=data===undefined?await page.request.get(base+path):await page.request.fetch(base+path,{method,headers:{origin:base,'content-type':'application/json'},data});
    const value=await r.json();record({path,status:r.status(),value:path==='/api/workspace/service-principals'?{...value,secret:'[redacted]'}:value});
    need(r.status()===status,path+' expected HTTP'+status+' got '+r.status());
    return value;
  }
  const post=(path:string,data:any,status=200,page=owner)=>http(path,data,status,page);
  const admin=(path:string,data:any)=>post('/api/workspace/'+path,data,path==='service-principals'?201:200);
  async function cookies(){writeFileSync(cookie,(await owner.context().cookies(base)).map(c=>c.name+'='+c.value).join('; '),{mode:0o600});}
  function invoke(kind:'cli'|'mcp',op:string,args:any={},asService=false,mode:'ok'|'refused'|'invalid'='ok') {
    onStep(kind+':'+op+':'+(asService?'service':'human'));
    const common=['--server',base,'--allow-loopback-http',asService?'--service-key-file':'--cookie-file',asService?key:cookie];
    let argv:string[],input:string|undefined;
    if(kind==='cli'){
      const names:Record<string,string>={accountNo:'account'};
      argv=['dimensions',op,...common,'--company','synthetic','--json',...Object.entries(args).filter(([k])=>k!=='confirm'||args.confirm!==undefined).flatMap(([k,v])=>['--'+(names[k]??k.replace(/[A-Z]/g,c=>'-'+c.toLowerCase())),typeof v==='object'?JSON.stringify(v):k==='confirm'?(v?'yes':'no'):String(v)])];
    }else{
      argv=common;const name=toolsByOperation[op];
      input=[{jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'original-dimensions177',version:'1'}}},{jsonrpc:'2.0',method:'notifications/initialized'},{jsonrpc:'2.0',id:2,method:op==='discovery'?'tools/list':'tools/call',params:op==='discovery'?{}:{name,arguments:{company:'synthetic',...args}}}].map(v=>JSON.stringify(v)).join('\n')+'\n';
    }
    const r=spawnSync(join(work,kind==='cli'?'rentemester-rust':'rentemester-mcp'),argv,{input,encoding:'utf8',timeout:45000,maxBuffer:16*1024*1024});
    for(const file of [cookie,key]){const secret=readFileSync(file,'utf8').trim();need(!r.stdout.includes(secret)&&!r.stderr.includes(secret),'credential privacy');}
    record({kind,op,args,asService,exit:r.status,stdout:r.stdout,stderr:r.stderr,terminalError:r.error?.name});
    need(!r.error&&r.status===(kind==='cli'&&mode!=='ok'?(mode==='invalid'?2:1):0),'bounded client terminal; never retry unknown writes');
    if(kind==='cli'){if(mode==='invalid')return;const v=JSON.parse(r.stdout);need(v.ok===(mode==='ok'),'CLI flat outcome');return v;}
    const response=r.stdout.trim().split('\n').map(s=>JSON.parse(s)).find(v=>v.id===2);
    if(op==='discovery'){need(!response?.error&&Array.isArray(response?.result?.tools),'tool discovery');return response.result.tools;}
    if(mode==='invalid'){need(response?.error?.code===-32602,'strict RPC selectors');return;}
    const result=response?.result,v=result?.structuredContent;
    need(!response?.error&&result.content.length===1&&equal(JSON.parse(result.content[0].text),v),'MCP text/structured equality');
    need(v?.ok===(mode==='ok')&&result.isError===(mode!=='ok')&&Array.isArray(v.errors),'MCP source envelope');
    if(mode!=='ok')need(!Object.hasOwn(v,'data'),'source refusal omits success data');
    return {ok:v.ok,...v.data,...(mode!=='ok'||op==='budget-apply'?{errors:v.errors}:{})};
  }
  const canonical=(v:any):string=>v===null||typeof v!=='object'?JSON.stringify(v):Array.isArray(v)?'['+v.map(canonical).join(',')+']':'{'+Object.keys(v).sort().map(k=>JSON.stringify(k)+':'+canonical(v[k])).join(',')+'}';
  function checkedPlan(v:any){const {planHash,...facts}=v.plan;need(createHash('sha256').update(canonical(facts)).digest('hex')===planHash,'independent exact source canonical hash');return v.plan;}
  async function financial(){const v:any={};for(const path of ['journal','trial-balance','vat','payables?status=all','bank'])v[path]=await http(root+path);return v;}
  async function snapshot(){return {definitions:await http(api+'/definitions'),members:await http(api+'/members?dimensionId='+id),assignments:await http(api+'/assignments?journalLineId='+line),budgets:await http(api+'/budgets'),alpha:(await http(root+`budget-dimension-actuals?year=${year}&dimension=${id}&member=alpha`)).budgetDimensionActuals,beta:(await http(root+`budget-dimension-actuals?year=${year}&dimension=${id}&member=beta`)).budgetDimensionActuals,financial:await financial()};}
  async function physical(){const r=await owner.request.get(base+root+`documents/${doc}/file`);need(r.ok()&&(await r.body()).toString()===original,'exact retained original');}
  async function parity(){for(const kind of ['cli','mcp'] as const)for(const [op,args,field] of [['definitions',{},'definitions'],['members',{dimensionId:id},'members'],['list',{journalLineId:line},'assignments'],['budgets',{},'budgets']] as const)need(equal(invoke(kind,op,args),stable[field]),kind+' exact '+op+' parity');}
  async function run(page:Page,reviewer:Page){
    need(!ran,'finite run; no automatic mutation retry');ran=true;owner=page;await cookies();record({facts:originalDimensionsFacts});
    const who=await http('/api/me'),other=await http('/api/me',undefined,200,reviewer);
    date=(await http(root+'scheduled-recurring')).scheduledRecurring.asOfDate;period=date.slice(0,7);year=date.slice(0,4);
    for(const [accountNo,type,normalBalance] of [[account,'expense','debit'],['8729','equity','credit']])await post(root+'accounts',{accountNo,name:'Synthetic dimension budget '+accountNo,type,normalBalance});
    // Exact no-VAT source metadata and manual-journal request from the existing
    // budget-dimensions proof; rule version comes from the actual native reader.
    doc=(await post(root+'documents/ingest',{confirm:true,fileName:'synthetic-dimensions177-source.txt',fileBase64:Buffer.from(original).toString('base64'),metadata:{source:'Synthetic dimension budget fixture',documentType:'cash_register_receipt',issueDate:date,amountIncVat:12.50,vatAmount:0,currency:'DKK'}})).document.id;
    const j=(await http(root+'journal')).journal;
    await post(root+'manual-journal',{confirm:true,input:{format:'rentemester-manual-journal-post/1',expectedRuleVersion:j.nativePosting.ruleVersion,idempotencyKey:'synthetic-dimensions177-source',request:{transactionDate:date,text:'Synthetic dimension budget original177',currency:'DKK',documentId:String(doc),lines:[{accountNo:account,debitAmount:'12.50',creditAmount:'0.00'},{accountNo:'8729',debitAmount:'0.00',creditAmount:'12.50'}]}}});
    const journal=(await http(root+'journal?year='+year)).journal;
    need(journal.entries.length===1&&journal.entries[0].lines.length===2,'one expense journal, two lines');
    line=journal.entries[0].lines.find((l:any)=>l.accountNo===account).journalLineId;
    await post(root+'budget',{accountNo:account,period,amount:20});
    service=await admin('service-principals',{displayName:'Synthetic dimensions177',operationId:randomUUID(),confirm:true});writeFileSync(key,service.secret+'\n',{mode:0o600,flag:'wx'});
    await admin('members/access',{userId:service.serviceAccountId,action:'set-role',workspaceRole:'member'});
    await admin('members/company',{userId:service.serviceAccountId,companySlug:'synthetic',action:'grant',role:'bookkeeper'});
    baseline=await financial();record({baselineAfterSetup:baseline});
    const tools=invoke('mcp','discovery'),names=tools.map((t:any)=>t.name);need(tools.length===362&&new Set(names).size===362,'362 unique MCP tools');
    for(const [op,name] of Object.entries(toolsByOperation)){const t=tools.find((t:any)=>t.name===name),read=['definitions','members','plan','list','budget-plan','budgets'].includes(op);need(t?.inputSchema.additionalProperties===false&&t.annotations.readOnlyHint===read&&t.annotations.idempotentHint===(read||op==='budget-apply')&&t.annotations.destructiveHint===false&&t.annotations.openWorldHint===false,'discovery contract '+op);}
    // Narrow transport gates exercise every mutation on both interfaces without
    // growing the frozen fixture. Confirmation failure must precede HTTP writes.
    const gates:Record<string,any>={define:{dimensionId:id,kind:'project',name:'Synthetic'},member:{dimensionId:id,memberId:'alpha',name:'Synthetic'},'definition-lifecycle':{dimensionId:id,action:'rename',name:'Synthetic'},'member-lifecycle':{dimensionId:id,memberId:'alpha',action:'activate'},apply:{journalLineId:line,allocations:[{dimensionId:id,memberId:'alpha',amountMinor:1250,currency:'DKK'}],planHash:'0'.repeat(64)},replace:{journalLineId:line,allocations:[{dimensionId:id,memberId:'alpha',amountMinor:1250,currency:'DKK'}],planHash:'0'.repeat(64),expectedAssignmentId:1,reason:'Synthetic gate'},supersede:{assignmentId:1,reason:'Synthetic gate'},'budget-apply':{accountNo:account,period,allocations:[{dimensionId:id,memberId:'alpha',amount:20}],sourceRef:'Synthetic gate',planHash:'0'.repeat(64)}};
    for(const [op,args] of Object.entries(gates)){invoke('cli',op,args,false,'invalid');invoke('mcp',op,args,false,'refused');}
    await http(api+'/definitions?unknown=1',undefined,400);
    await http(api+'/assignments?journalLineId='+line+'&journalLineId='+line,undefined,400);
    await http(api+'/plan',{journalLineId:line,allocations:[],unknown:true},400);
    invoke('cli','define',{dimensionId:id,kind:'project',name:'Synthetic original projects',confirm:true});
    invoke('mcp','definition-lifecycle',{dimensionId:id,action:'rename',name:'Synthetic reviewed original projects',confirm:true});
    for(const memberId of ['alpha','beta'])invoke('cli','member',{dimensionId:id,memberId,name:'Synthetic '+memberId,confirm:true});
    const allocation=(a:number,b:number)=>[{dimensionId:id,memberId:'alpha',amountMinor:a,currency:'DKK'},{dimensionId:id,memberId:'beta',amountMinor:b,currency:'DKK'}];
    const initial={journalLineId:line,allocations:allocation(750,500),source:'reviewed',sourceRef:'synthetic177:initial-review'};
    need(equal(invoke('cli','plan',initial,true),invoke('mcp','plan',initial,true)),'service plan transport parity');
    const first=checkedPlan(invoke('cli','plan',initial,true));need(first.lineAmountMinor===1250&&first.journalLineId===line,'immutable amount/line binding');
    invoke('cli','apply',{...initial,planHash:first.planHash},true,'invalid');
    await http(api+'/apply',{...initial,planHash:first.planHash,confirm:true,principal:'user:forged'},403);
    const assigned=invoke('cli','apply',{...initial,planHash:first.planHash,confirm:true},true);
    invoke('mcp','member-lifecycle',{dimensionId:id,memberId:'alpha',action:'deactivate',confirm:true});
    invoke('mcp','plan',initial,false,'refused');
    need(invoke('cli','list',{journalLineId:line}).assignments.length===1,'inactive member history remains readable');
    invoke('cli','member-lifecycle',{dimensionId:id,memberId:'alpha',action:'activate',confirm:true});
    const corrected={journalLineId:line,allocations:allocation(500,750),source:'reviewed',sourceRef:'synthetic177:replacement-review'};
    const replacement=checkedPlan(invoke('mcp','plan',corrected));
    const changed=invoke('mcp','replace',{...corrected,expectedAssignmentId:assigned.id,reason:'Synthetic original reviewed correction',planHash:replacement.planHash,confirm:true});
    invoke('cli','replace',{...initial,expectedAssignmentId:assigned.id,reason:'Stale expected assignment',planHash:first.planHash,confirm:true},false,'refused');
    invoke('cli','supersede',{assignmentId:changed.id,reason:'Synthetic explicit fresh review',confirm:true});
    const fresh={...corrected,sourceRef:'synthetic177:fresh-reviewed-source'};const final=checkedPlan(invoke('mcp','plan',fresh));
    invoke('mcp','apply',{...fresh,planHash:final.planHash,confirm:true});
    const budget=(a:number,b:number,sourceRef:string)=>({accountNo:account,period,allocations:[{dimensionId:id,memberId:'alpha',amount:a},{dimensionId:id,memberId:'beta',amount:b}],sourceRef});
    const b1=budget(12,8,'synthetic177:budget-initial'),b2=budget(13,7,'synthetic177:budget-replacement');
    need(equal(invoke('cli','budget-plan',b1),invoke('mcp','budget-plan',b1)),'budget plan transport parity');
    const bp1=checkedPlan(invoke('cli','budget-plan',b1));
    need(equal(bp1,{schemaVersion:'rentemester-dimension-budget-v1',...b1,accountBudget:20,planHash:bp1.planHash}),'independent budget plan facts');
    invoke('mcp','budget-plan',budget(12,7,'invalid-sum'),false,'refused');
    invoke('cli','budget-apply',{...b1,planHash:'0'.repeat(64),confirm:true},false,'refused');
    const firstKey='synthetic177:%_review',secondKey='synthetic177:XXreview';
    const receipt=invoke('cli','budget-apply',{...b1,planHash:bp1.planHash,idempotencyKey:firstKey,confirm:true});
    const bp2=checkedPlan(invoke('mcp','budget-plan',b2));
    invoke('mcp','budget-apply',{...b2,planHash:bp2.planHash,idempotencyKey:secondKey,confirm:true});
    const recovery=invoke('cli','budget-apply',{...b1,planHash:bp1.planHash,idempotencyKey:firstKey,confirm:true});
    need(recovery.idempotent===true&&equal(recovery.ids,receipt.ids)&&receipt.ids.length===2,'explicit old key returns ONLY original two IDs without wildcard broadening');
    stable=await snapshot();need(equal(stable.financial,baseline),'no accounting effects beyond setup');
    const actors=['service_account:'+service.serviceAccountId,...Array(4).fill('user:'+who.user.id)];
    need(stable.definitions.definitions.length===2&&stable.members.members.length===4&&stable.assignments.assignments.length===5,'frozen master/assignment event counts');
    need(equal(stable.assignments.assignments.map((r:any)=>r.actor),actors)&&equal(stable.assignments.assignments.map((r:any)=>r.principal),actors),'exact five typed assignment identities');
    need(equal(stable.assignments.assignments.map((r:any)=>r.event_type),['assigned','superseded','assigned','superseded','assigned']),'append-only sequence');
    need(stable.budgets.budgets.length===2&&equal(stable.budgets.budgets.map((r:any)=>r.amount),[13,7]),'current replacement budgets only');
    for(const [member,actual,budgetAmount,variance] of [['alpha',5,13,8],['beta',7.5,7,-.5]] as const){const row=stable[member].comparison.find((r:any)=>r.accountNo===account&&r.period===period);need(row&&row.actual===actual&&row.accountActual===12.5&&row.unallocatedActual===0&&row.accountBudget===20&&row.budget===budgetAmount&&row.variance===variance,'authoritative actual/budget/variance '+member);}
    await physical();await parity();
    await admin('members/company',{userId:other.user.id,companySlug:'synthetic',action:'grant',role:'reader'});
    await http(api+'/budget-apply',{...b2,planHash:bp2.planHash,confirm:true},403,reviewer);
    await admin('members/company',{userId:service.serviceAccountId,companySlug:'synthetic',action:'grant',role:'reader'});
    invoke('mcp','apply',{...fresh,planHash:final.planHash,confirm:true},true,'refused');
    invoke('mcp','budget-apply',{...b2,planHash:bp2.planHash,confirm:true},true,'refused');
    need(equal(invoke('cli','budgets',{},true),stable.budgets),'current reader allowed read without Origin');
    await admin('members/company',{userId:service.serviceAccountId,companySlug:'synthetic',action:'revoke'});
    invoke('mcp','budgets',{},true,'refused');
    await admin('service-principals/revoke',{serviceAccountId:service.serviceAccountId,credentialId:service.credentialId,operationId:randomUUID(),confirm:true});
    need(equal(await snapshot(),stable),'access refusals leave immutable state');
    return {facts:originalDimensionsFacts,humanActor:'user:'+who.user.id,serviceActor:'service_account:'+service.serviceAccountId,baselineAfterSetup:true,noAutomaticWriteRetry:true,stable};
  }
  async function assertStable(page:Page){owner=page;need(equal(await snapshot(),stable),'exact nonempty history/actuals/accounting through restart and signed fresh-owner recovery');await physical();}
  async function reopen(page:Page){await assertStable(page);await cookies();await parity();invoke('mcp','budgets',{},true,'refused');}
  async function readOnly(){await parity();await parity();await assertStable(owner);return {repeatedReadOnly:true};}
  return {run,assertStable,reopen,readOnly};
}
