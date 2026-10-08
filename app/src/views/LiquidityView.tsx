import * as stylex from "@stylexjs/stylex";
import {
	Button,
	ButtonLink,
	Input,
	PageHeader,
	Select,
} from "../components/ui";
import { cockpitStyles } from "../design/cockpit.stylex";
import { useMutationOutcome } from "../lib/useMutationOutcome";
import { useUnsavedChanges } from "../lib/useUnsavedChanges";

// Likviditet / pengestrøm — the per-company cash-flow view (cockpit-redesign
// Runde 2, iteration 8).
//
// Renders `/api/companies/:slug/cashflow?year=`: actual money in and out of the
// bank for the year, read straight from the imported bank transactions (NOT
// the accrual ledger). A summary strip carries primo-saldo · ind · ud ·
// ultimo-saldo; a combined Chart.js graph shows the monthly indbetalinger /
// udbetalinger as bars and the real bank-balance trajectory as a line. When the
// company has no bank transactions a clean empty state is shown instead. All
// money fields are kroner — `formatKroner` is used throughout.

import { type FormEvent, useState } from "react";
import { useParams } from "react-router-dom";
import { CashflowChart } from "../components/CashflowChart";
import { CompanyNav, useCompanyYear } from "../components/CompanyNav";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { ErrorState, Loading } from "../components/Feedback";
import { api } from "../lib/api";
import { formatKroner } from "../lib/format";
import type { CompanyCashflow } from "../lib/types";
import { useAsync } from "../lib/useAsync";

function CommitmentMatchForm({
	slug,
	rows,
	onDone,
}: {
	slug: string;
	rows: Array<{ commitmentId: string; vendor?: string; purpose?: string }>;
	onDone: () => void;
}) {
	const [commitmentId, setCommitmentId] = useState(rows[0]?.commitmentId ?? "");
	const [occurrenceDate, setOccurrenceDate] = useState("");
	const [kind, setKind] = useState<
		"canonical_document" | "payable" | "bank_transaction"
	>("canonical_document");
	const [evidenceId, setEvidenceId] = useState("");
	const [confirmed, setConfirmed] = useState(false);
	const [message, setMessage] = useState("");
	const outcome = useMutationOutcome(onDone);
	const markSaved = useUnsavedChanges(
		Boolean(occurrenceDate || evidenceId || confirmed),
	);
	const submit = async (event: FormEvent) => {
		event.preventDefault();
		if (outcome.isBlocked()) return;
		try {
			await outcome.run(() =>
				api.supplierCommitmentMatch(slug, {
					commitmentId,
					occurrenceDate,
					evidence: { kind, id: evidenceId },
				}),
			);
			setMessage("Canonical evidence er matchet append-only.");
			setOccurrenceDate("");
			setEvidenceId("");
			setConfirmed(false);
			markSaved();
			onDone();
		} catch (cause) {
			setMessage(cause instanceof Error ? cause.message : String(cause));
		}
	};
	return (
		<form
			onSubmit={(event) => void submit(event)}
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				cockpitStyles.rowActions,
			)}
		>
			{outcome.feedback}
			<Select
				disabled={outcome.blocked}
				aria-label="Forpligtelse"
				value={commitmentId}
				onChange={(event) => setCommitmentId(event.target.value)}
				xstyle={[cockpitStyles.rowActionsSelectComposition2]}
			>
				{rows.map((row) => (
					<option
						key={row.commitmentId}
						value={row.commitmentId}
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						{row.vendor && row.purpose
							? `${row.vendor} · ${row.purpose}`
							: row.commitmentId}
					</option>
				))}
			</Select>
			<Input
				disabled={outcome.blocked}
				aria-label="Forventet dato"
				type="date"
				value={occurrenceDate}
				onChange={(event) => setOccurrenceDate(event.target.value)}
				xstyle={[cockpitStyles.rowActionsInputComposition]}
			/>
			<Select
				disabled={outcome.blocked}
				aria-label="Evidenstype"
				value={kind}
				onChange={(event) => setKind(event.target.value as typeof kind)}
				xstyle={[cockpitStyles.rowActionsSelectComposition2]}
			>
				<option
					value="canonical_document"
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					Bilag
				</option>
				<option
					value="payable"
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					Kreditor
				</option>
				<option
					value="bank_transaction"
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					Bankpost
				</option>
			</Select>
			<Input
				disabled={outcome.blocked}
				aria-label="Canonical evidence-id"
				value={evidenceId}
				onChange={(event) => setEvidenceId(event.target.value)}
				xstyle={[cockpitStyles.rowActionsInputComposition]}
			/>
			<label
				{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
			>
				<Input
					disabled={outcome.blocked}
					type="checkbox"
					checked={confirmed}
					onChange={(event) => setConfirmed(event.target.checked)}
					xstyle={[cockpitStyles.rowActionsInputComposition]}
				/>{" "}
				Bekræft match
			</label>
			<Button
				requiredPermission="company.draft.write"
				variant="secondary"
				type="submit"
				disabled={
					outcome.blocked ||
					!confirmed || !commitmentId || !occurrenceDate ||
					!evidenceId
				}
				xstyle={[cockpitStyles.buttonComposition]}
			>
				Match faktisk occurrence
			</Button>
			{message && (
				<span
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.muted,
					)}
				>
					{message}
				</span>
			)}
		</form>
	);
}

