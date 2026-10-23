import type { Page } from '@playwright/test';
import { spawnSync } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { isDeepStrictEqual as equal } from 'node:util';

// No posting fixture: allowDirectPosting is stored, not a universal journal policy.
// The native bad-debt unit couples the source writer to its existing targeted guard.
export const originalAccountsFacts = Object.freeze({accounts:9, requiredRoles:8, mappings:9, currentMappings:8, audits:18, serviceAudits:3, humanAudits:15, restoredAudits:19, documents:0, journals:0, bankTransactions:0, scannerRequests:0, providerRequests:0});
const roles=['bank','debtors','creditors','output_vat','input_vat','reverse_charge_vat','vat_settlement','operational_default'];
const targets=['1790','1791','7790','2790','4790','2790','4590','3790'];
const chart=[['1790','asset'],['1791','asset'],['7790','liability'],['2790','vat'],['4790','vat'],['4590','liability'],['3790','expense'],['3791','expense'],['8790','equity']];
export function createOriginalAccountsProof(base:string,work:string,onStep:(name:string)=>void){
  const root='/api/companies/synthetic/',api=root+'original/accounts/';
  const cookie=join(work,'accounts179.cookie'),readerCookie=join(work,'accounts179-reader.cookie'),key=join(work,'accounts179.key');
  let owner:Page,reader:Page,who:any,other:any,service:any,stable:any,ran=false,refused=false;
  const observations:any[]=[];
  const need=(v:unknown,s:string)=>{if(!v)throw Error('original_accounts: '+s);};
  function record(v:any){observations.push(v);writeFileSync(join(work,'accounts179-observations.json'),JSON.stringify(observations),{mode:0o600});}
  async function http(path:string,data?:any,status=200,page=owner,method='POST'){
    onStep((data===undefined?'GET':method)+':'+path);
    const r=data===undefined?await page.request.get(base+path):await page.request.fetch(base+path,{method,headers:{origin:base,'content-type':'application/json'},data});
    const v=await r.json();record({path,status:r.status(),value:path==='/api/workspace/service-principals'?{...v,secret:'[redacted]'}:v});
    need(r.status()===status,path+' expected HTTP'+status+' got '+r.status());return v;
  }
  const admin=(path:string,data:any)=>http('/api/workspace/'+path,data,path==='service-principals'?201:200);
  async function cookies(){for(const [page,file] of [[owner,cookie],[reader,readerCookie]] as const)writeFileSync(file,(await page.context().cookies(base)).map(c=>c.name+'='+c.value).join('; '),{mode:0o600});}
  function invoke(kind:'cli'|'mcp',op:string,args:any={},identity:'human'|'reader'|'service'='human',mode:'ok'|'refused'|'invalid'='ok'){
    onStep(kind+':'+op+':'+identity);
    const file=identity==='service'?key:identity==='reader'?readerCookie:cookie;
    const common=['--server',base,'--allow-loopback-http',identity==='service'?'--service-key-file':'--cookie-file',file];
    let argv:string[],input:string|undefined;
    if(kind==='cli'){
      const flat=op==='add'?{...args.input,...Object.fromEntries(Object.entries(args).filter(([k])=>k!=='input'))}:args;
      const flags=Object.entries(flat).flatMap(([k,v])=>['--'+(k==='accountNo'&&op==='role-confirm'?'account':k.replace(/[A-Z]/g,c=>'-'+c.toLowerCase())),k==='confirm'?(v?'yes':'no'):String(v)]);
      argv=['accounts',op,...common,'--company','synthetic','--json',...flags];
    }else{
      argv=common;
      input=[{jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'original-accounts179',version:'1'}}},{jsonrpc:'2.0',method:'notifications/initialized'},{jsonrpc:'2.0',id:2,method:op==='discovery'?'tools/list':'tools/call',params:op==='discovery'?{}:{name:'accounts_'+op.replaceAll('-','_'),arguments:{company:'synthetic',...args}}}].map(v=>JSON.stringify(v)).join('\n')+'\n';
    }
    const r=spawnSync(join(work,kind==='cli'?'rentemester-rust':'rentemester-mcp'),argv,{input,encoding:'utf8',timeout:45000,maxBuffer:16*1024*1024});
    for(const f of [cookie,readerCookie,key]){const secret=readFileSync(f,'utf8').trim();need(secret.length>0&&!r.stdout.includes(secret)&&!r.stderr.includes(secret),'credential privacy');}
    record({kind,op,args,identity,exit:r.status,stdout:r.stdout,stderr:r.stderr,terminalError:r.error?.name});
    need(!r.error&&r.status===(kind==='cli'&&mode!=='ok'?(mode==='invalid'?2:1):0),'bounded exact exit; no automatic write retry');
    if(kind==='cli'){
      if(mode==='invalid')return;
      const v=JSON.parse(r.stdout);need(v.ok===(mode==='ok'),'flat source ok');
      if(mode==='refused')need(Array.isArray(v.errors)&&v.errors.length>0,'real refusal errors');
      return v;
    }
    const reply=r.stdout.trim().split('\n').map(s=>JSON.parse(s)).find(v=>v.id===2);
    if(op==='discovery'){need(!reply?.error&&Array.isArray(reply?.result?.tools),'discovery');return reply.result.tools;}
    if(mode==='invalid'){need(reply?.error?.code===-32602,'strict RPC selectors');return;}
    const result=reply?.result,v=result?.structuredContent;
    need(!reply?.error&&result.content.length===1&&equal(JSON.parse(result.content[0].text),v),'MCP exact text/structured equality');
    need(v?.ok===(mode==='ok')&&result.isError===(mode!=='ok')&&Array.isArray(v.errors),'MCP envelope');
    if(mode!=='ok'){need(v.errors.length>0&&!Object.hasOwn(v,'data'),'failure no data');return v;}
    need(equal(v.errors,[])&&v.data,'success data');
    if(op==='add'){
      need(equal(Object.keys(v).sort(),['appliedRules','data','errors','ok'])&&equal(v.appliedRules,['DK-CHART-OF-ACCOUNTS-ADD-001'])&&equal(Object.keys(v.data),['accountNo']),'outer appliedRules, source add projection');
      return {ok:true,accountNo:v.data.accountNo,appliedRules:v.appliedRules,errors:[]};
    }
    if(op==='list'){
      need(equal(Object.keys(v.data).sort(),['accounts','count']),'list source envelope');
      return {ok:true,count:v.data.count,rows:v.data.accounts.map((r:any)=>{need(equal(Object.keys(r).sort(),['accountNo','defaultVatCode','name','type']),'four source list fields');return {account_no:r.accountNo,name:r.name,type:r.type,default_vat_code:r.defaultVatCode};})};
    }
    if(op==='role-confirm'){need(v.data.ok===true&&!Object.hasOwn(v.data,'errors'),'nested core ok retained');return v.data;}
    need(!Object.hasOwn(v.data,'ok')&&!Object.hasOwn(v.data,'errors'),'status excludes HTTP ok');return {ok:true,...v.data};
  }
  async function snapshot(){return {list:await http(api+'list'),status:await http(api+'roles-status'),native:await http(root+'accounts')};}
  function status(v:any,complete:boolean){
    need(v.ok===true&&v.status===(complete?'complete':'incomplete')&&equal(v.missing,complete?[]:roles)&&equal(v.ambiguous,[])&&equal(v.proposals,[])&&equal(v.candidates,[])&&v.roles.length===8&&equal(v.roles.map((r:any)=>r.role),roles),'exact eight source roles/order/status; no FX claims');
    if(complete)need(v.roles.every((r:any,i:number)=>r.ok===true&&r.accountNo===targets[i]&&r.version===(i===0?2:1)&&r.confirmedBy==='user:'+who.user.id&&r.confirmationSource==='explicit')&&equal(v.reasons,[]),'current explicit reconfirmed human resolutions');
    else need(v.roles.every((r:any)=>r.ok===false&&typeof r.error==='string'&&r.error.length>0),'unresolved errors');
  }
  async function parity(){
    const listed=await http(api+'list'),s=await http(api+'roles-status');
    need(equal(listed,invoke('cli','list'))&&equal(listed,invoke('mcp','list')),'exact list CLI/MCP/HTTP parity');
    need(equal(s,invoke('cli','roles-status'))&&equal(s,invoke('mcp','roles-status')),'exact status CLI/MCP/HTTP parity');
    return {listed,s};
  }
  async function run(page:Page,member:Page){
    need(!ran,'one finite public accounts journey');ran=true;owner=page;reader=member;
    who=await http('/api/me');other=await http('/api/me',undefined,200,reader);need(who.user.id!==other.user.id,'two independent normal MFA humans');
    const initial=await snapshot();need(equal(initial.list,{ok:true,count:0,rows:[]}),'fresh empty chart');status(initial.status,false);
    service=await admin('service-principals',{displayName:'Synthetic accounts179',operationId:randomUUID(),confirm:true});writeFileSync(key,service.secret+'\n',{mode:0o600,flag:'wx'});
    await admin('members/access',{userId:service.serviceAccountId,action:'set-role',workspaceRole:'member'});
    await admin('members/company',{userId:service.serviceAccountId,companySlug:'synthetic',action:'grant',role:'owner'});
    await cookies();
    const tools=invoke('mcp','discovery'),names=tools.map((t:any)=>t.name);need(tools.length===369&&new Set(names).size===369,'369 unique MCP tools');
    for(const op of ['list','add','roles_status','role_confirm']){const t=tools.find((t:any)=>t.name==='accounts_'+op),read=op==='list'||op==='roles_status';need(t?.inputSchema.additionalProperties===false&&t.inputSchema.required.includes('company')&&!Object.hasOwn(t.inputSchema.properties,'actor')&&!Object.hasOwn(t.inputSchema.properties,'principal')&&t.annotations.readOnlyHint===read&&t.annotations.idempotentHint===read&&t.annotations.destructiveHint===false&&t.annotations.openWorldHint===false,'exact discovery '+op);}
    await parity();
    for(const [i,[accountNo,type]] of chart.entries()){
      const input:any={accountNo,name:'Synthetic account '+accountNo,type};
      if(i===0){input.accountNo=' 1790 ';input.name=' Synthetic account 1790 ';}
      if(i===4)input.normalBalance='debit';
      if(i===7)input.allowDirectPosting=false;
      const receipt=invoke(i%2===0?'mcp':'cli','add',{input,confirm:true},i===0||i===6?'service':'human');
      need(equal(receipt,{ok:true,accountNo,appliedRules:['DK-CHART-OF-ACCOUNTS-ADD-001'],errors:[]}),'exact trimmed source add receipt');
    }
    for(const [i,role] of roles.entries()){
      const receipt=invoke(i%2===0?'cli':'mcp','role-confirm',{role,accountNo:targets[i],confirm:true},i===0?'service':'human');
      need(receipt.ok===true&&receipt.resolution.ok===true&&receipt.resolution.accountNo===targets[i]&&receipt.resolution.version===1&&receipt.resolution.confirmedBy===(i===0?'service_account:'+service.serviceAccountId:'user:'+who.user.id),'typed current role confirmation');
    }
    const reconfirm=invoke('mcp','role-confirm',{role:'bank',accountNo:'1790',confirm:true});need(reconfirm.resolution.version===2,'other interface explicit bank reconfirm appends v2');status(reconfirm,true);
    const {listed,s}=await parity();status(s,true);
    const expected=chart.map(([account_no,type])=>({account_no,name:'Synthetic account '+account_no,type,default_vat_code:null})).sort((a,b)=>a.account_no.localeCompare(b.account_no));
    need(equal(listed,{ok:true,count:9,rows:expected}),'ordered exact source list all nine');
    stable=await snapshot();
    return {facts:originalAccountsFacts,humanActor:'user:'+who.user.id,serviceActor:'service_account:'+service.serviceAccountId,currentServiceRoleAtWrites:'owner',stable,noAutomaticWriteRetry:true,manualDirectPostingPolicyClaim:false};
  }
  async function refusals(){
    need(ran&&!refused,'one bounded refusal phase after independent raw baseline');refused=true;
    const input={accountNo:'1790',name:'Synthetic duplicate',type:'asset'};
    invoke('cli','add',{input,confirm:true},'human','refused');invoke('mcp','add',{input,confirm:true},'human','refused');
    for(const bad of [{normalBalance:'sideways'},{type:'unknown'},{defaultVatCode:'NOT_A_VAT_CODE'}]){
      const v=await http(api+'add',{input:{accountNo:'invalid179',name:'Synthetic invalid',type:'expense',...bad},confirm:true},422);need(v.ok===false&&v.errors.length>0,'real business refusal');
    }
    const incompatible=await http(api+'role-confirm',{role:'bank',accountNo:'8790',confirm:true},422);need(incompatible.ok===false&&incompatible.errors.length>0,'incompatible source refusal');
    invoke('mcp','role-confirm',{role:'bank',accountNo:'8790',confirm:true},'human','refused');
    invoke('cli','add',{input},'human','invalid');invoke('mcp','add',{input},'human','refused');
    invoke('cli','role-confirm',{role:'bank',accountNo:'1790'},'human','invalid');
    await http(api+'add',{input,confirm:true,actor:'user:forged'},403);
    await http(api+'role-confirm',{role:'bank',accountNo:'1790',confirm:true,principal:'service_account:forged'},403);
    await admin('members/company',{userId:other.user.id,companySlug:'synthetic',action:'grant',role:'reader'});
    need(equal(await http(api+'list',undefined,200,reader),stable.list),'human reader read success');
    await http(api+'add',{input,confirm:true},403,reader);await http(api+'role-confirm',{role:'bank',accountNo:'1790',confirm:true},403,reader);
    invoke('cli','add',{input,confirm:true},'reader','refused');
    await admin('members/company',{userId:service.serviceAccountId,companySlug:'synthetic',action:'grant',role:'reader'});
    need(equal(invoke('mcp','list',{},'service'),stable.list),'current reader service can read');
    invoke('mcp','role-confirm',{role:'bank',accountNo:'1790',confirm:true},'service','refused');invoke('cli','add',{input,confirm:true},'service','refused');
    await admin('members/company',{userId:service.serviceAccountId,companySlug:'synthetic',action:'revoke'});
    await admin('service-principals/revoke',{serviceAccountId:service.serviceAccountId,credentialId:service.credentialId,operationId:randomUUID(),confirm:true});
    invoke('mcp','list',{},'service','refused');invoke('cli','add',{input,confirm:true},'service','refused');
    await http(api+'list?unknown=1',undefined,400);await http(api+'add',{input,confirm:true,unknown:true},400);
    await assertStable(owner);return {duplicateIncompatibleMissingConfirmForgedIdentityRefused:true,currentHumanReaderReadAndAdminRefusal:true,currentServiceReaderThenRevoked:true};
  }
  async function assertStable(page:Page){owner=page;need(equal(await snapshot(),stable),'chart/native reader/role history exact across reads, restart and fresh-owner restore');}
  async function readOnly(){await parity();await parity();await assertStable(owner);return {repeatedReadsNoMutations:true};}
  async function reopen(page:Page){await assertStable(page);await cookies();await parity();invoke('mcp','list',{},'service','refused');}
  return {run,refusals,assertStable,readOnly,reopen};
}
