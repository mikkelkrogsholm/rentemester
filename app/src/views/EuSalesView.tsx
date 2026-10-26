import { cockpitStyles } from "../design/cockpit.stylex";
import * as stylex from "@stylexjs/stylex";
import {useState} from "react";
import {useParams} from "react-router-dom";
import {api} from "../lib/api";
import {todayIso,formatKroner} from "../lib/format";
import {useAsync} from "../lib/useAsync";
import {Button,Input,PageHeader} from "../components/ui";
import {ErrorState,Loading} from "../components/Feedback";
export function EuSalesView(){
  const {slug=""}=useParams();const today=todayIso();
  const [from,setFrom]=useState(`${today.slice(0,4)}-01-01`);const [to,setTo]=useState(today);
  const [period,setPeriod]=useState({from,to});
  const state=useAsync(signal=>api.euSales(slug,period.from,period.to,{signal}),[slug,period.from,period.to]);
  const r=state.data;
  return <section {...stylex.props(cockpitStyles.statement)}><PageHeader title="EU-salg uden moms" description="EU B2B-ydelser pr. kundes momsnummer. Rapporten forbereder indberetning; den sender ikke oplysninger til myndighederne."/>
    <form {...stylex.props(cockpitStyles.rowActions)} onSubmit={e=>{e.preventDefault();setPeriod({from,to});}}><label>Fra<Input type="date" required value={from} onChange={e=>setFrom(e.target.value)}/></label><label>Til<Input type="date" required min={from} value={to} onChange={e=>setTo(e.target.value)}/></label><Button type="submit">Vis periode</Button></form>
    {state.loading&&!r?<Loading label="Henter EU-salg…"/>:state.error?<ErrorState message={state.error} onRetry={state.reload}/>:r&&<>
      <p>{r.periodStart} – {r.periodEnd} · {r.invoiceCount} fakturaer · {r.creditNoteCount} kreditnotaer · I alt {formatKroner(r.totalValue,"DKK")}</p>
      <p {...stylex.props(cockpitStyles.muted)}>Beløbene er i DKK efter fakturaernes fastlåste kurser. Kreditnotaer fratrækkes på deres udstedelsesdato.</p>
      {r.warnings.map((w,i)=><p key={i} role="status">{w}</p>)}{r.errors.map((v,i)=><p key={i} role="alert">{v}</p>)}
      <table {...stylex.props(...[cockpitStyles.tableData])} aria-label="EU-salg pr. kunde"><thead><tr><th>Momsnummer</th><th>Kunde</th><th>Fakturaer / kreditnotaer</th><th {...stylex.props(cockpitStyles.num)}>Ydelser, DKK</th></tr></thead><tbody>{r.customers.map(c=><tr key={c.vatNumber}><td>{c.vatNumber}</td><td>{c.customerName}</td><td>{c.invoiceNumbers.join(", ")}{c.creditNoteNumbers.length>0&&` / ${c.creditNoteNumbers.join(", ")}`}</td><td {...stylex.props(cockpitStyles.num)}>{formatKroner(c.totalValue,"DKK")}</td></tr>)}</tbody></table>
      {r.customers.length===0&&<p>Ingen bogførte EU B2B-ydelser i perioden.</p>}
      <details><summary>Bilagsgrundlag</summary><table {...stylex.props(...[cockpitStyles.tableData])}><thead><tr><th>Bilag</th><th>Dato</th><th>Momsnummer</th><th>Fakturavaluta</th><th>Netto, DKK</th></tr></thead><tbody>{r.sources.map(s=><tr key={s.documentId}><td>{s.documentNumber}</td><td>{s.issueDate}</td><td>{s.vatNumber}</td><td>{s.currency}</td><td>{formatKroner(s.netAmountDkk,"DKK")}</td></tr>)}</tbody></table></details>
    </>}
  </section>;
}
