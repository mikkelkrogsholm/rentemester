// Existing React authoritative source lifecycle on the common Rust server; synthetic data only.
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

  const today=(await get('/api/portfolio')).portfolio.asOf,year=today.slice(0,4),from=year+'-07-01',to=year+'-09-30';
  const profile=await page.request.patch(base+'/api/companies/synthetic/company',{headers,data:{cvr:'87654321',address:'Synthetic Sources Street 1',postalCode:'1000',city:'Synthetic City',vatPeriodType:'quarter',paymentTermsDays:14}});if(!profile.ok())throw Error('Profile: '+await profile.text());
  for(const [role,accountNo,type,normalBalance] of [['bank','1712','asset','debit'],['debtors','1713','asset','debit'],['creditors','7012','liability','credit']]){await post('/api/companies/synthetic/accounts',{accountNo,name:'Synthetic source '+role,type,normalBalance});await post('/api/companies/synthetic/account-roles',{role,accountNo,confirm:true});}
  const results:any[]=[];page.on("response",async r=>{if(r.request().method()==="POST"&&new URL(r.url()).pathname==="/api/tasks/sync"&&r.ok()){results.push(await r.json());save("source-results.json",results);}});const writes:any[]=[];page.on('request',r=>{if(r.method()==='POST'&&new URL(r.url()).pathname==='/api/tasks/sync'){writes.push(r.postDataJSON());save('source-writes.json',writes);}});
  await page.goto(base+'/companies/synthetic/opgaver');await page.getByRole('button',{name:'Opdatér fra kilder',exact:true}).click();
  await expect.poll(async()=>(await get('/api/tasks')).tasks.filter((t:any)=>t.source?.kind==='vat_filing').length).toBeGreaterThan(0);
  const task=(await get('/api/tasks')).tasks.find((t:any)=>t.source?.kind==='vat_filing'&&t.source.ref===from+':'+to);expect(task).toBeTruthy();expect(task.status).toBe('open');const id=task.taskId;
  save('source-progress.json',{stage:'discovered',id,task,writes});
  await page.goto(base+'/opgaver/'+encodeURIComponent(id));await page.getByRole('button',{name:'Afslut med dokumentation',exact:true}).click();let dialog=page.getByRole('dialog');await dialog.getByLabel('Afslutningsnote eller begrundelse').fill('Synthetic attempted completion before any recorded VAT result');await dialog.getByLabel('Sikre referencer').fill('external_receipt:synthetic-not-filed');
  const refusal=page.waitForResponse(r=>r.request().method()==='POST'&&r.url().includes('/complete'));await dialog.getByRole('button',{name:'Gem afslutning',exact:true}).click();const refused=await refusal;expect(refused.status()).toBe(422);const refusedBody=await refused.json();expect((await get('/api/tasks/'+id)).task).toEqual(task);save('source-refusal.json',{status:refused.status(),body:refusedBody,unchanged:true});
  await page.goto(base+'/companies/synthetic/opgaver');
  function period(op:string,flags:Record<string,string|number>){const r=spawnSync(join(bin,'rentemester-rust'),['period',op,'--server',base,'--allow-loopback-http','--cookie-file',join(out,'session.cookie'),'--company','synthetic','--json',...Object.entries(flags).flatMap(([k,v])=>['--'+k,String(v)])],{encoding:'utf8',timeout:45000});if(r.status!==0)throw Error('Period '+op+': '+r.stdout+' '+r.stderr);return JSON.parse(r.stdout);}
  const packet=period('readiness',{from,to}).packet;expect(packet.blockers).toBe(0);const review=period('review',{from,to,'packet-hash':packet.hash,confirm:'yes'});const flags={from,to,'review-id':review.id,'packet-hash':review.packet.hash,confirm:'yes'};
  const closed=period('close',{...flags,status:'closed'});expect(closed.ok).toBe(true);save('source-progress.json',{stage:'closed',id,task,writes,review,closed});
  await page.getByRole('button',{name:'Opdatér fra kilder',exact:true}).click();await expect.poll(()=>writes.length).toBe(2);await expect.poll(async()=>(await get('/api/tasks/operations/'+writes[1].idempotencyKey)).receipt).not.toBeNull();expect((await get('/api/tasks/'+id)).task.status).toBe('open');
  const recorded=period('close',{...flags,status:'reported',reference:'SYNTHETIC-RECORDED-VAT-REFERENCE'});expect(recorded.ok).toBe(true);save('source-progress.json',{stage:'recorded',id,task,writes,review,closed,recorded});
  await page.getByRole('button',{name:'Opdatér fra kilder',exact:true}).click();await expect.poll(async()=>(await get('/api/tasks/'+id)).task.status).toBe('done');
  const final=await get('/api/tasks/'+id);expect(final.task.completion.assurance).toBe('product_verified');expect(final.task.completion.references.some((r:any)=>r.kind==='period'&&r.ref===from+':'+to)).toBe(true);expect(final.history.map((e:any)=>e.operation)).toEqual(['source_created','source_updated','completed']);
  await page.goto(base+'/opgaver/'+encodeURIComponent(id));await expect(page.getByText(/Verificeret ved produktkilden/)).toBeVisible();await page.screenshot({path:join(out,'source.png'),fullPage:true});
  const args=['--server',base,'--allow-loopback-http','--cookie-file',join(out,'session.cookie')];const clientInput={companySlugs:['synthetic'],asOfDate:today,idempotencyKey:'source-cli-sync'};const cli=spawnSync(join(bin,'rentemester-rust'),['tasks','sync',...args,'--confirm'],{input:JSON.stringify(clientInput),encoding:'utf8'});if(cli.status!==0)throw Error('Source CLI: '+cli.stderr+' '+cli.stdout);expect(JSON.parse(cli.stdout).data.sync.created).toBe(0);
  const frames=[{jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'synthetic-sources',version:'1'}}},{jsonrpc:'2.0',method:'notifications/initialized'},{jsonrpc:'2.0',id:2,method:'tools/call',params:{name:'tasks_sync',arguments:{...clientInput,idempotencyKey:'source-mcp-sync',confirm:true}}}];const mcp=spawnSync(join(bin,'rentemester-mcp'),args,{input:frames.map(f=>JSON.stringify(f)).join('\n')+'\n',encoding:'utf8'});if(mcp.status!==0)throw Error('Source MCP: '+mcp.stderr);const response=mcp.stdout.trim().split('\n').map(s=>JSON.parse(s)).find(r=>r.id===2);expect(response.result.isError).toBe(false);expect(response.result.structuredContent.data.sync.created).toBe(0);
  await expect.poll(()=>results.length).toBe(3);const actual=await get('/api/tasks/'+id);expect(actual).toEqual(final);const count=(await get('/api/tasks')).count;save('source-state.json',{id,final,count,writes,results,clientInput,closed,recorded});save('source-first.json',{ok:true,syntheticOnly:true,browser:['native discovery','unresolved completion refused','closed is not reported','recorded reference updates actual source','product_verified with native evidence'],clients:['tasks sync','tasks_sync'],errors});
 }else{
  const state=JSON.parse(readFileSync(join(out,'source-state.json'),'utf8'));await page.goto(base+'/opgaver/'+encodeURIComponent(state.id));await expect(page.getByText(/Verificeret ved produktkilden/)).toBeVisible();expect(await get('/api/tasks/'+state.id)).toEqual(state.final);
  for(const [index,body] of state.writes.entries()){const first=await get('/api/tasks/operations/'+body.idempotencyKey);expect(first.receipt).not.toBeNull();expect(await post('/api/tasks/sync',body)).toEqual(state.results[index]);}
  expect(await get('/api/tasks/'+state.id)).toEqual(state.final);expect((await get('/api/tasks')).count).toBe(state.count);save('source-reopen.json',{ok:true,noDuplicate:true,exactOriginalReceipts:true,persistentProductEvidence:true,errors});
 }
 expect(errors).toEqual([]);await context.close();console.log(`PASS native task sources ${reopen?'restart and exact receipts':'existing React authoritative lifecycle and clients'}`);
}catch(e){save('source-failure.json',{message:String(e)});throw e;}finally{await browser.close();}
