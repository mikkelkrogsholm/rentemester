import * as stylex from "@stylexjs/stylex";
import {
	Button,
	ButtonLink,
	FilterBar,
	Input,
	PageHeader,
	Pagination,
	Select,
} from "../components/ui";
import { cockpitStyles } from "../design/cockpit.stylex";
// Bank — the per-company bank transactions (cockpit-redesign iteration 3).
//
// Renders `/api/companies/:slug/bank?year=`: the imported bank_transactions
// rows for the year — date, text, amount, running balance — each with its
// reconciliation status (matched vs unmatched to a posted journal entry). The
// registered bank account and its booked ledger balance are shown above the
// table. All money fields are kroner — `formatKroner` is used throughout.
//
// #451 — filter-bar: fritekstsøgning (transaktionstekst og posteringsnr.),
// datointerval (fra/til), status-filter (alle/afstemt/uafstemt) og sortering
// på dato/beløb. Alle filtre er client-side og afspejles i URL-params
// (`q`, `from`, `to`, `status`) så ejeren kan dele linket eller komme tilbage
// til samme udsnit. En "Ryd filtre"-knap dukker op når et filter er aktivt.
// Status-linjen under filter-baren viser hvor mange transaktioner der matcher
// — og hvor mange af dem der er afstemt vs. uafstemte.

import { type ReactNode, useMemo, useState } from "react";
import { useParams, useSearchParams } from "react-router-dom";
import { BankCorrectionModal } from "../components/BankCorrectionModal";
import { BankImportModal } from "../components/BankImportModal";
import {
	BankReconcileModal,
	type BankReconcileTransaction,
} from "../components/BankReconcileModal";
import { MetricCard, PageState } from "../components/CockpitPrimitives";
import { CompanyNav, useCompanyYear } from "../components/CompanyNav";
import { PartyLink } from "../components/PartyLink";
import { api } from "../lib/api";
import { formatDateDa, formatKroner } from "../lib/format";
import type { BankTransactionRow, CompanyBank } from "../lib/types";
import { useAsync } from "../lib/useAsync";

// #451 — the URL keys we own; listed once so "Ryd filtre" can clear them all
// without touching other params (e.g. `?year=`).
const FILTER_PARAM_KEYS = [
	"q",
	"from",
	"to",
	"status",
	"transactionId",
] as const;

type StatusFilter = "all" | "matched" | "unmatched";
type SortKey = "date" | "amount";
type SortDir = "asc" | "desc";

function isStatusFilter(v: string): v is StatusFilter {
	return v === "all" || v === "matched" || v === "unmatched";
}

function txMatchesText(tx: BankTransactionRow, needle: string): boolean {
	if (tx.text && tx.text.toLowerCase().includes(needle)) return true;
	if (tx.journalEntryNo && tx.journalEntryNo.toLowerCase().includes(needle))
		return true;
	return false;
}

