/** Rebuild the finite synthetic oracle from the actual source engine; no ledger/provider actions. */
import { createHash } from "node:crypto";
import { readFileSync, writeFileSync, mkdtempSync, rmSync } from "node:fs";
import { join, resolve } from "node:path";
import { openWorkspaceControlDb } from "../../../../../src/core/workspace-control";
import { saveTaskSeries, projectTaskSeries, materializeTaskSeries } from "../../../../../src/core/task-series";
import { listTasks } from "../../../../../src/core/tasks";
import type { TaskSeriesDraft } from "../../../../../src/core/tasks-types";
const out=import.meta.dir;
const repo=resolve(out,"../../../../..");
const build=process.env.SGD_BUILD_DIR;
if (!build) throw new Error("Explicit SGD_BUILD_DIR is required for synthetic oracle scratch files");
const scope={kind:"company" as const,companySlug:"synthetic"};
const draft=(extra:Partial<TaskSeriesDraft>={}):TaskSeriesDraft=>({seriesId:"monthly",title:"Månedlig afstemning",scope,template:{title:"Månedlig afstemning",scope},cadence:"month",every:1,anchor:"calendar",startDate:"2026-01-01",endDate:null,fiscalYearStartMonth:1,workDayOffset:0,deadlineDayOffset:14,relevance:"relevant",active:true,...extra});
const ctx=(key:string,expectedVersion?:number,now="2026-01-01T00:00:00.000Z")=>({actor:"agent:test",principal:"synthetic",idempotencyKey:key,expectedVersion,now});
const cases:unknown[]=[];
function scenario(name:string,setup:(db:ReturnType<typeof openWorkspaceControlDb>)=>void,query:{from:string,to:string,companySlugs?:string[],includeWorkspace?:boolean}) {
 const root=mkdtempSync(join(build,"series-oracle-"));
 const db=openWorkspaceControlDb(root);
 try {
  setup(db);
  const rows=db.query("SELECT series_id,payload_json FROM rm_task_series_events ORDER BY series_id,version").all() as Array<{series_id:string,payload_json:string}>;
  const histories=Object.values(Object.groupBy(rows,r=>r.series_id)).map(rows=>rows!.map(r=>JSON.parse(r.payload_json)));
  const tasks=listTasks(db,{showDone:true});
  cases.push({name,histories,tasks,query:{companySlugs:null,includeWorkspace:false,...query},expected:projectTaskSeries(db,query)});
 } finally {db.close();rmSync(root,{recursive:true,force:true});}
}
const year={from:"2026-01-01",to:"2026-12-31"};
scenario("monthly",db=>{saveTaskSeries(db,draft(),ctx("monthly"));},year);
scenario("fiscal",db=>{saveTaskSeries(db,draft({cadence:"quarter",anchor:"fiscal",startDate:"2026-07-01",fiscalYearStartMonth:7}),ctx("fiscal"));},{from:"2026-07-01",to:"2027-06-30"});
scenario("custom",db=>{saveTaskSeries(db,draft({cadence:"custom",every:2}),ctx("custom"));},year);
scenario("offset",db=>{saveTaskSeries(db,draft({startDate:"2026-01-10",endDate:"2026-02-01",workDayOffset:9,deadlineDayOffset:-1}),ctx("offset"));},year);
scenario("revision",db=>{saveTaskSeries(db,draft(),ctx("old"));saveTaskSeries(db,draft({title:"Ny rutine",workDayOffset:9}),ctx("new",1,"2026-03-01T00:00:00.000Z"));},year);
scenario("concrete-cadence",db=>{saveTaskSeries(db,draft(),ctx("old"));materializeTaskSeries(db,{asOfDate:"2026-02-01",actor:"agent:test",principal:"synthetic"});saveTaskSeries(db,draft({cadence:"quarter",title:"Kvartalsrutine"}),ctx("new",1,"2026-04-01T00:00:00.000Z"));},year);
scenario("pause",db=>{saveTaskSeries(db,draft(),ctx("old"));materializeTaskSeries(db,{asOfDate:"2026-02-01",actor:"agent:test",principal:"synthetic"});saveTaskSeries(db,draft({active:false,title:"Paused",workDayOffset:9}),ctx("new",1));},year);
scenario("activity",db=>{saveTaskSeries(db,draft({relevance:"activity"}),ctx("old"));materializeTaskSeries(db,{asOfDate:"2026-01-02",actor:"agent:test",principal:"synthetic"});},year);
for(const companySlugs of [["synthetic"],["synthetic","other"],[]]) {
 scenario(`selection-${companySlugs.join("-")}`,db=>{
  for(const [seriesId,companySlugs] of [["shared",["synthetic","other"]],["workspace",[]]] as const) {
   const scope={kind:"workspace" as const,companySlugs:[...companySlugs]};
   saveTaskSeries(db,draft({seriesId,scope,template:{title:"Fælles opgave",scope}}),ctx(seriesId));
  }
 },{...year,companySlugs,includeWorkspace:true});
}
const sources=Object.fromEntries(["src/core/task-series.ts","src/core/tasks-types.ts","src/core/dates.ts"].map(p=>[p,createHash("sha256").update(readFileSync(join(repo,p))).digest("hex")]));
writeFileSync(join(out,"main-series.json"),JSON.stringify({sources,cases},null,2)+"\n");
console.log(`Generated ${cases.length} source projection cases`);
