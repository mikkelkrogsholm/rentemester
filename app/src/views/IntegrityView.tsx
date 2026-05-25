// Integritet & backup-panel (#333).
//
// Per-virksomhed read-only panel der viser SMB-ejeren hash-kædens status,
// backup-compliance (hvornår sidst, om der er ny aktivitet siden, om der
// snart er forfald) og hvilke backup-destinationer der er konfigureret.
// Genbruger eksisterende kerne-helpers via /api/companies/:slug/integrity —
// ingen genimplementering på cockpit-siden.

import { useState } from "react";
import { Link, useParams } from "react-router-dom";
import { api } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import type { CompanyIntegrity } from "../lib/types";
import { ErrorState, Loading } from "../components/Feedback";

export function IntegrityView() {
  const { slug = "" } = useParams();
  // En `refresh`-tæller lader brugeren trykke "Verificér igen" og få cockpittet
  // til at re-fetche /integrity. Endpointet er idempotent, så det er sikkert.
  const [refresh, setRefresh] = useState(0);
  const state = useAsync<CompanyIntegrity>(
    () => api.integrity(slug),
    [slug, refresh],
  );

  if (state.loading) return <Loading />;
  if (state.error) return <ErrorState message={state.error} />;
  const data = state.data!;

  const chainOk = data.auditChain.ok;
  const backupOk = data.backup.ok && !data.backup.backupDue;

  return (
    <section className="integrity-view">
      <header className="page-head">
        <div>
          <h2>{data.company.name}</h2>
          <p className="muted">
            {data.company.cvr ? `CVR ${data.company.cvr} · ` : ""}
            {data.company.country} · Integritet &amp; backup
          </p>
        </div>
        <div className="row-actions">
          <button
            type="button"
            className="btn secondary"
            onClick={() => setRefresh((n) => n + 1)}
          >
            Verificér igen
          </button>
          <Link className="btn secondary" to={`/companies/${slug}/manage`}>
            Administrér
          </Link>
        </div>
      </header>

      {!chainOk && (
        <div className="callout danger">
          <strong>Revisionskæden er brudt.</strong> Nogen har ændret bogføringen
          efter den blev godkendt. {data.auditChain.errors.length} afvigelse
          {data.auditChain.errors.length === 1 ? "" : "r"} fundet. Bogfør ikke
          videre — kontakt din revisor og overvej at genskabe fra seneste
          verificerede backup.
        </div>
      )}
      {!backupOk && (
        <div className="callout warn">
          <strong>Backup forfalden.</strong> Seneste backup{" "}
          {data.backup.latestBackupAt
            ? `er fra ${data.backup.latestBackupAt}`
            : "findes ikke endnu"}
          . Bogføringsloven kræver at materialet opbevares forsvarligt — lav en
          backup snarest.
        </div>
      )}

      <section className="card">
        <h3>Hash-kæden (digital plombe)</h3>
        <p>
          Hver bogføringspost har et SHA-256-fingeraftryk der bindes til den
          forrige post. Hvis kæden er hel, kan ingen ændre en bogføring uden at
          det opdages. Verificeres på hvert kald — det er sikkert at trykke
          "Verificér igen".
        </p>
        <table className="table">
          <tbody>
            <tr>
              <th>Status</th>
              <td className={chainOk ? "ok" : "warn"}>
                {chainOk ? "PASS — kæden er hel" : "FAIL — kæden er brudt"}
              </td>
            </tr>
            <tr>
              <th>Antal posteringer</th>
              <td>{data.auditChain.entries}</td>
            </tr>
          </tbody>
        </table>
        {data.auditChain.errors.length > 0 && (
          <details open>
            <summary>
              {data.auditChain.errors.length} afvigelse
              {data.auditChain.errors.length === 1 ? "" : "r"}
            </summary>
            <ul className="audit-errors">
              {data.auditChain.errors.map((err, i) => (
                <li key={i}>
                  <code>{err}</code>
                </li>
              ))}
            </ul>
          </details>
        )}
      </section>

      <section className="card">
        <h3>Backup-status</h3>
        <table className="table">
          <tbody>
            <tr>
              <th>Seneste backup</th>
              <td>{data.backup.latestBackupAt ?? "Ingen backup endnu"}</td>
            </tr>
            <tr>
              <th>Backup ID</th>
              <td>
                {data.backup.latestBackupId ? (
                  <code>{data.backup.latestBackupId}</code>
                ) : (
                  "—"
                )}
              </td>
            </tr>
            <tr>
              <th>Antal backups</th>
              <td>{data.backup.backupsFound}</td>
            </tr>
            <tr>
              <th>Forfalden</th>
              <td className={data.backup.backupDue ? "warn" : "ok"}>
                {data.backup.backupDue ? "Ja — lav en backup nu" : "Nej"}
              </td>
            </tr>
            <tr>
              <th>Aktivitet siden sidste backup</th>
              <td>{data.backup.hasActivitySinceBackup ? "Ja" : "Nej"}</td>
            </tr>
            <tr>
              <th>Dage siden sidste backup</th>
              <td>{data.backup.daysSinceLatestBackup ?? "—"}</td>
            </tr>
            <tr>
              <th>Verificeret pr.</th>
              <td>{data.backup.checkedAt}</td>
            </tr>
          </tbody>
        </table>
      </section>

      <section className="card">
        <h3>Backup-destinationer ({data.destinations.length})</h3>
        {data.destinations.length === 0 ? (
          <p className="muted">
            Ingen destinationer konfigureret. Brug{" "}
            <code>rentemester system backup-add-destination</code> for at
            registrere en EU/EØS-host (bogføringsloven § 15, stk. 1).
          </p>
        ) : (
          <table className="table">
            <thead>
              <tr>
                <th>Label</th>
                <th>Type</th>
                <th>Placering</th>
                <th>EU/EØS</th>
                <th>3.-part</th>
                <th>Senest brugt</th>
              </tr>
            </thead>
            <tbody>
              {data.destinations.map((d) => (
                <tr key={d.id}>
                  <td>{d.label}</td>
                  <td>{d.kind}</td>
                  <td>{d.location}</td>
                  <td className={d.inEeaOrEu ? "ok" : "warn"}>
                    {d.inEeaOrEu ? `Ja${d.country ? ` (${d.country})` : ""}` : "Nej"}
                  </td>
                  <td>{d.nonRelatedParty ? "Ja" : "Nej"}</td>
                  <td>{d.lastPlacementAt ?? "—"}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>

      <section className="card">
        <h3>Lovgrundlag</h3>
        <p>
          {data.legalCitation.note}{" "}
          <Link to={`/lovgrundlag#${data.legalCitation.sourceId}`}>
            Se {data.legalCitation.sourceId} i Lovgrundlag-visningen
          </Link>
          .
        </p>
      </section>
    </section>
  );
}
