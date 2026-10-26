import { cockpitStyles } from "../design/cockpit.stylex";
import * as stylex from "@stylexjs/stylex";
import type {NativeCashflow} from "../lib/types/native-reports";
import { Button, ButtonLink, Input, PageHeader, Select } from "../components/ui";
import { useOptionalAuth } from "../lib/auth-context";
import { useMutationOutcome } from "../lib/useMutationOutcome";
import { useUnsavedChanges } from "../lib/useUnsavedChanges";
import { NativeLiquidityView } from "./NativeLiquidityView";

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

import { type FormEvent, useState } from "react";
import { useParams } from "react-router-dom";
import { CashflowChart } from "../components/CashflowChart";
import { CompanyNav, useCompanyYear } from "../components/CompanyNav";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { ErrorState, Loading } from "../components/Feedback";
import { api } from "../lib/api";
import { formatKroner } from "../lib/format";
import type { CompanyCashflow } from "../lib/types";
import { useAsync } from "../lib/useAsync";

function CommitmentMatchForm({slug,rows,onDone}:{slug:string;rows:Array<{commitmentId:string;vendor?:string;purpose?:string}>;onDone:()=>void}) {
  const [commitmentId,setCommitmentId]=useState(rows[0]?.commitmentId??"");const [occurrenceDate,setOccurrenceDate]=useState("");const [kind,setKind]=useState<"canonical_document"|"payable"|"bank_transaction">("canonical_document");const [evidenceId,setEvidenceId]=useState("");const [confirmed,setConfirmed]=useState(false);const [message,setMessage]=useState("");
  const outcome = useMutationOutcome(onDone);
  const markSaved = useUnsavedChanges(Boolean(occurrenceDate || evidenceId || confirmed));
  const submit=async(event:FormEvent)=>{event.preventDefault(); if (outcome.isBlocked()) return;try{await api.supplierCommitmentMatch(slug,{commitmentId,occurrenceDate,evidence:{kind,id:evidenceId}}).catch(outcome.reject);setMessage("Canonical evidence er matchet append-only.");setOccurrenceDate("");setEvidenceId("");setConfirmed(false);markSaved();onDone();}catch(cause){setMessage(cause instanceof Error?cause.message:String(cause));}};
  return <form onSubmit={event=>void submit(event)} {...stylex.props(cockpitStyles.rowActions)}>
    {outcome.feedback}<Select disabled={outcome.blocked} aria-label="Forpligtelse" value={commitmentId} onChange={event=>setCommitmentId(event.target.value)}>{rows.map(row=><option key={row.commitmentId} value={row.commitmentId} {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>{row.vendor && row.purpose ? `${row.vendor} · ${row.purpose}` : row.commitmentId}</option>)}</Select><Input disabled={outcome.blocked} aria-label="Forventet dato" type="date" value={occurrenceDate} onChange={event=>setOccurrenceDate(event.target.value)}/><Select disabled={outcome.blocked} aria-label="Evidenstype" value={kind} onChange={event=>setKind(event.target.value as typeof kind)}><option value="canonical_document" {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>Bilag</option><option value="payable" {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>Kreditor</option><option value="bank_transaction" {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>Bankpost</option></Select><Input disabled={outcome.blocked} aria-label="Canonical evidence-id" value={evidenceId} onChange={event=>setEvidenceId(event.target.value)}/><label {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}><Input disabled={outcome.blocked} type="checkbox" checked={confirmed} onChange={event=>setConfirmed(event.target.checked)}/> Bekræft match</label><Button requiredPermission="company.draft.write" variant="secondary" type="submit"  disabled={outcome.blocked || (!confirmed||!commitmentId||!occurrenceDate||!evidenceId)}>Match faktisk occurrence</Button>{message&&<span {...stylex.props(cockpitStyles.muted)}>{message}</span>}</form>;
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

function LegacyLiquidityView() {
  const { slug = "" } = useParams();
  const { year, setYear } = useCompanyYear();
  const [asOf, setAsOf] = useState(() => new Date().toISOString().slice(0, 10));
  const [pending, setPending] = useState<{commitmentId:string;action:"paused"|"ended";expectedPayloadHash:string}|null>(null);
  const state = useAsync<CompanyCashflow>(
    (signal) => api.cashflow(slug, year, { signal }),
    [slug, year],
  );
  const commitments = useAsync(
    (signal) => api.supplierCommitments(slug, asOf, { signal }),
    [slug, asOf],
  );

  if (state.loading && !state.data)
    return <Loading label="Henter likviditet…" />;
  if (state.error)
    return <ErrorState message={state.error} onRetry={state.reload} />;

  const cf = state.data!;
  const currency = cf.company.currency || "DKK";
  const netto = cf.totalIn - cf.totalOut;
  const forecastAmount=(value:number|null)=>value===null?"Ikke tilgængelig":formatKroner(value,currency);
  const balances = monthlyBalances(cf);

  return (
    <section {...stylex.props(cockpitStyles.statement)}>
      <PageHeader title="Likviditet" actions={<><div {...stylex.props(cockpitStyles.rowActions)}>
          <ButtonLink  to={`/companies/${slug}/manage`}>
            Administrér
          </ButtonLink>
        </div></>}>
        <div {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>

          <p {...stylex.props(cockpitStyles.muted)}>
            {cf.company.cvr ? `CVR ${cf.company.cvr} · ` : ""}
            {cf.company.country} · {currency} · Likviditet
          </p>
        </div>

      </PageHeader>

      <CompanyNav
        slug={slug}
        years={cf.fiscalYears}
        selectedYear={cf.selectedYear}
        onYearChange={setYear}
      />

      {commitments.data && (
        <section {...stylex.props(cockpitStyles.section)}>
          <h3 {...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.h3,
							cockpitStyles.sectionH3,
						)}>13 ugers likviditetsprognose</h3>
          <label {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>Prognose fra <Input aria-label="Prognose fra" type="date" value={asOf} onChange={event=>setAsOf(event.target.value)}/></label>
          <p {...stylex.props(cockpitStyles.muted)}>Primo {forecastAmount(commitments.data.forecast.openingCash)} · laveste basepunkt {forecastAmount(commitments.data.forecast.lowestPoint)}. Base er kanoniske cash-kilder; scenarieultimo tilføjer kun daterede, reviewede antagelser.</p>
          <div {...stylex.props(cockpitStyles.card, cockpitStyles.statementCard, cockpitStyles.tableScroll)}><table {...stylex.props(...[cockpitStyles.tableData])}><thead {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}><tr {...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}><th {...stylex.props(
											cockpitStyles.statementTableScrollTableThComposition7,
										)}>Uge</th><th {...stylex.props(cockpitStyles.num)}>Tilgodehavender</th><th {...stylex.props(cockpitStyles.num)}>Kreditorer</th><th {...stylex.props(cockpitStyles.num)}>Forpligtelser</th><th {...stylex.props(cockpitStyles.num)}>Base ultimo</th><th {...stylex.props(cockpitStyles.num)}>Scenarie ultimo</th></tr></thead><tbody {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>{commitments.data.forecast.periods.map(p=><tr key={p.weekStart} {...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}><td {...stylex.props(
												cockpitStyles.statementTableScrollTableTdComposition11,
											)}>{p.weekStart}</td><td {...stylex.props(cockpitStyles.num)}>{formatKroner(p.receivables,currency)}</td><td {...stylex.props(cockpitStyles.num)}>{formatKroner(p.payables,currency)}</td><td {...stylex.props(cockpitStyles.num)}>{formatKroner(p.commitments+p.obligations,currency)}</td><td {...stylex.props(cockpitStyles.num)}>{forecastAmount(p.closingCash)}</td><td {...stylex.props(cockpitStyles.num)}>{forecastAmount(p.scenarioClosingCash)}</td></tr>)}</tbody></table></div>
          <details {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}><summary {...stylex.props(cockpitStyles.summaryComposition)}>Antagelser, forpligtelser og kilder</summary><div {...stylex.props(cockpitStyles.card, cockpitStyles.statementCard, cockpitStyles.tableScroll)}><table {...stylex.props(...[cockpitStyles.tableData])}><thead {...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}><tr {...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}><th {...stylex.props(
												cockpitStyles.statementTableScrollTableThComposition7,
											)}>Uge</th><th {...stylex.props(cockpitStyles.num)}>Dateret budget</th><th {...stylex.props(cockpitStyles.num)}>Scenarie</th><th {...stylex.props(cockpitStyles.num)}>Intercompany</th><th {...stylex.props(cockpitStyles.num)}>Udateret månedsbudget</th><th {...stylex.props(
												cockpitStyles.statementTableScrollTableThComposition7,
											)}>Kilder</th></tr></thead><tbody {...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}>{commitments.data.forecast.periods.map(p=><tr key={`sources:${p.weekStart}`} {...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}><td {...stylex.props(
													cockpitStyles.statementTableScrollTableTdComposition11,
												)}>{p.weekStart}</td><td {...stylex.props(cockpitStyles.num)}>{formatKroner(p.budgets,currency)}</td><td {...stylex.props(cockpitStyles.num)}>{formatKroner(p.scenarios,currency)}</td><td {...stylex.props(cockpitStyles.num)}>{formatKroner(p.intercompany,currency)}</td><td {...stylex.props(cockpitStyles.num)}>{formatKroner(p.undatedBudgetAssumptions,currency)}</td><td {...stylex.props(
													cockpitStyles.statementTableScrollTableTdComposition11,
												)}>{p.sources.map(source=><div key={`${source.source}:${source.reference}`} {...stylex.props(
															cockpitStyles.element,
															cockpitStyles.focusVisible,
														)}><code {...stylex.props(
																cockpitStyles.element,
																cockpitStyles.focusVisible,
																cockpitStyles.code,
															)}>{source.source}</code> · {formatKroner(source.amount,currency)} · <code {...stylex.props(
																cockpitStyles.element,
																cockpitStyles.focusVisible,
																cockpitStyles.code,
															)}>{source.reference}</code>{source.assumption?" · antagelse":""}{source.settlementStatus==="unknown"?" · settlement ukendt":""}</div>)}</td></tr>)}</tbody></table></div></details>
          <h3 {...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.h3,
							cockpitStyles.sectionH3,
						)}>Abonnementer og leverandørforpligtelser</h3>
          {commitments.data.alerts.length>0&&<div {...stylex.props(cockpitStyles.card)}><h4 {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.h4,
								)}>Fornyelse og opsigelse</h4><ul {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>{commitments.data.alerts.map(alert=><li key={`${alert.commitmentId}:${alert.kind}:${alert.date}`} {...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}><strong {...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}>{alert.date}</strong> · {alert.kind} · <code {...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
												cockpitStyles.code,
											)}>{alert.commitmentId}</code></li>)}</ul></div>}
          {commitments.data.commitments.length===0?<p {...stylex.props(cockpitStyles.muted)}>Ingen godkendte forpligtelser.</p>:<div {...stylex.props(cockpitStyles.card, cockpitStyles.statementCard, cockpitStyles.tableScroll)}><table {...stylex.props(...[cockpitStyles.tableData])}><thead {...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}><tr {...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}><th {...stylex.props(
												cockpitStyles.statementTableScrollTableThComposition7,
											)}>Leverandør</th><th {...stylex.props(
												cockpitStyles.statementTableScrollTableThComposition7,
											)}>Formål</th><th {...stylex.props(cockpitStyles.num)}>Beløb</th><th {...stylex.props(
												cockpitStyles.statementTableScrollTableThComposition7,
											)}>Frekvens</th><th {...stylex.props(
												cockpitStyles.statementTableScrollTableThComposition7,
											)}>Næste</th><th {...stylex.props(
												cockpitStyles.statementTableScrollTableThComposition7,
											)}>Fornyelse</th><th {...stylex.props(
												cockpitStyles.statementTableScrollTableThComposition7,
											)}>Bilag</th><th {...stylex.props(
												cockpitStyles.statementTableScrollTableThComposition7,
											)}>Handling</th></tr></thead><tbody {...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}>{commitments.data.commitments.map(c=><tr key={c.commitmentId} {...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}><td {...stylex.props(
													cockpitStyles.statementTableScrollTableTdComposition11,
												)}>{c.vendor}</td><td {...stylex.props(
													cockpitStyles.statementTableScrollTableTdComposition11,
												)}>{c.purpose}</td><td {...stylex.props(cockpitStyles.num)}>{c.amount===null?"—":formatKroner(c.amount,c.currency??currency)}</td><td {...stylex.props(
													cockpitStyles.statementTableScrollTableTdComposition11,
												)}>{c.frequency}</td><td {...stylex.props(
													cockpitStyles.statementTableScrollTableTdComposition11,
												)}>{c.nextDate}</td><td {...stylex.props(
													cockpitStyles.statementTableScrollTableTdComposition11,
												)}>{c.renewalDate??"—"}</td><td {...stylex.props(
													cockpitStyles.statementTableScrollTableTdComposition11,
												)}>{c.evidenceRefs.length?c.evidenceRefs.join(", "):"Mangler"}</td><td {...stylex.props(
													cockpitStyles.statementTableScrollTableTdComposition11,
												)}><Button requiredPermission="company.draft.write" variant="secondary" type="button"  onClick={()=>setPending({commitmentId:c.commitmentId,action:"paused",expectedPayloadHash:c.payloadHash})}>Pause</Button> <Button requiredPermission="company.draft.write" variant="secondary" type="button"  onClick={()=>setPending({commitmentId:c.commitmentId,action:"ended",expectedPayloadHash:c.payloadHash})}>Afslut</Button></td></tr>)}</tbody></table></div>}
          {commitments.data.commitments.length>0&&<details {...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}><summary {...stylex.props(cockpitStyles.summaryComposition)}>Match faktisk bilag, kreditor eller bankpost</summary><CommitmentMatchForm slug={slug} rows={commitments.data.commitments} onDone={commitments.reload}/></details>}
          {commitments.data.matches.length>0&&<div {...stylex.props(cockpitStyles.card)}><h4 {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.h4,
								)}>Faktisk mod forventet</h4><ul {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>{commitments.data.matches.map(match=><li key={`${match.commitmentId}:${match.occurrenceDate}`} {...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}><code {...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
												cockpitStyles.code,
											)}>{match.commitmentId}</code> · {match.occurrenceDate} · {match.variance.dateDays} dage · {match.variance.amount===null?"valuta kan ikke sammenlignes":formatKroner(match.variance.amount,currency)} · bilag {match.variance.documentation}</li>)}</ul></div>}
          <p {...stylex.props(cockpitStyles.muted)}>Udeladt: {commitments.data.forecast.completeness.excluded.join("; ")}</p>
          {pending&&<ConfirmDialog title={pending.action==="paused"?"Pause forpligtelse":"Afslut forpligtelse"} body={<p {...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}>Ændringen er append-only og påvirker kun fremtidige forecast-occurrences.</p>} confirmLabel={pending.action==="paused"?"Pause":"Afslut"} confirmKind={pending.action==="ended"?"danger":"primary"} noteLabel="Begrundelse" onConfirm={async reason=>{if(!reason)throw new Error("Begrundelse er påkrævet.");await api.supplierCommitmentChange(slug,{...pending,reason});commitments.reload();}} onClose={()=>setPending(null)} onRefresh={commitments.reload}/>}
        </section>
      )}

      {cf.archived ? (
        <ArchivedNotice year={cf.selectedYear} />
      ) : !cf.hasTransactions ? (
        <div {...stylex.props(cockpitStyles.card, cockpitStyles.archivedNotice)}>
          <h3 {...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.h3,
							cockpitStyles.archivedNoticeH3,
						)}>Ingen pengestrøm</h3>
          <p {...stylex.props(cockpitStyles.muted)}>
            Der er ingen banktransaktioner i regnskabsåret {cf.selectedYear}.
            Når et kontoudtog er importeret, vises penge ind og ud og den
            faktiske bankudvikling her.
          </p>
        </div>
      ) : (
        <>
          <p {...stylex.props(cockpitStyles.statementAsof, cockpitStyles.muted)}>
            Faktiske penge ind og ud — regnskabsår {cf.selectedYear}
          </p>

          <div {...stylex.props(cockpitStyles.statusGrid)}>
            <div {...stylex.props(cockpitStyles.card, cockpitStyles.statusCard)}>
              <h3 {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.h3,
									cockpitStyles.statusCardH3,
								)}>Primo-saldo</h3>
              <div {...stylex.props(cockpitStyles.statusFigure)}>
                {cf.openingBalance === null
                  ? "—"
                  : formatKroner(cf.openingBalance, currency)}
              </div>
              <p {...stylex.props(cockpitStyles.muted, cockpitStyles.statusNote)}>
                Faktisk banksaldo ved årets start
              </p>
            </div>
            <div {...stylex.props(cockpitStyles.card, cockpitStyles.statusCard)}>
              <h3 {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.h3,
									cockpitStyles.statusCardH3,
								)}>Indbetalinger</h3>
              <div {...stylex.props(cockpitStyles.statusFigure)}>
                {formatKroner(cf.totalIn, currency)}
              </div>
              <p {...stylex.props(cockpitStyles.muted, cockpitStyles.statusNote)}>Penge ind i året</p>
            </div>
            <div {...stylex.props(cockpitStyles.card, cockpitStyles.statusCard)}>
              <h3 {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.h3,
									cockpitStyles.statusCardH3,
								)}>Udbetalinger</h3>
              <div {...stylex.props(cockpitStyles.statusFigure)}>
                {formatKroner(cf.totalOut, currency)}
              </div>
              <p {...stylex.props(cockpitStyles.muted, cockpitStyles.statusNote)}>Penge ud af året</p>
            </div>
            <div {...stylex.props(cockpitStyles.card, cockpitStyles.statusCard)}>
              <h3 {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.h3,
									cockpitStyles.statusCardH3,
								)}>Ultimo-saldo</h3>
              <div {...stylex.props(cockpitStyles.statusFigure)}>
                {cf.closingBalance === null
                  ? "—"
                  : formatKroner(cf.closingBalance, currency)}
              </div>
              <p {...stylex.props(cockpitStyles.muted, cockpitStyles.statusNote)}>
                Faktisk banksaldo ved årets slut
              </p>
            </div>
          </div>

          <div {...stylex.props(cockpitStyles.section)}>
            <h3 {...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.h3,
								cockpitStyles.sectionH3,
							)}>Pengestrøm og banksaldo — {cf.selectedYear}</h3>
            <div {...stylex.props(cockpitStyles.card, cockpitStyles.chartCard)}>
              <CashflowChart
                months={cf.months}
                balanceByMonth={balances}
                currency={currency}
                dataTableId="cashflow-months"
              />
            </div>
          </div>

          <div {...stylex.props(cockpitStyles.card, cockpitStyles.statementCard, cockpitStyles.tableScroll)}>
            <table id="cashflow-months" {...stylex.props(...[cockpitStyles.tableData, cockpitStyles.tableStatementTable])}>
              <caption {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>Pengestrøm og banksaldo pr. måned ({currency})</caption>
              <thead {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>
                <tr {...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}>
                  <th {...stylex.props(
											cockpitStyles.statementTableScrollTableThComposition,
										)}>Måned</th>
                  <th {...stylex.props(cockpitStyles.num)}>Indbetalinger</th>
                  <th {...stylex.props(cockpitStyles.num)}>Udbetalinger</th>
                  <th {...stylex.props(cockpitStyles.num)}>Netto</th>
                  <th {...stylex.props(cockpitStyles.num)}>Banksaldo ultimo</th>
                </tr>
              </thead>
              <tbody {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>
                {cf.months.map((m, index) => (
                  <tr key={m.month} {...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}>
                    <td {...stylex.props(
												cockpitStyles.statementTableScrollTableTdComposition,
											)}>{m.label}</td>
                    <td {...stylex.props(cockpitStyles.num)}>
                      {formatKroner(m.indbetalinger, currency)}
                    </td>
                    <td {...stylex.props(cockpitStyles.num)}>
                      {formatKroner(m.udbetalinger, currency)}
                    </td>
                    <td {...stylex.props(cockpitStyles.num)}>
                      {formatKroner(m.netto, currency)}
                    </td>
                    <td {...stylex.props(cockpitStyles.num)}>{balances[index] == null ? "—" : formatKroner(balances[index]!, currency)}</td>
                  </tr>
                ))}
                <tr
                  {...stylex.props(...[cockpitStyles.statementResultTd, ...(netto >= 0 ? [] : [cockpitStyles.amountNegative])])}
                >
                  <td {...stylex.props(
											cockpitStyles.statementTableScrollTableTdComposition10,
										)}>I alt</td>
                  <td {...stylex.props(cockpitStyles.num)}>{formatKroner(cf.totalIn, currency)}</td>
                  <td {...stylex.props(cockpitStyles.num)}>
                    {formatKroner(cf.totalOut, currency)}
                  </td>
                  <td {...stylex.props(cockpitStyles.num)}>{formatKroner(netto, currency)}</td>
                  <td {...stylex.props(cockpitStyles.num)}>{cf.closingBalance == null ? "—" : formatKroner(cf.closingBalance, currency)}</td>
                </tr>
              </tbody>
            </table>
          </div>
          <p {...stylex.props(cockpitStyles.statementCheck)}>
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
    <div {...stylex.props(cockpitStyles.card, cockpitStyles.archivedNotice)}>
      <h3 {...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.h3,
					cockpitStyles.archivedNoticeH3,
				)}>Likviditet er ikke tilgængelig for {year}</h3>
      <p {...stylex.props(cockpitStyles.muted)}>
        {year} er et arkiveret regnskabsår. Likviditet bygger på de importerede
        banktransaktioner, og der findes ingen kontoudtogsdata for et arkiveret
        år — pengestrømmen vises derfor ikke. Resultatopgørelse, balance,
        saldobalance og posteringer for {year} er tilgængelige.
      </p>
    </div>
  );
}

function NativeHistoricalCashflow(){
 const {slug=""}=useParams();const {year,setYear}=useCompanyYear();
 const state=useAsync(signal=>api.nativeCashflow(slug,year,{signal}),[slug,year]);
 if(state.loading&&!state.data)return <Loading label="Henter historisk pengestrøm…"/>;
 if(state.error)return <ErrorState message={state.error} onRetry={state.reload}/>;
 const count=(value:number|null)=>value===null?"ukendt":String(value);
 const cf:NativeCashflow=state.data!;const amount=(value:number|null)=>value===null?"Ikke tilgængelig":formatKroner(value,cf.currency);
 const chartReady=cf.months.length>0&&cf.months.every(month=>month.indbetalinger!==null&&month.udbetalinger!==null&&month.netto!==null);
 return <section {...stylex.props(cockpitStyles.statement)} aria-label="Historisk pengestrøm"><PageHeader title="Historisk pengestrøm"/><p>Faktiske importerede bankposteringer · {cf.periodStart??"Periode ukendt"} – {cf.periodEnd??"Periode ukendt"}. Dette er historik, ikke en prognose.</p>
  <CompanyNav slug={slug} years={cf.fiscalYears} selectedYear={cf.selectedYear} onYearChange={setYear}/>
  {cf.status!=="complete"&&<p role="status">Grundlaget er {cf.status==="partial"?"delvist":"ikke tilgængeligt"}. Ukendte beløb vises ikke som nul.</p>}
  {cf.limitations.map(message=><p key={message}>{message}</p>)}
  <div {...stylex.props(cockpitStyles.rollupStrip)}>{[['Primo',amount(cf.openingBalance)],['Indbetalinger',amount(cf.totalIn)],['Udbetalinger',amount(cf.totalOut)],['Ultimo',amount(cf.closingBalance)]].map(([label,value])=><div {...stylex.props(cockpitStyles.rollupCell)} key={label}><span {...stylex.props(...[cockpitStyles.rollupCellRollupLabel])}>{label}</span><span {...stylex.props(...[cockpitStyles.rollupCellRollupValue])}>{value}</span></div>)}</div>
  <p>{count(cf.coverage.valuedTransactionCount)} værdiansatte af {count(cf.coverage.transactionCount)} bankposteringer · {count(cf.coverage.unvaluedTransactionCount)} uden kendt DKK-værdi.</p>
  {chartReady&&<CashflowChart months={cf.months.map(month=>({...month,indbetalinger:month.indbetalinger!,udbetalinger:month.udbetalinger!,netto:month.netto!}))} balanceByMonth={[]} currency={cf.currency} dataTableId="native-cashflow-months"/>}
  <table {...stylex.props(...[cockpitStyles.tableData])} id="native-cashflow-months"><caption>Faktiske månedlige bankbevægelser</caption><thead><tr><th>Måned</th><th>Indbetalinger</th><th>Udbetalinger</th><th>Netto</th></tr></thead><tbody>{cf.months.map(month=><tr key={month.label}><th scope="row">{month.label}</th><td>{amount(month.indbetalinger)}</td><td>{amount(month.udbetalinger)}</td><td>{amount(month.netto)}</td></tr>)}</tbody></table>
  <h3>Uafhængige kontoudtogssaldi</h3>{cf.balanceReason.series&&<p>{cf.balanceReason.series==="aggregate_statement_evidence_unavailable"?"Nogle datoer mangler en entydig samlet saldo og er udeladt fra saldoserien.":cf.balanceReason.series}</p>}{cf.balanceSeries.length?<table {...stylex.props(...[cockpitStyles.tableData])}><thead><tr><th>Dato</th><th>Saldo</th></tr></thead><tbody>{cf.balanceSeries.map(point=><tr key={point.date}><td>{point.date}</td><td>{amount(point.balance)}</td></tr>)}</tbody></table>:<p>Ingen kendt samlet saldoserie fra kontoudtog.</p>}
 </section>;
}
export function LiquidityView(){const auth=useOptionalAuth();return auth&&auth.backend!=="rust"?<LegacyLiquidityView/>:<><NativeHistoricalCashflow/><NativeLiquidityView showCompanyNavigation={false}/></>;}
