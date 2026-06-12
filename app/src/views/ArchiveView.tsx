// Om arkivet — an explainer for the read-only #197 archive (Runde 3, it. 11).
//
// Before Runde 3 the core views could only render the live year, so this tab
// carried a raw archived SaldoBalance to give the old years anywhere to live.
// Iteration 10 made Resultatopgørelse / Balance / Saldobalance / Posteringer /
// Overblik archive-aware via the fiscal-year selector, so the raw table here
// became redundant. This tab is now a concise "Om arkivet" page: which years
// are archived, where the data came from (the Dinero import #197) and that it
// is read-only — with links pointing the user into the archive-aware views.

import { Link, useParams } from "react-router-dom";
import { api } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import type { FiscalYearEntry } from "../lib/types";
import { ErrorState, Loading } from "../components/Feedback";
import { CompanyNav, useCompanyYear } from "../components/CompanyNav";

export function ArchiveView() {
  const { slug = "" } = useParams();
  const { year, setYear } = useCompanyYear();

  const yearsState = useAsync<FiscalYearEntry[]>(
    () => api.fiscalYears(slug),
    [slug],
  );

  if (yearsState.loading && !yearsState.data)
    return <Loading label="Henter arkiv…" />;
  if (yearsState.error)
    return (
      <ErrorState message={yearsState.error} onRetry={yearsState.reload} />
    );

  const years = yearsState.data!;
  // Archived years, newest-first — the years the explainer is about.
  const archiveYears = years
    .filter((y) => y.source === "archive")
    .sort((a, b) => b.label.localeCompare(a.label));
  const liveYears = years
    .filter((y) => y.source === "live")
    .sort((a, b) => b.label.localeCompare(a.label));
  const selectedLabel = year ?? years[0]?.label ?? "";

  return (
    <section className="statement">
      <div className="page-head">
        <div>
          <h2>Om arkivet</h2>
          <p className="muted">Tidligere regnskabsår · skrivebeskyttet</p>
        </div>
        <div className="row-actions">
          <Link className="btn secondary" to={`/companies/${slug}/manage`}>
            Administrér
          </Link>
        </div>
      </div>

      <CompanyNav
        slug={slug}
        years={years}
        selectedYear={selectedLabel}
        onYearChange={setYear}
      />

      {archiveYears.length === 0 ? (
        <div className="card archived-notice">
          <h3>Ingen arkiverede regnskabsår</h3>
          <p className="muted">
            Denne virksomhed har ingen tidligere år i det skrivebeskyttede
            arkiv — alle dens regnskabsår ligger i den aktive ledger.
          </p>
        </div>
      ) : (
        <>
          <div className="card archive-banner">
            <span className="flag warning">Arkiveret</span>
            <p>
              <strong>Det arkiverede regnskab er skrivebeskyttet.</strong> De
              arkiverede år blev importeret fra virksomhedens tidligere
              Dinero-regnskab — fuld saldobalance og alle posteringer pr. år.
              Tallene ligger uden for den aktive bogføring og kan ikke
              redigeres.
            </p>
          </div>

          <div className="section">
            <h3>Arkiverede regnskabsår</h3>
            <p className="muted">
              Hvert arkiveret år kan ses i de almindelige visninger — vælg året
              i regnskabsårs-vælgeren ovenfor, så viser Resultatopgørelse,
              Balance, Saldobalance, Posteringer og Overblik arkiv-tallene med
              et skrivebeskyttet-banner.
            </p>
            <div className="card statement-card table-scroll">
              <table className="data statement-table">
                <thead>
                  <tr>
                    <th>Regnskabsår</th>
                    <th>Kilde</th>
                    <th>Status</th>
                    <th>Visninger</th>
                  </tr>
                </thead>
                <tbody>
                  {archiveYears.map((y) => (
                    <tr key={y.label}>
                      <td>
                        {y.label}
                        <span className="flag warning archive-tag">arkiv</span>
                      </td>
                      <td>Importeret fra Dinero</td>
                      <td>Skrivebeskyttet</td>
                      <td>
                        <Link
                          to={`/companies/${slug}/saldobalance?year=${y.label}`}
                        >
                          Saldobalance
                        </Link>
                        {" · "}
                        <Link
                          to={`/companies/${slug}/resultatopgorelse?year=${y.label}`}
                        >
                          Resultatopgørelse
                        </Link>
                        {" · "}
                        <Link to={`/companies/${slug}?year=${y.label}`}>
                          Overblik
                        </Link>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </div>

          <div className="section">
            <h3>Aktive regnskabsår</h3>
            {liveYears.length === 0 ? (
              <p className="muted">
                Der er endnu ingen bogførte år i den aktive ledger.
              </p>
            ) : (
              <p className="muted">
                {liveYears.map((y) => y.label).join(", ")} bogføres i den
                aktive ledger og kan redigeres. Se også{" "}
                <Link to={`/companies/${slug}/fleraar`}>Flerår</Link> for et
                samlet overblik på tværs af alle år.
              </p>
            )}
          </div>
        </>
      )}
    </section>
  );
}
