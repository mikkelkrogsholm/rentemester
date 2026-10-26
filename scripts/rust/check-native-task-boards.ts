// Existing React custom board lifecycle on the common Rust server; synthetic data only.
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
  const created=await post('/api/tasks',{title:'Synthetic board task',scope:{kind:'company',companySlug:'synthetic'},references:[{kind:'document',ref:'synthetic-board-proof'}],idempotencyKey:'board-task-created',confirm:true});const id=created.task.taskId;
  save('board-state.json',{id,created});const writes:any[]=[];page.on('request',r=>{if(r.method()==='POST'&&new URL(r.url()).pathname==='/api/task-boards'){writes.push(r.postDataJSON());save('board-writes.json',writes);}});
  await page.goto(base+'/companies/synthetic/opgaver?view=kanban');await page.getByRole('button',{name:'Tilpas kolonner',exact:true}).click();let dialog=page.getByRole('dialog');await dialog.getByRole('button',{name:'Tilføj kolonne',exact:true}).click();await dialog.getByLabel('Kolonnenavn 5',{exact:true}).fill('Synthetic review');await dialog.getByRole('button',{name:'Vis konsekvenser',exact:true}).click();await dialog.getByRole('button',{name:'Anvend viste ændringer',exact:true}).click();await expect(dialog).toHaveCount(0);
  const first=(await get('/api/task-boards')).boards.find((b:any)=>b.boardId==='company:synthetic');expect(first.version).toBe(1);const custom=first.columns.find((c:any)=>c.name==='Synthetic review');expect(custom.status).toBe('waiting');expect((await get('/api/tasks/'+id)).task.version).toBe(1);
  await page.getByLabel('Flyt Synthetic board task',{exact:true}).selectOption(custom.columnId);await page.getByRole('button',{name:'Flyt opgave',exact:true}).click();await expect.poll(async()=>(await get('/api/tasks/'+id)).task.version).toBe(2);expect((await get('/api/tasks/'+id)).task.columnId).toBe(custom.columnId);
  await page.getByRole('button',{name:'Tilpas kolonner',exact:true}).click();dialog=page.getByRole('dialog');await dialog.getByRole('button',{name:'Fjern Synthetic review',exact:true}).click();await dialog.getByLabel('Flyt opgaver fra Synthetic review til',{exact:true}).selectOption('waiting');await dialog.getByRole('button',{name:'Vis konsekvenser',exact:true}).click();await dialog.getByRole('button',{name:'Anvend viste ændringer',exact:true}).click();await expect(dialog).toHaveCount(0);
  const final=await get('/api/tasks/'+id),board=(await get('/api/task-boards')).boards.find((b:any)=>b.boardId==='company:synthetic');expect(board.version).toBe(2);expect(final.task.version).toBe(3);expect(final.task.columnId).toBe('waiting');expect(final.task.status).toBe('waiting');expect(final.task.references).toEqual(created.task.references);expect(final.history.map((e:any)=>e.operation)).toEqual(['created','moved','board_changed']);expect(writes).toHaveLength(2);
  const args=['--server',base,'--allow-loopback-http','--cookie-file',join(out,'session.cookie')];const cli=spawnSync(join(bin,'rentemester-rust'),['task-boards','list',...args],{input:'{}',encoding:'utf8'});if(cli.status!==0)throw Error('Board CLI: '+cli.stderr);expect(JSON.parse(cli.stdout).data.boards.find((b:any)=>b.boardId==='company:synthetic')).toEqual(board);
  const frames=[{jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'synthetic-boards',version:'1'}}},{jsonrpc:'2.0',method:'notifications/initialized'},{jsonrpc:'2.0',id:2,method:'tools/call',params:{name:'task_boards_preview',arguments:{board:{boardId:board.boardId,scope:board.scope,columns:board.columns}}}}];const mcp=spawnSync(join(bin,'rentemester-mcp'),args,{input:frames.map(f=>JSON.stringify(f)).join('\n')+'\n',encoding:'utf8'});if(mcp.status!==0)throw Error('Board MCP: '+mcp.stderr);const response=mcp.stdout.trim().split('\n').map(s=>JSON.parse(s)).find(r=>r.id===2);expect(response.result.isError).toBe(false);expect(response.result.structuredContent.data.preview.affectedTaskIds).toEqual([]);
  save('board-state.json',{id,created,final,board,writes});await page.screenshot({path:join(out,'board.png'),fullPage:true});save('board-first.json',{ok:true,syntheticOnly:true,browser:['custom column preview/apply','task move','populated column removal preview/relocation','evidence/history retained'],clients:['task-boards list','task_boards_preview'],errors});
 }else{
  const state=JSON.parse(readFileSync(join(out,'board-state.json'),'utf8'));await page.goto(base+'/companies/synthetic/opgaver?view=kanban');await page.getByRole('heading',{name:'Kanban',exact:true}).waitFor();expect(await get('/api/tasks/'+state.id)).toEqual(state.final);expect((await get('/api/task-boards')).boards.find((b:any)=>b.boardId==='company:synthetic')).toEqual(state.board);
  for(const [index,body] of state.writes.entries()){const replay=await post('/api/task-boards',body);expect(replay.board.version).toBe(index+1);expect((await get('/api/tasks/operations/'+body.idempotencyKey)).receipt.version).toBe(index+1);}
  expect(await get('/api/tasks/'+state.id)).toEqual(state.final);expect((await get('/api/tasks')).tasks).toHaveLength(1);save('board-reopen.json',{ok:true,boardVersion:2,taskVersion:3,noDuplicate:true,exactOriginalReceipts:true,errors});
 }
 expect(errors).toEqual([]);await context.close();console.log(`PASS native custom board ${reopen?'restart and exact receipts':'existing React lifecycle and clients'}`);
}catch(e){save('board-failure.json',{message:String(e)});throw e;}finally{await browser.close();}
