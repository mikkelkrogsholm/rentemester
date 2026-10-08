import * as stylex from "@stylexjs/stylex";
import { ButtonLink, PageHeader } from "../components/ui";
import { cockpitStyles } from "../design/cockpit.stylex";
// Retention status view (#343) — per-virksomhed read-only side der viser
// hvor langt 5-års bogføringspligten er kommet for hver data-domæne. Bygger
// på det eksisterende `buildRetentionStatusReport` i kernen (ingen
// genimplementering) og deep-linker til Lovgrundlag-viewet (#347) for at
// citere bogføringslovens § 12, stk. 1.

import { Link, useParams } from "react-router-dom";
import { ErrorState, Loading } from "../components/Feedback";
import { api } from "../lib/api";
import type { CompanyRetention } from "../lib/types";
import { useAsync } from "../lib/useAsync";

const TABLE_LABELS: Record<string, string> = {
	documents: "Bilag",
	journal_entries: "Posteringer",
	bank_transactions: "Banktransaktioner",
};

export function RetentionView() {
	const { slug = "" } = useParams();
	const state = useAsync<CompanyRetention>(
		(signal) => api.retention(slug, { signal }),
		[slug],
	);

	if (state.loading) return <Loading />;
	if (state.error) return <ErrorState message={state.error} />;
	const r = state.data!;

	const totalExpired = r.report.rows.reduce((acc, row) => acc + row.expired, 0);

	return (
		<section
			data-cockpit-page="retention"
			data-evidence-issue="655"
			{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
		>
			<PageHeader
				title="Opbevaring"
				actions={
					<>
						<div
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.rowActions,
							)}
						>
							<ButtonLink
								to={`/companies/${slug}/manage`}
								variant={"secondary"}
								xstyle={[cockpitStyles.retentionViewBtnComposition]}
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
						{r.company.cvr ? `CVR ${r.company.cvr} · ` : ""}
						{r.company.country} · Retention (5-års bogføringspligt)
					</p>
				</div>
			</PageHeader>

			<p
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.muted,
				)}
			>
				Overblik over, hvornår personoplysninger fortsat skal opbevares sammen
				med bogføringsmaterialet.
			</p>

			{totalExpired > 0 && (
				<div
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					{totalExpired} post{totalExpired === 1 ? "" : "er"} har overskredet
					den 5-årige opbevaringspligt og kan anonymiseres.
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
					{totalExpired > 0
						? "Nogle oplysninger kan nu vurderes til anonymisering"
						: "Oplysningerne er fortsat under opbevaring"}
				</h3>
				<p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
					{totalExpired > 0
						? `${totalExpired} post${totalExpired === 1 ? " er" : "er"} har passeret opbevaringsfristen. Bogføringsmateriale slettes ikke automatisk.`
						: "Der er ingen poster, som endnu har passeret opbevaringsfristen."}
				</p>
				<p
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.muted,
					)}
				>
					Senest vurderet: {r.report.asOf}. Næste skridt:{" "}
					{totalExpired > 0 ? (
						<Link
							to={`/companies/${slug}/gdpr`}
							{...stylex.props(cockpitStyles.aComposition)}
						>
							Find oplysninger før anonymisering
						</Link>
					) : (
						"Gennemgå igen ved næste opbevaringskontrol."
					)}
				</p>
			</section>

			<details
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.card,
				)}
			>
				<summary {...stylex.props(cockpitStyles.summaryComposition)}>
					Se opbevaringsdetaljer pr. datakilde
				</summary>
				<table
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.retentionViewTableTable,
					)}
				>
					<thead
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
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
									cockpitStyles.retentionViewTableTableThLastChild,
								)}
							>
								Domæne
							</th>
							<th
								{...stylex.props(
									cockpitStyles.tableDataTh,
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.retentionViewTableTableThLastChild,
								)}
							>
								Antal poster
							</th>
							<th
								{...stylex.props(
									cockpitStyles.tableDataTh,
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.retentionViewTableTableThLastChild,
								)}
							>
								Udløbet
							</th>
							<th
								{...stylex.props(
									cockpitStyles.tableDataTh,
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.retentionViewTableTableThLastChild,
								)}
							>
								Næste udløb
							</th>
							<th
								{...stylex.props(
									cockpitStyles.tableDataTh,
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.retentionViewTableTableThLastChild,
								)}
							>
								Ældste udløbet
							</th>
						</tr>
					</thead>
					<tbody
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						{r.report.rows.map((row) => (
							<tr
								key={row.table}
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
										cockpitStyles.retentionViewTableTableTdLastChild,
									)}
								>
									{TABLE_LABELS[row.table] ?? row.table}
								</td>
								<td
									{...stylex.props(
										cockpitStyles.tableDataTd,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.retentionViewTableTableTdLastChild,
									)}
								>
									{row.total}
								</td>
								<td
									{...stylex.props(
										cockpitStyles.tableDataTd,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.retentionViewTableTableTdLastChild,
									)}
								>
									{row.expired}
								</td>
								<td
									{...stylex.props(
										cockpitStyles.tableDataTd,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.retentionViewTableTableTdLastChild,
									)}
								>
									{row.nextExpiry ?? "—"}
								</td>
								<td
									{...stylex.props(
										cockpitStyles.tableDataTd,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.retentionViewTableTableTdLastChild,
									)}
								>
									{row.oldestExpired ?? "—"}
								</td>
							</tr>
						))}
					</tbody>
				</table>
			</details>

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
					{r.legalCitation.note}{" "}
					<Link
						to={`/lovgrundlag#${r.legalCitation.sourceId}`}
						{...stylex.props(cockpitStyles.aComposition)}
					>
						Se {r.legalCitation.sourceId} i Lovgrundlag-visningen
					</Link>
					.
				</p>
				<details
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					<summary {...stylex.props(cockpitStyles.summaryComposition)}>
						Se teknisk regelgrundlag
					</summary>
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}
					>
						Anvendte regler: {r.report.appliedRules.join(", ")}
					</p>
				</details>
			</section>
		</section>
	);
}
