import * as stylex from "@stylexjs/stylex";
import { ButtonLink, PageHeader } from "../components/ui";
import { cockpitStyles } from "../design/cockpit.stylex";
// Flerårsoversigt — a multi-year comparison (cockpit-redesign iteration 4;
// enriched in Runde 3, iteration 11; #452: år-over-år Δ-kolonner).
//
// Renders `/api/companies/:slug/multi-year`: for every fiscal year a company
// has — live ledger years and the read-only #197 archive years alike — the
// P&L (omsætning / udgifter / resultat), the balance-sheet development
// (balancesum / egenkapital) and the key ratios (bruttomargin,
// egenkapitalandel), each as a comparison table and a Chart.js trend chart.
// This is the "alle år på ét overblik" view. Money fields are kroner
// (`formatKroner`); the ratios are 0–1 fractions (`formatPercent`).
//
// #452: each numeric metric carries an Δ-column pair year-over-year (Δ kr +
// Δ % for kroner, Δ pp for the ratios). The Δ is computed against the
// chronologically prior year; the oldest year shows "—"; the partial
// live year shows "(ej sammenligneligt — år til dato)"; a 0-denominator
// renders "—" rather than NaN/∞.

import { useParams } from "react-router-dom";
import { CompanyNav, useCompanyYear } from "../components/CompanyNav";
import { ErrorState, Loading } from "../components/Feedback";
import { MultiYearBalanceChart } from "../components/MultiYearBalanceChart";
import { MultiYearChart } from "../components/MultiYearChart";
import { api } from "../lib/api";
import { formatKroner, formatPercent } from "../lib/format";
import type { CompanyMultiYear, MultiYearRow } from "../lib/types";
import { useAsync } from "../lib/useAsync";

