import { useRef, useState } from "react";
import { ApiError, request } from "../lib/api/_shared";
import { Button } from "./ui";
type Status = {
  ok:true; slug:string;
  backup:{available:true;scope:"local-retained-snapshots";backupDue:boolean;hasActivitySinceBackup:boolean;latestBackupAt:string|null;requiredBy:string|null;checkedAt:string;backupsFound:number};
  lock:{enforced:boolean;graceDays:number;locked:boolean;lockDeadline:string|null};
};
function verifiedResponse(value:Status,slug:string):Status {
  if(value?.ok!==true||value.slug!==slug||value.backup?.available!==true||value.backup.scope!=="local-retained-snapshots"
    ||typeof value.backup.backupDue!=="boolean"||typeof value.backup.hasActivitySinceBackup!=="boolean"
    ||typeof value.backup.checkedAt!=="string"||!Number.isSafeInteger(value.backup.backupsFound)||value.backup.backupsFound<0
    ||typeof value.lock?.enforced!=="boolean"||typeof value.lock.locked!=="boolean"
    ||!Number.isSafeInteger(value.lock.graceDays)||value.lock.graceDays<0||value.lock.graceDays>365
    ||![value.backup.latestBackupAt,value.backup.requiredBy,value.lock.lockDeadline].every(v=>v===null||typeof v==="string")) {
    throw Error("Serverens backupstatus kunne ikke bekræftes. Hent status igen.");
  }
  return value;
}
const pendingKey=(slug:string)=>`rentemester:backup-lock:pending:${slug}`;
function remembered(slug:string):boolean {try{return sessionStorage.getItem(pendingKey(slug))!==null;}catch{return true;}}
/** Explicit human-admin status/lock control; no automatic requests or backup creation. */
export function NativeBackupStatus({slug}:{slug:string}) {
  const [status,setStatus]=useState<Status|null>(null),[enforced,setEnforced]=useState(false),[grace,setGrace]=useState("0");
  const [confirmed,setConfirmed]=useState(false),[pending,setPending]=useState(()=>remembered(slug));
  const [busy,setBusy]=useState(false),[error,setError]=useState(""),[message,setMessage]=useState("");
  const sending=useRef(false),base=`/api/companies/${encodeURIComponent(slug)}`;
  function accept(value:Status){
    const current=verifiedResponse(value,slug);sessionStorage.removeItem(pendingKey(slug));
    setPending(false);setConfirmed(false);setStatus(current);setEnforced(current.lock.enforced);setGrace(String(current.lock.graceDays));
  }
  async function load(){
    if(sending.current)return;sending.current=true;setBusy(true);setError("");setMessage("");setConfirmed(false);
    try{accept(await request<Status>(`${base}/backup-status`));}
    catch(cause){setError(cause instanceof Error?cause.message:"Backupstatus kunne ikke hentes.");}
    finally{sending.current=false;setBusy(false);}
  }
  async function save(){
    const days=Number(grace);
    if(sending.current||pending||!status||!confirmed||!/^\d+$/.test(grace)||!Number.isSafeInteger(days)||days<0||days>365)return;
    sending.current=true;setBusy(true);setError("");setMessage("");setConfirmed(false);
    try{
      // Record intent before transport, including across leaving this view.
      try{sessionStorage.setItem(pendingKey(slug),"pending");}catch{throw Error("Ændringen kunne ikke sikres mod gentagelse. Ingen anmodning er sendt.");}
      setPending(true);
      try{
        accept(await request<Status>(`${base}/backup-lock`,{method:"POST",body:JSON.stringify({confirm:true,enforced,graceDays:days})}));
        setMessage("Indstillingerne er gemt. Backupstatus er opdateret fra serverens svar.");
      }catch(cause){
        if(cause instanceof ApiError&&cause.status>=400&&cause.status<500){sessionStorage.removeItem(pendingKey(slug));setPending(false);}
        throw cause;
      }
    }catch(cause){setError(cause instanceof Error?cause.message:"Indstillingerne kunne ikke gemmes.");}
    finally{sending.current=false;setBusy(false);}
  }
  const validGrace=/^\d+$/.test(grace)&&Number(grace)<=365&&Number.isSafeInteger(Number(grace));
  return <section className="card" aria-label="Backupstatus og bogføringslås">
    <h3>Backupstatus og bogføringslås</h3>
    <p className="muted">Status gælder serverens gemte backupfiler for denne virksomhed. Den dokumenterer ikke opbevaring i skyen eller en afprøvet gendannelse. Ændringer kræver adgang til at administrere virksomheden.</p>
    <Button type="button" disabled={busy} onClick={()=>void load()}>Hent backupstatus</Button>
    {pending&&<p role="alert">En ændring kan være gennemført. Nye ændringer er blokeret, indtil du henter og kontrollerer backupstatus. Anmodningen gentages ikke automatisk.</p>}
    {status&&<>
      <dl>
        <dt>Seneste gemte backup</dt><dd>{status.backup.latestBackupAt??"Ingen gemt backup"}</dd>
        <dt>Backupbehov</dt><dd>{status.backup.backupDue?"Backup er forfalden":status.backup.hasActivitySinceBackup?"Backup er endnu ikke forfalden":"Ingen ny aktivitet siden backup"}</dd>
        <dt>Bogføringslås</dt><dd>{status.lock.locked?"Bogføring er låst":status.lock.enforced?"Slået til; bogføring er ikke låst":"Slået fra"}</dd>
        {status.lock.lockDeadline&&<><dt>Låsens frist</dt><dd>{status.lock.lockDeadline}</dd></>}
        <dt>Kontrolleret</dt><dd>{status.backup.checkedAt}</dd>
      </dl>
      <label><input type="checkbox" checked={enforced} disabled={busy||pending} onChange={e=>{setEnforced(e.target.checked);setConfirmed(false);}}/> Lås bogføring, når en backup er forfalden ud over fristen</label>
      <label>Ekstra frist i dage<input aria-label="Ekstra frist i dage" type="number" min="0" max="365" step="1" value={grace} disabled={busy||pending} onChange={e=>{setGrace(e.target.value);setConfirmed(false);}}/></label>
      <p className="muted">Låsen er frivillig. Fristen er syv dage plus den ekstra frist, når der er ny aktivitet. Har virksomheden aldrig haft backup, regnes låsefristen fra den første aktivitet. En ny backup dannes separat ovenfor.</p>
      <label><input type="checkbox" checked={confirmed} disabled={busy||pending||!validGrace} onChange={e=>setConfirmed(e.target.checked)}/> Jeg har gennemgået ændringen af bogføringslåsen.</label>
      <Button type="button" disabled={busy||pending||!confirmed||!validGrace} onClick={()=>void save()}>Gem bogføringslås</Button>
    </>}
    {error&&<p role="alert">{error}</p>}{message&&<p role="status">{message}</p>}
  </section>;
}
