import { cockpitStyles } from "../design/cockpit.stylex";
import * as stylex from "@stylexjs/stylex";
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
import { NativeAuditVerify } from "../components/NativeAuditVerify";
import { NativeAuditLog } from "../components/NativeAuditLog";
import { NativeBackupCard } from "../components/NativeBackupCard";
import { NativeBackupRegistrations } from "../components/NativeBackupRegistrations";
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
    <section {...stylex.props(...[cockpitStyles.statement])}>
      <PageHeader title="Integritet" actions={<><div {...stylex.props(cockpitStyles.rowActions)}>
          <Button variant="secondary"
            type="button"

            onClick={state.reload} xstyle={[cockpitStyles.integrityViewBtnComposition]}
          >
            Verificér igen
          </Button>
          <ButtonLink  to={`/companies/${slug}/manage`}>
            Administrér
          </ButtonLink>
        </div></>}>
        <div {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>

          <p {...stylex.props(cockpitStyles.muted)}>
            {data.company.cvr ? `CVR ${data.company.cvr} · ` : ""}
            {data.company.country} · Integritet &amp; backup
          </p>
        </div>

      </PageHeader>

      {auth?.backend === "rust" && <><NativeAuditVerify key={`verify:${slug}`} slug={slug} /><NativeAuditLog key={`audit:${slug}`} slug={slug} /><NativeBackupCard /><NativeBackupStatus key={slug} slug={slug} /><NativeBackupGovernance key={`governance:${slug}`} slug={slug} /><NativeBackupRegistrations key={`registrations:${slug}`} slug={slug} /></>}

      {chainFailed && (
        <div {...stylex.props(...[cockpitStyles.banner, cockpitStyles.bannerError])}>
          <strong>Hashkontrollen fandt afvigelser.</strong> Kædens gemte værdier
          stemmer ikke med de kontrollerede data. {data.auditChain.errors.length} afvigelse
          {data.auditChain.errors.length === 1 ? "" : "r"} fundet. Undersøg
          afvigelserne, før resultatet bruges som dokumentation.
        </div>
      )}
      {!chainAvailable && (
        <div {...stylex.props(...[cockpitStyles.banner, cockpitStyles.statusChipWarning])}><strong {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>Hashkontrol utilgængelig.</strong> {data.auditChain.unavailableReason ?? "Kontrollen kunne ikke gennemføres."}</div>
      )}
      {!backupAvailable && (
        <div {...stylex.props(...[cockpitStyles.banner, cockpitStyles.statusChipWarning])}><strong {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>Backupstatus kan ikke fastslås.</strong> {data.backup.unavailableReason ?? "Backup-beviser er ikke verificeret."}</div>
      )}
      {backupAvailable && !backupOk && data.backup.backupDue === true && (
        <div {...stylex.props(...[cockpitStyles.banner, cockpitStyles.statusChipWarning])}>
          <strong {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>Backup forfalden.</strong> Seneste backup{" "}
          {data.backup.latestBackupAt
            ? `er fra ${data.backup.latestBackupAt}`
            : "findes ikke endnu"}
          . Bogføringsloven kræver at materialet opbevares forsvarligt — lav en
          backup snarest.
        </div>
      )}

      <section {...stylex.props(cockpitStyles.card)}>
        <h3 {...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.h3,
					)}>Hash-kæden (digital plombe)</h3>
        <p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
          Hver bogføringspost har et SHA-256-fingeraftryk, der bindes til den
          forrige post. Kontrollen sammenligner de gemte værdier med dataenes
          indhold. En hel kæde er ikke i sig selv bevis for fuld revisionsintegritet.
          Kontrollen læser data uden at ændre dem.
        </p>
        {data.auditChain.scopeNote && <p {...stylex.props(cockpitStyles.muted)}>{data.auditChain.scopeNote}</p>}
        <div {...stylex.props(cockpitStyles.tableScroll)}><table {...stylex.props(...[cockpitStyles.tableData])}>
          <tbody {...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}>
            <tr {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>
              <th {...stylex.props(
										cockpitStyles.tableDataTh,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.integrityViewTableTableThLastChild,
									)}>Status</th>
              <td {...stylex.props(...(chainOk ? [cockpitStyles.statusChipSuccess] : [cockpitStyles.statusChipWarning]))}>
                {!chainAvailable ? "Kan ikke verificeres" : chainOk ? "OK — den kontrollerede kæde er hel" : "FEJL — afvigelser i kæden"}
              </td>
            </tr>
            <tr {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>
              <th {...stylex.props(
										cockpitStyles.tableDataTh,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.integrityViewTableTableThLastChild,
									)}>Antal posteringer</th>
              <td {...stylex.props(
										cockpitStyles.tableDataTd,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.integrityViewTableTableTdLastChild,
									)}>{data.auditChain.entries ?? "Ukendt"}</td>
            </tr>
          </tbody>
        </table></div>
        {data.auditChain.errors.length > 0 && (
          <details open>
            <summary {...stylex.props(cockpitStyles.summaryComposition)}>
              {data.auditChain.errors.length} afvigelse
              {data.auditChain.errors.length === 1 ? "" : "r"}
            </summary>
            <ul {...stylex.props(...[cockpitStyles.bannerError])}>
              {data.auditChain.errors.map((err, i) => (
                <li key={i} {...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}>
                  <code {...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
												cockpitStyles.code,
											)}>{err}</code>
                </li>
              ))}
            </ul>
          </details>
        )}
      </section>

      <section {...stylex.props(cockpitStyles.card)}>
        <h3 {...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.h3,
					)}>Backup-status</h3>
        <div {...stylex.props(cockpitStyles.tableScroll)}><table {...stylex.props(...[cockpitStyles.tableData])}>
          <tbody {...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}>
            <tr {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>
              <th {...stylex.props(
										cockpitStyles.tableDataTh,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.integrityViewTableTableThLastChild,
									)}>Seneste backup</th>
              <td {...stylex.props(
										cockpitStyles.tableDataTd,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.integrityViewTableTableTdLastChild,
									)}>{data.backup.latestBackupAt ?? (backupAvailable ? "Ingen backup endnu" : "Ukendt")}</td>
            </tr>
            <tr {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>
              <th {...stylex.props(
										cockpitStyles.tableDataTh,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.integrityViewTableTableThLastChild,
									)}>Backup ID</th>
              <td {...stylex.props(
										cockpitStyles.tableDataTd,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.integrityViewTableTableTdLastChild,
									)}>
                {data.backup.latestBackupId ? (
                  <code {...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
												cockpitStyles.code,
											)}>{data.backup.latestBackupId}</code>
                ) : (
                  "—"
                )}
              </td>
            </tr>
            <tr {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>
              <th {...stylex.props(
										cockpitStyles.tableDataTh,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.integrityViewTableTableThLastChild,
									)}>Antal backups</th>
              <td {...stylex.props(
										cockpitStyles.tableDataTd,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.integrityViewTableTableTdLastChild,
									)}>{data.backup.backupsFound ?? "Ukendt"}</td>
            </tr>
            <tr {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>
              <th {...stylex.props(
										cockpitStyles.tableDataTh,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.integrityViewTableTableThLastChild,
									)}>Forfalden</th>
              <td {...stylex.props(...(data.backup.backupDue === null ? [cockpitStyles.muted] : (data.backup.backupDue ? [cockpitStyles.statusChipWarning] : [cockpitStyles.statusChipSuccess])))}>
                {data.backup.backupDue === null ? "Ukendt" : data.backup.backupDue ? "Ja — lav en backup nu" : "Nej"}
              </td>
            </tr>
            <tr {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>
              <th {...stylex.props(
										cockpitStyles.tableDataTh,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.integrityViewTableTableThLastChild,
									)}>Aktivitet siden sidste backup</th>
              <td {...stylex.props(
										cockpitStyles.tableDataTd,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.integrityViewTableTableTdLastChild,
									)}>{data.backup.hasActivitySinceBackup === null ? "Ukendt" : data.backup.hasActivitySinceBackup ? "Ja" : "Nej"}</td>
            </tr>
            <tr {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>
              <th {...stylex.props(
										cockpitStyles.tableDataTh,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.integrityViewTableTableThLastChild,
									)}>Dage siden sidste backup</th>
              <td {...stylex.props(
										cockpitStyles.tableDataTd,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.integrityViewTableTableTdLastChild,
									)}>{data.backup.daysSinceLatestBackup ?? "—"}</td>
            </tr>
            <tr {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>
              <th {...stylex.props(
										cockpitStyles.tableDataTh,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.integrityViewTableTableThLastChild,
									)}>Verificeret pr.</th>
              <td {...stylex.props(
										cockpitStyles.tableDataTd,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.integrityViewTableTableTdLastChild,
									)}>{data.backup.checkedAt ?? "Ikke verificeret"}</td>
            </tr>
          </tbody>
        </table></div>
      </section>

      <section {...stylex.props(cockpitStyles.card)}>
        <h3 {...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.h3,
					)}>Backup-destinationer{destinationsAvailable ? ` (${data.destinations.length})` : ""}</h3>
        {!destinationsAvailable ? (
          <p {...stylex.props(cockpitStyles.muted)}>{data.destinationsUnavailableReason ?? "Backup-destinationer er ikke kontrolleret."}</p>
        ) : data.destinations.length === 0 ? (
          <p {...stylex.props(cockpitStyles.muted)}>
            Ingen destinationer konfigureret. Brug{" "}
            <code {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.code,
								)}>rentemester system backup-add-destination</code> for at
            registrere en EU/EØS-host (bogføringsloven § 15, stk. 1).
          </p>
        ) : (
          <div {...stylex.props(cockpitStyles.tableScroll)}><table {...stylex.props(...[cockpitStyles.tableData])}>
            <thead {...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}>
              <tr {...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}>
                <th {...stylex.props(
												cockpitStyles.tableDataTh,
												cockpitStyles.element,
												cockpitStyles.focusVisible,
												cockpitStyles.integrityViewTableTableThLastChild,
											)}>Label</th>
                <th {...stylex.props(
												cockpitStyles.tableDataTh,
												cockpitStyles.element,
												cockpitStyles.focusVisible,
												cockpitStyles.integrityViewTableTableThLastChild,
											)}>Type</th>
                <th {...stylex.props(
												cockpitStyles.tableDataTh,
												cockpitStyles.element,
												cockpitStyles.focusVisible,
												cockpitStyles.integrityViewTableTableThLastChild,
											)}>Placering</th>
                <th {...stylex.props(
												cockpitStyles.tableDataTh,
												cockpitStyles.element,
												cockpitStyles.focusVisible,
												cockpitStyles.integrityViewTableTableThLastChild,
											)}>EU/EØS</th>
                <th {...stylex.props(
												cockpitStyles.tableDataTh,
												cockpitStyles.element,
												cockpitStyles.focusVisible,
												cockpitStyles.integrityViewTableTableThLastChild,
											)}>3.-part</th>
                <th {...stylex.props(
												cockpitStyles.tableDataTh,
												cockpitStyles.element,
												cockpitStyles.focusVisible,
												cockpitStyles.integrityViewTableTableThLastChild,
											)}>Senest brugt</th>
              </tr>
            </thead>
            <tbody {...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}>
              {data.destinations.map((d) => (
                <tr key={d.id} {...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}>
                  <td {...stylex.props(
													cockpitStyles.tableDataTd,
													cockpitStyles.element,
													cockpitStyles.focusVisible,
													cockpitStyles.integrityViewTableTableTdLastChild,
												)}>{d.label}</td>
                  <td {...stylex.props(
													cockpitStyles.tableDataTd,
													cockpitStyles.element,
													cockpitStyles.focusVisible,
													cockpitStyles.integrityViewTableTableTdLastChild,
												)}>{d.kind}</td>
                  <td {...stylex.props(
													cockpitStyles.tableDataTd,
													cockpitStyles.element,
													cockpitStyles.focusVisible,
													cockpitStyles.integrityViewTableTableTdLastChild,
												)}>{d.location}</td>
                  <td {...stylex.props(...(d.inEeaOrEu ? [cockpitStyles.statusChipSuccess] : [cockpitStyles.statusChipWarning]))}>
                    {d.inEeaOrEu ? `Ja${d.country ? ` (${d.country})` : ""}` : "Nej"}
                  </td>
                  <td {...stylex.props(
													cockpitStyles.tableDataTd,
													cockpitStyles.element,
													cockpitStyles.focusVisible,
													cockpitStyles.integrityViewTableTableTdLastChild,
												)}>{d.nonRelatedParty ? "Ja" : "Nej"}</td>
                  <td {...stylex.props(
													cockpitStyles.tableDataTd,
													cockpitStyles.element,
													cockpitStyles.focusVisible,
													cockpitStyles.integrityViewTableTableTdLastChild,
												)}>{d.lastPlacementAt ?? "—"}</td>
                </tr>
              ))}
            </tbody>
          </table></div>
        )}
      </section>

      <section {...stylex.props(cockpitStyles.card)}>
        <h3 {...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.h3,
					)}>Lovgrundlag</h3>
        <p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
          {data.legalCitation.note}{" "}
          <Link to={`/lovgrundlag#${data.legalCitation.sourceId}`} {...stylex.props(cockpitStyles.aComposition)}>
            Se {data.legalCitation.sourceId} i Lovgrundlag-visningen
          </Link>
          .
        </p>
      </section>
    </section>
  );
}
