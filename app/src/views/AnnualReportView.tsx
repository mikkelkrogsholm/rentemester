// Årsrapport-builder UI (#338).
//
// Per-virksomhed view der bygger en regnskabsklasse-B-årsrapport for et
// regnskabsår. Brugeren angiver start + slut; resten kommer fra kernens
// `buildAnnualReport` (samme funktion som CLI'ens `report annual`).
// Forudsætnings-fejl (CVR mangler, periode er ikke låst, bøgerne
// balancerer ikke) vises tydeligt så ejeren kan rette dem og prøve igen.

import { useState } from "react";
import { Link, useParams } from "react-router-dom";
import { api, ApiError } from "../lib/api";
import { formatKroner } from "../lib/format";
import type {
  AnnualReport,
  CompanyAnnualReportResponse,
} from "../lib/types";
import { StatusChip } from "../components/CockpitPrimitives";
import { useAsync } from "../lib/useAsync";

export function AnnualReportView() {
  const { slug = "" } = useParams();
  const today = new Date();
  const defaultYear = today.getUTCFullYear() - 1;
  const [selectedYear, setSelectedYear] = useState(String(defaultYear));
  const readiness = useAsync(() => api.annualReport(slug, "", "", selectedYear), [slug, selectedYear]);
  const [data, setData] = useState<CompanyAnnualReportResponse["annualReport"] | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const build = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    setLoading(true);
    try {
      const result = await api.annualReport(slug, "", "", selectedYear);
      setData(result);
    } catch (err) {
      setError(err instanceof ApiError ? err.message : "Kunne ikke bygge årsrapport.");
      setData(null);
    } finally {
      setLoading(false);
    }
  };

  return (
    <section className="annual-report-view">
      <header className="page-head">
        <div>
          <h2>Årsrapport (regnskabsklasse B)</h2>
          <p className="muted">
            Forbereder en årsrapport for et lukket regnskabsår. Rentemester
            samler resultatopgørelse, balance og noter — den endelige aflæsning
            sker hos revisor.
          </p>
        </div>
        <div className="row-actions">
          <Link className="btn secondary" to={`/companies/${slug}/manage`}>
            Administrér
          </Link>
        </div>
      </header>

      <section className="card">
        <h3>Vælg regnskabsår</h3>
        <form onSubmit={build} className="filter-bar">
          <label htmlFor="annual-year">Regnskabsår
            <select id="annual-year" value={selectedYear} onChange={(e) => setSelectedYear(e.target.value)}>
              {[0, 1, 2, 3, 4].map(offset => <option key={defaultYear - offset} value={defaultYear - offset}>{defaultYear - offset}</option>)}
            </select>
          </label>
          <button type="submit" className="btn primary" disabled={loading || readiness.loading || readiness.data?.readiness?.status === "Ikke klar"} title={readiness.data?.readiness?.status === "Ikke klar" ? "Ret readiness-kontrollerne før build" : undefined}>
            {loading ? "Bygger …" : "Byg årsrapport"}
          </button>
        </form>
      </section>

      {error && (
        <div className="callout danger" role="alert">
          {error}
        </div>
      )}

      {readiness.data && <section className="card"><div className="statement-card-head"><h3>Readiness</h3><StatusChip tone={readiness.data.readiness?.status === "Klar" ? "success" : "danger"}>{readiness.data.readiness?.status ?? "Ikke klar"}</StatusChip></div><p>Canoniske datoer: {readiness.data.fiscalYearStart} – {readiness.data.fiscalYearEnd}</p><ul>{(readiness.data.readiness?.items ?? []).map(item => <li key={item.label}>{item.ok ? "✓" : "•"} {item.label}{!item.ok && <> — <Link to={`/companies/${slug}/${item.destination === "periods" ? "perioder" : "manage"}`}>Åbn løsning</Link></>}</li>)}</ul></section>}

      {data && <ReportPanel report={data.report} />}
    </section>
  );
}

function ReportPanel({ report }: { report: AnnualReport }) {
  if (!report.ok) {
    return (
      <section className="card">
        <h3>Forudsætninger mangler</h3>
        <p className="muted">
          For at en årsrapport kan dannes skal: virksomhedens CVR være
          registreret, regnskabsåret være lukket under{" "}
          <Link to="../periods">Periodelås</Link>, og bøgerne skal balancere.
        </p>
        <ul className="audit-errors">
          {report.errors.map((err, i) => (
            <li key={i}>{err}</li>
          ))}
        </ul>
      </section>
    );
  }
  const currency = report.company.currency || "DKK";
  return (
    <>
      <section className="card">
        <h3>
          Stamdata pr. {report.fiscalYearStart} — {report.fiscalYearEnd}
        </h3>
        <table className="table">
          <tbody>
            <tr>
              <th>Virksomhed</th>
              <td>{report.company.name}</td>
            </tr>
            <tr>
              <th>CVR</th>
              <td>{report.company.cvr ?? "—"}</td>
            </tr>
            <tr>
              <th>Land</th>
              <td>{report.company.country}</td>
            </tr>
            <tr>
              <th>Valuta</th>
              <td>{report.company.currency}</td>
            </tr>
          </tbody>
        </table>
      </section>

      {report.profitAndLoss && (
        <section className="card">
          <h3>Resultatopgørelse</h3>
          <table className="table">
            <tbody>
              <tr>
                <th>Indtægter i alt</th>
                <td className="num">
                  {formatKroner(report.profitAndLoss.income.total, currency)}
                </td>
              </tr>
              <tr>
                <th>Omkostninger i alt</th>
                <td className="num">
                  {formatKroner(report.profitAndLoss.expense.total, currency)}
                </td>
              </tr>
              <tr className="statement-result">
                <th>Årets resultat</th>
                <td className="num">
                  {formatKroner(report.profitAndLoss.result, currency)}
                </td>
              </tr>
            </tbody>
          </table>
        </section>
      )}

      {report.balanceSheet && (
        <section className="card">
          <h3>Balance ultimo {report.fiscalYearEnd}</h3>
          <table className="table">
            <tbody>
              <tr>
                <th>Aktiver i alt</th>
                <td className="num">
                  {formatKroner(report.balanceSheet.assets.total, currency)}
                </td>
              </tr>
              <tr>
                <th>Gæld i alt</th>
                <td className="num">
                  {formatKroner(
                    report.balanceSheet.liabilities.total,
                    currency,
                  )}
                </td>
              </tr>
              <tr>
                <th>Egenkapital i alt</th>
                <td className="num">
                  {formatKroner(report.balanceSheet.equity.total, currency)}
                </td>
              </tr>
            </tbody>
          </table>
        </section>
      )}

      {report.notes && report.notes.length > 0 && (
        <section className="card">
          <h3>Noter</h3>
          <ul>
            {report.notes.map((n) => (
              <li key={n.id}>
                <strong>{n.title}</strong>
                <p>{n.body}</p>
              </li>
            ))}
          </ul>
        </section>
      )}

      {report.ledelsespategning && (
        <section className="card">
          <h3>Ledelsespåtegning</h3>
          <p className="muted">Dato: {report.ledelsespategning.date}</p>
          <p>{report.ledelsespategning.body}</p>
        </section>
      )}
    </>
  );
}
