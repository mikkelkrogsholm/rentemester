import { cockpitStyles } from "../design/cockpit.stylex";
import * as stylex from "@stylexjs/stylex";
import { Button, ButtonLink, Input, PageHeader, Select } from "../components/ui";
import { Link } from "react-router-dom";
import { useMemo, useState } from "react";
import { ErrorState, Loading } from "../components/Feedback";
import { api } from "../lib/api";
import { formatKroner, todayIso } from "../lib/format";
import type { CfoAnalyticsResponse, CfoAnalyticsScope } from "../lib/types";
import { useAsync } from "../lib/useAsync";

const amountText=(value:number|string,currency?:string)=>currency?formatKroner(value,currency):"—";

function startOfYear(day: string): string { return `${day.slice(0, 4)}-01-01`; }

/** A read-only operational projection. Financial calculations stay in #581/core. */
export function CfoCockpitView() {
  const today = useMemo(todayIso, []);
  const [selectedProfile, setSelectedProfile] = useState("");
  const [cursor, setCursor] = useState<string | null>(null);
  const [scope, setScope] = useState<CfoAnalyticsScope>("company");
  const [companySlug, setCompanySlug] = useState("");
  const [asOf, setAsOf] = useState(today);
  const [from, setFrom] = useState(startOfYear(today));
  const companiesState = useAsync((signal) => api.companies({ signal }), []);
  const companies = companiesState.data?.filter((company) => !company.archived) ?? [];
  const effectiveCompanySlug = companySlug || companies[0]?.slug || "";
  const profilesState = useAsync(
    (signal) => scope === "group" ? api.groupReportProfiles(asOf, { signal }) : Promise.resolve(null),
    [scope, asOf],
  );
  const profiles = profilesState.data?.profiles ?? [];
  const profileId = profiles.some(p => p.id === selectedProfile) ? selectedProfile : profiles[0]?.id;
  const analyticsState = useAsync<CfoAnalyticsResponse | null>(
    (signal) => {
      if (scope === "company" && !effectiveCompanySlug) return Promise.resolve(null);
      if (scope === "group" && !profileId) return Promise.resolve(null);
      return api.cfoAnalytics({ scope, from, to: asOf, ...(cursor ? {cursor} : {}), ...(scope === "company" ? { companySlug: effectiveCompanySlug } : {}), ...(scope === "group" ? { groupProfileId: profileId } : {}) }, { signal });
    },
    [scope, effectiveCompanySlug, from, asOf, profileId, cursor],
  );

  const loading = companiesState.loading || profilesState.loading || analyticsState.loading;
  const error = companiesState.error || profilesState.error || analyticsState.error;
  return <section {...stylex.props(...[cockpitStyles.statement])}>
    <PageHeader title="CFO-overblik">
      <div {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}><p {...stylex.props(cockpitStyles.muted)}>Read-only, kildehenvisende overblik. Bogføring sker fortsat i den enkelte virksomheds normale flow.</p></div>
    </PageHeader>
    <div {...stylex.props(cockpitStyles.rowActions)} role="group" aria-label="CFO-afgrænsning">
      <label {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>Visning<Select aria-label="Visning" value={scope} onChange={(event) => {setScope(event.target.value as CfoAnalyticsScope);setCursor(null);}}><option value="company" {...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}>Virksomhed</option><option value="portfolio" {...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}>Portefølje</option><option value="group" {...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}>Koncern</option></Select></label>
      {scope === "company" && <label {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>Virksomhed<Select aria-label="Virksomhed" value={effectiveCompanySlug} onChange={(event) => {setCompanySlug(event.target.value);setCursor(null);}}>{companies.map((company) => <option key={company.slug} value={company.slug} {...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}>{company.name}</option>)}</Select></label>}
      {scope === "group" && profiles.length > 0 && <label {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>Koncernprofil<Select aria-label="Koncernprofil" value={profileId} onChange={event => {setSelectedProfile(event.target.value);setCursor(null);}}>{profiles.map(p=><option key={p.id} value={p.id}>{p.id} · {p.currency}</option>)}</Select></label>}
      <label>Fra<Input aria-label="Fra dato" type="date" value={from} max={asOf} onChange={(event) => {setFrom(event.target.value);setCursor(null);}} /></label>
      <label {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>Pr. dato<Input aria-label="Pr. dato" type="date" value={asOf} min={from} onChange={(event) => {setAsOf(event.target.value);setCursor(null);}} /></label>
    </div>
    {loading && <Loading label="Henter kildehenvisende CFO-overblik…" />}
    {!loading && error && <ErrorState message="CFO-overblikket kan ikke vises sikkert." onRetry={() => { companiesState.reload(); profilesState.reload(); analyticsState.reload(); }} />}
    {!loading && !error && scope === "company" && !effectiveCompanySlug && <p {...stylex.props(cockpitStyles.muted)}>Ingen tilgængelige virksomheder.</p>}
    {!loading && !error && scope === "group" && !profileId && <section {...stylex.props(cockpitStyles.banner)} role="status">Ingen godkendt koncernrapportprofil er tilgængelig pr. den valgte dato.</section>}
    {!loading && !error && analyticsState.data && <><CfoResult result={analyticsState.data} companyNames={new Map(companies.map((company) => [company.slug, company.name]))} />{analyticsState.data.scope !== "group" && <div {...stylex.props(cockpitStyles.rowActions)} aria-label="Kildeposter sider">{cursor && <Button variant="secondary" onClick={()=>setCursor(null)}>Første kildeposter</Button>}{analyticsState.data.page.nextCursor && <Button variant="secondary" onClick={()=>setCursor(analyticsState.data!.scope !== "group" ? analyticsState.data!.page.nextCursor : null)}>Næste kildeposter</Button>}</div>}</>}
  </section>;
}

function CfoResult({ result, companyNames }: { result: CfoAnalyticsResponse; companyNames: Map<string, string> }) {
  if (result.scope === "group") return <GroupResult result={result} />;
  const portfolio = result.scope === "portfolio";
  const title = portfolio ? "Portefølje — ikke konsolideret" : `Virksomhed: ${companyNames.get(result.companies[0] ?? "") ?? result.companies[0]}`;
  return <>
    <section {...stylex.props(cockpitStyles.card)} aria-label="Analyseafgrænsning"><h3 {...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.h3,
					)}>{title}</h3><p {...stylex.props(cockpitStyles.muted)}>Pr. {result.asOf} · periode {result.from} – {result.to} · schema <code {...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.code,
						)}>{result.schemaVersion}</code></p>{portfolio && <div {...stylex.props(cockpitStyles.banner)} role="status">Porteføljen er en sideordnet, ikke-konsolideret visning. Der vises ingen implicitte elimineringer eller valutaomregning.</div>}{result.status === "incomplete" && <div {...stylex.props(cockpitStyles.banner)} role="status">Ufuldstændigt udsnit: skjulte virksomheder og deres tal er ikke medtaget.</div>}<Freshness entries={result.freshness} />{result.limitations.map((reason,index)=><p key={index}>{reason}</p>)}</section>
    <Evidence result={result} />
    {result.rows.length === 0 ? <p {...stylex.props(cockpitStyles.muted)}>Ingen kildeposteringer i den valgte periode.</p> : <>
      {!result.partial && result.status === "ready" && (!portfolio || result.aggregate === "sum") && result.reconciliation.amountByCurrency && <section {...stylex.props(cockpitStyles.statusGrid)} aria-label="Kildebaserede beløb">{Object.entries(result.reconciliation.amountByCurrency).map(([currency, amount]) => <article {...stylex.props(cockpitStyles.card)} key={currency}><h3 {...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
												cockpitStyles.h3,
											)}>Analyseret bevægelse</h3><p {...stylex.props(cockpitStyles.amount)}>{amountText(amount, currency)}</p><p {...stylex.props(cockpitStyles.muted)}>{result.reconciliation.rowCount} kildeposter · {currency}</p></article>)}</section>}
      <SourceRows rows={result.rows} />
    </>}
  </>;
}

function Freshness({ entries }: { entries: Array<{ source: "ledger" | "archive"; companySlug: string; latestTransactionDate: string }> }) {
  if (!entries.length) return <p {...stylex.props(cockpitStyles.muted)}>Ingen kildekilder med posteringer i perioden.</p>;
  return <><p {...stylex.props(cockpitStyles.muted)}>{entries.map((entry) => `Seneste ${entry.source}: ${entry.latestTransactionDate} (${entry.companySlug})`).join(" · ")}</p><p {...stylex.props(cockpitStyles.muted)}>Stale-status kan ikke udledes af posteringernes dato alene; visningen viser derfor kun den seneste verificerede kildepost.</p></>;
}

function Evidence({ result }: { result: Extract<CfoAnalyticsResponse, { scope: "company" | "portfolio" }> }) {
  return <section {...stylex.props(cockpitStyles.statusGrid)} aria-label="Kontrol og genveje">{result.evidenceCompleteness.map((entry) => <article {...stylex.props(cockpitStyles.card)} key={entry.companySlug}><h3 {...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.h3,
						)}>Kontrol: {entry.companySlug}</h3>{entry.status === "unavailable" ? <p {...stylex.props(cockpitStyles.muted)}>Ikke tilgængelig: {entry.reason}</p> : <><p {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>{entry.postedWithoutDocument} bogførte poster uden bilag · {entry.openExceptions} åbne undtagelser</p><div {...stylex.props(cockpitStyles.rowActions)}><ButtonLink  to={`/companies/${entry.companySlug}/bank`}>Bank</ButtonLink><ButtonLink  to={`/companies/${entry.companySlug}/bilag`}>Bilag</ButtonLink><ButtonLink  to={`/companies/${entry.companySlug}/undtagelser`}>Undtagelser</ButtonLink><ButtonLink  to={`/companies/${entry.companySlug}/moms`}>Moms</ButtonLink><ButtonLink  to={`/companies/${entry.companySlug}/leverandoerfaktura`}>Leverandørfaktura</ButtonLink><ButtonLink  to={`/companies/${entry.companySlug}/budget`}>Budget</ButtonLink></div></>}</article>)}</section>;
}

function SourceRows({ rows }: { rows: Extract<CfoAnalyticsResponse, { scope: "company" | "portfolio" }> ["rows"] }) {
  return <section {...stylex.props(cockpitStyles.tableWrap)} aria-label="Kildeposter"><div {...stylex.props(cockpitStyles.tableScroll)}><table {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}><thead {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}><tr {...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}><th {...stylex.props(
									cockpitStyles.tableDataTh,
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>Dato</th><th {...stylex.props(
									cockpitStyles.tableDataTh,
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>Virksomhed</th><th {...stylex.props(
									cockpitStyles.tableDataTh,
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>Postering</th><th {...stylex.props(
									cockpitStyles.tableDataTh,
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>Modpart</th><th {...stylex.props(
									cockpitStyles.tableDataTh,
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>Beløb</th><th {...stylex.props(
									cockpitStyles.tableDataTh,
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>Kilde</th><th>Kildedetaljer</th></tr></thead><tbody {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>{rows.map((row) => <tr key={`${row.companySlug}:${row.sourceType}:${row.sourceId}`}><td {...stylex.props(
										cockpitStyles.tableDataTd,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}>{row.transactionDate}</td><td {...stylex.props(
										cockpitStyles.tableDataTd,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}><Link to={`/companies/${row.companySlug}`} {...stylex.props(cockpitStyles.aComposition)}>{row.companySlug}</Link></td><td {...stylex.props(
										cockpitStyles.tableDataTd,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}><Link aria-label={`Postering ${row.journalEntryNo ?? row.sourceId}`} to={`/companies/${row.companySlug}/posteringer?account=${encodeURIComponent(row.accountNo)}`} {...stylex.props(cockpitStyles.aComposition)}>{row.journalEntryNo ?? row.accountNo}</Link></td><td {...stylex.props(
										cockpitStyles.tableDataTd,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}>{row.partyName ?? "—"}</td><td {...stylex.props(cockpitStyles.num)}>{amountText(row.amount, row.currency)}</td><td {...stylex.props(
										cockpitStyles.tableDataTd,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}><code title={row.sourceHash} {...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.code,
										)}>{row.sourceType}</code></td><td><details><summary>Kildedetaljer {row.sourceId}</summary><p>Konto: {row.accountNo} · {row.accountName ?? "Navn ikke tilgængeligt"}</p><p>{row.text ?? "Ingen posteringstekst"}</p><p>Kilde-ID: {row.sourceId} · hash: <code>{row.sourceHash}</code></p>{row.documentId !== null && <p><a href={`/api/companies/${encodeURIComponent(row.companySlug)}/documents/${row.documentId}/file`} target="_blank" rel="noopener noreferrer">Åbn originalbilag</a> · bilagshash: <code>{row.documentHash ?? "Ikke tilgængelig"}</code></p>}{row.documentPartyName && <p>Udtrukket bilagsnavn (ikke en partstilknytning): {row.documentPartyName}</p>}{row.partyLinkProvenance && <p>Partstilknytning {row.partyLinkProvenance.linkId} · {row.partyLinkProvenance.role} · {row.partyLinkProvenance.evidenceKind} · {row.partyLinkProvenance.linkedAt} · plan-hash <code>{row.partyLinkProvenance.planHash}</code></p>}{row.dimensions?.map((d,index)=><p key={index}>Dimension {d.dimensionId}: {d.memberId} · {d.amountMinor} mindsteenheder {d.currency}</p>)}</details></td></tr>)}</tbody></table></div></section>;
}

function GroupResult({ result }: { result: Extract<CfoAnalyticsResponse, { scope: "group" }> }) {
  const group = result.group;
  if (result.status !== "ready" || group.status !== "ready" || !group.consolidatedFigures || !group.currency) return <section {...stylex.props(cockpitStyles.banner)} role="status"><strong {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>Koncernrapporten understøttes ikke for dette udsnit.</strong><ul {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>{[...(result.limitations ?? []), ...(group.blockers ?? [])].map((blocker) => <li key={blocker} {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>{blocker}</li>)}</ul></section>;
  return <section {...stylex.props(cockpitStyles.card)} aria-label="Koncernrapport"><h3 {...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.h3,
				)}>Koncernrapport — kontrolleret profil</h3><p {...stylex.props(cockpitStyles.muted)}>Pr. {result.asOf} · rå summer, elimineringer og konsolideret beløb vises hver for sig.</p><div {...stylex.props(cockpitStyles.tableScroll)}><table {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}><thead {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}><tr {...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}><th {...stylex.props(
									cockpitStyles.tableDataTh,
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>Linje</th><th {...stylex.props(
									cockpitStyles.tableDataTh,
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>Rå sum</th><th {...stylex.props(
									cockpitStyles.tableDataTh,
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>Eliminering</th><th {...stylex.props(
									cockpitStyles.tableDataTh,
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>Konsolideret</th></tr></thead><tbody {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>{group.consolidatedFigures.map((line) => <tr key={line.lineId} {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}><td {...stylex.props(
										cockpitStyles.tableDataTd,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}>{line.label}</td><td {...stylex.props(cockpitStyles.num)}>{amountText(line.rawCompanySum, group.currency)}</td><td {...stylex.props(cockpitStyles.num)}>{amountText(line.eliminationAdjustment, group.currency)}</td><td {...stylex.props(cockpitStyles.num)}>{amountText(line.consolidatedAmount, group.currency)}</td></tr>)}</tbody></table></div><ButtonLink  to="/koncernstruktur">Se struktur, afstemning og kilde-evidens</ButtonLink></section>;
}