export function MultiYearView() {
	const { slug = "" } = useParams();
	const { setYear } = useCompanyYear();
	const state = useAsync<CompanyMultiYear>(
		(signal) => api.multiYear(slug, { signal }),
		[slug],
	);

	if (state.loading && !state.data)
		return <Loading label="Henter flerårsoversigt…" />;
	if (state.error)
		return <ErrorState message={state.error} onRetry={state.reload} />;

	const m = state.data!;
	const currency = m.company.currency || "DKK";
	// The live/current fiscal year is a partial year next to the full archived
	// ones — the newest "live" row. Mark it "(år til dato)" so the comparison
	// is not read as like-for-like.
	const currentYear =
		[...m.years]
			.filter((y) => y.source === "live")
			.sort((a, b) => b.year.localeCompare(a.year))[0]?.year ?? null;
	// The fiscal-year selector is shown for consistency with the other views;
	// newest-first like everywhere else. The Flerårsoversigt itself shows every
	// year, so the selected year only routes the other views.
	const selectorYears = [...m.years]
		.map((y) => ({
			label: y.year,
			start: null,
			end: null,
			source: y.source,
		}))
		.sort((a, b) => b.label.localeCompare(a.label));
	const selectedYear = selectorYears[0]?.label ?? "";

	// #452: build a chronological priorByYear lookup so a row can compare to
	// the year immediately before it. The API returns years oldest→newest.
	const chronological = [...m.years].sort((a, b) =>
		a.year.localeCompare(b.year),
	);
	const priorByYear = new Map<string, MultiYearRow>();
	for (let i = 1; i < chronological.length; i += 1) {
		priorByYear.set(chronological[i].year, chronological[i - 1]);
	}
	// Show Δ columns only when there is more than one year — a column that
	// is always "—" is noise.
	const showDelta = m.years.length > 1;

	return (
		<section
			data-cockpit-page="multi-year"
			data-evidence-issue="655"
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				cockpitStyles.statement,
			)}
		>
			<PageHeader
				title="Flerårsoverblik"
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
						{m.company.cvr ? `CVR ${m.company.cvr} · ` : ""}
						{m.company.country} · {currency} · Flerårsoversigt
					</p>
				</div>
			</PageHeader>

			<CompanyNav
				slug={slug}
				years={selectorYears}
				selectedYear={selectedYear}
				onYearChange={setYear}
			/>

			{m.years.length === 0 ? (
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
						Ingen regnskabsår
					</h3>
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}
					>
						Denne virksomhed har endnu ingen bogførte eller arkiverede
						regnskabsår at sammenligne.
					</p>
				</div>
			) : (
				<>
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
							Resultat — omsætning, udgifter og resultat
						</h3>
						<div
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.card,
								cockpitStyles.chartCard,
							)}
						>
							<MultiYearChart
								years={m.years}
								currentYear={currentYear}
								currency={currency}
								dataTableId="multiyear-result"
							/>
						</div>
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
								id="multiyear-result"
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.tableData,
									cockpitStyles.tableStatementTable,
									cockpitStyles.statementTableScrollTable,
								)}
							>
								<caption
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									Omsætning, udgifter og resultat pr. regnskabsår ({currency})
								</caption>
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
												cockpitStyles.statementTableScrollTableThComposition2,
											)}
										>
											Omsætning
										</th>
										{showDelta ? (
											<th
												{...stylex.props(
													cockpitStyles.statementTableScrollTableThComposition2,
												)}
											>
												Omsætning Δ (kr)
											</th>
										) : null}
										{showDelta ? (
											<th
												{...stylex.props(
													cockpitStyles.statementTableScrollTableThComposition2,
												)}
											>
												Omsætning Δ (%)
											</th>
										) : null}
										<th
											{...stylex.props(
												cockpitStyles.statementTableScrollTableThComposition2,
											)}
										>
											Udgifter
										</th>
										{showDelta ? (
											<th
												{...stylex.props(
													cockpitStyles.statementTableScrollTableThComposition2,
												)}
											>
												Udgifter Δ (kr)
											</th>
										) : null}
										{showDelta ? (
											<th
												{...stylex.props(
													cockpitStyles.statementTableScrollTableThComposition2,
												)}
											>
												Udgifter Δ (%)
											</th>
										) : null}
										<th
											{...stylex.props(
												cockpitStyles.statementTableScrollTableThComposition2,
											)}
										>
											Resultat
										</th>
										{showDelta ? (
											<th
												{...stylex.props(
													cockpitStyles.statementTableScrollTableThComposition2,
												)}
											>
												Resultat Δ (kr)
											</th>
										) : null}
										{showDelta ? (
											<th
												{...stylex.props(
													cockpitStyles.statementTableScrollTableThComposition2,
												)}
											>
												Resultat Δ (%)
											</th>
										) : null}
									</tr>
								</thead>
								<tbody
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									{m.years.map((y) => {
										const prior = priorByYear.get(y.year) ?? null;
										const isPartial = y.year === currentYear;
										return (
											<tr
												key={y.year}
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
													<YearLabel
														year={y.year}
														source={y.source}
														currentYear={currentYear}
													/>
												</td>
												<td
													{...stylex.props(
														cockpitStyles.tableDataTdNumComposition,
													)}
												>
													{formatKroner(y.omsaetning, currency)}
												</td>
												{showDelta ? (
													<DeltaKr
														current={y.omsaetning}
														prior={prior?.omsaetning ?? null}
														currency={currency}
														partial={isPartial}
													/>
												) : null}
												{showDelta ? (
													<DeltaPct
														current={y.omsaetning}
														prior={prior?.omsaetning ?? null}
														partial={isPartial}
													/>
												) : null}
												<td
													{...stylex.props(
														cockpitStyles.tableDataTdNumComposition,
													)}
												>
													{formatKroner(y.udgifter, currency)}
												</td>
												{showDelta ? (
													<DeltaKr
														current={y.udgifter}
														prior={prior?.udgifter ?? null}
														currency={currency}
														partial={isPartial}
														// Higher expenses is "worse" — invert the
														// positive/negative tone so a rise reads red.
														invertTone
													/>
												) : null}
												{showDelta ? (
													<DeltaPct
														current={y.udgifter}
														prior={prior?.udgifter ?? null}
														partial={isPartial}
														invertTone
													/>
												) : null}
												<td
													{...stylex.props(
														cockpitStyles.element,
														cockpitStyles.focusVisible,
														cockpitStyles.num,
														y.resultat >= 0 && cockpitStyles.amountPositive,
														!(y.resultat >= 0) && cockpitStyles.amountNegative,
														cockpitStyles.tableDataTdNumComposition10,
													)}
												>
													{formatKroner(y.resultat, currency)}
												</td>
												{showDelta ? (
													<DeltaKr
														current={y.resultat}
														prior={prior?.resultat ?? null}
														currency={currency}
														partial={isPartial}
													/>
												) : null}
												{showDelta ? (
													<DeltaPct
														current={y.resultat}
														prior={prior?.resultat ?? null}
														partial={isPartial}
													/>
												) : null}
											</tr>
										);
									})}
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
							Balance — balancesum og egenkapital
						</h3>
						<div
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.card,
								cockpitStyles.chartCard,
							)}
						>
							<MultiYearBalanceChart
								years={m.years}
								currentYear={currentYear}
								currency={currency}
								dataTableId="multiyear-balance"
							/>
						</div>
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
								id="multiyear-balance"
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.tableData,
									cockpitStyles.tableStatementTable,
									cockpitStyles.statementTableScrollTable,
								)}
							>
								<caption
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									Balancesum og egenkapital pr. regnskabsår ({currency})
								</caption>
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
												cockpitStyles.statementTableScrollTableThComposition2,
											)}
										>
											Balancesum
										</th>
										{showDelta ? (
											<th
												{...stylex.props(
													cockpitStyles.statementTableScrollTableThComposition2,
												)}
											>
												Balancesum Δ (kr)
											</th>
										) : null}
										{showDelta ? (
											<th
												{...stylex.props(
													cockpitStyles.statementTableScrollTableThComposition2,
												)}
											>
												Balancesum Δ (%)
											</th>
										) : null}
										<th
											{...stylex.props(
												cockpitStyles.statementTableScrollTableThComposition2,
											)}
										>
											Egenkapital
										</th>
										{showDelta ? (
											<th
												{...stylex.props(
													cockpitStyles.statementTableScrollTableThComposition2,
												)}
											>
												Egenkapital Δ (kr)
											</th>
										) : null}
										{showDelta ? (
											<th
												{...stylex.props(
													cockpitStyles.statementTableScrollTableThComposition2,
												)}
											>
												Egenkapital Δ (%)
											</th>
										) : null}
									</tr>
								</thead>
								<tbody
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									{m.years.map((y) => {
										const prior = priorByYear.get(y.year) ?? null;
										const isPartial = y.year === currentYear;
										return (
											<tr
												key={y.year}
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
													<YearLabel
														year={y.year}
														source={y.source}
														currentYear={currentYear}
													/>
												</td>
												<td
													{...stylex.props(
														cockpitStyles.tableDataTdNumComposition,
													)}
												>
													{formatKroner(y.balancesum, currency)}
												</td>
												{showDelta ? (
													<DeltaKr
														current={y.balancesum}
														prior={prior?.balancesum ?? null}
														currency={currency}
														partial={isPartial}
													/>
												) : null}
												{showDelta ? (
													<DeltaPct
														current={y.balancesum}
														prior={prior?.balancesum ?? null}
														partial={isPartial}
													/>
												) : null}
												<td
													{...stylex.props(
														cockpitStyles.element,
														cockpitStyles.focusVisible,
														cockpitStyles.num,
														y.egenkapital >= 0 && cockpitStyles.amountPositive,
														!(y.egenkapital >= 0) &&
															cockpitStyles.amountNegative,
														cockpitStyles.tableDataTdNumComposition10,
													)}
												>
													{formatKroner(y.egenkapital, currency)}
												</td>
												{showDelta ? (
													<DeltaKr
														current={y.egenkapital}
														prior={prior?.egenkapital ?? null}
														currency={currency}
														partial={isPartial}
													/>
												) : null}
												{showDelta ? (
													<DeltaPct
														current={y.egenkapital}
														prior={prior?.egenkapital ?? null}
														partial={isPartial}
													/>
												) : null}
											</tr>
										);
									})}
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
							Nøgletal pr. regnskabsår
						</h3>
						<p
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.muted,
							)}
						>
							Overskudsgrad er resultat ÷ omsætning; egenkapitalandel er
							egenkapital ÷ balancesum. Et bindestreg betyder, at nøgletallet
							ikke kan beregnes (nævneren er nul). Ændringen vises i
							procentpoint (pp), så et spring fra 17,6 % til 22,4 % læses som
							«+4,8 pp», ikke «+27 %».
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
												cockpitStyles.statementTableScrollTableThComposition2,
											)}
										>
											Overskudsgrad
										</th>
										{showDelta ? (
											<th
												{...stylex.props(
													cockpitStyles.statementTableScrollTableThComposition2,
												)}
											>
												Overskudsgrad Δ (pp)
											</th>
										) : null}
										<th
											{...stylex.props(
												cockpitStyles.statementTableScrollTableThComposition2,
											)}
										>
											Egenkapitalandel
										</th>
										{showDelta ? (
											<th
												{...stylex.props(
													cockpitStyles.statementTableScrollTableThComposition2,
												)}
											>
												Egenkapitalandel Δ (pp)
											</th>
										) : null}
									</tr>
								</thead>
								<tbody
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									{m.years.map((y) => {
										const prior = priorByYear.get(y.year) ?? null;
										const isPartial = y.year === currentYear;
										return (
											<tr
												key={y.year}
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
													<YearLabel
														year={y.year}
														source={y.source}
														currentYear={currentYear}
													/>
												</td>
												<td
													{...stylex.props(
														cockpitStyles.tableDataTdNumComposition,
													)}
												>
													{formatPercent(y.bruttomargin)}
												</td>
												{showDelta ? (
													<DeltaPp
														current={y.bruttomargin}
														prior={prior?.bruttomargin ?? null}
														partial={isPartial}
													/>
												) : null}
												<td
													{...stylex.props(
														cockpitStyles.tableDataTdNumComposition,
													)}
												>
													{formatPercent(y.egenkapitalandel)}
												</td>
												{showDelta ? (
													<DeltaPp
														current={y.egenkapitalandel}
														prior={prior?.egenkapitalandel ?? null}
														partial={isPartial}
													/>
												) : null}
											</tr>
										);
									})}
								</tbody>
							</table>
						</div>
					</div>
				</>
			)}
		</section>
	);
}

