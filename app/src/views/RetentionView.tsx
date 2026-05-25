// Retention status view (#343) — per-virksomhed read-only side der viser
// hvor langt 5-års bogføringspligten er kommet for hver data-domæne. Bygger
// på det eksisterende `buildRetentionStatusReport` i kernen (ingen
// genimplementering) og deep-linker til Lovgrundlag-viewet (#347) for at
// citere bogføringslovens § 12, stk. 1.

import { Link, useParams } from "react-router-dom";
import { api } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import type { CompanyRetention } from "../lib/types";
import { ErrorState, Loading } from "../components/Feedback";

const TABLE_LABELS: Record<string, string> = {
  documents: "Bilag",
  journal_entries: "Posteringer",
  bank_transactions: "Banktransaktioner",
};

export function RetentionView() {
  const { slug = "" } = useParams();
  const state = useAsync<CompanyRetention>(() => api.retention(slug), [slug]);

  if (state.loading) return <Loading />;
  if (state.error) return <ErrorState message={state.error} />;
  const r = state.data!;

  const totalExpired = r.report.rows.reduce((acc, row) => acc + row.expired, 0);

  return (
    <section className="retention-view">
      <header className="page-head">
        <div>
          <h2>{r.company.name}</h2>
          <p className="muted">
            {r.company.cvr ? `CVR ${r.company.cvr} · ` : ""}
            {r.company.country} · Retention (5-års bogføringspligt)
          </p>
        </div>
        <div className="row-actions">
          <Link className="btn secondary" to={`/companies/${slug}/manage`}>
            Administrér
          </Link>
        </div>
      </header>

      <p className="muted">
        Pr. {r.report.asOf}. Hver data-domæne skal opbevares i 5 år efter udløb
        af det regnskabsår posten vedrører. Tabellen viser hvor mange poster der
        ligger udløbet (klar til{" "}
        <Link to={`/companies/${slug}/manage`}>GDPR-anonymisering</Link>) og
        hvornår næste post udløber.
      </p>

      {totalExpired > 0 && (
        <div className="callout warn">
          {totalExpired} post{totalExpired === 1 ? "" : "er"} har overskredet
          den 5-årige opbevaringspligt og kan anonymiseres.
        </div>
      )}

      <table className="table">
        <thead>
          <tr>
            <th>Domæne</th>
            <th>Antal poster</th>
            <th>Udløbet</th>
            <th>Næste udløb</th>
            <th>Ældste udløbet</th>
          </tr>
        </thead>
        <tbody>
          {r.report.rows.map((row) => (
            <tr key={row.table}>
              <td>{TABLE_LABELS[row.table] ?? row.table}</td>
              <td>{row.total}</td>
              <td className={row.expired > 0 ? "warn" : ""}>{row.expired}</td>
              <td>{row.nextExpiry ?? "—"}</td>
              <td>{row.oldestExpired ?? "—"}</td>
            </tr>
          ))}
        </tbody>
      </table>

      <section className="card">
        <h3>Lovgrundlag</h3>
        <p>
          {r.legalCitation.note}{" "}
          <Link to={`/lovgrundlag#${r.legalCitation.sourceId}`}>
            Se {r.legalCitation.sourceId} i Lovgrundlag-visningen
          </Link>
          .
        </p>
        <p className="muted">
          Anvendte regler: {r.report.appliedRules.join(", ")}
        </p>
      </section>
    </section>
  );
}
