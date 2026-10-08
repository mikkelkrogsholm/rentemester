import * as stylex from "@stylexjs/stylex";
import { Button, ButtonLink, PageHeader } from "../components/ui";
import { cockpitStyles } from "../design/cockpit.stylex";
// Integritet & backup-panel (#333).
//
// Per-virksomhed read-only panel der viser SMB-ejeren hash-kædens status,
// backup-compliance (hvornår sidst, om der er ny aktivitet siden, om der
// snart er forfald) og hvilke backup-destinationer der er konfigureret.
// Genbruger eksisterende kerne-helpers via /api/companies/:slug/integrity —
// ingen genimplementering på cockpit-siden.

import { Link, useParams } from "react-router-dom";
import { ErrorState, Loading } from "../components/Feedback";
import { api } from "../lib/api";
import type { CompanyIntegrity } from "../lib/types";
import { useAsync } from "../lib/useAsync";

export function IntegrityView() {
	const { slug = "" } = useParams();
	const state = useAsync<CompanyIntegrity>(
		(signal) => api.integrity(slug, { signal }),
		[slug],
	);

	if (state.loading && !state.data) return <Loading />;
	if (state.error && !state.data)
		return <ErrorState message={state.error} onRetry={state.reload} />;
	const data = state.data!;

	const chainOk = data.auditChain.ok;
	const backupOk = data.backup.ok && !data.backup.backupDue;

	return (
		<section
			data-cockpit-page="integrity"
			data-evidence-issue="655"
			{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
		>
			<PageHeader
				title="Integritet"
				actions={
					<>
						<div
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.rowActions,
							)}
						>
							<Button
								variant="secondary"
								type="button"
								onClick={state.reload}
								xstyle={[cockpitStyles.integrityViewBtnComposition]}
							>
								Verificér igen
							</Button>
							<ButtonLink
								to={`/companies/${slug}/manage`}
								variant={"secondary"}
								xstyle={[cockpitStyles.integrityViewBtnComposition2]}
							>
								Administrér
							</ButtonLink>
						</div>
					</>
				}
			>
				<div
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}
					>
						{data.company.cvr ? `CVR ${data.company.cvr} · ` : ""}
						{data.company.country} · Integritet &amp; backup
					</p>
				</div>
			</PageHeader>

			{state.loading && (
				<p
					role="status"
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.muted,
					)}
				>
					Kontrollen opdateres. Den seneste gennemførte kontrol vises fortsat.
				</p>
			)}
			{state.error && (
				<div
					role="alert"
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					<strong
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Kontrollen kunne ikke opdateres.
					</strong>{" "}
					{state.error} Den seneste gennemførte kontrol vises fortsat. Vælg
					Verificér igen for at prøve igen.
				</div>
			)}

			{!chainOk && (
				<div
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					<strong
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Revisionskæden er brudt.
					</strong>{" "}
					Nogen har ændret bogføringen efter den blev godkendt.{" "}
					{data.auditChain.errors.length} afvigelse
					{data.auditChain.errors.length === 1 ? "" : "r"} fundet. Bogfør ikke
					videre — kontakt din revisor og overvej at genskabe fra seneste
					verificerede backup.
				</div>
			)}
			{!backupOk && (
				<div
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					<strong
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Backup forfalden.
					</strong>{" "}
					Seneste backup{" "}
					{data.backup.latestBackupAt
						? `er fra ${data.backup.latestBackupAt}`
						: "findes ikke endnu"}
					. Bogføringsloven kræver at materialet opbevares forsvarligt — lav en
					backup snarest.
				</div>
			)}

			<section
				aria-label="Konklusion"
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.card,
				)}
			>
				<h3
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.h3,
					)}
				>
					{chainOk && backupOk
						? "Bogføringen og backup ser sunde ud"
						: "Der kræves handling"}
				</h3>
				<p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
					{chainOk
						? "Bogføringen er kontrolleret uden fundne ændringer."
						: "Bogføringen må ikke bruges videre, før afvigelsen er undersøgt."}
					{backupOk
						? " Backup er inden for den valgte rytme."
						: " Backup skal gennemgås nu."}
				</p>
				<p
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.muted,
					)}
				>
					Senest kontrolleret: {data.backup.checkedAt.slice(0, 10)}. Næste
					skridt:{" "}
					{chainOk && backupOk
						? "Verificér igen efter væsentlige ændringer."
						: "Følg advarslen ovenfor og verificér igen."}
				</p>
			</section>

			<section
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.card,
				)}
			>
				<h3
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.h3,
					)}
				>
					Kontrol af bogføringen
				</h3>
				<p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
					Hver bogføringspost har et SHA-256-fingeraftryk der bindes til den
					forrige post. Hvis kæden er hel, kan ingen ændre en bogføring uden at
					det opdages. Verificeres på hvert kald — det er sikkert at trykke
					"Verificér igen".
				</p>
				<details
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					<summary {...stylex.props(cockpitStyles.summaryComposition)}>
						Se tekniske kontroloplysninger
					</summary>
					<table
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.integrityViewTableTable,
						)}
					>
						<tbody
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							<tr
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								<th
									{...stylex.props(
										cockpitStyles.tableDataTh,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.integrityViewTableTableThLastChild,
									)}
								>
									Status
								</th>
								<td
									{...stylex.props(
										cockpitStyles.tableDataTd,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.integrityViewTableTableTdLastChild,
									)}
								>
									{chainOk ? "OK — kæden er hel" : "FEJL — kæden er brudt"}
								</td>
							</tr>
							<tr
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								<th
									{...stylex.props(
										cockpitStyles.tableDataTh,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.integrityViewTableTableThLastChild,
									)}
								>
									Antal posteringer
								</th>
								<td
									{...stylex.props(
										cockpitStyles.tableDataTd,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.integrityViewTableTableTdLastChild,
									)}
								>
									{data.auditChain.entries}
								</td>
							</tr>
						</tbody>
					</table>
					{data.auditChain.errors.length > 0 && (
						<details
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							<summary {...stylex.props(cockpitStyles.summaryComposition)}>
								{data.auditChain.errors.length} afvigelse
								{data.auditChain.errors.length === 1 ? "" : "r"}
							</summary>
							<ul
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								{data.auditChain.errors.map((err, i) => (
									<li
										key={i}
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										<code
											{...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
												cockpitStyles.code,
											)}
										>
											{err}
										</code>
									</li>
								))}
							</ul>
						</details>
					)}
				</details>
			</section>

			<section
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.card,
				)}
			>
				<h3
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.h3,
					)}
				>
					Backup
				</h3>
				<p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
					{backupOk
						? "En backup er registreret, og der er ingen forfalden handling."
						: "Backup kræver opmærksomhed, før du fortsætter normalt arbejde."}
				</p>
				<details
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					<summary {...stylex.props(cockpitStyles.summaryComposition)}>
						Se backupdetaljer
					</summary>
					<table
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.integrityViewTableTable,
						)}
					>
						<tbody
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							<tr
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								<th
									{...stylex.props(
										cockpitStyles.tableDataTh,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.integrityViewTableTableThLastChild,
									)}
								>
									Seneste backup
								</th>
								<td
									{...stylex.props(
										cockpitStyles.tableDataTd,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.integrityViewTableTableTdLastChild,
									)}
								>
									{data.backup.latestBackupAt ?? "Ingen backup endnu"}
								</td>
							</tr>
							<tr
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								<th
									{...stylex.props(
										cockpitStyles.tableDataTh,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.integrityViewTableTableThLastChild,
									)}
								>
									Backup ID
								</th>
								<td
									{...stylex.props(
										cockpitStyles.tableDataTd,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.integrityViewTableTableTdLastChild,
									)}
								>
									{data.backup.latestBackupId ? (
										<code
											{...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
												cockpitStyles.code,
											)}
										>
											{data.backup.latestBackupId}
										</code>
									) : (
										"—"
									)}
								</td>
							</tr>
							<tr
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								<th
									{...stylex.props(
										cockpitStyles.tableDataTh,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.integrityViewTableTableThLastChild,
									)}
								>
									Antal backups
								</th>
								<td
									{...stylex.props(
										cockpitStyles.tableDataTd,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.integrityViewTableTableTdLastChild,
									)}
								>
									{data.backup.backupsFound}
								</td>
							</tr>
							<tr
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								<th
									{...stylex.props(
										cockpitStyles.tableDataTh,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.integrityViewTableTableThLastChild,
									)}
								>
									Forfalden
								</th>
								<td
									{...stylex.props(
										cockpitStyles.tableDataTd,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.integrityViewTableTableTdLastChild,
									)}
								>
									{data.backup.backupDue ? "Ja — lav en backup nu" : "Nej"}
								</td>
							</tr>
							<tr
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								<th
									{...stylex.props(
										cockpitStyles.tableDataTh,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.integrityViewTableTableThLastChild,
									)}
								>
									Aktivitet siden sidste backup
								</th>
								<td
									{...stylex.props(
										cockpitStyles.tableDataTd,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.integrityViewTableTableTdLastChild,
									)}
								>
									{data.backup.hasActivitySinceBackup ? "Ja" : "Nej"}
								</td>
							</tr>
							<tr
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								<th
									{...stylex.props(
										cockpitStyles.tableDataTh,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.integrityViewTableTableThLastChild,
									)}
								>
									Dage siden sidste backup
								</th>
								<td
									{...stylex.props(
										cockpitStyles.tableDataTd,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.integrityViewTableTableTdLastChild,
									)}
								>
									{data.backup.daysSinceLatestBackup ?? "—"}
								</td>
							</tr>
							<tr
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								<th
									{...stylex.props(
										cockpitStyles.tableDataTh,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.integrityViewTableTableThLastChild,
									)}
								>
									Verificeret pr.
								</th>
								<td
									{...stylex.props(
										cockpitStyles.tableDataTd,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.integrityViewTableTableTdLastChild,
									)}
								>
									{data.backup.checkedAt}
								</td>
							</tr>
						</tbody>
					</table>
				</details>
			</section>

			<section
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.card,
				)}
			>
				<h3
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.h3,
					)}
				>
					Backup-destinationer
				</h3>
				<details
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					<summary {...stylex.props(cockpitStyles.summaryComposition)}>
						Se tekniske destinationer ({data.destinations.length})
					</summary>
					{data.destinations.length === 0 ? (
						<p
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.muted,
							)}
						>
							Ingen destinationer konfigureret. Brug{" "}
							<code
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.code,
								)}
							>
								rentemester system backup-add-destination
							</code>{" "}
							for at registrere en EU/EØS-host (bogføringsloven § 15, stk. 1).
						</p>
					) : (
						<div
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.tableScroll,
							)}
							data-ui="table-scroll"
						>
							<table
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.integrityViewTableTable,
								)}
							>
								<thead
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									<tr
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										<th
											{...stylex.props(
												cockpitStyles.tableDataTh,
												cockpitStyles.element,
												cockpitStyles.focusVisible,
												cockpitStyles.integrityViewTableTableThLastChild,
											)}
										>
											Label
										</th>
										<th
											{...stylex.props(
												cockpitStyles.tableDataTh,
												cockpitStyles.element,
												cockpitStyles.focusVisible,
												cockpitStyles.integrityViewTableTableThLastChild,
											)}
										>
											Type
										</th>
										<th
											{...stylex.props(
												cockpitStyles.tableDataTh,
												cockpitStyles.element,
												cockpitStyles.focusVisible,
												cockpitStyles.integrityViewTableTableThLastChild,
											)}
										>
											Placering
										</th>
										<th
											{...stylex.props(
												cockpitStyles.tableDataTh,
												cockpitStyles.element,
												cockpitStyles.focusVisible,
												cockpitStyles.integrityViewTableTableThLastChild,
											)}
										>
											EU/EØS
										</th>
										<th
											{...stylex.props(
												cockpitStyles.tableDataTh,
												cockpitStyles.element,
												cockpitStyles.focusVisible,
												cockpitStyles.integrityViewTableTableThLastChild,
											)}
										>
											3.-part
										</th>
										<th
											{...stylex.props(
												cockpitStyles.tableDataTh,
												cockpitStyles.element,
												cockpitStyles.focusVisible,
												cockpitStyles.integrityViewTableTableThLastChild,
											)}
										>
											Senest brugt
										</th>
									</tr>
								</thead>
								<tbody
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									{data.destinations.map((d) => (
										<tr
											key={d.id}
											{...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}
										>
											<td
												{...stylex.props(
													cockpitStyles.tableDataTd,
													cockpitStyles.element,
													cockpitStyles.focusVisible,
													cockpitStyles.integrityViewTableTableTdLastChild,
												)}
											>
												{d.label}
											</td>
											<td
												{...stylex.props(
													cockpitStyles.tableDataTd,
													cockpitStyles.element,
													cockpitStyles.focusVisible,
													cockpitStyles.integrityViewTableTableTdLastChild,
												)}
											>
												{d.kind}
											</td>
											<td
												{...stylex.props(
													cockpitStyles.tableDataTd,
													cockpitStyles.element,
													cockpitStyles.focusVisible,
													cockpitStyles.integrityViewTableTableTdLastChild,
												)}
											>
												{d.location}
											</td>
											<td
												{...stylex.props(
													cockpitStyles.tableDataTd,
													cockpitStyles.element,
													cockpitStyles.focusVisible,
													cockpitStyles.integrityViewTableTableTdLastChild,
												)}
											>
												{d.inEeaOrEu
													? `Ja${d.country ? ` (${d.country})` : ""}`
													: "Nej"}
											</td>
											<td
												{...stylex.props(
													cockpitStyles.tableDataTd,
													cockpitStyles.element,
													cockpitStyles.focusVisible,
													cockpitStyles.integrityViewTableTableTdLastChild,
												)}
											>
												{d.nonRelatedParty ? "Ja" : "Nej"}
											</td>
											<td
												{...stylex.props(
													cockpitStyles.tableDataTd,
													cockpitStyles.element,
													cockpitStyles.focusVisible,
													cockpitStyles.integrityViewTableTableTdLastChild,
												)}
											>
												{d.lastPlacementAt ?? "—"}
											</td>
										</tr>
									))}
								</tbody>
							</table>
						</div>
					)}
				</details>
			</section>

			<section
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.card,
				)}
			>
				<h3
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.h3,
					)}
				>
					Lovgrundlag
				</h3>
				<p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
					{data.legalCitation.note}{" "}
					<Link
						to={`/lovgrundlag#${data.legalCitation.sourceId}`}
						{...stylex.props(cockpitStyles.aComposition)}
					>
						Se {data.legalCitation.sourceId} i Lovgrundlag-visningen
					</Link>
					.
				</p>
			</section>
		</section>
	);
}
