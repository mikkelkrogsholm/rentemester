// Kørsel — the per-company mileage register (#335).
//
// Renders `/api/companies/:slug/mileage?year=`: a deterministic mileage log
// for the selected fiscal year. The page mirrors the cockpit's other
// company views — a `CompanyNav` + `useCompanyYear` chrome, a summary card
// strip ("Sum pr. periode") and a table of trips. A primary "Registrér
// kørsel" button opens `MileageRegisterModal` which POSTs through the same
// `createMileageEntry` core the CLI's `mileage add` command uses.
//
// The mileage register is documentation/audit data — Rentemester never
// posts it to the journal. The view therefore lives under the "Bogføring"
// sub-nav group as a reference register, not a posting screen. An archived
// regnskabsår hides the action button (same pattern as BankView /
// DocumentsView): historical mileage is read-only.

import { useState } from "react";
import { Link, useParams } from "react-router-dom";
import { api } from "../lib/api";
import { formatKroner } from "../lib/format";
import { useAsync } from "../lib/useAsync";
import type { CompanyMileage } from "../lib/types";
import { ErrorState, Loading } from "../components/Feedback";
import { CompanyNav, useCompanyYear } from "../components/CompanyNav";
import { MileageRegisterModal } from "../components/MileageRegisterModal";

export function MileageView() {
  const { slug = "" } = useParams();
  const { year, setYear } = useCompanyYear();
  const state = useAsync<CompanyMileage>(
    () => api.mileage(slug, year),
    [slug, year],
  );
  const [registering, setRegistering] = useState(false);

  if (state.loading && !state.data)
    return <Loading label="Henter kørselsregister…" />;
  if (state.error)
    return <ErrorState message={state.error} onRetry={state.reload} />;

  const m = state.data!;
  const currency = m.company.currency || "DKK";
  const archived = m.archived;
  const hasEntries = m.entries.length > 0;

  return (
    <section className="statement">
      <div className="page-head">
        <div>
          <h2>{m.company.name}</h2>
          <p className="muted">
            {m.company.cvr ? `CVR ${m.company.cvr} · ` : ""}
            {m.company.country} · {currency} · Kørsel
          </p>
        </div>
        <div className="row-actions">
          {!archived && (
            <button
              type="button"
              className="btn"
              onClick={() => setRegistering(true)}
            >
              Registrér kørsel
            </button>
          )}
          <Link className="btn secondary" to={`/companies/${slug}/manage`}>
            Administrér
          </Link>
        </div>
      </div>

      <CompanyNav
        slug={slug}
        years={m.fiscalYears}
        selectedYear={m.selectedYear}
        onYearChange={setYear}
      />

      {archived ? (
        <ArchivedNotice year={m.selectedYear} />
      ) : !hasEntries ? (
        <EmptyState
          year={m.selectedYear}
          onRegister={() => setRegistering(true)}
        />
      ) : (
        <>
          <p className="statement-asof muted">
            Kørselsregister — regnskabsår {m.selectedYear}
          </p>

          <SummaryCards mileage={m} currency={currency} />

          <MonthlyBreakdown mileage={m} currency={currency} />

          <div className="card statement-card table-scroll">
            <table className="data statement-table">
              <thead>
                <tr>
                  <th>Bilag</th>
                  <th>Dato</th>
                  <th>Formål</th>
                  <th>Fra → Til</th>
                  <th className="num">Km</th>
                  <th className="num">Takst</th>
                  <th className="num">Grundlag</th>
                  <th>Takst-basis</th>
                </tr>
              </thead>
              <tbody>
                {m.entries.map((row) => (
                  <tr key={row.id}>
                    <td className="account-no">{row.entryNo}</td>
                    <td className="entry-date">{row.tripDate}</td>
                    <td>{row.purpose}</td>
                    <td>
                      {row.fromLocation} → {row.toLocation}
                    </td>
                    <td className="num">{row.kilometers}</td>
                    <td className="num">
                      {formatKroner(row.ratePerKm, currency)}/km
                    </td>
                    <td className="num">
                      {formatKroner(row.amountBasis, currency)}
                    </td>
                    <td>
                      {row.rateSource ? (
                        <a
                          href={row.rateSource}
                          target="_blank"
                          rel="noreferrer noopener"
                        >
                          {row.rateBasis}
                        </a>
                      ) : (
                        row.rateBasis
                      )}
                    </td>
                  </tr>
                ))}
                <tr className="statement-result">
                  <td colSpan={4}>I alt — {m.selectedYear}</td>
                  <td className="num">{m.totalKilometers}</td>
                  <td className="num">—</td>
                  <td className="num">
                    {formatKroner(m.totalAmountBasis, currency)}
                  </td>
                  <td />
                </tr>
              </tbody>
            </table>
          </div>
          <p className="statement-check ok">
            Kørselsregisteret er dokumentation. Rentemester bogfører aldrig
            kørselsgodtgørelse direkte — vurdér med din rådgiver om turen er
            fradragsberettiget, og brug fx en udlægspostering hvis du vil
            udbetale eller fratrække beløbet. Officielle satser findes på{" "}
            <a
              href="https://skat.dk/erhverv/moms/regler-og-satser/satser-for-erhvervsmaessig-koersel"
              target="_blank"
              rel="noreferrer noopener"
            >
              skat.dk
            </a>
            .
          </p>
        </>
      )}

      {registering && (
        <MileageRegisterModal
          slug={slug}
          onRegistered={() => state.reload()}
          onClose={() => setRegistering(false)}
        />
      )}
    </section>
  );
}

