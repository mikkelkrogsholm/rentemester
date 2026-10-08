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
// Budget — the per-company budget vs. faktisk view (#339).
//
// Two faces of the same data, toggled by a single switch:
//
//   1. Plan-mode ("Budget"): an input grid (konto × måned) where the owner
//      types planned amounts. Each saved cell appends a new revision via
//      `POST /api/companies/:slug/budget` — the core is append-only, so
//      re-saving a cell is always safe and the history is fully auditable.
//
//   2. Compare-mode ("Sammenlign med faktisk"): the same grid replaced by
//      the comparison table read from `GET .../budget-vs-actual`. Each row
//      is one (account, month) cell with budget, actual, variance (kr) and
//      variance % alongside.
//
// The sign convention follows core/budget.ts: positive variance = "good"
// (under budget for expense accounts, over target for income accounts).
// Money is kroner throughout — `formatKroner` is used everywhere, `formatPercent`
// for the % column.

import { useEffect, useMemo, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { CompanyNav, useCompanyYear } from "../components/CompanyNav";
import { ErrorState, Loading } from "../components/Feedback";
import { api } from "../lib/api";
import { formatKroner, formatPercent, parseDanishAmount } from "../lib/format";
import type {
	CompanyBudget,
	CompanyBudgetDimensionActuals,
	CompanyBudgetLine,
	CompanyBudgetVsActual,
	CompanyBudgetVsActualLine,
} from "../lib/types";
import { useAsync } from "../lib/useAsync";
import { useCapabilities } from "../lib/useCapabilities";

/** Danish month abbreviations, jan→dec — same set the other views use. */
const MONTH_LABELS_DK = [
	"jan",
	"feb",
	"mar",
	"apr",
	"maj",
	"jun",
	"jul",
	"aug",
	"sep",
	"okt",
	"nov",
	"dec",
];

/** Pretty Danish label for a `YYYY-MM` period, e.g. `2026-06` → `jun 2026`. */
function periodLabel(period: string): string {
	const m = /^(\d{4})-(\d{2})$/.exec(period);
	if (!m) return period;
	const year = m[1]!;
	const month = Number(m[2]);
	if (!(month >= 1 && month <= 12)) return period;
	return `${MONTH_LABELS_DK[month - 1]} ${year}`;
}

export function BudgetView() {
	const { slug = "" } = useParams();
	const { year, setYear } = useCompanyYear();
	const [mode, setMode] = useState<"plan" | "compare">("plan");

	const plan = useAsync<CompanyBudget>(
		(signal) => api.budget(slug, year, { signal }),
		[slug, year, mode],
	);
	const compare = useAsync<CompanyBudgetVsActual>(
		(signal) => api.budgetVsActual(slug, year, { signal }),
		[slug, year, mode],
	);
	const dimensionActuals = useAsync<CompanyBudgetDimensionActuals | null>(
		(signal) =>
			mode === "compare"
				? api.budgetDimensionActuals(slug, year, { signal })
				: Promise.resolve(null),
		[slug, year, mode],
	);

	// We always need ONE of the two payloads to render. The plan/compare toggle
	// picks which one drives the body. The other one is also fetched so a
	// toggle is instant after first load.
	const state = mode === "plan" ? plan : compare;

	if (state.loading && !state.data) return <Loading label="Henter budget…" />;
	if (state.error && !state.data)
		return <ErrorState message={state.error} onRetry={state.reload} />;

	const data = state.data!;
	const currency = data.company.currency || "DKK";

	return (
		<section
			data-cockpit-page="budget"
			data-evidence-issue="655"
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				cockpitStyles.statement,
			)}
		>
			{state.error && (
				<div
					role="alert"
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.banner,
						cockpitStyles.bannerWarning,
					)}
				>
					Status kunne ikke opdateres. Din formular er bevaret; oplysningerne
					bag den er fra den seneste gennemførte læsning.
				</div>
			)}
			<PageHeader
				title="Budget"
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
						{data.company.cvr ? `CVR ${data.company.cvr} · ` : ""}
						{data.company.country} · {currency} · Budget
					</p>
				</div>
			</PageHeader>

			<CompanyNav
				slug={slug}
				years={data.fiscalYears}
				selectedYear={data.selectedYear}
				onYearChange={setYear}
			/>

			<div
				role="group"
				aria-label="Budget-visning"
				{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
			>
				<Button
					type="button"
					aria-pressed={mode === "plan"}
					onClick={() => setMode("plan")}
					variant={!(mode === "plan") ? "secondary" : "primary"}
					xstyle={[cockpitStyles.statementBtnComposition]}
				>
					Budget
				</Button>
				<Button
					type="button"
					aria-pressed={mode === "compare"}
					onClick={() => setMode("compare")}
					variant={!(mode === "compare") ? "secondary" : "primary"}
					xstyle={[cockpitStyles.statementBtnComposition]}
				>
					Sammenlign med faktisk
				</Button>
			</div>

			{data.archived ? (
				<ArchivedNotice year={data.selectedYear} />
			) : mode === "plan" ? (
				<BudgetGrid
					slug={slug}
					data={plan.data!}
					currency={currency}
					onSaved={() => {
						plan.reload();
						compare.reload();
					}}
				/>
			) : (
				<>
					<BudgetVsActualTable data={compare.data!} currency={currency} />
					<DimensionBudgetComparison
						slug={slug}
						data={dimensionActuals.data ?? null}
						currency={currency}
					/>
				</>
			)}
		</section>
	);
}

