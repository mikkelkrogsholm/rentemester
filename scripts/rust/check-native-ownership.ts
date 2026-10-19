import type {Page} from '@playwright/test';
import {mkdirSync,readFileSync,writeFileSync} from 'node:fs';
import {isDeepStrictEqual} from 'node:util';
const path='/tmp/rentemester-rust-reports/native-ownership-browser.json';
type Snapshot={snapshotId:string;snapshotHash:string;diffHash:string;state:string;principal:{kind:string;id:string};history:Array<{event_type:string;principal_id:string;actor:string}>;[key:string]:unknown};
type Fact={owner:{kind:string;partyId?:string};ownedCompanySlug:string;economicBasisPoints:number;validFrom:string;validToExclusive?:string};
type Evidence={company:string;partyId:string;recordId:string;snapshots:Snapshot[];original:unknown;initialVerified:boolean;restartVerified?:boolean};
/** Requires two ordinary independently MFA-authenticated current company owners.
 * The launcher creates/grants the explicit dedicated synthetic company via normal APIs. */
export async function checkNativeOwnership(author:Page,reviewer:Page,base:string,company:string,restarted=false){
 if(!/^http:\/\/127\.0\.0\.1:\d+$/.test(base)||!/^synthetic-ownership-[a-z0-9-]+$/.test(company))throw Error('Dedicated owned synthetic company/loopback fixture required');
 const url=`${base}/api/companies/${encodeURIComponent(company)}`,headers={origin:base,'content-type':'application/json'};
 async function get(page:Page,suffix:string){const r=await page.request.get(`${url}/${suffix}`);if(!r.ok())throw Error(`Ownership canonical read refused ${r.status()}`);return r.json();}
 async function post(page:Page,suffix:string,data:unknown,status=200){const r=await page.request.post(`${url}/${suffix}`,{headers,data});if(r.status()!==status)throw Error(`Ownership ${suffix}: expected${status}, got${r.status()}`);return r.json();}
 const me=await author.request.get(`${base}/api/me`),other=await reviewer.request.get(`${base}/api/me`);if(!me.ok()||!other.ok())throw Error('Two ordinary current MFA sessions required');const a=await me.json(),b=await other.json();if(!a.user?.id||!b.user?.id||a.user.id===b.user.id)throw Error('Ownership requires independent authenticated reviewer');
 const journal=await get(author,'journal');let evidence:Evidence;
 async function open(page:Page){await page.goto(`${base}/companies/${encodeURIComponent(company)}/workspace-register`);const region=page.getByRole('region',{name:'Ejer- og kontrolforhold',exact:true});await region.getByText('Forslag, review og apply',{exact:true}).click();return region;}
 async function ui(page:Page,name:string,action:string,body:unknown){const region=await open(page),form=region.getByRole('form',{name,exact:true});await form.getByLabel(`${name} JSON`,{exact:true}).fill(JSON.stringify(body));await form.getByRole('checkbox').check();const done=page.waitForResponse(r=>r.url()===`${url}/ownership/${action}`&&r.request().method()==='POST');await form.getByRole('button',{name,exact:true}).click();const r=await done;if(!r.ok())throw Error(`Reviewed ownership UI ${action} refused${r.status()}`);return r.json();}
 if(!restarted){
  const partyId='party-ownership-browser',recordId='record-ownership-browser';
  await post(author,'workspace-parties',{partyId,kind:'person',name:'Synthetic Ownership Browser Person',role:'owner',source:'synthetic-governance',observedAt:'2000-01-01',reviewAssertion:'Synthetic owner explicitly reviewed source',confirm:true});
  await post(author,'corporate-records',{recordId,type:'shareholder_register',filename:'synthetic-ownership-browser.txt',source:'synthetic-governance',receivedAt:'2000-01-01',uploader:'synthetic-browser',bytesBase64:Buffer.from('Synthetic browser ownership original\n').toString('base64'),confirm:true});
  const original=await get(author,`corporate-records/${recordId}/download`);
  const fact=(from:string,bp:number)=>({owner:{kind:'party',partyId},ownedCompanySlug:company,validFrom:from,economicBasisPoints:bp,controlType:'equity',jurisdiction:'DK',evidenceRefs:[recordId]});
  const make=(snapshotId:string,from:string,bp:number)=>({snapshotId,source:'synthetic-browser-reviewed-governance',observedAt:'2001-01-02',facts:[fact(from,bp)]});
  const first=(await ui(author,'Foreslå snapshot','propose',make('ownership-browser-initial','2000-01-01',1000))).snapshot as Snapshot;
  const exact=(s:Snapshot)=>({snapshotId:s.snapshotId,snapshotHash:s.snapshotHash,diffHash:s.diffHash,confirm:true});
  await post(author,'ownership/review',{snapshotId:first.snapshotId,decision:'approved',confirm:true},422);
  await post(author,'ownership/apply',exact(first),422);
  await post(author,'ownership/review',{snapshotId:first.snapshotId,decision:'approved',principal:{kind:'user',id:b.user.id},confirm:true},400);
  await post(author,'ownership/propose',{...make('ownership-browser-unconfirmed','2000-01-01',1000),confirm:false},400);
  await ui(reviewer,'Review snapshot','review',{snapshotId:first.snapshotId,decision:'approved'});
  await post(author,'ownership/apply',{...exact(first),diffHash:'0'.repeat(64)},422);
  await ui(author,'Apply eksakt diff','apply',exact(first));
  const second=(await ui(author,'Foreslå snapshot','propose',make('ownership-browser-later','2001-01-01',2000))).snapshot as Snapshot;
  await ui(reviewer,'Review snapshot','review',{snapshotId:second.snapshotId,decision:'approved'});await ui(author,'Apply eksakt diff','apply',exact(second));
  const snapshots:Snapshot[]=[];for(const s of[first,second]){const history=await get(author,`ownership/history?snapshotId=${s.snapshotId}`);const current=history.history.find((v:Snapshot)=>v.snapshotId===s.snapshotId)as Snapshot;if(!current||current.state!=='applied'||current.principal.id!==a.user.id)throw Error('Canonical ownership application/proposer missing');for(const[event,user]of [['proposed',a.user.id],['approved',b.user.id],['applied',a.user.id]])if(!current.history.some(h=>h.event_type===event&&h.principal_id===user&&h.actor===`user:${user}`))throw Error('Independent ownership audit principal missing');snapshots.push(current);}
  evidence={company,partyId,recordId,snapshots,original,initialVerified:true};
 }else{evidence=JSON.parse(readFileSync(path,'utf8'));if(evidence.company!==company)throw Error('Ownership evidence company mismatch');}
 if(!isDeepStrictEqual(await get(author,`corporate-records/${evidence.recordId}/download`),evidence.original))throw Error('Ownership changed original corporate evidence bytes/SHA');
 for(const expected of evidence.snapshots){const history=await get(author,`ownership/history?snapshotId=${expected.snapshotId}`);if(!isDeepStrictEqual(history.history.find((s:Snapshot)=>s.snapshotId===expected.snapshotId),expected))throw Error('Immutable ownership source/diff/history changed');}
 for(const[at,bp,end]of[['1999-12-31',0,undefined],['2000-12-31',1000,'2001-01-01'],['2001-01-01',2000,undefined]]as const){const graph=await get(author,`ownership?asOf=${at}`);const own=(graph.facts as Fact[]).filter(f=>f.owner.partyId===evidence.partyId&&f.ownedCompanySlug===company);if(graph.consolidation.eligible!==false||own.length!==(bp?1:0)||(bp&&(own[0]?.economicBasisPoints!==bp||own[0]?.validToExclusive!==end)))throw Error('Exact as-of ownership boundary changed');}
 const region=await open(author);const receipt=region.getByText(`${evidence.snapshots[1].snapshotId} · applied`,{exact:true});await receipt.click();await region.getByText(evidence.snapshots[1].snapshotHash,{exact:true}).waitFor();await region.getByText(evidence.snapshots[1].diffHash,{exact:true}).waitFor();
 const dated=author.waitForResponse(r=>r.url()===`${url}/ownership?asOf=2000-12-31`&&r.request().method()==='GET');await region.getByLabel('Ownership as-of',{exact:true}).fill('2000-12-31');if(!(await dated).ok())throw Error('Ownership dated UI read refused');const row=region.getByRole('list',{name:'Direkte ejerfacts',exact:true}).getByRole('listitem').filter({hasText:evidence.partyId});await row.waitFor();if(!(await row.innerText()).includes('1000 bp')||!(await row.innerText()).includes('2001-01-01'))throw Error('UI omitted exact dated direct fact');
 if(!isDeepStrictEqual(await get(author,'journal'),journal))throw Error('Ownership governance changed journal');
 if(restarted)evidence.restartVerified=true;mkdirSync('/tmp/rentemester-rust-reports',{recursive:true});writeFileSync(path,JSON.stringify(evidence),{mode:0o600});await region.screenshot({path:'/tmp/rentemester-rust-reports/native-ownership-browser.png'});
 console.log(`PASS: ownership real UI propose→independent authenticated review→exact hash apply, append-only dated succession, self-review/hash/confirmation/forgery refusal and unchanged journal${restarted?' survive fresh-MFA restart':''}`);
}
