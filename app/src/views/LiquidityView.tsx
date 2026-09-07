// Likviditet / pengestrøm — the per-company cash-flow view (cockpit-redesign
// Runde 2, iteration 8).
//
// Renders `/api/companies/:slug/cashflow?year=`: actual money in and out of the
// bank for the year, read straight from the imported bank transactions (NOT
// the accrual ledger). A summary strip carries primo-saldo · ind · ud ·
// ultimo-saldo; a combined Chart.js graph shows the monthly indbetalinger /
// udbetalinger as bars and the real bank-balance trajectory as a line. When the
// company has no bank transactions a clean empty state is shown instead. All
// money fields are kroner — `formatKroner` is used throughout.

import { Link, useParams } from "react-router-dom";
import { api } from "../lib/api";
import { formatKroner } from "../lib/format";
import { useAsync } from "../lib/useAsync";
import type { CompanyCashflow } from "../lib/types";
import { ErrorState, Loading } from "../components/Feedback";
import { CashflowChart } from "../components/CashflowChart";
import { CompanyNav, useCompanyYear } from "../components/CompanyNav";
import { useState, type FormEvent } from "react";
import { ConfirmDialog } from "../components/ConfirmDialog";

function CommitmentMatchForm({slug,rows,onDone}:{slug:string;rows:Array<{commitmentId:string}>;onDone:()=>void}) {
  const [commitmentId,setCommitmentId]=useState(rows[0]?.commitmentId??"");const [occurrenceDate,setOccurrenceDate]=useState("");const [kind,setKind]=useState<"canonical_document"|"payable"|"bank_transaction">("canonical_document");const [evidenceId,setEvidenceId]=useState("");const [confirmed,setConfirmed]=useState(false);const [message,setMessage]=useState("");
  const submit=async(event:FormEvent)=>{event.preventDefault();try{await api.supplierCommitmentMatch(slug,{commitmentId,occurrenceDate,evidence:{kind,id:evidenceId}});setMessage("Canonical evidence er matchet append-only.");onDone();}catch(cause){setMessage(cause instanceof Error?cause.message:String(cause));}};
  return <form onSubmit={event=>void submit(event)} className="row-actions"><select aria-label="Forpligtelse" value={commitmentId} onChange={event=>setCommitmentId(event.target.value)}>{rows.map(row=><option key={row.commitmentId}>{row.commitmentId}</option>)}</select><input aria-label="Forventet dato" type="date" value={occurrenceDate} onChange={event=>setOccurrenceDate(event.target.value)}/><select aria-label="Evidenstype" value={kind} onChange={event=>setKind(event.target.value as typeof kind)}><option value="canonical_document">Bilag</option><option value="payable">Kreditor</option><option value="bank_transaction">Bankpost</option></select><input aria-label="Canonical evidence-id" value={evidenceId} onChange={event=>setEvidenceId(event.target.value)}/><label><input type="checkbox" checked={confirmed} onChange={event=>setConfirmed(event.target.checked)}/> Bekræft match</label><button type="submit" className="btn secondary" disabled={!confirmed||!commitmentId||!occurrenceDate||!evidenceId}>Match faktisk occurrence</button>{message&&<span className="muted">{message}</span>}</form>;
}

/**
 * The bank balance at the end of each of the twelve calendar months: the last
 * statement point dated in or before that month. Months before the first
 * statement point are `null` so the trajectory line starts where the data
 * does. Returns `[]` when no statement carries a running balance.
 */
function monthlyBalances(cf: CompanyCashflow): Array<number | null> {
  if (cf.balanceSeries.length === 0) return [];
  const result: Array<number | null> = new Array(12).fill(null);
  let pointer = 0;
  let last: number | null = null;
  for (let month = 1; month <= 12; month += 1) {
    while (
      pointer < cf.balanceSeries.length &&
      parseInt(cf.balanceSeries[pointer]!.date.slice(5, 7), 10) <= month
    ) {
      last = cf.balanceSeries[pointer]!.balance;
      pointer += 1;
    }
    result[month - 1] = last;
  }
  return result;
}

