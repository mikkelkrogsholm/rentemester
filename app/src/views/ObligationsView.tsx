// Forpligtelser — the per-company obligations view (cockpit-redesign it. 7).
//
// Renders `/api/companies/:slug/obligations?year=`: the "hvad skylder jeg og
// hvornår" list — outstanding VAT, corporation tax, trade creditors, accrued
// auditor and any other payable read from the ledger, each with the amount
// owed and a due date where one is derivable. Rows are sorted by due date
// (soonest first); a row with no known date shows "—". The total owed sits in
// a summary card above the table. All money fields are kroner — `formatKroner`
// is used throughout.

import { Link, useParams } from "react-router-dom";
import { api } from "../lib/api";
import { formatKroner } from "../lib/format";
import { useAsync } from "../lib/useAsync";
import type { CompanyObligations, ObligationRow } from "../lib/types";
import { ErrorState, Loading } from "../components/Feedback";
import { CompanyNav, useCompanyYear } from "../components/CompanyNav";

export function ObligationsView() {
  const { slug = "" } = useParams();
  const { year, setYear } = useCompanyYear();
  const state = useAsync<CompanyObligations>(
    () => api.obligations(slug, year),
    [slug, year],
  );

  if (state.loading && !state.data)
    return <Loading label="Henter forpligtelser…" />;
  if (state.error)
    return <ErrorState message={state.error} onRetry={state.reload} />;

  const o = state.data!;
  const currency = o.company.currency || "DKK";

  return (
    <section className="statement">
      <div className="page-head">
        <div>
          <h2>{o.company.name}</h2>
          <p className="muted">
            {o.company.cvr ? `CVR ${o.company.cvr} · ` : ""}
            {o.company.country} · {currency} · Forpligtelser
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
        years={o.fiscalYears}
        selectedYear={o.selectedYear}
        onYearChange={setYear}
      />

      {o.archived ? (
        <ArchivedNotice year={o.selectedYear} />
      ) : o.obligations.length === 0 ? (
        <div className="card archived-notice">
          <h3>Ingen forpligtelser</h3>
          <p className="muted">
            Der er ingen udestående moms, skat eller kreditorgæld i
            regnskabsåret {o.selectedYear}. Skyldige beløb vises her, så snart
            de er bogført.
          </p>
        </div>
      ) : (
        <>
          <p className="statement-asof muted">
            Hvad virksomheden skylder — regnskabsår {o.selectedYear}
          </p>

          <div className="status-grid invoices-summary">
            <div className="card status-card">
              <h3>Skyldige beløb i alt</h3>
              <div
                className={`status-figure${
                  o.totalOwed > 0 ? " status-alert" : ""
                }`}
              >
                {formatKroner(o.totalOwed, currency)}
              </div>
              <p className="muted status-note">
                {o.obligations.length}{" "}
                {o.obligations.length === 1 ? "forpligtelse" : "forpligtelser"}{" "}
                · sorteret efter frist
              </p>
            </div>
          </div>

          <div className="card statement-card table-scroll">
            <table className="data statement-table">
              <thead>
                <tr>
                  <th>Forpligtelse</th>
                  <th>Konto</th>
                  <th>Frist</th>
                  <th>Status</th>
                  <th className="num">Skyldigt beløb</th>
                  <th>Handlinger</th>
                </tr>
              </thead>
              <tbody>
                {o.obligations.map((row, i) => (
                  <tr key={`${row.kind}-${row.accountNo ?? i}`}>
                    <td>{row.label}</td>
                    <td className="account-no">{row.accountNo ?? "—"}</td>
                    <td className="entry-date">{row.dueDate ?? "—"}</td>
                    <td>
                      <DeadlineFlag row={row} />
                    </td>
                    <td className="num">
                      {/* The annual-report row is a filing DEADLINE, not a
                          debt — it has no kroner amount, so show a dash. */}
                      {row.kind === "annual-report"
                        ? "—"
                        : formatKroner(row.amount, currency)}
                    </td>
                    <td>
                      <ObligationActions
                        row={row}
                        slug={slug}
                        currency={currency}
                      />
                    </td>
                  </tr>
                ))}
                <tr className="statement-result negative">
                  <td colSpan={4}>Skyldige beløb i alt</td>
                  <td className="num">
                    {formatKroner(o.totalOwed, currency)}
                  </td>
                  <td></td>
                </tr>
              </tbody>
            </table>
          </div>
          <p className="statement-check ok">
            Beløbene er læst direkte fra ledgeren. Frister vises hvor de kan
            udledes — moms efter virksomhedens momsperiode (måned, kvartal eller
            halvår), selskabsskat efter indkomståret, og årsrapporten til
            Erhvervsstyrelsen efter regnskabsåret; øvrige poster har ingen kendt
            dato.
          </p>
        </>
      )}
    </section>
  );
}

