import * as stylex from "@stylexjs/stylex";
import { Button, ButtonLink, PageHeader } from "../components/ui";
import { cockpitStyles } from "../design/cockpit.stylex";
// Forpligtelser — the per-company obligations view (cockpit-redesign it. 7).
//
// Renders `/api/companies/:slug/obligations?year=`: the "hvad skylder jeg og
// hvornår" list — outstanding VAT, corporation tax, trade creditors, accrued
// auditor and any other payable read from the ledger, each with the amount
// owed and a due date where one is derivable. Rows are sorted by due date
// (soonest first); a row with no known date shows "—". The total owed sits in
// a summary card above the table. All money fields are kroner — `formatKroner`
// is used throughout.

import { useParams } from "react-router-dom";
import { CompanyNav, useCompanyYear } from "../components/CompanyNav";
import { ErrorState, Loading } from "../components/Feedback";
import { api } from "../lib/api";
import { formatKroner, tastSelvNumber } from "../lib/format";
import type { CompanyObligations, ObligationRow } from "../lib/types";
import { useAsync } from "../lib/useAsync";

export function ObligationsView() {
	const { slug = "" } = useParams();
	const { year, setYear } = useCompanyYear();
	const state = useAsync<CompanyObligations>(
		(signal) => api.obligations(slug, year, { signal }),
		[slug, year],
	);

	if (state.loading && !state.data)
		return <Loading label="Henter forpligtelser…" />;
	if (state.error)
		return <ErrorState message={state.error} onRetry={state.reload} />;

	const o = state.data!;
	const currency = o.company.currency || "DKK";

	return (
		<section
			data-cockpit-page="obligations"
			data-evidence-issue="655"
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				cockpitStyles.statement,
			)}
		>
			<PageHeader
				title="Forpligtelser"
				actions={
					<>
						<ButtonLink
							variant="secondary"
							to={`/companies/${slug}/opgaver`}
							xstyle={[cockpitStyles.aComposition]}
						>
							Planlæg i Opgaver
						</ButtonLink>
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
						{o.company.cvr ? `CVR ${o.company.cvr} · ` : ""}
						{o.company.country} · {currency} · Forpligtelser
					</p>
				</div>
			</PageHeader>

			<CompanyNav
				slug={slug}
				years={o.fiscalYears}
				selectedYear={o.selectedYear}
				onYearChange={setYear}
			/>

			{o.archived ? (
				<ArchivedNotice year={o.selectedYear} />
			) : o.obligations.length === 0 ? (
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
						Ingen forpligtelser
					</h3>
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}
					>
						Der er ingen udestående moms, skat eller kreditorgæld i
						regnskabsåret {o.selectedYear}. Skyldige beløb vises her, så snart
						de er bogført.
					</p>
				</div>
			) : (
				<>
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
							cockpitStyles.statementAsof,
						)}
					>
						Hvad virksomheden skylder — regnskabsår {o.selectedYear}
					</p>

					<div
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.invoicesSummary,
							cockpitStyles.statusGrid,
						)}
					>
						<div
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.card,
								cockpitStyles.statusCard,
							)}
						>
							<h3
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.h3,
									cockpitStyles.statusCardH3,
								)}
							>
								Skyldige beløb i alt
							</h3>
							<div
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.statusFigure,
									o.totalOwed > 0 && cockpitStyles.statusFigureStatusAlert,
								)}
							>
								{formatKroner(o.totalOwed, currency)}
							</div>
							<p
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.muted,
									cockpitStyles.statusNote,
								)}
							>
								{o.obligations.length}{" "}
								{o.obligations.length === 1 ? "forpligtelse" : "forpligtelser"}{" "}
								· sorteret efter frist
							</p>
						</div>
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
										Forpligtelse
									</th>
									<th
										{...stylex.props(
											cockpitStyles.statementTableScrollTableThComposition,
										)}
									>
										Konto
									</th>
									<th
										{...stylex.props(
											cockpitStyles.statementTableScrollTableThComposition,
										)}
									>
										Frist
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
											cockpitStyles.statementTableScrollTableThComposition2,
										)}
									>
										Skyldigt beløb
									</th>
									<th
										{...stylex.props(
											cockpitStyles.statementTableScrollTableThComposition,
										)}
									>
										Handlinger
									</th>
								</tr>
							</thead>
							<tbody
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								{o.obligations.map((row, i) => (
									<tr
										key={`${row.kind}-${row.accountNo ?? i}`}
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
											{row.label}
										</td>
										<td
											{...stylex.props(
												cockpitStyles.tableStatementTableTdAccountNoComposition,
											)}
										>
											{row.accountNo ?? "—"}
										</td>
										<td
											{...stylex.props(
												cockpitStyles.statementTableScrollTableTdComposition9,
											)}
										>
											{row.dueDate ?? "—"}
										</td>
										<td
											{...stylex.props(
												cockpitStyles.statementTableScrollTableTdComposition,
											)}
										>
											<DeadlineFlag row={row} />
										</td>
										<td
											{...stylex.props(cockpitStyles.tableDataTdNumComposition)}
										>
											{/* The annual-report row is a filing DEADLINE, not a
                          debt — it has no kroner amount, so show a dash. */}
											{row.kind === "annual-report"
												? "—"
												: formatKroner(row.amount, currency)}
										</td>
										<td
											{...stylex.props(
												cockpitStyles.statementTableScrollTableTdComposition,
											)}
										>
											<ObligationActions
												row={row}
												slug={slug}
												currency={currency}
											/>
										</td>
									</tr>
								))}
								<tr
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									<td
										colSpan={4}
										{...stylex.props(
											cockpitStyles.statementTableScrollTableTdComposition10,
										)}
									>
										Skyldige beløb i alt
									</td>
									<td
										{...stylex.props(
											cockpitStyles.statementResultNegativeTdNumComposition,
										)}
									>
										{formatKroner(o.totalOwed, currency)}
									</td>
									<td
										{...stylex.props(
											cockpitStyles.statementTableScrollTableTdComposition10,
										)}
									></td>
								</tr>
							</tbody>
						</table>
					</div>
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.statementCheck,
							cockpitStyles.statementCheckOk,
						)}
					>
						Beløbene er læst direkte fra ledgeren. Frister vises hvor de kan
						udledes — moms efter virksomhedens momsperiode (måned, kvartal eller
						halvår), selskabsskat efter indkomståret, og årsrapporten til
						Erhvervsstyrelsen efter regnskabsåret; øvrige poster har ingen kendt
						dato.
					</p>
				</>
			)}
		</section>
	);
}