export function LiquidityView() {
  const { slug = "" } = useParams();
  const { year, setYear } = useCompanyYear();
  const [asOf, setAsOf] = useState(() => new Date().toISOString().slice(0, 10));
  const [pending, setPending] = useState<{commitmentId:string;action:"paused"|"ended"}|null>(null);
  const state = useAsync<CompanyCashflow>(
    () => api.cashflow(slug, year),
    [slug, year],
  );
  const commitments = useAsync(
    () => api.supplierCommitments(slug, asOf),
    [slug, asOf],
  );

  if (state.loading && !state.data)
    return <Loading label="Henter likviditet…" />;
  if (state.error)
    return <ErrorState message={state.error} onRetry={state.reload} />;

  const cf = state.data!;
  const currency = cf.company.currency || "DKK";
  const netto = cf.totalIn - cf.totalOut;

  return (
    <section className="statement" data-cockpit-page="liquidity" data-evidence-issue="655">
      <div className="page-head">
        <div>
          <h2>{cf.company.name}</h2>
          <p className="muted">
            {cf.company.cvr ? `CVR ${cf.company.cvr} · ` : ""}
            {cf.company.country} · {currency} · Likviditet
          </p>
        </div>
        <div className="row-actions">
          <Link className="btn secondary" to={`/companies/${slug}/manage`}>
            Administrér
          </Link>
        </div>
      </div>

      <CompanyNav
        slug={slug}
        years={cf.fiscalYears}
        selectedYear={cf.selectedYear}
        onYearChange={setYear}
      />

      {commitments.data && (
        <section className="section">
          <h3>13 ugers likviditetsprognose</h3>
          <label>Prognose fra <input aria-label="Prognose fra" type="date" value={asOf} onChange={event=>setAsOf(event.target.value)}/></label>
          <p className="muted">Primo {commitments.data.forecast.openingBalanceVerified ? formatKroner(commitments.data.forecast.openingCash, currency) : "— (ikke verificeret)"} · laveste basepunkt {formatKroner(commitments.data.forecast.lowestPoint, currency)}. Base er kanoniske cash-kilder; scenarieultimo tilføjer kun daterede, reviewede antagelser.</p>
          <div className="card statement-card table-scroll"><table className="data"><thead><tr><th>Uge</th><th className="num">Tilgodehavender</th><th className="num">Kreditorer</th><th className="num">Forpligtelser</th><th className="num">Base ultimo</th><th className="num">Scenarie ultimo</th></tr></thead><tbody>{commitments.data.forecast.periods.map(p=><tr key={p.weekStart}><td>{p.weekStart}</td><td className="num">{formatKroner(p.receivables,currency)}</td><td className="num">{formatKroner(p.payables,currency)}</td><td className="num">{formatKroner(p.commitments+p.obligations,currency)}</td><td className="num">{formatKroner(p.closingCash,currency)}</td><td className="num">{formatKroner(p.scenarioClosingCash,currency)}</td></tr>)}</tbody></table></div>
          <details><summary>Antagelser, forpligtelser og kilder</summary><div className="card statement-card table-scroll"><table className="data"><thead><tr><th>Uge</th><th className="num">Dateret budget</th><th className="num">Scenarie</th><th className="num">Intercompany</th><th className="num">Udateret månedsbudget</th><th>Kilder</th></tr></thead><tbody>{commitments.data.forecast.periods.map(p=><tr key={`sources:${p.weekStart}`}><td>{p.weekStart}</td><td className="num">{formatKroner(p.budgets,currency)}</td><td className="num">{formatKroner(p.scenarios,currency)}</td><td className="num">{formatKroner(p.intercompany,currency)}</td><td className="num">{formatKroner(p.undatedBudgetAssumptions,currency)}</td><td>{p.sources.map(source=><div key={`${source.source}:${source.reference}`}><code>{source.source}</code> · {formatKroner(source.amount,currency)} · <code>{source.reference}</code>{source.assumption?" · antagelse":""}{source.settlementStatus==="unknown"?" · settlement ukendt":""}</div>)}</td></tr>)}</tbody></table></div></details>
          <h3>Abonnementer og leverandørforpligtelser</h3>
          {commitments.data.alerts.length>0&&<div className="card"><h4>Fornyelse og opsigelse</h4><ul>{commitments.data.alerts.map(alert=><li key={`${alert.commitmentId}:${alert.kind}:${alert.date}`}><strong>{alert.date}</strong> · {alert.kind} · <code>{alert.commitmentId}</code></li>)}</ul></div>}
          {commitments.data.commitments.length===0?<p className="muted">Ingen godkendte forpligtelser.</p>:<div className="card statement-card table-scroll"><table className="data"><thead><tr><th>Leverandør</th><th>Formål</th><th className="num">Beløb</th><th>Frekvens</th><th>Næste</th><th>Fornyelse</th><th>Bilag</th><th>Handling</th></tr></thead><tbody>{commitments.data.commitments.map(c=><tr key={c.commitmentId}><td>{c.vendor}</td><td>{c.purpose}</td><td className="num">{c.amount===null?"—":formatKroner(c.amount,c.currency??currency)}</td><td>{c.frequency}</td><td>{c.nextDate}</td><td>{c.renewalDate??"—"}</td><td>{c.evidenceRefs.length?c.evidenceRefs.join(", "):"Mangler"}</td><td><button type="button" className="btn secondary" onClick={()=>setPending({commitmentId:c.commitmentId,action:"paused"})}>Pause</button> <button type="button" className="btn secondary" onClick={()=>setPending({commitmentId:c.commitmentId,action:"ended"})}>Afslut</button></td></tr>)}</tbody></table></div>}
          {commitments.data.commitments.length>0&&<details><summary>Match faktisk bilag, kreditor eller bankpost</summary><CommitmentMatchForm slug={slug} rows={commitments.data.commitments} onDone={commitments.reload}/></details>}
          {commitments.data.matches.length>0&&<div className="card"><h4>Faktisk mod forventet</h4><ul>{commitments.data.matches.map(match=><li key={`${match.commitmentId}:${match.occurrenceDate}`}><code>{match.commitmentId}</code> · {match.occurrenceDate} · {match.variance.dateDays} dage · {match.variance.amount===null?"valuta kan ikke sammenlignes":formatKroner(match.variance.amount,currency)} · bilag {match.variance.documentation}</li>)}</ul></div>}
          <p className="muted">Udeladt: {commitments.data.forecast.completeness.excluded.join("; ")}</p>
          {pending&&<ConfirmDialog title={pending.action==="paused"?"Pause forpligtelse":"Afslut forpligtelse"} body={<p>Ændringen er append-only og påvirker kun fremtidige forecast-occurrences.</p>} confirmLabel={pending.action==="paused"?"Pause":"Afslut"} confirmKind={pending.action==="ended"?"danger":"primary"} noteLabel="Begrundelse" onConfirm={async reason=>{if(!reason)throw new Error("Begrundelse er påkrævet.");await api.supplierCommitmentChange(slug,{...pending,reason});commitments.reload();}} onClose={()=>setPending(null)}/>}
        </section>
      )}

      {cf.archived ? (
        <ArchivedNotice year={cf.selectedYear} />
      ) : !cf.hasTransactions ? (
        <div className="card archived-notice">
          <h3>Ingen pengestrøm</h3>
          <p className="muted">
            Der er ingen banktransaktioner i regnskabsåret {cf.selectedYear}.
            Når et kontoudtog er importeret, vises penge ind og ud og den
            faktiske bankudvikling her.
          </p>
        </div>
      ) : (
        <>
          <p className="statement-asof muted">
            Faktiske penge ind og ud — regnskabsår {cf.selectedYear}
          </p>

          <div className="status-grid cashflow-summary">
            <div className="card status-card">
              <h3>Primo-saldo</h3>
              <div className="status-figure">
                {cf.openingBalance === null
                  ? "—"
                  : formatKroner(cf.openingBalance, currency)}
              </div>
              <p className="muted status-note">
                Faktisk banksaldo ved årets start
              </p>
            </div>
            <div className="card status-card">
              <h3>Indbetalinger</h3>
              <div className="status-figure status-in">
                {formatKroner(cf.totalIn, currency)}
              </div>
              <p className="muted status-note">Penge ind i året</p>
            </div>
            <div className="card status-card">
              <h3>Udbetalinger</h3>
              <div className="status-figure status-out">
                {formatKroner(cf.totalOut, currency)}
              </div>
              <p className="muted status-note">Penge ud af året</p>
            </div>
            <div className="card status-card">
              <h3>Ultimo-saldo</h3>
              <div className="status-figure">
                {cf.closingBalance === null
                  ? "—"
                  : formatKroner(cf.closingBalance, currency)}
              </div>
              <p className="muted status-note">
                Faktisk banksaldo ved årets slut
              </p>
            </div>
          </div>

          <div className="section">
            <h3>Pengestrøm og banksaldo — {cf.selectedYear}</h3>
            <div className="card chart-card">
              <CashflowChart
                months={cf.months}
                balanceByMonth={monthlyBalances(cf)}
              />
            </div>
          </div>

          <div className="card statement-card table-scroll">
            <table className="data statement-table">
              <thead>
                <tr>
                  <th>Måned</th>
                  <th className="num">Indbetalinger</th>
                  <th className="num">Udbetalinger</th>
                  <th className="num">Netto</th>
                </tr>
              </thead>
              <tbody>
                {cf.months.map((m) => (
                  <tr key={m.month}>
                    <td>{m.label}</td>
                    <td className="num">
                      {formatKroner(m.indbetalinger, currency)}
                    </td>
                    <td className="num">
                      {formatKroner(m.udbetalinger, currency)}
                    </td>
                    <td className="num">
                      {formatKroner(m.netto, currency)}
                    </td>
                  </tr>
                ))}
                <tr
                  className={`statement-result ${
                    netto >= 0 ? "" : "negative"
                  }`}
                >
                  <td>I alt</td>
                  <td className="num">{formatKroner(cf.totalIn, currency)}</td>
                  <td className="num">
                    {formatKroner(cf.totalOut, currency)}
                  </td>
                  <td className="num">{formatKroner(netto, currency)}</td>
                </tr>
              </tbody>
            </table>
          </div>
          <p className="statement-check ok">
            Tallene er faktiske pengebevægelser fra kontoudtoget — ikke det
            bogførte resultat. Pengestrømmen kan derfor afvige fra
            resultatopgørelsen.
          </p>
        </>
      )}
    </section>
  );
}

function ArchivedNotice({ year }: { year: string }) {
  return (
    <div className="card archived-notice">
      <h3>Likviditet er ikke tilgængelig for {year}</h3>
      <p className="muted">
        {year} er et arkiveret regnskabsår. Likviditet bygger på de importerede
        banktransaktioner, og der findes ingen kontoudtogsdata for et arkiveret
        år — pengestrømmen vises derfor ikke. Resultatopgørelse, balance,
        saldobalance og posteringer for {year} er tilgængelige.
      </p>
    </div>
  );
}
