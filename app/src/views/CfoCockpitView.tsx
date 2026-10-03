import { ButtonLink, Input, PageHeader, Select } from "../components/ui";
import { Link } from "react-router-dom";
import { useMemo, useState } from "react";
import { ErrorState, Loading } from "../components/Feedback";
import { api } from "../lib/api";
import { formatKroner, todayIso } from "../lib/format";
import type { CfoAnalyticsResponse, CfoAnalyticsScope } from "../lib/types";
import { useAsync } from "../lib/useAsync";

function startOfYear(day: string): string { return `${day.slice(0, 4)}-01-01`; }

/** A read-only operational projection. Financial calculations stay in #581/core. */
export function CfoCockpitView() {
  const today = useMemo(todayIso, []);
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
  const profileId = profilesState.data?.profiles[0]?.id;
  const analyticsState = useAsync<CfoAnalyticsResponse | null>(
    (signal) => {
      if (scope === "company" && !effectiveCompanySlug) return Promise.resolve(null);
      if (scope === "group" && !profileId) return Promise.resolve(null);
      return api.cfoAnalytics({ scope, from, to: asOf, ...(scope === "company" ? { companySlug: effectiveCompanySlug } : {}), ...(scope === "group" ? { groupProfileId: profileId } : {}) }, { signal });
    },
    [scope, effectiveCompanySlug, from, asOf, profileId],
  );

  const loading = companiesState.loading || profilesState.loading || analyticsState.loading;
  const error = companiesState.error || profilesState.error || analyticsState.error;
  return <section className="cfo-cockpit">
    <PageHeader title="CFO-overblik">
      <div><p className="muted">Read-only, kildehenvisende overblik. Bogføring sker fortsat i den enkelte virksomheds normale flow.</p></div>
    </PageHeader>
    <div className="row-actions" role="group" aria-label="CFO-afgrænsning">
      <label>Visning<Select aria-label="Visning" value={scope} onChange={(event) => setScope(event.target.value as CfoAnalyticsScope)}><option value="company">Virksomhed</option><option value="portfolio">Portefølje</option><option value="group">Koncern</option></Select></label>
      {scope === "company" && <label>Virksomhed<Select aria-label="Virksomhed" value={effectiveCompanySlug} onChange={(event) => setCompanySlug(event.target.value)}>{companies.map((company) => <option key={company.slug} value={company.slug}>{company.name}</option>)}</Select></label>}
      <label>Fra<Input aria-label="Fra dato" type="date" value={from} max={asOf} onChange={(event) => setFrom(event.target.value)} /></label>
      <label>Pr. dato<Input aria-label="Pr. dato" type="date" value={asOf} min={from} onChange={(event) => setAsOf(event.target.value)} /></label>
    </div>
    {loading && <Loading label="Henter kildehenvisende CFO-overblik…" />}
    {!loading && error && <ErrorState message="CFO-overblikket kan ikke vises sikkert." onRetry={() => { companiesState.reload(); profilesState.reload(); analyticsState.reload(); }} />}
    {!loading && !error && scope === "company" && !effectiveCompanySlug && <p className="muted">Ingen tilgængelige virksomheder.</p>}
    {!loading && !error && scope === "group" && !profileId && <section className="banner warning" role="status">Ingen godkendt koncernrapportprofil er tilgængelig pr. den valgte dato.</section>}
    {!loading && !error && analyticsState.data && <CfoResult result={analyticsState.data} companyNames={new Map(companies.map((company) => [company.slug, company.name]))} />}
  </section>;
}

function CfoResult({ result, companyNames }: { result: CfoAnalyticsResponse; companyNames: Map<string, string> }) {
  if (result.scope === "group") return <GroupResult result={result} />;
  const portfolio = result.scope === "portfolio";
  const title = portfolio ? "Portefølje — ikke konsolideret" : `Virksomhed: ${companyNames.get(result.companies[0] ?? "") ?? result.companies[0]}`;
  return <>
    <section className="card" aria-label="Analyseafgrænsning"><h3>{title}</h3><p className="muted">Pr. {result.asOf} · periode {result.from} – {result.to} · schema <code>{result.schemaVersion}</code></p>{portfolio && <div className="banner warning" role="status">Porteføljen er en sideordnet, ikke-konsolideret visning. Der vises ingen implicitte elimineringer eller valutaomregning.</div>}{result.status === "incomplete" && <div className="banner warning" role="status">Ufuldstændigt udsnit: skjulte virksomheder og deres tal er ikke medtaget.</div>}<Freshness entries={result.freshness} /></section>
    <Evidence result={result} />
    {result.rows.length === 0 ? <p className="muted">Ingen kildeposteringer i den valgte periode.</p> : <>
      {!result.partial && result.reconciliation.amountByCurrency && <section className="status-grid" aria-label="Kildebaserede beløb">{Object.entries(result.reconciliation.amountByCurrency).map(([currency, amount]) => <article className="card" key={currency}><h3>Analyseret bevægelse</h3><p className="amount">{formatKroner(amount, currency)}</p><p className="muted">{result.reconciliation.rowCount} kildeposter · {currency}</p></article>)}</section>}
      <SourceRows rows={result.rows} />
    </>}
  </>;
}