export function BankView() {
	const { slug = "" } = useParams();
	const { year, setYear } = useCompanyYear();
	const [params, setParams] = useSearchParams();
	const state = useAsync<CompanyBank>(
		(signal) => api.bank(slug, year, { signal }),
		[slug, year],
	);
	// True while the bank-CSV-import modal (#213, slice 2) is open.
	const [importing, setImporting] = useState(false);
	// The unmatched row currently being settled from the cockpit (#365). Null
	// while no settle-modal is open.
	const [reconciling, setReconciling] =
		useState<BankReconcileTransaction | null>(null);
	const [correcting, setCorrecting] = useState<{
		id: number;
		text: string;
		journalEntryNo?: string | null;
	} | null>(null);

	// --- #451 filter-bar params (client-side; reflected in URL) ---------------
	const q = params.get("q") ?? "";
	const fromDate = params.get("from") ?? "";
	const toDate = params.get("to") ?? "";
	const statusRaw = params.get("status") ?? "all";
	const status: StatusFilter = isStatusFilter(statusRaw) ? statusRaw : "all";
	const transactionId = Number(params.get("transactionId")) || null;

	// #451 — sorter for the date/amount columns. Default is the import order
	// (chronological as inserted); only after the owner clicks a column-header
	// do we override that order.
	const sortKey = params.get("sort");
	const sort: { key: SortKey; dir: SortDir } | null =
		sortKey === "date" || sortKey === "amount"
			? {
					key: sortKey,
					dir: params.get("direction") === "desc" ? "desc" : "asc",
				}
			: null;

	function setFilter(key: (typeof FILTER_PARAM_KEYS)[number], value: string) {
		const next = new URLSearchParams(params);
		if (value === "" || value === "all") {
			next.delete(key);
		} else {
			next.set(key, value);
		}
		next.delete("page");
		setParams(next, { replace: true });
	}

	function clearAllFilters() {
		const next = new URLSearchParams(params);
		for (const k of FILTER_PARAM_KEYS) next.delete(k);
		next.delete("page");
		setParams(next, { replace: true });
	}

	const hasActiveFilter =
		q !== "" ||
		fromDate !== "" ||
		toDate !== "" ||
		status !== "all" ||
		transactionId !== null;

	function toggleSort(key: SortKey) {
		const next = new URLSearchParams(params);
		if (!sort || sort.key !== key) {
			next.set("sort", key);
			next.set("direction", "asc");
		} else if (sort.dir === "asc") next.set("direction", "desc");
		else {
			next.delete("sort");
			next.delete("direction");
		}
		next.delete("page");
		setParams(next, { replace: true });
	}

	function sortIndicator(key: SortKey): string {
		if (!sort || sort.key !== key) return "";
		return sort.dir === "asc" ? " ▲" : " ▼";
	}

	// #UI-13 — expose the sort state to assistive tech on the sortable columns.
	function ariaSort(key: SortKey): "ascending" | "descending" | "none" {
		if (!sort || sort.key !== key) return "none";
		return sort.dir === "asc" ? "ascending" : "descending";
	}

	const allTransactions = state.data?.transactions ?? [];

	const filteredTransactions = useMemo(() => {
		if (!hasActiveFilter) return allTransactions;
		const needle = q.trim().toLowerCase();
		return allTransactions.filter((tx) => {
			if (transactionId !== null && tx.id !== transactionId) return false;
			if (needle !== "" && !txMatchesText(tx, needle)) return false;
			if (fromDate !== "" && tx.date < fromDate) return false;
			if (toDate !== "" && tx.date > toDate) return false;
			if (status === "matched" && tx.reconciliationStatus !== "matched")
				return false;
			if (status === "unmatched" && tx.reconciliationStatus !== "unmatched")
				return false;
			return true;
		});
	}, [
		allTransactions,
		hasActiveFilter,
		q,
		fromDate,
		toDate,
		status,
		transactionId,
	]);

	const sortedTransactions = useMemo(() => {
		if (!sort) return filteredTransactions;
		const out = [...filteredTransactions];
		out.sort((a, b) => {
			let cmp = 0;
			if (sort.key === "date") {
				cmp = a.date < b.date ? -1 : a.date > b.date ? 1 : 0;
			} else {
				cmp = a.amount - b.amount;
			}
			return sort.dir === "asc" ? cmp : -cmp;
		});
		return out;
	}, [filteredTransactions, sort]);

	const requestedSize = Number(params.get("pageSize"));
	const pageSize = [25, 50, 100].includes(requestedSize) ? requestedSize : 50;
	const requestedPage = Number(params.get("page"));
	const page = Math.min(
		Math.max(1, Number.isSafeInteger(requestedPage) ? requestedPage : 1),
		Math.max(1, Math.ceil(sortedTransactions.length / pageSize)),
	);
	const pageTransactions = sortedTransactions.slice(
		(page - 1) * pageSize,
		page * pageSize,
	);
	function changePage(value: number, size = pageSize) {
		const next = new URLSearchParams(params);
		next.set("page", String(value));
		next.set("pageSize", String(size));
		setParams(next, { replace: true });
	}

	if (state.loading && !state.data)
		return (
			<section
				data-evidence-issue="655"
				{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
			>
				<p
					data-evidence-heading
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					Bank
				</p>
				<p
					data-evidence-status="loading"
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					Henter bankposter
				</p>
				<BankPageShell>
					<PageState kind="loading" title="Henter bankposter" />
				</BankPageShell>
			</section>
		);
	if (state.error && !state.data)
		return (
			<section
				data-evidence-issue="655"
				{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
			>
				<p
					data-evidence-heading
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					Bank
				</p>
				<p
					data-evidence-status={
						/403|forbudt|adgang/i.test(state.error)
							? "warning-or-blocked"
							: "error"
					}
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					{/403|forbudt|adgang/i.test(state.error)
						? "Bank kræver afstemning"
						: "Bankposter kunne ikke hentes"}
				</p>
				<BankPageShell>
					<PageState
						kind="error"
						title="Bankposter kunne ikke hentes"
						onRetry={state.reload}
					>
						{state.error}
					</PageState>
				</BankPageShell>
			</section>
		);

	const b = state.data!;
	const currency = b.company.currency || "DKK";

	return (
		<section
			data-cockpit-page="bank"
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
				evidenceHeading
				title={
					<span
						data-evidence-heading
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Bank
					</span>
				}
				actions={
					<>
						<div
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.rowActions,
							)}
						>
							{/* The bank-import write action — hidden for an archived (read-only)
              year, where no live ledger is available to import into. */}
							{!b.archived && (
								<Button
									requiredPermission="company.ledger.post"
									type="button"
									data-evidence-core-action
									onClick={() => setImporting(true)}
									xstyle={[cockpitStyles.statementBtnComposition]}
								>
									Importér kontoudtog
								</Button>
							)}
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
						{b.company.country} · {currency} · Bank
					</p>
				</div>
			</PageHeader>

			<p
				data-evidence-status={b.transactions.length ? "normal" : "empty"}
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.muted,
				)}
			>
				{b.transactions.length
					? "Bank klar til gennemgang"
					: "Ingen bankposter i perioden"}
			</p>
			<CompanyNav
				slug={slug}
				years={b.fiscalYears}
				selectedYear={b.selectedYear}
				onYearChange={setYear}
			/>

			{importing && (
				<BankImportModal
					slug={slug}
					onImported={state.reload}
					onClose={() => setImporting(false)}
				/>
			)}

			{reconciling && (
				<BankReconcileModal
					slug={slug}
					transaction={reconciling}
					onReconciled={state.reload}
					onClose={() => setReconciling(null)}
				/>
			)}
			{correcting && (
				<BankCorrectionModal
					slug={slug}
					transaction={correcting}
					onApplied={state.reload}
					onClose={() => setCorrecting(null)}
				/>
			)}

			{b.archived ? (
				<ArchivedBankView
					bank={b}
					currency={currency}
					q={q}
					fromDate={fromDate}
					toDate={toDate}
					status={status}
					hasActiveFilter={hasActiveFilter}
					setFilter={setFilter}
					clearAllFilters={clearAllFilters}
					sortIndicator={sortIndicator}
					ariaSort={ariaSort}
					toggleSort={toggleSort}
					sortedTransactions={sortedTransactions}
					pageTransactions={pageTransactions}
				/>
			) : (
				<>
					<BankDifferenceBanner bank={b} currency={currency} />

					<div {...stylex.props(cockpitStyles.statusGridComposition)}>
						<MetricCard
							label="Faktisk saldo"
							value={
								b.actualBalance === null
									? "—"
									: formatKroner(b.actualBalance, currency)
							}
						>
							{/* #305: distinguish "no statement imported" from "a
                    statement was imported but its CSV had no balance column".
                    Saying "intet kontoudtog importeret" for the second case
                    would wrongly suggest the import failed. */}
							{b.actualBalance !== null
								? "Seneste saldo fra kontoudtoget"
								: b.bankStatementStatus === "no-balance-column"
									? "Banksaldo ukendt — kontoudtoget havde ingen saldo-kolonne"
									: b.bankStatementStatus === "ambiguous"
										? "Banksaldo ukendt — kontoudtogets rækkefølge eller saldo-kæde kan ikke bevises"
										: "Intet kontoudtog importeret"}
						</MetricCard>
						<MetricCard
							label="Bogført saldo"
							value={formatKroner(b.bookedBalance, currency)}
						>
							{b.accounts.length > 0
								? b.accounts
										.map((a) =>
											[a.bankName, a.name].filter(Boolean).join(" · "),
										)
										.join(", ")
								: "Bank- og kassekonti"}
						</MetricCard>
						<MetricCard
							label="Afstemning"
							value={`${b.matchedCount} / ${b.transactions.length}`}
						>
							{b.matchedCount} afstemt · {b.unmatchedCount} uafstemte
							transaktioner
						</MetricCard>
					</div>

					<BankFilterBar
						q={q}
						fromDate={fromDate}
						toDate={toDate}
						status={status}
						hasActiveFilter={hasActiveFilter}
						setFilter={setFilter}
						clearAllFilters={clearAllFilters}
					/>

					<BankFilterSummary
						allTransactions={b.transactions}
						filteredTransactions={sortedTransactions}
						hasActiveFilter={hasActiveFilter}
					/>

					<div
						data-evidence-data
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
							role="table"
							{...stylex.props(cockpitStyles.dailyListTableComposition2)}
						>
							<thead
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.dailyListThead,
								)}
							>
								<tr
									role="row"
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.dailyListTr,
									)}
								>
									<th
										role="columnheader"
										scope="col"
										aria-sort={ariaSort("date")}
										{...stylex.props(
											cockpitStyles.statementTableScrollTableThComposition,
										)}
									>
										<Button
											type="button"
											onClick={() => toggleSort("date")}
											aria-label="Sortér efter dato"
											xstyle={[cockpitStyles.statementButtonComposition]}
										>
											Dato{sortIndicator("date")}
										</Button>
									</th>
									<th
										role="columnheader"
										scope="col"
										{...stylex.props(
											cockpitStyles.statementTableScrollTableThComposition,
										)}
									>
										Tekst
									</th>
									<th
										role="columnheader"
										scope="col"
										aria-sort={ariaSort("amount")}
										{...stylex.props(
											cockpitStyles.statementTableScrollTableThComposition2,
										)}
									>
										<Button
											type="button"
											onClick={() => toggleSort("amount")}
											aria-label="Sortér efter beløb"
											xstyle={[cockpitStyles.statementButtonComposition]}
										>
											Beløb{sortIndicator("amount")}
										</Button>
									</th>
									<th
										role="columnheader"
										scope="col"
										{...stylex.props(
											cockpitStyles.statementTableScrollTableThComposition2,
										)}
									>
										Saldo
									</th>
									<th
										role="columnheader"
										scope="col"
										{...stylex.props(
											cockpitStyles.statementTableScrollTableThComposition,
										)}
									>
										Afstemning
									</th>
								</tr>
							</thead>
							<tbody
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.dailyListTbody,
								)}
							>
								{sortedTransactions.length === 0 ? (
									<tr
										role="row"
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.dailyListTr,
										)}
									>
										<td
											role="cell"
											colSpan={5}
											{...stylex.props(cockpitStyles.dailyListTdComposition6)}
										>
											{hasActiveFilter
												? "Ingen transaktioner matcher filtrene."
												: "Ingen banktransaktioner i året."}
										</td>
									</tr>
								) : (
									pageTransactions.map((tx) => (
										<tr
											role="row"
											key={tx.id}
											{...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
												cockpitStyles.dailyListTr,
											)}
										>
											<td
												data-label="Dato"
												role="cell"
												{...stylex.props(cockpitStyles.dailyListTdComposition7)}
											>
												{tx.date}
											</td>
											<td
												data-label="Tekst"
												role="cell"
												{...stylex.props(cockpitStyles.dailyListTdComposition8)}
											>
												<PartyLink slug={slug} partyId={tx.partyId}>
													{tx.text}
												</PartyLink>
											</td>
											<td
												data-label="Beløb"
												role="cell"
												{...stylex.props(cockpitStyles.dailyListTdComposition9)}
											>
												{formatKroner(tx.amount, currency)}
											</td>
											<td
												data-label="Saldo"
												role="cell"
												{...stylex.props(cockpitStyles.dailyListTdComposition9)}
											>
												{tx.runningBalance === null
													? "—"
													: formatKroner(tx.runningBalance, currency)}
											</td>
											<td
												data-label="Afstemning"
												role="cell"
												{...stylex.props(cockpitStyles.dailyListTdComposition8)}
											>
												{tx.reconciliationStatus === "matched" ? (
													<div
														{...stylex.props(
															cockpitStyles.element,
															cockpitStyles.focusVisible,
															cockpitStyles.rowActions,
														)}
													>
														<span
															{...stylex.props(
																cockpitStyles.element,
																cockpitStyles.focusVisible,
																cockpitStyles.flag,
																cockpitStyles.flagOk,
															)}
														>
															Afstemt
															{tx.journalEntryNo
																? ` · ${tx.journalEntryNo}`
																: ""}
														</span>
														<Button
															requiredPermission="company.ledger.post"
															variant="secondary"
															type="button"
															onClick={() =>
																setCorrecting({
																	id: tx.id,
																	text: tx.text,
																	journalEntryNo: tx.journalEntryNo,
																})
															}
															xstyle={[cockpitStyles.statementBtnComposition]}
														>
															Ret
														</Button>
													</div>
												) : (
													<div
														{...stylex.props(
															cockpitStyles.element,
															cockpitStyles.focusVisible,
															cockpitStyles.rowActions,
														)}
													>
														<span
															{...stylex.props(
																cockpitStyles.element,
																cockpitStyles.focusVisible,
																cockpitStyles.flag,
																cockpitStyles.flagWarning,
															)}
														>
															Uafstemt
														</span>
														<Button
															requiredPermission="company.ledger.post"
															variant="secondary"
															type="button"
															onClick={() =>
																setReconciling({
																	id: tx.id,
																	date: tx.date,
																	text: tx.text,
																	amount: tx.amount,
																	currency,
																})
															}
															xstyle={[cockpitStyles.statementBtnComposition]}
														>
															Bogfør
														</Button>
														<ButtonLink
															to={`/companies/${slug}/koebsoverblik?sourceKind=bank_transaction&sourceId=${tx.id}`}
															variant={"secondary"}
															xstyle={[cockpitStyles.statementBtnComposition2]}
														>
															Åbn købscase
														</ButtonLink>
													</div>
												)}
											</td>
										</tr>
									))
								)}
							</tbody>
						</table>
					</div>
				</>
			)}
			<Pagination
				total={sortedTransactions.length}
				page={page}
				pageSize={pageSize}
				onPageChange={(value) => changePage(value)}
				onPageSizeChange={(size) => changePage(1, size)}
			/>
		</section>
	);
}

