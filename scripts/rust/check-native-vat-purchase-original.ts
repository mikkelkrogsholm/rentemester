import type { Page } from '@playwright/test';
import { spawnSync } from 'node:child_process';
import { createHash, randomUUID } from 'node:crypto';
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { isDeepStrictEqual as equal } from 'node:util';

/** Independent facts frozen before setup; actual never derives its oracle from receipts.
 * No bank synthesis / SQL seed / implicit VIES. Existing private verified TLS gateway
 * and expense preflight apply are the only provider path. FX and configured omission
 * mapping are narrow domain/transport fixtures, not another network fixture here.
 */
export const originalVatPurchaseExpected = Object.freeze({ documents:3, journalEntries:2, journalLines:8, bankTransactions:1, bankReconciliations:1, scannerRequests:3, providerRequests:1, resolvedExceptions:1, euNoBank:true, euExpense:100, euInputVat:25, euReverseChargeVat:25, representationExpense:118.75, representationInputVat:6.25, representationPayment:125 });
export function createOriginalVatPurchaseProof(base:string,work:string,onStep:(s:string)=>void) {
  const root='/api/companies/synthetic/', api=root+'original/vat/', cookie=join(work,'vat174.cookie'), key=join(work,'vat174.key');
  const observations:any[]=[], originals:any[]=[], receipts:any[]=[];
  let owner:Page, service:any, stable:any, today:string, ran=false;
  const hash=(v:Buffer|string)=>createHash('sha256').update(v).digest('hex');
  const need=(v:unknown,s:string)=>{if(!v)throw Error('original_vat_purchase: '+s);};
  function record(v:any){observations.push(v);writeFileSync(join(work,'vat174-observations.json'),JSON.stringify(observations),{mode:0o600});}
  async function http(path:string,data?:any,status=200,page=owner,method='POST') {
    onStep((data===undefined?'GET':method)+':'+path);
    const r=data===undefined?await page.request.get(base+path):await page.request.fetch(base+path,{method,headers:{origin:base},data});
    const value=await r.json();record({path,status:r.status(),value:path==='/api/workspace/service-principals'?{...value,secret:'[redacted]'}:value});
    need(r.status()===status,path+' expected '+status+' got '+r.status());return value;
  }
  const admin=(path:string,data:any)=>http('/api/workspace/'+path,data,path==='service-principals'?201:200);
  async function cookies(){writeFileSync(cookie,(await owner.context().cookies(base)).map(c=>c.name+'='+c.value).join('; '),{mode:0o600});}
  function terminal(kind:'cli'|'mcp',argv:string[],input?:string,asService=false,exit=0){
    const r=spawnSync(join(work,kind==='cli'?'rentemester-rust':'rentemester-mcp'),argv,{input,encoding:'utf8',timeout:45000,maxBuffer:16*1024*1024});
    const secret=readFileSync(asService?key:cookie,'utf8').trim();
    need(!r.stdout.includes(secret)&&!r.stderr.includes(secret),'credential privacy');
    record({kind,argv,exit:r.status,stdout:r.stdout,stderr:r.stderr,terminalError:r.error?.name});
    need(!r.error&&r.status===exit,'exact bounded terminal; unknown write never retried');return r;
  }
  const common=(asService=false)=>['--server',base,'--allow-loopback-http',asService?'--service-key-file':'--cookie-file',asService?key:cookie];
  function cli(op:string,payload:any,exit=0,extra:string[]=[]){
    onStep('cli:'+op);const flags:string[]=[];
    if(payload!==undefined){const file=join(work,'vat174-input-'+randomUUID()+'.json');writeFileSync(file,JSON.stringify(payload),{mode:0o600,flag:'wx'});flags.push('--input',file);}
    const r=terminal('cli',['vat',op,...common(),'--company','synthetic','--json',...flags,...extra],undefined,false,exit);
    if(exit===2)return r;const v=JSON.parse(r.stdout);need(v.ok===(exit===0),'flat source CLI outcome');return v;
  }
  function mcp(op:string,payload:any,asService=false,ok=true,confirm?:boolean){
    onStep('mcp:'+op);const input=[{jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'original-vat174',version:'1'}}},{jsonrpc:'2.0',method:'notifications/initialized'},{jsonrpc:'2.0',id:2,method:'tools/call',params:{name:'vat_'+op.replaceAll('-','_'),arguments:{company:'synthetic',payload,...(confirm===undefined?{}:{confirm})}}}].map(v=>JSON.stringify(v)).join('\n')+'\n';
    const r=terminal('mcp',common(asService),input,asService);
    const reply=r.stdout.trim().split('\n').map(s=>JSON.parse(s)).find(v=>v.id===2), result=reply?.result,v=result?.structuredContent;
    need(!reply?.error&&result.content.length===1&&equal(JSON.parse(result.content[0].text),v),'MCP exact text/structured');
    need(v.ok===ok&&result.isError===!ok&&Array.isArray(v.errors),'MCP source outcome');
    if(!ok){need(!Object.hasOwn(v,'data')&&v.errors.length>0,'ALL false omit success data');return v;}
    return {ok:true,...v.data,errors:v.errors,...(v.appliedRules?{appliedRules:v.appliedRules}:{})};
  }
  function receipt(v:any,rule:string){
    need(equal(Object.keys(v).sort(),['ok','entryId','entryNo','entryHash','appliedRules','errors'].sort()),'only source receipt fields');
    need(v.ok&&Number.isSafeInteger(v.entryId)&&v.entryId>0&&typeof v.entryNo==='string'&&v.entryNo.trim()&&/^[a-f0-9]{64}$/.test(v.entryHash)&&equal(v.errors,[])&&v.appliedRules.includes(rule),'real immutable source receipt');receipts.push(v);
  }
  function trace(){const file=join(work,'vat174-provider-trace.json');return existsSync(file)?JSON.parse(readFileSync(file,'utf8')):[];}
  async function snapshot(){const v:any={};for(const p of ['journal','trial-balance','bank','vat','documents'])v[p]=await http(root+p+(['journal','trial-balance','bank','vat'].includes(p)?'?year='+today.slice(0,4):''));return v;}
  async function sources(){for(const d of originals){const r=await owner.request.get(base+root+`documents/${d.id}/file`);need(r.status()===200&&hash(await r.body())===d.sha256,'immutable scanned bytes');}}
  async function ingest(name:string,sender:any,gross:number,vat:number,invoiceNo:string,structured=false){
    const bytes=Buffer.from(`Synthetic VAT174 ${name} original\nGross ${gross} VAT ${vat} DKK\n`);
    const metadata:any={source:'Synthetic VAT174 ordinary intake',documentType:'purchase_sale',issueDate:today,invoiceNo,deliveryDescription:'Synthetic purchased services',amountIncVat:gross,vatAmount:vat,currency:'DKK',sender:{name:'Synthetic supplier '+name,address:'Synthetic Supplier Street 174',...sender},recipient:{name:'Synthetic Batch Company',address:'Synthetic VAT Street 174',vatOrCvr:'DK87654321'}};
    if(structured)metadata.purchaseVatLines=[{classification:'exempt',netAmount:100,vatAmount:0}];
    const v=await http(root+'documents/ingest',{confirm:true,fileName:'synthetic-vat174-'+name+'.txt',fileBase64:bytes.toString('base64'),metadata});
    need(v.ok&&Number.isSafeInteger(v.document.id),'ordinary original registration');const d={id:v.document.id,sha256:hash(bytes)};originals.push(d);return d;
  }
  function exactLines(entry:any,expected:any[]){
    const lines=entry.lines.map((l:any)=>[l.accountNo,l.debit,l.credit]);
    need(equal(lines,expected.map(v=>v.slice(0,3))),'independent exact account allocation and native writer line order; VAT codes verified in read-only inventory');
  }
  async function run(page:Page,member:Page){
    need(!ran,'one finite actual; no write replay');ran=true;owner=page;record({expected:originalVatPurchaseExpected,representationOmission:'unique active explicit default_vat_code=REPRESENTATION_SPECIAL mapping only; no fixed chart fallback',historicalImportParityQualified:false,installedNativeQualified:false});await cookies();
    const who=await http('/api/me');today=(await http(root+'scheduled-recurring')).scheduledRecurring.asOfDate;
    await http(root+'company',{cvr:'87654321',address:'Synthetic VAT Street 174',postalCode:'1000',city:'Synthetic City',vatPeriodType:'quarter',paymentTermsDays:14},200,owner,'PATCH');
    for(const [accountNo,type,normalBalance]of [['1800','asset','debit'],['3000','expense','debit'],['3100','expense','debit'],['4700','vat','debit'],['4710','vat','credit']])await http(root+'accounts',{accountNo,name:'Synthetic VAT174 '+accountNo,type,normalBalance});
    for(const [role,accountNo]of [['bank','1800'],['input_vat','4700'],['reverse_charge_vat','4710']])await http(root+'account-roles',{role,accountNo,confirm:true});
    await http(root+'bank-accounts',{slug:'synthetic-vat174',name:'Synthetic VAT174 bank',currency:'DKK',ledgerAccountNo:'1800'});
    // Same invoiceNo, newer EU ID: selector must override the supplied older DK ID.
    const dk=await ingest('dk',{vatOrCvr:'DK12345678',countryCode:'DK',identifierKind:'dk_cvr'},125,25,'SYN-VAT174-SHARED');
    const eu=await ingest('eu',{vatOrCvr:'DE168000001',countryCode:'DE',identifierKind:'eu_vat'},100,0,'SYN-VAT174-SHARED');
    const noneu=await ingest('noneu',{vatOrCvr:'SYN-US-174',countryCode:'US',identifierKind:'non_eu'},100,0,'SYN-VAT174-NONEU',true);
    const csv=`dato;tekst;beløb;valuta;saldo\n${today};SYN-VAT174-SHARED;-125,00;DKK;-125,00\n`;
    await http(root+'bank/import',{confirm:true,csvContent:csv,account:'synthetic-vat174'});
    const bank=(await http(root+'bank?year='+today.slice(0,4))).bank.transactions;need(bank.length===1&&bank[0].amount===-125,'one real ordinary bank row');
    service=await admin('service-principals',{displayName:'Synthetic VAT174',operationId:randomUUID(),confirm:true});writeFileSync(key,service.secret+'\n',{mode:0o600,flag:'wx'});
    await admin('members/access',{userId:service.serviceAccountId,action:'set-role',workspaceRole:'member'});await admin('members/company',{userId:service.serviceAccountId,companySlug:'synthetic',action:'grant',role:'bookkeeper'});
    const euInput={transactionDate:today,text:'Synthetic EU service174',documentId:eu.id,netAmount:100,expenseAccountNo:'3000'};
    const repInput={transactionDate:today,text:'Synthetic representation174',documentId:dk.id,netAmount:100,expenseAccountNo:'3100',sourceBankTransactionId:bank[0].id};
    const before=await snapshot();
    const missing=cli('post-eu-service-purchase',undefined,2);need(missing.stderr.trim()==='Missing required --input <file.json>','exact missing-input usage');
    const unresolved={...euInput,invoiceNo:'  SYN-VAT174-MISSING  '};
    const usage=cli('post-eu-service-purchase',unresolved,2);need(usage.stderr.trim()==='Could not resolve document for invoiceNo SYN-VAT174-MISSING','trimmed exact unresolved selector CLI usage');
    const unresolvedMcp=mcp('post-eu-service-purchase',unresolved,false,false,true);need(equal(unresolvedMcp.errors,['Could not resolve document for invoiceNo SYN-VAT174-MISSING'])&&!Object.hasOwn(unresolvedMcp,'code'),'unresolved selector businessfalse without invented code');
    cli('post-eu-service-purchase',euInput,1); // No cached VIES; never auto-call.
    for(const documentId of [dk.id,noneu.id])cli('post-eu-service-purchase',{...euInput,documentId},1);
    const structured=await http(api+'representation-purchase',{payload:{...repInput,documentId:noneu.id},confirm:true},422);need(!structured.ok&&structured.errors.some((e:string)=>/structured|purchaseVatLines/i.test(e)),'structured VAT original definite refusal');
    for(const confirm of [undefined,false]){const v=mcp('post-representation-purchase',repInput,true,false,confirm);need(v.code==='CONFIRM_REQUIRED'&&equal(v.errors,['confirm: true required for write tool vat_post_representation_purchase']),'exact local confirmation refusal');}
    await http(api+'eu-service-purchase',{payload:euInput,confirm:true,actor:'user:forged'},403);
    await http(api+'eu-service-purchase',{payload:{...euInput,actor:'user:forged'},confirm:true},400);
    const memberId=(await http('/api/me',undefined,200,member)).user.id;
    await admin('members/company',{userId:memberId,companySlug:'synthetic',action:'grant',role:'reader'});
    await http(api+'eu-service-purchase',{payload:euInput,confirm:true},403,member);
    await admin('members/company',{userId:memberId,companySlug:'synthetic',action:'grant',role:'bookkeeper'});
    await admin('members/company',{userId:service.serviceAccountId,companySlug:'synthetic',action:'grant',role:'reader'});mcp('post-representation-purchase',repInput,true,false,true);
    await admin('members/company',{userId:service.serviceAccountId,companySlug:'synthetic',action:'grant',role:'bookkeeper'});
    need(trace().length===0&&equal(await snapshot(),before),'all preflight/selector/auth/region refusals no books/provider effects');
    // Exactly one explicit EXISTING source expense preflight apply, not a new provider harness.
    onStep('explicit-existing-expense-vat-preflight-apply');
    const preflight=terminal('cli',['expense','vat-preflight',...common(),'--company','synthetic','--json','--document-id',String(eu.id),'--apply','yes']);
    const cache=JSON.parse(preflight.stdout);need(cache.ok&&cache.cached&&trace().length===1&&equal(await snapshot(),before),'explicit cache population only, no journal effects');
    receipt(cli('post-eu-service-purchase',{...euInput,documentId:dk.id,invoiceNo:'  SYN-VAT174-SHARED  '}),'DK-VAT-REVERSE-CHARGE-001');
    receipt(mcp('post-representation-purchase',repInput,true,true,true),'DK-VAT-REPRESENTATION-001');
    stable=await snapshot();const entries=stable.journal.journal.entries;
    need(entries.length===2,'two journals only');const euEntry=entries.find((e:any)=>e.documentId===eu.id),repEntry=entries.find((e:any)=>e.documentId===dk.id);
    need(euEntry&&repEntry,'latest exact invoice selector overrides older document');
    exactLines(euEntry,[['3000',100,0,'EU_SERVICE_REVERSE_CHARGE'],['4700',25,0,null],['1800',0,100,null],['4710',0,25,null]]);
    exactLines(repEntry,[['3100',100,0,'REPRESENTATION_SPECIAL'],['3100',18.75,0,'REPRESENTATION_NON_DEDUCTIBLE_VAT'],['4700',6.25,0,null],['1800',0,125,null]]);
    need(stable.vat.vat.inputVat===31.25&&stable.vat.vat.payable===-6.25,'independent combined input/reverse VAT');
    need(stable.bank.bank.matchedCount===1&&stable.bank.bank.unmatchedCount===0,'optional real bank linked, no synthetic EU row');
    cli('post-eu-service-purchase',euInput,1);mcp('post-representation-purchase',repInput,true,false,true);
    await admin('service-principals/revoke',{serviceAccountId:service.serviceAccountId,credentialId:service.credentialId,operationId:randomUUID(),confirm:true});mcp('post-representation-purchase',repInput,true,false,true);
    need(equal(await snapshot(),stable)&&trace().length===1,'duplicate / revoked key definite no-effect no-provider');await sources();
    return {expected:originalVatPurchaseExpected,receipts,originals,csvSha256:hash(csv),humanActor:'user:'+who.user.id,serviceActor:'service_account:'+service.serviceAccountId,noBankEu:true,explicitRepresentationAccount:'3100',historicalImportParityQualified:false,installedNativeQualified:false};
  }
  async function assertStable(page:Page){owner=page;need(equal(await snapshot(),stable),'exact restart / signed source-free fresh-owner recovery / reopen');await sources();need(trace().length===1,'no hidden provider calls');}
  async function reopen(page:Page){await assertStable(page);await cookies();}
  return {run,assertStable,reopen};
}