/**
 * A fiscal-year cell label — the year, an "arkiv" flag for read-only #197
 * years, and an "(år til dato)" marker for the partial live year so the
 * comparison is not read as like-for-like.
 */
function YearLabel({
	year,
	source,
	currentYear,
}: {
	year: string;
	source: "live" | "archive";
	currentYear: string | null;
}) {
	return (
		<>
			{year}
			{source === "archive" ? (
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
			) : null}
			{year === currentYear ? (
				<span
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.muted,
						cockpitStyles.multiYearCurrent,
					)}
				>
					(år til dato)
				</span>
			) : null}
		</>
	);
}

// --- #452: Δ-cell helpers ---------------------------------------------------

const PARTIAL_NOTE = "(ej sammenligneligt — år til dato)";

/**
 * Renders the formatted kroner difference between `current` and `prior`. A
 * missing prior shows "—" (the row is the oldest year); a partial live year
 * shows the explicit "(ej sammenligneligt)" note so the comparison is not
 * read as like-for-like.
 *
 * `invertTone` flips the green/red mapping for cost-like metrics (udgifter):
 * a *rise* in expenses is the "bad" direction.
 */
function DeltaKr({
	current,
	prior,
	currency,
	partial,
	invertTone = false,
}: {
	current: number;
	prior: number | null;
	currency: string;
	partial: boolean;
	invertTone?: boolean;
}) {
	if (partial) {
		return (
			<td {...stylex.props(cockpitStyles.tdNumComposition2)}>{PARTIAL_NOTE}</td>
		);
	}
	if (prior === null || !Number.isFinite(prior)) {
		return <td {...stylex.props(cockpitStyles.tdNumComposition2)}>—</td>;
	}
	const diff = current - prior;
	const positiveIsGood = !invertTone;
	const isGood = positiveIsGood ? diff > 0 : diff < 0;
	const isBad = positiveIsGood ? diff < 0 : diff > 0;
	const tone = isGood ? "amount-positive" : isBad ? "amount-negative" : "";
	const sign = diff > 0 ? "+" : diff < 0 ? "-" : "";
	// formatKroner already adds a minus prefix for negatives — strip it and
	// prepend our own sign so positives also carry a "+".
	const body = formatKroner(Math.abs(diff), currency);
	return (
		<td
			{...stylex.props(
				cockpitStyles.numComposition,
				tone === "amount-positive" && cockpitStyles.amountPositive,
				tone === "amount-negative" && cockpitStyles.amountNegative,
				cockpitStyles.tdNum,
			)}
		>
			{sign}
			{body}
		</td>
	);
}

