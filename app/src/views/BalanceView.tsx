import * as stylex from "@stylexjs/stylex";
import { ButtonLink, PageHeader } from "../components/ui";
import { cockpitStyles } from "../design/cockpit.stylex";
// Balance — the per-company balance sheet (cockpit-redesign iteration 2).
//
// Renders `/api/companies/:slug/balance?year=`: assets, liabilities and equity
// sections with section totals, as of the fiscal year's end date. The fiscal
// year's result is folded into the equity section as an "Årets resultat" line
// so `equity.total` is the equity an owner reads and the sheet balances
// (assets = liabilities + equity). All money fields are kroner.
//
// Sammenligningstal (#400 / ÅRL § 24): every line and every section total
// also carries the prior year's figure, so the balance fulfils the same
// regnskabskrav as resultatopgørelsen. When the ledger has no foregående
// regnskabsår, the prior column shows «—» rather than 0.

import { Link, useParams } from "react-router-dom";
import { ArchivedBanner } from "../components/ArchivedBanner";
import { PageState, StatusChip } from "../components/CockpitPrimitives";
import {
	accountPostingsTo,
	CompanyNav,
	useCompanyYear,
} from "../components/CompanyNav";
import { ErrorState, Loading } from "../components/Feedback";
import { api } from "../lib/api";
import { formatKroner } from "../lib/format";
import type { BalanceLine, CompanyBalance } from "../lib/types";
import { useAsync } from "../lib/useAsync";

/** Render a prior-year amount cell — «—» when no prior year exists. */
function priorCell(amount: number | null, currency: string) {
	if (amount === null) return "—";
	return formatKroner(amount, currency);
}

export function BalanceView() {
	const { slug = "" } = useParams();
	const { year, setYear } = useCompanyYear();
	const state = useAsync<CompanyBalance>(
		(signal) => api.balance(slug, year, { signal }),
		[slug, year],
	);

	if (state.loading && !state.data)
		return (
			<section
				data-evidence-issue="654"
				{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
			>
				<h2
					data-evidence-heading
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.h2,
					)}
				>
					Balance
				</h2>
				<p
					data-evidence-status="loading"
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					Henter balance
				</p>
				<Loading label="Henter balance…" />
			</section>
		);
	if (state.error)
		return (
			<section
				data-evidence-issue="654"
				{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
			>
				<h2
					data-evidence-heading
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.h2,
					)}
				>
					Balance
				</h2>
				<p
					data-evidence-status={
						/403|forbudt|adgang/i.test(state.error)
							? "warning-or-blocked"
							: "error"
					}
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					{/403|forbudt|adgang/i.test(state.error)
						? "Ufuldstændigt grundlag"
						: "Balance kunne ikke hentes"}
				</p>
				<ErrorState message={state.error} onRetry={state.reload} />
			</section>
		);

	const b = state.data!;
	const currency = b.company.currency || "DKK";
	const priorYear = String(parseInt(b.selectedYear, 10) - 1);

	return (
		<section
			data-cockpit-page="balance"
			data-evidence-issue="654"
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				cockpitStyles.statement,
			)}
		>
			<PageHeader
				evidenceHeading
				title="Balance"
				actions={
					<>
						<div
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.rowActions,
							)}
						>
							{/* #372 — "Hent CSV". #463 — "Hent PDF" som ren printbar version. */}
							<a
								href={api.statementCsvUrl(slug, "balance", b.selectedYear)}
								download
								{...stylex.props(cockpitStyles.statementBtnComposition2)}
							>
								Hent CSV
							</a>
							<a
								href={api.statementPdfUrl(slug, "balance", b.selectedYear)}
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
						{b.company.cvr ? `CVR ${b.company.cvr} · ` : ""}
						{b.company.country} · {currency} · Balance
					</p>
				</div>

				<p
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.muted,
					)}
				>
					{b.company.name} · {currency}
				</p>
				<p
					data-evidence-status={
						b.assets.lines.length ||
						b.liabilities.lines.length ||
						b.equity.lines.length
							? "normal"
							: "empty"
					}
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.muted,
					)}
				>
					{b.assets.lines.length ||
					b.liabilities.lines.length ||
					b.equity.lines.length
						? "Aktuel bogføring"
						: "Ingen balanceposter i perioden"}
				</p>
			</PageHeader>

			<CompanyNav
				slug={slug}
				years={b.fiscalYears}
				selectedYear={b.selectedYear}
				onYearChange={setYear}
			/>

			{b.archived && (
				<ArchivedBanner year={b.selectedYear} source={b.archivedSource} />
			)}
			<p
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.muted,
					cockpitStyles.statementAsof,
				)}
			>
				<StatusChip coverage={b.coverage} /> · Pr. {b.asOfDate}
			</p>
			{b.coverage.comparison === "not_comparable" && (
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
			{!(
				b.assets.lines.length ||
				b.liabilities.lines.length ||
				b.equity.lines.length
			) ? (
				<PageState kind="empty" title="Ingen balanceposter i perioden">
					Der er endnu ingen poster at vise i balancen.
				</PageState>
			) : (
				<div
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.card,
						cockpitStyles.statementCard,
					)}
					data-ui="statement-card"
				>
					<table
						data-evidence-data
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.tableData,
							cockpitStyles.tableStatementTable,
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
										cockpitStyles.tableStatementTableThComposition3,
									)}
								>
									Konto
								</th>
								<th
									{...stylex.props(
										cockpitStyles.tableStatementTableThComposition3,
									)}
								>
									Navn
								</th>
								<th
									{...stylex.props(
										cockpitStyles.tableStatementTableThComposition4,
									)}
								>
									Pr. {b.asOfDate}
								</th>
								<th
									{...stylex.props(
										cockpitStyles.tableStatementTableThComposition4,
									)}
								>
									Pr. {priorYear}-12-31
								</th>
							</tr>
						</thead>
						<BalanceSection
							heading="Aktiver"
							lines={b.assets.lines}
							total={b.assets.total}
							priorTotal={b.assets.priorTotal}
							totalLabel="Aktiver i alt"
							currency={currency}
							slug={slug}
							year={b.selectedYear}
						/>
						<BalanceSection
							heading="Passiver"
							lines={b.liabilities.lines}
							total={b.liabilities.total}
							priorTotal={b.liabilities.priorTotal}
							totalLabel="Gæld i alt"
							currency={currency}
							slug={slug}
							year={b.selectedYear}
						/>
						<BalanceSection
							heading="Egenkapital"
							lines={b.equity.lines}
							total={b.equity.total}
							priorTotal={b.equity.priorTotal}
							totalLabel="Egenkapital i alt"
							currency={currency}
							slug={slug}
							year={b.selectedYear}
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
										cockpitStyles.tableStatementTableTdComposition3,
									)}
								>
									Passiver og egenkapital i alt
								</td>
								<td {...stylex.props(cockpitStyles.tableDataTdNumComposition4)}>
									{formatKroner(b.totalLiabilitiesAndEquity, currency)}
								</td>
								<td
									{...stylex.props(
										cockpitStyles.tableStatementTableTdNumMutedComposition2,
									)}
								>
									{priorCell(b.priorTotalLiabilitiesAndEquity, currency)}
								</td>
							</tr>
						</tbody>
					</table>
				</div>
			)}
			<details
				data-evidence-progressive
				{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
			>
				<summary {...stylex.props(cockpitStyles.summaryComposition)}>
					Se rapportgrundlag
				</summary>
				<p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
					Balancen bygger på den valgte periodes bogføring.
				</p>
			</details>
			<BalanceCheck balanced={b.balanced} />
		</section>
	);
}

