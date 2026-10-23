import type { Page } from '@playwright/test';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { writeFileSync, unlinkSync } from 'node:fs';
import { join } from 'node:path';
import { isDeepStrictEqual as equal } from 'node:util';

// Work-Issue #697. Source adapters only; the existing native atomic engine owns money.
export function createOriginalBookkeepingBatchProof(base:string, work:string, onStep:(name:string)=>void) {
  const root='/api/companies/synthetic/', batch=root+'bookkeeping-batch';
  const need=(v:unknown,s:string)=>{if(!v)throw Error('original_bookkeeping_batch: '+s);};
  let owner:Page, reviewer:Page, stable:any, applied:any, persisted:any, plan:any, fixture:any, ran=false;
  const evidence:any={cli:[],mcp:[],refusals:[]};
  async function get(page:Page,path:string){const r=await page.request.get(base+path);need(r.status()===200,'canonical read '+path);return r.json();}
  async function native(page:Page,path:string,data:any){const r=await page.request.post(base+path,{headers:{origin:base,'content-type':'application/json'},data});return {status:r.status(),value:await r.json()};}
  async function invoke(page:Page,kind:'cli'|'mcp',op:string,input:any={},exit=0){
    onStep(kind+':'+op);const cookie=join(work,'batch182-private.cookie');
    writeFileSync(cookie,(await page.context().cookies(base)).map(c=>c.name+'='+c.value).join('; '),{mode:0o600,flag:'wx'});
    try {
      const common=['--server',base,'--allow-loopback-http','--cookie-file',cookie];
      const flags=Object.entries(input).flatMap(([k,v])=>['--'+k.replace(/[A-Z]/g,c=>'-'+c.toLowerCase()),k==='confirm'?(v?'yes':'no'):String(v)]);
      const params=op==='list'?{method:'tools/list',params:{}}:{method:'tools/call',params:{name:'bookkeeping_batch_'+op.replace('-','_'),arguments:{company:'synthetic',...input}}};
      const wire=[{jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'original-batch182',version:'1'}}},{jsonrpc:'2.0',method:'notifications/initialized'},{jsonrpc:'2.0',id:2,...params}].map(v=>JSON.stringify(v)).join('\n')+'\n';
      const r=spawnSync(join(work,kind==='cli'?'rentemester-rust':'rentemester-mcp'),kind==='cli'?['bookkeeping-batch',op,...common,'--company','synthetic','--json',...flags]:common,{input:kind==='mcp'?wire:'',encoding:'utf8',timeout:45000,maxBuffer:16*1024*1024});
      need(!r.error&&r.status===(kind==='cli'?exit:0),'terminal '+kind+' '+op+' exit '+r.status+'; never retry unknown writes');
      if(kind==='cli'&&exit===2){need(!r.stdout&&r.stderr.trim(),'usage stderr only');evidence.refusals.push({op,exit});return;}
      if(kind==='cli'){const v=JSON.parse(r.stdout);need(v.ok===(exit===0),'source CLI outcome');evidence.cli.push({op,exit,value:v});return v;}
      const reply=r.stdout.trim().split('\n').map(l=>JSON.parse(l)).find(v=>v.id===2);need(!reply.error&&reply.result,'MCP protocol result');
      if(op==='list')return reply.result.tools;
      const v=reply.result.structuredContent;need(v&&v.ok===(exit===0),'source MCP outcome');
      const text=reply.result.content.filter((c:any)=>c.type==='text').map((c:any)=>c.text).join('\n');need(equal(JSON.parse(text),v),'structured/text agreement');
      evidence.mcp.push({op,exit,value:v});
      return op==='status'?v.data:{ok:v.ok,...v.data};
    } finally {unlinkSync(cookie);}
  }
  const reports=async(page:Page)=>({journal:await get(page,root+'journal'),trialBalance:await get(page,root+'trial-balance'),bank:await get(page,root+'bank'),vat:await get(page,root+'vat')});
  async function physical(page:Page){for(const d of fixture.originals){const r=await page.request.get(base+root+`documents/${d.id}/file`);need(r.ok()&&createHash('sha256').update(await r.body()).digest('hex')===d.sha256,'retained original bytes');}return fixture.originals;}
  async function snapshot(page:Page){return {state:await get(page,batch+'/runs/'+persisted.runId),reports:await reports(page),originals:await physical(page),supplier:await get(page,root+'workspace-parties/'+encodeURIComponent(fixture.supplier.party.partyId)),partyLinks:await Promise.all(fixture.linkedOriginals.map(async(x:any)=>({documentId:x.documentId,history:await get(page,root+`documents/${x.documentId}/party-links`)})))};}
  async function unchanged(fn:()=>Promise<any>){const before=await snapshot(owner);await fn();need(equal(await snapshot(owner),before),'reads/replays/refusals preserve exact history and finances');}
  async function run(ownerPage:Page,memberPage:Page,input:any){
    need(!ran,'single finite run');ran=true;owner=ownerPage;reviewer=memberPage;fixture=input;
    const tools=await invoke(owner,'mcp','list');need(tools.length===370&&new Set(tools.map((t:any)=>t.name)).size===370,'370 unique tools');for(const name of ['plan','persist','dry_run','approve','apply','status'])need(tools.some((t:any)=>t.name==='bookkeeping_batch_'+name),'required source '+name);evidence.toolNames=tools.map((t:any)=>t.name);
    const before=await reports(owner),originalPlanResponse=await get(owner,batch+'?'+new URLSearchParams(input.scope));
    const cliPlan=await invoke(owner,'cli','plan',input.scope),mcpPlan=await invoke(owner,'mcp','plan',input.scope);
    need(equal(cliPlan,originalPlanResponse)&&equal(mcpPlan,originalPlanResponse),'source plan exact native parity');plan=cliPlan.plan;
    need(plan.executionPolicy==='atomic_v1'&&plan.items.length===2&&plan.items.every((i:any)=>i.partition==='ready'&&input.originals.some((d:any)=>d.id===i.documentId)&&i.ruleApplication.payloadHash===input.rule.payloadHash),'two real source-backed ready purchases');
    need(equal(await reports(owner),before),'pure planning effect free');
    const reviewed={...input.scope,runKey:'synthetic-original-batch182',expectedPlanHash:plan.planHash,confirm:true};
    persisted=await invoke(owner,'cli','persist',reviewed);need(persisted.runId>0&&!persisted.duplicate&&equal(persisted.plan,plan),'one immutable reviewed run');
    await unchanged(async()=>{for(const [kind,op] of [['cli','dry-run'],['mcp','persist'],['mcp','dry-run']] as const){const r=await invoke(owner,kind,op,reviewed);need(r.duplicate===true&&r.runId===persisted.runId&&equal(r.state,persisted.state)&&equal(r.plan,plan),'same-key durable dry-run exact duplicate');if(op==='dry-run')need(r.dryRun===true,'durable dry-run projection');}});
    const exact={runId:persisted.runId,planHash:plan.planHash,confirm:true};
    await unchanged(async()=>{
      await invoke(owner,'cli','persist',{...reviewed,expectedPlanHash:'0'.repeat(64)},1);
      await invoke(owner,'cli','approve',exact,1);
      const {confirm,...unconfirmed}=reviewed;await invoke(owner,'cli','persist',unconfirmed,2);
      await invoke(owner,'cli','plan',{...input.scope,unknown:'refuse'},2);
      await invoke(owner,'cli','plan',{...input.scope,companyId:plan.companyId+1},1);
      const stale=await native(owner,batch+'/apply',{...exact,planHash:'0'.repeat(64)});need(stale.status===422&&stale.value.ok===false,'observed native stale hash refusal');evidence.refusals.push(stale);
    });
    const who=await get(owner,'/api/me'),them=await get(reviewer,'/api/me');need(who.user.id!==them.user.id,'independent current MFA humans');
    // Current company authority is changed through the existing admission API, never forged in input.
    const access=async(action:string,role?:string)=>{const r=await native(owner,'/api/workspace/members/company',{userId:them.user.id,companySlug:'synthetic',action,...(role?{role}:{})});need(r.status===200&&r.value.ok===true,'ordinary company admission');};
    await access('grant','reader');await unchanged(async()=>{await invoke(reviewer,'cli','approve',exact,1);});
    await access('revoke');await unchanged(async()=>{await invoke(reviewer,'cli','approve',exact,1);});await access('grant','reviewer');
    const approval=await invoke(reviewer,'cli','approve',exact);
    need(approval.state.revisions.some((r:any)=>r.planHash===plan.planHash&&r.plannerSubjectId===who.user.id&&r.approverSubjectId===them.user.id&&r.approvedAt),'independent exact human review');
    await unchanged(async()=>{need(equal(await invoke(reviewer,'mcp','approve',exact),approval),'approval exact replay');});
    applied=await invoke(owner,'cli','apply',exact);need(applied.results.length===2&&applied.results.every((r:any)=>r.outcome==='applied'),'first atomic apply');
    for(const name of ['audit_chain','trial_balance','reconciliation','vat'])need(applied.checks.some((c:any)=>c.name===name&&c.ok===true&&c.detail&&typeof c.detail==='object'),'real '+name+' checks');
    for(const i of plan.items){const r=applied.results.find((r:any)=>r.actionKey===i.actionKey)?.receipt;need(r&&r.evidenceHash===i.evidenceHash&&r.planHash===plan.planHash&&r.documentId===i.documentId&&r.bankTransactionId===i.bankTransactionId,'exact receipt source binding');for(const key of ['journalEntryId','vatPreflightId','postingRuleApplicationId','purchaseApplicationId'])need(Number.isSafeInteger(r[key])&&r[key]>0,'persisted '+key);need(/^[a-f0-9]{64}$/.test(r.journalEntryHash),'journal hash');}
    stable=await snapshot(owner);need(stable.state.state.receipts.length===2&&stable.reports.journal.journal.entries.length===2,'two durable receipts and journals');
    const entries=stable.reports.journal.journal.entries;need(entries.reduce((n:number,e:any)=>n+e.lines.length,0)===6,'six journal lines');for(const d of input.originals)need(entries.filter((e:any)=>e.documentId===d.id&&e.total===d.gross&&e.lines.length===3).length===1,'exact 125/250 source posting');
    const tb=stable.reports.trialBalance.trialBalance,bank=stable.reports.bank.bank,vat=stable.reports.vat.vat;need(tb.totalDebit===375&&tb.totalCredit===375,'375 balanced');need(bank.matchedCount===2&&bank.unmatchedCount===0&&bank.actualBalance===-375&&bank.bookedBalance===-375&&bank.difference===0,'real bank reconciliation');need(vat.inputVat===75&&vat.payable===-75&&vat.vatReportErrors.length===0,'VAT75');
    await unchanged(async()=>{const replay=await invoke(owner,'mcp','apply',exact);need(replay.duplicate===true&&replay.results.length===2&&replay.results.every((r:any)=>r.outcome==='duplicate'&&equal(r.receipt,applied.results.find((x:any)=>x.actionKey===r.actionKey).receipt)),'explicit MCP exact persisted receipt replay');for(const kind of ['cli','mcp'] as const)need(equal(await invoke(owner,kind,'status',{runId:persisted.runId}),stable.state),'exact source status compatibility');});
    await owner.goto(base+'/companies/synthetic/batchbogfoering?runId='+persisted.runId);
    await owner.getByRole('heading',{name:'Bogføringskø',exact:true}).waitFor();
    await owner.screenshot({path:process.env.RM_BATCH_SCREENSHOT,fullPage:true});
    return {plan,persisted,approval,applied,stable,originalPlanResponse,evidence};
  }
  async function assertStable(page:Page){need(equal(await snapshot(page),stable),'immutable historical batch/source/report snapshot');}
  return {run,assertStable,reopen:assertStable};
}
