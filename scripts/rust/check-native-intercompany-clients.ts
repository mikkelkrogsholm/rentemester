import {spawnSync} from 'node:child_process';
import {mkdtempSync,mkdirSync,readFileSync,rmSync,writeFileSync} from 'node:fs';
import {join} from 'node:path';
import {isDeepStrictEqual} from 'node:util';
import type {Page} from '@playwright/test';
const evidencePath='/tmp/rentemester-rust-reports/native-intercompany-clients.json';
export type IntercompanyClientFixture={
 manifest:{version:1;groups:Array<{id:string;name:string;memberships:Array<{id:string;companySlug:string;validFrom:string;validToExclusive?:string}>;ownership:unknown[]}>};
 mapping:{id:string;groupId:string;leftCompanySlug:string;rightCompanySlug:string;leftAccountNos:string[];rightAccountNos:string[];leftPosition:'receivable'|'payable';rightPosition:'receivable'|'payable';evidenceRefs:string[];validFrom:string;validToExclusive?:string};
 asOf:string;expectedBalance:string;
};
/** Fresh workspace/profile separate from browser helper: manifest is GLOBAL. Caller prepares explicit real accounts/journals through ordinary authenticated APIs, never SQL/seeds. */
export async function checkNativeIntercompanyClients(author:Page,reviewer:Page,base:string,anchor:string,other:string,fixture:IntercompanyClientFixture,restarted=false){
 if(!/^http:\/\/127\.0\.0\.1:\d+$/.test(base)||anchor===other||![anchor,other].every(s=>/^synthetic-intercompany-[a-z0-9-]+$/.test(s))||fixture.mapping.leftCompanySlug!==anchor||fixture.mapping.rightCompanySlug!==other||!/^\d{4}-\d{2}-\d{2}$/.test(fixture.asOf)||!/^\d+\.\d{2}$/.test(fixture.expectedBalance))throw Error('Explicit dedicated synthetic intercompany pair/source/date/decimal fixture required');
 const cliBinary=process.env.RM_NATIVE_CLI_BIN,mcpBinary=process.env.RM_NATIVE_MCP_BIN;if(!cliBinary||!mcpBinary)throw Error('Explicit qualified CLI/MCP artifacts required');
 const path=`/api/companies/${anchor}/intercompany`,reportPath=`/api/group-reconciliation?asOf=${fixture.asOf}`;
 const get=async(page:Page,path:string)=>{const r=await page.request.get(base+path);if(!r.ok())throw Error(`Intercompany canonical read refused HTTP${r.status()}`);return r.json();};
 const maker=await get(author,'/api/me'),checker=await get(reviewer,'/api/me');
 if(!maker.user?.id||!checker.user?.id||maker.user.id===checker.user.id||![maker,checker].every(who=>[anchor,other].every(s=>who.companies.some((c:any)=>c.slug===s&&c.role==='owner'))))throw Error('Two distinct ordinary current MFA company owners required');
 const journals=await Promise.all([anchor,other].map(s=>get(author,`/api/companies/${s}/journal`)));
 const privateDir=mkdtempSync('/tmp/rm-intercompany-clients-');
 try{
  for(const[name,page]of [['author',author],['reviewer',reviewer]]as const)writeFileSync(join(privateDir,name),(await page.context().cookies(base)).map(c=>`${c.name}=${c.value}`).join('; '),{flag:'wx',mode:0o600});
  const common=(who:string)=>['--server',base,'--allow-loopback-http','--cookie-file',join(privateDir,who)];
  const cli=(who:string,command:string,flags:string[]=[],body?:string,refusal=false)=>{
   const r=spawnSync(cliBinary,['remote',command,...common(who),...flags,...(body===undefined?[]:['--confirm'])],{input:body,encoding:'utf8',timeout:30000,maxBuffer:16*1024*1024});
   if(r.error||r.status!==(refusal?1:0))throw Error(`Intercompany CLI ${command} process/result failed`);
   const value=JSON.parse(r.stdout);if(refusal){if(!JSON.stringify(value).includes('server_refused'))throw Error('Expected definite native permission/hash/self-review refusal');return value;}return value.data;
  };
  const mcp=(who:string,name:string,args:Record<string,unknown>,refusal=false)=>{
   const messages=[{jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'synthetic-intercompany-clients',version:'1'}}},{jsonrpc:'2.0',method:'notifications/initialized'},{jsonrpc:'2.0',id:2,method:'tools/call',params:{name,arguments:args}}];
   const r=spawnSync(mcpBinary,common(who),{input:messages.map(m=>JSON.stringify(m)).join('\n')+'\n',encoding:'utf8',timeout:30000,maxBuffer:16*1024*1024});if(r.error||r.status!==0)throw Error(`Intercompany MCP ${name} process failed`);const call=r.stdout.trim().split('\n').map(row=>JSON.parse(row)).find(row=>row.id===2);
   if(refusal){if(!call?.result?.isError||call.result.structuredContent?.error?.code!=='server_refused'||call.result.structuredContent.error.status!==422)throw Error('Expected definite native current/hash/self-review refusal');return call.result.structuredContent.error;}
   if(call?.error||call?.result?.isError)throw Error(`Intercompany MCP ${name} refused`);return call.result.structuredContent.data;
  };
  let evidence:any;
  if(!restarted){
   const prior=await get(author,path);if(prior.manifest!==null||prior.mappings.length)throw Error('Client helper requires OWN initially unconfigured GLOBAL manifest; never compose with captured browser baseline');
   const manifest=JSON.stringify({...fixture.manifest,confirm:true});cli('author','apply-group-manifest',['--company',anchor],manifest);
   const current=await get(author,path);if(!current.manifestHash||current.consolidationAvailable!==true||!isDeepStrictEqual(cli('author','intercompany',['--company',anchor]),current)||!isDeepStrictEqual(mcp('reviewer','intercompany',{company:anchor}),await get(reviewer,path)))throw Error('Full current manifest/client parity unavailable');
   const proposal=JSON.stringify({...fixture.mapping,confirm:true});mcp('author','propose_intercompany_mapping',{company:anchor,body:proposal,confirm:true});
   const proposed=await get(author,path),state=proposed.mappings.find((m:any)=>m.mapping.id===fixture.mapping.id);if(state?.status!=='proposed'||!state.mappingHash)throw Error('Client mapping proposal missing canonical hash');
   const exact=JSON.stringify({mappingId:fixture.mapping.id,mappingHash:state.mappingHash,confirm:true});
   cli('author','approve-intercompany-mapping',['--company',anchor],exact,true);if(!isDeepStrictEqual(await get(author,path),proposed))throw Error('Refused self-review changed canonical mapping');
   mcp('reviewer','approve_intercompany_mapping',{company:anchor,body:JSON.stringify({mappingId:fixture.mapping.id,mappingHash:'0'.repeat(64),confirm:true}),confirm:true},true);
   cli('reviewer','approve-intercompany-mapping',['--company',anchor],exact);
   const report=await get(author,reportPath),row=report.rows.find((v:any)=>v.mappingId===fixture.mapping.id);
   if(row?.status!=='matched'||row.difference!=='0.00'||row.left.balance!==fixture.expectedBalance||row.right.balance!==fixture.expectedBalance||!row.left.sourceRefs.length||!row.right.sourceRefs.length||![...row.left.sourceRefs,...row.right.sourceRefs].every((v:any)=>typeof v.amount==='string'))throw Error('Actual reciprocal source/decimal-string facts differ');
   if(!isDeepStrictEqual(cli('author','group-reconciliation',['--as-of',fixture.asOf]),report)||!isDeepStrictEqual(mcp('author','group_reconciliation',{asOf:fixture.asOf}),report))throw Error('Full global HTTP/CLI/MCP projection mismatch');
   mcp('author','revoke_intercompany_mapping',{company:anchor,body:exact,confirm:true});if((await get(author,reportPath)).rows.some((v:any)=>v.mappingId===fixture.mapping.id))throw Error('Explicitly revoked mapping still reconciles');
   // Explicit NEW lifecycle after confirmed revoke, not an automatic retry of uncertainty.
   cli('author','propose-intercompany-mapping',['--company',anchor],proposal);mcp('reviewer','approve_intercompany_mapping',{company:anchor,body:exact,confirm:true});
   evidence={anchor,other,fixture,principals:[maker.user.id,checker.user.id],ledgerUuids:journals.map(v=>v.journal.ledgerUuid),current:await get(author,path),report:await get(author,reportPath),journals};
   mkdirSync('/tmp/rentemester-rust-reports',{recursive:true});writeFileSync(evidencePath,JSON.stringify(evidence),{mode:0o600});
  }else evidence=JSON.parse(readFileSync(evidencePath,'utf8'));
  if(evidence.anchor!==anchor||evidence.other!==other||!isDeepStrictEqual(evidence.fixture,fixture)||!isDeepStrictEqual(evidence.principals,[maker.user.id,checker.user.id])||!isDeepStrictEqual(evidence.ledgerUuids,journals.map(v=>v.journal.ledgerUuid)))throw Error('Intercompany source fixture/principal/ledger identity changed');
  const current=await get(author,path),report=await get(author,reportPath);
  if(!isDeepStrictEqual(current,evidence.current)||!isDeepStrictEqual(report,evidence.report)||!isDeepStrictEqual(cli('author','intercompany',['--company',anchor]),current)||!isDeepStrictEqual(mcp('author','intercompany',{company:anchor}),current)||!isDeepStrictEqual(cli('author','group-reconciliation',['--as-of',fixture.asOf]),report)||!isDeepStrictEqual(mcp('author','group_reconciliation',{asOf:fixture.asOf}),report))throw Error('Canonical current mapping/source or full client parity changed');
  for(const[i,slug]of[anchor,other].entries())if(!isDeepStrictEqual(await get(author,`/api/companies/${slug}/journal`),evidence.journals[i]))throw Error('Intercompany client operation changed accounting');
  return{ok:true,restarted,anchor,other,mappingId:fixture.mapping.id,independentNormalMfaReview:true,fullCanonicalClientParity:true,decimalStringsPreserved:true,accountingEffect:'none'};
 }finally{rmSync(privateDir,{recursive:true,force:true});}
}