function BalanceSection({
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
	lines: BalanceLine[];
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
						Ingen konti.
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
							{/* A synthetic line (e.g. "Årets resultat") has no real
                  account number — render it plain rather than as a link. */}
							{line.accountNo === "—" ? (
								"—"
							) : (
								<Link
									data-evidence-core-action
									to={accountPostingsTo(slug, year, line.accountNo)}
									{...stylex.props(cockpitStyles.accountLinkComposition)}
								>
									{line.accountNo}
								</Link>
							)}
						</td>
						<td
							{...stylex.props(cockpitStyles.tableStatementTableTdComposition4)}
						>
							{line.name}
						</td>
						<td {...stylex.props(cockpitStyles.tdNumComposition)}>
							{formatKroner(line.amount, currency)}{" "}
							{line.accountNo !== "—" && (
								<Link
									to={`${accountPostingsTo(slug, year, line.accountNo)}&reportLine=${encodeURIComponent(line.name)}&asOf=${encodeURIComponent("" + year + "-12-31")}`}
									{...stylex.props(cockpitStyles.mutedComposition)}
								>
									Forklar tallet
								</Link>
							)}
						</td>
						<td {...stylex.props(cockpitStyles.tdNumComposition2)}>
							{priorCell(line.priorAmount, currency)}
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
					{priorCell(priorTotal, currency)}
				</td>
			</tr>
		</tbody>
	);
}

function BalanceCheck({ balanced }: { balanced: boolean }) {
	return (
		<p
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				cockpitStyles.statementCheck,
				balanced && cockpitStyles.statementCheckOk,
				!balanced && cockpitStyles.statementCheckAlert,
			)}
		>
			{balanced
				? "Balancen stemmer — aktiver = passiver + egenkapital."
				: "Balancen stemmer ikke. Kontrollér ledgeren."}
		</p>
	);
}
