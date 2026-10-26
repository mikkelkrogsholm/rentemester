// Existing React bank import/reconciliation on the common Rust server; synthetic data only.
import {chromium,expect} from '@playwright/test';
import {readFileSync,writeFileSync,readdirSync,chmodSync,existsSync} from 'node:fs';
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


 }
 const bankPath='/api/companies/synthetic/bank?year=2026';
 if(phase==='first'){
  const csv='date,text,amount,currency,balance_after\r\n2026-10-24,final,-100,DKK,100\r\n2026-10-24,middle,100,DKK,200\r\n2026-10-24,opening,1,DKK,100\r\n';
  writeFileSync(join(out,'statement.csv'),csv,{mode:0o600});
  await page.goto(base+'/companies/synthetic/bank');await page.getByRole('button',{name:'Importér kontoudtog',exact:true}).click();
  const dialog=page.getByRole('dialog',{name:'Importér kontoudtog',exact:true});
  await dialog.locator('input[type=file]').setInputFiles({name:'synthetic-statement.csv',mimeType:'text/csv',buffer:Buffer.from(csv)});
  await dialog.getByRole('combobox',{name:'Rækkefølge i kontoudtoget (valgfri)',exact:true}).selectOption('descending');
  const response=page.waitForResponse(r=>r.url().endsWith('/api/companies/synthetic/bank/import')&&r.request().method()==='POST');
  save('bank-import-intent.json',{newOwnedSyntheticImport:true,statementOrder:'descending'});
  await dialog.getByRole('button',{name:'Importér',exact:true}).click();const imported=await response;expect(imported.ok()).toBe(true);save('bank-import-response.json',await imported.json());
  await expect(dialog.getByText(/3 transaktioner importeret/)).toBeVisible();await dialog.getByRole('button',{name:'Luk',exact:true}).click();
  await page.reload();const before=await get(bankPath);expect(before.bank.actualBalance).toBe(100);expect(before.bank.difference).toBe(-100);expect(before.bank.transactions).toHaveLength(3);expect(before.bank.transactions.every((r:any)=>r.statementOrder==='descending')).toBe(true);
  save('bank-before-clients.json',before);
 }
 if(phase==='first'||phase==='clients'){
  const args=['--server',base,'--allow-loopback-http','--cookie-file',join(out,'session.cookie')];
  if(!existsSync(join(out,'bank-cli.json'))){
   const cli=spawnSync(join(bin,'rentemester-rust'),['bank','import','--company','synthetic','--file',join(out,'statement.csv'),'--statement-order','ascending','--json',...args],{encoding:'utf8'});if(cli.status!==0)throw Error('Bank CLI: '+cli.stderr+' '+cli.stdout);const result=JSON.parse(cli.stdout);expect(result.imported).toBe(0);expect(result.skippedDuplicates).toBe(3);save('bank-cli.json',{ok:true,imported:result.imported,skippedDuplicates:result.skippedDuplicates});
  }
  if(!existsSync(join(out,'bank-mcp.json'))){
   const frames=[{jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'synthetic-bank-order',version:'1'}}},{jsonrpc:'2.0',method:'notifications/initialized'},{jsonrpc:'2.0',id:2,method:'tools/call',params:{name:'bank_import',arguments:{company:'synthetic',csvPath:join(out,'statement.csv'),statementOrder:'ascending',confirm:true}}},{jsonrpc:'2.0',id:3,method:'tools/call',params:{name:'bank_import',arguments:{company:'synthetic',csvContent:readFileSync(join(out,'statement.csv'),'utf8'),statementOrder:'ascending',confirm:true}}}];
   const mcp=spawnSync(join(bin,'rentemester-mcp'),args,{input:frames.map(f=>JSON.stringify(f)).join('\n')+'\n',encoding:'utf8'});if(mcp.status!==0)throw Error('Bank MCP: '+mcp.stderr);const replies=mcp.stdout.trim().split('\n').map(s=>JSON.parse(s));for(const id of [2,3]){const r=replies.find(r=>r.id===id);expect(r.result.isError).toBe(false);expect(r.result.structuredContent.data.imported).toBe(0);expect(r.result.structuredContent.data.skippedDuplicates).toBe(3);expect(JSON.parse(r.result.content[0].text)).toEqual(r.result.structuredContent);}save('bank-mcp.json',{ok:true,sourceForms:['path','inline'],imported:0,skippedDuplicates:3});
  }
  const before=JSON.parse(readFileSync(join(out,'bank-before-clients.json'),'utf8'));const after=await get(bankPath);expect(after).toEqual(before);
  await page.goto(base+'/companies/synthetic/bank');await expect(page.getByRole('heading',{name:'Faktisk saldo',exact:true})).toBeVisible();
  save('bank-order-first.json',{ok:true,ordinaryOwnerVerificationMfa:true,existingReactRawFileImport:true,visibleStatementBalance:100,bookedBalance:0,difference:-100,cliAndMcpOppositeOrderNoRewrite:true,transactions:3,errors});
 }else{
  const before=JSON.parse(readFileSync(join(out,'bank-before-clients.json'),'utf8'));expect(await get(bankPath)).toEqual(before);await page.goto(base+'/companies/synthetic/bank');await expect(page.getByRole('heading',{name:'Faktisk saldo',exact:true})).toBeVisible();save('bank-order-reopen.json',{ok:true,exactPersistedStatementSourceOrderAndBalances:true,transactions:3,errors});
 }
 expect(errors).toEqual([]);await context.close();console.log(`PASS native bank source-order ${phase}`);
}catch(e){save('bank-order-failure.json',{message:String(e)});throw e;}finally{await browser.close();}
