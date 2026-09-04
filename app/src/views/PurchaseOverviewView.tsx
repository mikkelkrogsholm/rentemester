import { useState } from "react";
import { useParams } from "react-router-dom";
import { api } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import type { PurchaseNeedGroup, PurchaseOverview } from "../lib/types";
import { ErrorState, Loading } from "../components/Feedback";
import { ConfirmDialog } from "../components/ConfirmDialog";

const today=()=>new Date().toISOString().slice(0,10);
const yearStart=()=>`${new Date().getUTCFullYear()}-01-01`;
export function PurchaseOverviewView(){
  const {slug=""}=useParams(); const [from,setFrom]=useState(yearStart); const [to,setTo]=useState(today); const [selected,setSelected]=useState<PurchaseNeedGroup|null>(null);
  const state=useAsync<PurchaseOverview>(()=>api.purchaseOverview(slug,{from,to}),[slug,from,to]);
  if(state.loading&&!state.data)return <Loading label="Henter købsoversigt…"/>;
  if(state.error)return <ErrorState message={state.error} onRetry={state.reload}/>;
  const overview=state.data!;
  return <section className="statement"><div className="page-head"><div><h2>Købsoverblik</h2><p className="muted">Kildebaseret status og dokumenterede behov. Beløb summeres ikke på tværs af kildetyper.</p></div></div><div className="row-actions"><label>Fra <input type="date" value={from} onChange={event=>setFrom(event.target.value)}/></label><label>Til <input type="date" value={to} onChange={event=>setTo(event.target.value)}/></label></div><div className="cards"><article className="card"><strong>{overview.basis.canonical.postedCaseCount}</strong><span>Bogførte cases</span></article><article className="card"><strong>{overview.basis.canonical.unpostedCaseCount}</strong><span>Ikke-bogførte cases</span></article><article className="card"><strong>{overview.basis.provisional.unresolvedDocumentationCount}</strong><span>Uafklaret dokumentation</span></article></div><div className="card statement-card"><h3>Grupperede behov</h3>{overview.groups.length===0?<p className="muted">Ingen åbne grupper i perioden.</p>:<ul className="plain-list">{overview.groups.map(group=><li key={group.selectionHash}><strong>{group.need.question}</strong><p className="muted">{group.caseCount} eksakte cases · {group.need.key}</p>{group.need.key==="documentation:unresolved"&&<button type="button" className="btn secondary" onClick={()=>setSelected(group)}>Review den valgte gruppe</button>}</li>)}</ul>}</div>{selected&&<ConfirmDialog title="Review købsdokumentation" body={<p>Review kun de {selected.caseCount} præcist viste cases. Handlingen bogfører ikke og ændrer ikke momsstatus.</p>} confirmLabel="Bekræft review" confirmKind="primary" onConfirm={async note=>{await api.reviewPurchaseCaseGroup(slug,{groupId:`ui-group-${crypto.randomUUID()}`,members:selected.members.map(member=>({caseId:member.caseId,expectedVersion:member.version,expectedSourceFingerprint:member.sourceFingerprint})),documentationOutcome:"ordinary_evidence_sufficient",note,idempotencyKey:crypto.randomUUID()});setSelected(null);state.reload();}} onClose={()=>setSelected(null)}/>}</section>;
}
