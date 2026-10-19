import {spawnSync} from 'node:child_process';
import {mkdtempSync,mkdirSync,readFileSync,rmSync,writeFileSync} from 'node:fs';
import {join} from 'node:path';
import {isDeepStrictEqual} from 'node:util';
import type {Page} from '@playwright/test';
export type EliminationClientFixture={asOf:string;inputs:Array<{id:string;mappingId:string;evidenceRefs:string[]}>};
const evidencePath='/tmp/rentemester-rust-reports/native-elimination-clients.json';
/** Separate ordinary two-MFA fixture: caller owns real companies, source journals and approved mappings. Never run after freezing browser-helper baselines. */
export async function checkNativeEliminationClients(author:Page,reviewer:Page,base:string,anchor:string,other:string,fixture:EliminationClientFixture,restarted=false){
 if(!/^http:\/\/127\.0\.0\.1:\d+$/.test(base)||anchor===other||![anchor,other].every(s=>/^synthetic-eliminations-[a-z0-9-]+$/.test(s))||!/^\d{4}-\d{2}-\d{2}$/.test(fixture.asOf)||fixture.inputs.length!==2||!fixture.inputs.every(i=>/^synthetic-client-elimination-[a-z0-9-]+$/.test(i.id)))throw Error('Own explicit synthetic elimination pair/two-orientation source fixture required');
 const cliBinary=process.env.RM_NATIVE_CLI_BIN,mcpBinary=process.env.RM_NATIVE_MCP_BIN;if(!cliBinary||!mcpBinary)throw Error('Explicit qualified CLI/MCP artifacts required');
 const path=`/api/companies/${anchor}/eliminations`,global=`/api/group-eliminations?asOf=${fixture.asOf}`;
 const get=async(page:Page,path:string)=>{const r=await page.request.get(base+path);if(!r.ok())throw Error(`Elimination source read HTTP${r.status()}`);return r.json();};
 const maker=await get(author,'/api/me'),checker=await get(reviewer,'/api/me');if(!maker.user?.id||maker.user.id===checker.user?.id||![maker,checker].every(who=>[anchor,other].every(s=>who.companies.some((c:any)=>c.slug===s&&c.role==='owner'))))throw Error('Distinct normal verified MFA owners with current both-company authority required');
 const journals=await Promise.all([anchor,other].map(s=>get(author,`/api/companies/${s}/journal`)));
 const privateDir=mkdtempSync('/tmp/rm-elimination-clients-');
 try{
  for(const[name,page]of [['author',author],['reviewer',reviewer]]as const)writeFileSync(join(privateDir,name),(await page.context().cookies(base)).map(c=>`${c.name}=${c.value}`).join('; '),{flag:'wx',mode:0o600});
  const common=(who:string)=>['--server',base,'--allow-loopback-http','--cookie-file',join(privateDir,who)];
  const cli=(who:string,command:string,flags:string[],body?:string,refusal=false)=>{
   const write=body!==undefined&&command!=='plan-elimination';const r=spawnSync(cliBinary,['remote',command,...common(who),...flags,...(write?['--confirm']:[])],{input:body,encoding:'utf8',timeout:30000,maxBuffer:16*1024*1024});if(r.error||r.status!==(refusal?1:0))throw Error(`Elimination CLI ${command} failed`);const v=JSON.parse(r.stdout);if(refusal){if(v.errors?.[0]?.code!=='server_refused'&&!JSON.stringify(v).includes('server_refused'))throw Error('Expected definite native refusal');return v;}return v.data;
  };
  const mcp=(who:string,name:string,args:Record<string,unknown>,refusal=false)=>{
   const messages=[{jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'synthetic-elimination-clients',version:'1'}}},{jsonrpc:'2.0',method:'notifications/initialized'},{jsonrpc:'2.0',id:2,method:'tools/call',params:{name,arguments:args}}];const r=spawnSync(mcpBinary,common(who),{input:messages.map(m=>JSON.stringify(m)).join('\n')+'\n',encoding:'utf8',timeout:30000,maxBuffer:16*1024*1024});if(r.error||r.status!==0)throw Error(`Elimination MCP ${name} process failed`);const call=r.stdout.trim().split('\n').map(row=>JSON.parse(row)).find(row=>row.id===2);if(refusal){if(!call?.result?.isError||call.result.structuredContent?.error?.code!=='server_refused'||call.result.structuredContent.error.status!==422)throw Error('Expected definite native lifecycle refusal');return call.result.structuredContent.error;}if(call?.error||call?.result?.isError)throw Error(`Elimination MCP ${name} refused`);return call.result.structuredContent.data;
  };
  let evidence:any;
  if(!restarted){
   const prior=await get(author,path);if(fixture.inputs.some(i=>prior.rows.some((r:any)=>r.payload.id===i.id)))throw Error('Own new elimination IDs required; no automatic lifecycle replay');
   const positions:string[]=[];
   for(const input of fixture.inputs){
    const planInput=JSON.stringify({...input,asOf:fixture.asOf}),before=await get(author,path);
    const plan=cli('author','plan-elimination',['--company',anchor],planInput);if(!isDeepStrictEqual(plan,mcp('author','plan_elimination',{company:anchor,body:planInput}))||!isDeepStrictEqual(before,await get(author,path)))throw Error('Readonly plan parity or no-write invariant failed');
    const payload=plan.plan?.payload,hash=plan.plan?.payloadHash;if(payload?.version!==2||!/^[a-f0-9]{64}$/.test(hash)||typeof payload.amountOre!=='string'||!/^\d+$/.test(payload.amountOre)||payload.left.companySlug!==anchor||payload.right.companySlug!==other)throw Error('Authoritative V2 frozen source identity/money proof missing');positions.push(payload.left.position);
    const proposal=JSON.stringify({...input,asOf:fixture.asOf,expectedPayloadHash:hash,confirm:true}),exact=JSON.stringify({eliminationId:input.id,payloadHash:hash,confirm:true});
    cli('author','propose-elimination',['--company',anchor],proposal);
    const proposed=await get(author,path);mcp('author','approve_elimination',{company:anchor,body:exact,confirm:true},true);cli('reviewer','apply-elimination',['--company',anchor],exact,true);if(!isDeepStrictEqual(proposed,await get(author,path)))throw Error('Refused self-review/premature apply changed state');
    mcp('reviewer','reject_elimination',{company:anchor,body:exact,confirm:true});
    // Explicit new proposal after definite rejection, never an uncertain retry.
    mcp('author','propose_elimination',{company:anchor,body:proposal,confirm:true});
    cli('reviewer','approve-elimination',['--company',anchor],exact);
    mcp('reviewer','apply_elimination',{company:anchor,body:exact,confirm:true});
    cli('author','reverse-elimination',['--company',anchor],exact);
    cli('author','propose-elimination',['--company',anchor],proposal);
    mcp('reviewer','approve_elimination',{company:anchor,body:exact,confirm:true});
    cli('reviewer','apply-elimination',['--company',anchor],exact);
    const state=(await get(author,path)).rows.find((r:any)=>r.payload.id===input.id);if(state?.status!=='applied'||state.payloadHash!==hash||!isDeepStrictEqual(state.payload,payload))throw Error('Applied canonical exact versioned payload changed');
   }
   if(!positions.includes('payable')||!positions.includes('receivable'))throw Error('Explicit fixture must exercise both real accounting orientations');
   evidence={anchor,other,fixture,principals:[maker.user.id,checker.user.id],journals,current:await get(author,path),applied:await get(author,global)};mkdirSync('/tmp/rentemester-rust-reports',{recursive:true});writeFileSync(evidencePath,JSON.stringify(evidence),{mode:0o600});
  }else evidence=JSON.parse(readFileSync(evidencePath,'utf8'));
  if(evidence.anchor!==anchor||evidence.other!==other||!isDeepStrictEqual(evidence.fixture,fixture)||!isDeepStrictEqual(evidence.principals,[maker.user.id,checker.user.id]))throw Error('Elimination fixture identity changed');
  const current=await get(author,path),applied=await get(author,global);if(!isDeepStrictEqual(current,evidence.current)||!isDeepStrictEqual(applied,evidence.applied)||!isDeepStrictEqual(cli('author','eliminations',['--company',anchor]),current)||!isDeepStrictEqual(mcp('author','eliminations',{company:anchor}),current)||!isDeepStrictEqual(cli('author','group-eliminations',['--as-of',fixture.asOf]),applied)||!isDeepStrictEqual(mcp('author','group_eliminations',{asOf:fixture.asOf}),applied))throw Error('Full current/applied HTTP CLI MCP restart parity changed');
  for(const[i,s]of[anchor,other].entries())if(!isDeepStrictEqual(await get(author,`/api/companies/${s}/journal`),evidence.journals[i]))throw Error('Evidence-only client lifecycle altered immutable ledger');
  return{ok:true,restarted,anchor,other,bothOrientations:true,fullCanonicalParity:true,independentNormalMfaReview:true,accountingEffect:'none'};
 }finally{rmSync(privateDir,{recursive:true,force:true});}
}
