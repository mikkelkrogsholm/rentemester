import { useState } from "react";
import { dineroImportApi } from "../lib/api/dinero-import";
import { formatKroner, todayIso } from "../lib/format";
import { useAsync } from "../lib/useAsync";
import { Button, Input } from "./ui";
export function ImportedReceivablesPanel({slug}:{slug:string}){
 const [date,setDate]=useState(todayIso()),[asOf,setAsOf]=useState(date);
 const state=useAsync(signal=>dineroImportApi.receivables(slug,asOf,{signal}),[slug,asOf]);
 return <section className="card" aria-label="Importerede tilgodehavender"><h2>Importerede tilgodehavender</h2><p>Fakturaer udstedt i det tidligere system. De udstedes og sendes ikke igen her. Oplyste dokumentreferencer er kildeangivelser og beviser ikke, at fakturaoriginalen er bevaret.</p><label>Opgørelsesdato for importerede tilgodehavender<Input type="date" value={date} onChange={e=>setDate(e.target.value)}/></label><Button variant="secondary" disabled={!date||state.loading} onClick={()=>{if(date===asOf)state.reload();else setAsOf(date);}}>Opdatér importerede tilgodehavender</Button>
 {state.loading&&<p>Henter importerede tilgodehavender…</p>}{state.error&&<p role="alert">Importerede tilgodehavender kunne ikke verificeres. Der vises ingen estimeret saldo.</p>}
 {!state.loading&&!state.error&&state.data&&(state.data.ok!==true||state.data.errors.length>0?<p role="alert">{state.data.errors.join(" · ")||"Opgørelsen kunne ikke verificeres."}</p>:<><p>Opgjort {state.data.asOfDate}: {state.data.count} fakturaer · åbent {formatKroner(state.data.totalOpen,"DKK")}.</p><p>{state.data.boundary}</p>{state.data.rows.length===0?<p>Ingen importerede fakturaer på opgørelsesdatoen.</p>:<table><thead><tr><th>Eksternt fakturanummer</th><th>Kunde</th><th>Fakturadato</th><th>Forfald</th><th>Brutto DKK</th><th>Betalt/krediteret DKK</th><th>Åbent DKK</th><th>Debitorkonto</th><th>Kildereference</th></tr></thead><tbody>{state.data.rows.map(r=><tr key={`${r.scheduleHash}:${r.externalInvoiceId}`}><td>{r.externalInvoiceId}</td><td>{r.customerName??r.customerExternalId??"Ikke angivet"}</td><td>{r.invoiceDate}</td><td>{r.dueDate??"Ikke angivet"}</td><td>{formatKroner(r.grossAmount,"DKK")}</td><td>{formatKroner(r.paidAmount,"DKK")}</td><td>{formatKroner(r.openBalance,"DKK")}</td><td>{r.controlAccountNo}</td><td>{r.sourceRecognitionRef}</td></tr>)}</tbody></table>}</>)}
 </section>;
}
