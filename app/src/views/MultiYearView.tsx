// Flerårsoversigt — a multi-year comparison (cockpit-redesign iteration 4;
// enriched in Runde 3, iteration 11).
//
// Renders `/api/companies/:slug/multi-year`: for every fiscal year a company
// has — live ledger years and the read-only #197 archive years alike — the
// P&L (omsætning / udgifter / resultat), the balance-sheet development
// (balancesum / egenkapital) and the key ratios (bruttomargin,
// egenkapitalandel), each as a comparison table and a Chart.js trend chart.
// This is the "alle år på ét overblik" view. Money fields are kroner
// (`formatKroner`); the ratios are 0–1 fractions (`formatPercent`).

import { Link, useParams } from "react-router-dom";
import { api } from "../lib/api";
import { formatKroner, formatPercent } from "../lib/format";
import { useAsync } from "../lib/useAsync";
import type { CompanyMultiYear } from "../lib/types";
import { ErrorState, Loading } from "../components/Feedback";
import { CompanyNav, useCompanyYear } from "../components/CompanyNav";
import { MultiYearChart } from "../components/MultiYearChart";
import { MultiYearBalanceChart } from "../components/MultiYearBalanceChart";

export function MultiYearView() {
  const { slug = "" } = useParams();
  const { setYear } = useCompanyYear();
  const state = useAsync<CompanyMultiYear>(
    () => api.multiYear(slug),
    [slug],
  );

  if (state.loading && !state.data)
    return <Loading label="Henter flerårsoversigt…" />;
  if (state.error)
    return <ErrorState message={state.error} onRetry={state.reload} />;

  const m = state.data!;
  const currency = m.company.currency || "DKK";
  // The live/current fiscal year is a partial year next to the full archived
  // ones — the newest "live" row. Mark it "(år til dato)" so the comparison
  // is not read as like-for-like.
  const currentYear =
    [...m.years]
      .filter((y) => y.source === "live")
      .sort((a, b) => b.year.localeCompare(a.year))[0]?.year ?? null;
  // The fiscal-year selector is shown for consistency with the other views;
  // newest-first like everywhere else. The Flerårsoversigt itself shows every
  // year, so the selected year only routes the other views.
  const selectorYears = [...m.years]
    .map((y) => ({
      label: y.year,
      start: null,
      end: null,
      source: y.source,
    }))
    .sort((a, b) => b.label.localeCompare(a.label));
  const selectedYear = selectorYears[0]?.label ?? "";

  return (
    <section className="statement">
      <div className="page-head">
        <div>
          <h2>{m.company.name}</h2>
          <p className="muted">
            {m.company.cvr ? `CVR ${m.company.cvr} · ` : ""}
            {m.company.country} · {currency} · Flerårsoversigt
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
        years={selectorYears}
        selectedYear={selectedYear}
        onYearChange={setYear}
      />

      {m.years.length === 0 ? (
        <div className="card archived-notice">
          <h3>Ingen regnskabsår</h3>
          <p className="muted">
            Denne virksomhed har endnu ingen bogførte eller arkiverede
            regnskabsår at sammenligne.
          </p>
        </div>
      ) : (
        <>
          <div className="section">
            <h3>Resultat — omsætning, udgifter og resultat</h3>
            <div className="card chart-card">
              <MultiYearChart years={m.years} currentYear={currentYear} />
            </div>
            <div className="card statement-card table-scroll">
              <table className="data statement-table">
                <thead>
                  <tr>
                    <th>Regnskabsår</th>
                    <th className="num">Omsætning</th>
                    <th className="num">Udgifter</th>
                    <th className="num">Resultat</th>
                  </tr>
                </thead>
                <tbody>
                  {m.years.map((y) => (
                    <tr key={y.year}>
                      <td>
                        <YearLabel year={y.year} source={y.source} currentYear={currentYear} />
                      </td>
                      <td className="num">
                        {formatKroner(y.omsaetning, currency)}
                      </td>
                      <td className="num">
                        {formatKroner(y.udgifter, currency)}
                      </td>
                      <td
                        className={`num ${
                          y.resultat >= 0 ? "amount-positive" : "amount-negative"
                        }`}
                      >
                        {formatKroner(y.resultat, currency)}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </div>

          <div className="section">
            <h3>Balance — balancesum og egenkapital</h3>
            <div className="card chart-card">
              <MultiYearBalanceChart years={m.years} currentYear={currentYear} />
            </div>
            <div className="card statement-card table-scroll">
              <table className="data statement-table">
                <thead>
                  <tr>
                    <th>Regnskabsår</th>
                    <th className="num">Balancesum</th>
                    <th className="num">Egenkapital</th>
                  </tr>
                </thead>
                <tbody>
                  {m.years.map((y) => (
                    <tr key={y.year}>
                      <td>
                        <YearLabel year={y.year} source={y.source} currentYear={currentYear} />
                      </td>
                      <td className="num">
                        {formatKroner(y.balancesum, currency)}
                      </td>
                      <td
                        className={`num ${
                          y.egenkapital >= 0
                            ? "amount-positive"
                            : "amount-negative"
                        }`}
                      >
                        {formatKroner(y.egenkapital, currency)}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </div>

          <div className="section">
            <h3>Nøgletal pr. regnskabsår</h3>
            <p className="muted">
              Overskudsgrad er resultat ÷ omsætning; egenkapitalandel er
              egenkapital ÷ balancesum. Et bindestreg betyder, at nøgletallet
              ikke kan beregnes (nævneren er nul).
            </p>
            <div className="card statement-card table-scroll">
              <table className="data statement-table">
                <thead>
                  <tr>
                    <th>Regnskabsår</th>
                    <th className="num">Overskudsgrad</th>
                    <th className="num">Egenkapitalandel</th>
                  </tr>
                </thead>
                <tbody>
                  {m.years.map((y) => (
                    <tr key={y.year}>
                      <td>
                        <YearLabel year={y.year} source={y.source} currentYear={currentYear} />
                      </td>
                      <td className="num">{formatPercent(y.bruttomargin)}</td>
                      <td className="num">
                        {formatPercent(y.egenkapitalandel)}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </div>
        </>
      )}
    </section>
  );
}

/**
 * A fiscal-year cell label — the year, an "arkiv" flag for read-only #197
 * years, and an "(år til dato)" marker for the partial live year so the
 * comparison is not read as like-for-like.
 */
function YearLabel({
  year,
  source,
  currentYear,
}: {
  year: string;
  source: "live" | "archive";
  currentYear: string | null;
}) {
  return (
    <>
      {year}
      {source === "archive" ? (
        <span className="flag warning archive-tag">arkiv</span>
      ) : null}
      {year === currentYear ? (
        <span className="multi-year-current muted">(år til dato)</span>
      ) : null}
    </>
  );
}
