import type { Page } from '@playwright/test';
import { createHash, createPublicKey, verify } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { isDeepStrictEqual } from 'node:util';
import { createLocalRuntimeProof } from './check-native-local-runtime';

/** Whole708 actual current MFA/local Rust owner; no authority/retention seeded in SQL. */
export function createPrivacyComplianceProof(owner: Page, base: string, work: string, member: Page, users: Parameters<typeof createLocalRuntimeProof>[4]) {
  const local = createLocalRuntimeProof(owner, base, work, member, users);
  const company='synthetic', other='synthetic-privacy-other', prefix='/api/companies/'+company+'/';
  const free={name:'Synthetic Privacy708 Eligible',cvr:'SYN-PRIVACY708-FREE'}, retained={name:'Synthetic Privacy708 Retained',cvr:'SYN-PRIVACY708-KEEP'};
  const cookie=join(work,'privacy708.cookie'), holdId='synthetic-privacy708-hold';
  const original=Buffer.from('Synthetic mandatory retained Privacy708 original.\n');
  const need=(ok:unknown,label:string)=>{if(!ok)throw Error('Privacy708: '+label);};
  const hash=(v:Buffer|string)=>createHash('sha256').update(v).digest('hex');
  let baseline:any, documentId:number, today:string, finalFree:any, finalRetained:any, sourceReport:any;
  async function post(path:string,data:any,page=owner){const r=await page.request.post(base+path,{headers:{origin:base},data});let body:any;try{body=await r.json();}catch{body=null;}return{status:r.status(),body};}
  async function good(path:string,data:any){const r=await post(path,data);need(r.status>=200&&r.status<300&&r.body?.ok===true,path+' HTTP'+r.status+': '+JSON.stringify(r.body).slice(0,500));return r.body;}
  async function get(path:string){const r=await owner.request.get(base+path);need(r.ok(),path+' HTTP'+r.status());return r.json();}
  const exportSubject=async(subject:any)=>(await good(prefix+'gdpr/export',{...subject,confirm:true})).gdpr.export;
  const books=async()=>({a:await get(prefix+'journal/entries?all=true'),b:await get('/api/companies/'+other+'/journal/entries?all=true'),trial:await get(prefix+'trial-balance')});
  async function refreshCookie(){writeFileSync(cookie,(await owner.context().cookies(base)).map(c=>`${c.name}=${c.value}`).join('; '),{mode:0o600});}
  function client(kind:'cli'|'mcp',operation:string,body:any,success=true,slug=company){
    const common=['--server',base,'--allow-loopback-http','--cookie-file',cookie];
    const argv=kind==='cli'?['remote',operation.replaceAll('_','-'),...common,'--company',slug,'--confirm']:common;
    const input=kind==='cli'?JSON.stringify(body):[
      {jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'synthetic-privacy708',version:'1'}}},
      {jsonrpc:'2.0',method:'notifications/initialized'},
      {jsonrpc:'2.0',id:2,method:'tools/call',params:{name:operation,arguments:{company:slug,body:JSON.stringify(body),confirm:true}}}
    ].map(v=>JSON.stringify(v)).join('\n')+'\n';
    const out=spawnSync(join(work,kind==='cli'?'rentemester-rust':'rentemester-mcp'),argv,{input,encoding:'utf8',timeout:45000,maxBuffer:16*1024*1024});
    need(!out.error&&out.status===(kind==='cli'&&!success?1:0),kind+' '+operation+' exit'+out.status+': '+out.stdout.slice(0,500));
    const secret=readFileSync(cookie,'utf8').trim();need(secret&&!out.stdout.includes(secret)&&!out.stderr.includes(secret),'No credential disclosure');
    if(kind==='cli'){const v=JSON.parse(out.stdout);need(v.ok===success,'CLI typed result');return success?v.data:null;}
    const call=out.stdout.trim().split('\n').map(s=>JSON.parse(s)).find(v=>v.id===2);need(call?.result?.isError===!success,'MCP typed result '+JSON.stringify(call).slice(0,500));
    need(isDeepStrictEqual(JSON.parse(call.result.content[0].text),call.result.structuredContent),'MCP text/structured agree');return success?call.result.structuredContent.data:null;
  }
  async function protectedState(){const current=await exportSubject(retained);need(current.records.some((r:any)=>r.source==='documents'&&r.sourceRowId===documentId&&r.underRetention&&!r.erased&&!r.erasable),'Mandatory original remains under retention');const r=await owner.request.get(base+prefix+`documents/${documentId}/file`);need(r.ok()&&(await r.body()).equals(original),'Exact mandatory original bytes preserved');need(isDeepStrictEqual(await books(),baseline),'Both books exact unchanged');return current;}
  return {
    async run(){
      today=(await get('/api/portfolio')).portfolio.asOf;
      await good('/api/companies',{name:'Synthetic Privacy Other',slug:other});
      for(const subject of [free,retained])await good(prefix+'customers',{name:subject.name,vatOrCvr:subject.cvr,address:'Synthetic Privacy708 Street',email:'synthetic-privacy708@example.invalid'});
      const d=await good(prefix+'documents/ingest',{confirm:true,fileName:'synthetic-privacy708-retained.txt',fileBase64:original.toString('base64'),metadata:{source:'Synthetic Privacy708 retained fixture',documentType:'cash_register_receipt',issueDate:today,sender:{name:retained.name,vatOrCvr:retained.cvr}}});documentId=d.document.id;
      const inherited=await local.run();await refreshCookie();baseline=await books();await protectedState();
      const before=await exportSubject(free);need(before.records.some((r:any)=>r.source==='customers'&&r.erasable&&!r.erased),'Eligible nonfinancial subject present');
      const hold={...free,holdId,operation:'place',reason:'Synthetic current authorized preservation',confirm:true};
      await owner.goto(base+'/companies/synthetic/gdpr');await owner.getByLabel('CVR',{exact:true}).fill(free.cvr);await owner.getByLabel('Navn',{exact:true}).fill(free.name);await owner.getByLabel('Hold-ID',{exact:true}).fill(holdId);await owner.getByLabel('Begrundelse',{exact:true}).fill(hold.reason);await owner.getByRole('button',{name:'Placér hold',exact:true}).click();const [placedResponse]=await Promise.all([owner.waitForResponse(r=>r.url().endsWith('/gdpr/legal-hold')&&r.request().method()==='POST'),owner.getByRole('dialog',{name:'Bekræft GDPR-handling',exact:true}).getByRole('button',{name:'Udfør bekræftet handling',exact:true}).click()]);const placed=await placedResponse.json();need(placedResponse.ok()&&placed.ok&&placed.gdprLegalHold,'Shared web UI confirmed hold reaches same native server');await owner.getByRole('dialog',{name:'Bekræft GDPR-handling',exact:true}).waitFor({state:'hidden'});
      const discovery=client('mcp','gdpr_discover',{...free,confirm:true});need(discovery.ok&&discovery.gdprDiscovery&&JSON.stringify(discovery.gdprDiscovery).includes(holdId),'MCP discover includes actual hold history');
      for(const operation of ['forget','erase']){const held=await post(prefix+'gdpr/'+operation,{...free,...(operation==='forget'?{afterRetentionExpiry:true}:{}),confirm:true});need((held.status===422||held.status===200)&&!held.body?.gdprErasure?.erasedCount,'Active hold refuses '+operation);}
      need((await exportSubject(free)).records.some((r:any)=>r.source==='customers'&&!r.erased),'Held refusal preserved customer');
      const wrong=await post('/api/companies/'+other+'/gdpr/legal-hold',{...free,holdId,operation:'release',reason:'Synthetic wrong-company release',confirm:true});need(wrong.status===422,'Different company cannot release hold identity');
      const forged=await post(prefix+'gdpr/forget',{...retained,afterRetentionExpiry:true,asOf:'9999-12-31',confirm:true});need(forged.status===400,'Caller date cannot authorize erasure');
      const premature=await good(prefix+'gdpr/forget',{...retained,afterRetentionExpiry:true,confirm:true});need(premature.gdprErasure.erasedCount===0&&premature.gdprErasure.refusedCount>0,'Premature mandatory deletion refused');
      const memberId=(await member.request.get(base+'/api/me').then(r=>r.json())).user.id;
      await good('/api/workspace/members/company',{userId:memberId,action:'grant',companySlug:company,role:'reader'});
      need((await post(prefix+'gdpr/legal-hold',{...hold,operation:'release'},member)).status===403,'Reader cannot release legal hold');
      need((await post(prefix+'gdpr/audit-log',{confirm:true},member)).status===200,'Existing reader CompanyExport permission permits audited export');
      await good('/api/workspace/members/company',{userId:memberId,action:'revoke',companySlug:company});
      need((await post(prefix+'gdpr/discover',{...free,confirm:true},member)).status===403,'Current revoked principal cannot discover');need((await post(prefix+'gdpr/audit-log',{confirm:true},member)).status===403,'Current revoked principal cannot export audit');
      const released=client('mcp','gdpr_legal_hold',{...hold,operation:'release',reason:'Synthetic resolved preservation'});need(released.ok&&released.gdprLegalHold,'MCP current authorized release');
      const forgotten=client('cli','gdpr_forget',{...free,afterRetentionExpiry:true,confirm:true});need(forgotten.gdprErasure.erasedCount>0,'Eligible confirmed forget commits scoped overlay');
      finalFree=await exportSubject(free);need(finalFree.records.some((r:any)=>r.source==='customers'&&r.erased&&r.personalData.email===null),'Overlay hides customer PII');need(JSON.stringify(finalFree).includes(holdId),'Hold history survives forgetting');
      const preservedHold=client('cli','gdpr_legal_hold',{...retained,holdId:holdId+'-retained',operation:'place',reason:'Synthetic active hold retained across backup/restart',confirm:true});need(preservedHold.gdprLegalHold,'Second active hold preserved in signed snapshot');finalRetained=await protectedState();
      const audit=client('mcp','gdpr_audit_log',{confirm:true});need(audit.ok&&audit.gdprAudit,'Signed native GDPR audit exported');writeFileSync(join(work,'privacy708-audit.json'),JSON.stringify(audit.gdprAudit),{mode:0o600,flag:'wx'});
      const envelope=audit.gdprAudit,trusted=readFileSync(join(work,'trusted.pub')),signed=Buffer.concat([Buffer.from('rentemester-gdpr-audit/1\0'),Buffer.from(envelope.canonicalPayload)]);need(envelope.format==='rentemester-gdpr-audit/1'&&envelope.algorithm==='Ed25519'&&envelope.fingerprint===hash(signed)&&envelope.signerPublicFingerprint===hash(trusted)&&Buffer.from(envelope.signerPublicKey,'base64').equals(trusted),'Signed audit independent trusted metadata');const publicKey=createPublicKey({format:'der',type:'spki',key:Buffer.concat([Buffer.from('302a300506032b6570032100','hex'),trusted])});need(verify(null,signed,publicKey,Buffer.from(envelope.signature,'base64')),'Actual independent Ed25519 verification');need(!verify(null,Buffer.concat([signed,Buffer.from('tampered')]),publicKey,Buffer.from(envelope.signature,'base64')),'Tampered signed payload refused');const payload=JSON.parse(envelope.canonicalPayload);need(payload.events.length>0&&payload.events.every((e:any)=>e.event_type.startsWith('gdpr_')&&e.actor.startsWith('user:')),'Audit only literal GDPR events and attributed current humans');
      const staticResponse=await fetch(base+'/api/regulatory/coverage');need(staticResponse.status===200,'Public immutable regulatory coverage available without credentials');const staticCoverage=await staticResponse.json() as any;const staticCli=spawnSync(join(work,'rentemester-rust'),['reg','coverage','--json'],{encoding:'utf8',timeout:45000,maxBuffer:16*1024*1024});need(!staticCli.error,'Static native corpus CLI terminal');const cliCoverage=JSON.parse(staticCli.stdout);need(staticCli.status===(cliCoverage.ok?0:1)&&isDeepStrictEqual(cliCoverage.coverage,staticCoverage.regulatoryCoverage),'Static offline CLI and immutable HTTP exact versioned corpus, truthful failure exit');const citationResponse=await fetch(base+'/api/regulatory/citations');need(citationResponse.status===200,'Public immutable citations available');const citations=await citationResponse.json() as any;need(typeof citations.regulatoryCitations?.report==='string'&&isDeepStrictEqual(citations.regulatoryCitations.coverage,cliCoverage.coverage),'Citations same exact source versions');
      sourceReport=await get(prefix+'compliance-report?asOf='+today);need(sourceReport.ok&&sourceReport.complianceReport.contentType==='text/html; charset=utf-8'&&hash(sourceReport.complianceReport.html)===sourceReport.complianceReport.sha256,'Company HTML deterministic fingerprint');
      const repeat=await get(prefix+'compliance-report?asOf='+today);need(hash(repeat.complianceReport.html)===repeat.complianceReport.sha256&&isDeepStrictEqual(repeat.complianceReport.data.coverage,sourceReport.complianceReport.data.coverage),'Live observation may advance backup checkedAt; HTML fingerprint and frozen rule/source versions remain exact');
      need((await owner.request.get(base+prefix+'compliance-report?asOf=9999-99-99')).status()===400,'Malformed report date refused');
      await owner.goto(base+'/companies/synthetic/gdpr');await owner.getByRole('heading',{name:'GDPR-indsigt',exact:true}).waitFor();
      const backup=await owner.request.post(base+'/api/workspace/backup',{headers:{origin:base},data:{confirm:true}});need(backup.ok()&&backup.headers()['content-type']==='application/vnd.rentemester.native-backup+json','Legal hold history accepted by signed snapshot');const bytes=await backup.body();writeFileSync(join(work,'privacy708.native-backup.json'),bytes,{mode:0o600,flag:'wx'});const verified=spawnSync(join(work,'rentemester-server'),['--verify-backup',join(work,'privacy708.native-backup.json'),'--trusted-backup-key',join(work,'trusted.pub')],{encoding:'utf8',timeout:60000,maxBuffer:4*1024*1024});need(!verified.error&&verified.status===0,'Native signed snapshot independently verifies');
      await protectedState();return {...inherited,privacy708:{qualified:true,ordinaryMfa:true,holdId,documentId,today,activeHoldRefusedBothDeletePaths:true,otherCompanyCannotRelease:true,readerAndCurrentRevocationRefused:true,serverDateOnly:true,eligibleOverlay:true,mandatoryOriginalPreserved:true,accountingUnchanged:true,holdHistorySurvivesForget:true,signedAuditFile:'privacy708-audit.json',signedSnapshotSha256:hash(bytes),signedSnapshotFile:'privacy708.native-backup.json',complianceHtmlSha256:sourceReport.complianceReport.sha256,sourceFreeRestoreQualified:false}};
    },
    async reopen(){const inherited=await local.reopen();await refreshCookie();await protectedState();const freeAfter=await exportSubject(free);need(isDeepStrictEqual(freeAfter.records.filter((r:any)=>r.source!=='audit_log'),finalFree.records.filter((r:any)=>r.source!=='audit_log')),'Normal MFA restart preserves subject overlays/hold history');need(JSON.stringify(freeAfter).includes(holdId),'Restart preserves hold history');return {...inherited,privacy708:{qualified:true,exactNormalMfaRestartState:true,holdHistoryAndOverlayPreserved:true,mandatoryOriginalPreserved:true,accountingUnchanged:true}};}
  };
}
