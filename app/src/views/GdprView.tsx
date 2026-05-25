// GDPR export + forget UI (#334).
//
// Per-virksomhed view der hjælper ejeren med at besvare en indsigtsanmodning
// (Persondataforordningens art. 15) og udføre en sletning (art. 17). Begge
// flows er tynde skaller over kernens buildGdprSubjectExport og
// eraseGdprSubject — kernen håndterer 5-års retention og audit-log'ing.

import { useState } from "react";
import { Link, useParams } from "react-router-dom";
import { api, ApiError } from "../lib/api";
import type {
  CompanyGdpr,
  GdprErasureResult,
  GdprExportRecord,
} from "../lib/types";

const SOURCE_LABEL: Record<string, string> = {
  customers: "Kunde",
  vendors: "Leverandør",
  documents: "Bilag",
  bank_transactions: "Banktransaktion",
};

export function GdprView() {
  const { slug = "" } = useParams();
  const [cvr, setCvr] = useState("");
  const [name, setName] = useState("");
  const [exportData, setExportData] = useState<CompanyGdpr | null>(null);
  const [erasure, setErasure] = useState<GdprErasureResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [erasing, setErasing] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const runExport = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    setErasure(null);
    setLoading(true);
    try {
      const data = await api.gdprExport(slug, {
        cvr: cvr.trim() || undefined,
        name: name.trim() || undefined,
      });
      setExportData(data);
    } catch (err) {
      setError(err instanceof ApiError ? err.message : "Indsigtsopslag fejlede.");
      setExportData(null);
    } finally {
      setLoading(false);
    }
  };

  const runErase = async () => {
    if (!exportData) return;
    if (
      !confirm(
        "Anonymisering skriver append-only tombstones. Rækker der stadig er under bogføringspligt (5 år) afvises. Fortsæt?",
      )
    )
      return;
    setError(null);
    setErasing(true);
    try {
      const result = await api.gdprErase(slug, {
        cvr: cvr.trim() || undefined,
        name: name.trim() || undefined,
      });
      setErasure(result);
      // Re-run export så ejeren ser den opdaterede status.
      const refreshed = await api.gdprExport(slug, {
        cvr: cvr.trim() || undefined,
        name: name.trim() || undefined,
      });
      setExportData(refreshed);
    } catch (err) {
      setError(
        err instanceof ApiError ? err.message : "Anonymisering fejlede.",
      );
    } finally {
      setErasing(false);
    }
  };

  return (
    <section className="gdpr-view">
      <header className="page-head">
        <div>
          <h2>GDPR-indsigt</h2>
          <p className="muted">
            Find personoplysninger om en data-subject (kunde/leverandør) og
            anonymisér dem hvor bogføringspligten ikke længere kræver dem.
          </p>
        </div>
        <div className="row-actions">
          <Link className="btn secondary" to={`/companies/${slug}/manage`}>
            Administrér
          </Link>
        </div>
      </header>

      <section className="card">
        <h3>Søg subject</h3>
        <form onSubmit={runExport} className="filter-bar">
          <label>
            CVR
            <input
              type="text"
              value={cvr}
              onChange={(e) => setCvr(e.target.value)}
              placeholder="DK…"
            />
          </label>
          <label>
            Navn
            <input
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="fx 'Acme ApS'"
            />
          </label>
          <button
            type="submit"
            className="btn primary"
            disabled={loading || (!cvr.trim() && !name.trim())}
          >
            {loading ? "Søger …" : "Hent indsigtsrapport"}
          </button>
        </form>
        <p className="muted">
          Mindst ét felt er påkrævet. Søgning er sag-følsom på navn.
        </p>
      </section>

      {error && (
        <div className="callout danger" role="alert">
          {error}
        </div>
      )}

      {exportData && (
        <ExportPanel
          data={exportData}
          erasing={erasing}
          onErase={runErase}
        />
      )}

      {erasure && <ErasureSummary result={erasure} />}
    </section>
  );
}

function ExportPanel({
  data,
  erasing,
  onErase,
}: {
  data: CompanyGdpr;
  erasing: boolean;
  onErase: () => void;
}) {
  const { records } = data.export;
  const underRetention = records.filter((r) => r.underRetention).length;
  const erasable = records.filter(
    (r) => !r.underRetention && !r.erased,
  ).length;
  const alreadyErased = records.filter((r) => r.erased).length;

  return (
    <section className="card">
      <h3>
        Indsigtsrapport ({records.length} række
        {records.length === 1 ? "" : "r"})
      </h3>
      <p className="muted">
        Hentet pr. {data.export.asOf}. Under bogføringspligt:{" "}
        {underRetention}. Allerede anonymiseret: {alreadyErased}. Kan
        anonymiseres nu: {erasable}.
      </p>
      {records.length === 0 ? (
        <p className="muted">
          Ingen personoplysninger fundet for det angivne subject.
        </p>
      ) : (
        <>
          <table className="table">
            <thead>
              <tr>
                <th>Kilde</th>
                <th>Navn</th>
                <th>CVR</th>
                <th>Email</th>
                <th>Adresse</th>
                <th>Opbevares til</th>
                <th>Status</th>
              </tr>
            </thead>
            <tbody>
              {records.map((r, i) => (
                <RecordRow key={`${r.source}-${r.sourceRowId}-${i}`} row={r} />
              ))}
            </tbody>
          </table>
          <div className="row-actions">
            <button
              type="button"
              className="btn danger"
              onClick={onErase}
              disabled={erasing || erasable === 0}
            >
              {erasing
                ? "Anonymiserer …"
                : `Anonymisér de ${erasable} mulige rækker`}
            </button>
          </div>
        </>
      )}
    </section>
  );
}

function RecordRow({ row }: { row: GdprExportRecord }) {
  return (
    <tr>
      <td>{SOURCE_LABEL[row.source] ?? row.source}</td>
      <td>{row.personalData.name ?? "—"}</td>
      <td>{row.personalData.vatOrCvr ?? "—"}</td>
      <td>{row.personalData.email ?? "—"}</td>
      <td className="muted">{row.personalData.address ?? "—"}</td>
      <td className="muted">{row.retainUntil ?? "—"}</td>
      <td>
        {row.erased ? (
          <span className="pill">Anonymiseret</span>
        ) : row.underRetention ? (
          <span className="pill warn">Under bogføringspligt</span>
        ) : (
          <span className="pill ok">Kan anonymiseres</span>
        )}
      </td>
    </tr>
  );
}

function ErasureSummary({ result }: { result: GdprErasureResult }) {
  return (
    <section className="card">
      <h3>Anonymisering — resultat</h3>
      <p className="muted">
        Pr. {result.asOf}. Anonymiseret: {result.erasedCount}. Allerede
        anonymiseret: {result.alreadyErasedCount}. Afvist (under bogføringspligt):{" "}
        {result.refusedCount}.
      </p>
      {result.refused.length > 0 && (
        <>
          <h4>Afviste rækker</h4>
          <table className="table">
            <thead>
              <tr>
                <th>Kilde</th>
                <th>Reference</th>
                <th>Opbevares til</th>
                <th>Grund</th>
              </tr>
            </thead>
            <tbody>
              {result.refused.map((r, i) => (
                <tr key={i}>
                  <td>{SOURCE_LABEL[r.source] ?? r.source}</td>
                  <td>{r.label ?? `#${r.sourceRowId}`}</td>
                  <td className="muted">{r.retainUntil}</td>
                  <td>{r.reason}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </>
      )}
    </section>
  );
}