/**
 * This deliberately places approved dimension actuals beside (not inside) the
 * legal account budget. A dimension can cover only part of an account, so a
 * variance against the whole account budget would be misleading.
 */
function DimensionBudgetComparison({
	slug,
	data,
	currency,
}: {
	slug: string;
	data: CompanyBudgetDimensionActuals | null;
	currency: string;
}) {
	const [selection, setSelection] = useState("");
	if (!data || data.archived) return null;
	const selected =
		selection === ""
			? []
			: data.rows.filter((row) =>
					selection.includes(":")
						? `${row.dimensionId}:${row.memberId}` === selection
						: row.dimensionId === selection,
				);
	const grouped = new Map<
		string,
		{
			accountNo: string;
			period: string;
			actual: number;
			journalLineIds: number[];
		}
	>();
	for (const row of selected) {
		const key = `${row.accountNo}\u001f${row.period}`;
		const prior = grouped.get(key) ?? {
			accountNo: row.accountNo,
			period: row.period,
			actual: 0,
			journalLineIds: [],
		};
		prior.actual += row.actual;
		prior.journalLineIds.push(row.journalLineId);
		grouped.set(key, prior);
	}
	const rows = [...grouped.values()].sort(
		(a, b) =>
			a.accountNo.localeCompare(b.accountNo) ||
			a.period.localeCompare(b.period),
	);
	const accountActual = new Map(
		data.accountTotals.map((row) => [
			`${row.accountNo}\u001f${row.period}`,
			row.actual,
		]),
	);
	const dimensionBudget = new Map(
		data.dimensionBudgets
			.filter((row) =>
				selection.includes(":")
					? `${row.dimensionId}:${row.memberId}` === selection
					: row.dimensionId === selection,
			)
			.map((row) => [`${row.accountNo}\u001f${row.period}`, row]),
	);
	return (
		<section
			aria-label="Dimensionssammenligning"
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				cockpitStyles.card,
				cockpitStyles.statementCard,
			)}
			data-ui="statement-card"
		>
			<h3
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.h3,
				)}
			>
				Dimensioner mod konto-budget
			</h3>
			<p
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.muted,
				)}
			>
				En dimensionsvariance vises kun, når der findes en eksplicit reviewet
				fordeling, som stemmer præcist med konto-budgettet. Ellers er budgettet
				konto-niveau og kan ikke sammenlignes som dimensionsbudget.
			</p>
			{data.dimensionOptions.length === 0 ? (
				<p
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.muted,
					)}
				>
					Ingen godkendte dimensionsklassifikationer i perioden.
				</p>
			) : (
				<>
					<label
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Filter dimension
						<Select
							aria-label="Filter dimension"
							value={selection}
							onChange={(event) => setSelection(event.target.value)}
							xstyle={[cockpitStyles.selectComposition]}
						>
							<option
								value=""
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								Vælg dimension eller medlem
							</option>
							{data.dimensionOptions.map((option) => (
								<option
									key={option.value}
									value={option.value}
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									{option.label}
								</option>
							))}
						</Select>
					</label>
					{selection !== "" && (
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
											Måned
										</th>
										<th
											{...stylex.props(
												cockpitStyles.tableStatementTableThComposition4,
											)}
										>
											Dimensionsaktual
										</th>
										<th
											{...stylex.props(
												cockpitStyles.tableStatementTableThComposition4,
											)}
										>
											Dimensionsbudget
										</th>
										<th
											{...stylex.props(
												cockpitStyles.tableStatementTableThComposition4,
											)}
										>
											Variance
										</th>
										<th
											{...stylex.props(
												cockpitStyles.tableStatementTableThComposition4,
											)}
										>
											Kontoaktual
										</th>
										<th
											{...stylex.props(
												cockpitStyles.tableStatementTableThComposition3,
											)}
										>
											Kilde
										</th>
									</tr>
								</thead>
								<tbody
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									{rows.length === 0 ? (
										<tr
											{...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}
										>
											<td
												colSpan={7}
												{...stylex.props(
													cockpitStyles.tableStatementTableTdComposition,
												)}
											>
												Ingen godkendte tildelinger for dette filter.
											</td>
										</tr>
									) : (
										rows.map((row) => {
											const key = `${row.accountNo}\u001f${row.period}`;
											const reviewedBudget = dimensionBudget.get(key);
											return (
												<tr
													key={key}
													{...stylex.props(
														cockpitStyles.element,
														cockpitStyles.focusVisible,
													)}
												>
													<td
														{...stylex.props(
															cockpitStyles.tableStatementTableTdAccountNoComposition2,
														)}
													>
														{row.accountNo}
													</td>
													<td
														{...stylex.props(
															cockpitStyles.tableStatementTableTdComposition2,
														)}
													>
														{periodLabel(row.period)}
													</td>
													<td
														{...stylex.props(
															cockpitStyles.tableDataTdNumComposition3,
														)}
													>
														{formatKroner(row.actual, currency)}
													</td>
													{reviewedBudget ? (
														<>
															<td
																{...stylex.props(
																	cockpitStyles.tableDataTdNumComposition3,
																)}
															>
																{formatKroner(reviewedBudget.budget, currency)}
															</td>
															<td
																{...stylex.props(
																	cockpitStyles.tableDataTdNumComposition3,
																)}
															>
																{formatKroner(
																	reviewedBudget.budget - row.actual,
																	currency,
																)}
															</td>
														</>
													) : (
														<>
															<td
																{...stylex.props(
																	cockpitStyles.tableStatementTableTdNumMutedComposition,
																)}
															>
																Ikke understøttet
															</td>
															<td
																{...stylex.props(
																	cockpitStyles.tableStatementTableTdNumMutedComposition,
																)}
															>
																—
															</td>
														</>
													)}
													<td
														{...stylex.props(
															cockpitStyles.tableDataTdNumComposition3,
														)}
													>
														{formatKroner(
															accountActual.get(key) ?? 0,
															currency,
														)}
													</td>
													<td
														{...stylex.props(
															cockpitStyles.tableStatementTableTdComposition2,
														)}
													>
														<Link
															to={`/companies/${slug}/posteringer?journalLineId=${row.journalLineIds[0]}`}
															{...stylex.props(cockpitStyles.aComposition)}
														>
															Journal-linje {row.journalLineIds[0]}
														</Link>
														{reviewedBudget && (
															<>
																<br
																	{...stylex.props(
																		cockpitStyles.element,
																		cockpitStyles.focusVisible,
																	)}
																/>
																<span
																	{...stylex.props(
																		cockpitStyles.element,
																		cockpitStyles.focusVisible,
																		cockpitStyles.muted,
																	)}
																>
																	{reviewedBudget.sourceRef}
																</span>
															</>
														)}
													</td>
												</tr>
											);
										})
									)}
								</tbody>
							</table>
						</div>
					)}
				</>
			)}
		</section>
	);
}

