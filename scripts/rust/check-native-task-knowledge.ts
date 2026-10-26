// Existing React custom board lifecycle on the common Rust server; synthetic data only.
import {chromium,expect} from '@playwright/test';
import {readFileSync,writeFileSync,readdirSync,chmodSync} from 'node:fs';
import {join} from 'node:path';
import {createHmac} from 'node:crypto';
import {spawnSync} from 'node:child_process';
const base=process.env.RM_TASK_ORIGIN!,out=process.env.RM_TASK_OUTPUT!,bin=process.env.RM_TASK_BINARY_DIR!,phase=process.env.RM_TASK_PHASE!,reopen=phase!=='first';
if(!base?.startsWith('http://127.0.0.1:')||!out||!bin)throw Error('Use the disposable native task runner');
function save(name:string,value:unknown){const path=join(out,name);writeFileSync(path,JSON.stringify(value,null,2)+'\n',{mode:0o600});chmodSync(path,0o600);}
function totp(uri:string){const secret=new URL(uri).searchParams.get('secret')!;let bits=0,count=0;const bytes:number[]=[];for(const c of secret){const n='ABCDEFGHIJKLMNOPQRSTUVWXYZ234567'.indexOf(c);if(n<0)throw Error('base32');bits=(bits<<5)|n;count+=5;if(count>=8){count-=8;bytes.push((bits>>>count)&255);}}const ctr=Buffer.alloc(8);ctr.writeBigUInt64BE(BigInt(Math.floor(Date.now()/30000)));const digest=createHmac('sha1',Buffer.from(bytes)).update(ctr).digest();const i=digest[19]&15;return String((digest.readUInt32BE(i)&0x7fffffff)%1000000).padStart(6,'0');}
const browser=await chromium.launch({headless:true});
try{
 const context=await browser.newContext(reopen?{storageState:join(out,'session.json')}:{viewport:{width:1440,height:1000}});
 const page=await context.newPage();page.setDefaultTimeout(15000);const errors:string[]=[];page.on('pageerror',e=>errors.push(e.message));
 const headers={origin:base,'content-type':'application/json'};
 async function get(path:string){const r=await page.request.get(base+path);if(!r.ok())throw Error(`GET ${path}: ${r.status()} ${await r.text()}`);return r.json();}
 async function post(path:string,data:unknown){const r=await page.request.post(base+path,{headers,data});if(!r.ok())throw Error(`POST ${path}: ${r.status()} ${await r.text()}`);return r.json();}

 if(phase==='first'){
  const token=new URLSearchParams(new URL(process.env.RM_TASK_SETUP!).hash.slice(1)).get('setup')!;
  const owner=await page.request.post(base+'/api/setup/first-owner',{headers:{...headers,'x-rentemester-install-token':token},data:{name:'Synthetic Task Owner',email:'owner@example.invalid',password:'synthetic-password-123'}});expect(owner.ok()).toBe(true);
  const root=process.env.RM_TASK_WORKSPACE!;if(!root.includes('Rentemester-Synthetic-Tasks-'))throw Error('Only disposable fixture allowed');
  const handoff=readdirSync(join(root,'.rentemester')).find(s=>s.startsWith('verification-')&&s.endsWith('.txt'))!;
  const verified=await page.request.get(readFileSync(join(root,'.rentemester',handoff),'utf8').trim());expect(verified.ok()).toBe(true);
  const enrollment=await post('/api/auth/mfa-enrollment/begin',{email:'owner@example.invalid',password:'synthetic-password-123'});
  await post('/api/auth/mfa-enrollment/confirm',{code:totp(enrollment.totpURI)});
  await context.storageState({path:join(out,'session.json')});chmodSync(join(out,'session.json'),0o600);
  writeFileSync(join(out,'session.cookie'),(await context.cookies(base)).map(c=>`${c.name}=${c.value}`).join('; '),{mode:0o600});

  const created=await post('/api/tasks',{title:'Synthetic protected company task',scope:{kind:'company',companySlug:'synthetic'},references:[{kind:'knowledge',ref:'page-company'}],idempotencyKey:'knowledge-company-task',confirm:true});
  const workspace=await post('/api/tasks',{title:'Synthetic protected workspace task',scope:{kind:'workspace',companySlugs:[]},references:[{kind:'knowledge',ref:'page-workspace'}],idempotencyKey:'knowledge-workspace-task',confirm:true});
  const service=await post('/api/workspace/service-principals',{displayName:'Synthetic knowledge reader',operationId:crypto.randomUUID(),confirm:true});await post('/api/workspace/members/access',{userId:service.serviceAccountId,action:'set-role',workspaceRole:'member'});await post('/api/workspace/members/company',{userId:service.serviceAccountId,companySlug:'synthetic',action:'grant',role:'reader'});
  writeFileSync(join(out,'transient-reader.key'),service.secret,{mode:0o600,flag:'wx'});save('knowledge-state.json',{companyTask:created.task.taskId,workspaceTask:workspace.task.taskId,serviceId:service.serviceAccountId});save('knowledge-first.json',{ok:true,ordinaryOwnerVerificationMfa:true,syntheticTasksOnly:true,pagesProvisionedSeparatelyByNativeProgrammaticCore:true,errors});
 }else{
  const state=JSON.parse(readFileSync(join(out,'knowledge-state.json'),'utf8'));const companyPath='/api/companies/synthetic/knowledge-pages/page-company',workspacePath='/api/knowledge-pages/page-workspace';
  const company=await get(companyPath),workspace=await get(workspacePath);expect(company.page.title).toBe('Synthetic company playbook');expect(workspace.page.title).toBe('Synthetic workspace playbook');
  if(phase==='read'){
   await page.goto(base+'/opgaver/'+state.companyTask);await page.getByRole('link',{name:'knowledge: page-company',exact:true}).click();await expect(page.getByRole('heading',{name:'Synthetic company playbook',exact:true})).toBeVisible();await expect(page.getByRole('article',{name:'Viden og playbook'})).toContainText('<script>window.syntheticInjected=true</script>');expect(await page.evaluate(()=>Object.hasOwn(globalThis,'syntheticInjected'))).toBe(false);
   await page.goto(base+'/opgaver/'+state.workspaceTask);await page.getByRole('link',{name:'knowledge: page-workspace',exact:true}).click();await expect(page.getByRole('heading',{name:'Synthetic workspace playbook',exact:true})).toBeVisible();
   for(const path of ['/api/knowledge-pages/page-company','/api/companies/synthetic/knowledge-pages/page-expired','/api/companies/synthetic/knowledge-pages/page-missing']){const r=await page.request.get(base+path);expect(r.status()).toBe(404);expect(await r.text()).not.toContain('protected instructions');}
   await page.goto(base+'/companies/synthetic/viden/page-missing');await expect(page.getByText('Referencen er ikke tilgængelig.',{exact:true})).toBeVisible();
   const anonymous=await browser.newContext();const anon=await anonymous.request.get(base+companyPath);expect(anon.status()).toBe(401);
   const key=readFileSync(join(out,'transient-reader.key'),'utf8'),serviceHeaders={'x-rentemester-service-key':key,origin:base};
   const allowed=await anonymous.request.get(base+companyPath,{headers:serviceHeaders});expect(allowed.status()).toBe(200);expect((await allowed.json()).page).toEqual(company.page);
   const hidden=await anonymous.request.get(base+workspacePath,{headers:serviceHeaders});expect(hidden.status()).toBe(403);expect(await hidden.text()).not.toContain('protected instructions');
   await post('/api/workspace/members/company',{userId:state.serviceId,companySlug:'synthetic',action:'revoke'});const revoked=await anonymous.request.get(base+companyPath,{headers:serviceHeaders});expect(revoked.status()).toBe(403);expect(await revoked.text()).not.toContain('protected instructions');
   await post('/api/workspace/members/company',{userId:state.serviceId,companySlug:'synthetic',action:'grant',role:'reader'});expect((await anonymous.request.get(base+companyPath,{headers:serviceHeaders})).status()).toBe(200);
   await anonymous.close();save('knowledge-state.json',{...state,company,workspace});await page.screenshot({path:join(out,'knowledge.png'),fullPage:true});save('knowledge-read.json',{ok:true,existingReactTaskLinks:['company','workspace'],escapedTextNoScriptExecution:true,currentServiceCompanyRead:true,workspaceOwnerOnly:true,freshRevocationBeforeContent:true,refusals:['anonymous401','wrongScope404','expired404','missing404','workspaceNonOwner403','revokedCompany403'],errors});
  }else{
   expect(company).toEqual(state.company);expect(workspace).toEqual(state.workspace);await page.goto(base+'/companies/synthetic/viden/page-company');await expect(page.getByRole('heading',{name:'Synthetic company playbook',exact:true})).toBeVisible();save('knowledge-reopen.json',{ok:true,exactPersistedCompanyAndWorkspacePages:true,existingReactAfterActualRestart:true,errors});
  }
 }
 expect(errors).toEqual([]);await context.close();console.log(`PASS native protected knowledge ${phase}`);
}catch(e){save('knowledge-failure.json',{message:String(e)});throw e;}finally{await browser.close();}
