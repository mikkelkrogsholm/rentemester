// Synthetic current-main oracle. No financial/provider data or effects.
import {mkdtempSync,rmSync,writeFileSync} from 'node:fs';import {tmpdir} from 'node:os';import {join} from 'node:path';
import {openWorkspaceControlDb} from '../../../../../src/core/workspace-control';
import {createTask,listTasks,moveTask,completeTask,TaskError} from '../../../../../src/core/tasks';
import {defaultTaskBoard,getTaskBoard,listTaskBoards,previewTaskBoard,saveTaskBoard} from '../../../../../src/core/task-boards';
const now='2026-10-08T10:00:00.000Z',scope={kind:'company',companySlug:'synthetic-alpha'} as const,ctx=(key:string,expectedVersion?:number)=>({actor:'user:synthetic',principal:'synthetic-member',idempotencyKey:key,expectedVersion,now});const cases:any[]=[];
for(const mode of ['rename','relocate','meaning','unreviewed','stale','complete','reopen','workspace-relocate','ambiguous']){
 const root=mkdtempSync(join(tmpdir(),'rm-native-board-oracle-')),db=openWorkspaceControlDb(root);
 try{
  if(mode==='ambiguous'){const b=defaultTaskBoard(scope);b.columns.push({columnId:'other-waiting',name:'Other waiting',status:'waiting',isDefault:false});saveTaskBoard(db,{boardId:b.boardId,scope:b.scope,columns:b.columns},ctx('company-board',0));}
  const task=createTask(db,{taskId:'synthetic-task',title:'Synthetic board task',scope,references:[{kind:'document',ref:'synthetic-proof'}]},ctx('create'));
  if(mode==='reopen')completeTask(db,task.taskId,{outcome:'completed',note:'Synthetic evidence checked'},ctx('completed',1));
  const workspace=mode==='workspace-relocate'||mode==='ambiguous';const b=defaultTaskBoard(workspace?{kind:'workspace',companySlugs:[]}:scope);let input:any={boardId:b.boardId,scope:b.scope,columns:b.columns};
  if(mode==='rename')input.columns=input.columns.map((c:any)=>({...c,name:c.name+' 😀'})).reverse();
  else if(mode.includes('relocate')){input.columns=input.columns.map((c:any)=>c.columnId==='open'?{...c,columnId:'new-open'}:c);input.relocations={open:'new-open'};}
  else {const from=mode==='reopen'?'done':'open',to=mode==='complete'?'done':mode==='reopen'?'open':'waiting';input.columns=input.columns.map((c:any)=>c.columnId===from?{...c,status:to,isDefault:false}:c);input.columns.push({columnId:'new-'+from,name:'New '+from,status:from,isDefault:true});if(mode!=='unreviewed')input.previewHash=previewTaskBoard(db,input).previewHash;}
  if(mode==='stale')moveTask(db,task.taskId,{status:'in_progress'},ctx('move-after-preview',1));
  const prior=getTaskBoard(db,input.boardId),boards=listTaskBoards(db),tasks=listTasks(db,{showDone:true}),preview=previewTaskBoard(db,input);let expected:any;
  try{expected={board:saveTaskBoard(db,input,ctx('save',0)),tasks:listTasks(db,{showDone:true})};}catch(e){if(!(e instanceof TaskError))throw e;expected={error:e.code};}
  cases.push({mode,input,prior,boards,tasks,preview,expected,now,expectedVersion:0});
 }finally{db.close();rmSync(root,{recursive:true,force:true});}
}
writeFileSync(new URL('./main-boards.json',import.meta.url),JSON.stringify(cases,null,2)+'\n');
