import { ButtonLink, Button, Input, PageHeader } from "../components/ui";
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

export function AnnualReportView() {
  const { slug = "" } = useParams();
  const today = new Date();
  const defaultYear = today.getUTCFullYear() - 1;
  const [fiscalYearStart, setFiscalYearStart] = useState(`${defaultYear}-01-01`);
  const [fiscalYearEnd, setFiscalYearEnd] = useState(`${defaultYear}-12-31`);
  const [data, setData] = useState<CompanyAnnualReportResponse["annualReport"] | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const build = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    setLoading(true);
    try {
      const result = await api.annualReport(slug, fiscalYearStart, fiscalYearEnd);
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
      <PageHeader title="Årsrapport" actions={<><div className="row-actions">
          <ButtonLink className="btn secondary" to={`/companies/${slug}/manage`}>
            Administrér
          </ButtonLink>
        </div></>}>
        <div>

          <p className="muted">
            Forbereder en årsrapport for et lukket regnskabsår. Rentemester
            samler resultatopgørelse, balance og noter. Ejer eller revisor
            gennemgår materialet og er ansvarlig for indberetning.
          </p>
        </div>

      </PageHeader>

      <section className="card">
        <h3>Vælg regnskabsår</h3>
        <form onSubmit={build} className="filter-bar">
          <label>
            Start (YYYY-MM-DD)
            <Input
              type="date"
              value={fiscalYearStart}
              onChange={(e) => setFiscalYearStart(e.target.value)}
              required
            />
          </label>
          <label>
            Slut (YYYY-MM-DD)
            <Input
              type="date"
              value={fiscalYearEnd}
              onChange={(e) => setFiscalYearEnd(e.target.value)}
              required
            />
          </label>
          <Button type="submit" className="btn primary" disabled={loading}>
            {loading ? "Bygger …" : "Byg årsrapport"}
          </Button>
        </form>
      </section>

      {error && (
        <div className="callout danger" role="alert">
          {error}
        </div>
      )}

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
          registreret under <Link to="../manage">Administrér</Link>, regnskabsåret være lukket under{" "}
          <Link to="../periodelas">Periodelås</Link>, og bøgerne skal balancere.
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
        <div className="table-scroll"><table className="table">
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
        </table></div>
      </section>

      {report.profitAndLoss && (
        <section className="card">
          <h3>Resultatopgørelse</h3>
          <div className="table-scroll"><table className="table">
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
          </table></div>
        </section>
      )}

      {report.balanceSheet && (
        <section className="card">
          <h3>Balance ultimo {report.fiscalYearEnd}</h3>
          <div className="table-scroll"><table className="table">
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
          </table></div>
        </section>
      )}

      {report.comparison && (
        <section className="card">
          <h3>Sammenligningstal {report.comparison.fiscalYearStart} — {report.comparison.fiscalYearEnd}</h3>
          {!report.comparison.available ? <p className="muted">{report.comparison.reason === "prior_range_precedes_explicit_cutover"
            ? "Sammenligningstal er ikke tilgængelige: den foregående periode starter før den eksplicitte overtagelsesdato. Arkivbeløb indgår ikke automatisk."
            : "Ingen sammenligningstal — ingen saldi i den foregående periode."}</p> : (
            <div className="table-scroll"><table className="table"><tbody>
              <tr><th>Indtægter i alt</th><td className="num">{formatKroner(report.comparison.totalIncome, currency)}</td></tr>
              <tr><th>Omkostninger i alt</th><td className="num">{formatKroner(report.comparison.totalExpense, currency)}</td></tr>
              <tr><th>Årets resultat</th><td className="num">{formatKroner(report.comparison.aretsResultat, currency)}</td></tr>
              <tr><th>Aktiver i alt</th><td className="num">{formatKroner(report.comparison.totalAssets, currency)}</td></tr>
              <tr><th>Egenkapital i alt</th><td className="num">{formatKroner(report.comparison.equity, currency)}</td></tr>
            </tbody></table></div>
          )}
        </section>
      )}
      {report.disclaimer && <p className="callout">{report.disclaimer}</p>}

      {report.notes && report.notes.length > 0 && (
        <section className="card">
          <h3>Noter</h3>
          <ul>
            {report.notes.map((n) => (
              <li key={n.id}>
                <strong>{n.title}</strong>
                <p>{n.body}</p>
                {n.placeholder && <span className="muted">Udfyldes og gennemgås af ejer eller revisor.</span>}
              </li>
            ))}
          </ul>
        </section>
      )}

      {report.ledelsespategning && (
        <section className="card">
          <h3>Ledelsespåtegning</h3>
          {report.ledelsespategning.placeholder && <p className="muted">Skabelon — udfyld dato, sted og underskrifter inden indberetning.</p>}
          <p>{report.ledelsespategning.text}</p>
        </section>
      )}
    </>
  );
}
