import type { Page } from '@playwright/test';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { writeFileSync, readFileSync, unlinkSync } from 'node:fs';
import { join } from 'node:path';
import { isDeepStrictEqual } from 'node:util';

type Query = {kind:string; query:Record<string,string>};
/** Original three dated CLI reports, with ordinary retained originals and current authority. */
export function createDatedReportsProof(page:Page, base:string, work:string, member:Page) {
  const prefix='/api/companies/synthetic/', ownerFile=join(work,'dated-owner.cookie'), memberFile=join(work,'dated-member.cookie');
  const need=(v:unknown,m:string)=>{if(!v)throw Error(m);};
  const hash=(v:unknown)=>createHash('sha256').update(JSON.stringify(v)).digest('hex');
  let stable:any, saved:any[], cases:Query[], completed=false, sourceFiles:{id:number;bytes:Buffer}[]=[];
  async function get(path:string, actor=page){const r=await actor.request.get(base+path);need(r.ok(),'Dated canonical read '+path+' HTTP'+r.status());return r.json();}
  async function post(path:string,data:unknown){const r=await page.request.post(base+path,{headers:{origin:base},data});const v=await r.json();need(r.ok()&&v.ok!==false,'Dated reviewed write '+path+' HTTP'+r.status()+' '+JSON.stringify(v));return v;}
  async function cookie(actor:Page,file:string){const c=await actor.context().cookies(base);need(c.some(c=>c.httpOnly),'Ordinary MFA cookie');writeFileSync(file,c.map(c=>c.name+'='+c.value).join('; '),{mode:0o600});}
  function cli(c:Query,human=false,file=ownerFile,expected=0,missing=false){
    const flags=missing?[]:Object.entries(c.query).flatMap(([k,v])=>['--'+(k==='asOf'?'as-of':k),v]);
    const r=spawnSync(join(work,'rentemester-rust'),['report',c.kind,'--server',base,'--cookie-file',file,'--company','synthetic','--allow-loopback-http',...(human?[]:['--json']),...flags],{encoding:'utf8',timeout:45000,maxBuffer:96*1024*1024});
    const secret=readFileSync(file,'utf8').trim();need(!r.error&&r.status===expected&&!r.stdout.includes(secret)&&!r.stderr.includes(secret),'Dated CLI terminal exit/credential privacy '+c.kind);
    if(expected===2||human&&expected===1||file===memberFile){need(r.stdout===''&&r.stderr.length>0,'Original refusal stderr and empty stdout');return r.stderr;}
    need(r.stderr===''&&r.stdout.endsWith('\n'),'Original report stdout and newline');
    return human?r.stdout:JSON.parse(r.stdout);
  }
  async function report(c:Query,actor=page){return get(prefix+'reports/'+c.kind+'?'+new URLSearchParams(c.query),actor);}
  async function books(){return {journal:await get(prefix+'journal/entries?all=true'),trial:await get(prefix+'trial-balance'),bank:await get(prefix+'bank'),documents:await get(prefix+'documents'),audit:await get(prefix+'audit-log?limit=5000')};}
  async function unchanged(){need(isDeepStrictEqual(await books(),stable),'Dated report reads preserve books, originals and company audit');}
  async function originals(){for(const s of sourceFiles){const r=await page.request.get(base+prefix+'documents/'+s.id+'/file');need(r.ok()&&(await r.body()).equals(s.bytes),'Retained exact dated report source bytes');}}
  async function audit(){const v=await get(prefix+'audit-verify');need(v.audit?.available&&v.audit.ok&&v.audit.entries===4&&v.audit.components.length===5&&v.audit.components.every((c:any)=>c.available&&c.ok&&c.errors.length===0),'Complete four-entry accounting/source audit');return v;}
  async function projections(){const out=[];for(const c of cases){const v=await report(c);need(v.ok===true&&Array.isArray(v.appliedRules)&&v.errors.length===0&&!('data' in v),'Original flat successful report');need(isDeepStrictEqual(cli(c),v),'Exact original CLI/HTTP numeric report '+c.kind);out.push({...c,report:v});}
    const [trial,pnl,balance,after,day]=out.map(v=>v.report);
    need(trial.totalDebit===100&&trial.totalCredit===100&&trial.linesConsidered===4&&trial.journalEntryCount===2,'Exact inclusive period debit/credit/counts');
    need(pnl.totalIncome===80&&pnl.totalExpense===20&&pnl.result===60,'Exact period P&L');
    need(balance.totalAssets===160&&balance.equity.total===100&&balance.periodResult===60&&balance.totalLiabilitiesAndEquity===160&&balance.balanced,'Original cumulative equity excludes separate result');
    need(after.totalAssets===80&&after.equity.total===100&&after.periodResult===-20&&after.balanced&&day.result===-80,'Reversal respects its date and cumulative result');
    const finalTrial=out[5].report;need(finalTrial.accounts.find((a:any)=>a.accountNo==='3731')?.balance===0&&finalTrial.journalEntryCount===4&&finalTrial.linesConsidered===8,'Touched zero and cross-year counts retained');
    for(const [i,title] of [[0,'Saldobalance'],[1,'Resultatopgørelse'],[2,'Balance']] as const)need(cli(cases[i],true).includes(title),'Original Danish human report heading');
    await originals();await unchanged();return out;
  }
  return {
    async run(){need(!completed,'Dated fixture executes once');await cookie(page,ownerFile);await cookie(member,memberFile);
      const today=(await get('/api/portfolio')).portfolio.asOf;need(/^\d{4}-\d{2}-\d{2}$/.test(today),'Trusted current date');
      const shift=(days:number)=>new Date(Date.parse(today+'T12:00:00Z')+days*86400000).toISOString().slice(0,10),cutoff=shift(-2),reversalDate=shift(-1),first=String(Number(today.slice(0,4))-1)+'-12-31';
      for(const [accountNo,type,normalBalance] of [['1731','asset','debit'],['8731','equity','credit'],['3731','income','credit'],['4731','expense','debit']])await post(prefix+'accounts',{accountNo,name:'Synthetic dated '+accountNo,type,normalBalance});
      const version=(await get(prefix+'journal')).journal.nativePosting.ruleVersion,receipts=[];
      for(const [date,amount,debit,credit,key] of [[first,'100.00','1731','8731','capital'],[cutoff,'80.00','1731','3731','income'],[cutoff,'20.00','4731','1731','expense']]){
        let documentId: string|undefined;
        if(key!=='capital'){const bytes=Buffer.from('Synthetic dated '+key+' '+amount+' DKK\n');const uploaded=await post(prefix+'documents/ingest',{confirm:true,fileName:'synthetic-dated-'+key+'.txt',fileBase64:bytes.toString('base64'),metadata:{source:'Synthetic dated '+key,documentType:'cash_register_receipt',issueDate:date}});documentId=String(uploaded.document.id);sourceFiles.push({id:uploaded.document.id,bytes});}
        receipts.push(await post(prefix+'manual-journal',{confirm:true,input:{format:'rentemester-manual-journal-post/1',expectedRuleVersion:version,idempotencyKey:'synthetic-dated132-'+key,request:{transactionDate:date,text:'Synthetic dated '+key,currency:'DKK',...(documentId?{documentId}:{}),lines:[{accountNo:debit,debitAmount:amount,creditAmount:'0.00'},{accountNo:credit,debitAmount:'0.00',creditAmount:amount}]}}}));
      }
      const reversal=await post(prefix+'manual-journal-reversal',{confirm:true,input:{format:'rentemester-manual-journal-reversal/1',expectedRuleVersion:version,idempotencyKey:'synthetic-dated132-reversal',request:{entryId:String(receipts[1].entryId),transactionDate:reversalDate,reason:'Synthetic dated income correction'}}});
      cases=[{kind:'trial-balance',query:{from:cutoff,to:cutoff}},{kind:'profit-loss',query:{from:cutoff,to:cutoff}},{kind:'balance',query:{asOf:cutoff}},{kind:'balance',query:{asOf:reversalDate}},{kind:'profit-loss',query:{from:reversalDate,to:reversalDate}},{kind:'trial-balance',query:{from:first,to:reversalDate}}];
      stable=await books();saved=await projections();
      const failures:Query[]=[{kind:'trial-balance',query:{from:'2026-02-30',to:cutoff}},{kind:'profit-loss',query:{from:reversalDate,to:cutoff}},{kind:'balance',query:{asOf:'not-a-date'}},{kind:'balance',query:{asOf:' '+cutoff+' '}}];
      const businessFailures=[];for(const c of failures){const v=await report(c);need(v.ok===false&&v.errors.length>0&&!('data' in v),'Original full date business refusal');need(isDeepStrictEqual(cli(c,false,ownerFile,1),v),'Original JSON business refusal/exit1');cli(c,true,ownerFile,1);businessFailures.push({...c,report:v});}
      for(const c of cases.slice(0,3))cli(c,false,ownerFile,2,true);
      for(const raw of ['','from=a','from=a&to=b&from=c','from=a&to=b&year=2026','from=a&to=b&asOf=c','from=a&to=b&source=archive'])need((await page.request.get(base+prefix+'reports/trial-balance?'+raw)).status()===400,'Structural selector refusal');
      need(isDeepStrictEqual(await report(cases[0],member),saved[0].report),'Current invited reader exact report');
      const memberId=(await get('/api/me',member)).user.id;await post('/api/workspace/members/company',{userId:memberId,action:'revoke',companySlug:'synthetic'});
      for(const c of cases.slice(0,3)){need((await member.request.get(base+prefix+'reports/'+c.kind+'?'+new URLSearchParams(c.query))).status()===403,'Current revoked reader refused');cli(c,false,memberFile,1);}
      await unchanged();const fullAudit=await audit();await unchanged();completed=true;
      return {qualified:true,ordinaryMfa:true,originalThreeCliQualified:true,readOnly:true,currentRevocationRefused:true,projections:saved,businessFailures,fullAudit,accountingFingerprint:hash(stable),receipts,reversal,sourceOriginals:sourceFiles.map(s=>({id:s.id,sha256:createHash('sha256').update(s.bytes).digest('hex')})),sourceFreeRestoreQualified:false,installedNativeQualified:false};
    },
    async reopen(){need(completed,'Completed dated fixture required');await cookie(page,ownerFile);await unchanged();const projectionsAfter=await projections();need(isDeepStrictEqual(projectionsAfter,saved),'Exact original reports after normal MFA restart');const fullAudit=await audit();await unchanged();unlinkSync(ownerFile);unlinkSync(memberFile);return {qualified:true,ordinaryMfa:true,exactNormalMfaRestartState:true,projections:projectionsAfter,fullAudit,accountingFingerprint:hash(stable),sourceFreeRestoreQualified:false,installedNativeQualified:false};},
  };
}
