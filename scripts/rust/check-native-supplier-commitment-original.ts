import type { Page } from '@playwright/test';
import { spawnSync } from 'node:child_process';
import { createHash, randomUUID } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { isDeepStrictEqual as equal } from 'node:util';

// Frozen before the first actual. Native source-bound hashes and nextDate+90
// schedules are deliberately NOT aliases of the legacy pure-TS proposal.
export const originalCommitmentFacts = Object.freeze({ amount:10.01, canonicalAmount:12.34, variance:2.33, documents:1, events:4, matches:1, scannerRequests:1, providerRequests:0, journals:0, payables:0, payments:0, bank:0 });
export function createOriginalCommitmentProof(base:string,work:string,onStep:(name:string)=>void){
  const root='/api/companies/synthetic/',api=root+'original/supplier-commitments',id='synthetic-original-commitment176';
  const cookie=join(work,'commitment176.cookie'),key=join(work,'commitment176.key'),observations:any[]=[];
  const original='Synthetic original supplier commitment176: canonical DKK12.34, planned DKK10.01 monthly; no accounting.\n';
  let owner:Page,service:any,stable:any,baseline:any,date:string,renewal:string,doc:number,ran=false;
  const need=(v:unknown,s:string)=>{if(!v)throw Error('original_commitment: '+s);};
  function record(value:any){observations.push(value);writeFileSync(join(work,'commitment176-observations.json'),JSON.stringify(observations),{mode:0o600});}
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
      const names:Record<string,string>={payloadHash:'plan-hash',idempotencyKey:'idempotency-key',commitmentId:'id',asOf:'as-of'};
      argv=['supplier-commitment',op,...common,'--company','synthetic','--json',...Object.entries(args).filter(([k])=>k!=='confirm'||args.confirm!==undefined).flatMap(([k,v])=>['--'+(names[k]??k),typeof v==='object'?JSON.stringify(v):k==='confirm'?(v?'yes':'no'):String(v)])];
    }else{
      argv=common;const name='supplier_commitment_'+op;
      input=[{jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'original-commitment176',version:'1'}}},{jsonrpc:'2.0',method:'notifications/initialized'},{jsonrpc:'2.0',id:2,method:op==='discovery'?'tools/list':'tools/call',params:op==='discovery'?{}:{name,arguments:{company:'synthetic',...args}}}].map(v=>JSON.stringify(v)).join('\n')+'\n';
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
    return {ok:v.ok,...v.data,...(['list','matches','alerts'].includes(op)||mode!=='ok'?{errors:v.errors}:{}),...(v.appliedRules?{appliedRules:v.appliedRules}:{})};
  }
  async function financial(){
    const value:any={};
    for(const path of ['journal','payables?status=all','bank'])value[path]=await http(root+path);
    return value;
  }
  async function snapshot(){return {list:await http(api),matches:await http(api+'/matches?commitmentId='+id),alerts:await http(api+'/alerts?asOf='+date),native:(await http(root+'supplier-commitments?startDate='+date)).supplierCommitments,financial:await financial()};}
  async function physical(){const r=await owner.request.get(base+root+`documents/${doc}/file`);need(r.ok()&&(await r.body()).toString()===original,'physical original exact bytes');}
  async function parity(){
    for(const kind of ['cli','mcp'] as const){
      for(const [op,args,field] of [['list',{},'list'],['matches',{commitmentId:id},'matches'],['alerts',{asOf:date},'alerts']] as const)
        need(equal(invoke(kind,op,args),stable[field]),kind+' exact source '+op);
      need(invoke(kind,'matches',{commitmentId:'synthetic-absent'}).rows.length===0,'exact match filter');
    }
  }
  const projected=(v:any)=>v.forecast.periods.flatMap((p:any)=>p.sources).filter((s:any)=>s.reference===id).reduce((n:number,s:any)=>n+Math.round(s.amount*100),0);
  async function native(){return (await http(root+'supplier-commitments?startDate='+date)).supplierCommitments;}
  async function run(page:Page,reviewer:Page){
    need(!ran,'finite run, no automatic mutation retry');ran=true;owner=page;await cookies();record({facts:originalCommitmentFacts});
    const who=await http('/api/me'),other=await http('/api/me',undefined,200,reviewer);
    const today=(await http(root+'scheduled-recurring')).scheduledRecurring.asOfDate;
    date=today.slice(0,4)+'-01-31';
    renewal=new Date(Date.parse(date+'T00:00:00Z')+30*86400000).toISOString().slice(0,10);
    const vendor=(await post(root+'vendors',{name:'Synthetic Original Commitment Supplier'})).vendor.id;
    doc=(await post(root+'documents/ingest',{confirm:true,fileName:'synthetic-commitment176-source.txt',fileBase64:Buffer.from(original).toString('base64'),metadata:{source:'Synthetic reviewed original commitment',documentType:'cash_register_receipt',issueDate:date,currency:'DKK',amountIncVat:12.34}})).document.id;
    service=await admin('service-principals',{displayName:'Synthetic commitment176',operationId:randomUUID(),confirm:true});writeFileSync(key,service.secret+'\n',{mode:0o600,flag:'wx'});
    await admin('members/access',{userId:service.serviceAccountId,action:'set-role',workspaceRole:'member'});
    await admin('members/company',{userId:service.serviceAccountId,companySlug:'synthetic',action:'grant',role:'bookkeeper'});
    // Baseline AFTER deliberate vendor/source/service setup, before any plan.
    baseline=await financial();
    need(baseline.journal.journal.entries.length===0&&baseline['payables?status=all'].payables.rows.length===0&&baseline.bank.bank.transactions.length===0,'frozen independent zero financial baseline');
    const commitment={commitmentId:id,vendorPartyId:'vendor:'+vendor,type:'subscription',description:'Synthetic original monthly subscription',businessPurpose:'Synthetic reviewed planning only',amount:10.01,currency:'DKK',frequency:'monthly',nextDate:date,noticeDate:date,renewalDate:renewal,evidenceRefs:['document:'+doc]};
    const tools=invoke('mcp','discovery'),names=tools.map((t:any)=>t.name);
    need(tools.length===348&&new Set(names).size===348,'348 unique tools, no source/native name collision');
    for(const op of ['plan','apply','list','change','match','matches','alerts']){const t=tools.find((t:any)=>t.name==='supplier_commitment_'+op);need(t?.inputSchema.additionalProperties===false&&t.annotations.readOnlyHint===['plan','list','matches','alerts'].includes(op),'strict source tool '+op);}
    need(names.includes('native_supplier_commitment_matches'),'explicit native match alias retained');
    const before=await snapshot();
    await http(api+'?unknown=1',undefined,400);
    await http(api+'/matches?commitmentId='+id+'&commitmentId='+id,undefined,400);
    await http(api+'/alerts',undefined,400);
    const planned=invoke('cli','plan',{input:commitment});
    need(equal(planned,invoke('mcp','plan',{commitment})),'native source-bound plan client parity');
    const p=planned.proposal;
    const feb=new Date(Date.UTC(Number(date.slice(0,4)),2,0)).toISOString().slice(0,10);
    need(p.amount===10.01&&p.vendorSnapshot==='Synthetic Original Commitment Supplier'&&p.previousPayloadHash===null&&p.sourceEvidence.length===1&&p.vendorProvenance.kind==='ledger_vendor','native reviewed source provenance');
    need(equal(p.schedule,[date,feb,date.slice(0,4)+'-03-31',date.slice(0,4)+'-04-30'].map(d=>({date:d,amount:10.01,currency:'DKK'}))),'native inclusive90-day/month-end schedule');
    const binding={...p};delete binding.payloadHash;delete binding.schedule;delete binding.warnings;
    const canonical=(v:any):string=>v===null||typeof v!=='object'?JSON.stringify(v):Array.isArray(v)?'['+v.map(canonical).join(',')+']':'{'+Object.keys(v).sort().map(k=>JSON.stringify(k)+':'+canonical(v[k])).join(',')+'}';
    need(createHash('sha256').update(canonical(binding)).digest('hex')===p.payloadHash,'independent native source binding hash');
    need(equal(await snapshot(),before),'plan readonly exact state');
    invoke('cli','apply',{input:commitment,payloadHash:p.payloadHash},false,'invalid');
    invoke('cli','apply',{input:commitment,payloadHash:'0'.repeat(64),confirm:true},false,'refused');
    invoke('cli','plan',{input:{...commitment,vendorSnapshot:'Forged authority'}},false,'refused');
    invoke('cli','plan',{input:{...commitment,status:'paused'}},false,'refused');
    invoke('mcp','apply',{commitment,payloadHash:p.payloadHash},false,'refused');
    await http(api+'/apply',{commitment,payloadHash:p.payloadHash,confirm:true,actor:'user:forged'},403);
    need(equal(await snapshot(),before),'refusals leave exact source state');
    const approved=invoke('cli','apply',{input:commitment,payloadHash:p.payloadHash,idempotencyKey:'synthetic-commitment176-reviewed',confirm:true});
    need(approved.idempotent===false&&approved.commitmentId===id&&approved.payloadHash===p.payloadHash,'exact explicit approval receipt');
    need(projected(await native())===-4004,'four native forecast occurrences');
    const active=invoke('cli','list');need(active.rows.length===1&&active.rows[0].commitment_id===id&&typeof active.rows[0].payload_json==='string'&&equal(JSON.parse(active.rows[0].payload_json),p),'source raw current row');
    const alerts={ok:true,rows:[{commitmentId:id,kind:'notice',date,severity:'due'},{commitmentId:id,kind:'renewal',date:renewal,severity:'upcoming'}],errors:[]};
    need(equal(invoke('cli','alerts',{asOf:date}),alerts)&&equal(invoke('mcp','alerts',{asOf:date}),alerts),'inclusive deterministic30day alerts');
    need(invoke('cli','alerts',{asOf:'invalid'}).rows.length===0,'malformed asOf source empty');
    invoke('cli','alerts',{},false,'invalid');
    const match={commitmentId:id,occurrenceDate:date,evidence:{kind:'canonical_document',id:String(doc)},confirm:true};
    invoke('mcp','match',{...match,evidence:{...match.evidence,amount:10.01}},true,'invalid');
    const matched=invoke('mcp','match',match,true);
    need(matched.idempotent===false&&equal(matched.variance,{amount:2.33,dateDays:0,late:false,currency:{expected:'DKK',actual:'DKK',status:'same'},party:{expected:'vendor:'+vendor,actual:null,status:'missing'},documentation:'present',comparability:'native_currency'}),'canonical source-only exact variance');
    need(projected(await native())===-3003,'canonical occurrence removed exactly once');
    const matches=invoke('cli','matches',{commitmentId:id});need(matches.rows.length===1&&matches.rows[0].evidence.amount===12.34&&matches.rows[0].payloadHash===matched.payloadHash,'canonical source facts retained');
    invoke('mcp','match',{...match,occurrenceDate:feb},true,'refused');
    invoke('mcp','change',{commitmentId:id,action:'paused',reason:'Synthetic reviewed pause',confirm:true},true);
    need(projected(await native())===0&&invoke('cli','list').rows.length===0&&invoke('mcp','alerts',{asOf:date}).rows.length===0,'pause removes current list/alerts/future forecast');
    invoke('cli','change',{commitmentId:id,action:'ended',reason:'Paused is not active',confirm:true},false,'refused');
    const resumed=invoke('cli','plan',{input:commitment}).proposal;
    need(resumed.previousPayloadHash&&resumed.payloadHash!==p.payloadHash,'fresh reviewed source-bound reactivation');
    invoke('cli','apply',{input:commitment,payloadHash:resumed.payloadHash,idempotencyKey:'synthetic-commitment176-reviewed-resume',confirm:true});
    invoke('cli','change',{commitmentId:id,action:'ended',reason:'Synthetic reviewed end',confirm:true});
    const recovered=invoke('cli','apply',{input:commitment,payloadHash:p.payloadHash,idempotencyKey:'synthetic-commitment176-reviewed',confirm:true});
    need(recovered.idempotent===true&&recovered.payloadHash===p.payloadHash,'explicit original-key receipt recovery after end, no replan/reapplication');
    const n=await native();need(n.history.length===4&&n.matches.length===1&&n.commitments[0].status==='ended'&&projected(n)===0,'terminal history retained');
    stable=await snapshot();need(equal(stable.financial,baseline),'no additional journal/payable/payment/bank effects');await physical();await parity();
    await admin('members/company',{userId:service.serviceAccountId,companySlug:'synthetic',action:'grant',role:'reader'});
    invoke('mcp','change',{commitmentId:id,action:'ended',reason:'Reader forbidden',confirm:true},true,'refused');
    need(equal(invoke('mcp','list',{},true),stable.list),'reader source read');
    await admin('members/company',{userId:other.user.id,companySlug:'synthetic',action:'grant',role:'reader'});
    await http(api+'/change',{commitmentId:id,action:'ended',reason:'Reader forbidden',confirm:true},403,reviewer);
    await admin('members/company',{userId:service.serviceAccountId,companySlug:'synthetic',action:'revoke'});
    invoke('mcp','list',{},true,'refused');
    await admin('service-principals/revoke',{serviceAccountId:service.serviceAccountId,credentialId:service.credentialId,operationId:randomUUID(),confirm:true});
    invoke('mcp','list',{},true,'refused');
    need(equal(await snapshot(),stable),'access refusals immutable');
    return {facts:originalCommitmentFacts,approvalHash:p.payloadHash,matchHash:matched.payloadHash,humanActor:'user:'+who.user.id,humanPrincipal:'user:'+who.user.id,serviceActor:'service_account:'+service.serviceAccountId,servicePrincipal:'service_account:'+service.serviceAccountId,stable,nativeCompatibility:'source-bound native hash; nextDate+90 inclusive, not pureTS parity',baselineAfterSetup:true,noAutomaticWriteRetry:true};
  }
  async function assertStable(page:Page){owner=page;need(equal(await snapshot(),stable),'exact commitment/source/financial state through restart and signed recovery');await physical();}
  async function reopen(page:Page){await assertStable(page);await cookies();await parity();invoke('mcp','list',{},true,'refused');}
  async function readOnly(){await parity();await parity();await assertStable(owner);return {repeatedReadOnly:true};}
  return {run,assertStable,reopen,readOnly};
}