function SummaryCards({
  mileage,
  currency,
}: {
  mileage: CompanyMileage;
  currency: string;
}) {
  return (
    <div className="status-grid invoices-summary">
      <div className="card status-card">
        <h3>Antal ture</h3>
        <div className="status-figure">{mileage.tripCount}</div>
        <p className="muted status-note">
          {mileage.tripCount === 1 ? "kørsel" : "kørsler"} i{" "}
          {mileage.selectedYear}
        </p>
      </div>
      <div className="card status-card">
        <h3>Samlet km</h3>
        <div className="status-figure">{mileage.totalKilometers}</div>
        <p className="muted status-note">km i regnskabsåret</p>
      </div>
      <div className="card status-card">
        <h3>Godtgørelsesgrundlag</h3>
        <div className="status-figure">
          {formatKroner(mileage.totalAmountBasis, currency)}
        </div>
        <p className="muted status-note">km × takst — dokumentation</p>
      </div>
    </div>
  );
}

function MonthlyBreakdown({
  mileage,
  currency,
}: {
  mileage: CompanyMileage;
  currency: string;
}) {
  // Only show the breakdown when at least one month has activity — otherwise
  // it is just twelve rows of zero, which the summary card already conveys.
  if (mileage.months.every((m) => m.tripCount === 0)) return null;
  return (
    <div className="card statement-card table-scroll">
      <h3 style={{ margin: "0.25rem 0 0.5rem" }}>Sum pr. måned</h3>
      <table className="data statement-table">
        <thead>
          <tr>
            <th>Måned</th>
            <th className="num">Ture</th>
            <th className="num">Km</th>
            <th className="num">Grundlag</th>
          </tr>
        </thead>
        <tbody>
          {mileage.months.map((row) => (
            <tr key={row.month}>
              <td>{row.label}</td>
              <td className="num">{row.tripCount}</td>
              <td className="num">{row.kilometers}</td>
              <td className="num">
                {row.amountBasis > 0
                  ? formatKroner(row.amountBasis, currency)
                  : "—"}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function EmptyState({
  year,
  onRegister,
}: {
  year: string;
  onRegister: () => void;
}) {
  return (
    <div className="card archived-notice">
      <h3>Ingen kørsler registreret i {year}</h3>
      <p className="muted">
        Klik på <strong>Registrér kørsel</strong> for at logge en tur — dato,
        formål, fra/til, antal km og den officielle takst du har slået op.
        Rentemester gemmer registret som dokumentation og udregner aldrig en
        skattesats for dig.
      </p>
      <div className="row-actions" style={{ marginTop: "0.5rem" }}>
        <button type="button" className="btn" onClick={onRegister}>
          Registrér første kørsel
        </button>
      </div>
    </div>
  );
}

function ArchivedNotice({ year }: { year: string }) {
  return (
    <div className="card archived-notice">
      <h3>Kørsel er ikke tilgængelig for {year}</h3>
      <p className="muted">
        {year} er et arkiveret regnskabsår. Kørselsregisteret findes kun for
        den aktive ledger; arkiverede år vises som read-only historik på de
        øvrige skærmbilleder (Resultatopgørelse, Balance osv.).
      </p>
    </div>
  );
}