// #451 — the filter-bar above the bank-transactions table. Same shape and
// CSS classes as DocumentsView's filter-bar so the cockpit reads consistently
// across "tunge tabeller".
function BankFilterBar({
	q,
	fromDate,
	toDate,
	status,
	setFilter,
	clearAllFilters,
	// #UI-18 — the archived branch has no reconciliation column (afstemning for
	// arkiverede år ligger i det gamle system), so a reconciliation-status filter
	// there is meaningless. Hide the Status select for that case.
	showStatusFilter = true,
}: {
	q: string;
	fromDate: string;
	toDate: string;
	status: StatusFilter;
	hasActiveFilter: boolean;
	setFilter: (key: (typeof FILTER_PARAM_KEYS)[number], value: string) => void;
	clearAllFilters: () => void;
	showStatusFilter?: boolean;
}) {
	return (
		<FilterBar
			activeCount={
				[q, fromDate, toDate, status !== "all" ? status : ""].filter(Boolean)
					.length
			}
			onReset={clearAllFilters}
		>
			<div
				role="search"
				{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
			>
				<label
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					<span
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}
					>
						Søg
					</span>
					<Input
						type="search"
						value={q}
						placeholder="Søg på tekst eller posteringsnr…"
						onChange={(e) => setFilter("q", e.target.value)}
						xstyle={[cockpitStyles.journalFilterFieldInputComposition]}
					/>
				</label>
				<details
					data-evidence-progressive
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					<summary {...stylex.props(cockpitStyles.summaryComposition)}>
						Avancerede filtre
					</summary>{" "}
					<label
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						<span
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.muted,
							)}
						>
							Fra
						</span>
						<Input
							type="date"
							value={fromDate}
							onChange={(e) => setFilter("from", e.target.value)}
							xstyle={[cockpitStyles.journalFilterFieldInputComposition]}
						/>
					</label>
					<label
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						<span
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.muted,
							)}
						>
							Til
						</span>
						<Input
							type="date"
							value={toDate}
							onChange={(e) => setFilter("to", e.target.value)}
							xstyle={[cockpitStyles.journalFilterFieldInputComposition]}
						/>
					</label>
				</details>
				{showStatusFilter && (
					<label
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						<span
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.muted,
							)}
						>
							Status
						</span>
						<Select
							value={status}
							onChange={(e) => setFilter("status", e.target.value)}
							xstyle={[cockpitStyles.journalFilterFieldSelectComposition]}
						>
							<option
								value="all"
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								Alle
							</option>
							<option
								value="matched"
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								Kun afstemte
							</option>
							<option
								value="unmatched"
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								Kun uafstemte
							</option>
						</Select>
					</label>
				)}
			</div>
		</FilterBar>
	);
}

