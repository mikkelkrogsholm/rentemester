import { ButtonLink, Button, Input, PageHeader, Select } from "../components/ui";
import { useState } from "react";
import { api } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import { ErrorState, Loading } from "../components/Feedback";

function currentIsoDate(): string {
  const now = new Date();
  return `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}-${String(now.getDate()).padStart(2, "0")}`;
}

export function GroupOverviewView() {
  const [asOf, setAsOf] = useState(currentIsoDate);
  const [from, setFrom] = useState(() => `${currentIsoDate().slice(0, 4)}-01-01`);
  const [selectedProfileId, setSelectedProfileId] = useState("");
  const [dispositionId, setDispositionId] = useState("");
  const [queriedDispositionId, setQueriedDispositionId] = useState("");
  const dispositionState = useAsync((signal) => queriedDispositionId ? api.intercompanyDispositionStatus(queriedDispositionId, asOf, { signal }) : Promise.resolve(null), [queriedDispositionId, asOf]);
  const dispositionStatus = dispositionState.data;
  const state = useAsync((signal) => api.groupOverview(asOf, { signal }), [asOf]);
  const reconciliationState = useAsync((signal) => api.groupReconciliation(asOf, { signal }), [asOf]);
  const eliminationState = useAsync((signal) => api.groupEliminations(asOf, { signal }), [asOf]);
  const profileState = useAsync((signal) => api.groupReportProfiles(asOf, { signal }), [asOf]);
  const effectiveProfileId = selectedProfileId || profileState.data?.profiles[0]?.id || "";
  const reportState = useAsync(
    (signal) => effectiveProfileId
      ? api.groupConsolidatedReport(effectiveProfileId, from, asOf, { signal })
      : Promise.resolve(null),
    [effectiveProfileId, from, asOf],
  );

  if (state.loading || reconciliationState.loading || eliminationState.loading || profileState.loading || reportState.loading) return <Loading label="Henter koncernstruktur…" />;
  if (state.error || reconciliationState.error || eliminationState.error || profileState.error || reportState.error) return <ErrorState message="Koncernstrukturen kan ikke vises sikkert." onRetry={() => { state.reload(); reconciliationState.reload(); eliminationState.reload(); profileState.reload(); reportState.reload(); }} />;
  const overview = state.data!;
  const reconciliation = reconciliationState.data!;
  const eliminations = eliminationState.data!;
  const profiles = profileState.data!.profiles;
  const report = reportState.data;
  if (overview.consolidatedFigures !== null || overview.rawCompanySums !== null || overview.consolidationStatus !== "not-available") {
    return <ErrorState message="Koncernstrukturen kan ikke vises sikkert." onRetry={() => { state.reload(); reconciliationState.reload(); eliminationState.reload(); profileState.reload(); reportState.reload(); }} />;
  }

  return <section className="group-overview">
    <PageHeader title="Koncernstruktur" actions={<ButtonLink to="/opgaver">Samlede opgaver</ButtonLink>}>
      <div>

        <p className="muted">Datoafgrænset struktur, afstemning og godkendte read-only koncernrapporter.</p>
      </div>
      <label className="group-as-of">Pr. dato<Input aria-label="Pr. dato" type="date" value={asOf} onChange={(event) => setAsOf(event.target.value)} required /></label>
    </PageHeader>
    <div className="banner warning" role="status"><strong>Strukturvisning</strong><br />Strukturen indeholder ingen økonomiske tal. Tal vises kun nedenfor, når en godkendt rapportprofil består alle kontroller.</div>
    {overview.manifestStatus !== "ready" && <div className="banner warning" role="status">Strukturen er blokeret eller ikke konfigureret. Konsoliderede rapporter er ikke tilgængelige.</div>}
    {overview.blockers.length > 0 && <Blockers blockers={overview.blockers} />}
    {overview.groups.length === 0 ? <p className="muted">Ingen aktiv koncernstruktur på den valgte dato.</p> : overview.groups.map((group, index) => <article className="group-card" key={group.id ?? `partial-${index}`}>
      <header><h3>{group.partial ? "Delvist synlig koncernstruktur" : group.name}</h3><span className={`group-readiness ${group.readiness}`}>{group.readiness === "ready" ? "Klar struktur" : "Blokeret"}</span></header>
      <h4>Aktive, synlige medlemskaber</h4>
      <ul>{group.visibleMemberships.map((member) => <li key={member.id}><code>{member.companySlug}</code> · aktiv fra {member.validFrom}{member.validToExclusive ? ` til ${member.validToExclusive}` : ""} · <strong>{member.archived ? "Arkiveret" : "Aktiv"}</strong></li>)}</ul>
      <h4>Synlige ejerskabsrelationer</h4>
      {group.visibleOwnership.length === 0 ? <p className="muted">Ingen synlige ejerskabsrelationer på den valgte dato.</p> : <ul>{group.visibleOwnership.map((ownership) => <li key={ownership.id}><code>{ownership.parentCompanySlug}</code> → <code>{ownership.childCompanySlug}</code> · evidence: {ownership.evidenceRefs.join(", ")}</li>)}</ul>}
      {group.blockers.length > 0 && <Blockers blockers={group.blockers} />}
    </article>)}
    <section className="group-reconciliation" aria-label="Mellemregningsafstemning">
      <h3>Mellemregningsafstemning</h3>
      <p className="muted">Kun godkendte, eksplicitte kontomappings. Beløb sammenlignes eksakt og kun i samme funktionsvaluta.</p>
      {reconciliation.rows.length === 0 ? <p className="muted">Ingen aktive, godkendte mappings på den valgte dato.</p> : <div className="table-wrap"><div className="table-scroll"><table><thead><tr><th>Mapping</th><th>Venstre</th><th>Højre</th><th>Difference</th><th>Status</th></tr></thead><tbody>{reconciliation.rows.map((row, index) => <tr key={row.mappingId ?? `blocked-${index}`}>
        <td>{row.mappingId ?? "Skjult"}</td>
        <td>{row.left ? `${row.left.companySlug}: ${row.left.balance.toFixed(2)} ${row.left.currency}` : "Ikke synlig"}</td>
        <td>{row.right ? `${row.right.companySlug}: ${row.right.balance.toFixed(2)} ${row.right.currency}` : "Ikke synlig"}</td>
        <td>{row.difference == null ? "—" : row.difference.toFixed(2)}</td>
        <td><strong>{row.status === "matched" ? "Afstemt" : row.status === "mismatch" ? "Difference" : "Ikke sammenlignelig"}</strong>{row.blockers.length > 0 && <div className="muted">{row.blockers.join(" · ")}</div>}</td>
      </tr>)}</tbody></table></div></div>}
    </section>
    <section className="group-eliminations" aria-label="Elimineringer">
      <h3>Elimineringer</h3>
      <p className="muted">Kun anvendte, append-only balanceelimineringer afledt af eksakt afstemte mellemregninger. Selskabernes hovedbøger ændres ikke.</p>
      {eliminations.rows.length === 0 ? <p className="muted">Ingen anvendte eliminationer på den valgte dato.</p> : <ul>{eliminations.rows.map((row, index) => <li key={row.eliminationId ?? `blocked-${index}`}>{row.status === "blocked" || !row.payload ? <><strong>Ikke synlig</strong> · {row.blockers.join(" · ")}</> : <><strong>{row.eliminationId}</strong> · {Number(BigInt(row.payload.amountOre)) / 100} {row.payload.currency} · {row.payload.left.companySlug} ↔ {row.payload.right.companySlug}</>}</li>)}</ul>}
    </section>
    <section className="group-dispositions" aria-label="Intercompany dispositioner">
      <h3>Intercompany dispositioner</h3>
      <p className="muted">To juridiske ledgers forbliver adskilte. Denne lifecycle viser og binder kun dokumenteret evidence; den bogfører og eliminerer aldrig selv.</p>
      <form onSubmit={(event) => { event.preventDefault(); if (!dispositionId.trim()) return; setQueriedDispositionId(dispositionId.trim()); dispositionState.reload(); }}>
        <label>Disposition-ID<Input aria-label="Disposition-ID" value={dispositionId} onChange={(event) => { setDispositionId(event.target.value); setQueriedDispositionId(""); }} required /></label><Button busy={dispositionState.loading} type="submit">Vis status</Button>
      </form>
      {dispositionState.error && <p className="error" role="alert">Dispositionen kan ikke vises med din aktuelle adgang.</p>}
      {dispositionStatus && <div className="banner warning" role="status"><strong>{dispositionStatus.status}</strong>{Array.isArray(dispositionStatus.exceptions) && dispositionStatus.exceptions.length > 0 && <ul>{dispositionStatus.exceptions.map((exception,index)=><li key={index}>{exception.kind}</li>)}</ul>}</div>}
      <p className="muted">Plan, forslag, godkendelse, link, settlement, supersession og reopen bruger den samme confirmed API/MCP-lifecycle med adgang til begge selskaber.</p>
    </section>
    <section className="group-report" aria-label="Konsolideret rapport">
      <h3>Konsolideret rapport</h3>
      <p className="muted">Read-only visning. Selskabernes hovedbøger ændres ikke, og kun godkendte profiler med fuld selskabssynlighed kan vælges.</p>
      {profiles.length === 0 ? <p className="muted">Ingen aktiv, godkendt rapportprofil på den valgte dato.</p> : <>
        <div className="group-report-controls">
          <label>Rapportprofil<Select aria-label="Rapportprofil" value={effectiveProfileId} onChange={(event) => setSelectedProfileId(event.target.value)}>{profiles.map((profile) => <option key={profile.id} value={profile.id}>{profile.id} · {profile.currency}</option>)}</Select></label>
          <label>Fra dato<Input aria-label="Fra dato" type="date" value={from} max={asOf} onChange={(event) => setFrom(event.target.value)} required /></label>
        </div>
        {report?.status === "blocked" ? <Blockers blockers={report.blockers} /> : report?.consolidatedFigures ? <>
          <div className="banner success" role="status"><strong>Kontrolleret koncernrapport</strong><br />Periode {report.period.from} til {report.period.to} · {report.currency}</div>
          <div className="table-wrap"><div className="table-scroll"><table><thead><tr><th>Rapportlinje</th><th>Rå selskabssum</th><th>Eliminering</th><th>Konsolideret</th></tr></thead><tbody>{report.consolidatedFigures.map((line) => <tr key={line.lineId}><td>{line.label}</td><td>{formatGroupAmount(line.rawCompanySum, report.currency!)}</td><td>{formatGroupAmount(line.eliminationAdjustment, report.currency!)}</td><td><strong>{formatGroupAmount(line.consolidatedAmount, report.currency!)}</strong></td></tr>)}</tbody></table></div></div>
          <details><summary>Kildeevidens</summary><ul>{report.sourceSnapshots.map((snapshot) => <li key={snapshot.companySlug}><code>{snapshot.companySlug}</code> · {snapshot.entryCount} poster · ledger-head <code>{snapshot.ledgerHeadHash?.slice(0, 16) ?? "tom"}</code></li>)}</ul></details>
        </> : null}
      </>}
    </section>
  </section>;
}

function formatGroupAmount(amount: number, currency: string): string {
  return new Intl.NumberFormat("da-DK", { style: "currency", currency }).format(amount);
}

function Blockers({ blockers }: { blockers: readonly string[] }) {
  return <section className="group-blockers" aria-label="Blokeringer"><h3>Blokeringer</h3><ul>{blockers.map((blocker) => <li key={blocker}>{blocker}</li>)}</ul></section>;
}
