import { cockpitStyles } from "../design/cockpit.stylex";
import * as stylex from "@stylexjs/stylex";
import { useRef, useState } from "react";
import { ApiError, request } from "../lib/api/_shared";
import { Button } from "./ui";
import { hasRememberedRemoteVerification, NativeBackupRemote } from "./NativeBackupRemote";

type Destination = { id:string;label:string;offsiteAttested:boolean;kind:string };
type Pending = { destinationId:string;operationId:string };
type Placement = { archiveSha256:string;archiveSizeBytes:number;verified:boolean;auditRecorded:boolean;remoteSyncVerified:boolean };
const key = (workspace:string) => `rentemester-backup-placement:${workspace}`;
function remembered(workspace:string):Pending|null {try{const v=JSON.parse(localStorage.getItem(key(workspace))??"null");return v&&typeof v.destinationId==="string"&&typeof v.operationId==="string"?v:null;}catch{return null;}}
export function NativeBackupDestinations(){
 const [destinations,setDestinations]=useState<Destination[]|null>(null),[selected,setSelected]=useState("");
 const [pending,setPending]=useState<Pending|null>(null),[placement,setPlacement]=useState<Placement|null>(null);
 const [remoteDestinations,setRemoteDestinations]=useState<string[]>([]);
 const [workspace,setWorkspace]=useState(""); const sending=useRef(false);
 const [confirmed,setConfirmed]=useState(false),[busy,setBusy]=useState(false),[error,setError]=useState("");
 async function load(){setBusy(true);setError("");try{const r=await request<{workspaceId:string;destinations:Destination[];remoteVerificationDestinations?:string[]}>("/api/workspace/backup-destinations");setDestinations(r.destinations);setRemoteDestinations(r.remoteVerificationDestinations??[]);setWorkspace(r.workspaceId);const prior=remembered(r.workspaceId);setPending(prior);setSelected(prior?.destinationId??r.destinations[0]?.id??"");}catch(e){setError(e instanceof Error?e.message:"Destinationerne kunne ikke hentes.");}finally{setBusy(false);}}
 async function run(verify:boolean){
  if(sending.current||busy||(!verify&&(!confirmed||!selected||pending)))return;
  const operation=verify?pending:{destinationId:selected,operationId:crypto.randomUUID()};if(!operation)return;
  sending.current=true;
  if(!verify){try{localStorage.setItem(key(workspace),JSON.stringify(operation));}catch{sending.current=false;setError("Operations-id kunne ikke gemmes. Ingen backup er sendt.");return;}setPending(operation);setPlacement(null);setConfirmed(false);}
  setBusy(true);setError("");
  try{
   const path=`/api/workspace/backup-destinations/${encodeURIComponent(operation.destinationId)}`;
   const r=await request<{placement:Placement}>(verify?`${path}/placements/${encodeURIComponent(operation.operationId)}`:`${path}/place`,verify?undefined:{method:"POST",body:JSON.stringify({operationId:operation.operationId,confirm:true})});
   if(!r.placement.verified)throw Error("Placeringens bytes kunne ikke verificeres.");setPlacement(r.placement);
  }catch(e){const definite=e instanceof ApiError&&e.status>=400&&e.status<500&&e.code!=="backup_destination_exists_read_back";if(!verify&&definite){localStorage.removeItem(key(workspace));setPending(null);}setError(`${e instanceof Error?e.message:"Placeringen kunne ikke bekræftes."} Kontrollér den samme placering før et nyt forsøg. Et ukendt udfald kræver afstemning med serverens administrator.`);}
  finally{sending.current=false;setBusy(false);}
 }
 function next(){if(!placement?.verified)return;localStorage.removeItem(key(workspace));setPending(null);setPlacement(null);setError("");}
 return <section aria-label="Backupdestinationer">
  <h4>Placér backup på en konfigureret destination</h4>
  <p {...stylex.props(cockpitStyles.muted)}>Serverens administrator vælger mapper og registrerer oplysninger om opbevaringen. Kontrol af en synkroniseret mappe beviser ikke, at filen er nået til skyen.</p>
  <Button type="button" disabled={busy} onClick={()=>void load()}>Vis backupdestinationer</Button>
  {destinations!==null&&(destinations.length===0?<p>Ingen backupdestinationer er konfigureret.</p>:<>
   <label>Backupdestination<select aria-label="Backupdestination" value={selected} disabled={busy||!!pending} onChange={e=>setSelected(e.target.value)}>{destinations.map(d=><option key={d.id} value={d.id}>{d.label}</option>)}</select></label>
   <p>{destinations.find(d=>d.id===selected)?.offsiteAttested?"Opbevaringskrav er erklæret af administratoren; de er ikke uafhængigt verificeret.":"Denne destination har ikke alle erklæringer om ekstern opbevaring."}</p>
   {!pending&&<><label><input type="checkbox" checked={confirmed} disabled={busy} onChange={e=>setConfirmed(e.target.checked)}/>Jeg vil danne og placere en backup af hele workspacet.</label><Button type="button" disabled={busy||!confirmed} onClick={()=>void run(false)}>Dan og placér backup</Button></>}
  </>)}
  {pending&&<><p>Placering: {pending.operationId}</p><Button type="button" disabled={busy} onClick={()=>void run(true)}>Kontrollér placeringen</Button></>}
  {placement&&<><p role="status">Backupfilen er genlæst og dens signatur kontrolleret ({placement.archiveSizeBytes} bytes). SHA256: {placement.archiveSha256}. Gendannelse og synkronisering til skyen er ikke verificeret her.</p>{!placement.auditRecorded&&<p role="alert">Filen findes, men placeringskvitteringen mangler i auditloggen. Kontakt administratoren før næste placering.</p>}{placement.auditRecorded&&<Button type="button" disabled={busy} onClick={next}>Klargør næste backup</Button>}</>}
  {workspace&&(remoteDestinations.includes(selected)||hasRememberedRemoteVerification(workspace,selected))&&<NativeBackupRemote key={`${workspace}:${selected}`} workspace={workspace} destination={selected} enabled={remoteDestinations.includes(selected)} placementOperationId={placement?.verified&&placement.auditRecorded?pending?.operationId??"":""}/> }
  {error&&<p role="alert">{error}</p>}
 </section>;
}