/**
 * The deadline status flag for an obligation row: a "X dage tilbage"
 * countdown that turns critical once the deadline is near or passed, and a
 * neutral "Ingen frist" for a dateless payable.
 */
function DeadlineFlag({ row }: { row: ObligationRow }) {
	if (row.dueDate === null || row.daysRemaining === null) {
		return (
			<span
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.flag,
					cockpitStyles.flagNeutral,
				)}
			>
				Ingen frist
			</span>
		);
	}
	const days = row.daysRemaining;
	if (days < 0) {
		return (
			<span
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.flag,
					cockpitStyles.flagCritical,
				)}
			>
				Overskredet {Math.abs(days)} {Math.abs(days) === 1 ? "dag" : "dage"}
			</span>
		);
	}
	if (days === 0) {
		return (
			<span
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.flag,
					cockpitStyles.flagCritical,
				)}
			>
				Frist i dag
			</span>
		);
	}
	const tone = days <= 30 ? "warning" : "ok";
	return (
		<span
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				cockpitStyles.flag,
				tone === "warning" && cockpitStyles.flagWarning,
				tone === "ok" && cockpitStyles.flagOk,
			)}
		>
			{days} {days === 1 ? "dag" : "dage"} tilbage
		</span>
	);
}

/**
 * Pr-række handlinger til Forpligtelser (#391):
 *
 * - Eksterne handoff-links til den officielle indberetnings-portal (skat.dk,
 *   indberet.virk.dk) så ejeren ikke skal google sig frem.
 * - 'Kopiér beløb' så ejeren ikke skal taste fx 47.310,00 manuelt ind i
 *   sin bank-app.
 * - Krydslink til Moms-viewet for vat-rækken, så ejeren kan se SKAT-
 *   rubrikkerne ved siden af.
 *
 * Note: 'Markér som betalt'-flowet (#391 acceptkriterium 4) er ikke
 * implementeret her — det er et write-flow med actor + audit-event og
 * spores som follow-up.
 */
const EXTERNAL_LINK: Partial<
	Record<ObligationRow["kind"], { href: string; label: string }>
> = {
	vat: {
		href: "https://www.skat.dk/erhverv/moms",
		label: "Indberet på skat.dk",
	},
	"corporation-tax": {
		href: "https://www.skat.dk/erhverv/selskabsskat",
		label: "Selskabsskat på skat.dk",
	},
	"annual-report": {
		href: "https://indberet.virk.dk",
		label: "Indberet på virk.dk",
	},
};

function ObligationActions({
	row,
	slug,
	currency,
}: {
	row: ObligationRow;
	slug: string;
	currency: string;
}) {
	const external = EXTERNAL_LINK[row.kind];
	const copyAmount = async () => {
		if (row.kind === "annual-report") return;
		// #UI-10: share the SKAT TastSelv number format with VatView — no thousand
		// separator, whole kroner as bare integers — so the two copy buttons agree.
		const text = tastSelvNumber(row.amount);
		try {
			await navigator.clipboard.writeText(text);
		} catch {
			// Clipboard API may be blocked (Safari permission, http) — silent fail.
		}
	};
	return (
		<div
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				cockpitStyles.rowActions,
			)}
		>
			{external && (
				<a
					href={external.href}
					target="_blank"
					rel="noopener noreferrer"
					{...stylex.props(cockpitStyles.aComposition)}
				>
					{external.label}
				</a>
			)}
			{row.kind === "vat" && (
				<ButtonLink
					to={`/companies/${slug}/moms`}
					variant={"secondary"}
					xstyle={[cockpitStyles.aComposition]}
				>
					SKAT-rubrikker
				</ButtonLink>
			)}
			{row.kind !== "annual-report" && (
				<Button
					variant="secondary"
					type="button"
					onClick={copyAmount}
					aria-label={`Kopiér beløb (${formatKroner(row.amount, currency)})`}
					xstyle={[cockpitStyles.buttonComposition]}
				>
					Kopiér beløb
				</Button>
			)}
		</div>
	);
}

function ArchivedNotice({ year }: { year: string }) {
	return (
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
				Forpligtelser er ikke tilgængelige for {year}
			</h3>
			<p
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.muted,
				)}
			>
				{year} er et arkiveret regnskabsår. Forpligtelser — moms, selskabsskat
				og kreditorgæld med forfaldsdato — opgøres kun for den aktive ledger og
				vises derfor ikke for et arkiveret år. Resultatopgørelse, balance,
				saldobalance og posteringer for {year} er tilgængelige.
			</p>
		</div>
	);
}
