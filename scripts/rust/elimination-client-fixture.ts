import type {Page} from '@playwright/test';
import type {EliminationClientFixture} from './check-native-elimination-clients';

/** Dedicated normal-auth fixture; no retry/adoption after a partial setup. Caller persists the returned selector for restart checks. */
export async function prepareEliminationClients(author:Page,reviewer:Page,base:string,anchor:string,other:string):Promise<EliminationClientFixture>{
 if(!/^http:\/\/127\.0\.0\.1:\d+$/.test(base)||anchor!=='synthetic-eliminations-clients-left'||other!=='synthetic-eliminations-clients-right')throw Error('Explicit fresh dedicated elimination-client companies and loopback origin required');
 const headers={origin:base,'content-type':'application/json'},intercompany=`/api/companies/${anchor}/intercompany`;
 async function get(page:Page,path:string){const r=await page.request.get(base+path);if(!r.ok())throw Error(`Elimination-client fixture read ${path}: HTTP${r.status()}`);return r.json();}
 async function post(page:Page,path:string,data:unknown){const r=await page.request.post(base+path,{headers,data});if(r.status()!==200)throw Error(`Elimination-client fixture write ${path}: HTTP${r.status()}; do not resubmit automatically`);return r.json();}
 const maker=await get(author,'/api/me'),checker=await get(reviewer,'/api/me');
 if(!maker.user?.id||!checker.user?.id||maker.user.id===checker.user.id||![maker,checker].every(who=>[anchor,other].every(s=>who.companies.some((c:any)=>c.slug===s&&c.role==='owner'))))throw Error('Two distinct normal verified/MFA principals with current owner grants on both companies required');
 const fresh=await Promise.all([anchor,other].map(s=>get(author,`/api/companies/${s}/journal`)));
 if(fresh.some(j=>!Array.isArray(j.journal?.entries)||j.journal.entries.length!==0||!Array.isArray(j.journal.nativePosting?.accounts)||j.journal.nativePosting.accounts.length!==0)||fresh.some(j=>!/^[a-f0-9]{32}$/.test(j.journal.ledgerUuid))||fresh[0].journal.ledgerUuid===fresh[1].journal.ledgerUuid)throw Error('Fixture requires distinct admitted fresh ledgers with no journals or accounts');
 const current=await get(author,intercompany),eliminations=await get(author,`/api/companies/${anchor}/eliminations`);
 if(current.manifest!==null||!Array.isArray(current.mappings)||current.mappings.length!==0||!Array.isArray(eliminations.rows)||eliminations.rows.length!==0)throw Error('Fixture requires empty global manifest/mappings/eliminations; never overwrite a browser baseline');
 const asOf=new Date().toLocaleDateString('sv-SE',{timeZone:'Europe/Copenhagen'});
 for(const slug of [anchor,other]){
  const accounts=slug===anchor?[['1871','asset','debit'],['2872','liability','credit'],['8871','equity','credit']]:[['2871','liability','credit'],['1872','asset','debit'],['8871','equity','credit']];
  for(const[accountNo,type,normalBalance]of accounts)await post(author,`/api/companies/${slug}/accounts`,{accountNo,name:`Synthetic elimination client ${accountNo}`,type,normalBalance});
 }
 async function journal(slug:string,lines:Array<{accountNo:string;debitAmount?:string;creditAmount?:string}>,key:string){
  const j=(await get(author,`/api/companies/${slug}/journal`)).journal;
  await post(author,`/api/companies/${slug}/manual-journal`,{confirm:true,input:{format:'rentemester-manual-journal-post/1',expectedRuleVersion:j.nativePosting.ruleVersion,idempotencyKey:key,request:{transactionDate:asOf,text:'Synthetic explicit elimination client source',currency:'DKK',lines}}});
 }
 await journal(anchor,[{accountNo:'1871',debitAmount:'23.45'},{accountNo:'8871',creditAmount:'23.45'}],'synthetic-client-elimination-forward-left');
 await journal(other,[{accountNo:'8871',debitAmount:'23.45'},{accountNo:'2871',creditAmount:'23.45'}],'synthetic-client-elimination-forward-right');
 await journal(anchor,[{accountNo:'8871',debitAmount:'67.89'},{accountNo:'2872',creditAmount:'67.89'}],'synthetic-client-elimination-reverse-left');
 await journal(other,[{accountNo:'1872',debitAmount:'67.89'},{accountNo:'8871',creditAmount:'67.89'}],'synthetic-client-elimination-reverse-right');
 const groupId='synthetic-elimination-clients';
 await post(author,intercompany+'/manifest',{version:1,groups:[{id:groupId,name:'Synthetic elimination clients group',memberships:[{id:'client-left-member',companySlug:anchor,validFrom:'2000-01-01'},{id:'client-right-member',companySlug:other,validFrom:'2000-01-01'}],ownership:[{id:'client-explicit-ownership',parentCompanySlug:anchor,childCompanySlug:other,basisPoints:10000,evidenceRefs:['synthetic-client-explicit-reviewed-ownership'],validFrom:'2000-01-01'}]}],confirm:true});
 const inputs:EliminationClientFixture['inputs']=[];
 for(const reverse of [false,true]){
  const direction=reverse?'reverse':'forward',mappingId=`synthetic-client-mapping-${direction}`;
  const mapping={id:mappingId,groupId,leftCompanySlug:anchor,rightCompanySlug:other,leftAccountNos:[reverse?'2872':'1871'],rightAccountNos:[reverse?'1872':'2871'],leftPosition:reverse?'payable':'receivable',rightPosition:reverse?'receivable':'payable',evidenceRefs:[`synthetic-client-reviewed-${direction}-source`],validFrom:'2000-01-01'};
  const proposed=await post(author,intercompany+'/propose',{...mapping,confirm:true});
  if(!/^[a-f0-9]{64}$/.test(proposed.mappingHash))throw Error('Server proposal did not return exact canonical mappingHash');
  await post(reviewer,intercompany+'/approve',{mappingId,mappingHash:proposed.mappingHash,confirm:true});
  inputs.push({id:`synthetic-client-elimination-${direction}`,mappingId,evidenceRefs:[`synthetic-client-reviewed-${direction}-source`]});
 }
 const report=await get(author,`/api/group-reconciliation?asOf=${asOf}`);
 for(const[i,input]of inputs.entries()){
  const row=report.rows.find((r:any)=>r.mappingId===input.mappingId),amount=i===0?'23.45':'67.89';
  if(row?.status!=='matched'||row.difference!=='0.00'||row.left.companySlug!==anchor||row.right.companySlug!==other||row.left.balance!==amount||row.right.balance!==amount||!row.left.sourceRefs?.length||!row.right.sourceRefs?.length)throw Error('Authoritative actual reciprocal source proof differs from explicit fixture');
 }
 if((await get(author,`/api/companies/${anchor}/eliminations`)).rows.length!==0)throw Error('Preparation unexpectedly created elimination events');
 return{asOf,inputs};
}