/**
 * The Budget input grid — one row per account that already carries a budget
 * line, one input per calendar month. Saving a cell appends a new revision
 * via `POST /api/companies/:slug/budget`; the core's append-only schema
 * collapses to the latest revision on the next `GET .../budget`.
 *
 * A "Tilføj konto"-form below the grid lets the owner introduce a new
 * (account, period) cell that no budget line existed for yet.
 */
function BudgetGrid({
	slug,
	data,
	currency,
	onSaved,
}: {
	slug: string;
	data: CompanyBudget;
	currency: string;
	onSaved: () => void;
}) {
	// Bucket the existing lines into a Map<accountNo, Map<period, line>> so we
	// can render the grid with one row per account and one cell per period.
	const grouped = useMemo(() => groupByAccount(data.lines), [data.lines]);
	const accountKeys = useMemo(() => [...grouped.keys()].sort(), [grouped]);

	return (
		<>
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
						Samlet budget
					</h3>
					<div
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.statusFigure,
						)}
					>
						{formatKroner(data.totalBudget, currency)}
					</div>
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
							cockpitStyles.statusNote,
						)}
					>
						{data.lines.length}{" "}
						{data.lines.length === 1 ? "budgetlinje" : "budgetlinjer"} ·
						regnskabsår {data.selectedYear}
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
								scope="col"
								{...stylex.props(
									cockpitStyles.tableStatementTableThComposition3,
								)}
							>
								Konto
							</th>
							{data.periods.map((p) => (
								<th
									key={p}
									scope="col"
									{...stylex.props(
										cockpitStyles.tableStatementTableThComposition4,
									)}
								>
									{periodLabel(p)}
								</th>
							))}
							<th
								scope="col"
								{...stylex.props(
									cockpitStyles.tableStatementTableThComposition4,
								)}
							>
								Total
							</th>
						</tr>
					</thead>
					<tbody
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						{accountKeys.length === 0 ? (
							<tr
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								<td
									colSpan={data.periods.length + 2}
									{...stylex.props(
										cockpitStyles.tableStatementTableTdComposition2,
									)}
								>
									<p
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.muted,
										)}
									>
										Ingen budgetlinjer endnu — tilføj en linje nedenfor for at
										komme i gang.
									</p>
								</td>
							</tr>
						) : (
							accountKeys.map((accountNo) => {
								const rowMap = grouped.get(accountNo)!;
								const accountName =
									[...rowMap.values()][0]?.accountName ?? null;
								const rowTotal = [...rowMap.values()].reduce(
									(sum, l) => sum + l.amount,
									0,
								);
								return (
									<tr
										key={accountNo}
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										<td
											{...stylex.props(
												cockpitStyles.tableStatementTableTdAccountNoComposition2,
											)}
										>
											{accountNo}
											{accountName ? (
												<span
													{...stylex.props(
														cockpitStyles.element,
														cockpitStyles.focusVisible,
														cockpitStyles.muted,
													)}
												>
													{" "}
													· {accountName}
												</span>
											) : null}
										</td>
										{data.periods.map((period) => {
											const existing = rowMap.get(period);
											return (
												<td
													key={period}
													{...stylex.props(
														cockpitStyles.tableDataTdNumComposition3,
													)}
												>
													<BudgetAmountInput
														slug={slug}
														accountNo={accountNo}
														period={period}
														initialAmount={existing?.amount ?? null}
														onSaved={onSaved}
													/>
												</td>
											);
										})}
										<td
											{...stylex.props(
												cockpitStyles.tableDataTdNumComposition3,
											)}
										>
											{formatKroner(rowTotal, currency)}
										</td>
									</tr>
								);
							})
						)}
					</tbody>
				</table>
			</div>

			<AddBudgetLineForm slug={slug} periods={data.periods} onSaved={onSaved} />
		</>
	);
}