function Freshness({ entries }: { entries: Array<{ source: "ledger" | "archive"; companySlug: string; latestTransactionDate: string }> }) {
  if (!entries.length) return <p className="muted">Ingen kildekilder med posteringer i perioden.</p>;
  return <><p className="muted">{entries.map((entry) => `Seneste ${entry.source}: ${entry.latestTransactionDate} (${entry.companySlug})`).join(" · ")}</p><p className="muted">Stale-status kan ikke udledes af posteringernes dato alene; visningen viser derfor kun den seneste verificerede kildepost.</p></>;
}

function Evidence({ result }: { result: Extract<CfoAnalyticsResponse, { scope: "company" | "portfolio" }> }) {
  return <section className="status-grid" aria-label="Kontrol og genveje">{result.evidenceCompleteness.map((entry) => <article className="card" key={entry.companySlug}><h3>Kontrol: {entry.companySlug}</h3>{entry.status === "unavailable" ? <p className="muted">Ikke tilgængelig: {entry.reason}</p> : <><p>{entry.postedWithoutDocument} bogførte poster uden bilag · {entry.openExceptions} åbne undtagelser</p><div className="row-actions"><ButtonLink className="btn secondary" to={`/companies/${entry.companySlug}/bank`}>Bank</ButtonLink><ButtonLink className="btn secondary" to={`/companies/${entry.companySlug}/bilag`}>Bilag</ButtonLink><ButtonLink className="btn secondary" to={`/companies/${entry.companySlug}/undtagelser`}>Undtagelser</ButtonLink><ButtonLink className="btn secondary" to={`/companies/${entry.companySlug}/moms`}>Moms</ButtonLink><ButtonLink className="btn secondary" to={`/companies/${entry.companySlug}/leverandoerfaktura`}>Leverandørfaktura</ButtonLink><ButtonLink className="btn secondary" to={`/companies/${entry.companySlug}/budget`}>Budget</ButtonLink></div></>}</article>)}</section>;
}

function SourceRows({ rows }: { rows: Extract<CfoAnalyticsResponse, { scope: "company" | "portfolio" }> ["rows"] }) {
  return <section className="table-wrap" aria-label="Kildeposter"><div className="table-scroll"><table><thead><tr><th>Dato</th><th>Virksomhed</th><th>Postering</th><th>Modpart</th><th>Beløb</th><th>Kilde</th></tr></thead><tbody>{rows.map((row) => <tr key={row.sourceId}><td>{row.transactionDate}</td><td><Link to={`/companies/${row.companySlug}`}>{row.companySlug}</Link></td><td><Link aria-label={`Postering ${row.journalEntryNo ?? row.sourceId}`} to={`/companies/${row.companySlug}/posteringer?account=${encodeURIComponent(row.accountNo)}`}>{row.journalEntryNo ?? row.accountNo}</Link></td><td>{row.partyName ?? "—"}</td><td className="num">{formatKroner(row.amount, row.currency)}</td><td><code title={row.sourceHash}>{row.sourceType}</code></td></tr>)}</tbody></table></div></section>;
}

function GroupResult({ result }: { result: Extract<CfoAnalyticsResponse, { scope: "group" }> }) {
  const group = result.group;
  if (result.status !== "ready" || group.status !== "ready" || !group.consolidatedFigures) return <section className="banner warning" role="status"><strong>Koncernrapporten understøttes ikke for dette udsnit.</strong><ul>{[...(result.limitations ?? []), ...(group.blockers ?? [])].map((blocker) => <li key={blocker}>{blocker}</li>)}</ul></section>;
  return <section className="card" aria-label="Koncernrapport"><h3>Koncernrapport — kontrolleret profil</h3><p className="muted">Pr. {result.asOf} · rå summer, elimineringer og konsolideret beløb vises hver for sig.</p><div className="table-scroll"><table><thead><tr><th>Linje</th><th>Rå sum</th><th>Eliminering</th><th>Konsolideret</th></tr></thead><tbody>{group.consolidatedFigures.map((line: any) => <tr key={line.lineId}><td>{line.label}</td><td className="num">{formatKroner(line.rawCompanySum, group.currency)}</td><td className="num">{formatKroner(line.eliminationAdjustment, group.currency)}</td><td className="num">{formatKroner(line.consolidatedAmount, group.currency)}</td></tr>)}</tbody></table></div><ButtonLink className="btn secondary" to="/koncernstruktur">Se struktur, afstemning og kilde-evidens</ButtonLink></section>;
}