/**
 * Renders the relative change as a Danish-formatted percent against `prior`.
 * A zero or null denominator yields "—" (no ∞/NaN). A partial live year
 * shows the "(ej sammenligneligt)" note.
 */
function DeltaPct({
	current,
	prior,
	partial,
	invertTone = false,
}: {
	current: number;
	prior: number | null;
	partial: boolean;
	invertTone?: boolean;
}) {
	if (partial) {
		return (
			<td {...stylex.props(cockpitStyles.tdNumComposition2)}>{PARTIAL_NOTE}</td>
		);
	}
	if (prior === null || !Number.isFinite(prior) || prior === 0) {
		return <td {...stylex.props(cockpitStyles.tdNumComposition2)}>—</td>;
	}
	const ratio = (current - prior) / Math.abs(prior);
	if (!Number.isFinite(ratio)) {
		return <td {...stylex.props(cockpitStyles.tdNumComposition2)}>—</td>;
	}
	const positiveIsGood = !invertTone;
	const isGood = positiveIsGood ? ratio > 0 : ratio < 0;
	const isBad = positiveIsGood ? ratio < 0 : ratio > 0;
	const tone = isGood ? "amount-positive" : isBad ? "amount-negative" : "";
	const sign = ratio > 0 ? "+" : ratio < 0 ? "-" : "";
	const body = formatPercent(Math.abs(ratio));
	return (
		<td
			{...stylex.props(
				cockpitStyles.numComposition,
				tone === "amount-positive" && cockpitStyles.amountPositive,
				tone === "amount-negative" && cockpitStyles.amountNegative,
				cockpitStyles.tdNum,
			)}
		>
			{sign}
			{body}
		</td>
	);
}

