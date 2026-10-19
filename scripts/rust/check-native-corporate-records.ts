import type {Page} from '@playwright/test';
import {createHash} from 'node:crypto';
import {mkdtempSync,readFileSync,writeFileSync,mkdirSync,rmSync} from 'node:fs';
import {join} from 'node:path';
import {isDeepStrictEqual} from 'node:util';
const saved='/tmp/rentemester-rust-reports/native-corporate-records.json';
type Evidence={originalId:string;replacementId:string;original:string;replacement:string;records:unknown[]};
/** Real authenticated synthetic UI/API journey; own record snapshots only, no global list baseline. */
export async function checkNativeCorporateRecords(page:Page,base:string,restarted=false){
 if(!/^http:\/\/127\.0\.0\.1:\d+$/.test(base))throw Error('Owned synthetic loopback fixture required');
 const url=`${base}/api/companies/synthetic`,headers={origin:base,'content-type':'application/json'};
 const get=async(path:string)=>{const r=await page.request.get(`${url}/${path}`);if(!r.ok())throw Error(`Corporate canonical read refused ${r.status()}`);return r.json();};
 const post=async(path:string,data:unknown,status=200)=>{const r=await page.request.post(`${url}/${path}`,{headers,data});if(r.status()!==status)throw Error(`Corporate mutation ${path} expected${status}, received${r.status()}`);return r.json();};
 const journal=await get('journal');let evidence:Evidence;
 if(!restarted){
  const original='Synthetic browser corporate immutable original\n',replacement='Synthetic browser corporate corrected original\n';
  await page.goto(`${base}/companies/synthetic/workspace-register`);await page.getByText('Indlæs og vedligehold records',{exact:true}).click();
  const form=page.getByRole('form',{name:'Indlæs corporate record',exact:true});
  async function upload(name:string,content:string){await form.getByLabel('Corporate record fil',{exact:true}).setInputFiles({name,mimeType:'text/plain',buffer:Buffer.from(content)});await form.getByLabel('Corporate record type',{exact:true}).selectOption('articles');if(!(await form.getByLabel('Bekræft corporate record upload',{exact:true}).isChecked()))await form.getByLabel('Bekræft corporate record upload',{exact:true}).check();const received=page.waitForResponse(r=>r.url()===`${url}/corporate-records`&&r.request().method()==='POST');await form.getByRole('button',{name:'Indlæs immutable record',exact:true}).click();const r=await received;if(!r.ok())throw Error('Corporate UI original refused');return(await r.json()).record.recordId as string;}
  const originalId=await upload('synthetic-corporate-browser-original.txt',original),replacementId=await upload('synthetic-corporate-browser-replacement.txt',replacement);
  const party=(await post('workspace-parties',{partyId:'party-corporate-browser',kind:'organization',name:'Synthetic Corporate Browser Party',role:'supplier',source:'synthetic-governance-source',observedAt:'2000-01-01',reviewAssertion:'Synthetic owner reviewed source',confirm:true})).party.partyId;
  async function action(name:string,body:unknown){const f=page.getByRole('form',{name,exact:true});await f.getByLabel(`${name} JSON`,{exact:true}).fill(JSON.stringify(body));await f.getByRole('checkbox').check();const done=page.waitForResponse(r=>r.url().includes(`/corporate-records/${originalId}/`)&&r.request().method()==='POST');await f.getByRole('button',{name,exact:true}).click();if(!(await done).ok())throw Error('Corporate reviewed UI action refused');}
  await action('Knyt record',{recordId:originalId,type:'party',id:party});await action('Berig metadata',{recordId:originalId,assertion:'Synthetic reviewed metadata'});await action('Supersedér record',{recordId:originalId,replacementRecordId:replacementId,reason:'Synthetic corrected source'});
  const input={recordId:originalId,type:'articles',bytesBase64:Buffer.from(original).toString('base64'),filename:'synthetic-corporate-browser-original.txt',source:'synthetic',receivedAt:'2026-01-01',uploader:'synthetic',confirm:true};
  const before=await get(`corporate-records/${originalId}`);
  await post('corporate-records',{...input,recordId:'record-browser-corporate-duplicate'},422);await post('corporate-records',{...input,bytesBase64:Buffer.from('changed').toString('base64')},422);await post('corporate-records',{...input,confirm:false},400);await post(`corporate-records/${originalId}/enrich`,{recordId:originalId,assertion:'Forged',actor:'user:forged',confirm:true},400);await post(`corporate-records/${originalId}/supersede`,{replacementRecordId:'missing-synthetic-record',reason:'missing',confirm:true},404);
  if(!isDeepStrictEqual(await get(`corporate-records/${originalId}`),before))throw Error('Refused corporate writes changed history');
  const records=[before,await get(`corporate-records/${replacementId}`)];
  const me=await(await page.request.get(`${base}/api/me`)).json();const history=before.record.history as Array<{event_type:string;actor:string}>;
  for(const event of ['ingested','linked','enriched','superseded'])if(!history.some(h=>h.event_type===event&&h.actor===`user:${me.user.id}`))throw Error('Corporate original actor/history missing');
  evidence={originalId,replacementId,original,replacement,records};mkdirSync('/tmp/rentemester-rust-reports',{recursive:true});writeFileSync(saved,JSON.stringify(evidence),{mode:0o600});
 }else evidence=JSON.parse(readFileSync(saved,'utf8'));
 const temp=mkdtempSync('/tmp/rm-corporate-browser-');
 try{
  for(const[index,id,source]of [[0,evidence.originalId,evidence.original],[1,evidence.replacementId,evidence.replacement]]as const){
   const canonical=await get(`corporate-records/${id}`);if(!isDeepStrictEqual(canonical,evidence.records[index]))throw Error('Corporate immutable metadata/history changed after restart');
   const bytes=Buffer.from((await get(`corporate-records/${id}/download`)).bytesBase64,'base64');if(!bytes.equals(Buffer.from(source))||createHash('sha256').update(bytes).digest('hex')!==canonical.record.sha256)throw Error('Corporate original SHA/bytes mismatch');
   const rows=(await get('corporate-records?cursor=0&limit=100')).rows;if(!rows.some((r:{recordId:string})=>r.recordId===id))throw Error('Own corporate record missing from current list');
  }
  const listResponse=page.waitForResponse(r=>r.url()===`${base}/api/companies/synthetic/corporate-records`&&r.request().method()==='GET');
  await page.goto(`${base}/companies/synthetic/workspace-register`);
  const listing=await listResponse;if(!listing.ok())throw Error(`Corporate UI list refused ${listing.status()}: ${await listing.text()}`);
  try{await page.getByRole('button',{name:`Vis record ${evidence.originalId}`,exact:true}).click();}catch(error){console.error('Corporate UI diagnostic',JSON.stringify({url:page.url(),listStatus:listing.status(),list:await listing.json(),body:await page.locator('body').innerText()}));throw error;}
  const event=page.waitForEvent('download');await page.getByRole('button',{name:`Hent original ${evidence.originalId}`,exact:true}).click();const download=await event;const path=join(temp,'original.txt');await download.saveAs(path);if(!readFileSync(path).equals(Buffer.from(evidence.original)))throw Error('Actual corporate browser download changed original');
  if(!isDeepStrictEqual(await get('journal'),journal))throw Error('Corporate governance flow changed journal');
  console.log(`PASS: corporate UI upload/original/link/enrichment/reasoned supersession with immutable hashes/history, duplicate/forgery refusal and no journal effects${restarted?' survive restart':''}`);
 }finally{rmSync(temp,{recursive:true,force:true});}
}