/**
 * One editable cell of the Budget grid: a numeric input that POSTs to the
 * server on blur or Enter. A change persists when the value differs from
 * the last saved amount; an unchanged blur is a no-op. The append-only
 * core guarantees re-saving an unchanged value is safe — we just avoid the
 * round-trip.
 */
function BudgetAmountInput({
	slug,
	accountNo,
	period,
	initialAmount,
	onSaved,
}: {
	slug: string;
	accountNo: string;
	period: string;
	initialAmount: number | null;
	onSaved: () => void;
}) {
	const { can } = useCapabilities(slug);
	const editable = can("company.admin");
	const [value, setValue] = useState<string>(
		initialAmount === null ? "" : String(initialAmount),
	);
	const [saving, setSaving] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const outcome = useMutationOutcome(onSaved);
	// Keep the displayed value in sync when the parent reloads — a save returns
	// fresh data and the cell must reflect it, not a stale initial render.
	useEffect(() => {
		if (outcome.blocked) return;
		setValue(initialAmount === null ? "" : String(initialAmount));
	}, [initialAmount, outcome.blocked]);

	const lastSaved = initialAmount === null ? "" : String(initialAmount);

	useUnsavedChanges(value.trim() !== lastSaved.trim());

	async function commit() {
		if (!editable || saving || outcome.isBlocked()) return;
		if (value.trim() === lastSaved.trim()) return;
		const trimmed = value.trim();
		if (trimmed.length === 0) {
			// Empty input is "leave the previous revision alone" — clearing a
			// budget is not part of the append-only model, so just snap back.
			setValue(lastSaved);
			return;
		}
		const parsed = parseDanishAmount(trimmed);
		if (parsed === null || parsed < 0) {
			setError("Beløb skal være et tal ≥ 0");
			return;
		}
		setSaving(true);
		setError(null);
		try {
			await outcome.run(() =>
				api.setBudget(slug, { accountNo, period, amount: parsed }),
			);
			onSaved();
		} catch (err) {
			setError(err instanceof Error ? err.message : String(err));
		} finally {
			setSaving(false);
		}
	}

	return (
		<div {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
			{outcome.feedback}
			<Input
				type="text"
				inputMode="decimal"
				value={value}
				onChange={(e) => setValue(e.target.value)}
				onBlur={commit}
				onKeyDown={(e) => {
					if (e.key === "Enter") (e.target as HTMLInputElement).blur();
				}}
				aria-label={`Budget for konto ${accountNo} ${period}`}
				disabled={saving || outcome.blocked}
				readOnly={!editable}
				xstyle={[cockpitStyles.inputComposition]}
			/>
			{error ? (
				<span
					role="alert"
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.muted,
					)}
				>
					{error}
				</span>
			) : null}
		</div>
	);
}

