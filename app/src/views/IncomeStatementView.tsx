import * as stylex from "@stylexjs/stylex";
import { ButtonLink, PageHeader } from "../components/ui";
import { cockpitStyles } from "../design/cockpit.stylex";
// Resultatopgørelse — the per-company income statement (cockpit-redesign it. 2).
//
// Renders `/api/companies/:slug/income-statement?year=`: income accounts and
// expense accounts grouped under section headings, each with a prior-year
// comparison column, and a clear result figure at the bottom. All money fields
// are kroner — `formatKroner` is used throughout.

import { Link, useParams } from "react-router-dom";
import { ArchivedBanner } from "../components/ArchivedBanner";
import { StatusChip } from "../components/CockpitPrimitives";
import {
	accountPostingsTo,
	CompanyNav,
	useCompanyYear,
} from "../components/CompanyNav";
import { ErrorState, Loading } from "../components/Feedback";
import { api } from "../lib/api";
import { formatKroner } from "../lib/format";
import type { CompanyIncomeStatement, IncomeStatementLine } from "../lib/types";
import { useAsync } from "../lib/useAsync";

export function IncomeStatementView() {
	const { slug = "" } = useParams();
	const { year, setYear } = useCompanyYear();
	const state = useAsync<CompanyIncomeStatement>(
		(signal) => api.incomeStatement(slug, year, { signal }),
		[slug, year],
	);

	if (state.loading && !state.data)
		return <Loading label="Henter resultatopgørelse…" />;
	if (state.error)
		return <ErrorState message={state.error} onRetry={state.reload} />;

	const s = state.data!;
	const currency = s.company.currency || "DKK";
	const priorYear = String(parseInt(s.selectedYear, 10) - 1);
	const positive = s.result >= 0;

	return (
		<section
			data-cockpit-page="income-statement"
			data-evidence-issue="655"
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				cockpitStyles.statement,
			)}
		>
			<PageHeader
				title="Resultatopgørelse"
				actions={
					<>
						<div
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.rowActions,
							)}
						>
							{/* #372 — CSV-eksport til Excel/Numbers/Sheets.
              #463 — PDF-eksport, ren printbar uden cockpit-chrome. */}
							<a
								href={api.statementCsvUrl(
									slug,
									"income-statement",
									s.selectedYear,
								)}
								download
								{...stylex.props(cockpitStyles.statementBtnComposition2)}
							>
								Hent CSV
							</a>
							<a
								href={api.statementPdfUrl(
									slug,
									"income-statement",
									s.selectedYear,
								)}
								download
								{...stylex.props(cockpitStyles.statementBtnComposition2)}
							>
								Hent PDF
							</a>
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
						{s.company.cvr ? `CVR ${s.company.cvr} · ` : ""}
						{s.company.country} · {currency} · Resultatopgørelse
					</p>
				</div>
			</PageHeader>

			<CompanyNav
				slug={slug}
				years={s.fiscalYears}
				selectedYear={s.selectedYear}
				onYearChange={setYear}
			/>

			{s.archived && (
				<ArchivedBanner year={s.selectedYear} source={s.archivedSource} />
			)}
			<p
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.muted,
					cockpitStyles.statementAsof,
				)}
			>
				<StatusChip coverage={s.coverage} />
				{s.coverage.asOfDate ? ` · Pr. ${s.coverage.asOfDate}` : ""}
			</p>
			{s.coverage.comparison === "not_comparable" && (
				<p
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.muted,
					)}
				>
					Ingen kilde for foregående år — ikke sammenlignelig.
				</p>
			)}
			<div
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.card,
					cockpitStyles.statementCard,
				)}
				data-ui="statement-card"
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
									Konto
								</th>
								<th
									{...stylex.props(
										cockpitStyles.statementTableScrollTableThComposition,
									)}
								>
									Navn
								</th>
								<th
									{...stylex.props(
										cockpitStyles.statementTableScrollTableThComposition2,
									)}
								>
									{s.selectedYear}
								</th>
								<th
									{...stylex.props(
										cockpitStyles.statementTableScrollTableThComposition2,
									)}
								>
									{priorYear}
								</th>
							</tr>
						</thead>
						<StatementSection
							heading="Indtægter"
							lines={s.income}
							total={s.totalIncome}
							priorTotal={s.priorTotalIncome}
							totalLabel="Indtægter i alt"
							currency={currency}
							slug={slug}
							year={s.selectedYear}
						/>
						<StatementSection
							heading="Udgifter"
							lines={s.expense}
							total={s.totalExpense}
							priorTotal={s.priorTotalExpense}
							totalLabel="Udgifter i alt"
							currency={currency}
							slug={slug}
							year={s.selectedYear}
						/>
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
								<td
									colSpan={2}
									{...stylex.props(
										cockpitStyles.statementTableScrollTableTdComposition10,
									)}
								>
									Årets resultat
								</td>
								<td
									{...stylex.props(
										cockpitStyles.tableDataTdNumComposition9,
										positive && cockpitStyles.statementResultPositiveTdNum,
										!positive && cockpitStyles.statementResultNegativeTdNum,
									)}
								>
									{formatKroner(s.result, currency)}
								</td>
								<td
									{...stylex.props(
										cockpitStyles.tableDataTdNumComposition9,
										positive && cockpitStyles.statementResultPositiveTdNum,
										!positive && cockpitStyles.statementResultNegativeTdNum,
									)}
								>
									{s.priorResult === null
										? "—"
										: formatKroner(s.priorResult, currency)}
								</td>
							</tr>
						</tbody>
					</table>
				</div>
			</div>
		</section>
	);
}

