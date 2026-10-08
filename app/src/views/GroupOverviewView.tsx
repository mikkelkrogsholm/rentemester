import * as stylex from "@stylexjs/stylex";
import { useState } from "react";
import { ErrorState, Loading } from "../components/Feedback";
import {
	Button,
	ButtonLink,
	Input,
	PageHeader,
	Select,
} from "../components/ui";
import { cockpitStyles } from "../design/cockpit.stylex";
import { api } from "../lib/api";
import { useAsync } from "../lib/useAsync";

function currentIsoDate(): string {
	const now = new Date();
	return `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}-${String(now.getDate()).padStart(2, "0")}`;
}

export function GroupOverviewView() {
	const [asOf, setAsOf] = useState(currentIsoDate);
	const [from, setFrom] = useState(
		() => `${currentIsoDate().slice(0, 4)}-01-01`,
	);
	const [selectedProfileId, setSelectedProfileId] = useState("");
	const [dispositionId, setDispositionId] = useState("");
	const [queriedDispositionId, setQueriedDispositionId] = useState("");
	const dispositionState = useAsync(
		(signal) =>
			queriedDispositionId
				? api.intercompanyDispositionStatus(queriedDispositionId, asOf, {
						signal,
					})
				: Promise.resolve(null),
		[queriedDispositionId, asOf],
	);
	const dispositionStatus = dispositionState.data;
	const state = useAsync(
		(signal) => api.groupOverview(asOf, { signal }),
		[asOf],
	);
	const reconciliationState = useAsync(
		(signal) => api.groupReconciliation(asOf, { signal }),
		[asOf],
	);
	const eliminationState = useAsync(
		(signal) => api.groupEliminations(asOf, { signal }),
		[asOf],
	);
	const profileState = useAsync(
		(signal) => api.groupReportProfiles(asOf, { signal }),
		[asOf],
	);
	const effectiveProfileId =
		selectedProfileId || profileState.data?.profiles[0]?.id || "";
	const reportState = useAsync(
		(signal) =>
			effectiveProfileId
				? api.groupConsolidatedReport(effectiveProfileId, from, asOf, {
						signal,
					})
				: Promise.resolve(null),
		[effectiveProfileId, from, asOf],
	);

	if (
		state.loading ||
		reconciliationState.loading ||
		eliminationState.loading ||
		profileState.loading ||
		reportState.loading
	)
		return <Loading label="Henter koncernstruktur…" />;
	if (
		state.error ||
		reconciliationState.error ||
		eliminationState.error ||
		profileState.error ||
		reportState.error
	)
		return (
			<ErrorState
				message="Koncernstrukturen kan ikke vises sikkert."
				onRetry={() => {
					state.reload();
					reconciliationState.reload();
					eliminationState.reload();
					profileState.reload();
					reportState.reload();
				}}
			/>
		);
	const overview = state.data!;
	const reconciliation = reconciliationState.data!;
	const eliminations = eliminationState.data!;
	const profiles = profileState.data!.profiles;
	const report = reportState.data;
	if (
		overview.consolidatedFigures !== null ||
		overview.rawCompanySums !== null ||
		overview.consolidationStatus !== "not-available"
	) {
		return (
			<ErrorState
				message="Koncernstrukturen kan ikke vises sikkert."
				onRetry={() => {
					state.reload();
					reconciliationState.reload();
					eliminationState.reload();
					profileState.reload();
					reportState.reload();
				}}
			/>
		);
	}

	return (
		<section
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				cockpitStyles.groupOverview,
			)}
		>
			<PageHeader
				title="Koncernstruktur"
				actions={
					<ButtonLink to="/opgaver" xstyle={[cockpitStyles.aComposition]}>
						Samlede opgaver
					</ButtonLink>
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
						Datoafgrænset struktur, afstemning og godkendte read-only
						koncernrapporter.
					</p>
				</div>
				<label
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.groupAsOf,
					)}
				>
					Pr. dato
					<Input
						aria-label="Pr. dato"
						type="date"
						value={asOf}
						onChange={(event) => setAsOf(event.target.value)}
						required
						xstyle={[cockpitStyles.groupAsOfInputComposition]}
					/>
				</label>
			</PageHeader>
			<div
				role="status"
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.banner,
					cockpitStyles.bannerWarning,
				)}
			>
				<strong
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					Strukturvisning
				</strong>
				<br
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				/>
				Strukturen indeholder ingen økonomiske tal. Tal vises kun nedenfor, når
				en godkendt rapportprofil består alle kontroller.
			</div>
			{overview.manifestStatus !== "ready" && (
				<div
					role="status"
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.banner,
						cockpitStyles.bannerWarning,
					)}
				>
					Strukturen er blokeret eller ikke konfigureret. Konsoliderede
					rapporter er ikke tilgængelige.
				</div>
			)}
			{overview.blockers.length > 0 && (
				<Blockers blockers={overview.blockers} />
			)}
			{overview.groups.length === 0 ? (
				<p
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.muted,
					)}
				>
					Ingen aktiv koncernstruktur på den valgte dato.
				</p>
			) : (
				overview.groups.map((group, index) => (
					<article
						key={group.id ?? `partial-${index}`}
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.groupCard,
						)}
					>
						<header
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.groupCardHeader,
							)}
						>
							<h3
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.h3,
									cockpitStyles.groupCardH3,
								)}
							>
								{group.partial ? "Delvist synlig koncernstruktur" : group.name}
							</h3>
							<span
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.groupReadiness,
									group.readiness === "blocked" &&
										cockpitStyles.groupReadinessBlocked,
								)}
							>
								{group.readiness === "ready" ? "Klar struktur" : "Blokeret"}
							</span>
						</header>
						<h4
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.h4,
								cockpitStyles.groupCardH4,
							)}
						>
							Aktive, synlige medlemskaber
						</h4>
						<ul
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.groupCardUl,
							)}
						>
							{group.visibleMemberships.map((member) => (
								<li
									key={member.id}
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
										{member.companySlug}
									</code>{" "}
									· aktiv fra {member.validFrom}
									{member.validToExclusive
										? ` til ${member.validToExclusive}`
										: ""}{" "}
									·{" "}
									<strong
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										{member.archived ? "Arkiveret" : "Aktiv"}
									</strong>
								</li>
							))}
						</ul>
						<h4
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.h4,
								cockpitStyles.groupCardH4,
							)}
						>
							Synlige ejerskabsrelationer
						</h4>
						{group.visibleOwnership.length === 0 ? (
							<p
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.muted,
								)}
							>
								Ingen synlige ejerskabsrelationer på den valgte dato.
							</p>
						) : (
							<ul
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.groupCardUl,
								)}
							>
								{group.visibleOwnership.map((ownership) => (
									<li
										key={ownership.id}
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
											{ownership.parentCompanySlug}
										</code>{" "}
										→{" "}
										<code
											{...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
												cockpitStyles.code,
											)}
										>
											{ownership.childCompanySlug}
										</code>{" "}
										· evidence: {ownership.evidenceRefs.join(", ")}
									</li>
								))}
							</ul>
						)}
						{group.blockers.length > 0 && (
							<Blockers blockers={group.blockers} />
						)}
					</article>
				))
			)}
			<section
				aria-label="Mellemregningsafstemning"
				{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
			>
				<h3
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.h3,
					)}
				>
					Mellemregningsafstemning
				</h3>
				<p
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.muted,
					)}
				>
					Kun godkendte, eksplicitte kontomappings. Beløb sammenlignes eksakt og
					kun i samme funktionsvaluta.
				</p>
				{reconciliation.rows.length === 0 ? (
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}
					>
						Ingen aktive, godkendte mappings på den valgte dato.
					</p>
				) : (
					<div
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.tableWrap,
						)}
					>
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
											)}
										>
											Mapping
										</th>
										<th
											{...stylex.props(
												cockpitStyles.tableDataTh,
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}
										>
											Venstre
										</th>
										<th
											{...stylex.props(
												cockpitStyles.tableDataTh,
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}
										>
											Højre
										</th>
										<th
											{...stylex.props(
												cockpitStyles.tableDataTh,
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}
										>
											Difference
										</th>
										<th
											{...stylex.props(
												cockpitStyles.tableDataTh,
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}
										>
											Status
										</th>
									</tr>
								</thead>
								<tbody
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									{reconciliation.rows.map((row, index) => (
										<tr
											key={row.mappingId ?? `blocked-${index}`}
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
												)}
											>
												{row.mappingId ?? "Skjult"}
											</td>
											<td
												{...stylex.props(
													cockpitStyles.tableDataTd,
													cockpitStyles.element,
													cockpitStyles.focusVisible,
												)}
											>
												{row.left
													? `${row.left.companySlug}: ${row.left.balance.toFixed(2)} ${row.left.currency}`
													: "Ikke synlig"}
											</td>
											<td
												{...stylex.props(
													cockpitStyles.tableDataTd,
													cockpitStyles.element,
													cockpitStyles.focusVisible,
												)}
											>
												{row.right
													? `${row.right.companySlug}: ${row.right.balance.toFixed(2)} ${row.right.currency}`
													: "Ikke synlig"}
											</td>
											<td
												{...stylex.props(
													cockpitStyles.tableDataTd,
													cockpitStyles.element,
													cockpitStyles.focusVisible,
												)}
											>
												{row.difference == null
													? "—"
													: row.difference.toFixed(2)}
											</td>
											<td
												{...stylex.props(
													cockpitStyles.tableDataTd,
													cockpitStyles.element,
													cockpitStyles.focusVisible,
												)}
											>
												<strong
													{...stylex.props(
														cockpitStyles.element,
														cockpitStyles.focusVisible,
													)}
												>
													{row.status === "matched"
														? "Afstemt"
														: row.status === "mismatch"
															? "Difference"
															: "Ikke sammenlignelig"}
												</strong>
												{row.blockers.length > 0 && (
													<div
														{...stylex.props(
															cockpitStyles.element,
															cockpitStyles.focusVisible,
															cockpitStyles.muted,
														)}
													>
														{row.blockers.join(" · ")}
													</div>
												)}
											</td>
										</tr>
									))}
								</tbody>
							</table>
						</div>
					</div>
				)}
			</section>
			<section
				aria-label="Elimineringer"
				{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
			>
				<h3
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.h3,
					)}
				>
					Elimineringer
				</h3>
				<p
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.muted,
					)}
				>
					Kun anvendte, append-only balanceelimineringer afledt af eksakt
					afstemte mellemregninger. Selskabernes hovedbøger ændres ikke.
				</p>
				{eliminations.rows.length === 0 ? (
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}
					>
						Ingen anvendte eliminationer på den valgte dato.
					</p>
				) : (
					<ul
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						{eliminations.rows.map((row, index) => (
							<li
								key={row.eliminationId ?? `blocked-${index}`}
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								{row.status === "blocked" || !row.payload ? (
									<>
										<strong
											{...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}
										>
											Ikke synlig
										</strong>{" "}
										· {row.blockers.join(" · ")}
									</>
								) : (
									<>
										<strong
											{...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}
										>
											{row.eliminationId}
										</strong>{" "}
										· {Number(BigInt(row.payload.amountOre)) / 100}{" "}
										{row.payload.currency} · {row.payload.left.companySlug} ↔{" "}
										{row.payload.right.companySlug}
									</>
								)}
							</li>
						))}
					</ul>
				)}
			</section>
			<section
				aria-label="Intercompany dispositioner"
				{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
			>
				<h3
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.h3,
					)}
				>
					Intercompany dispositioner
				</h3>
				<p
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.muted,
					)}
				>
					To juridiske ledgers forbliver adskilte. Denne lifecycle viser og
					binder kun dokumenteret evidence; den bogfører og eliminerer aldrig
					selv.
				</p>
				<form
					onSubmit={(event) => {
						event.preventDefault();
						if (!dispositionId.trim()) return;
						setQueriedDispositionId(dispositionId.trim());
						dispositionState.reload();
					}}
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					<label
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Disposition-ID
						<Input
							aria-label="Disposition-ID"
							value={dispositionId}
							onChange={(event) => {
								setDispositionId(event.target.value);
								setQueriedDispositionId("");
							}}
							required
							xstyle={[cockpitStyles.inputComposition]}
						/>
					</label>
					<Button
						busy={dispositionState.loading}
						type="submit"
						xstyle={[cockpitStyles.buttonComposition]}
					>
						Vis status
					</Button>
				</form>
				{dispositionState.error && (
					<p
						role="alert"
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Dispositionen kan ikke vises med din aktuelle adgang.
					</p>
				)}
				{dispositionStatus && (
					<div
						role="status"
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.banner,
							cockpitStyles.bannerWarning,
						)}
					>
						<strong
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							{dispositionStatus.status}
						</strong>
						{Array.isArray(dispositionStatus.exceptions) &&
							dispositionStatus.exceptions.length > 0 && (
								<ul
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									{dispositionStatus.exceptions.map((exception, index) => (
										<li
											key={index}
											{...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}
										>
											{exception.kind}
										</li>
									))}
								</ul>
							)}
					</div>
				)}
				<p
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.muted,
					)}
				>
					Plan, forslag, godkendelse, link, settlement, supersession og reopen
					bruger den samme confirmed API/MCP-lifecycle med adgang til begge
					selskaber.
				</p>
			</section>
			<section
				aria-label="Konsolideret rapport"
				{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
			>
				<h3
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.h3,
					)}
				>
					Konsolideret rapport
				</h3>
				<p
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.muted,
					)}
				>
					Read-only visning. Selskabernes hovedbøger ændres ikke, og kun
					godkendte profiler med fuld selskabssynlighed kan vælges.
				</p>
				{profiles.length === 0 ? (
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}
					>
						Ingen aktiv, godkendt rapportprofil på den valgte dato.
					</p>
				) : (
					<>
						<div
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							<label
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								Rapportprofil
								<Select
									aria-label="Rapportprofil"
									value={effectiveProfileId}
									onChange={(event) => setSelectedProfileId(event.target.value)}
									xstyle={[cockpitStyles.selectComposition]}
								>
									{profiles.map((profile) => (
										<option
											key={profile.id}
											value={profile.id}
											{...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}
										>
											{profile.id} · {profile.currency}
										</option>
									))}
								</Select>
							</label>
							<label
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								Fra dato
								<Input
									aria-label="Fra dato"
									type="date"
									value={from}
									max={asOf}
									onChange={(event) => setFrom(event.target.value)}
									required
									xstyle={[cockpitStyles.inputComposition]}
								/>
							</label>
						</div>
						{report?.status === "blocked" ? (
							<Blockers blockers={report.blockers} />
						) : report?.consolidatedFigures ? (
							<>
								<div
									role="status"
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.banner,
										cockpitStyles.bannerSuccess,
									)}
								>
									<strong
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										Kontrolleret koncernrapport
									</strong>
									<br
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									/>
									Periode {report.period.from} til {report.period.to} ·{" "}
									{report.currency}
								</div>
								<div
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.tableWrap,
									)}
								>
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
														)}
													>
														Rapportlinje
													</th>
													<th
														{...stylex.props(
															cockpitStyles.tableDataTh,
															cockpitStyles.element,
															cockpitStyles.focusVisible,
														)}
													>
														Rå selskabssum
													</th>
													<th
														{...stylex.props(
															cockpitStyles.tableDataTh,
															cockpitStyles.element,
															cockpitStyles.focusVisible,
														)}
													>
														Eliminering
													</th>
													<th
														{...stylex.props(
															cockpitStyles.tableDataTh,
															cockpitStyles.element,
															cockpitStyles.focusVisible,
														)}
													>
														Konsolideret
													</th>
												</tr>
											</thead>
											<tbody
												{...stylex.props(
													cockpitStyles.element,
													cockpitStyles.focusVisible,
												)}
											>
												{report.consolidatedFigures.map((line) => (
													<tr
														key={line.lineId}
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
															)}
														>
															{line.label}
														</td>
														<td
															{...stylex.props(
																cockpitStyles.tableDataTd,
																cockpitStyles.element,
																cockpitStyles.focusVisible,
															)}
														>
															{formatGroupAmount(
																line.rawCompanySum,
																report.currency!,
															)}
														</td>
														<td
															{...stylex.props(
																cockpitStyles.tableDataTd,
																cockpitStyles.element,
																cockpitStyles.focusVisible,
															)}
														>
															{formatGroupAmount(
																line.eliminationAdjustment,
																report.currency!,
															)}
														</td>
														<td
															{...stylex.props(
																cockpitStyles.tableDataTd,
																cockpitStyles.element,
																cockpitStyles.focusVisible,
															)}
														>
															<strong
																{...stylex.props(
																	cockpitStyles.element,
																	cockpitStyles.focusVisible,
																)}
															>
																{formatGroupAmount(
																	line.consolidatedAmount,
																	report.currency!,
																)}
															</strong>
														</td>
													</tr>
												))}
											</tbody>
										</table>
									</div>
								</div>
								<details
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									<summary {...stylex.props(cockpitStyles.summaryComposition)}>
										Kildeevidens
									</summary>
									<ul
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										{report.sourceSnapshots.map((snapshot) => (
											<li
												key={snapshot.companySlug}
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
													{snapshot.companySlug}
												</code>{" "}
												· {snapshot.entryCount} poster · ledger-head{" "}
												<code
													{...stylex.props(
														cockpitStyles.element,
														cockpitStyles.focusVisible,
														cockpitStyles.code,
													)}
												>
													{snapshot.ledgerHeadHash?.slice(0, 16) ?? "tom"}
												</code>
											</li>
										))}
									</ul>
								</details>
							</>
						) : null}
					</>
				)}
			</section>
		</section>
	);
}

function formatGroupAmount(amount: number, currency: string): string {
	return new Intl.NumberFormat("da-DK", { style: "currency", currency }).format(
		amount,
	);
}

function Blockers({ blockers }: { blockers: readonly string[] }) {
	return (
		<section
			aria-label="Blokeringer"
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				cockpitStyles.groupBlockers,
			)}
		>
			<h3
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.h3,
					cockpitStyles.groupBlockersH3,
				)}
			>
				Blokeringer
			</h3>
			<ul
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.groupBlockersUl,
				)}
			>
				{blockers.map((blocker) => (
					<li
						key={blocker}
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						{blocker}
					</li>
				))}
			</ul>
		</section>
	);
}