/**
 * A small form below the grid to seed a new (account, period) cell when no
 * budget line existed yet. Once saved, the grid re-renders with the new row
 * — subsequent edits happen inline.
 */
function AddBudgetLineForm({
	slug,
	periods,
	onSaved,
}: {
	slug: string;
	periods: string[];
	onSaved: () => void;
}) {
	const [accountNo, setAccountNo] = useState("");
	const [period, setPeriod] = useState(periods[0] ?? "");
	const [amount, setAmount] = useState("");
	const [saving, setSaving] = useState(false);
	const [error, setError] = useState<string | null>(null);

	const outcome = useMutationOutcome(onSaved);
	useUnsavedChanges(Boolean(accountNo || amount));

	async function submit(e: React.FormEvent) {
		e.preventDefault();
		if (outcome.isBlocked()) return;
		setError(null);
		const trimmedAcc = accountNo.trim();
		if (!trimmedAcc) {
			setError("Kontonr. er påkrævet");
			return;
		}
		const parsed = parseDanishAmount(amount);
		if (parsed === null || parsed < 0) {
			setError("Beløb skal være et tal ≥ 0");
			return;
		}
		setSaving(true);
		try {
			await outcome.run(() =>
				api.setBudget(slug, {
					accountNo: trimmedAcc,
					period,
					amount: parsed,
				}),
			);
			setAccountNo("");
			setAmount("");
			onSaved();
		} catch (err) {
			setError(err instanceof Error ? err.message : String(err));
		} finally {
			setSaving(false);
		}
	}

	return (
		<form
			onSubmit={submit}
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				cockpitStyles.card,
			)}
		>
			{outcome.feedback}
			<h3
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.h3,
				)}
			>
				Tilføj budgetlinje
			</h3>
			<p
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.muted,
				)}
			>
				Vælg en konto fra kontoplanen og en måned, og angiv det planlagte beløb.
				En ny linje med samme konto+periode tilføjer en ny revision — den nyeste
				vinder.
			</p>
			<div {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
				<label
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					Konto
					<Input
						disabled={outcome.blocked}
						type="text"
						value={accountNo}
						onChange={(e) => setAccountNo(e.target.value)}
						placeholder="fx 2200"
						aria-label="Kontonr."
						xstyle={[cockpitStyles.inputComposition]}
					/>
				</label>
				<label
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					Måned
					<Select
						disabled={outcome.blocked}
						value={period}
						onChange={(e) => setPeriod(e.target.value)}
						aria-label="Måned"
						xstyle={[cockpitStyles.selectComposition]}
					>
						{periods.map((p) => (
							<option
								key={p}
								value={p}
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								{periodLabel(p)}
							</option>
						))}
					</Select>
				</label>
				<label
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					Beløb (kr)
					<Input
						disabled={outcome.blocked}
						type="text"
						inputMode="decimal"
						value={amount}
						onChange={(e) => setAmount(e.target.value)}
						placeholder="fx 5000"
						aria-label="Beløb"
						xstyle={[cockpitStyles.inputComposition]}
					/>
				</label>
				<Button
					requiredPermission="company.admin"
					type="submit"
					disabled={outcome.blocked || saving}
					xstyle={[cockpitStyles.buttonComposition]}
				>
					{saving ? "Gemmer…" : "Tilføj budgetlinje"}
				</Button>
			</div>
			{error ? (
				<p
					role="alert"
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.muted,
					)}
				>
					{error}
				</p>
			) : null}
		</form>
	);
}

