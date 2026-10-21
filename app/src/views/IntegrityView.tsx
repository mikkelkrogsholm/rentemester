import { ButtonLink, Button, PageHeader } from "../components/ui";
// Integritet & backup-panel (#333).
//
// Per-virksomhed read-only panel der viser SMB-ejeren hash-kædens status,
// backup-compliance (hvornår sidst, om der er ny aktivitet siden, om der
// snart er forfald) og hvilke backup-destinationer der er konfigureret.
// Genbruger eksisterende kerne-helpers via /api/companies/:slug/integrity —
// ingen genimplementering på cockpit-siden.

import { Link, useParams } from "react-router-dom";
import { api } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import type { CompanyIntegrity } from "../lib/types";
import { ErrorState, Loading } from "../components/Feedback";
import { useOptionalAuth } from "../lib/auth-context";
import { NativeBackupCard } from "../components/NativeBackupCard";
import { NativeBackupGovernance } from "../components/NativeBackupGovernance";
import { NativeBackupStatus } from "../components/NativeBackupStatus";

export function IntegrityView() {
  const auth = useOptionalAuth();
  const { slug = "" } = useParams();
  const state = useAsync<CompanyIntegrity>(
    (signal) => api.integrity(slug, { signal }),
    [slug],
  );

  if (state.loading) return <Loading />;
  if (state.error) return <ErrorState message={state.error} onRetry={state.reload} />;
  const data = state.data!;

  const chainAvailable = data.auditChain.available !== false && data.auditChain.ok !== null;
  const chainOk = data.auditChain.ok === true;
  const chainFailed = chainAvailable && data.auditChain.ok === false;
  const backupAvailable = data.backup.available !== false && data.backup.ok !== null && data.backup.backupDue !== null;
  const backupOk = data.backup.ok === true && data.backup.backupDue === false;
  const destinationsAvailable = data.destinationsAvailable !== false;

  return (
    <section className="integrity-view">
      <PageHeader title="Integritet" actions={<><div className="row-actions">
          <Button variant="secondary"
            type="button"
            className="btn secondary"
            onClick={state.reload}
          >
            Verificér igen
          </Button>
          <ButtonLink className="btn secondary" to={`/companies/${slug}/manage`}>
            Administrér
          </ButtonLink>
        </div></>}>
        <div>

          <p className="muted">
            {data.company.cvr ? `CVR ${data.company.cvr} · ` : ""}
            {data.company.country} · Integritet &amp; backup
          </p>
        </div>

      </PageHeader>

      {auth?.backend === "rust" && <><NativeBackupCard /><NativeBackupStatus key={slug} slug={slug} /><NativeBackupGovernance key={`governance:${slug}`} slug={slug} /></>}

      {chainFailed && (
        <div className="callout danger">
          <strong>Hashkontrollen fandt afvigelser.</strong> Kædens gemte værdier
          stemmer ikke med de kontrollerede data. {data.auditChain.errors.length} afvigelse
          {data.auditChain.errors.length === 1 ? "" : "r"} fundet. Undersøg
          afvigelserne, før resultatet bruges som dokumentation.
        </div>
      )}
      {!chainAvailable && (
        <div className="callout warn"><strong>Hashkontrol utilgængelig.</strong> {data.auditChain.unavailableReason ?? "Kontrollen kunne ikke gennemføres."}</div>
      )}
      {!backupAvailable && (
        <div className="callout warn"><strong>Backupstatus kan ikke fastslås.</strong> {data.backup.unavailableReason ?? "Backup-beviser er ikke verificeret."}</div>
      )}
      {backupAvailable && !backupOk && data.backup.backupDue === true && (
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
          Hver bogføringspost har et SHA-256-fingeraftryk, der bindes til den
          forrige post. Kontrollen sammenligner de gemte værdier med dataenes
          indhold. En hel kæde er ikke i sig selv bevis for fuld revisionsintegritet.
          Kontrollen læser data uden at ændre dem.
        </p>
        {data.auditChain.scopeNote && <p className="muted">{data.auditChain.scopeNote}</p>}
        <div className="table-scroll"><table className="table">
          <tbody>
            <tr>
              <th>Status</th>
              <td className={chainOk ? "ok" : "warn"}>
                {!chainAvailable ? "Kan ikke verificeres" : chainOk ? "OK — den kontrollerede kæde er hel" : "FEJL — afvigelser i kæden"}
              </td>
            </tr>
            <tr>
              <th>Antal posteringer</th>
              <td>{data.auditChain.entries ?? "Ukendt"}</td>
            </tr>
          </tbody>
        </table></div>
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
        <div className="table-scroll"><table className="table">
          <tbody>
            <tr>
              <th>Seneste backup</th>
              <td>{data.backup.latestBackupAt ?? (backupAvailable ? "Ingen backup endnu" : "Ukendt")}</td>
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
              <td>{data.backup.backupsFound ?? "Ukendt"}</td>
            </tr>
            <tr>
              <th>Forfalden</th>
              <td className={data.backup.backupDue === null ? "muted" : data.backup.backupDue ? "warn" : "ok"}>
                {data.backup.backupDue === null ? "Ukendt" : data.backup.backupDue ? "Ja — lav en backup nu" : "Nej"}
              </td>
            </tr>
            <tr>
              <th>Aktivitet siden sidste backup</th>
              <td>{data.backup.hasActivitySinceBackup === null ? "Ukendt" : data.backup.hasActivitySinceBackup ? "Ja" : "Nej"}</td>
            </tr>
            <tr>
              <th>Dage siden sidste backup</th>
              <td>{data.backup.daysSinceLatestBackup ?? "—"}</td>
            </tr>
            <tr>
              <th>Verificeret pr.</th>
              <td>{data.backup.checkedAt ?? "Ikke verificeret"}</td>
            </tr>
          </tbody>
        </table></div>
      </section>

      <section className="card">
        <h3>Backup-destinationer{destinationsAvailable ? ` (${data.destinations.length})` : ""}</h3>
        {!destinationsAvailable ? (
          <p className="muted">{data.destinationsUnavailableReason ?? "Backup-destinationer er ikke kontrolleret."}</p>
        ) : data.destinations.length === 0 ? (
          <p className="muted">
            Ingen destinationer konfigureret. Brug{" "}
            <code>rentemester system backup-add-destination</code> for at
            registrere en EU/EØS-host (bogføringsloven § 15, stk. 1).
          </p>
        ) : (
          <div className="table-scroll"><table className="table">
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
          </table></div>
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
