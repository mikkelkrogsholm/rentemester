// One real existing React routine journey; called by check-native-tasks.py --routine.
import {chromium,expect} from '@playwright/test';
import {readFileSync,writeFileSync,readdirSync,chmodSync} from 'node:fs';
import {join} from 'node:path';
import {createHmac} from 'node:crypto';
import {spawnSync} from 'node:child_process';
const base=process.env.RM_TASK_ORIGIN!,out=process.env.RM_TASK_OUTPUT!,bin=process.env.RM_TASK_BINARY_DIR!,reopen=process.env.RM_TASK_PHASE==='reopen';
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
 if(!reopen){
  const token=new URLSearchParams(new URL(process.env.RM_TASK_SETUP!).hash.slice(1)).get('setup')!;
  const owner=await page.request.post(base+'/api/setup/first-owner',{headers:{...headers,'x-rentemester-install-token':token},data:{name:'Synthetic Task Owner',email:'owner@example.invalid',password:'synthetic-password-123'}});expect(owner.ok()).toBe(true);
  const root=process.env.RM_TASK_WORKSPACE!;if(!root.includes('Rentemester-Synthetic-Tasks-'))throw Error('Only disposable fixture allowed');
  const handoff=readdirSync(join(root,'.rentemester')).find(s=>s.startsWith('verification-')&&s.endsWith('.txt'))!;
  const verified=await page.request.get(readFileSync(join(root,'.rentemester',handoff),'utf8').trim());expect(verified.ok()).toBe(true);
  const enrollment=await post('/api/auth/mfa-enrollment/begin',{email:'owner@example.invalid',password:'synthetic-password-123'});
  await post('/api/auth/mfa-enrollment/confirm',{code:totp(enrollment.totpURI)});
  await context.storageState({path:join(out,'session.json')});chmodSync(join(out,'session.json'),0o600);
  writeFileSync(join(out,'session.cookie'),(await context.cookies(base)).map(c=>`${c.name}=${c.value}`).join('; '),{mode:0o600});
  const today=new Date().toISOString().slice(0,10),year=today.slice(0,4),offset=Number(today.slice(8))-1;
  let saveBody:any,materializeBody:any;
  page.on('request',r=>{const path=new URL(r.url()).pathname;if(r.method()==='POST'&&path==='/api/task-series'&&!saveBody)saveBody=r.postDataJSON();if(r.method()==='POST'&&path==='/api/task-series/materialize'&&!materializeBody)materializeBody=r.postDataJSON();});
  await page.goto(base+'/opgaver?view=wheel');await page.getByRole('button',{name:'Opret rutine',exact:true}).click();let dialog=page.getByRole('dialog');
  await dialog.getByLabel('Rutinens titel',{exact:true}).fill('Synthetic native routine');await dialog.getByLabel('Opgavens selskab eller fælles scope',{exact:true}).selectOption('synthetic');
  await dialog.getByLabel('Starter',{exact:true}).fill(today);await dialog.getByLabel('Arbejdsdato: dage efter periodens start',{exact:true}).fill(String(offset));await dialog.getByLabel('Kræv afslutningsbevis',{exact:true}).check();
  await dialog.getByRole('button',{name:'Gem rutine',exact:true}).click();await expect(dialog).toHaveCount(0);expect(saveBody).toBeTruthy();
  const initial=await get('/api/task-series');expect(initial.series).toHaveLength(1);const seriesId=initial.series[0].seriesId;
  const projected=await get(`/api/task-series/project?from=${year}-01-01&to=${year}-12-31`);expect(projected.projections.length).toBeGreaterThan(0);expect(projected.projections.every((p:any)=>p.concreteTaskId===null)).toBe(true);
  await page.getByRole('group',{name:'Årshjul',exact:true}).waitFor();expect((await get('/api/tasks')).tasks).toHaveLength(0);
  await page.getByRole('button',{name:'Opret aktuelle forekomster',exact:true}).click();await expect.poll(async()=>(await get('/api/tasks')).tasks.length).toBe(1);expect(materializeBody).toBeTruthy();
  const current=(await get('/api/tasks')).tasks[0],id=current.taskId;expect(current.seriesId).toBe(seriesId);
  const repeated=await post('/api/task-series/materialize',{...materializeBody,idempotencyKey:'routine-repeat-check'});expect(repeated.materialized.created).toBe(0);expect(repeated.materialized.existing).toBe(1);
  await page.goto(base+'/opgaver/'+encodeURIComponent(id));await page.getByRole('button',{name:'Afslut med dokumentation',exact:true}).click();dialog=page.getByRole('dialog');await dialog.getByLabel('Afslutningsnote eller begrundelse').fill('Synthetic routine evidence checked');await dialog.getByLabel('Sikre referencer').fill('document:synthetic-routine-proof');await dialog.getByRole('button',{name:'Gem afslutning',exact:true}).click();await expect(dialog).toHaveCount(0);
  const concrete=await get('/api/tasks/'+id);expect(concrete.task.status).toBe('done');expect(concrete.history).toHaveLength(2);
  await page.goto(base+'/opgaver?view=wheel');await page.getByRole('button',{name:'Redigér eller pausér fremtidige gentagelser',exact:true}).click();dialog=page.getByRole('dialog');await dialog.getByLabel('Rutinens titel',{exact:true}).fill('Synthetic future revision');await dialog.getByLabel('Arbejdsdato: dage efter periodens start',{exact:true}).fill(String(offset+1));await dialog.getByRole('button',{name:'Gem rutine',exact:true}).click();await expect(dialog).toHaveCount(0);
  expect((await get('/api/task-series')).series[0].version).toBe(2);expect(await get('/api/tasks/'+id)).toEqual(concrete);
  await page.getByRole('button',{name:'Redigér eller pausér fremtidige gentagelser',exact:true}).click();dialog=page.getByRole('dialog');await dialog.getByLabel('Aktiv gentagelse',{exact:true}).uncheck();await dialog.getByRole('button',{name:'Gem rutine',exact:true}).click();await expect(dialog).toHaveCount(0);
  const finalSeries=(await get('/api/task-series')).series[0];expect(finalSeries.active).toBe(false);expect(finalSeries.version).toBe(3);expect(await get('/api/tasks/'+id)).toEqual(concrete);
  const args=['--server',base,'--allow-loopback-http','--cookie-file',join(out,'session.cookie')];const cookie=(await context.cookies(base)).map(c=>`${c.name}=${c.value}`).join('; ');writeFileSync(join(out,'session.cookie'),cookie,{mode:0o600});
  const cli=spawnSync(join(bin,'rentemester-rust'),['task-series','list',...args],{input:'{}',encoding:'utf8'});if(cli.status!==0)throw Error('Routine CLI: '+cli.stderr);expect(JSON.parse(cli.stdout).data.series[0]).toEqual(finalSeries);
  const frames=[{jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'synthetic-routine',version:'1'}}},{jsonrpc:'2.0',method:'notifications/initialized'},{jsonrpc:'2.0',id:2,method:'tools/call',params:{name:'task_series_project',arguments:{from:`${year}-01-01`,to:`${year}-12-31`}}}];
  const mcp=spawnSync(join(bin,'rentemester-mcp'),args,{input:frames.map(f=>JSON.stringify(f)).join('\n')+'\n',encoding:'utf8'});if(mcp.status!==0)throw Error('Routine MCP: '+mcp.stderr);const response=mcp.stdout.trim().split('\n').map(s=>JSON.parse(s)).find(r=>r.id===2);expect(response.result.isError).toBe(false);expect(response.result.structuredContent.data.projections[0].concreteTaskId).toBe(id);
  await context.storageState({path:join(out,'session.json')});chmodSync(join(out,'session.json'),0o600);save('routine-state.json',{id,seriesId,saveBody,materializeBody,concrete,finalSeries,year});await page.screenshot({path:join(out,'routine.png'),fullPage:true});save('routine-first.json',{ok:true,syntheticOnly:true,browser:['routine save','calendar projection','explicit materialization','documented completion','future revision','pause'],clients:['original task-series list','task_series_project'],errors});
 }else{
  const state=JSON.parse(readFileSync(join(out,'routine-state.json'),'utf8'));await page.goto(base+'/opgaver?view=wheel');await page.getByRole('heading',{name:'Synthetic future revision',exact:true}).waitFor();expect(await get('/api/tasks/'+state.id)).toEqual(state.concrete);expect((await get('/api/task-series')).series[0]).toEqual(state.finalSeries);
  expect((await post('/api/task-series',state.saveBody)).series.version).toBe(1);const replay=await post('/api/task-series/materialize',state.materializeBody);expect(replay.materialized.created).toBe(1);expect(replay.materialized.taskIds).toEqual([state.id]);
  expect(await get('/api/tasks/'+state.id)).toEqual(state.concrete);expect((await get('/api/tasks?showDone=true')).tasks).toHaveLength(1);expect((await get('/api/tasks/operations/'+state.saveBody.idempotencyKey)).receipt.version).toBe(1);expect((await get('/api/tasks/operations/'+state.materializeBody.idempotencyKey)).receipt.operation).toBe('series-materialize');save('routine-reopen.json',{ok:true,seriesVersion:3,concreteTaskVersion:2,noDuplicate:true,exactReceiptReplay:true,errors});
 }
 expect(errors).toEqual([]);await context.close();console.log(`PASS native routine ${reopen?'restart and exact receipts':'existing React lifecycle'}`);
}catch(e){save('routine-failure.json',{message:String(e)});throw e;}finally{await browser.close();}
