import * as stylex from "@stylexjs/stylex";
import { ButtonLink, PageHeader } from "../components/ui";
import { cockpitStyles } from "../design/cockpit.stylex";
// Om arkivet — an explainer for the read-only #197 archive (Runde 3, it. 11).
//
// Before Runde 3 the core views could only render the live year, so this tab
// carried a raw archived SaldoBalance to give the old years anywhere to live.
// Iteration 10 made Resultatopgørelse / Balance / Saldobalance / Posteringer /
// Overblik archive-aware via the fiscal-year selector, so the raw table here
// became redundant. This tab is now a concise "Om arkivet" page: which years
// are archived, where the data came from (the Dinero import #197) and that it
// is read-only — with links pointing the user into the archive-aware views.

import { Link, useParams } from "react-router-dom";
import { CompanyNav, useCompanyYear } from "../components/CompanyNav";
import { ErrorState, Loading } from "../components/Feedback";
import { api } from "../lib/api";
import type { FiscalYearEntry } from "../lib/types";
import { useAsync } from "../lib/useAsync";

export function ArchiveView() {
	const { slug = "" } = useParams();
	const { year, setYear } = useCompanyYear();

	const yearsState = useAsync<FiscalYearEntry[]>(
		(signal) => api.fiscalYears(slug, { signal }),
		[slug],
	);

	if (yearsState.loading && !yearsState.data)
		return <Loading label="Henter arkiv…" />;
	if (yearsState.error)
		return (
			<ErrorState message={yearsState.error} onRetry={yearsState.reload} />
		);

	const years = yearsState.data!;
	// Archived years, newest-first — the years the explainer is about.
	const archiveYears = years
		.filter((y) => y.source === "archive")
		.sort((a, b) => b.label.localeCompare(a.label));
	const liveYears = years
		.filter((y) => y.source === "live")
		.sort((a, b) => b.label.localeCompare(a.label));
	const selectedLabel = year ?? years[0]?.label ?? "";

	return (
		<section
			data-cockpit-page="archive"
			data-evidence-issue="655"
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				cockpitStyles.statement,
			)}
		>
			<PageHeader
				title="Arkiv"
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
								xstyle={[cockpitStyles.statementBtnComposition2]}
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
						Tidligere regnskabsår · skrivebeskyttet
					</p>
				</div>
			</PageHeader>

			<CompanyNav
				slug={slug}
				years={years}
				selectedYear={selectedLabel}
				onYearChange={setYear}
			/>

			{archiveYears.length === 0 ? (
				<div
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.card,
						cockpitStyles.archivedNotice,
					)}
				>
					<h3
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.h3,
							cockpitStyles.archivedNoticeH3,
						)}
					>
						Ingen arkiverede regnskabsår
					</h3>
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}
					>
						Denne virksomhed har ingen tidligere år i det skrivebeskyttede arkiv
						— alle dens regnskabsår ligger i den aktive ledger.
					</p>
				</div>
			) : (
				<>
					<div
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.card,
							cockpitStyles.archiveBanner,
						)}
					>
						<span
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.flag,
								cockpitStyles.flagWarning,
								cockpitStyles.archiveBannerFlag,
							)}
						>
							Arkiveret
						</span>
						<p
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.archiveBannerP,
							)}
						>
							<strong
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								Det arkiverede regnskab er skrivebeskyttet.
							</strong>{" "}
							De arkiverede år blev importeret fra virksomhedens tidligere
							Dinero-regnskab — fuld saldobalance og alle posteringer pr. år.
							Tallene ligger uden for den aktive bogføring og kan ikke
							redigeres.
						</p>
					</div>

					<div
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.section,
						)}
						data-ui="section"
					>
						<h3
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.h3,
								cockpitStyles.sectionH3,
							)}
						>
							Arkiverede regnskabsår
						</h3>
						<p
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.muted,
							)}
						>
							Hvert arkiveret år kan ses i de almindelige visninger — vælg året
							i regnskabsårs-vælgeren ovenfor, så viser Resultatopgørelse,
							Balance, Saldobalance, Posteringer og Overblik arkiv-tallene med
							et skrivebeskyttet-banner.
						</p>
						<div
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.card,
								cockpitStyles.statementCard,
								cockpitStyles.tableScroll,
							)}
							data-ui="statement-card"
						>
							<table
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.tableData,
									cockpitStyles.tableStatementTable,
									cockpitStyles.statementTableScrollTable,
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
												cockpitStyles.statementTableScrollTableThComposition,
											)}
										>
											Regnskabsår
										</th>
										<th
											{...stylex.props(
												cockpitStyles.statementTableScrollTableThComposition,
											)}
										>
											Kilde
										</th>
										<th
											{...stylex.props(
												cockpitStyles.statementTableScrollTableThComposition,
											)}
										>
											Status
										</th>
										<th
											{...stylex.props(
												cockpitStyles.statementTableScrollTableThComposition,
											)}
										>
											Visninger
										</th>
									</tr>
								</thead>
								<tbody
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									{archiveYears.map((y) => (
										<tr
											key={y.label}
											{...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}
										>
											<td
												{...stylex.props(
													cockpitStyles.statementTableScrollTableTdComposition,
												)}
											>
												{y.label}
												<span
													{...stylex.props(
														cockpitStyles.element,
														cockpitStyles.focusVisible,
														cockpitStyles.flag,
														cockpitStyles.archiveTag,
														cockpitStyles.flagWarning,
													)}
												>
													arkiv
												</span>
											</td>
											<td
												{...stylex.props(
													cockpitStyles.statementTableScrollTableTdComposition,
												)}
											>
												Importeret fra Dinero
											</td>
											<td
												{...stylex.props(
													cockpitStyles.statementTableScrollTableTdComposition,
												)}
											>
												Skrivebeskyttet
											</td>
											<td
												{...stylex.props(
													cockpitStyles.statementTableScrollTableTdComposition,
												)}
											>
												<Link
													to={`/companies/${slug}/saldobalance?year=${y.label}`}
													{...stylex.props(cockpitStyles.aComposition)}
												>
													Saldobalance
												</Link>
												{" · "}
												<Link
													to={`/companies/${slug}/resultatopgorelse?year=${y.label}`}
													{...stylex.props(cockpitStyles.aComposition)}
												>
													Resultatopgørelse
												</Link>
												{" · "}
												<Link
													to={`/companies/${slug}?year=${y.label}`}
													{...stylex.props(cockpitStyles.aComposition)}
												>
													Overblik
												</Link>
											</td>
										</tr>
									))}
								</tbody>
							</table>
						</div>
					</div>

					<div
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.section,
						)}
						data-ui="section"
					>
						<h3
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.h3,
								cockpitStyles.sectionH3,
							)}
						>
							Aktive regnskabsår
						</h3>
						{liveYears.length === 0 ? (
							<p
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.muted,
								)}
							>
								Der er endnu ingen bogførte år i den aktive ledger.
							</p>
						) : (
							<p
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.muted,
								)}
							>
								{liveYears.map((y) => y.label).join(", ")} bogføres i den aktive
								ledger og kan redigeres. Se også{" "}
								<Link
									to={`/companies/${slug}/fleraar`}
									{...stylex.props(cockpitStyles.aComposition)}
								>
									Flerår
								</Link>{" "}
								for et samlet overblik på tværs af alle år.
							</p>
						)}
					</div>
				</>
			)}
		</section>
	);
}
