import type { Page } from '@playwright/test';
import { spawnSync } from 'node:child_process';
import { createHash, randomUUID } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { isDeepStrictEqual as equal } from 'node:util';

// Independent oracle: the ordinary bank import creates ONE unmatched-bank task.
// No rule operation may create an exception or resolve that setup task.
export const originalPostingRulesFacts = Object.freeze({ documents:1, gross:125, vat:25, bankTransactions:1, versions:2, events:6, ruleAuditDelta:6, setupExceptions:1, exceptionDelta:0, journals:0, lines:0, applications:0, payables:0, payments:0, scannerRequests:1, providerRequests:0 });
export function createOriginalPostingRulesProof(base:string,work:string,onStep:(name:string)=>void){
  const root='/api/companies/synthetic/',api=root+'original/posting-rules/',ruleId='synthetic-original-posting-rules178';
  const cookie=join(work,'posting-rules178.cookie'),reviewCookie=join(work,'posting-rules178-reviewer.cookie'),key=join(work,'posting-rules178.key');
  const observations:any[]=[];
  let owner:Page,reviewer:Page,service:any,who:any,other:any,date:string,document:number,context:any,proposal:any,baseline:any,stable:any,setupRan=false,ran=false;
  const bytes=Buffer.from('Synthetic batch original 1\nSupplier DK12345678\nGross 125.00 VAT 25.00 DKK\n');
  const need=(v:unknown,s:string)=>{if(!v)throw Error('original_posting_rules: '+s);};
  function record(v:any){observations.push(v);writeFileSync(join(work,'posting-rules178-observations.json'),JSON.stringify(observations),{mode:0o600});}
  async function http(path:string,data?:any,status=200,page=owner,method='POST'){
    onStep((data===undefined?'GET':method)+':'+path);
    const r=data===undefined?await page.request.get(base+path):await page.request.fetch(base+path,{method,headers:{origin:base,'content-type':'application/json'},data});
    const v=await r.json();record({path,status:r.status(),value:path==='/api/workspace/service-principals'?{...v,secret:'[redacted]'}:v});
    need(r.status()===status,path+' expected HTTP'+status+' got '+r.status());return v;
  }
  const admin=(path:string,data:any)=>http('/api/workspace/'+path,data,path==='service-principals'?201:200);
  async function cookies(){for(const [page,file] of [[owner,cookie],[reviewer,reviewCookie]] as const)writeFileSync(file,(await page.context().cookies(base)).map(c=>c.name+'='+c.value).join('; '),{mode:0o600});}
  function invoke(kind:'cli'|'mcp',op:string,args:any={},identity:'human'|'reviewer'|'service'='human',mode:'ok'|'refused'|'invalid'='ok'){
    onStep(kind+':'+op+':'+identity);
    const file=identity==='service'?key:identity==='reviewer'?reviewCookie:cookie;
    const common=['--server',base,'--allow-loopback-http',identity==='service'?'--service-key-file':'--cookie-file',file];
    let argv:string[],input:string|undefined;
    if(kind==='cli'){
      const flags=Object.entries(args).flatMap(([k,v])=>['--'+(k==='proposal'?'input':k.replace(/[A-Z]/g,c=>'-'+c.toLowerCase())),typeof v==='object'?JSON.stringify(v):k==='confirm'?(v?'yes':'no'):String(v)]);
      argv=['posting-rules',op,...common,'--company','synthetic','--json',...flags];
    }else{
      argv=common;
      input=[{jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'original-posting-rules178',version:'1'}}},{jsonrpc:'2.0',method:'notifications/initialized'},{jsonrpc:'2.0',id:2,method:op==='discovery'?'tools/list':'tools/call',params:op==='discovery'?{}:{name:'posting_rule_'+op,arguments:{company:'synthetic',...args}}}].map(v=>JSON.stringify(v)).join('\n')+'\n';
    }
    const r=spawnSync(join(work,kind==='cli'?'rentemester-rust':'rentemester-mcp'),argv,{input,encoding:'utf8',timeout:45000,maxBuffer:16*1024*1024});
    for(const f of [cookie,reviewCookie,key]){const secret=readFileSync(f,'utf8').trim();need(secret.length>0&&!r.stdout.includes(secret)&&!r.stderr.includes(secret),'credential privacy');}
    record({kind,op,args,identity,exit:r.status,stdout:r.stdout,stderr:r.stderr,terminalError:r.error?.name});
    need(!r.error&&r.status===(kind==='cli'&&mode!=='ok'?(mode==='invalid'?2:1):0),'bounded exact exit; never retry unknown write');
    if(kind==='cli'){
      if(mode==='invalid')return;
      const v=JSON.parse(r.stdout);
      if(['test','explain'].includes(op)&&mode==='ok')need(!Object.hasOwn(v,'ok')&&!Object.hasOwn(v,'errors')&&v.dryRun===true,'flat source read without ok/errors');
      else need(v.ok===(mode==='ok')&&Array.isArray(v.errors)&&(mode==='ok'||v.errors.length>0),'flat source write/refusal');
      return v;
    }
    const reply=r.stdout.trim().split('\n').map(s=>JSON.parse(s)).find(v=>v.id===2);
    if(op==='discovery'){need(!reply?.error&&Array.isArray(reply?.result?.tools),'tool discovery');return reply.result.tools;}
    if(mode==='invalid'){need(reply?.error?.code===-32602,'strict RPC selectors');return;}
    const result=reply?.result,v=result?.structuredContent;
    need(!reply?.error&&result.content.length===1&&equal(JSON.parse(result.content[0].text),v),'MCP exact text/structured equality');
    need(v?.ok===(mode==='ok')&&result.isError===(mode!=='ok')&&Array.isArray(v.errors),'MCP source outer envelope');
    if(mode!=='ok'){need(v.errors.length>0&&!Object.hasOwn(v,'data'),'source failure omits data');return v;}
    need(equal(v.errors,[])&&v.data&&!Object.hasOwn(v.data,'ok')&&!Object.hasOwn(v.data,'errors'),'core minus ok/errors');
    return op==='explain'?v.data:{ok:true,errors:[],...v.data};
  }
  async function financial(){const v:any={};for(const p of ['journal','trial-balance','vat','bank','payables?status=all','documents'])v[p]=await http(root+p);return v;}
  async function snapshot(){return {rules:await http(root+'posting-rules'),financial:await financial(),explain:await http(api+'explain',{context})};}
  async function physical(){const r=await owner.request.get(base+root+`documents/${document}/file`);need(r.status()===200&&equal(await r.body(),bytes),'exact retained scanned original');}
  function proposed(v:any,receipt:any){need(v.decision==='proposed'&&v.dryRun===true&&v.payloadHash===receipt.payloadHash&&Number.isSafeInteger(v.ruleVersionId)&&v.ruleVersionId>0&&equal(v.outcome,proposal.outcome)&&equal(v.reasons,[])&&v.matched.length===1,'unique exact independent proposal');need(v.explanations.some((e:any)=>e.ruleId===ruleId&&e.version===receipt.version&&e.payloadHash===receipt.payloadHash&&e.state==='approved'&&e.matched),'exact matching explanation');}
  function human(v:any){need(v.decision==='human_decision'&&v.dryRun===true&&v.reasons.length>0&&['outcome','ruleVersionId','payloadHash'].every(k=>!Object.hasOwn(v,k)),'successful human decision without purchase authority');}
  async function setup(page:Page,member:Page){
    need(!setupRan,'one finite public setup');setupRan=true;owner=page;reviewer=member;record({facts:originalPostingRulesFacts});
    who=await http('/api/me');other=await http('/api/me',undefined,200,reviewer);need(who.user.id!==other.user.id,'two independent normal MFA humans');
    date=(await http(root+'scheduled-recurring')).scheduledRecurring.asOfDate;
    await http(root+'company',{cvr:'87654321',address:'Synthetic Batch Street 1',postalCode:'1000',city:'Synthetic City',vatPeriodType:'quarter',paymentTermsDays:14},200,owner,'PATCH');
    for(const [accountNo,type,normalBalance] of [['1800','asset','debit'],['6000','expense','debit'],['4700','vat','debit']])await http(root+'accounts',{accountNo,name:'Synthetic Batch '+accountNo,type,normalBalance});
    for(const [role,accountNo] of [['bank','1800'],['input_vat','4700']])await http(root+'account-roles',{role,accountNo,confirm:true});
    await http(root+'bank-accounts',{slug:'synthetic-batch-bank',name:'Synthetic Batch Bank',currency:'DKK',ledgerAccountNo:'1800'});
    const ingested=await http(root+'documents/ingest',{confirm:true,fileName:'synthetic-batch-purchase-1.txt',fileBase64:bytes.toString('base64'),metadata:{source:'Synthetic batch ordinary intake',documentType:'purchase_sale',issueDate:date,invoiceNo:'SYN-BATCH-1',deliveryDescription:'Synthetic supplies 1',amountIncVat:125,vatAmount:25,currency:'DKK',sender:{name:'Synthetic supplier',address:'Synthetic supplier street 1',vatOrCvr:'DK12345678'},recipient:{name:'Synthetic Batch Company',address:'Synthetic Batch Street 1',vatOrCvr:'DK87654321'}}});
    document=ingested.document.id;
    const csv=`dato;tekst;beløb;valuta;saldo\n${date};SYN-BATCH-1;-125,00;DKK;-125,00\n`;
    const imported=await http(root+'bank/import',{confirm:true,csvContent:csv,account:'synthetic-batch-bank'});need(imported.import.imported===1&&imported.import.skippedDuplicates===0,'one actual archived bank source');
    service=await admin('service-principals',{displayName:'Synthetic posting rules178',operationId:randomUUID(),confirm:true});writeFileSync(key,service.secret+'\n',{mode:0o600,flag:'wx'});
    await admin('members/access',{userId:service.serviceAccountId,action:'set-role',workspaceRole:'member'});
    await admin('members/company',{userId:service.serviceAccountId,companySlug:'synthetic',action:'grant',role:'reviewer'});
    await cookies();
    context={supplierIdentity:'Synthetic supplier',documentType:'purchase_sale',supplierCountry:'DK',supplierVat:'DK12345678',currency:'DKK',amount:125,vatAmount:25,attributes:{bankText:'SYN-BATCH-1'},documentId:document};
    proposal={ruleId,effectiveFrom:date,conditions:{supplierIdentity:'Synthetic supplier',documentType:'purchase_sale',supplierCountry:'DK',supplierVat:'DK12345678',currency:'DKK',vat:'positive',amount:{min:100,max:150},attributes:{bankText:'SYN-BATCH-1'}},outcome:{account:'6000',vatTreatment:'standard',textTemplate:'Synthetic source purchase'},provenance:'Synthetic ordinary scanned original178',rationale:'Synthetic reviewed broad source rule v1'};
    baseline=await financial();need(baseline.journal.journal.entries.length===0&&baseline.bank.bank.transactions.length===1,'baseline AFTER setup, no journals');await physical();
    return {facts:originalPostingRulesFacts,baselineAfterSetup:baseline,csvSha256:createHash('sha256').update(csv).digest('hex'),document};
  }
  async function run(){
    need(setupRan&&!ran,'one finite journey; never retry mutations');ran=true;
    const tools=invoke('mcp','discovery'),names=tools.map((t:any)=>t.name);need(tools.length===365&&new Set(names).size===365,'365 unique discovery tools');
    for(const op of ['propose','approve','explain']){const t=tools.find((t:any)=>t.name==='posting_rule_'+op);need(t?.inputSchema.additionalProperties===false&&t.inputSchema.required.includes('company')&&!Object.hasOwn(t.inputSchema.properties,'actor')&&!Object.hasOwn(t.inputSchema.properties,'principal')&&t.annotations.readOnlyHint===(op==='explain')&&t.annotations.idempotentHint===(op!=='approve')&&t.annotations.destructiveHint===false&&t.annotations.openWorldHint===false,'source discovery '+op);}
    for(const op of ['disable','supersede','test'])need(!names.includes('posting_rule_'+op),'no extra source MCP alias');
    const transition=(v:any)=>({ruleId,version:v.version,expectedPayloadHash:v.payloadHash,rationale:'Synthetic independent exact original review',provenance:'Synthetic source178',confirm:true});
    invoke('cli','propose',{proposal},'service','invalid');invoke('mcp','propose',{proposal},'service','refused');
    invoke('cli','propose',{proposal,confirm:true,unknown:true},'service','invalid');
    await http(api+'propose',{proposal,confirm:true,unknown:true},400);
    await http(api+'propose',{proposal,confirm:true,actor:'user:forged'},403);
    const first=invoke('cli','propose',{proposal,confirm:true},'service');need(first.version===1&&first.duplicate===false&&/^[a-f0-9]{64}$/.test(first.payloadHash),'v1 source receipt');
    const afterProposal=await http(root+'posting-rules');
    const duplicate=invoke('cli','propose',{proposal,confirm:true},'service');need(duplicate.duplicate===true&&duplicate.version===1&&duplicate.payloadHash===first.payloadHash,'duplicate same version/hash');
    invoke('cli','approve',transition(first),'service','refused');
    invoke('mcp','approve',{...transition(first),expectedPayloadHash:'0'.repeat(64)},'reviewer','refused');
    need(equal(await http(root+'posting-rules'),afterProposal),'duplicate/self/wrong hash no history effects');
    invoke('cli','approve',{...transition(first),confirm:false},'human','invalid');
    invoke('mcp','approve',transition(first),'reviewer');
    const exact=invoke('cli','test',{context});proposed(exact,first);need(equal(exact,invoke('mcp','explain',{context})),'CLI test / MCP explain exact source parity');
    need(equal(exact,invoke('cli','explain',{context})),'sixth CLI explain exact parity');
    for(const changed of [{amount:151},{currency:'EUR'},{attributes:{bankText:'different'}}]){const v=invoke('cli','test',{context:{...context,...changed}});human(v);need(v.matched.length===0,'exact mismatch not silently discarded');}
    for(const flag of ['changedBuyer','changedSupplier','changedVat','changedCurrency','unusualAmount','contradictoryEvidence'])human(invoke('mcp','explain',{context:{...context,[flag]:true}}));
    // Public batch read invokes the EXISTING purchase_rules::inspect. Broad rules
    // must not acquire typed purchase authority or fall back to another rule.
    const q=new URLSearchParams({accountingFrom:date,accountingTo:date,bankFrom:date,bankTo:date});
    const native=await http(root+'bookkeeping-batch?'+q);
    need(native.dryRun===true&&native.plan.items.length===1,'one genuine native purchase candidate');
    const item=native.plan.items[0];need(item.documentId===document&&item.partition==='humanDecision'&&item.ruleApplication===null&&item.detail.rule.decision==='human_decision'&&item.detail.rule.explanation.reasons.some((r:string)=>r.includes('unsupported')),'native broad purchase human refusal, no rule application');
    invoke('cli','disable',transition(first));human(invoke('cli','explain',{context}));
    proposal={...proposal,rationale:'Synthetic reviewed broad source rule v2'};
    const second=invoke('cli','propose',{proposal,confirm:true},'service');need(second.version===2&&!second.duplicate&&second.payloadHash!==first.payloadHash,'separate v2, changed rationale hash');
    invoke('mcp','approve',transition(second),'reviewer');proposed(invoke('cli','test',{context}),second);
    invoke('cli','supersede',transition(second));human(invoke('mcp','explain',{context}));
    await admin('members/company',{userId:other.user.id,companySlug:'synthetic',action:'grant',role:'reader'});
    await http(api+'approve',transition(second),403,reviewer);human(await http(api+'explain',{context},200,reviewer));
    await admin('members/company',{userId:service.serviceAccountId,companySlug:'synthetic',action:'grant',role:'reader'});
    invoke('mcp','propose',{proposal,confirm:true},'service','refused');human(invoke('mcp','explain',{context},'service'));
    await admin('members/company',{userId:service.serviceAccountId,companySlug:'synthetic',action:'revoke'});
    await admin('service-principals/revoke',{serviceAccountId:service.serviceAccountId,credentialId:service.credentialId,operationId:randomUUID(),confirm:true});
    invoke('mcp','explain',{context},'service','refused');invoke('cli','propose',{proposal,confirm:true},'service','refused');
    await http(api+'explain?unknown=1',{context},400);await http(api+'test',{context:{...context,unknown:true}},400);
    stable=await snapshot();need(equal(stable.financial,baseline),'all writes/reads/refusals preserve setup financial inventory');await physical();
    return {facts:originalPostingRulesFacts,versions:[first,second],humanActor:'user:'+who.user.id,reviewerActor:'user:'+other.user.id,serviceActor:'service_account:'+service.serviceAccountId,baselineAfterSetup:true,nativeBroadPurchaseHumanRefusal:true,noAutomaticWriteRetry:true,stable};
  }
  async function assertStable(page:Page){owner=page;need(equal(await snapshot(),stable),'exact terminal versions/financial history across restart and fresh-owner recovery');await physical();}
  async function reopen(page:Page){await assertStable(page);await cookies();human(invoke('cli','test',{context}));need(equal(invoke('mcp','explain',{context}),stable.explain),'restart source parity');invoke('mcp','explain',{context},'service','refused');}
  async function readOnly(){for(let i=0;i<2;i++){human(invoke('cli','test',{context}));need(equal(invoke('mcp','explain',{context}),stable.explain),'repeat dry-run parity');}await assertStable(owner);return {repeatedReadOnly:true};}
  return {setup,run,assertStable,reopen,readOnly};
}
