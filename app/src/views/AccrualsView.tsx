// Periodisering / accrual register (#337).
//
// Read-only view: per-virksomhed liste over registrerede accruals med
// recognized amount, remaining amount og portfolio-totals. Genbruger
// kernens `buildAccrualRegisterReport` direkte.
//
// Register-new-accrual + recognize-period write-flows er parkeret som
// follow-ups — kernens CLI (`accrual register` / `accrual recognize`)
// dækker dem indtil videre.

import { Link, useParams } from "react-router-dom";
import { api } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import { formatKroner } from "../lib/format";
import type {
  AccrualRegisterRow,
  CompanyAccrualsResponse,
} from "../lib/types";
import { ErrorState, Loading } from "../components/Feedback";

const TYPE_LABEL: Record<AccrualRegisterRow["accrualType"], string> = {
  prepaid_expense: "Forudbetalt omkostning",
  accrued_expense: "Skyldig omkostning",
  deferred_revenue: "Udskudt omsætning",
};

export function AccrualsView() {
  const { slug = "" } = useParams();
  const state = useAsync<CompanyAccrualsResponse["accruals"]>(
    () => api.accruals(slug),
    [slug],
  );

  if (state.loading) return <Loading />;
  if (state.error) return <ErrorState message={state.error} />;
  const data = state.data!;
  const r = data.report;
  const currency = data.company.currency || "DKK";

  return (
    <section className="accruals-view">
      <header className="page-head">
        <div>
          <h2>{data.company.name}</h2>
          <p className="muted">
            {data.company.cvr ? `CVR ${data.company.cvr} · ` : ""}
            {data.company.country} · {currency} · Periodisering
          </p>
        </div>
        <div className="row-actions">
          <Link className="btn secondary" to={`/companies/${slug}/manage`}>
            Administrér
          </Link>
        </div>
      </header>

      <p className="muted">
        Periodeafgrænsningsposter (PAP): forudbetalte omkostninger,
        skyldige omkostninger, udskudt omsætning. Beløb rulles ud lineært
        over de aftalte perioder. Read-only; nye accruals registreres via
        CLI'ens <code>accrual register</code> (skrive-flow i cockpittet
        følger).
      </p>

      <section className="card">
        <h3>Portfolio</h3>
        <div className="filter-bar">
          <span className="pill">
            I alt: {formatKroner(r.totals.totalAmount, currency)}
          </span>
          <span className="pill ok">
            Realiseret: {formatKroner(r.totals.recognizedAmount, currency)}
          </span>
          <span className="pill warn">
            Tilbage: {formatKroner(r.totals.remainingAmount, currency)}
          </span>
        </div>
      </section>

      <section className="card">
        <h3>Accruals ({r.accruals.length})</h3>
        {r.accruals.length === 0 ? (
          <p className="muted">
            Ingen accruals registreret. Periodeafgrænsningsposter oprettes via
            CLI'ens <code>accrual register</code>.
          </p>
        ) : (
          <table className="table">
            <thead>
              <tr>
                <th>Type</th>
                <th>Beskrivelse</th>
                <th>Total</th>
                <th>Realiseret</th>
                <th>Tilbage</th>
                <th>Perioder</th>
                <th>Første dato</th>
                <th>Balance / Result</th>
                <th>Status</th>
              </tr>
            </thead>
            <tbody>
              {r.accruals.map((a) => (
                <AccrualRow key={a.accrualId} row={a} currency={currency} />
              ))}
            </tbody>
          </table>
        )}
      </section>
    </section>
  );
}

function AccrualRow({
  row,
  currency,
}: {
  row: AccrualRegisterRow;
  currency: string;
}) {
  return (
    <tr>
      <td>{TYPE_LABEL[row.accrualType]}</td>
      <td>{row.description}</td>
      <td className="num">{formatKroner(row.totalAmount, currency)}</td>
      <td className="num">{formatKroner(row.recognizedAmount, currency)}</td>
      <td className="num">{formatKroner(row.remainingAmount, currency)}</td>
      <td>
        {row.recognizedPeriods}/{row.recognitionPeriods} ×{" "}
        {row.periodStepMonths} mdr
      </td>
      <td className="entry-date">{row.firstRecognitionDate}</td>
      <td>
        <code>{row.balanceAccountNo}</code> /{" "}
        <code>{row.resultAccountNo}</code>
      </td>
      <td>
        {row.fullyRecognized ? (
          <span className="pill ok">Fuldt realiseret</span>
        ) : (
          <span className="pill">Aktiv</span>
        )}
      </td>
    </tr>
  );
}