function StatementSection({
	heading,
	lines,
	total,
	priorTotal,
	totalLabel,
	currency,
	slug,
	year,
}: {
	heading: string;
	lines: IncomeStatementLine[];
	total: number;
	priorTotal: number | null;
	totalLabel: string;
	currency: string;
	slug: string;
	year: string;
}) {
	return (
		<tbody {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
			<tr {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
				<th
					colSpan={4}
					{...stylex.props(cockpitStyles.statementSectionHeadThComposition)}
				>
					{heading}
				</th>
			</tr>
			{lines.length === 0 ? (
				<tr
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					<td
						colSpan={4}
						{...stylex.props(cockpitStyles.emptyInlineComposition)}
					>
						Ingen posteringer i året.
					</td>
				</tr>
			) : (
				lines.map((line) => (
					<tr
						key={line.accountNo}
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.trAccountRowHover,
						)}
					>
						<td
							{...stylex.props(cockpitStyles.tableStatementTableTdComposition4)}
						>
							<Link
								to={accountPostingsTo(slug, year, line.accountNo)}
								{...stylex.props(cockpitStyles.accountLinkComposition)}
							>
								{line.accountNo}
							</Link>
						</td>
						<td
							{...stylex.props(cockpitStyles.tableStatementTableTdComposition4)}
						>
							{line.name}
						</td>
						<td {...stylex.props(cockpitStyles.tdNumComposition)}>
							{formatKroner(line.amount, currency)}{" "}
							<Link
								to={`${accountPostingsTo(slug, year, line.accountNo)}&reportLine=${encodeURIComponent(line.name)}&asOf=${year}-12-31`}
								{...stylex.props(cockpitStyles.mutedComposition)}
							>
								Forklar tallet
							</Link>
						</td>
						<td {...stylex.props(cockpitStyles.tdNumComposition2)}>
							{line.priorAmount === null
								? "—"
								: formatKroner(line.priorAmount, currency)}
						</td>
					</tr>
				))
			)}
			<tr {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
				<td
					colSpan={2}
					{...stylex.props(cockpitStyles.statementSubtotalTdComposition)}
				>
					{totalLabel}
				</td>
				<td {...stylex.props(cockpitStyles.statementSubtotalTdComposition2)}>
					{formatKroner(total, currency)}
				</td>
				<td {...stylex.props(cockpitStyles.statementSubtotalTdComposition3)}>
					{priorTotal === null ? "—" : formatKroner(priorTotal, currency)}
				</td>
			</tr>
		</tbody>
	);
}