/**
 * The Sammenlign-med-faktisk table — one row per (account, month) cell, with
 * budget, faktisk, variance (kr) and variance (%). A positive variance is
 * "good" (formatted as a positive figure with a check); negative is "bad".
 */
function BudgetVsActualTable({
	data,
	currency,
}: {
	data: CompanyBudgetVsActual;
	currency: string;
}) {
	const summaryTone = data.totalVariance >= 0 ? "ok" : "alert";
	return (
		<>
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
						Samlet budget
					</h3>
					<div
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.statusFigure,
						)}
					>
						{formatKroner(data.totalBudget, currency)}
					</div>
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
						Samlet faktisk
					</h3>
					<div
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.statusFigure,
						)}
					>
						{formatKroner(data.totalActual, currency)}
					</div>
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
						Samlet afvigelse
					</h3>
					<div
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.statusFigure,
							summaryTone === "alert" && cockpitStyles.statusFigureStatusAlert,
						)}
					>
						{formatKroner(data.totalVariance, currency)}
					</div>
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
								scope="col"
								{...stylex.props(
									cockpitStyles.tableStatementTableThComposition3,
								)}
							>
								Konto
							</th>
							<th
								scope="col"
								{...stylex.props(
									cockpitStyles.tableStatementTableThComposition3,
								)}
							>
								Måned
							</th>
							<th
								scope="col"
								{...stylex.props(
									cockpitStyles.tableStatementTableThComposition4,
								)}
							>
								Budget
							</th>
							<th
								scope="col"
								{...stylex.props(
									cockpitStyles.tableStatementTableThComposition4,
								)}
							>
								Faktisk
							</th>
							<th
								scope="col"
								{...stylex.props(
									cockpitStyles.tableStatementTableThComposition4,
								)}
							>
								Afvigelse
							</th>
							<th
								scope="col"
								{...stylex.props(
									cockpitStyles.tableStatementTableThComposition4,
								)}
							>
								Afvigelse %
							</th>
						</tr>
					</thead>
					<tbody
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						{data.lines.length === 0 ? (
							<tr
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								<td
									colSpan={6}
									{...stylex.props(
										cockpitStyles.tableStatementTableTdComposition2,
									)}
								>
									<p
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.muted,
										)}
									>
										Ingen budget- eller faktisk-bevægelser i {data.selectedYear}
										.
									</p>
								</td>
							</tr>
						) : (
							data.lines.map((row) => (
								<ComparisonRow
									key={`${row.accountNo}-${row.period}`}
									row={row}
									currency={currency}
								/>
							))
						)}
						{data.lines.length > 0 ? (
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
									I alt
								</td>
								<td
									{...stylex.props(
										cockpitStyles.tableDataTdNumComposition4,
										summaryTone === "ok" &&
											cockpitStyles.statementResultPositiveTdNum,
										!(summaryTone === "ok") &&
											cockpitStyles.statementResultNegativeTdNum,
									)}
								>
									{formatKroner(data.totalBudget, currency)}
								</td>
								<td
									{...stylex.props(
										cockpitStyles.tableDataTdNumComposition4,
										summaryTone === "ok" &&
											cockpitStyles.statementResultPositiveTdNum,
										!(summaryTone === "ok") &&
											cockpitStyles.statementResultNegativeTdNum,
									)}
								>
									{formatKroner(data.totalActual, currency)}
								</td>
								<td
									{...stylex.props(
										cockpitStyles.tableDataTdNumComposition4,
										summaryTone === "ok" &&
											cockpitStyles.statementResultPositiveTdNum,
										!(summaryTone === "ok") &&
											cockpitStyles.statementResultNegativeTdNum,
									)}
								>
									{formatKroner(data.totalVariance, currency)}
								</td>
								<td
									{...stylex.props(
										cockpitStyles.tableDataTdNumComposition4,
										summaryTone === "ok" &&
											cockpitStyles.statementResultPositiveTdNum,
										!(summaryTone === "ok") &&
											cockpitStyles.statementResultNegativeTdNum,
									)}
								>
									—
								</td>
							</tr>
						) : null}
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
				Positiv afvigelse betyder "godt": for udgiftskonti = under budget, for
				indtægtskonti = over mål. Tallene er læst direkte fra ledgeren og
				budget-linjer — samme funktion som CLI-rapporten kalder.
			</p>
		</>
	);
}

