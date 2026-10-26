// Execute original main reminder policy in disposable control databases. No financial/provider effects.
import {mkdtempSync,rmSync,writeFileSync} from 'node:fs';import{tmpdir}from'node:os';import{join}from'node:path';
import{openWorkspaceControlDb}from'../../../../../src/core/workspace-control';import{activateWorkspaceUser}from'../../../../../src/core/workspace-access';import{createTask}from'../../../../../src/core/tasks';import{configureTaskReminder,runTaskReminders,listTaskNotifications}from'../../../../../src/core/task-reminders';
const stamp='2026-01-01T00:00:00.000Z',rows:any[]=[];
for(const mode of ['deadline-once','deadline-default','daily-copenhagen-dst','daily-newyork','followup-with-optional-days','disabled','no-deadline','invalid-zone']){
 const root=mkdtempSync(join(tmpdir(),'rm-native-reminder-oracle-')),db=openWorkspaceControlDb(root);
 try{
 db.query('INSERT INTO "user"(id,name,email,emailVerified,createdAt,updatedAt) VALUES(?,?,?,?,?,?)').run('user-one','Synthetic','one@example.invalid',1,stamp,stamp);activateWorkspaceUser(db,{userId:'user-one',workspaceRole:'member',createdBy:'agent:test'});
 const task=createTask(db,{taskId:'task-reminder',title:'Synthetic reminder',scope:{kind:'company',companySlug:'synthetic'},deadline:mode==='no-deadline'?null:{date:'2026-03-29',kind:'internal',basis:'Synthetic deadline',certainty:'confirmed'}} as any,{actor:'agent:test',principal:'synthetic',idempotencyKey:'create',now:stamp});
 let reminder:any={reminderId:'deadline',recipientId:'user-one',enabled:mode!=='disabled',kind:'deadline',daysBefore:2,frequency:'once',timeZone:'Europe/Copenhagen'};
 if(mode==='deadline-default')delete reminder.daysBefore;
 if(mode.startsWith('daily')||mode==='followup-with-optional-days')Object.assign(reminder,{kind:'follow_up',followUpDate:'2026-03-29',frequency:'daily',timeZone:mode==='daily-newyork'?'America/New_York':'Europe/Copenhagen'});
 if(mode==='invalid-zone')reminder.timeZone='invalid-zone';
 let configured:any,error:any;try{configured=configureTaskReminder(db,{taskId:task.taskId,reminder},{actor:'agent:test',principal:'synthetic',idempotencyKey:'configure',expectedVersion:1,now:stamp});}catch(e){error=(e as any).code;}
 const probes:any[]=[];
 if(configured)for(const now of ['2026-03-28T23:30:00.000Z','2026-03-29T12:00:00.000Z','2026-04-04T12:00:00.000Z']){
 const today=new Intl.DateTimeFormat('en-CA',{timeZone:configured.reminders[0].timeZone,year:'numeric',month:'2-digit',day:'2-digit'}).format(new Date(now));const delivered=runTaskReminders(db,{now,canRead:()=>true}).delivered;probes.push({now,today,delivered,notifications:listTaskNotifications(db,'user-one')});}
 rows.push({mode,prior:task,reminder,configured:configured??null,error:error??null,probes});
 }finally{db.close();rmSync(root,{recursive:true,force:true});}
}
writeFileSync(new URL('./main-reminders.json',import.meta.url),JSON.stringify(rows,null,2)+'\n');