/**
 * The deadline status flag for an obligation row: a "X dage tilbage"
 * countdown that turns critical once the deadline is near or passed, and a
 * neutral "Ingen frist" for a dateless payable.
 */
function DeadlineFlag({ row }: { row: ObligationRow }) {
  if (row.dueDate === null || row.daysRemaining === null) {
    return <span className="flag neutral">Ingen frist</span>;
  }
  const days = row.daysRemaining;
  if (days < 0) {
    return (
      <span className="flag critical">
        Overskredet {Math.abs(days)} {Math.abs(days) === 1 ? "dag" : "dage"}
      </span>
    );
  }
  if (days === 0) {
    return <span className="flag critical">Frist i dag</span>;
  }
  const tone = days <= 30 ? "warning" : "ok";
  return (
    <span className={`flag ${tone}`}>
      {days} {days === 1 ? "dag" : "dage"} tilbage
    </span>
  );
}

/**
 * Pr-række handlinger til Forpligtelser (#391):
 *
 * - Eksterne handoff-links til den officielle indberetnings-portal (skat.dk,
 *   indberet.virk.dk) så ejeren ikke skal google sig frem.
 * - 'Kopiér beløb' så ejeren ikke skal taste fx 47.310,00 manuelt ind i
 *   sin bank-app.
 * - Krydslink til Moms-viewet for vat-rækken, så ejeren kan se SKAT-
 *   rubrikkerne ved siden af.
 *
 * Note: 'Markér som betalt'-flowet (#391 acceptkriterium 4) er ikke
 * implementeret her — det er et write-flow med actor + audit-event og
 * spores som follow-up.
 */
const EXTERNAL_LINK: Partial<Record<ObligationRow["kind"], { href: string; label: string }>> = {
  vat: {
    href: "https://www.skat.dk/erhverv/moms",
    label: "Indberet på skat.dk",
  },
  "corporation-tax": {
    href: "https://www.skat.dk/erhverv/selskabsskat",
    label: "Selskabsskat på skat.dk",
  },
  "annual-report": {
    href: "https://indberet.virk.dk",
    label: "Indberet på virk.dk",
  },
};

function ObligationActions({
  row,
  slug,
  currency,
}: {
  row: ObligationRow;
  slug: string;
  currency: string;
}) {
  const external = EXTERNAL_LINK[row.kind];
  const copyAmount = async () => {
    if (row.kind === "annual-report") return;
    const text = String(row.amount.toFixed(2)).replace(".", ",");
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      // Clipboard API may be blocked (Safari permission, http) — silent fail.
    }
  };
  return (
    <div className="row-actions inline-actions">
      {external && (
        <a
          className="btn small secondary"
          href={external.href}
          target="_blank"
          rel="noopener noreferrer"
        >
          {external.label}
        </a>
      )}
      {row.kind === "vat" && (
        <Link className="btn small secondary" to={`/companies/${slug}/moms`}>
          SKAT-rubrikker
        </Link>
      )}
      {row.kind !== "annual-report" && (
        <button
          type="button"
          className="btn small secondary"
          onClick={copyAmount}
          aria-label={`Kopiér beløb (${formatKroner(row.amount, currency)})`}
        >
          Kopiér beløb
        </button>
      )}
    </div>
  );
}

function ArchivedNotice({ year }: { year: string }) {
  return (
    <div className="card archived-notice">
      <h3>Forpligtelser er ikke tilgængelige for {year}</h3>
      <p className="muted">
        {year} er et arkiveret regnskabsår. Forpligtelser — moms, selskabsskat
        og kreditorgæld med forfaldsdato — opgøres kun for den aktive ledger og
        vises derfor ikke for et arkiveret år. Resultatopgørelse, balance,
        saldobalance og posteringer for {year} er tilgængelige.
      </p>
    </div>
  );
}