function BankPageShell({ children }: { children: ReactNode }) {
	return (
		<section
			data-cockpit-page="bank"
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				cockpitStyles.statement,
			)}
		>
			<div
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.pageHead,
				)}
			>
				<div
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					<h1
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.h1,
						)}
					>
						Bank
					</h1>
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}
					>
						Se bankposter, afstemning og næste skridt.
					</p>
				</div>
			</div>
			{children}
		</section>
	);
}

// #451 — the "X af Y matcher · Z afstemt · W uafstemte" status line under
// the filter bar. Always counts AGAINST the filtered set so the owner sees
// how the current filter slices reconciliation status.
function BankFilterSummary({
	allTransactions,
	filteredTransactions,
	hasActiveFilter,
}: {
	allTransactions: BankTransactionRow[];
	filteredTransactions: BankTransactionRow[];
	hasActiveFilter: boolean;
}) {
	const total = allTransactions.length;
	const matchCount = filteredTransactions.length;
	const matched = filteredTransactions.filter(
		(tx) => tx.reconciliationStatus === "matched",
	).length;
	const unmatched = matchCount - matched;
	return (
		<p
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				cockpitStyles.muted,
				cockpitStyles.statementAsof,
			)}
		>
			{hasActiveFilter
				? `${matchCount} af ${total} transaktioner matcher`
				: `${total} transaktioner`}
			{" · "}
			{matched} afstemt · {unmatched} uafstemte
		</p>
	);
}