/**
 * The bank balance at the end of each of the twelve calendar months: the last
 * statement point dated in or before that month. Months before the first
 * statement point are `null` so the trajectory line starts where the data
 * does. Returns `[]` when no statement carries a running balance.
 */
function monthlyBalances(cf: CompanyCashflow): Array<number | null> {
	if (cf.balanceSeries.length === 0) return [];
	const result: Array<number | null> = new Array(12).fill(null);
	let pointer = 0;
	let last: number | null = null;
	for (let month = 1; month <= 12; month += 1) {
		while (
			pointer < cf.balanceSeries.length &&
			parseInt(cf.balanceSeries[pointer]!.date.slice(5, 7), 10) <= month
		) {
			last = cf.balanceSeries[pointer]!.balance;
			pointer += 1;
		}
		result[month - 1] = last;
	}
	return result;
}

export function LiquidityView() {
	const { slug = "" } = useParams();
	const { year, setYear } = useCompanyYear();
	const [asOf, setAsOf] = useState(() => new Date().toISOString().slice(0, 10));
	const [pending, setPending] = useState<{
		commitmentId: string;
		action: "paused" | "ended";
	} | null>(null);
	const state = useAsync<CompanyCashflow>(
		(signal) => api.cashflow(slug, year, { signal }),
		[slug, year],
	);
	const commitments = useAsync(
		(signal) => api.supplierCommitments(slug, asOf, { signal }),
		[slug, asOf],
	);

	if (state.loading && !state.data)
		return <Loading label="Henter likviditet…" />;
	if (state.error)
		return <ErrorState message={state.error} onRetry={state.reload} />;

	const cf = state.data!;
	const currency = cf.company.currency || "DKK";
	const netto = cf.totalIn - cf.totalOut;
	const balances = monthlyBalances(cf);

	return (
		<section
			data-cockpit-page="liquidity"
			data-evidence-issue="655"
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				cockpitStyles.statement,
			)}
		>
			<PageHeader
				title="Likviditet"
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
						{cf.company.cvr ? `CVR ${cf.company.cvr} · ` : ""}
						{cf.company.country} · {currency} · Likviditet
					</p>
				</div>
			</PageHeader>

			<CompanyNav
				slug={slug}
				years={cf.fiscalYears}
				selectedYear={cf.selectedYear}
				onYearChange={setYear}
			/>

			{commitments.data && (
				<section
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
						13 ugers likviditetsprognose
					</h3>
					<label
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Prognose fra{" "}
						<Input
							aria-label="Prognose fra"
							type="date"
							value={asOf}
							onChange={(event) => setAsOf(event.target.value)}
							xstyle={[cockpitStyles.statementInputComposition]}
						/>
					</label>
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}
					>
						Primo{" "}
						{commitments.data.forecast.openingBalanceVerified
							? formatKroner(commitments.data.forecast.openingCash, currency)
							: "— (ikke verificeret)"}{" "}
						· laveste basepunkt{" "}
						{formatKroner(commitments.data.forecast.lowestPoint, currency)}.
						Base er kanoniske cash-kilder; scenarieultimo tilføjer kun daterede,
						reviewede antagelser.
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
											cockpitStyles.statementTableScrollTableThComposition7,
										)}
									>
										Uge
									</th>
									<th
										{...stylex.props(
											cockpitStyles.statementTableScrollTableThComposition8,
										)}
									>
										Tilgodehavender
									</th>
									<th
										{...stylex.props(
											cockpitStyles.statementTableScrollTableThComposition8,
										)}
									>
										Kreditorer
									</th>
									<th
										{...stylex.props(
											cockpitStyles.statementTableScrollTableThComposition8,
										)}
									>
										Forpligtelser
									</th>
									<th
										{...stylex.props(
											cockpitStyles.statementTableScrollTableThComposition8,
										)}
									>
										Base ultimo
									</th>
									<th
										{...stylex.props(
											cockpitStyles.statementTableScrollTableThComposition8,
										)}
									>
										Scenarie ultimo
									</th>
								</tr>
							</thead>
							<tbody
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								{commitments.data.forecast.periods.map((p) => (
									<tr
										key={p.weekStart}
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										<td
											{...stylex.props(
												cockpitStyles.statementTableScrollTableTdComposition11,
											)}
										>
											{p.weekStart}
										</td>
										<td
											{...stylex.props(
												cockpitStyles.tableDataTdNumComposition8,
											)}
										>
											{formatKroner(p.receivables, currency)}
										</td>
										<td
											{...stylex.props(
												cockpitStyles.tableDataTdNumComposition8,
											)}
										>
											{formatKroner(p.payables, currency)}
										</td>
										<td
											{...stylex.props(
												cockpitStyles.tableDataTdNumComposition8,
											)}
										>
											{formatKroner(p.commitments + p.obligations, currency)}
										</td>
										<td
											{...stylex.props(
												cockpitStyles.tableDataTdNumComposition8,
											)}
										>
											{formatKroner(p.closingCash, currency)}
										</td>
										<td
											{...stylex.props(
												cockpitStyles.tableDataTdNumComposition8,
											)}
										>
											{formatKroner(p.scenarioClosingCash, currency)}
										</td>
									</tr>
								))}
							</tbody>
						</table>
					</div>
					<details
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						<summary {...stylex.props(cockpitStyles.summaryComposition)}>
							Antagelser, forpligtelser og kilder
						</summary>
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
												cockpitStyles.statementTableScrollTableThComposition7,
											)}
										>
											Uge
										</th>
										<th
											{...stylex.props(
												cockpitStyles.statementTableScrollTableThComposition8,
											)}
										>
											Dateret budget
										</th>
										<th
											{...stylex.props(
												cockpitStyles.statementTableScrollTableThComposition8,
											)}
										>
											Scenarie
										</th>
										<th
											{...stylex.props(
												cockpitStyles.statementTableScrollTableThComposition8,
											)}
										>
											Intercompany
										</th>
										<th
											{...stylex.props(
												cockpitStyles.statementTableScrollTableThComposition8,
											)}
										>
											Udateret månedsbudget
										</th>
										<th
											{...stylex.props(
												cockpitStyles.statementTableScrollTableThComposition7,
											)}
										>
											Kilder
										</th>
									</tr>
								</thead>
								<tbody
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									{commitments.data.forecast.periods.map((p) => (
										<tr
											key={`sources:${p.weekStart}`}
											{...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}
										>
											<td
												{...stylex.props(
													cockpitStyles.statementTableScrollTableTdComposition11,
												)}
											>
												{p.weekStart}
											</td>
											<td
												{...stylex.props(
													cockpitStyles.tableDataTdNumComposition8,
												)}
											>
												{formatKroner(p.budgets, currency)}
											</td>
											<td
												{...stylex.props(
													cockpitStyles.tableDataTdNumComposition8,
												)}
											>
												{formatKroner(p.scenarios, currency)}
											</td>
											<td
												{...stylex.props(
													cockpitStyles.tableDataTdNumComposition8,
												)}
											>
												{formatKroner(p.intercompany, currency)}
											</td>
											<td
												{...stylex.props(
													cockpitStyles.tableDataTdNumComposition8,
												)}
											>
												{formatKroner(p.undatedBudgetAssumptions, currency)}
											</td>
											<td
												{...stylex.props(
													cockpitStyles.statementTableScrollTableTdComposition11,
												)}
											>
												{p.sources.map((source) => (
													<div
														key={`${source.source}:${source.reference}`}
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
															{source.source}
														</code>{" "}
														· {formatKroner(source.amount, currency)} ·{" "}
														<code
															{...stylex.props(
																cockpitStyles.element,
																cockpitStyles.focusVisible,
																cockpitStyles.code,
															)}
														>
															{source.reference}
														</code>
														{source.assumption ? " · antagelse" : ""}
														{source.settlementStatus === "unknown"
															? " · settlement ukendt"
															: ""}
													</div>
												))}
											</td>
										</tr>
									))}
								</tbody>
							</table>
						</div>
					</details>
					<h3
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.h3,
							cockpitStyles.sectionH3,
						)}
					>
						Abonnementer og leverandørforpligtelser
					</h3>
					{commitments.data.alerts.length > 0 && (
						<div
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.card,
							)}
						>
							<h4
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.h4,
								)}
							>
								Fornyelse og opsigelse
							</h4>
							<ul
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								{commitments.data.alerts.map((alert) => (
									<li
										key={`${alert.commitmentId}:${alert.kind}:${alert.date}`}
										{...stylex.props(
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
											{alert.date}
										</strong>{" "}
										· {alert.kind} ·{" "}
										<code
											{...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
												cockpitStyles.code,
											)}
										>
											{alert.commitmentId}
										</code>
									</li>
								))}
							</ul>
						</div>
					)}
					{commitments.data.commitments.length === 0 ? (
						<p
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.muted,
							)}
						>
							Ingen godkendte forpligtelser.
						</p>
					) : (
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
												cockpitStyles.statementTableScrollTableThComposition7,
											)}
										>
											Leverandør
										</th>
										<th
											{...stylex.props(
												cockpitStyles.statementTableScrollTableThComposition7,
											)}
										>
											Formål
										</th>
										<th
											{...stylex.props(
												cockpitStyles.statementTableScrollTableThComposition8,
											)}
										>
											Beløb
										</th>
										<th
											{...stylex.props(
												cockpitStyles.statementTableScrollTableThComposition7,
											)}
										>
											Frekvens
										</th>
										<th
											{...stylex.props(
												cockpitStyles.statementTableScrollTableThComposition7,
											)}
										>
											Næste
										</th>
										<th
											{...stylex.props(
												cockpitStyles.statementTableScrollTableThComposition7,
											)}
										>
											Fornyelse
										</th>
										<th
											{...stylex.props(
												cockpitStyles.statementTableScrollTableThComposition7,
											)}
										>
											Bilag
										</th>
										<th
											{...stylex.props(
												cockpitStyles.statementTableScrollTableThComposition7,
											)}
										>
											Handling
										</th>
									</tr>
								</thead>
								<tbody
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									{commitments.data.commitments.map((c) => (
										<tr
											key={c.commitmentId}
											{...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}
										>
											<td
												{...stylex.props(
													cockpitStyles.statementTableScrollTableTdComposition11,
												)}
											>
												{c.vendor}
											</td>
											<td
												{...stylex.props(
													cockpitStyles.statementTableScrollTableTdComposition11,
												)}
											>
												{c.purpose}
											</td>
											<td
												{...stylex.props(
													cockpitStyles.tableDataTdNumComposition8,
												)}
											>
												{c.amount === null
													? "—"
													: formatKroner(c.amount, c.currency ?? currency)}
											</td>
											<td
												{...stylex.props(
													cockpitStyles.statementTableScrollTableTdComposition11,
												)}
											>
												{c.frequency}
											</td>
											<td
												{...stylex.props(
													cockpitStyles.statementTableScrollTableTdComposition11,
												)}
											>
												{c.nextDate}
											</td>
											<td
												{...stylex.props(
													cockpitStyles.statementTableScrollTableTdComposition11,
												)}
											>
												{c.renewalDate ?? "—"}
											</td>
											<td
												{...stylex.props(
													cockpitStyles.statementTableScrollTableTdComposition11,
												)}
											>
												{c.evidenceRefs.length
													? c.evidenceRefs.join(", ")
													: "Mangler"}
											</td>
											<td
												{...stylex.props(
													cockpitStyles.statementTableScrollTableTdComposition11,
												)}
											>
												<Button
													requiredPermission="company.draft.write"
													variant="secondary"
													type="button"
													onClick={() =>
														setPending({
															commitmentId: c.commitmentId,
															action: "paused",
														})
													}
													xstyle={[cockpitStyles.statementBtnComposition]}
												>
													Pause
												</Button>{" "}
												<Button
													requiredPermission="company.draft.write"
													variant="secondary"
													type="button"
													onClick={() =>
														setPending({
															commitmentId: c.commitmentId,
															action: "ended",
														})
													}
													xstyle={[cockpitStyles.statementBtnComposition]}
												>
													Afslut
												</Button>
											</td>
										</tr>
									))}
								</tbody>
							</table>
						</div>
					)}
					{commitments.data.commitments.length > 0 && (
						<details
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							<summary {...stylex.props(cockpitStyles.summaryComposition)}>
								Match faktisk bilag, kreditor eller bankpost
							</summary>
							<CommitmentMatchForm
								slug={slug}
								rows={commitments.data.commitments}
								onDone={commitments.reload}
							/>
						</details>
					)}
					{commitments.data.matches.length > 0 && (
						<div
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.card,
							)}
						>
							<h4
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.h4,
								)}
							>
								Faktisk mod forventet
							</h4>
							<ul
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								{commitments.data.matches.map((match) => (
									<li
										key={`${match.commitmentId}:${match.occurrenceDate}`}
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
											{match.commitmentId}
										</code>{" "}
										· {match.occurrenceDate} · {match.variance.dateDays} dage ·{" "}
										{match.variance.amount === null
											? "valuta kan ikke sammenlignes"
											: formatKroner(match.variance.amount, currency)}{" "}
										· bilag {match.variance.documentation}
									</li>
								))}
							</ul>
						</div>
					)}
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}
					>
						Udeladt:{" "}
						{commitments.data.forecast.completeness.excluded.join("; ")}
					</p>
					{pending && (
						<ConfirmDialog
							title={
								pending.action === "paused"
									? "Pause forpligtelse"
									: "Afslut forpligtelse"
							}
							body={
								<p
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									Ændringen er append-only og påvirker kun fremtidige
									forecast-occurrences.
								</p>
							}
							confirmLabel={pending.action === "paused" ? "Pause" : "Afslut"}
							confirmKind={pending.action === "ended" ? "danger" : "primary"}
							noteLabel="Begrundelse"
							onConfirm={async (reason) => {
								if (!reason) throw new Error("Begrundelse er påkrævet.");
								await api.supplierCommitmentChange(slug, {
									...pending,
									reason,
								});
								commitments.reload();
							}}
							onClose={() => setPending(null)}
							onRefresh={commitments.reload}
						/>
					)}
				</section>
			)}

			{cf.archived ? (
				<ArchivedNotice year={cf.selectedYear} />
			) : !cf.hasTransactions ? (
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
						Ingen pengestrøm
					</h3>
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}
					>
						Der er ingen banktransaktioner i regnskabsåret {cf.selectedYear}.
						Når et kontoudtog er importeret, vises penge ind og ud og den
						faktiske bankudvikling her.
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
						Faktiske penge ind og ud — regnskabsår {cf.selectedYear}
					</p>

					<div {...stylex.props(cockpitStyles.statusGridComposition2)}>
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
								Primo-saldo
							</h3>
							<div
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.statusFigure,
								)}
							>
								{cf.openingBalance === null
									? "—"
									: formatKroner(cf.openingBalance, currency)}
							</div>
							<p
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.muted,
									cockpitStyles.statusNote,
								)}
							>
								Faktisk banksaldo ved årets start
							</p>
						</div>
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
								Indbetalinger
							</h3>
							<div
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.statusFigure,
									cockpitStyles.statusFigureStatusIn,
								)}
							>
								{formatKroner(cf.totalIn, currency)}
							</div>
							<p
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.muted,
									cockpitStyles.statusNote,
								)}
							>
								Penge ind i året
							</p>
						</div>
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
								Udbetalinger
							</h3>
							<div
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.statusFigure,
									cockpitStyles.statusFigureStatusOut,
								)}
							>
								{formatKroner(cf.totalOut, currency)}
							</div>
							<p
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.muted,
									cockpitStyles.statusNote,
								)}
							>
								Penge ud af året
							</p>
						</div>
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
								Ultimo-saldo
							</h3>
							<div
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.statusFigure,
								)}
							>
								{cf.closingBalance === null
									? "—"
									: formatKroner(cf.closingBalance, currency)}
							</div>
							<p
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.muted,
									cockpitStyles.statusNote,
								)}
							>
								Faktisk banksaldo ved årets slut
							</p>
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
							Pengestrøm og banksaldo — {cf.selectedYear}
						</h3>
						<div
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.card,
								cockpitStyles.chartCard,
							)}
						>
							<CashflowChart
								months={cf.months}
								balanceByMonth={balances}
								currency={currency}
								dataTableId="cashflow-months"
							/>
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
							id="cashflow-months"
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
								Pengestrøm og banksaldo pr. måned ({currency})
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
										Måned
									</th>
									<th
										{...stylex.props(
											cockpitStyles.statementTableScrollTableThComposition2,
										)}
									>
										Indbetalinger
									</th>
									<th
										{...stylex.props(
											cockpitStyles.statementTableScrollTableThComposition2,
										)}
									>
										Udbetalinger
									</th>
									<th
										{...stylex.props(
											cockpitStyles.statementTableScrollTableThComposition2,
										)}
									>
										Netto
									</th>
									<th
										{...stylex.props(
											cockpitStyles.statementTableScrollTableThComposition2,
										)}
									>
										Banksaldo ultimo
									</th>
								</tr>
							</thead>
							<tbody
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								{cf.months.map((m, index) => (
									<tr
										key={m.month}
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
											{m.label}
										</td>
										<td
											{...stylex.props(cockpitStyles.tableDataTdNumComposition)}
										>
											{formatKroner(m.indbetalinger, currency)}
										</td>
										<td
											{...stylex.props(cockpitStyles.tableDataTdNumComposition)}
										>
											{formatKroner(m.udbetalinger, currency)}
										</td>
										<td
											{...stylex.props(cockpitStyles.tableDataTdNumComposition)}
										>
											{formatKroner(m.netto, currency)}
										</td>
										<td
											{...stylex.props(cockpitStyles.tableDataTdNumComposition)}
										>
											{balances[index] == null
												? "—"
												: formatKroner(balances[index]!, currency)}
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
										{...stylex.props(
											cockpitStyles.statementTableScrollTableTdComposition10,
										)}
									>
										I alt
									</td>
									<td
										{...stylex.props(
											cockpitStyles.tableDataTdNumComposition9,
											!(netto >= 0) &&
												cockpitStyles.statementResultNegativeTdNum,
										)}
									>
										{formatKroner(cf.totalIn, currency)}
									</td>
									<td
										{...stylex.props(
											cockpitStyles.tableDataTdNumComposition9,
											!(netto >= 0) &&
												cockpitStyles.statementResultNegativeTdNum,
										)}
									>
										{formatKroner(cf.totalOut, currency)}
									</td>
									<td
										{...stylex.props(
											cockpitStyles.tableDataTdNumComposition9,
											!(netto >= 0) &&
												cockpitStyles.statementResultNegativeTdNum,
										)}
									>
										{formatKroner(netto, currency)}
									</td>
									<td
										{...stylex.props(
											cockpitStyles.tableDataTdNumComposition9,
											!(netto >= 0) &&
												cockpitStyles.statementResultNegativeTdNum,
										)}
									>
										{cf.closingBalance == null
											? "—"
											: formatKroner(cf.closingBalance, currency)}
									</td>
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
						Tallene er faktiske pengebevægelser fra kontoudtoget — ikke det
						bogførte resultat. Pengestrømmen kan derfor afvige fra
						resultatopgørelsen.
					</p>
				</>
			)}
		</section>
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
				Likviditet er ikke tilgængelig for {year}
			</h3>
			<p
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.muted,
				)}
			>
				{year} er et arkiveret regnskabsår. Likviditet bygger på de importerede
				banktransaktioner, og der findes ingen kontoudtogsdata for et arkiveret
				år — pengestrømmen vises derfor ikke. Resultatopgørelse, balance,
				saldobalance og posteringer for {year} er tilgængelige.
			</p>
		</div>
	);
}