function ComparisonRow({
	row,
	currency,
}: {
	row: CompanyBudgetVsActualLine;
	currency: string;
}) {
	return (
		<tr {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
			<td {...stylex.props(cockpitStyles.tableStatementTableTdComposition4)}>
				{row.accountNo}
				{row.accountName ? (
					<span
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}
					>
						{" "}
						· {row.accountName}
					</span>
				) : null}
			</td>
			<td {...stylex.props(cockpitStyles.tableStatementTableTdComposition4)}>
				{periodLabel(row.period)}
			</td>
			<td {...stylex.props(cockpitStyles.tdNumComposition)}>
				{formatKroner(row.budget, currency)}
			</td>
			<td {...stylex.props(cockpitStyles.tdNumComposition)}>
				{formatKroner(row.actual, currency)}
			</td>
			<td {...stylex.props(cockpitStyles.tdNumComposition)}>
				{formatKroner(row.variance, currency)}
			</td>
			<td {...stylex.props(cockpitStyles.tdNumComposition)}>
				{row.variancePercent === null
					? "—"
					: formatPercent(row.variancePercent)}
			</td>
		</tr>
	);
}

function groupByAccount(
	lines: CompanyBudgetLine[],
): Map<string, Map<string, CompanyBudgetLine>> {
	const out = new Map<string, Map<string, CompanyBudgetLine>>();
	for (const line of lines) {
		let bucket = out.get(line.accountNo);
		if (!bucket) {
			bucket = new Map();
			out.set(line.accountNo, bucket);
		}
		bucket.set(line.period, line);
	}
	return out;
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
				Budget er ikke tilgængeligt for {year}
			</h3>
			<p
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.muted,
				)}
			>
				{year} er et arkiveret regnskabsår. Budget vs. faktisk opgøres kun for
				den aktive ledger og vises derfor ikke for et arkiveret år.
			</p>
		</div>
	);
}
