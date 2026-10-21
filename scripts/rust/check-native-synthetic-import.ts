import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync, unlinkSync } from 'node:fs';
import { join } from 'node:path';
import { isDeepStrictEqual } from 'node:util';
import type { Page } from '@playwright/test';

/** Original J02: exact source CSV -> shared opening -> first non-VAT expense -> honest close blocker. */
export function createSyntheticImportProof(page: Page, base: string, work: string, memberPage: Page) {
 const company='synthetic-csv-j02',prefix='/api/companies/'+company+'/';
 const need=(v:unknown,m:string)=>{if(!v)throw Error(m);};
 const hash=(v:Buffer|string)=>createHash('sha256').update(v).digest('hex');
 const fingerprint=(v:unknown)=>hash(JSON.stringify(v));
 const content='\uFEFF# cutOverDate: 2025-07-01\r\n# note: Synthetic shifted non-VAT original opening\r\nsection,accountNo,name,debit,credit\r\naccount,1717,Synthetic imported bank,,\r\naccount,1177,Synthetic historical debtors,,\r\naccount,8777,Synthetic opening equity,,\r\nopening,1717,,150000,\r\nopening,1177,,40000,\r\nopening,8777,,,190000\r\n';
 const source={sourceSystem:'synthetic-csv',content},csvPath=join(work,'synthetic-original127.csv'),cookiePath=join(work,'synthetic-import-owner.cookie');
 const original=Buffer.from('Synthetic first expense original127\nSupplies DKK250 including declared source VAT50. Owner is not VAT registered.\n');
 let stable:any,fullAudit:any,done=false,accepted=false;
 async function cookies(fresh:boolean){const all=await page.context().cookies(base);need(all.some(c=>c.httpOnly),'Normal MFA owner cookie');writeFileSync(cookiePath,all.map(c=>`${c.name}=${c.value}`).join('; ')+'\n',{mode:0o600,flag:fresh?'wx':'w'});}
 async function get(resource:string){const r=await page.request.get(base+prefix+resource);need(r.ok(),`Synthetic GET ${resource} HTTP${r.status()}`);return r.json();}
 async function post(resource:string,data:any,who=page,global=false){const r=await who.request.post(base+(global?'':prefix)+resource,{headers:{origin:base},data});const v=await r.json();need(r.ok()&&v.ok===true,`Synthetic POST ${resource} HTTP${r.status()}: ${JSON.stringify(v)}`);return v;}
 function client(kind:'cli'|'mcp',operation:string,body?:unknown,apply=false,file=false){
  const common=['--server',base,'--cookie-file',cookiePath,'--allow-loopback-http'];
  const args=kind==='cli'?['remote',operation,...common,'--company',company,...(file?['--file',csvPath,...(apply?['--plan-hash',(body as any).planHash,'--idempotency-key',(body as any).idempotencyKey]:[])]:[]),...(apply?['--confirm']:[])]:common;
  const input=kind==='cli'?file||body===undefined?'':JSON.stringify(body):[
   {jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'synthetic-owner127',version:'1'}}},
   {jsonrpc:'2.0',method:'notifications/initialized'},
   {jsonrpc:'2.0',id:2,method:'tools/call',params:{name:operation,arguments:{company,...(body===undefined?{}:{body:JSON.stringify(body)}),...(apply?{confirm:true}:{})}}},
  ].map(v=>JSON.stringify(v)).join('\n')+'\n';
  const r=spawnSync(join(work,kind==='cli'?'rentemester-rust':'rentemester-mcp'),args,{input,encoding:'utf8',timeout:45000,maxBuffer:8*1024*1024});
  need(!r.error&&!r.stdout.includes(readFileSync(cookiePath,'utf8').trim())&&!r.stderr.includes(readFileSync(cookiePath,'utf8').trim()),'Client timeout/private cookie escaped');
  if(kind==='cli'){const v=JSON.parse(r.stdout);need(r.status===0&&v.ok===true&&v.data?.ok===true,`CLI ${operation} refused: ${r.stdout}`);return v.data;}
  need(r.status===0,'MCP terminal status');const reply=r.stdout.trim().split('\n').map(v=>JSON.parse(v)).find(v=>v.id===2);
  need(!reply?.error&&reply?.result?.isError===false&&reply.result.structuredContent?.data?.ok===true,`MCP ${operation} refused: ${JSON.stringify(reply)}`);return reply.result.structuredContent.data;
 }
 async function snapshot(){const year=accepted?'?year=2026&source=live':'';return {profile:await get('company'),accounts:await get('accounts'),opening:await get('opening-balance'),journal:await get('journal'+year),trial:await get('trial-balance'+year),income:await get('income-statement'+year),balance:await get('balance'+year),bank:await get('bank'),documents:await get('documents'),audit:await get('audit-log?limit=5000')};}
 async function originalRead(id:number){const r=await page.request.get(base+prefix+'documents/'+id+'/original');need(r.ok()&&isDeepStrictEqual(await r.body(),original),'Retained expense original differs');}
 return {async run(){
  await cookies(true);writeFileSync(csvPath,content,{mode:0o600,flag:'wx'});
  await post('/api/companies',{name:'Synthetic imported owner 127',slug:company,fiscalYearStartMonth:'7',fiscalYearLabelStrategy:'end-year',vatPeriodType:null},page,true);
  const profile=await get('company');need(profile.company.fiscalYearStartMonth===7&&profile.company.fiscalYearLabelStrategy==='end-year'&&profile.company.vatPeriodType===null,'Explicit shifted non-VAT company');
  const accounts=[['1717','Old synthetic bank','asset','debit'],['1177','Old synthetic debtors','asset','debit'],['8777','Old synthetic equity','equity','credit'],['3777','Synthetic first supplies','expense','debit'],['2777','Synthetic creditors','liability','credit']];
  for(const [accountNo,name,type,normalBalance]of accounts)await post('accounts',{accountNo,name,type,normalBalance});
  for(const [role,accountNo]of [['bank','1717'],['debtors','1177'],['creditors','2777'],['operational_default','3777']])await post('account-roles',{role,accountNo,confirm:true});
  const before=await snapshot();const planned=client('cli','opening-balance-plan',undefined,false,true);need(planned.plan.status==='ready'&&planned.plan.opening.totals.debit==='1900.00','Exact integer ore to reviewed DKK opening');
  need(isDeepStrictEqual(await snapshot(),before),'Read-only source plan changed company');
  const mcpPlan=client('mcp','opening_balance_plan',source);need(isDeepStrictEqual(mcpPlan,planned),'CLI/MCP source plan parity');
  const body={input:source,planHash:planned.plan.planHash,idempotencyKey:'synthetic-csv-j02-original127',confirm:true};
  const denied=await memberPage.request.post(base+prefix+'opening-balance/apply',{headers:{origin:base},data:body});need(denied.status()===403,'Reviewer source apply refused');need(isDeepStrictEqual(await snapshot(),before),'Denied source apply changed company');
  const receipt=client('cli','opening-balance-apply',body,true,true);need(receipt.openingBalance.sourceBasis.kind==='synthetic_csv','Retained explicit source kind');accepted=true;
  const once=await snapshot();const sourceEvent=once.audit.rows.find((r:any)=>r.eventType==='native_opening_balance');need(sourceEvent&&JSON.parse(sourceEvent.message).binding.syntheticSource.content===content,'Exact BOM/CRLF original retained in immutable opening audit');const replay=client('mcp','opening_balance_apply',body,true);need(replay.idempotency.replayed===true,'Deliberate exact-key replay');
  const afterReplay=await snapshot();need(isDeepStrictEqual({...afterReplay,audit:once.audit},once),'Replay changed financial effects');need(afterReplay.audit.total===once.audit.total+1,'Replay adds one audit event');
  const duplicate=await page.request.post(base+prefix+'opening-balance/apply',{headers:{origin:base},data:{...body,idempotencyKey:'synthetic-csv-j02-new-key127'}});need(duplicate.status()===422,'New key cannot duplicate opening');
  need(isDeepStrictEqual(await snapshot(),afterReplay),'Rejected duplicate changed state');
  need(isDeepStrictEqual(client('mcp','opening_balance'),afterReplay.opening),'Canonical MCP opening read');
  await page.goto(base+'/companies/'+company+'/manage');await page.getByRole('region',{name:'Bogført åbningsbalance',exact:true}).waitFor();await page.getByText('Åbningsbalancens dokumentation',{exact:true}).click();await page.getByText('CSV-kilden er bevaret i åbningsposteringens auditdokumentation.',{exact:true}).waitFor();need(await page.getByLabel('Skæringsdato',{exact:true}).count()===0,'Existing source opening cannot be re-entered in web form');
  await post('bank-accounts',{name:'Synthetic imported bank',slug:'synthetic-j02-bank',currency:'DKK',ledgerAccountNo:'1717'});
  await post('bank/import',{confirm:true,account:'synthetic-j02-bank',csvContent:'dato;tekst;beløb;valuta;saldo\n2025-07-02;Synthetic first imported expense;-250,00;DKK;1250,00\n'});
  await post('documents/ingest',{confirm:true,fileName:'synthetic-j02-receipt.txt',fileBase64:original.toString('base64'),metadata:{source:'Synthetic owner first expense',documentType:'cash_register_receipt',issueDate:'2025-07-02',amountIncVat:250,vatAmount:50,currency:'DKK',deliveryDescription:'Synthetic supplies',sender:{name:'Synthetic Supplier',address:'Synthetic Street',vatOrCvr:'DK12345678'}}});
  const document=(await get('documents')).documents.documents.find((d:any)=>d.filename==='synthetic-j02-receipt.txt');need(document,'Expense original registered');
  const bank=(await get('documents/'+document.id+'/booking-options')).options.unmatchedOutgoingBank.find((b:any)=>b.text==='Synthetic first imported expense');need(bank,'Actual imported outgoing bank');
  const expense={confirm:true,documentId:document.id,bankTransactionId:bank.id,expenseAccountNo:'3777',paymentAccountNo:'1717',transactionDate:'2025-07-02'};
  const beforeExpense=await snapshot();const vat=await page.request.post(base+prefix+'documents/book-expense',{headers:{origin:base},data:{...expense,vatTreatment:'standard'}});need(vat.status()===422&&(await vat.text()).includes('not VAT registered'),'No invented deductible VAT');need(isDeepStrictEqual(await snapshot(),beforeExpense),'Refused VAT changed state');
  await page.goto(base+'/companies/'+company+'/bilag');await page.getByRole('row').filter({hasText:document.documentNo}).getByRole('link',{name:'Bogfør bilag',exact:true}).click();
  await page.getByLabel(/^Udgiftskonto/).selectOption('3777');await page.getByLabel(/^Banktransaktion \(uafstemt, udgående\)/).selectOption(String(bank.id));await page.getByLabel(/^Moms-behandling/).selectOption('non_deductible');
  await page.getByRole('button',{name:'Bogfør',exact:true}).click();await page.getByText(/Bilaget blev bogført som journalpost/).waitFor();
  const posted=await snapshot();need(posted.journal.journal.entries.length===2,'Exactly opening and first expense');need(posted.income.incomeStatement.result===-250&&posted.income.incomeStatement.totalExpense===250,'Full non-deductible cost in shifted fiscal report');
  const expenseEntry=posted.journal.journal.entries.find((e:any)=>e.documentId===document.id);need(expenseEntry?.lines.length===2&&expenseEntry.total===250,'No VAT journal lines');need(posted.bank.bank.bookedBalance===1250&&posted.bank.bank.actualBalance===1250&&posted.bank.bank.difference===0,'Independent closing statement reconciles with imported opening');
  const readiness=await get('periods/close-readiness?from=2025-07-01&to=2026-06-30');const control=readiness.packet.items.find((c:any)=>c.code==='DKK_CONTROL_ACCOUNTS');need(control?.status==='blocked'&&control.waivable===false,'Unresolved historical debtors correctly block closure');
  const review=await post('periods/close-review',{periodStart:'2025-07-01',periodEnd:'2026-06-30',confirm:true});
  const close=await page.request.post(base+prefix+'periods/close',{headers:{origin:base},data:{periodStart:'2025-07-01',periodEnd:'2026-06-30',kind:'fiscal_year',packetHash:review.review.packet.hash,reviewId:review.review.id,confirm:true}});need(close.status()===422&&(await close.text()).includes('PERIOD_CLOSE_BLOCKED'),'Historical evidence blocker cannot be bypassed');
  await originalRead(document.id);fullAudit=await get('audit-verify');need(fullAudit.ok===true&&fullAudit.audit?.available===true&&fullAudit.audit.ok===true,'Complete accounting audit');
  stable=await snapshot();const exported=await page.request.post(base+'/api/workspace/backup',{headers:{origin:base},data:{confirm:true}});need(exported.ok()&&exported.headers()['content-type']?.split(';')[0]==='application/vnd.rentemester.native-backup+json','Signed source backup');const bytes=await exported.body();writeFileSync(join(work,'synthetic-source.native-backup.json'),bytes,{mode:0o600,flag:'wx'});
  // Backup creates workspace evidence, not another company financial effect.
  need(isDeepStrictEqual(await snapshot(),stable),'Backup changed company state');done=true;
  return {qualified:true,company,receipt,sourceSha256:hash(content),rawUtf8BomCrlfRetained:true,cliFilePlanApplyQualified:true,mcpPlanReadReplayQualified:true,firstExpenseWebQualified:true,nonVatShiftedYearQualified:true,unresolvedHistoricalEvidenceBlocked:true,readiness,fullAudit,accountingFingerprint:fingerprint(stable),expectedTerminalCounts:{documents:1,bank_transactions:1,journal_entries:2},signedBackup:{sha256:hash(bytes),bytes:bytes.length,independentlyVerified:false},sourceFreeRestoreQualified:false,installedNativeQualified:false};
 },async reopen(){need(done,'Completed source journey');await cookies(false);need(isDeepStrictEqual(await snapshot(),stable),'Exact normal-MFA CSV restart state');need(isDeepStrictEqual(await get('audit-verify'),fullAudit),'Restart audit');const doc=stable.documents.documents.documents.find((d:any)=>d.filename==='synthetic-j02-receipt.txt');await originalRead(doc.id);unlinkSync(cookiePath);return {qualified:true,exactNormalMfaRestartState:true,accountingFingerprint:fingerprint(stable),exactExpenseOriginalAfterRestart:true};}};
}
