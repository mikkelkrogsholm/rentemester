import type { Page } from '@playwright/test';
import { spawnSync } from 'node:child_process';
import { createHash, randomUUID } from 'node:crypto';
import { writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { isDeepStrictEqual as equal } from 'node:util';

// Work-Issue #697. Finite public-API fixture; never retry an uncertain write.
export const originalAccountingDraftFacts = Object.freeze({ documents:1, accounts:2, bankTransactions:0, events:7, versions:3, journalEntries:1, journalLines:2, amount:12.5, setupAudits:3, draftAudits:14, postAudits:1, audits:18, restoredAudits:19, scannerRequests:1, providerRequests:0, nativeMcpTools:369 });
export function createOriginalAccountingDraftProof(base:string, work:string, onStep:(name:string)=>void) {
  const root='/api/companies/synthetic/', api=root+'original/accounting-draft/', id='synthetic-draft180';
  const original='Synthetic accounting draft180 original: DKK12.50, VAT0.00, paid by owner.\n';
  const key=join(work,'draft180.key'), cookie=join(work,'draft180.cookie'), reviewerCookie=join(work,'draft180-reviewer.cookie');
  const states:any[]=[], payloads:any[]=[], observations:any[]=[];
  let owner:Page, reviewer:Page, service:any, stable:any, submitted:any, firstJournal:any, doc:number, ran=false;
  const need=(v:unknown,s:string)=>{if(!v)throw Error('original_accounting_draft: '+s);};
  async function http(path:string,data?:any,status=200,page=owner) {
    onStep((data===undefined?'GET:':'POST:')+path);
    const r=data===undefined?await page.request.get(base+path):await page.request.post(base+path,{headers:{origin:base,'content-type':'application/json'},data});
    const value=await r.json();
    observations.push({path,status:r.status(),value:path==='/api/workspace/service-principals'?{...value,secret:'[redacted]'}:value});
    writeFileSync(join(work,'draft180-observations.json'),JSON.stringify(observations),{mode:0o600});
    need(r.status()===status,path+' expected HTTP'+status+' got '+r.status()); return value;
  }
  const admin=(path:string,data:any)=>http('/api/workspace/'+path,data,path==='service-principals'?201:200);
  async function cookies(page:Page,file:string){writeFileSync(file,(await page.context().cookies(base)).map(c=>c.name+'='+c.value).join('; '),{mode:0o600});}
  function cli(op:string,args:any={},as:'owner'|'reviewer'|'service'='owner',exit=0) {
    onStep('cli:'+op+':'+as);
    const input={...args};
    if(Object.hasOwn(input,'payload')){const file=join(work,'draft180-payload-'+randomUUID()+'.json');writeFileSync(file,JSON.stringify(input.payload),{mode:0o600,flag:'wx'});delete input.payload;input.input=file;}
    const flags=Object.entries(input).flatMap(([k,v])=>['--'+k.replace(/[A-Z]/g,c=>'-'+c.toLowerCase()),k==='confirm'?(v?'yes':'no'):String(v)]);
    const r=spawnSync(join(work,'rentemester-rust'),['accounting-draft',op,'--company','synthetic','--server',base,'--allow-loopback-http',as==='service'?'--service-key-file':'--cookie-file',as==='service'?key:as==='reviewer'?reviewerCookie:cookie,'--json',...flags],{encoding:'utf8',timeout:45000,maxBuffer:4*1024*1024});
    need(!r.error&&r.status===exit,'bounded CLI '+op+' exit '+r.status+' expected '+exit+'; no automatic retry');
    need(!service||(!r.stdout.includes(service.secret)&&!r.stderr.includes(service.secret)),'credential privacy');
    if(exit===2){need(!r.stdout&&!!r.stderr.trim(),'usage stderr only');return;}
    const value=JSON.parse(r.stdout);need(value.ok===(exit===0),'flat source outcome');
    if(exit===1)need(Array.isArray(value.errors)&&value.errors.length>0,'real refusal strings');
    return value;
  }
  async function physical(){const r=await owner.request.get(base+root+`documents/${doc}/file`);need(r.ok()&&(await r.body()).equals(Buffer.from(original)),'exact retained original bytes');}
  async function snapshot(){const out:any={};for(const path of ['documents','accounts','journal','trial-balance','vat','bank','payables?status=all'])out[path]=await http(root+path);out.show=await http(api+'show?draftId='+id);out.list=await http(api+'list');return out;}
  async function parity(){need(equal(cli('show',{draftId:id}),await http(api+'show?draftId='+id)),'source show/direct HTTP parity');need(equal(cli('list'),await http(api+'list')),'source list/direct HTTP parity');}
  async function unchanged(action:()=>Promise<any>|any){const before=await snapshot();await action();need(equal(await snapshot(),before),'refusal/read/replay has no business effect');await physical();}
  async function discovery(){
    const input=[{jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'original-accounting-draft180',version:'1'}}},{jsonrpc:'2.0',method:'notifications/initialized'},{jsonrpc:'2.0',id:2,method:'tools/list',params:{}}].map(v=>JSON.stringify(v)).join('\n')+'\n';
    const r=spawnSync(join(work,'rentemester-mcp'),['--server',base,'--allow-loopback-http','--cookie-file',cookie],{input,encoding:'utf8',timeout:45000,maxBuffer:16*1024*1024});
    need(!r.error&&r.status===0,'native discovery terminal');const tools=r.stdout.trim().split('\n').map(s=>JSON.parse(s)).find(v=>v.id===2)?.result?.tools;
    need(tools?.length===369&&new Set(tools.map((t:any)=>t.name)).size===369,'369 unchanged native tools; source draft has no MCP aliases');
    return tools.map((t:any)=>t.name);
  }
  async function transition(op:string,args:any,as:'owner'|'reviewer'|'service',version:number,status:string){
    const value=cli(op,{draftId:id,...args,confirm:true},as),s=value.accountingDraft;
    need(s.id===id&&s.version===version&&s.status===status&&/^[a-f0-9]{64}$/.test(s.eventHash)&&/^[a-f0-9]{64}$/.test(s.payloadHash),'exact source transition');
    const expected=['id','version','status','payloadHash','eventHash','payload','actorId',...(status==='rejected'?['reason']:[]),...(status==='approved_posted'?['journalEntryId','journal']:[])].sort();
    need(equal(Object.keys(s).sort(),expected),'omit native nulls/exact state shape');
    states.push(s);await parity();return s;
  }
  async function run(page:Page,other:Page){
    need(!ran,'single finite lifecycle');ran=true;owner=page;reviewer=other;await cookies(owner,cookie);await cookies(reviewer,reviewerCookie);
    const who=await http('/api/me'),them=await http('/api/me',undefined,200,reviewer);
    const humanActor='user:'+who.user.id,reviewerActor='user:'+them.user.id;
    await admin('members/company',{userId:them.user.id,companySlug:'synthetic',action:'grant',role:'reviewer'});
    const date=(await http(root+'scheduled-recurring')).scheduledRecurring.asOfDate;
    for(const [accountNo,type,normalBalance] of [['3729','expense','debit'],['8729','equity','credit']])await http(root+'accounts',{accountNo,name:'Synthetic draft180 '+accountNo,type,normalBalance});
    doc=(await http(root+'documents/ingest',{confirm:true,fileName:'synthetic-draft180-source.txt',fileBase64:Buffer.from(original).toString('base64'),metadata:{source:'Synthetic accounting draft180',documentType:'cash_register_receipt',issueDate:date,amountIncVat:12.5,vatAmount:0,currency:'DKK'}})).document.id;
    need(Number.isSafeInteger(doc)&&doc>0,'real admitted original');
    service=await admin('service-principals',{displayName:'Synthetic accounting draft180',operationId:randomUUID(),confirm:true});writeFileSync(key,service.secret+'\n',{mode:0o600,flag:'wx'});
    await admin('members/access',{userId:service.serviceAccountId,action:'set-role',workspaceRole:'member'});
    await admin('members/company',{userId:service.serviceAccountId,companySlug:'synthetic',action:'grant',role:'bookkeeper'});
    const serviceActor='service_account:'+service.serviceAccountId;
    const nativeToolNames=await discovery();
    const payload={documentId:doc,transactionDate:date,text:'Synthetic draft180 v1',currency:'DKK',lines:[{accountNo:'3729',debitAmount:12.5,creditAmount:0},{accountNo:'8729',debitAmount:0,creditAmount:12.5}]};
    for(let v=1;v<=3;v++)payloads.push({...payload,text:'Synthetic draft180 v'+v});
    await unchanged(async()=>{
      need(equal(cli('show',{draftId:'missing-draft180'}),{ok:true,accountingDraft:null}),'missing read is null');
      cli('create',{draftId:id,payload},'service',2);
      await http(api+'create',{draftId:id,payload,confirm:false},422);
      await http(api+'create',{draftId:id,payload,confirm:true,unknown:true},400);
      await http(api+'create',{draftId:id,payload,confirm:true,actor:'user:forged'},403);
      await http(api+'create','{"draftId":"synthetic-draft180","draftId":"other","confirm":true}',400);
      await http(api+'list?unknown=1',undefined,400);
      await http(api+'show?draftId='+id+'&draftId='+id,undefined,400);
    });
    let s=await transition('create',{payload:payloads[0]},'service',1,'created');
    await unchanged(()=>{cli('create',{draftId:id,payload,confirm:true},'service',1);cli('revise',{draftId:id,payload:payloads[1],expectedEventHash:'0'.repeat(64),confirm:true},'service',1);});
    s=await transition('revise',{payload:payloads[1],expectedEventHash:s.eventHash},'service',2,'revised');
    s=await transition('submit',{expectedEventHash:s.eventHash},'owner',2,'submitted');
    await unchanged(async()=>{
      for(const op of ['reject','approve-and-post']){
        const args={draftId:id,expectedEventHash:s.eventHash,confirm:true,...(op==='reject'?{reason:'Self review forbidden'}:{})};
        cli(op,args,'owner',1);cli(op,args,'service',1);
      }
      await http(api+'approve-and-post',{draftId:id,expectedEventHash:s.eventHash,payload:payloads[2],confirm:true},400,reviewer);
      await http(api+'reject',{draftId:id,expectedEventHash:s.eventHash,reason:' ',confirm:true},422,reviewer);
    });
    s=await transition('reject',{expectedEventHash:s.eventHash,reason:'Independent reviewer requests corrected description'},'reviewer',2,'rejected');
    s=await transition('revise',{payload:payloads[2],expectedEventHash:s.eventHash},'service',3,'revised');
    submitted=await transition('submit',{expectedEventHash:s.eventHash},'owner',3,'submitted');
    s=await transition('approve-and-post',{expectedEventHash:submitted.eventHash},'reviewer',3,'approved_posted');firstJournal=s.journal;
    need(firstJournal.ok===true&&firstJournal.entryId===s.journalEntryId&&firstJournal.entryNo&&/^[a-f0-9]{64}$/.test(firstJournal.entryHash)&&firstJournal.appliedRules.length>0&&equal(firstJournal.errors,[]),'first approval retains actual required rules');
    need(equal(states.map(s=>s.actorId),[serviceActor,serviceActor,humanActor,reviewerActor,serviceActor,humanActor,reviewerActor]),'all seven exact typed actors');
    stable=await snapshot();need(stable.journal.journal.entries.length===1&&stable.journal.journal.entries[0].lines.length===2,'only one journal/two lines');
    await readOnly();
    await unchanged(async()=>{
      await admin('members/company',{userId:service.serviceAccountId,companySlug:'synthetic',action:'grant',role:'reader'});
      cli('revise',{draftId:id,payload:payloads[2],expectedEventHash:s.eventHash,confirm:true},'service',1);
      need(equal(cli('show',{draftId:id},'service'),stable.show),'current reader historical read');
      await admin('members/company',{userId:service.serviceAccountId,companySlug:'synthetic',action:'revoke'});
      cli('show',{draftId:id},'service',1);
      await admin('service-principals/revoke',{serviceAccountId:service.serviceAccountId,credentialId:service.credentialId,operationId:randomUUID(),confirm:true});
      cli('approve-and-post',{draftId:id,expectedEventHash:submitted.eventHash,confirm:true},'service',1);
      await admin('members/company',{userId:them.user.id,companySlug:'synthetic',action:'grant',role:'reader'});
      cli('approve-and-post',{draftId:id,expectedEventHash:submitted.eventHash,confirm:true},'reviewer',1);
      await admin('members/company',{userId:them.user.id,companySlug:'synthetic',action:'grant',role:'reviewer'});
    });
    return {facts:originalAccountingDraftFacts,humanActor,reviewerActor,serviceActor,states,payloads,firstJournal,stable,nativeToolNames,originalSha256:createHash('sha256').update(original).digest('hex'),noAutomaticWriteRetry:true};
  }
  async function readOnly(){await unchanged(async()=>{
    await parity();await parity();
    const replay=cli('approve-and-post',{draftId:id,expectedEventHash:submitted.eventHash,confirm:true},'reviewer').accountingDraft;
    need(equal(replay.journal,{...firstJournal,appliedRules:[]})&&equal({...replay,journal:undefined},{...states[6],journal:undefined}),'exact same-reviewer receipt replay, empty rules');
    cli('approve-and-post',{draftId:id,expectedEventHash:submitted.eventHash,confirm:true},'owner',1);
    cli('approve-and-post',{draftId:id,expectedEventHash:'0'.repeat(64),confirm:true},'reviewer',1);
    cli('revise',{draftId:id,expectedEventHash:states[6].eventHash,payload:payloads[2],confirm:true},'service',1);
  });return {repeatedReadOnly:true,exactReplay:true};}
  async function assertStable(page:Page){owner=page;need(equal(await snapshot(),stable),'exact restart/fresh-owner restore/reopen');await physical();}
  async function reopen(page:Page){await assertStable(page);await cookies(owner,cookie);await parity();cli('show',{draftId:id},'service',1);}
  return {run,readOnly,assertStable,reopen};
}