// The booked-vs-actual gap is the headline of a bank page — shown prominently
// at the top. When the gap is zero (or no statement is imported) the banner
// reassures rather than alarms.
function BankDifferenceBanner({
	bank,
	currency,
}: {
	bank: CompanyBank;
	currency: string;
}) {
	if (bank.actualBalance === null || bank.difference === null) {
		// #305: a statement WITH transactions but no balance column is not the
		// same as no statement at all — the wording must reflect which it is.
		const noBalanceColumn = bank.bankStatementStatus === "no-balance-column";
		const ambiguous = bank.bankStatementStatus === "ambiguous";
		return (
			<div
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.card,
					cockpitStyles.bankDiffBanner,
					cockpitStyles.bankDiffBannerNeutral,
				)}
			>
				<span
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.flag,
						cockpitStyles.bankDiffBannerFlag,
					)}
				>
					Bank
				</span>
				<p
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.bankDiffBannerP,
					)}
				>
					{ambiguous ? (
						<>
							Kontoudtogets rækkefølge eller løbende saldo kan ikke bevises.
							Rentemester viser derfor ikke en gættet banksaldo; kontrollér
							importen og kildeeksporten.
						</>
					) : noBalanceColumn ? (
						<>
							Kontoudtoget for {bank.selectedYear} indeholder ingen
							saldo-kolonne, så den faktiske banksaldo er ukendt — kun den
							bogførte saldo {formatKroner(bank.bookedBalance, currency)} kan
							vises.
						</>
					) : (
						<>
							Intet kontoudtog importeret for {bank.selectedYear} — kun den
							bogførte saldo {formatKroner(bank.bookedBalance, currency)} kan
							vises.
						</>
					)}
				</p>
			</div>
		);
	}

	const reconciled = Math.abs(bank.difference) < 0.005;
	if (reconciled) {
		return (
			<div
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.card,
					cockpitStyles.bankDiffBanner,
					cockpitStyles.bankDiffBannerOk,
				)}
			>
				<span
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.flag,
						cockpitStyles.flagOk,
						cockpitStyles.bankDiffBannerFlag,
					)}
				>
					Afstemt
				</span>
				<p
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.bankDiffBannerP,
					)}
				>
					Kontoudtog og bogført saldo stemmer:{" "}
					{formatKroner(bank.actualBalance, currency)}.
				</p>
			</div>
		);
	}

	return (
		<div
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				cockpitStyles.card,
				cockpitStyles.bankDiffBanner,
				cockpitStyles.bankDiffBannerAlert,
			)}
		>
			<span
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.flag,
					cockpitStyles.flagWarning,
					cockpitStyles.bankDiffBannerFlag,
				)}
			>
				Difference
			</span>
			<div {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
				<div
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.bankDiffFigure,
					)}
				>
					{formatKroner(bank.difference, currency)}
				</div>
				<p
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.bankDiffBannerP,
					)}
				>
					Bogført saldo {formatKroner(bank.bookedBalance, currency)} mod faktisk
					saldo på kontoudtoget {formatKroner(bank.actualBalance, currency)} —
					{bank.unmatchedCount > 0
						? ` ${bank.unmatchedCount} uafstemte transaktioner.`
						: " endnu ikke afstemt."}
				</p>
			</div>
		</div>
	);
}