/**
 * Renders the change between two ratio (0–1 fraction) values as Danish-style
 * procentpoint, e.g. 0.176 → 0.224 becomes "+4,8 pp". A null on either side
 * yields "—" (the ratio itself was undefined). A partial live year shows the
 * "(ej sammenligneligt)" note.
 */
function DeltaPp({
	current,
	prior,
	partial,
}: {
	current: number | null;
	prior: number | null;
	partial: boolean;
}) {
	if (partial) {
		return (
			<td {...stylex.props(cockpitStyles.tdNumComposition2)}>{PARTIAL_NOTE}</td>
		);
	}
	if (
		current === null ||
		prior === null ||
		!Number.isFinite(current) ||
		!Number.isFinite(prior)
	) {
		return <td {...stylex.props(cockpitStyles.tdNumComposition2)}>—</td>;
	}
	const diff = (current - prior) * 100; // 0–1 fraction → percentage points
	const tone = diff > 0 ? "amount-positive" : diff < 0 ? "amount-negative" : "";
	const sign = diff > 0 ? "+" : diff < 0 ? "-" : "";
	// One decimal — same precision as formatPercent.
	const body = `${Math.abs(diff).toLocaleString("da-DK", {
		minimumFractionDigits: 1,
		maximumFractionDigits: 1,
	})} pp`;
	return (
		<td
			{...stylex.props(
				cockpitStyles.numComposition,
				tone === "amount-positive" && cockpitStyles.amountPositive,
				tone === "amount-negative" && cockpitStyles.amountNegative,
				cockpitStyles.tdNum,
			)}
		>
			{sign}
			{body}
		</td>
	);
}
