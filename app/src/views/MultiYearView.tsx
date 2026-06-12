// Flerårsoversigt — a multi-year comparison (cockpit-redesign iteration 4;
// enriched in Runde 3, iteration 11; #452: år-over-år Δ-kolonner).
//
// Renders `/api/companies/:slug/multi-year`: for every fiscal year a company
// has — live ledger years and the read-only #197 archive years alike — the
// P&L (omsætning / udgifter / resultat), the balance-sheet development
// (balancesum / egenkapital) and the key ratios (bruttomargin,
// egenkapitalandel), each as a comparison table and a Chart.js trend chart.
// This is the "alle år på ét overblik" view. Money fields are kroner
// (`formatKroner`); the ratios are 0–1 fractions (`formatPercent`).
//
// #452: each numeric metric carries an Δ-column pair year-over-year (Δ kr +
// Δ % for kroner, Δ pp for the ratios). The Δ is computed against the
// chronologically prior year; the oldest year shows "—"; the partial
// live year shows "(ej sammenligneligt — år til dato)"; a 0-denominator
// renders "—" rather than NaN/∞.

import { Link, useParams } from "react-router-dom";
import { api } from "../lib/api";
import { formatKroner, formatPercent } from "../lib/format";
import { useAsync } from "../lib/useAsync";
import type { CompanyMultiYear, MultiYearRow } from "../lib/types";
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

  // #452: build a chronological priorByYear lookup so a row can compare to
  // the year immediately before it. The API returns years oldest→newest.
  const chronological = [...m.years].sort((a, b) =>
    a.year.localeCompare(b.year),
  );
  const priorByYear = new Map<string, MultiYearRow>();
  for (let i = 1; i < chronological.length; i += 1) {
    priorByYear.set(chronological[i].year, chronological[i - 1]);
  }
  // Show Δ columns only when there is more than one year — a column that
  // is always "—" is noise.
  const showDelta = m.years.length > 1;

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
                    {showDelta ? <th className="num">Omsætning Δ (kr)</th> : null}
                    {showDelta ? <th className="num">Omsætning Δ (%)</th> : null}
                    <th className="num">Udgifter</th>
                    {showDelta ? <th className="num">Udgifter Δ (kr)</th> : null}
                    {showDelta ? <th className="num">Udgifter Δ (%)</th> : null}
                    <th className="num">Resultat</th>
                    {showDelta ? <th className="num">Resultat Δ (kr)</th> : null}
                    {showDelta ? <th className="num">Resultat Δ (%)</th> : null}
                  </tr>
                </thead>
                <tbody>
                  {m.years.map((y) => {
                    const prior = priorByYear.get(y.year) ?? null;
                    const isPartial = y.year === currentYear;
                    return (
                      <tr key={y.year}>
                        <td>
                          <YearLabel
                            year={y.year}
                            source={y.source}
                            currentYear={currentYear}
                          />
                        </td>
                        <td className="num">
                          {formatKroner(y.omsaetning, currency)}
                        </td>
                        {showDelta ? (
                          <DeltaKr
                            current={y.omsaetning}
                            prior={prior?.omsaetning ?? null}
                            currency={currency}
                            partial={isPartial}
                          />
                        ) : null}
                        {showDelta ? (
                          <DeltaPct
                            current={y.omsaetning}
                            prior={prior?.omsaetning ?? null}
                            partial={isPartial}
                          />
                        ) : null}
                        <td className="num">
                          {formatKroner(y.udgifter, currency)}
                        </td>
                        {showDelta ? (
                          <DeltaKr
                            current={y.udgifter}
                            prior={prior?.udgifter ?? null}
                            currency={currency}
                            partial={isPartial}
                            // Higher expenses is "worse" — invert the
                            // positive/negative tone so a rise reads red.
                            invertTone
                          />
                        ) : null}
                        {showDelta ? (
                          <DeltaPct
                            current={y.udgifter}
                            prior={prior?.udgifter ?? null}
                            partial={isPartial}
                            invertTone
                          />
                        ) : null}
                        <td
                          className={`num ${
                            y.resultat >= 0
                              ? "amount-positive"
                              : "amount-negative"
                          }`}
                        >
                          {formatKroner(y.resultat, currency)}
                        </td>
                        {showDelta ? (
                          <DeltaKr
                            current={y.resultat}
                            prior={prior?.resultat ?? null}
                            currency={currency}
                            partial={isPartial}
                          />
                        ) : null}
                        {showDelta ? (
                          <DeltaPct
                            current={y.resultat}
                            prior={prior?.resultat ?? null}
                            partial={isPartial}
                          />
                        ) : null}
                      </tr>
                    );
                  })}
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
                    {showDelta ? <th className="num">Balancesum Δ (kr)</th> : null}
                    {showDelta ? <th className="num">Balancesum Δ (%)</th> : null}
                    <th className="num">Egenkapital</th>
                    {showDelta ? <th className="num">Egenkapital Δ (kr)</th> : null}
                    {showDelta ? <th className="num">Egenkapital Δ (%)</th> : null}
                  </tr>
                </thead>
                <tbody>
                  {m.years.map((y) => {
                    const prior = priorByYear.get(y.year) ?? null;
                    const isPartial = y.year === currentYear;
                    return (
                      <tr key={y.year}>
                        <td>
                          <YearLabel
                            year={y.year}
                            source={y.source}
                            currentYear={currentYear}
                          />
                        </td>
                        <td className="num">
                          {formatKroner(y.balancesum, currency)}
                        </td>
                        {showDelta ? (
                          <DeltaKr
                            current={y.balancesum}
                            prior={prior?.balancesum ?? null}
                            currency={currency}
                            partial={isPartial}
                          />
                        ) : null}
                        {showDelta ? (
                          <DeltaPct
                            current={y.balancesum}
                            prior={prior?.balancesum ?? null}
                            partial={isPartial}
                          />
                        ) : null}
                        <td
                          className={`num ${
                            y.egenkapital >= 0
                              ? "amount-positive"
                              : "amount-negative"
                          }`}
                        >
                          {formatKroner(y.egenkapital, currency)}
                        </td>
                        {showDelta ? (
                          <DeltaKr
                            current={y.egenkapital}
                            prior={prior?.egenkapital ?? null}
                            currency={currency}
                            partial={isPartial}
                          />
                        ) : null}
                        {showDelta ? (
                          <DeltaPct
                            current={y.egenkapital}
                            prior={prior?.egenkapital ?? null}
                            partial={isPartial}
                          />
                        ) : null}
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          </div>

          <div className="section">
            <h3>Nøgletal pr. regnskabsår</h3>
            <p className="muted">
              Overskudsgrad er resultat ÷ omsætning; egenkapitalandel er
              egenkapital ÷ balancesum. Et bindestreg betyder, at nøgletallet
              ikke kan beregnes (nævneren er nul). Ændringen vises i
              procentpoint (pp), så et spring fra 17,6 % til 22,4 % læses
              som «+4,8 pp», ikke «+27 %».
            </p>
            <div className="card statement-card table-scroll">
              <table className="data statement-table">
                <thead>
                  <tr>
                    <th>Regnskabsår</th>
                    <th className="num">Overskudsgrad</th>
                    {showDelta ? (
                      <th className="num">Overskudsgrad Δ (pp)</th>
                    ) : null}
                    <th className="num">Egenkapitalandel</th>
                    {showDelta ? (
                      <th className="num">Egenkapitalandel Δ (pp)</th>
                    ) : null}
                  </tr>
                </thead>
                <tbody>
                  {m.years.map((y) => {
                    const prior = priorByYear.get(y.year) ?? null;
                    const isPartial = y.year === currentYear;
                    return (
                      <tr key={y.year}>
                        <td>
                          <YearLabel
                            year={y.year}
                            source={y.source}
                            currentYear={currentYear}
                          />
                        </td>
                        <td className="num">{formatPercent(y.bruttomargin)}</td>
                        {showDelta ? (
                          <DeltaPp
                            current={y.bruttomargin}
                            prior={prior?.bruttomargin ?? null}
                            partial={isPartial}
                          />
                        ) : null}
                        <td className="num">
                          {formatPercent(y.egenkapitalandel)}
                        </td>
                        {showDelta ? (
                          <DeltaPp
                            current={y.egenkapitalandel}
                            prior={prior?.egenkapitalandel ?? null}
                            partial={isPartial}
                          />
                        ) : null}
                      </tr>
                    );
                  })}
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

// --- #452: Δ-cell helpers ---------------------------------------------------

const PARTIAL_NOTE = "(ej sammenligneligt — år til dato)";

/**
 * Renders the formatted kroner difference between `current` and `prior`. A
 * missing prior shows "—" (the row is the oldest year); a partial live year
 * shows the explicit "(ej sammenligneligt)" note so the comparison is not
 * read as like-for-like.
 *
 * `invertTone` flips the green/red mapping for cost-like metrics (udgifter):
 * a *rise* in expenses is the "bad" direction.
 */
function DeltaKr({
  current,
  prior,
  currency,
  partial,
  invertTone = false,
}: {
  current: number;
  prior: number | null;
  currency: string;
  partial: boolean;
  invertTone?: boolean;
}) {
  if (partial) {
    return <td className="num muted">{PARTIAL_NOTE}</td>;
  }
  if (prior === null || !Number.isFinite(prior)) {
    return <td className="num muted">—</td>;
  }
  const diff = current - prior;
  const positiveIsGood = !invertTone;
  const isGood = positiveIsGood ? diff > 0 : diff < 0;
  const isBad = positiveIsGood ? diff < 0 : diff > 0;
  const tone = isGood
    ? "amount-positive"
    : isBad
      ? "amount-negative"
      : "";
  const sign = diff > 0 ? "+" : diff < 0 ? "-" : "";
  // formatKroner already adds a minus prefix for negatives — strip it and
  // prepend our own sign so positives also carry a "+".
  const body = formatKroner(Math.abs(diff), currency);
  return (
    <td className={`num ${tone}`.trim()}>
      {sign}
      {body}
    </td>
  );
}

/**
 * Renders the relative change as a Danish-formatted percent against `prior`.
 * A zero or null denominator yields "—" (no ∞/NaN). A partial live year
 * shows the "(ej sammenligneligt)" note.
 */
function DeltaPct({
  current,
  prior,
  partial,
  invertTone = false,
}: {
  current: number;
  prior: number | null;
  partial: boolean;
  invertTone?: boolean;
}) {
  if (partial) {
    return <td className="num muted">{PARTIAL_NOTE}</td>;
  }
  if (prior === null || !Number.isFinite(prior) || prior === 0) {
    return <td className="num muted">—</td>;
  }
  const ratio = (current - prior) / Math.abs(prior);
  if (!Number.isFinite(ratio)) {
    return <td className="num muted">—</td>;
  }
  const positiveIsGood = !invertTone;
  const isGood = positiveIsGood ? ratio > 0 : ratio < 0;
  const isBad = positiveIsGood ? ratio < 0 : ratio > 0;
  const tone = isGood
    ? "amount-positive"
    : isBad
      ? "amount-negative"
      : "";
  const sign = ratio > 0 ? "+" : ratio < 0 ? "-" : "";
  const body = formatPercent(Math.abs(ratio));
  return (
    <td className={`num ${tone}`.trim()}>
      {sign}
      {body}
    </td>
  );
}

/**
 * Renders the change between two ratio (0–1 fraction) values as Danish-style
 * procentpoint, e.g. 0.176 → 0.224 becomes "+4,8 pp". A null on either side
 * yields "—" (the ratio itself was undefined). A partial live year shows the
 * "(ej sammenligneligt)" note.
 */
function DeltaPp({
  current,
  prior,
  partial,
}: {
  current: number | null;
  prior: number | null;
  partial: boolean;
}) {
  if (partial) {
    return <td className="num muted">{PARTIAL_NOTE}</td>;
  }
  if (
    current === null ||
    prior === null ||
    !Number.isFinite(current) ||
    !Number.isFinite(prior)
  ) {
    return <td className="num muted">—</td>;
  }
  const diff = (current - prior) * 100; // 0–1 fraction → percentage points
  const tone =
    diff > 0 ? "amount-positive" : diff < 0 ? "amount-negative" : "";
  const sign = diff > 0 ? "+" : diff < 0 ? "-" : "";
  // One decimal — same precision as formatPercent.
  const body = `${Math.abs(diff).toLocaleString("da-DK", {
    minimumFractionDigits: 1,
    maximumFractionDigits: 1,
  })} pp`;
  return (
    <td className={`num ${tone}`.trim()}>
      {sign}
      {body}
    </td>
  );
}
