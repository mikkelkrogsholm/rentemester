// Source-transition oracle only. Real financial source checks are qualified separately.
import {mkdtempSync,rmSync,writeFileSync} from 'node:fs';import {tmpdir} from 'node:os';import {join} from 'node:path';
import {openWorkspaceControlDb} from '../../../../../src/core/workspace-control';import {syncSourceTask,updateTask,completeTask,getTask,TaskError} from '../../../../../src/core/tasks';
const now='2026-10-26T11:00:00.000Z',ctx=(key:string,version?:number)=>({actor:'user:synthetic',principal:'synthetic-member',idempotencyKey:key,expectedVersion:version,now});const cases:any[]=[];
const draft=()=>({title:'Synthetic source task',scope:{kind:'company',companySlug:'synthetic'},origin:'system',evidenceRequired:true,source:{kind:'bank_work',identity:'0123456789abcdef0123456789abcdef:bank_work:1',companySlug:'synthetic',ref:'1',state:'open',contentHash:'a'.repeat(64),href:'/companies/synthetic/batchbogfoering'}});
for(const mode of ['create','proposal','noop','update','resolve','reopen','unknown','unknown-relevance','stale']){
 const root=mkdtempSync(join(tmpdir(),'rm-native-source-oracle-')),db=openWorkspaceControlDb(root);
 try{
  let input:any=draft(),prior:any=null,version:number|undefined;
  if(mode==='proposal'){input.origin='proposal';input.relevance='unknown';input.source.state='unknown';}
  if(mode!=='create'&&mode!=='proposal'){
   const first=syncSourceTask(db,input as any,ctx('first'));
   updateTask(db,first.taskId,{title:'User-kept title'},ctx('edit',1));prior=getTask(db,first.taskId)!;
   if(mode==='unknown-relevance'){updateTask(db,first.taskId,{relevance:'unknown'},ctx('unknown-relevance',prior.version));prior=getTask(db,first.taskId)!;}
   if(mode==='reopen'||mode==='unknown'){completeTask(db,first.taskId,{outcome:'completed',note:'Synthetic native source evidence',references:[]},{...ctx('complete',prior.version),sourceCheck:{state:'resolved',observedAt:now,evidence:[{kind:'bank_transaction',ref:'1',companySlug:'synthetic'}]}});prior=getTask(db,first.taskId)!;}
   version=mode==='stale'?99:prior.version;
   if(mode!=='noop')input.source.contentHash='b'.repeat(64);
   if(mode==='resolve')input.source.state='resolved';if(mode==='unknown')input.source.state='unknown';
  }
  let expected:any;try{expected={task:syncSourceTask(db,input,ctx('selected',version))};}catch(e){if(!(e instanceof TaskError))throw e;expected={error:e.code};}
  cases.push({mode,input,prior,expected,expectedVersion:version??null,now});
 }finally{db.close();rmSync(root,{recursive:true,force:true});}
}
writeFileSync(new URL('./main-sources.json',import.meta.url),JSON.stringify(cases,null,2)+'\n');
