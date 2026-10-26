// Regenerate only from the current reviewed TS implementation, in a synthetic in-memory DB.
// bun crates/rentemester-engine/src/tasks/generate-main-flow.ts
import { resolve } from "node:path";
import { Database } from "bun:sqlite";
import { createHash } from "node:crypto";
import { createTask, updateTask, moveTask, completeTask, reopenTask, taskPayloadHash } from "../../../../src/core/tasks";
import { canonicalJson } from "../../../../src/core/canonical-json";
const root=resolve(import.meta.dir,"../../../..");
const db=new Database(":memory:");
db.exec((await Bun.file(`${root}/src/core/workspace-migrations/0034-tasks-v34.json`).json()).sql);
const steps=[];
let version=0;
const operations=[
 ["create",{taskId:"task-synthetic",title:" Kontrollér perioden ",scope:{kind:"company",companySlug:"synthetic"},evidenceRequired:true,workDate:"2026-10-26",deadline:{date:"2026-11-01",kind:"agreement",certainty:"confirmed",basis:"Aftalt kontrol"}}],
 ["update",{description:"Bilag og kvittering skal stemme",nextAction:"Åbn den beskyttede kilde",waitingOn:"",references:[{kind:"document",ref:"doc:synthetic:7",companySlug:"synthetic"}]}],
 ["move",{status:"in_progress",workspaceColumnId:"in_progress"}],
 ["complete",{outcome:"completed",note:"Kvittering og bilag er kontrolleret",references:[{kind:"document",ref:"doc:synthetic:7",companySlug:"synthetic"}]}],
 ["reopen","Efterfølgende bilag ændrer vurderingen"],
] as const;
for(const [operation,input] of operations) {
 const now=`2026-10-26T11:0${version}:00.000Z`;
 const ctx={actor:"user:synthetic",principal:"member-synthetic",idempotencyKey:`oracle-${version}`,now,...(version?{expectedVersion:version}:{})};
 const expected=operation==="create"?createTask(db,input as any,ctx):operation==="update"?updateTask(db,"task-synthetic",input as any,ctx):operation==="move"?moveTask(db,"task-synthetic",input as any,ctx):operation==="complete"?completeTask(db,"task-synthetic",input as any,ctx):reopenTask(db,"task-synthetic",input as string,ctx);
 steps.push({operation,input,expectedVersion:version||null,now,expected}); version=expected.version;
}
const hashes=[{payload:{scope:{kind:"workspace",companySlugs:["beta","alpha"]},title:"Kontrol"},expectedVersion:null},{"\ue000":4,"😀":1e-7,"a":-0,"b":1e21},[true,null,"æ\n\t",1e20]].map(input=>({input,canonical:canonicalJson(input),sha256:taskPayloadHash(input)}));
const sources=Object.fromEntries(await Promise.all(["src/core/tasks.ts","src/core/tasks-types.ts","src/core/canonical-json.ts","src/core/workspace-migrations/0034-tasks-v34.json"].map(async f=>[f,createHash("sha256").update(await Bun.file(`${root}/${f}`).text()).digest("hex")])));
await Bun.write(`${root}/crates/rentemester-engine/src/tasks/main-flow.json`,JSON.stringify({format:"rentemester-task-main-oracle/1",sources,steps,hashes},null,2)+"\n");
console.log(JSON.stringify({steps:steps.length,hashes:hashes.length,sourceBound:true,syntheticInMemory:true}));
db.close();
