// Periodisering / accrual register (#337).
//
// Read-only view: per-virksomhed liste over registrerede accruals med
// recognized amount, remaining amount og portfolio-totals. Genbruger
// kernens `buildAccrualRegisterReport` direkte.
//
// Register-new-accrual + recognize-period write-flows er parkeret som
// follow-ups — kernens CLI (`accrual register` / `accrual recognize`)
// dækker dem indtil videre.

import { useState } from "react";
import { Link, useParams } from "react-router-dom";
import { api } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import { formatKroner } from "../lib/format";
import type {
  AccrualRegisterRow,
  CompanyAccrualsResponse,
} from "../lib/types";
import { PageState, StatusChip } from "../components/CockpitPrimitives";

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

  if (state.loading && !state.data) return <PageState kind="loading" title="Henter periodiseringer" />;
  if (state.error) return <PageState kind="error" title="Periodiseringer kunne ikke hentes" onRetry={state.reload}>{state.error}</PageState>;
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

      <section className="card"><div className="statement-card-head"><h3>Sikker næste handling</h3><StatusChip tone="info">Kræver review</StatusChip></div><p>Rentemester bogfører ikke en periodisering fra denne side. Gennemgå først opgaven og brug derefter den eksisterende agent-/review-arbejdsgang.</p><Link className="btn secondary" to={`/companies/${slug}/opmaerksomhed`}>Åbn opgaver der kræver opmærksomhed</Link><CopySafeStep slug={slug} /></section>

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
          <PageState kind="empty" title="Ingen periodiseringer registreret">Ingen accruals registreret. Åbn opgaver der kræver opmærksomhed og kopiér den sikre næste handling. Den opretter eller bogfører intet.</PageState>
        ) : (
          <div className="table-scroll"><table className="table responsive-table" aria-label="Periodiseringer">
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
          </table></div>
        )}
      </section>
    </section>
  );
}

function CopySafeStep({ slug }: { slug: string }) {
  const [copied, setCopied] = useState(false);
  const text = `Åbn Cockpit → ${slug} → Opgaver der kræver opmærksomhed. Gennemgå bilag og forslag, lav dry run og bekræft først derefter en periodisering.`;
  return <button type="button" className="btn secondary" onClick={async () => { try { await navigator.clipboard.writeText(text); setCopied(true); } catch { /* visible text remains safe */ } }}>{copied ? "Kopieret" : "Kopiér sikker næste handling"}</button>;
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
