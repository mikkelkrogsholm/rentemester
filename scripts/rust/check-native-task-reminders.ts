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

  const created=await post('/api/tasks',{title:'Synthetic native reminder',scope:{kind:'company',companySlug:'synthetic'},references:[{kind:'document',ref:'synthetic-reminder-proof'}],idempotencyKey:'reminder-created',confirm:true});const id=created.task.taskId;
  save('reminder-state.json',{id,created});const writes:any[]=[];
  page.on('request',r=>{if(r.method()==='POST'&&new URL(r.url()).pathname===`/api/tasks/${id}/reminder`){writes.push(r.postDataJSON());save('reminder-writes.json',writes);}});
  await page.goto(base+'/opgaver/'+id);await page.getByRole('button',{name:'Påmindelser',exact:true}).click();const dialog=page.getByRole('dialog',{name:'Påmindelser i Rentemester'});
  await expect(dialog.getByText('Påmindelsesruntime er stoppet. Planen kan gemmes, men leveringen er inaktiv.',{exact:true})).toBeVisible();
  await dialog.getByLabel('Aktivér denne påmindelsesplan',{exact:true}).check();await dialog.getByLabel('Opfølgningsdato',{exact:true}).fill(new Date().toISOString().slice(0,10));await dialog.getByLabel('Tidszone',{exact:true}).fill('UTC');
  await dialog.getByRole('button',{name:'Gem påmindelsesplan',exact:true}).click();await expect(dialog).toHaveCount(0);expect(writes).toHaveLength(1);
  const configured=await get('/api/tasks/'+id);expect(configured.task.reminders).toHaveLength(1);expect(configured.task.version).toBe(2);expect((await get('/api/tasks/runtime')).runtime.running).toBe(false);expect((await get('/api/tasks/notifications')).notifications).toEqual([]);
  const service=await post('/api/workspace/service-principals',{displayName:'Synthetic task executor',operationId:crypto.randomUUID(),confirm:true});
  await post('/api/workspace/members/access',{userId:service.serviceAccountId,action:'set-role',workspaceRole:'member'});
  await post('/api/workspace/members/company',{userId:service.serviceAccountId,companySlug:'synthetic',action:'grant',role:'bookkeeper'});
  writeFileSync(join(out,'transient-worker.key'),service.secret+'\n',{mode:0o600,flag:'wx'});
  save('reminder-state.json',{id,created,configured,writes});save('reminder-first.json',{ok:true,syntheticOnly:true,configuredThroughExistingReact:true,disabledRuntimeHonest:true,noPrematureDelivery:true,errors});
 }else{
  const state=JSON.parse(readFileSync(join(out,'reminder-state.json'),'utf8'));const id=state.id;
  await expect.poll(async()=>(await get('/api/tasks/runtime')).runtime.running,{timeout:15000}).toBe(true);
  await expect.poll(async()=>(await get('/api/tasks/notifications')).notifications.filter((n:any)=>n.taskId===id).length,{timeout:15000}).toBe(1);
  const notifications=(await get('/api/tasks/notifications')).notifications.filter((n:any)=>n.taskId===id);expect(notifications[0].title).toBe('Synthetic native reminder');
  if(phase==='worker'){
   await page.goto(base+'/companies/synthetic/opgaver');await page.getByText('Påmindelser til dig (1)',{exact:true}).click();await expect(page.locator('details').filter({has:page.getByText('Påmindelser til dig (1)',{exact:true})}).getByRole('link',{name:'Synthetic native reminder',exact:true})).toBeVisible();
   const args=['--server',base,'--allow-loopback-http','--cookie-file',join(out,'session.cookie')];
   const cli=spawnSync(join(bin,'rentemester-rust'),['tasks','notifications',...args],{input:'{}',encoding:'utf8'});if(cli.status!==0)throw Error('Reminder CLI: '+cli.stderr);expect(JSON.parse(cli.stdout).data.notifications).toEqual(notifications);
   const frames=[{jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'synthetic-reminders',version:'1'}}},{jsonrpc:'2.0',method:'notifications/initialized'},{jsonrpc:'2.0',id:2,method:'tools/call',params:{name:'tasks_runtime',arguments:{}}}];const mcp=spawnSync(join(bin,'rentemester-mcp'),args,{input:frames.map(f=>JSON.stringify(f)).join('\n')+'\n',encoding:'utf8'});if(mcp.status!==0)throw Error('Reminder MCP: '+mcp.stderr);const response=mcp.stdout.trim().split('\n').map(s=>JSON.parse(s)).find(r=>r.id===2);expect(response.result.isError).toBe(false);expect(response.result.structuredContent.data.runtime.running).toBe(true);
   // Disable in the unchanged dialog; delivery is suppressed immediately on current reads.
   await page.goto(base+'/opgaver/'+id);await page.getByRole('button',{name:'Påmindelser',exact:true}).click();let dialog=page.getByRole('dialog',{name:'Påmindelser i Rentemester'});await dialog.getByLabel('Aktivér denne påmindelsesplan',{exact:true}).uncheck();await dialog.getByRole('button',{name:'Gem påmindelsesplan',exact:true}).click();await expect(dialog).toHaveCount(0);expect((await get('/api/tasks/notifications')).notifications).toEqual([]);
   // Reenable the exact once schedule; it reveals the original slot without producing a duplicate.
   await page.getByRole('button',{name:'Påmindelser',exact:true}).click();dialog=page.getByRole('dialog',{name:'Påmindelser i Rentemester'});await dialog.getByLabel('Aktivér denne påmindelsesplan',{exact:true}).check();await dialog.getByRole('button',{name:'Gem påmindelsesplan',exact:true}).click();await expect(dialog).toHaveCount(0);expect((await get('/api/tasks/notifications')).notifications).toEqual(notifications);
   const body={companySlugs:['synthetic'],idempotencyKey:'explicit-reminder-run',confirm:true};const run=await post('/api/tasks/run',body);expect(run.run.delivered).toBe(0);
   const final=await get('/api/tasks/'+id);save('reminder-state.json',{...state,final,notifications,run,runBody:body});await page.screenshot({path:join(out,'reminder.png'),fullPage:true});save('reminder-delivery.json',{ok:true,realRegisteredCurrentServiceWorker:true,visibleNotification:true,disableSuppression:true,reenableOriginalOnceSlot:true,explicitOwnerRunNoDuplicate:true,clients:['tasks notifications','tasks_runtime'],errors});
  }else{
   expect(notifications).toEqual(state.notifications);expect(await get('/api/tasks/'+id)).toEqual(state.final);
   expect(await post('/api/tasks/'+id+'/reminder',state.writes[0])).toEqual({ok:true,task:state.configured.task});expect(await post('/api/tasks/run',state.runBody)).toEqual(state.run);expect(await get('/api/tasks/'+id)).toEqual(state.final);expect((await get('/api/tasks/notifications')).notifications).toEqual(state.notifications);
   await page.goto(base+'/opgaver/'+id);await page.getByRole('button',{name:'Afslut med dokumentation',exact:true}).click();const dialog=page.getByRole('dialog');await dialog.getByLabel('Afslutningsnote eller begrundelse').fill('Synthetic evidence reviewed');await dialog.getByLabel('Sikre referencer').fill('document:synthetic-reminder-proof');await dialog.getByRole('button',{name:'Gem afslutning',exact:true}).click();await expect(dialog).toHaveCount(0);expect((await get('/api/tasks/notifications')).notifications).toEqual([]);
   save('reminder-reopen.json',{ok:true,workerRestartDedup:true,exactOriginalConfigureAndRunReceipts:true,completionSuppresses:true,errors});
  }
 }
 expect(errors).toEqual([]);await context.close();console.log(`PASS native reminders ${phase}`);
}catch(e){save('reminder-failure.json',{message:String(e)});throw e;}finally{await browser.close();}
