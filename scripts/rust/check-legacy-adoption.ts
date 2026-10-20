/** Normal web/MFA accounting and source-free recovery for the paired owned runner. */
import assert from 'node:assert/strict';
import {createHash,createHmac} from 'node:crypto';
import {existsSync,lstatSync,readdirSync,readFileSync,writeFileSync} from 'node:fs';
import {basename,join,resolve} from 'node:path';
import {chromium} from '@playwright/test';

const work=resolve(process.env.RM_ADOPTION_WORK??''),base=process.env.RM_ADOPTION_BASE??'',phase=process.env.RM_ADOPTION_PHASE??'';
const meta=lstatSync(work);assert(basename(work).startsWith('Rentemester-Synthetic-Adoption-')&&meta.isDirectory()&&!meta.isSymbolicLink()&&(meta.mode&0o077)===0&&meta.uid===process.getuid?.());
assert.match(base,/^http:\/\/127\.0\.0\.1:\d+$/);assert(['source','source-reopen','restored','restored-reopen'].includes(phase));
const config=JSON.parse(readFileSync(join(work,'server.json'),'utf8')),corpus=JSON.parse(readFileSync(join(work,'legacy-corpus.json'),'utf8'));
assert(['synthetic-adoption-source','synthetic-adoption-restored'].includes(config.workspace));assert.equal(config.companySlug,'synthetic');
assert.equal(config.root,join(work,phase.startsWith('source')?'adopted':'restored'));
const browser=await chromium.launch({headless:true}),page=await browser.newPage();page.setDefaultTimeout(45000);
const authFile=join(work,`auth-${phase.startsWith('source')?'source':'restored'}.json`),headers={origin:base,'content-type':'application/json'};
const company=(route:string)=>`${base}/api/companies/synthetic/${route}`;
const digest=(raw:Buffer)=>createHash('sha256').update(raw).digest('hex');
async function get(route:string){const r=await page.request.get(company(route));assert(r.ok(),`${route}: ${r.status()} ${await r.text()}`);return r.json();}
async function post(route:string,data:unknown){const r=await page.request.post(company(route),{headers,data});assert(r.ok(),`${route}: ${r.status()} ${await r.text()}`);return r.json();}
function totp(secret:string){let bits=0,count=0;const bytes:number[]=[];for(const c of secret){const v='ABCDEFGHIJKLMNOPQRSTUVWXYZ234567'.indexOf(c);assert(v>=0);bits=(bits<<5)|v;count+=5;if(count>=8){count-=8;bytes.push((bits>>>count)&255);}}const counter=Buffer.alloc(8);counter.writeBigUInt64BE(BigInt(Math.floor(Date.now()/30000)));const d=createHmac('sha1',Buffer.from(bytes)).update(counter).digest(),offset=d[19]&15;return String((d.readUInt32BE(offset)&0x7fffffff)%1000000).padStart(6,'0');}
async function login(){
 const state=JSON.parse(readFileSync(authFile,'utf8'));const code=state.codes.shift();assert(code);writeFileSync(authFile,JSON.stringify(state),{mode:0o600});
 await page.goto(base);await page.getByLabel('E-mail',{exact:true}).fill(config.firstOwner.email);await page.getByLabel('Adgangskode',{exact:true}).fill(config.firstOwner.password);await page.getByRole('button',{name:'Log ind',exact:true}).click();await page.getByRole('button',{name:'Brug recovery code'}).click();await page.getByLabel('Recovery code',{exact:true}).fill(code);await page.getByRole('button',{name:'Bekræft',exact:true}).click();await page.getByRole('link',{name:'Åbn bogføring'}).first().waitFor();
}
async function enroll(){
 const spool=join(config.root,'.rentemester'),files=readdirSync(spool).filter(n=>n.startsWith('verification-')&&n.endsWith('.txt'));assert.equal(files.length,1);
 const file=join(spool,files[0]),info=lstatSync(file);assert(info.isFile()&&!info.isSymbolicLink()&&(info.mode&0o077)===0&&info.uid===process.getuid?.());const link=readFileSync(file,'utf8').trim();assert.equal(new URL(link).origin,base);
 await page.goto(link);await page.goto(base);await page.getByLabel('E-mail',{exact:true}).fill(config.firstOwner.email);await page.getByLabel('Adgangskode',{exact:true}).fill(config.firstOwner.password);await page.getByRole('button',{name:'Log ind',exact:true}).click();await page.getByRole('heading',{name:'Opsæt totrinsbekræftelse'}).waitFor();await page.getByLabel('Adgangskode',{exact:true}).fill(config.firstOwner.password);await page.getByRole('button',{name:'Start opsætning',exact:true}).click();const secret=await page.getByLabel('Opsætningsnøgle').inputValue();await page.getByLabel('Engangskode',{exact:true}).fill(totp(secret));await page.getByRole('button',{name:'Bekræft opsætning',exact:true}).click();await page.getByRole('region',{name:'Recovery codes'}).locator('li').first().waitFor();const codes=await page.getByRole('region',{name:'Recovery codes'}).locator('li').allTextContents();assert(codes.length>=4);writeFileSync(authFile,JSON.stringify({codes}),{mode:0o600,flag:'wx'});await page.getByRole('button',{name:'Jeg har gemt koderne',exact:true}).click();await page.getByRole('link',{name:'Åbn bogføring'}).first().waitFor();await page.context().clearCookies();await new Promise(r=>setTimeout(r,11000));await login();
}
async function snapshot(){const journal=(await get('journal')).journal;return{uuid:journal.ledgerUuid,entries:journal.entries,balance:(await get(`balance?year=${corpus.date.slice(0,4)}`)).balance,documents:await get('documents'),invoices:await get('invoices')};}
async function originals(){
 const active=corpus.companies.find((c:{slug:string})=>c.slug==='synthetic');assert(active);
 for(const original of active.originals){const r=await page.request.get(company(`documents/${original.documentId}/file`));assert(r.ok());assert.equal(digest(await r.body()),original.sha256);}
 const pdf=await page.request.get(company('invoices/1/pdf'));assert(pdf.ok());assert.equal(digest(await pdf.body()),active.originals.find((o:{sourcePath:string})=>o.sourcePath.endsWith('.pdf')).sha256);
 const archived=await page.request.get(`${base}/api/companies/synthetic-archived/journal`);assert(!archived.ok(),'Archived ledger must not become ordinary request target');
}
try{
 if(phase.endsWith('-reopen')||existsSync(authFile))await login();else await enroll();
 await originals();
 const savedPath=join(work,'native-source.json'),restoredPath=join(work,'native-restored.json');
 if(phase==='source'){
  const before=await snapshot();assert.equal(before.entries.length,1);assert.equal(before.uuid,corpus.companies[0].ledgerUuid);
  const journal=(await get('journal')).journal;
  const input={confirm:true,input:{format:'rentemester-manual-journal-post/1',expectedRuleVersion:journal.nativePosting.ruleVersion,idempotencyKey:'synthetic-adoption-native-capital',request:{transactionDate:corpus.date,text:'Synthetic native capital after TS adoption',currency:'DKK',lines:[{accountNo:'2000',debitAmount:'10.00',creditAmount:'0.00'},{accountNo:'5000',debitAmount:'0.00',creditAmount:'10.00'}]}}};
  await post('manual-journal',input);const after=await snapshot();assert.equal(after.entries.length,2);for(const old of before.entries)assert.deepEqual(after.entries.find((e:{id:number})=>e.id===old.id),old);
  await post('manual-journal',input);assert.deepEqual(await snapshot(),after);
  writeFileSync(savedPath,JSON.stringify({snapshot:after,sourceRequest:input},null,2),{mode:0o600,flag:'wx'});
 }else if(phase==='source-reopen'){
  const saved=JSON.parse(readFileSync(savedPath,'utf8'));assert.deepEqual(await snapshot(),saved.snapshot);
  await post('manual-journal',saved.sourceRequest);assert.deepEqual(await snapshot(),saved.snapshot);
  await page.goto(`${base}/companies/synthetic/integritet`);const card=page.locator('.native-backup');await card.getByLabel('Jeg vil hente en backup af hele workspacet.',{exact:true}).check();
  const [download,response]=await Promise.all([page.waitForEvent('download'),page.waitForResponse(r=>new URL(r.url()).pathname==='/api/workspace/backup'&&r.request().method()==='POST'),card.getByRole('button',{name:'Dan og download backup',exact:true}).click()]);assert(response.ok(),`Backup HTTP${response.status()}: ${await response.text()}`);assert.equal(response.headers()['content-type'],'application/vnd.rentemester.native-backup+json');await download.saveAs(join(work,'portable.native-backup.json'));await card.getByRole('status').filter({hasText:'Download startet.'}).waitFor();assert.deepEqual(await snapshot(),saved.snapshot,'Backup must preserve data and return owner to original active selection');
 }else if(phase==='restored'){
  const saved=JSON.parse(readFileSync(savedPath,'utf8'));assert.deepEqual(await snapshot(),saved.snapshot);
  const replay=await page.request.post(company('manual-journal'),{headers,data:saved.sourceRequest});assert.equal(replay.status(),400);assert.equal((await replay.json()).code,'RESTORED_KEY_RECONCILIATION_REQUIRED');assert.deepEqual(await snapshot(),saved.snapshot,'Old source intent must not post again after source-free restore');
  const fresh=structuredClone(saved.sourceRequest);fresh.input.idempotencyKey='synthetic-adoption-restored-fresh-capital';fresh.input.request.text='Synthetic fresh accounting after source-free restore';await post('manual-journal',fresh);const after=await snapshot();assert.equal(after.entries.length,3);for(const old of saved.snapshot.entries)assert.deepEqual(after.entries.find((e:{id:number})=>e.id===old.id),old);writeFileSync(restoredPath,JSON.stringify({snapshot:after,request:fresh},null,2),{mode:0o600,flag:'wx'});
 }else{
  const saved=JSON.parse(readFileSync(restoredPath,'utf8'));assert.deepEqual(await snapshot(),saved.snapshot);await post('manual-journal',saved.request);assert.deepEqual(await snapshot(),saved.snapshot);
 }
 writeFileSync(join(work,`${phase}-pass.json`),JSON.stringify({ok:true,phase,normalFreshPasswordAndMfa:true,originalsExact:true,ordinaryArchivedSelectionRefused:true,nativeJournalCount:(await snapshot()).entries.length}),{mode:0o600,flag:'wx'});
 console.log(`PASS: ${phase} normal MFA, preserved TS invoices/originals, exactly-once native accounting and recovery boundary`);
}catch(cause){
 writeFileSync(join(work,`${phase}-failure.txt`),await page.locator('body').innerText(),{mode:0o600});
 await page.screenshot({path:join(work,`${phase}-failure.png`),fullPage:true});
 throw cause;
}finally{await browser.close();}