// An archived fiscal year (#197): the ledger lives in the read-only archive,
// but the imported bank statement is live, append-only data that legitimately
// spans archived years too. The transactions are shown; the booked-balance and
// reconciliation comparison is not — there is no live ledger for that year to
// reconcile against, so a "difference" banner would be misleading.
//
// #451 — the filter-bar is also shown for archived years (read-only-egnet) so
// the owner can search historical bank movements with the same affordances.
function ArchivedBankView({
	bank,
	currency,
	q,
	fromDate,
	toDate,
	status,
	hasActiveFilter,
	setFilter,
	clearAllFilters,
	sortIndicator,
	ariaSort,
	toggleSort,
	sortedTransactions,
	pageTransactions,
}: {
	bank: CompanyBank;
	currency: string;
	q: string;
	fromDate: string;
	toDate: string;
	status: StatusFilter;
	hasActiveFilter: boolean;
	ariaSort: (key: SortKey) => "ascending" | "descending" | "none";
	setFilter: (key: (typeof FILTER_PARAM_KEYS)[number], value: string) => void;
	clearAllFilters: () => void;
	sortIndicator: (key: SortKey) => string;
	toggleSort: (key: SortKey) => void;
	sortedTransactions: BankTransactionRow[];
	pageTransactions: BankTransactionRow[];
}) {
	// The statement's closing balance for an archived year is the running
	// balance after its last imported transaction — exact, unlike a cross-year
	// "as of year-end" figure that could borrow a balance from another year.
	const lastTx =
		bank.transactions.length > 0
			? bank.transactions[bank.transactions.length - 1]!
			: null;
	const closingBalance = lastTx?.runningBalance ?? null;
	return (
		<>
			<div
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.card,
					cockpitStyles.bankDiffBanner,
					cockpitStyles.bankDiffBannerNeutral,
				)}
			>
				<span
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.flag,
						cockpitStyles.bankDiffBannerFlag,
					)}
				>
					Arkiveret år
				</span>
				<p
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.bankDiffBannerP,
					)}
				>
					{bank.selectedYear} er et arkiveret regnskabsår fra et tidligere
					bogføringssystem. Banktransaktionerne fra det importerede kontoudtog
					vises herunder. Selve bogføringen og afstemningen for arkiverede år
					ligger i regnskabet fra dengang — ikke i Rentemesters aktive ledger.
				</p>
			</div>

			{lastTx && closingBalance !== null && (
				<div {...stylex.props(cockpitStyles.statusGridComposition)}>
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
							Saldo på kontoudtoget
						</h3>
						<div
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.statusFigure,
							)}
						>
							{formatKroner(closingBalance, currency)}
						</div>
						<p
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.muted,
								cockpitStyles.statusNote,
							)}
						>
							Efter seneste postering den {formatDateDa(lastTx.date)}
						</p>
					</div>
				</div>
			)}

			<BankFilterBar
				q={q}
				fromDate={fromDate}
				toDate={toDate}
				status={status}
				hasActiveFilter={hasActiveFilter}
				setFilter={setFilter}
				clearAllFilters={clearAllFilters}
				showStatusFilter={false}
			/>

			<BankFilterSummary
				allTransactions={bank.transactions}
				filteredTransactions={sortedTransactions}
				hasActiveFilter={hasActiveFilter}
			/>

			<div
				data-evidence-data
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
					role="table"
					{...stylex.props(cockpitStyles.dailyListTableComposition3)}
				>
					<thead
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.dailyListThead,
						)}
					>
						<tr
							role="row"
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.dailyListTr,
							)}
						>
							<th
								role="columnheader"
								scope="col"
								aria-sort={ariaSort("date")}
								{...stylex.props(
									cockpitStyles.tableStatementTableThComposition3,
								)}
							>
								<Button
									type="button"
									onClick={() => toggleSort("date")}
									aria-label="Sortér efter dato"
									xstyle={[cockpitStyles.buttonComposition]}
								>
									Dato{sortIndicator("date")}
								</Button>
							</th>
							<th
								role="columnheader"
								scope="col"
								{...stylex.props(
									cockpitStyles.tableStatementTableThComposition3,
								)}
							>
								Tekst
							</th>
							<th
								role="columnheader"
								scope="col"
								aria-sort={ariaSort("amount")}
								{...stylex.props(
									cockpitStyles.tableStatementTableThComposition4,
								)}
							>
								<Button
									type="button"
									onClick={() => toggleSort("amount")}
									aria-label="Sortér efter beløb"
									xstyle={[cockpitStyles.buttonComposition]}
								>
									Beløb{sortIndicator("amount")}
								</Button>
							</th>
							<th
								role="columnheader"
								scope="col"
								{...stylex.props(
									cockpitStyles.tableStatementTableThComposition4,
								)}
							>
								Saldo
							</th>
						</tr>
					</thead>
					<tbody
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.dailyListTbody,
						)}
					>
						{sortedTransactions.length === 0 ? (
							<tr
								role="row"
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.dailyListTr,
								)}
							>
								<td
									role="cell"
									colSpan={4}
									{...stylex.props(cockpitStyles.dailyListTdComposition10)}
								>
									{hasActiveFilter
										? "Ingen transaktioner matcher filtrene."
										: `Ingen banktransaktioner importeret for ${bank.selectedYear}.`}
								</td>
							</tr>
						) : (
							pageTransactions.map((tx) => (
								<tr
									role="row"
									key={tx.id}
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.dailyListTr,
									)}
								>
									<td
										data-label="Dato"
										role="cell"
										{...stylex.props(cockpitStyles.dailyListTdComposition11)}
									>
										{tx.date}
									</td>
									<td
										data-label="Tekst"
										role="cell"
										{...stylex.props(cockpitStyles.dailyListTdComposition12)}
									>
										{tx.text}
									</td>
									<td
										data-label="Beløb"
										role="cell"
										{...stylex.props(cockpitStyles.dailyListTdComposition13)}
									>
										{formatKroner(tx.amount, currency)}
									</td>
									<td
										data-label="Saldo"
										role="cell"
										{...stylex.props(cockpitStyles.dailyListTdComposition13)}
									>
										{tx.runningBalance === null
											? "—"
											: formatKroner(tx.runningBalance, currency)}
									</td>
								</tr>
							))
						)}
					</tbody>
				</table>
			</div>
		</>
	);
}
