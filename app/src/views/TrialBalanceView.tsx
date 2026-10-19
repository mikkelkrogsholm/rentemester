import { ReportSourceNotice } from "../components/ReportSourceNotice";
import { ButtonLink, PageHeader } from "../components/ui";
// Saldobalance — the per-company trial balance (cockpit-redesign iteration 2).
//
// Renders `/api/companies/:slug/trial-balance?year=`: a table of every account
// that moved in the year, with its summed debit total, credit total and the
// signed net balance. A totals row closes the table; the report is balanced
// when total debit equals total credit. All money fields are kroner.

import { Link, useParams } from "react-router-dom";
import { api } from "../lib/api";
import { formatKroner } from "../lib/format";
import { useAsync } from "../lib/useAsync";
import type { CompanyTrialBalance } from "../lib/types";
import { ErrorState, Loading } from "../components/Feedback";
import { ArchivedBanner } from "../components/ArchivedBanner";
import {
  CompanyNav,
  accountPostingsTo,
  useCompanyYear,
} from "../components/CompanyNav";

export function TrialBalanceView() {
  const { slug = "" } = useParams();
  const { year, source, setYear } = useCompanyYear();
  const state = useAsync<CompanyTrialBalance>(
    (signal) => api.trialBalance(slug, year, { signal, ...(source ? {source} : {}) }),
    [slug, year, source],
  );

  if (state.loading && !state.data)
    return <Loading label="Henter saldobalance…" />;
  if (state.error)
    return <ErrorState message={state.error} onRetry={state.reload} />;

  const t = state.data!;
  const currency = t.declaredCurrency ?? t.company.currency ?? "DKK";
  const selectedSource = t.selectedSource ?? source;

  return (
    <section className="statement">
      <PageHeader title="Saldobalance" actions={<><div className="row-actions">
          {/* #372 — "Hent CSV". #463 — "Hent PDF" som printbar version. */}
          {t.completeness && t.completeness.status !== "ready" ? <p>CSV og PDF kræver et fuldstændigt rapportgrundlag.</p> : <>
          <a
            className="btn secondary"
            href={api.statementCsvUrl(slug, "trial-balance", t.selectedYear, selectedSource)}
            download
          >
            Hent CSV
          </a>
          <a
            className="btn secondary"
            href={api.statementPdfUrl(slug, "trial-balance", t.selectedYear, selectedSource)}
            download
          >
            Hent PDF
          </a>
          </>}
          <ButtonLink className="btn secondary" to={`/companies/${slug}/manage`}>
            Administrér
          </ButtonLink>
        </div></>}>
        <div>

          <p className="muted">
            {t.company.cvr ? `CVR ${t.company.cvr} · ` : ""}
            {t.company.country} · {currency} · Saldobalance
          </p>
        </div>

      </PageHeader>

      <CompanyNav
        slug={slug}
        years={t.fiscalYears}
        selectedYear={t.selectedYear}
        selectedSource={selectedSource}
        onYearChange={setYear}
      />

      <ReportSourceNotice report={t} slug={slug}/>
      {t.archived && (
        <ArchivedBanner year={t.selectedYear} source={t.archivedSource} />
      )}
      <p className="statement-asof muted">
        {t.periodStart && t.periodEnd ? `${t.periodStart} – ${t.periodEnd}` : "Arkiveret afslutningssnapshot — dato ikke oplyst"}{t.archived && " · Afslutningssaldi, ikke årets debet-/kreditomsætning"}
      </p>
      <div className="card statement-card table-scroll">
        <table className="data statement-table">
          <thead>
            <tr>
              <th>Konto</th>
              <th>Navn</th>
              <th className="num">Debet</th>
              <th className="num">Kredit</th>
              <th className="num">Saldo</th>
            </tr>
          </thead>
          <tbody>
            {t.rows.length === 0 ? (
              <tr>
                <td colSpan={5} className="empty-inline">
                  {t.completeness?.status === "unavailable" ? "Afslutningssaldi ikke tilgængelige." : "Ingen posteringer i året."}
                </td>
              </tr>
            ) : (
              t.rows.map((row) => (
                <tr key={row.accountNo} className="account-row">
                  <td className="account-no">
                    <Link
                      className="account-link"
                      to={accountPostingsTo(
                        slug,
                        t.selectedYear,
                        row.accountNo,
                        selectedSource,
                      )}
                    >
                      {row.accountNo}
                    </Link>
                  </td>
                  <td>{row.name}{row.sourceRows && <details><summary>Oprindelige saldi for konto {row.accountNo}</summary>{row.sourceRows.map(source=><p key={source.lineNo}>Række {source.lineNo}: {source.sourceAmount} · rapportbeløb {formatKroner(source.amount,currency)}</p>)}</details>}</td>
                  <td className="num">
                    {formatKroner(row.debit, currency)}
                  </td>
                  <td className="num">
                    {formatKroner(row.credit, currency)}
                  </td>
                  <td className="num">
                    {formatKroner(row.balance, currency)}
                  </td>
                </tr>
              ))
            )}
          </tbody>
          <tfoot>
            <tr className="statement-subtotal">
              <td colSpan={2}>I alt</td>
              <td className="num">
                {formatKroner(t.totalDebit, currency)}
              </td>
              <td className="num">
                {formatKroner(t.totalCredit, currency)}
              </td>
              <td className="num" />
            </tr>
          </tfoot>
        </table>
      </div>
      <p className={`statement-check ${t.balanced ? "ok" : "alert"}`}>
        {t.balanced === null ? "Saldobalancens kontrol kan ikke opgøres med det tilgængelige grundlag." : t.balanced
          ? "Saldobalancen stemmer — debet = kredit."
          : "Saldobalancen stemmer ikke. Kontrollér ledgeren."}
      </p>
    </section>
  );
}
